use std::collections::BTreeSet;

use bevy::prelude::*;
use hex_core::{Axial, SaveData, TileCatalog, World};

use super::{hull_ellipse, oar_angles, plan::*, ride, sail_angles, surface_y};
use crate::{play::PlayTile, walk::tests as walk_tests, water::tiles::WaterKind};

fn catalog() -> TileCatalog {
    TileCatalog::builtin().unwrap()
}

fn tile(q: i32, r: i32, id: &str, rotation: u8) -> PlayTile {
    PlayTile {
        cell: Axial::new(q, r),
        tile: id.to_owned(),
        rotation,
        level: 0,
    }
}

/// A river along r = 1 (edges 0 and 3, rotation 0) with a road bridge at q = 2.
fn river_world() -> Vec<PlayTile> {
    let mut t = vec![tile(0, 0, "meadow", 0)];
    for q in -1..=5 {
        let id = if q == 2 {
            "river_road_bridge"
        } else {
            "river_straight"
        };
        t.push(tile(q, 1, id, 0));
    }
    t
}

/// A sea of `ring` hex rings of ocean round (0, 0).
fn sea_world(ring: i32) -> Vec<PlayTile> {
    let mut t = vec![];
    for q in -ring..=ring {
        for r in -ring..=ring {
            if (q + r).abs() <= ring {
                t.push(tile(q, r, "ocean", 0));
            }
        }
    }
    t
}

#[test]
fn runs_end_at_the_bridge_and_boats_turn_back_short_of_it() {
    let c = catalog();
    let plan = BoatPlan::new(&river_world(), &c, 7);
    assert_eq!(
        plan.runs.len(),
        2,
        "{:?}",
        plan.runs.iter().map(|r| &r.cells).collect::<Vec<_>>()
    );
    let bridge = Vec2::from(Axial::new(2, 1).to_world(hex_core::HEX_RADIUS));
    for run in &plan.runs {
        assert!(run.ends.contains(&RunEnd::Bridge), "{:?}", run.ends);
        assert!(!run.cells.contains(&Axial::new(2, 1)));
    }
    let mut plan = plan;
    let rowers: Vec<u64> = plan
        .boats
        .iter()
        .filter(|b| matches!(b.motion, Motion::Rowing(_)))
        .map(|b| b.id)
        .collect();
    assert!(!rowers.is_empty());
    let mut turns = 0;
    let mut last_dir = std::collections::HashMap::new();
    for _ in 0..20 * 600 {
        plan.tick();
        for b in &plan.boats {
            if let Motion::Rowing(r) = &b.motion {
                let (p, _) = plan.pose(b);
                // The arch is at the bridge tile's centre: no boat comes within 14 m of it, nor
                // onto the bridge tile.
                assert!(p.distance(bridge) > 13.0, "boat {} at {p}", b.id);
                assert_ne!(cell_at_world(p), Axial::new(2, 1));
                let prev = last_dir.insert(b.id, r.dir);
                if prev.is_some_and(|d| d != r.dir) {
                    turns += 1;
                }
            }
        }
    }
    assert!(turns >= 2, "{turns} turns in ten minutes");
}

fn cell_at_world(p: Vec2) -> Axial {
    crate::boids::plan::cell_at(p)
}

#[test]
fn counts_scale_with_the_water_and_stop_at_three_of_each() {
    let c = catalog();
    // One river tile: one rowing boat, no sailing boat.
    let one = BoatPlan::new(&[tile(0, 0, "river_straight", 0)], &c, 1);
    let count = |p: &BoatPlan, k: BoatKind| p.boats.iter().filter(|b| b.kind == k).count();
    assert_eq!(count(&one, BoatKind::Rowing), 1);
    assert_eq!(count(&one, BoatKind::Sailing), 0);
    // A long river: three rowing boats at most (one per run, the second moored).
    let mut long = vec![];
    for q in -6..=6 {
        let id = if q % 4 == 0 {
            "river_road_bridge"
        } else {
            "river_straight"
        };
        long.push(tile(q, 0, id, 0));
    }
    let p = BoatPlan::new(&long, &c, 3);
    assert_eq!(count(&p, BoatKind::Rowing), MAX_ROWING);
    assert_eq!(
        p.boats
            .iter()
            .filter(|b| matches!(b.motion, Motion::Moored { .. }))
            .count(),
        1
    );
    assert_eq!(p.stages.len(), 1);
    // A big sea: three sailing boats, one at anchor; a small one fewer.
    let big = BoatPlan::new(&sea_world(3), &c, 5);
    assert_eq!(count(&big, BoatKind::Sailing), MAX_SAILING);
    assert_eq!(count(&big, BoatKind::Rowing), 0);
    assert_eq!(
        big.boats
            .iter()
            .filter(|b| matches!(b.motion, Motion::Anchored { .. }))
            .count(),
        1
    );
    let small = BoatPlan::new(&sea_world(1), &c, 5);
    assert!(count(&small, BoatKind::Sailing) <= 3);
    assert!(count(&small, BoatKind::Sailing) >= 1);
    // No water, no boats.
    assert!(
        BoatPlan::new(&[tile(0, 0, "meadow", 0)], &c, 1)
            .boats
            .is_empty()
    );
}

