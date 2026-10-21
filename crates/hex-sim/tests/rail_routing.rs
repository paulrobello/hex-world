//! Train routing (P3): trains head for station stops in turn, plan their path over the rail graph,
//! throw each junction to the leg their route needs, turn round only at line ends and stations,
//! and never lock each other up for good.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::*;
use hex_core::{Axial, TileCatalog, World};
use hex_sim::rail::{Blocker, Leg, Railway, STUCK_SECONDS, Train, TrainId};
use hex_sim::{LinkId, SimConfig, Simulation, TICK_HZ, geom};

fn place(r: &mut Railway, c: &TileCatalog, cell: Axial, id: &str, rot: u8) {
    r.insert_tile(cell, c.get(id).unwrap_or_else(|| panic!("{id}")), rot);
}

/// The three stations off one junction: the junction at the origin (toe east, straight leg west,
/// branch south-east), a station on each of its three arms and the line ending beyond each. 128 m
/// of track wants one train; it stands on the west station facing the junction (the spawner finds
/// no place clear of the junction zone on arms this short).
const EAST: Axial = Axial::new(1, 0);
const WEST: Axial = Axial::new(-1, 0);
const SOUTH: Axial = Axial::new(0, 1);

fn three_stations(r: &mut Railway, c: &TileCatalog) {
    place(r, c, Axial::ORIGIN, "rail_junction", 0);
    place(r, c, EAST, "rail_station", 0);
    place(r, c, WEST, "rail_station", 0);
    place(r, c, SOUTH, "rail_station", 1);
    let (l, fwd) = eastward(r, WEST);
    let len = r.network().link(l).unwrap().length();
    r.spawn_at(l, fwd, if fwd { len - 1.0 } else { 1.0 })
        .expect("a train fits on the west station");
}

/// Two stations off a junction with two tiles to each arm (the straight leg west and the branch
/// south-east, each with a station on its far tile, the toe arm plain track): 211 m, two trains.
fn two_stations_long(r: &mut Railway, c: &TileCatalog) {
    place(r, c, Axial::ORIGIN, "rail_junction", 0);
    for q in [1, 2] {
        place(r, c, Axial::new(q, 0), "rail_straight", 0);
    }
    place(r, c, Axial::new(-1, 0), "rail_straight", 0);
    place(r, c, Axial::new(-2, 0), "rail_station", 0);
    place(r, c, Axial::new(0, 1), "rail_straight", 1);
    place(r, c, Axial::new(0, 2), "rail_station", 1);
}

fn run(r: &mut Railway, seconds: u32) {
    for _ in 0..seconds * TICK_HZ {
        r.tick();
    }
}

/// No two trains share track.
fn assert_apart(r: &Railway) {
    let bodies: Vec<_> = r.trains().iter().map(|t| (t.id(), r.body(t))).collect();
    for (i, (a, ba)) in bodies.iter().enumerate() {
        for (b, bb) in &bodies[i + 1..] {
            for &(la, lo, hi) in ba {
                for &(lb, lo2, hi2) in bb {
                    assert!(
                        la != lb || hi < lo2 || hi2 < lo,
                        "{a:?} and {b:?} overlap on {la:?}"
                    );
                }
            }
        }
    }
}

/// At every junction: at most one train in the fouling zone, it holds the junction, and a train on
/// a leg's part of the zone has the switch set to that leg (it never runs through a switch set
/// against it).
fn assert_switches(r: &Railway, tick: u32) {
    for j in r.junctions() {
        let inside: Vec<&Train> = r.trains().iter().filter(|t| r.in_zone(j, t)).collect();
        assert!(
            inside.len() <= 1,
            "tick {tick}: {} trains in the zone of {:?}",
            inside.len(),
            j.cell()
        );
        for t in inside {
            assert_eq!(
                j.holder(),
                Some(t.id()),
                "tick {tick}: in the zone unreserved"
            );
            for leg in r.legs_under(j, t) {
                assert_eq!(
                    j.set(),
                    leg,
                    "tick {tick}: {:?} on the {leg:?} leg of {:?} with the switch set against it",
                    t.id(),
                    j.cell()
                );
            }
        }
    }
}

