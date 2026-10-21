//! Rail network (P3): stitching across seams, the switch at the junction toe, the fouling point,
//! line ends, loops and the level crossing.

mod common;

use std::collections::BTreeSet;

use common::*;
use hex_core::{Axial, NetworkKind, TileCatalog, World};
use hex_sim::geom;
use hex_sim::rail::{Blocker, Leg, Railway};
use hex_sim::{SimConfig, Simulation, TICK_HZ};

fn place(r: &mut Railway, c: &TileCatalog, cell: Axial, id: &str, rot: u8) {
    r.insert_tile(cell, c.get(id).unwrap_or_else(|| panic!("{id}")), rot);
}

/// The Y: a junction at the origin (toe east, straight west, branch south-east), each leg running
/// on two straights to a line end.
fn y_layout(r: &mut Railway, c: &TileCatalog) {
    place(r, c, Axial::new(0, 0), "rail_junction", 0);
    for q in [1, 2] {
        place(r, c, Axial::new(q, 0), "rail_straight", 0);
    }
    for q in [-1, -2] {
        place(r, c, Axial::new(q, 0), "rail_straight", 0);
    }
    for rr in [1, 2] {
        place(r, c, Axial::new(0, rr), "rail_straight", 1);
    }
}

/// A triangle loop around the origin: three bends and two straights per side (216 m).
pub fn triangle(n: i32) -> Vec<(Axial, &'static str, u8)> {
    let (q0, r0) = (-1, -1);
    let mut out = vec![(Axial::new(q0, r0), "rail_bend_60", 0)];
    for k in 1..=n {
        out.push((Axial::new(q0 + k, r0), "rail_straight", 0));
    }
    out.push((Axial::new(q0 + n + 1, r0), "rail_bend_60", 2));
    for k in 1..=n {
        out.push((Axial::new(q0 + n + 1 - k, r0 + k), "rail_straight", 2));
    }
    out.push((Axial::new(q0, r0 + n + 1), "rail_bend_60", 4));
    for k in 1..=n {
        out.push((Axial::new(q0, r0 + n + 1 - k), "rail_straight", 1));
    }
    out
}

fn run(r: &mut Railway, seconds: u32) {
    for _ in 0..seconds * TICK_HZ {
        r.tick();
    }
}

/// No two trains share track: their bodies' link intervals never overlap.
fn assert_apart(r: &Railway) {
    let bodies: Vec<_> = r.trains().iter().map(|t| (t.id(), r.body(t))).collect();
    for (i, (a, ba)) in bodies.iter().enumerate() {
        for (b, bb) in &bodies[i + 1..] {
            for &(la, lo, hi) in ba {
                for &(lb, lo2, hi2) in bb {
                    assert!(
                        la != lb || hi < lo2 || hi2 < lo,
                        "{a:?} and {b:?} overlap on {la:?}: [{lo}, {hi}] vs [{lo2}, {hi2}]"
                    );
                }
            }
        }
    }
}

#[test]
fn rail_tiles_stitch_across_every_seam_of_the_triangle_loop() {
    let c = catalog();
    let mut r = Railway::new();
    for (cell, id, rot) in triangle(2) {
        place(&mut r, &c, cell, id, rot);
    }
    let net = r.network();
    assert_eq!(net.kind(), NetworkKind::Rail);
    assert_eq!(net.links().count(), 9);
    // A closed loop: every node joins exactly two links, and every node is a seam.
    for (_, node) in net.nodes() {
        assert_eq!(node.links().len(), 2, "{:?}", node.key());
    }
    let len: f32 = net.links().map(|(_, l)| l.length()).sum();
    assert!(
        (len - (6.0 * 27.7128 + 3.0 * 16.7552)).abs() < 0.05,
        "{len}"
    );
}