#[test]
fn shares_go_to_the_largest_region_first() {
    let a = Axial::new(0, 0);
    let b = Axial::new(5, 5);
    let s = share(&[(2, a, 1), (9, b, 3)], 3);
    assert_eq!(s, vec![(1, 3), (0, 0)]);
    let s = share(&[(4, b, 2), (4, a, 2)], 3);
    assert_eq!(s, vec![(1, 2), (0, 1)]);
}

/// Every route point and every tick's pose keeps its whole disc on sailing water: ocean, or coast
/// and mouth water at least 3.6 m seaward.
#[test]
fn sailing_boats_never_leave_deep_sea_water() {
    let c = catalog();
    // A coast: beaches with ocean beyond, and open sea.
    let mut world = sea_world(2);
    world.retain(|t| t.cell.r > -2);
    for q in -2..=2 {
        world.push(tile(q, -2, "beach", 1));
    }
    let mut sailed = 0;
    for seed in 0..12u64 {
        let mut plan = BoatPlan::new(&world, &c, seed);
        let sea: BTreeSet<Axial> = plan
            .cells
            .iter()
            .filter(|(_, w)| matches!(w.kind, WaterKind::Ocean | WaterKind::Coast(_)))
            .map(|(c, _)| *c)
            .collect();
        let anchors: Vec<Vec2> = plan
            .boats
            .iter()
            .filter_map(|b| match b.motion {
                Motion::Anchored { pos, .. } => Some(pos),
                _ => None,
            })
            .collect();
        for r in &plan.routes {
            for p in r.samples(0.5) {
                assert!(
                    sailable(&plan.cells, &sea, p),
                    "seed {seed}: route point {p}"
                );
                for a in &anchors {
                    assert!(
                        a.distance(p) >= ANCHOR_CLEAR - 0.5,
                        "seed {seed}: {p} by anchor {a}"
                    );
                }
            }
        }
        for _ in 0..20 * 120 {
            plan.tick();
            for b in &plan.boats {
                if b.kind == BoatKind::Sailing {
                    let (p, _) = plan.pose(b);
                    assert!(
                        sea_water(&plan.cells, &sea, p),
                        "seed {seed}: boat {} at {p}",
                        b.id
                    );
                    if matches!(b.motion, Motion::Sailing(_)) {
                        assert!(sailable(&plan.cells, &sea, p), "seed {seed}: {p}");
                        sailed += 1;
                    }
                }
            }
        }
    }
    assert!(sailed > 0);
}

#[test]
fn the_same_world_runs_the_same_boats() {
    let c = catalog();
    let mut world = river_world();
    world.extend(sea_world(2).into_iter().map(|t| PlayTile {
        cell: Axial::new(t.cell.q + 10, t.cell.r - 6),
        ..t
    }));
    let mut a = BoatPlan::new(&world, &c, 42);
    let mut b = BoatPlan::new(&world, &c, 42);
    assert_eq!(a, b);
    let (mut ca, mut cb) = (vec![], vec![]);
    for _ in 0..12000 {
        ca.extend(a.tick());
        cb.extend(b.tick());
    }
    assert_eq!(a, b);
    assert_eq!(ca, cb);
    assert!(ca.iter().any(|c| matches!(c, BoatCue::Stroke { .. })));
    assert!(
        ca.iter().any(|c| matches!(c, BoatCue::SailFlap { .. })),
        "no sail flap in ten minutes"
    );
    // Another seed places them differently.
    let other = BoatPlan::new(&world, &c, 43);
    assert_ne!(other.poses(), BoatPlan::new(&world, &c, 42).poses());
}