/// The leg a facing train takes toward each station of the three-station layout: the branch for
/// the south station, the straight leg for the west one (the east one is on the toe arm).
fn leg_toward(cell: Axial) -> Option<Leg> {
    match cell {
        c if c == SOUTH => Some(Leg::Branch),
        c if c == WEST => Some(Leg::Straight),
        _ => None,
    }
}

/// One train, three stations off a junction: it visits the stations in turn (each arrival is the
/// stop after the last in stop order), every facing move through the junction takes the leg its
/// destination needs, so both legs are used, and the switch is never against it.
#[test]
fn a_train_visits_each_station_in_turn_taking_the_leg_its_route_needs() {
    let c = catalog();
    let mut r = Railway::new();
    three_stations(&mut r, &c);
    run(&mut r, 1);
    assert_eq!(r.trains().len(), 1, "128 m of track holds one train");
    let order: Vec<Axial> = r.stops().iter().map(|s| s.cell()).collect();
    assert_eq!(order.len(), 3);
    assert!(
        r.trains()[0].destination().is_some(),
        "a destination from the start"
    );
    let mut facing = 0;
    let mut legs: BTreeSet<&'static str> = BTreeSet::new();
    let mut seen = r.trains()[0].routes().to_vec();
    for tick in 0..900 * TICK_HZ {
        r.tick();
        assert_switches(&r, tick);
        let t = &r.trains()[0];
        if t.routes() != seen.as_slice() {
            // A facing move (one junction, so at most one a tick): the leg toward the destination.
            let &(_, leg) = t.routes().last().unwrap();
            let dest = t.destination().expect("a routed train");
            assert_eq!(
                Some(leg),
                leg_toward(dest),
                "tick {tick}: took {leg:?} heading for {dest:?}"
            );
            facing += 1;
            legs.insert(leg.as_str());
            seen = t.routes().to_vec();
        }
    }
    let t = &r.trains()[0];
    let arrivals = t.arrivals();
    assert!(arrivals.len() >= 6, "{arrivals:?}");
    for w in arrivals.windows(2) {
        let k = order.iter().position(|&c| c == w[0]).unwrap();
        assert_eq!(
            w[1],
            order[(k + 1) % order.len()],
            "{arrivals:?} out of turn"
        );
    }
    let visited: BTreeSet<Axial> = arrivals.iter().copied().collect();
    assert_eq!(visited.len(), 3, "{arrivals:?}");
    assert!(facing >= 2, "{facing} facing moves");
    assert_eq!(legs.len(), 2, "both legs used: {legs:?}");
}

/// The route's length runs down as the train runs toward its destination and the destination is
/// always one of the stops.
#[test]
fn the_route_runs_down_toward_the_destination() {
    let c = catalog();
    let mut r = Railway::new();
    three_stations(&mut r, &c);
    run(&mut r, 1);
    let mut last: Option<(Axial, f32)> = None;
    let mut fell = 0;
    for _ in 0..300 * TICK_HZ {
        r.tick();
        let t = &r.trains()[0];
        let dest = t.destination().expect("a destination");
        assert!(r.stops().iter().any(|s| s.cell() == dest));
        let Some(m) = r.route_length(t) else {
            continue;
        };
        assert!((0.0..2000.0).contains(&m), "{m}");
        if let Some((d, prev)) = last
            && d == dest
            && t.speed() > 0.0
            && m < prev
        {
            fell += 1;
        }
        last = Some((dest, m));
    }
    assert!(fell > 100, "the route shortened on {fell} ticks");
}