/// Criterion 2: a train routes across tile boundaries, and the junction's switch, set at the toe,
/// sends a facing train down the straight leg, then the branch, alternately.
#[test]
fn trains_cross_seams_and_take_each_leg_of_the_switch() {
    let c = catalog();
    let mut r = Railway::new();
    y_layout(&mut r, &c);
    run(&mut r, 1);
    assert!(!r.trains().is_empty(), "the Y gets a train");
    let mut cells: BTreeSet<Axial> = BTreeSet::new();
    for _ in 0..240 * TICK_HZ {
        r.tick();
        assert_apart(&r);
        for t in r.trains() {
            cells.extend(t.steps().iter().map(|s| s.link.cell));
        }
    }
    assert!(cells.len() >= 6, "trains ran over {cells:?}");
    let legs: Vec<Leg> = r
        .trains()
        .iter()
        .flat_map(|t| t.routes().iter().map(|(_, l)| *l))
        .collect();
    assert!(legs.contains(&Leg::Straight), "{legs:?}");
    assert!(legs.contains(&Leg::Branch), "{legs:?}");
    let j = &r.junctions()[0];
    assert_eq!(j.cell(), Axial::ORIGIN);
    assert!(j.throws() >= 2, "the switch moved {} times", j.throws());
    // Each facing passage alternates the leg.
    assert!(j.facing().len() >= 2, "{:?}", j.facing());
    for w in j.facing().windows(2) {
        assert_ne!(w[0], w[1], "{:?}", j.facing());
    }
}

/// Criterion 3: two trains on the Y never share the junction's fouling zone, the switch never
/// moves while a train is in it, a train waiting for the junction stands outside the zone (beyond
/// the fouling point, 7.42 m from the toe border), and neither train stays stopped for good.
#[test]
fn trains_clear_each_other_at_the_junction_fouling_point() {
    let c = catalog();
    let mut r = Railway::new();
    y_layout(&mut r, &c);
    // Two trains converge on the toe from the two legs: one on the straight leg's far tile heading
    // east, one on the branch's far tile heading back north-west, their fronts about equally far
    // from the junction.
    let link = |q: i32, rr: i32| r.network().links_of(Axial::new(q, rr)).next().unwrap().0;
    let (west, south) = (link(-2, 0), link(0, 2));
    let toward = |r: &Railway, l: hex_sim::LinkId, toward: Axial| {
        let pts = r.network().link(l).unwrap().points();
        let c = toward.to_world(hex_core::HEX_RADIUS);
        geom::dist(pts[pts.len() - 1], c) < geom::dist(pts[0], c)
    };
    let (fw, fs) = (
        toward(&r, west, Axial::ORIGIN),
        toward(&r, south, Axial::ORIGIN),
    );
    let len = r.network().link(west).unwrap().length();
    let front = |forward: bool| if forward { len - 1.0 } else { 1.0 };
    r.spawn_at(west, fw, front(fw))
        .expect("fits on the west leg");
    r.spawn_at(south, fs, front(fs))
        .expect("fits on the branch");
    run(&mut r, 1);
    assert_eq!(r.trains().len(), 2, "211 m of track holds two trains");
    let (mut waits, mut last_set) = (0, r.junctions()[0].set());
    let mut last_moved: Vec<u32> = vec![0; 2];
    for tick in 0..600 * TICK_HZ {
        r.tick();
        assert_apart(&r);
        let j = &r.junctions()[0];
        let inside: Vec<_> = r.trains().iter().filter(|t| r.in_zone(j, t)).collect();
        assert!(inside.len() <= 1, "tick {tick}: two trains in the zone");
        if j.set() != last_set {
            assert!(
                inside.is_empty(),
                "tick {tick}: the switch moved under {:?}",
                inside[0].id()
            );
            last_set = j.set();
        }
        if let Some(t) = inside.first() {
            assert_eq!(
                j.holder(),
                Some(t.id()),
                "tick {tick}: in the zone unreserved"
            );
        }
        for (k, t) in r.trains().iter().enumerate() {
            if t.blocker() == Blocker::Junction(Axial::ORIGIN) {
                waits += 1;
                assert!(!r.in_zone(j, t), "tick {tick}: waiting inside the zone");
            }
            if t.speed() > 0.0 {
                last_moved[k] = tick;
            }
            assert!(
                tick - last_moved[k] < 40 * TICK_HZ,
                "tick {tick}: {:?} stopped for 40 s",
                t.id()
            );
        }
    }
    assert!(waits > 0, "no train ever waited for the junction");
}