/// Boats are not saved: the save's bytes do not change however long they run, and a loaded world
/// derives the same boats.
#[test]
fn boats_are_never_saved() {
    let c = catalog();
    let mut w = World::new(9);
    for q in [0, 1, 2, 3, 4] {
        let id = if q == 2 {
            "river_road_bridge"
        } else {
            "river_straight"
        };
        w.place(&c, Axial::new(q, 1), id, hex_core::Orientation::new(0, 0))
            .unwrap();
    }
    let save = SaveData {
        world: w.clone(),
        unlocks: BTreeSet::new(),
        camera: hex_core::CameraState {
            focus: [0.0, 0.0, 0.0],
            yaw: 0.5,
            pitch: 0.9,
            distance: 120.0,
        },
        settings: hex_core::GameSettings {
            quality: hex_core::QualitySetting::High,
            tilt_shift: false,
            tilt_shift_chosen: false,
            tilt_shift_strength: 0.5,
            volumes: hex_core::AudioVolumes {
                master: 1.0,
                music: 0.5,
                sfx: 0.5,
                ambience: 0.5,
                ui: 0.5,
            },
        },
    };
    let tiles = |w: &World| -> Vec<PlayTile> {
        w.tiles()
            .iter()
            .map(|(cell, t)| PlayTile {
                cell: *cell,
                tile: t.def.clone(),
                rotation: t.rotation,
                level: t.level,
            })
            .collect()
    };
    let bytes = save.encode();
    let mut plan = BoatPlan::new(&tiles(&w), &c, w.seed());
    assert!(!plan.boats.is_empty());
    let fresh = plan.clone();
    for _ in 0..2000 {
        plan.tick();
    }
    assert_eq!(save.encode(), bytes);
    let loaded = SaveData::decode(&bytes, &c).unwrap();
    assert_eq!(
        BoatPlan::new(&tiles(&loaded.world), &c, loaded.world.seed()),
        fresh
    );
}

#[test]
fn a_moored_boat_lies_at_its_stage_and_passing_boats_trail_their_oars() {
    let c = catalog();
    let mut long = vec![];
    for q in -3..=3 {
        long.push(tile(q, 0, "river_straight", 0));
    }
    // Rowing boats 0 (moving) and 1 (moored) on one run.
    let mut plan = BoatPlan::new(
        &[long.clone(), vec![tile(4, 0, "river_road_bridge", 0)]].concat(),
        &c,
        2,
    );
    let moored = plan
        .boats
        .iter()
        .find(|b| matches!(b.motion, Motion::Moored { .. }))
        .cloned();
    let Some(moored) = moored else {
        panic!("no moored boat: {:?}", plan.boats);
    };
    let Motion::Moored { stage } = moored.motion else {
        unreachable!()
    };
    let s = plan.stages[stage];
    let (p, h) = plan.pose(&moored);
    assert!((p.distance(s.origin) - MOORING_X).abs() < 1e-4);
    assert!(h.dot(s.x).abs() < 1e-4, "moored along the flow");
    // The mooring is 1.465 m from the centre line, on the stage's side.
    let (c0, t) = plan.runs[s.run].line.at(s.u);
    let right = Vec2::new(-t.y, t.x);
    assert!(((p - c0).dot(right) - s.side * (STAGE_D - MOORING_X)).abs() < 0.05);
    let mut passed = false;
    for _ in 0..20 * 400 {
        plan.tick();
        for b in &plan.boats {
            if let Motion::Rowing(r) = &b.motion
                && r.run == s.run
                && (r.u - s.u).abs() < 1.0
                && r.turn == 0.0
            {
                let (q, _) = plan.pose(b);
                // Past the mooring: oars trailed, the hull clear of the moored boat's side.
                let off = (q - c0).dot(right) * s.side;
                if r.dir == s.side {
                    assert!(r.trail > 0.99, "trail {}", r.trail);
                    assert!(off < 0.0, "offset {off}");
                    passed = true;
                }
            }
        }
    }
    assert!(passed, "no boat passed the mooring on its side");
}