/// Two trains on two stations off a junction: they never share track or a fouling zone, the
/// switch is never set against a train on a leg, neither stands still for good, and both reach
/// destinations.
#[test]
fn two_trains_with_crossing_routes_never_share_the_fouling_zone() {
    let c = catalog();
    let mut r = Railway::new();
    two_stations_long(&mut r, &c);
    run(&mut r, 1);
    assert_eq!(r.trains().len(), 2, "211 m holds two trains");
    let mut last_moved: BTreeMap<TrainId, u32> = BTreeMap::new();
    let mut waits = 0;
    for tick in 0..900 * TICK_HZ {
        r.tick();
        assert_apart(&r);
        assert_switches(&r, tick);
        for t in r.trains() {
            if matches!(t.blocker(), Blocker::Junction(_)) {
                waits += 1;
            }
            let moved = last_moved.entry(t.id()).or_insert(tick);
            if t.speed() > 0.0 {
                *moved = tick;
            }
            assert!(
                tick - *moved < 45 * TICK_HZ,
                "tick {tick}: {:?} stopped for 45 s ({:?})",
                t.id(),
                t.blocker()
            );
        }
    }
    assert!(waits > 0, "no train ever waited for the junction");
    for t in r.trains() {
        assert!(!t.arrivals().is_empty(), "{:?} never arrived", t.id());
    }
    let stations: BTreeSet<Axial> = r
        .trains()
        .iter()
        .flat_map(|t| t.arrivals().iter().copied())
        .collect();
    assert_eq!(stations.len(), 2, "{stations:?}");
}

/// A loop with two stations: trains stop at both, take them as destinations, and never turn round
/// (a station on a stretch with no line end is no place to turn).
#[test]
fn trains_on_a_loop_with_stations_never_turn_round() {
    let c = catalog();
    let mut r = Railway::new();
    let tiles = hex_triangle();
    // The first straight of the first and second sides become stations.
    let mut stations = vec![];
    for (k, (cell, id, rot)) in tiles.iter().enumerate() {
        if *id == "rail_straight" && (k == 1 || k == 4) {
            place(&mut r, &c, *cell, "rail_station", *rot);
            stations.push(*cell);
        } else {
            place(&mut r, &c, *cell, id, *rot);
        }
    }
    assert_eq!(r.stops().len(), 2, "{stations:?}");
    run(&mut r, 1);
    assert_eq!(r.trains().len(), 2);
    for _ in 0..600 * TICK_HZ {
        r.tick();
        assert_apart(&r);
    }
    for t in r.trains() {
        assert_eq!(t.reversals(), 0, "{:?} turned round on a loop", t.id());
        assert!(t.stops() >= 4, "{:?} stopped {} times", t.id(), t.stops());
        assert!(t.arrivals().len() >= 2, "{:?}: {:?}", t.id(), t.arrivals());
    }
}

