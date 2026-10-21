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
        w.place(&c, Axial::new(q, 1), id, 0).unwrap();
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
        w.place(&c, Axial::new(q, r), id, rot)
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