#[test]
fn the_stage_deck_is_ground_and_the_boats_block_the_walker() {
    let tiles = [(0, 0, "river_straight", 0u8)];
    let mut g = walk_tests::world(&tiles);
    let c = catalog();
    let plan = BoatPlan::new(&[tile(0, 0, "river_straight", 0)], &c, 1);
    let s = Stage {
        cell: Axial::new(0, 0),
        origin: Vec2::new(0.0, 4.0),
        x: Vec2::new(0.0, -1.0),
        run: 0,
        u: 0.0,
        side: 1.0,
    };
    let stage_t = Transform::from_xyz(s.origin.x, -1.4, s.origin.y)
        .with_rotation(Quat::from_rotation_y(-s.yaw()));
    let (bp, bh) = s.mooring();
    let boat_t = Transform::from_xyz(bp.x, -1.4, bp.y)
        .with_rotation(Quat::from_rotation_y(-bh.y.atan2(bh.x)));
    g.pose_boats(&[("boat_rowing", boat_t)], &[stage_t]);
    let deck = s.origin + s.x * 0.3;
    assert!(s.on_deck(deck) && g.on_deck(deck));
    assert!((g.ground(deck).unwrap() - (-0.95)).abs() < 1e-4);
    let o = g.obstacle(bp).expect("the moored boat blocks");
    assert_eq!(crate::walk::prop_kind(&o.piece), "boat");
    assert_eq!(crate::walk::prop_kind("landing_stage"), "stage");
    let _ = plan;
}

/// At the oars' furthest aft sweep both blades trail behind their rowlocks (bow +x), and the
/// trailed pose lies them alongside the hull.
#[test]
fn oars_sweep_aft_on_both_sides() {
    let loom_port = Vec3::new(0.0, -0.235, -0.972);
    let loom_stbd = Vec3::new(0.0, -0.235, 0.972);
    for (phase, trail) in [(0.5, 0.0), (0.0, 1.0)] {
        let (sweep, dip) = oar_angles(phase, trail);
        let port =
            Quat::from_rotation_y(sweep.to_radians()) * Quat::from_rotation_x(dip.to_radians());
        let stbd =
            Quat::from_rotation_y(-sweep.to_radians()) * Quat::from_rotation_x(-dip.to_radians());
        let (p, s) = (port * loom_port, stbd * loom_stbd);
        assert!(
            p.x < -0.4 && s.x < -0.4,
            "phase {phase} trail {trail}: {p} {s}"
        );
        assert!(
            (p.x - s.x).abs() < 1e-5 && (p.z + s.z).abs() < 1e-5,
            "mirrored"
        );
    }
    // On the drive the blade is buried (dip down), on the recovery lifted.
    assert!(oar_angles(0.25, 0.0).1 < -4.0);
    assert!(oar_angles(0.75, 0.0).1 > 5.0);
}

#[test]
fn sails_swing_to_leeward() {
    // Wind from -WIND_TO. Heading across it with the wind on the starboard side: boom to port.
    let from = -WIND_TO;
    let h = Vec2::new(from.y, -from.x);
    let right = Vec2::new(-h.y, h.x);
    assert!(right.dot(from) > 0.9);
    assert_eq!(leeward(h, 1.0), -1.0);
    let (main, jib) = sail_angles(h, -1.0);
    assert!(main < 0.0 && jib < 0.0);
    assert!((main.abs() - 45f32.to_radians()).abs() < 0.01);
    // Running before the wind the main is fully out, capped at 70 degrees.
    assert!((sail_angles(WIND_TO, 1.0).0 - 70f32.to_radians()).abs() < 1e-4);
}

#[test]
fn boats_ride_the_surface() {
    let c = catalog();
    let plan = BoatPlan::new(&sea_world(1), &c, 1);
    // Rivers are flat; the open sea moves with the Gerstner sum.
    let river = BoatPlan::new(&[tile(0, 0, "river_straight", 0)], &c, 1);
    let y = surface_y(&river.cells, Vec2::ZERO, 3.0, 8);
    assert!((y - (-1.4)).abs() < 1e-4, "{y}");
    let ys: Vec<f32> = (0..20)
        .map(|i| surface_y(&plan.cells, Vec2::new(3.0, 2.0), i as f32 * 0.3, 8))
        .collect();
    let spread =
        ys.iter().cloned().fold(f32::MIN, f32::max) - ys.iter().cloned().fold(f32::MAX, f32::min);
    assert!(spread > 0.05, "{ys:?}");
    let t = ride(BoatKind::Sailing, Vec2::new(3.0, 2.0), Vec2::X, 0.0, |q| {
        surface_y(&plan.cells, q, 1.0, 8)
    });
    let mid = surface_y(&plan.cells, Vec2::new(3.0, 2.0), 1.0, 8);
    assert!((t.translation.y - mid).abs() < 0.2);
}