/// The triangle loop of `tests/rail.rs` (two straights a side).
fn hex_triangle() -> Vec<(Axial, &'static str, u8)> {
    let (q0, r0, n) = (-1, -1, 2);
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

/// Two junctions joined by four tiles of single track, each with a station on both legs. Returns
/// the west junction's west station and the east junction's east station.
fn dumbbell(r: &mut Railway, c: &TileCatalog) -> (Axial, Axial) {
    place(r, c, Axial::ORIGIN, "rail_junction", 0);
    place(r, c, Axial::new(-1, 0), "rail_station", 0);
    place(r, c, Axial::ORIGIN.neighbor(1), "rail_station", 1);
    for q in 1..=4 {
        place(r, c, Axial::new(q, 0), "rail_straight", 0);
    }
    let east = Axial::new(5, 0);
    place(r, c, east, "rail_junction", 3);
    place(r, c, Axial::new(6, 0), "rail_station", 0);
    place(r, c, east.neighbor(4), "rail_station", 1);
    (Axial::new(-1, 0), Axial::new(6, 0))
}

/// The link on `cell` and whether running it forward heads east (+x).
fn eastward(r: &Railway, cell: Axial) -> (LinkId, bool) {
    let (id, l) = r.network().links_of(cell).next().unwrap();
    let p = l.points();
    (id, p[p.len() - 1][0] > p[0][0])
}

/// Deadlock safety: two trains on the single track between two junctions, heading toward each
/// other for stations past each other. They stop nose to nose, one gives way within a few
/// seconds of [`STUCK_SECONDS`], neither stands still for 40 s at any time, and both go on to
/// reach destinations.
#[test]
fn trains_meeting_head_on_on_single_track_never_deadlock() {
    let c = catalog();
    let mut r = Railway::new();
    let (west, east) = dumbbell(&mut r, &c);
    // A heads east on (2, 0), B west on (3, 0), fronts 5 m either side of their shared seam.
    let (la, fa) = eastward(&r, Axial::new(2, 0));
    let (lb, fb) = eastward(&r, Axial::new(3, 0));
    let len = r.network().link(la).unwrap().length();
    let a = r
        .spawn_at(la, fa, if fa { len - 5.0 } else { 5.0 })
        .unwrap();
    let b = r
        .spawn_at(lb, !fb, if fb { 5.0 } else { len - 5.0 })
        .unwrap();
    assert!(r.send(a, east), "A heads for the east station");
    assert!(r.send(b, west), "B heads for the west station");
    let mut gave_way: Option<u32> = None;
    let mut last_moved: BTreeMap<TrainId, u32> = BTreeMap::new();
    let mut stuck_seen = false;
    for tick in 0..900 * TICK_HZ {
        r.tick();
        assert_apart(&r);
        assert_switches(&r, tick);
        assert_eq!(r.trains().len(), 2, "no train spawned or lost");
        if r.trains()
            .iter()
            .all(|t| matches!(t.blocker(), Blocker::Train(_)))
        {
            stuck_seen = true;
        }
        if gave_way.is_none() && r.trains().iter().any(|t| t.reversals() > 0) {
            gave_way = Some(tick);
        }
        for t in r.trains() {
            let moved = last_moved.entry(t.id()).or_insert(tick);
            if t.speed() > 0.0 {
                *moved = tick;
            }
            assert!(
                tick - *moved < 40 * TICK_HZ,
                "tick {tick}: {:?} stopped for 40 s ({:?})",
                t.id(),
                t.blocker()
            );
        }
    }
    assert!(stuck_seen, "the trains never met nose to nose");
    let first = gave_way.expect("one gave way");
    assert!(
        first < ((STUCK_SECONDS + 5.0) * TICK_HZ as f32) as u32,
        "gave way only at tick {first}"
    );
    for t in r.trains() {
        assert!(t.stops() >= 2, "{:?} made {} stops", t.id(), t.stops());
        assert!(!t.arrivals().is_empty(), "{:?} never arrived", t.id());
    }
}

/// Bulldozing a tile on a train's route (not under it) replans at once: the route never names a
/// link that is gone, and a destination cut off is replaced.
#[test]
fn an_edit_on_the_route_replans_at_once() {
    let c = catalog();
    let mut r = Railway::new();
    three_stations(&mut r, &c);
    run(&mut r, 1);
    // Run until the train heads somewhere its body is not.
    let mut cut = None;
    for _ in 0..120 * TICK_HZ {
        r.tick();
        let t = &r.trains()[0];
        let under: BTreeSet<Axial> = t.steps().iter().map(|s| s.link.cell).collect();
        if let Some(d) = t.destination()
            && !under.contains(&d)
            && d != Axial::ORIGIN
        {
            cut = Some(d);
            break;
        }
    }
    let cut = cut.expect("the train heads for a station away from it");
    r.remove_tile(cut);
    let t = &r.trains()[0];
    assert_ne!(t.destination(), Some(cut));
    for s in t.route() {
        assert!(
            r.network().link(s.link).is_some(),
            "a dead link on the route"
        );
    }
    let _ = r.route_length(t);
    run(&mut r, 120);
    assert!(r.trains()[0].destination() != Some(cut));
}

/// With no station a train keeps following the track: no destination, no route.
#[test]
fn without_a_station_a_train_has_no_destination() {
    let c = catalog();
    let mut r = Railway::new();
    for q in 0..3 {
        place(&mut r, &c, Axial::new(q, 0), "rail_straight", 0);
    }
    run(&mut r, 30);
    let t = &r.trains()[0];
    assert_eq!(t.destination(), None);
    assert!(t.route().is_empty());
    assert_eq!(r.route_length(t), None);
}

/// Random legal layouts of the rail tiles with stations: trains on them get destinations, and
/// a world replayed from the same seed runs its trains identically (fronts, destinations and
/// routes), so nothing about routing needs saving.
#[test]
fn routing_is_deterministic_per_world_seed() {
    let c = catalog();
    let mut routed = 0;
    for seed in 0..8u64 {
        let world = random_world(&c, seed);
        let mut a = Simulation::from_world(&world, &c, SimConfig::default());
        let mut b = Simulation::from_world(&world, &c, SimConfig::default());
        for _ in 0..120 * TICK_HZ {
            a.tick();
            b.tick();
        }
        let snap = |s: &Simulation| -> Vec<(u64, [f32; 2], Option<Axial>, usize)> {
            s.rails()
                .trains()
                .iter()
                .map(|t| (t.id().0, t.front(), t.destination(), t.route().len()))
                .collect()
        };
        assert_eq!(snap(&a), snap(&b), "seed {seed}");
        routed += a
            .rails()
            .trains()
            .iter()
            .filter(|t| t.destination().is_some())
            .count();
    }
    assert!(routed >= 3, "{routed} routed trains over all seeds");
}

/// A random world of rail tiles (stations among them) grown from the start meadow.
pub fn random_world(c: &TileCatalog, seed: u64) -> World {
    const RAIL: [&str; 6] = [
        "rail_straight",
        "rail_bend_60",
        "rail_junction",
        "rail_station",
        "rail_straight",
        "rail_bend_60",
    ];
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
        let rots = world.legal_rotations(c, cell, def);
        if rots.is_empty() {
            let meadow = c.get("meadow").unwrap();
            if world.fits(c, cell, meadow, 0) {
                world.place(c, cell, "meadow", 0).unwrap();
            }
            continue;
        }
        let rot = rots[next(rots.len())];
        world.place(c, cell, &def.id, rot).unwrap();
    }
    world
}