/// The way a train runs round a loop about the loop's centroid: +1 or -1 (the sign of the cross
/// product of the engine's offset from the centroid and its heading, negated while it propels).
fn sense(r: &Railway, t: &hex_sim::rail::Train) -> f32 {
    let pts: Vec<[f32; 2]> = r
        .network()
        .links()
        .flat_map(|(_, l)| l.points().to_vec())
        .collect();
    let n = pts.len() as f32;
    let c = [
        pts.iter().map(|p| p[0]).sum::<f32>() / n,
        pts.iter().map(|p| p[1]).sum::<f32>() / n,
    ];
    // The engine faces the engine end; propelling, the train runs the other way.
    let e = t.vehicles(1.0)[0].pose;
    let o = geom::sub(e.pos, c);
    let way = if t.engine_leads() { 1.0 } else { -1.0 };
    way * (o[0] * e.dir[1] - o[1] * e.dir[0]).signum()
}

/// Every train spawned on a loop runs the same way round, from its first tick: the spawner walks
/// the loop once and lays each train along that walk, whatever way each link was stitched.
#[test]
fn trains_spawned_on_a_loop_all_run_the_same_way_round() {
    let c = catalog();
    for n in [2, 3, 4, 5] {
        let mut r = Railway::new();
        for (cell, id, rot) in triangle(n) {
            place(&mut r, &c, cell, id, rot);
        }
        r.tick();
        assert!(
            r.trains().len() >= 2,
            "triangle({n}): {} trains",
            r.trains().len()
        );
        let senses: Vec<f32> = r.trains().iter().map(|t| sense(&r, t)).collect();
        assert!(
            senses.iter().all(|s| *s == senses[0]),
            "triangle({n}): trains run {senses:?} round"
        );
    }
}

/// Every vehicle faces the engine end of its train (the engine's smokebox is the kit piece's +x,
/// `scripts/blender/kit/_train.py`), engine first in the consist, before and after the train runs
/// back from a line end: the engine's heading points away from the first carriage and each
/// carriage's toward the vehicle ahead of it.
#[test]
fn every_vehicle_faces_the_engine_end_before_and_after_a_reversal() {
    let c = catalog();
    let mut r = Railway::new();
    y_layout(&mut r, &c);
    let mut reversed = false;
    for _ in 0..240 * TICK_HZ {
        r.tick();
        for t in r.trains() {
            let v = t.vehicles(1.0);
            assert_eq!(v[0].kind, hex_sim::VehicleKind::Engine);
            for w in v.windows(2) {
                let ahead = geom::sub(w[0].pose.pos, w[1].pose.pos);
                for p in [w[0].pose, w[1].pose] {
                    assert!(
                        geom::dot(p.dir, ahead) > 0.0,
                        "{:?} reversals {}: a vehicle faces away from the engine",
                        t.id(),
                        t.reversals()
                    );
                }
            }
            reversed |= t.reversals() > 0 && !t.engine_leads();
        }
    }
    assert!(reversed, "a train ran back with the engine propelling");
}

/// Built side by side, the triangle's three sides are three stretches with a train each, laid
/// along each side's own walk. When the corners close them into one loop, the trains running
/// against the oldest turn round where they stand, so all three lap the same way and never meet.
#[test]
fn trains_on_stretches_closed_into_a_loop_turn_to_run_the_same_way_round() {
    let c = catalog();
    let mut r = Railway::new();
    let tiles = triangle(2);
    for (cell, id, rot) in tiles.iter().filter(|t| t.1 == "rail_straight") {
        place(&mut r, &c, *cell, id, *rot);
    }
    run(&mut r, 2);
    assert_eq!(r.trains().len(), 3, "a train on each side");
    for (cell, id, rot) in tiles.iter().filter(|t| t.1 == "rail_bend_60") {
        place(&mut r, &c, *cell, id, *rot);
    }
    r.tick();
    let senses: Vec<f32> = r.trains().iter().map(|t| sense(&r, t)).collect();
    assert!(
        senses.iter().all(|s| *s == senses[0]),
        "trains run {senses:?} round"
    );
    let before: Vec<u32> = r.trains().iter().map(|t| t.reversals()).collect();
    for _ in 0..120 * TICK_HZ {
        r.tick();
        assert_apart(&r);
    }
    for (t, n) in r.trains().iter().zip(before) {
        assert_eq!(
            t.reversals(),
            n,
            "{:?} ran back after the loop closed",
            t.id()
        );
        assert!(t.speed() > 5.0, "{:?} at {} m/s", t.id(), t.speed());
    }
}