/// The water's hull clip stays inside each hull's waterline planform (LOD0 vertices within 4 cm of
/// the waterline), so no water is cut away outside a hull.
#[test]
fn hull_ellipses_lie_inside_the_hulls() {
    for kind in [BoatKind::Rowing, BoatKind::Sailing] {
        let tris = walk_tests::mesh_tris(kind.piece(), "lod0").expect("lod0");
        let (hl, hb, off, top) = hull_ellipse(kind);
        let pts: Vec<Vec3> = tris
            .iter()
            .flat_map(|(t, _)| t.iter().map(|p| Vec3::from(*p)))
            .filter(|p| p.y.abs() < 0.04)
            .collect();
        assert!(!pts.is_empty());
        // The planform's half breadth near each station x: the most outboard waterline vertex.
        // The waterline vertices lie on the loft's stations: the half breadth between two stations
        // is the chord between theirs (inside the convex planform).
        let mut stations: std::collections::BTreeMap<i32, f32> = Default::default();
        for p in &pts {
            let e = stations.entry((p.x * 50.0).round() as i32).or_insert(0.0);
            *e = e.max(p.z.abs());
        }
        let st: Vec<(f32, f32)> = stations
            .iter()
            .map(|(k, z)| (*k as f32 / 50.0, *z))
            .collect();
        let breadth = |x: f32| {
            st.windows(2)
                .find(|w| w[0].0 <= x && x <= w[1].0)
                .map_or(0.0, |w| {
                    let t = (x - w[0].0) / (w[1].0 - w[0].0).max(1e-6);
                    w[0].1 + (w[1].1 - w[0].1) * t
                })
        };
        let (x0, x1) = pts
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.x), b.max(p.x)));
        if std::env::var("HULL_PROFILE").is_ok() {
            for i in 0..=24 {
                let x = x0 + (x1 - x0) * i as f32 / 24.0;
                eprintln!("{kind:?} x {x:.2} half breadth {:.3}", breadth(x));
            }
        }
        assert!(
            off - hl > x0 && off + hl < x1,
            "{kind:?}: {x0}..{x1} vs {}",
            hl
        );
        for i in 1..20 {
            let x = off - hl + 2.0 * hl * i as f32 / 20.0;
            let e = hb * (1.0 - ((x - off) / hl).powi(2)).sqrt();
            assert!(
                e < breadth(x),
                "{kind:?} x {x}: ellipse {e} > hull {}",
                breadth(x)
            );
        }
        assert!(top > 0.0);
    }
}

/// The boats checkpoint world (`docs/checkpoints/boats-play/`), placed by the rules: a river along
/// r = 1 from (-1, 1) through a road bridge at (2, 1) to a river mouth at (5, 1), and the sea
/// beyond it.
pub(crate) fn checkpoint_world() -> World {
    let c = catalog();
    let mut w = World::new(1);
    let mut put = |q: i32, r: i32, id: &str, rot: u8| {
        w.place(&c, Axial::new(q, r), id, hex_core::Orientation::new(rot, 0))
            .unwrap_or_else(|e| panic!("{id} at ({q}, {r}) r{rot}: {e:?}"));
    };
    put(0, 1, "river_straight", 0);
    put(1, 1, "river_straight", 0);
    put(-1, 1, "river_straight", 0);
    put(2, 1, "river_road_bridge", 0);
    put(3, 1, "river_straight", 0);
    put(4, 1, "river_straight", 0);
    put(5, 1, "river_mouth", 0);
    for (q, r) in [
        (6, 1),
        (5, 2),
        (6, 2),
        (7, 0),
        (7, 1),
        (4, 3),
        (5, 3),
        (6, 3),
        (7, 2),
        (8, 0),
        (8, 1),
    ] {
        put(q, r, "ocean", 0);
    }
    w
}