/// The front of a train in world XZ never jumps: every tick it moves at most a line-speed step
/// (a turn round moves the front to the old tail, which the reversal count marks).
#[test]
fn routed_trains_move_smoothly_or_turn_round() {
    let c = catalog();
    let mut r = Railway::new();
    three_stations(&mut r, &c);
    run(&mut r, 1);
    let mut prev = (r.trains()[0].front(), r.trains()[0].reversals());
    for tick in 0..300 * TICK_HZ {
        r.tick();
        let t = &r.trains()[0];
        let now = (t.front(), t.reversals());
        if now.1 == prev.1 {
            let d = geom::dist(now.0, prev.0);
            assert!(d < 0.5, "tick {tick}: the front jumped {d} m");
        }
        prev = now;
    }
}

/// The two-station dumbbell with 3.5 m buffer stops: the trains want each other's terminal across
/// one headshunt, so they can never swap. A train giving way for good used to take the nearest
/// station by route, which was the one the other train stood at, so it followed that train in and
/// out of the same branch for good and never arrived. It now takes the nearest station no other
/// train stands at: each train reaches a station within the 900 s run.
#[test]
fn two_trains_swapping_terminals_across_one_headshunt_do_not_livelock() {
    let c = catalog();
    let mut r = Railway::new();
    two_stations_long(&mut r, &c);
    run(&mut r, 1);
    assert_eq!(r.trains().len(), 2);
    let mut first: BTreeMap<TrainId, u32> = BTreeMap::new();
    for tick in 0..900 * TICK_HZ {
        r.tick();
        for t in r.trains() {
            if !t.arrivals().is_empty() {
                first.entry(t.id()).or_insert(tick / TICK_HZ);
            }
        }
    }
    eprintln!("first arrivals (s): {first:?}");
    assert_eq!(first.len(), 2, "first arrivals (s): {first:?}");
}