/// Six wide bends on the ring round the start meadow close one 150.8 m circle (radius 24 m about
/// the origin). A stretch of at least 55 m gets one train plus one per further 140 m, so the circle
/// holds one, which laps it at line speed and never runs back.
#[test]
fn six_wide_bends_close_a_circle_that_trains_lap() {
    let c = catalog();
    let mut r = Railway::new();
    for d in 0..6u8 {
        place(&mut r, &c, ring_cell(d), "rail_bend_120", (d + 2) % 6);
    }
    let net = r.network();
    assert_eq!(net.links().count(), 6);
    for (_, node) in net.nodes() {
        assert_eq!(node.links().len(), 2, "{:?}", node.key());
    }
    let len: f32 = net.links().map(|(_, l)| l.length()).sum();
    let radius = APOTHEM * 3f32.sqrt();
    assert!(
        (len - 2.0 * std::f32::consts::PI * radius).abs() < 0.05,
        "{len}"
    );
    for (_, l) in net.links() {
        for p in l.points() {
            assert!(
                (geom::len(*p) - radius).abs() < 0.002,
                "{p:?} off the circle"
            );
        }
    }
    run(&mut r, 60);
    assert_eq!(r.trains().len(), 1);
    let start = r.trains()[0].front();
    let mut lapped = false;
    for _ in 0..120 * TICK_HZ {
        r.tick();
        let t = &r.trains()[0];
        assert!(t.speed() > 5.0, "{:?} at {} m/s", t.id(), t.speed());
        lapped |= geom::dist(t.front(), start) > 40.0;
    }
    let t = &r.trains()[0];
    assert!(lapped, "the train never left its start");
    assert_eq!(t.reversals(), 0, "{:?} ran back on the circle", t.id());
}

/// On a loop a train never stops: it laps the 216 m triangle at line speed.
#[test]
fn a_train_laps_a_loop_without_stopping() {
    let c = catalog();
    let mut r = Railway::new();
    for (cell, id, rot) in triangle(2) {
        place(&mut r, &c, cell, id, rot);
    }
    // Both trains spawn running the same way round, so neither ever runs back.
    run(&mut r, 60);
    assert_eq!(r.trains().len(), 2);
    let reversals: Vec<u32> = r.trains().iter().map(|t| t.reversals()).collect();
    assert_eq!(reversals.iter().sum::<u32>(), 0, "{reversals:?}");
    for _ in 0..120 * TICK_HZ {
        r.tick();
        assert_apart(&r);
    }
    for (t, before) in r.trains().iter().zip(reversals) {
        assert!(t.speed() > 5.0, "{:?} at {} m/s", t.id(), t.speed());
        assert_eq!(t.reversals(), before, "{:?} ran back on a loop", t.id());
    }
}

/// At a line end the train stops inside the last tile, waits and runs back with the engine at the
/// other end, propelling.
#[test]
fn a_train_runs_back_from_a_line_end() {
    let c = catalog();
    let mut r = Railway::new();
    for q in 0..3 {
        place(&mut r, &c, Axial::new(q, 0), "rail_straight", 0);
    }
    run(&mut r, 1);
    assert_eq!(r.trains().len(), 1, "83 m of line holds one train");
    let mut ends = 0;
    let mut was_end = false;
    for _ in 0..120 * TICK_HZ {
        r.tick();
        let t = &r.trains()[0];
        let at_end = t.blocker() == Blocker::LineEnd;
        if at_end && !was_end {
            ends += 1;
            // The front stands inside the line: at least the end margin short of the outer border.
            let f = t.front();
            let border = [
                Axial::new(-1, 0).to_world(hex_core::HEX_RADIUS),
                Axial::new(3, 0).to_world(hex_core::HEX_RADIUS),
            ];
            let mids = border.map(|b| {
                geom::lerp(
                    b,
                    if b[0] < 0.0 {
                        Axial::new(0, 0).to_world(hex_core::HEX_RADIUS)
                    } else {
                        Axial::new(2, 0).to_world(hex_core::HEX_RADIUS)
                    },
                    0.5,
                )
            });
            let d = mids
                .iter()
                .map(|m| geom::dist(*m, f))
                .fold(f32::INFINITY, f32::min);
            assert!(
                d > 4.0,
                "the engine's middle stands {d} m from the line end"
            );
        }
        was_end = at_end;
    }
    let t = &r.trains()[0];
    assert!(
        ends >= 2 && t.reversals() >= 2,
        "{ends} ends, {} reversals",
        t.reversals()
    );
}