/// The checkpoint world holds three of each kind: two rowing boats on the two runs either side
/// of the bridge and one at a landing stage, two sailing boats on routes and one at anchor.
/// `BOATS_SAVE=<path>` also writes it as a `.hexworld` for the evidence script.
#[test]
fn the_checkpoint_world_holds_three_of_each() {
    let w = checkpoint_world();
    let tiles: Vec<PlayTile> = w
        .tiles()
        .iter()
        .map(|(cell, t)| PlayTile {
            cell: *cell,
            tile: t.def.clone(),
            rotation: t.rotation,
            level: t.level,
        })
        .collect();
    let plan = BoatPlan::new(&tiles, &catalog(), w.seed());
    let n = |f: &dyn Fn(&Motion) -> bool| plan.boats.iter().filter(|b| f(&b.motion)).count();
    assert_eq!(
        n(&|m| matches!(m, Motion::Rowing(_))),
        2,
        "{:?}",
        plan.boats
    );
    assert_eq!(n(&|m| matches!(m, Motion::Moored { .. })), 1);
    assert_eq!(
        n(&|m| matches!(m, Motion::Sailing(_))),
        2,
        "{:?}",
        plan.boats
    );
    assert_eq!(n(&|m| matches!(m, Motion::Anchored { .. })), 1);
    // At anchor off the beach: on the mouth tile's sea side, not out in open ocean.
    for b in &plan.boats {
        if let Motion::Anchored { pos, .. } = b.motion {
            let c = crate::boids::plan::cell_at(pos);
            assert!(
                matches!(
                    plan.cells.get(&c).map(|w| &w.kind),
                    Some(WaterKind::Coast(_) | WaterKind::RiverMouth(_))
                ),
                "anchored at {pos} in {c:?}"
            );
        }
    }
    if let Ok(path) = std::env::var("BOATS_SAVE") {
        let save = SaveData {
            world: w,
            unlocks: BTreeSet::new(),
            camera: hex_core::CameraState {
                focus: [60.0, 0.0, 30.0],
                yaw: 0.5,
                pitch: 0.9,
                distance: 140.0,
            },
            settings: hex_core::GameSettings {
                quality: hex_core::QualitySetting::High,
                tilt_shift: false,
                tilt_shift_chosen: true,
                tilt_shift_strength: 0.5,
                volumes: hex_core::AudioVolumes {
                    master: 1.0,
                    music: 0.5,
                    sfx: 0.5,
                    ambience: 0.5,
                    ui: 0.5,
                },
            },
        };
        std::fs::write(path, save.encode()).unwrap();
    }
}

fn play_tiles(w: &World) -> Vec<PlayTile> {
    w.tiles()
        .iter()
        .map(|(cell, t)| PlayTile {
            cell: *cell,
            tile: t.def.clone(),
            rotation: t.rotation,
            level: t.level,
        })
        .collect()
}

/// The fork cell of [`fork_world`]: beside the start tile, a grass edge toward it.
fn fork_cell() -> Axial {
    Axial::ORIGIN.neighbor(2)
}

/// A river fork (arms on world edges 0, 2 and 4) with two straights up each arm, placed by the
/// rules beside the start tile.
fn fork_world() -> Vec<PlayTile> {
    let c = catalog();
    let mut w = World::new(5);
    w.place(
        &c,
        fork_cell(),
        "river_fork",
        hex_core::Orientation::new(0, 0),
    )
    .unwrap();
    // A straight at rotation r carries world edges r and r + 3.
    for (e, rot) in [(0u8, 0u8), (2, 2), (4, 1)] {
        let mut cell = fork_cell();
        for _ in 0..2 {
            cell = cell.neighbor(e);
            w.place(
                &c,
                cell,
                "river_straight",
                hex_core::Orientation::new(rot, 0),
            )
            .unwrap_or_else(|err| panic!("arm {e} at {cell:?}: {err:?}"));
        }
    }
    play_tiles(&w)
}

/// Whether `p` lies on river water at least 5 cm deep: in a river cell of the plan, below the
/// surface of its bed.
fn on_river(plan: &BoatPlan, p: Vec2) -> bool {
    use crate::water::{WaterField, tiles::WATER_Y};
    plan.cells
        .get(&cell_at_world(p))
        .is_some_and(|w| w.kind == WaterKind::River && w.field().height(p.x, p.y) < WATER_Y - 0.05)
}