/// The brake third (its veranda and lookout at the train's tail) stays the vehicle farthest from
/// the engine both ways, propelled as well as hauled.
#[test]
fn the_brake_third_runs_last_both_ways() {
    let c = catalog();
    let mut r = Railway::new();
    for q in 0..3 {
        place(&mut r, &c, Axial::new(q, 0), "rail_straight", 0);
    }
    run(&mut r, 1);
    let mut checked = 0;
    for _ in 0..90 * TICK_HZ {
        r.tick();
        let t = &r.trains()[0];
        let vs = t.vehicles(1.0);
        let at = |k: hex_sim::VehicleKind| vs.iter().find(|v| v.kind == k).unwrap().pose.pos;
        let e = at(hex_sim::VehicleKind::Engine);
        let (car, brake) = (
            at(hex_sim::VehicleKind::Carriage),
            at(hex_sim::VehicleKind::BrakeCarriage),
        );
        assert!(geom::dist(e, brake) > geom::dist(e, car) + 7.0);
        checked += 1;
    }
    assert!(checked > 0 && r.trains()[0].reversals() >= 1);
}

/// Trains are derived: a fresh simulation of the same world spawns the same trains in the same
/// places, so nothing about them needs saving.
#[test]
fn trains_are_rederived_from_the_world() {
    let c = catalog();
    let mut world = World::new(5);
    // The triangle around the start meadow: sides first (each touches the meadow), then corners.
    let tiles = triangle(2);
    for (cell, id, rot) in tiles.iter().filter(|t| t.1 == "rail_straight") {
        world.place(&c, *cell, id, *rot).unwrap();
    }
    for (cell, id, rot) in tiles.iter().filter(|t| t.1 == "rail_bend_60") {
        world.place(&c, *cell, id, *rot).unwrap();
    }
    let mut a = Simulation::from_world(&world, &c, SimConfig::default());
    let mut b = Simulation::from_world(&world, &c, SimConfig::default());
    for _ in 0..30 * TICK_HZ {
        a.tick();
        b.tick();
    }
    let fronts = |s: &Simulation| -> Vec<[f32; 2]> {
        s.rails().trains().iter().map(|t| t.front()).collect()
    };
    assert_eq!(fronts(&a).len(), 2);
    assert_eq!(fronts(&a), fronts(&b));
}

/// Bulldozing a tile under a train removes that train; the rest of the line keeps its trains.
#[test]
fn a_world_edit_removes_the_trains_on_the_cell() {
    let c = catalog();
    let mut r = Railway::new();
    for (cell, id, rot) in triangle(2) {
        place(&mut r, &c, cell, id, rot);
    }
    run(&mut r, 5);
    let t = r.trains()[0].clone();
    let cell = t.steps()[0].link.cell;
    r.remove_tile(cell);
    assert!(r.trains().iter().all(|u| u.id() != t.id()));
    assert!(
        r.trains()
            .iter()
            .all(|u| u.steps().iter().all(|s| s.link.cell != cell))
    );
    run(&mut r, 60);
    assert_apart(&r);
}