/// Ticks `plan` for `ticks`, checking every rowing boat's hull (centre, bow, stern and both sides)
/// stays on river water and that no pose jumps between ticks. Returns each rower's (run, tick)
/// whenever it enters another run, and how many times rowers turned back.
fn row_and_check(plan: &mut BoatPlan, ticks: usize, what: &str) -> (Vec<(u64, usize)>, usize) {
    let mut entered = vec![];
    let mut turns = 0;
    let mut last: std::collections::HashMap<u64, (Vec2, usize, f32)> = Default::default();
    for t in 0..ticks {
        plan.tick();
        for b in &plan.boats {
            let Motion::Rowing(r) = &b.motion else {
                continue;
            };
            let (p, h) = plan.pose(b);
            let side = Vec2::new(-h.y, h.x);
            for q in [p, p + h * 2.3, p - h * 2.3, p + side * 0.6, p - side * 0.6] {
                assert!(
                    on_river(plan, q),
                    "{what}, tick {t}: boat {} at {p} heading {h}: {q} off the water ({r:?})",
                    b.id
                );
            }
            if let Some((p0, run0, dir0)) = last.insert(b.id, (p, r.run, r.dir)) {
                // A step at the rowing speed, plus a lane change, plus a chord's corner.
                assert!(
                    p.distance(p0) < 0.3,
                    "{what}, tick {t}: boat {} jumped {p0} -> {p}",
                    b.id
                );
                if run0 != r.run {
                    entered.push((b.id, r.run));
                } else if dir0 != r.dir {
                    turns += 1;
                }
            }
        }
    }
    (entered, turns)
}

#[test]
fn a_fork_and_the_tight_bend_are_rowable_river() {
    let c = catalog();
    for id in ["river_fork", "river_bend_60"] {
        let plan = BoatPlan::new(&[tile(0, 0, id, 0)], &c, 1);
        let def = c.get(id).unwrap();
        assert!(rowable(def, &plan.cells[&Axial::ORIGIN]), "{id}");
        let rowing: Vec<_> = plan
            .boats
            .iter()
            .filter(|b| b.kind == BoatKind::Rowing)
            .collect();
        assert_eq!(rowing.len(), 1, "{id}: {:?}", plan.boats);
        assert!(matches!(rowing[0].motion, Motion::Rowing(_)), "{id}");
    }
    // Stages stay on plain straights and 120 degree bends.
    let plan = BoatPlan::new(&fork_world(), &c, 3);
    assert!(!plan.stages.is_empty());
    for s in &plan.stages {
        let id = &plan.cells[&s.cell].tile.tile;
        assert_eq!(id, "river_straight", "stage on {id}");
    }
}

/// Each fork arm is its own run ending at the node, each pair of arms is joined by a link, and
/// rowers take more than one arm out of the fork across seeds, on the water and without a jump,
/// never back up the arm they came down.
#[test]
fn rowing_boats_take_the_arms_of_a_fork_and_stay_on_the_water() {
    let c = catalog();
    let world = fork_world();
    let plan = BoatPlan::new(&world, &c, 0);
    let arms: Vec<usize> = (0..plan.runs.len())
        .filter(|&i| plan.runs[i].joins.is_none())
        .collect();
    assert_eq!(arms.len(), 3, "{:?}", plan.runs);
    for &i in &arms {
        let run = &plan.runs[i];
        assert_eq!(run.cells.len(), 3, "{:?}", run.cells);
        let at_node = (0..2).filter(|&e| run.ends[e] == RunEnd::Junction).count();
        assert_eq!(at_node, 1, "{:?}", run.ends);
        assert!(run.cells.contains(&fork_cell()));
    }
    let links: Vec<[(usize, usize); 2]> = plan.runs.iter().filter_map(|r| r.joins).collect();
    assert_eq!(links.len(), 3);
    for [a, b] in &links {
        assert_ne!(a.0, b.0, "a link joins two arms");
        // The link meets each arm where the rower leaves it.
        let link = plan
            .runs
            .iter()
            .find(|r| r.joins == Some([*a, *b]))
            .unwrap();
        for (k, (arm, end)) in [(0usize, *a), (1, *b)] {
            let line = &plan.runs[arm].line;
            let u = if end == 0 {
                JUNCTION_D
            } else {
                line.length() - JUNCTION_D
            };
            let at = if k == 0 { 0.0 } else { link.line.length() };
            assert!(line.at(u).0.distance(link.line.at(at).0) < 1e-3);
            assert!(line.at(u).1.dot(link.line.at(at).1).abs() > 0.99, "tangent");
        }
    }
    let mut out_arms = BTreeSet::new();
    let mut hops = 0;
    for seed in 0..6u64 {
        let mut plan = BoatPlan::new(&world, &c, seed);
        let system = systems(&plan.runs);
        let moving = plan
            .boats
            .iter()
            .filter(|b| matches!(b.motion, Motion::Rowing(_)))
            .count();
        assert_eq!(moving, 1, "seed {seed}: one moving rower per river system");
        assert!(system.iter().all(|s| *s == system[0]), "one system");
        let (entered, _) = row_and_check(&mut plan, 20 * 600, &format!("seed {seed}"));
        // A link leads only to the arms it joins.
        for w in entered.windows(2) {
            let (from, to) = (w[0].1, w[1].1);
            if let Some([a, b]) = plan.runs[from].joins {
                assert!(to == a.0 || to == b.0, "seed {seed}: link {from} to {to}");
            }
        }
        // Arm, link, arm: never back up the arm it came down.
        let path: Vec<usize> = entered.iter().map(|e| e.1).collect();
        for w in path.windows(3) {
            if plan.runs[w[1]].joins.is_some() {
                assert_ne!(w[0], w[2], "seed {seed}: back up its own arm");
                out_arms.insert(w[2]);
                hops += 1;
            }
        }
    }
    assert!(hops >= 6, "{hops} junction crossings");
    // A lone fork in every rotation: 4.66 m between an arm's turn-back and its link.
    for rot in 0..6u8 {
        let mut plan = BoatPlan::new(&[tile(0, 0, "river_fork", rot)], &c, rot as u64);
        let (entered, turns) = row_and_check(&mut plan, 20 * 300, &format!("lone fork r{rot}"));
        assert!(
            entered.len() >= 4 && turns >= 2,
            "r{rot}: {entered:?}, {turns} turns"
        );
    }
    assert!(
        out_arms.len() >= 2,
        "arms taken out of the fork: {out_arms:?}"
    );
}

/// A rowing boat on the tight bend, alone (its turn-backs on the arc) and between straights, keeps
/// its hull on the water at its lane in every rotation.
#[test]
fn rowing_boats_keep_to_the_channel_round_the_tight_bend() {
    let c = catalog();
    for rot in 0..6u8 {
        let mut plan = BoatPlan::new(&[tile(0, 0, "river_bend_60", rot)], &c, rot as u64);
        assert_eq!(plan.runs.len(), 1);
        let (_, turns) = row_and_check(&mut plan, 20 * 300, &format!("lone r{rot}"));
        assert!(turns >= 4, "r{rot}: {turns} turns");
    }
    // Beside the start tile, a grass edge toward it.
    let bend = Axial::ORIGIN.neighbor(5);
    let mut w = World::new(2);
    w.place(&c, bend, "river_bend_60", hex_core::Orientation::new(0, 0))
        .unwrap();
    w.place(
        &c,
        bend.neighbor(0),
        "river_straight",
        hex_core::Orientation::new(0, 0),
    )
    .unwrap();
    w.place(
        &c,
        bend.neighbor(1),
        "river_straight",
        hex_core::Orientation::new(1, 0),
    )
    .unwrap();
    let tiles = play_tiles(&w);
    for seed in 0..4u64 {
        let mut plan = BoatPlan::new(&tiles, &c, seed);
        assert_eq!(plan.runs.len(), 1, "{:?}", plan.runs);
        assert_eq!(plan.runs[0].cells.len(), 3);
        row_and_check(
            &mut plan,
            20 * 300,
            &format!("bend between straights, seed {seed}"),
        );
    }
}

#[test]
fn the_same_fork_world_runs_the_same_boats() {
    let c = catalog();
    let world = fork_world();
    let mut a = BoatPlan::new(&world, &c, 42);
    let mut b = BoatPlan::new(&world, &c, 42);
    assert_eq!(a, b);
    let (mut ca, mut cb) = (vec![], vec![]);
    for _ in 0..12000 {
        ca.extend(a.tick());
        cb.extend(b.tick());
        assert_eq!(a.poses(), b.poses());
    }
    assert_eq!(a, b);
    assert_eq!(ca, cb);
    let other = BoatPlan::new(&world, &c, 43);
    assert_ne!(other.poses(), BoatPlan::new(&world, &c, 42).poses());
}