/// Random legal rail layouts with stations: trains never overlap, never share a junction's
/// fouling zone or run on a leg with the switch set against them, and none stays stopped for good
/// while routing between the stations.
#[test]
fn random_rail_layouts_keep_trains_apart_and_moving() {
    let c = catalog();
    const RAIL: [&str; 6] = [
        "rail_straight",
        "rail_bend_60",
        "rail_junction",
        "rail_station",
        "rail_straight",
        "rail_bend_60",
        "rail_bend_120",
    ];
    let (mut trains_seen, mut routed, mut arrivals) = (0, 0, 0);
    for seed in 0..16u64 {
        let mut world = World::new(seed);
        let mut rng = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let mut next = move |n: usize| {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            (rng % n as u64) as usize
        };
        for _ in 0..60 {
            let frontier = world.frontier();
            let cell = frontier[next(frontier.len())];
            let def = c.get(RAIL[next(RAIL.len())]).unwrap();
            let rots = world.legal_rotations(&c, cell, def);
            if rots.is_empty() {
                let meadow = c.get("meadow").unwrap();
                if world.fits(&c, cell, meadow, 0) {
                    world.place(&c, cell, "meadow", 0).unwrap();
                }
                continue;
            }
            let rot = rots[next(rots.len())];
            world.place(&c, cell, &def.id, rot).unwrap();
        }
        let mut sim = Simulation::from_world(&world, &c, SimConfig::default());
        let mut last_moved: std::collections::BTreeMap<u64, u32> = Default::default();
        for tick in 0..300 * TICK_HZ {
            sim.tick();
            let r = sim.rails();
            assert_apart(r);
            for j in r.junctions() {
                let inside: Vec<_> = r.trains().iter().filter(|t| r.in_zone(j, t)).collect();
                assert!(inside.len() <= 1, "seed {seed} tick {tick}: two in a zone");
                for t in inside {
                    assert_eq!(j.holder(), Some(t.id()), "seed {seed} tick {tick}");
                    for leg in r.legs_under(j, t) {
                        assert_eq!(j.set(), leg, "seed {seed} tick {tick}: switch against");
                    }
                }
            }
            for t in r.trains() {
                let moved = last_moved.entry(t.id().0).or_insert(tick);
                if t.speed() > 0.0 {
                    *moved = tick;
                }
                assert!(
                    tick - *moved < 75 * TICK_HZ,
                    "seed {seed} tick {tick}: {:?} stuck ({:?})",
                    t.id(),
                    t.blocker()
                );
            }
        }
        let r = sim.rails();
        trains_seen += r.trains().len();
        routed += r
            .trains()
            .iter()
            .filter(|t| t.destination().is_some())
            .count();
        arrivals += r.trains().iter().map(|t| t.arrivals().len()).sum::<usize>();
    }
    assert!(trains_seen >= 6, "{trains_seen} trains over all seeds");
    assert!(routed >= 3, "{routed} trains with a destination");
    assert!(arrivals >= 3, "{arrivals} arrivals over all seeds");
}

/// The level crossing (phase 4): a road between two house clusters crosses a five-tile line with a
/// shuttling train. Cars hold short of the crossing while it is closed, and no car is ever on the
/// deck while a train is over it.
#[test]
fn cars_wait_at_the_level_crossing_while_a_train_is_near() {
    let c = catalog();
    let mut sim = Simulation::new(3, SimConfig::default());
    let mut put = |cell: Axial, id: &str, rot: u8| sim.apply_place(cell, c.get(id).unwrap(), rot);
    put(Axial::new(0, 0), "rail_road_crossing", 0);
    put(Axial::new(0, 1), "house_cluster", 4);
    put(Axial::new(0, -1), "house_cluster", 1);
    for q in [-2, -1, 1, 2] {
        put(Axial::new(q, 0), "rail_straight", 0);
    }
    let at = sim.rails().crossings()[0].at();
    assert!(
        geom::dist(at, [0.0, 0.0]) < 0.01,
        "the crossing point is the tile centre"
    );
    let (mut held, mut closed_ticks, mut trips) = (0, 0, BTreeSet::new());
    for tick in 0..600 * TICK_HZ {
        let before: Vec<(u64, f32)> = sim
            .cars()
            .iter()
            .map(|c| (c.id().0, c.distance()))
            .collect();
        sim.tick();
        let closed = sim.rails().crossings()[0].closed();
        closed_ticks += usize::from(closed);
        for car in sim.cars() {
            trips.insert(car.id().0);
            let was = before.iter().find(|(id, _)| *id == car.id().0).map(|b| b.1);
            for &(_, stop) in car.crossings() {
                if closed && was == Some(car.distance()) && (car.distance() - stop).abs() < 1e-3 {
                    held += 1;
                }
            }
            // Never on the deck while a train is over it.
            let p = car.pose_at(1.0).pos;
            if geom::dist(p, at) < 4.0 {
                for t in sim.rails().trains() {
                    for v in t.vehicles(1.0) {
                        let half = if v.kind == hex_sim::VehicleKind::Engine {
                            4.47
                        } else {
                            3.57
                        };
                        assert!(
                            geom::dist(v.pose.pos, at) > half + 1.5,
                            "tick {tick}: car {} on the deck at {p:?} with {:?} over it",
                            car.id().0,
                            t.id()
                        );
                    }
                }
            }
        }
    }
    assert!(!sim.rails().trains().is_empty(), "the line has a train");
    assert!(closed_ticks > 0, "the train never closed the crossing");
    assert!(trips.len() >= 3, "{} trips", trips.len());
    assert!(held > 0, "no car ever waited at the crossing");
}
