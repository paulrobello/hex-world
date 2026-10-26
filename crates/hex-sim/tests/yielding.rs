//! Car following and merge yielding (PRD §7.2): a car keeps the queue spacing behind the car ahead
//! on its lane, gives way where routes merge (a junction entry, a driveway joining a lane, two
//! departures from one cluster), never deadlocks and stays deterministic per seed.

mod common;

use std::collections::BTreeMap;

use common::*;
use hex_core::Axial;
use hex_sim::rail::CAR_LENGTH;
use hex_sim::{Car, CarId, CarPhase, SimConfig, Simulation, geom};

/// The width of the car body the sim sizes for (m), with its [`CAR_LENGTH`].
const CAR_WIDTH: f32 = 2.0;

/// Whether `b` is oncoming traffic for `a`: where `b` stands, `a`'s path runs the opposite way
/// (more than about 150° from `b`'s heading), so `b` drives the other lane of `a`'s road. Their
/// bodies passing close in a junction's tight turns is the lanes' geometry, which the follow and
/// merge rules leave alone.
fn oncoming(a: &Car, b: &Car) -> bool {
    let pb = b.pose_at(1.0);
    let (_, s) = hex_sim::rail::closest_distance(a.path().points(), pb.pos);
    geom::dot(a.path().pose_at_distance(s).dir, pb.dir) < -0.85
}

/// The centre distance of two driving cars whose 5.4 x 2 m bodies overlap at any angle (each
/// shrunk by 5 cm a side), by the separating axis test on the two boxes; oncoming pairs are left
/// out ([`oncoming`]).
fn overlapping(a: &Car, b: &Car) -> Option<f32> {
    let (pa, pb) = (a.pose_at(1.0), b.pose_at(1.0));
    let half = [0.5 * CAR_LENGTH - 0.05, 0.5 * CAR_WIDTH - 0.05];
    let axes = |d: [f32; 2]| [d, [-d[1], d[0]]];
    let (ua, ub) = (axes(pa.dir), axes(pb.dir));
    let d = geom::sub(pb.pos, pa.pos);
    let radius = |u: &[[f32; 2]; 2], axis: [f32; 2]| {
        half[0] * geom::dot(u[0], axis).abs() + half[1] * geom::dot(u[1], axis).abs()
    };
    let separated = ua
        .iter()
        .chain(ub.iter())
        .any(|&axis| geom::dot(d, axis).abs() > radius(&ua, axis) + radius(&ub, axis));
    (!separated && !oncoming(a, b) && !oncoming(b, a)).then_some(geom::dist(pa.pos, pb.pos))
}

/// What a run saw, checked every tick.
#[derive(Debug, Default)]
struct Watch {
    /// `(tick, a, b, centre distance)` for every overlapping pair.
    overlaps: Vec<(u64, u64, u64, f32)>,
    /// The longest any car stood still mid-trip, in ticks, and which car.
    longest_stop: (u64, u64),
    /// Ticks each car has stood still for now.
    stopped: BTreeMap<CarId, u64>,
    /// Ticks each car stood still on the road lanes, over the run.
    road_stops: BTreeMap<u64, u64>,
    /// Ticks each car waited in its spot before pulling out, over the run.
    spot_waits: BTreeMap<u64, u64>,
    /// Trips finished.
    arrivals: u64,
    /// Every driving car's `(tick, id, distance bits)`.
    trace: Vec<(u64, u64, u32)>,
}

impl Watch {
    fn tick(&mut self, sim: &mut Simulation) {
        let before: BTreeMap<CarId, f32> =
            sim.cars().iter().map(|c| (c.id(), c.distance())).collect();
        sim.tick();
        let t = sim.ticks();
        let cars = sim.cars();
        let now: BTreeMap<CarId, &Car> = cars.iter().map(|c| (c.id(), c)).collect();
        self.arrivals += before.keys().filter(|id| !now.contains_key(id)).count() as u64;
        for car in cars {
            self.trace.push((t, car.id().0, car.distance().to_bits()));
            if before.get(&car.id()) == Some(&car.distance()) && !car.arrived() {
                let n = self.stopped.entry(car.id()).or_insert(0);
                *n += 1;
                if *n > self.longest_stop.0 {
                    self.longest_stop = (*n, car.id().0);
                }
                if car.phase() == CarPhase::Road {
                    *self.road_stops.entry(car.id().0).or_insert(0) += 1;
                }
                if car.distance() == 0.0 {
                    *self.spot_waits.entry(car.id().0).or_insert(0) += 1;
                }
            } else {
                self.stopped.remove(&car.id());
            }
        }
        self.stopped.retain(|id, _| now.contains_key(id));
        for (i, a) in cars.iter().enumerate() {
            for b in &cars[i + 1..] {
                if let Some(d) = overlapping(a, b) {
                    self.overlaps.push((t, a.id().0, b.id().0, d));
                }
            }
        }
    }
}

fn quiet() -> SimConfig {
    SimConfig {
        spawn_per_destination_hz: 0.0,
        ..SimConfig::default()
    }
}

fn busy() -> SimConfig {
    SimConfig {
        spawn_per_destination_hz: 0.5,
        ..SimConfig::default()
    }
}

fn place(sim: &mut Simulation, cell: Axial, id: &str, rot: u8) {
    let c = catalog();
    sim.apply_place(cell, c.get(id).unwrap(), rot);
}

/// The parked cars of the cluster at `cell`, in id order.
fn parked_at(sim: &Simulation, cell: Axial) -> Vec<CarId> {
    sim.parked()
        .iter()
        .filter(|c| c.spot().cell == cell)
        .map(|c| c.id())
        .collect()
}

const EAST: Axial = Axial::new(1, 0);
const SOUTH_WEST: Axial = Axial::new(-1, 1);
const NORTH: Axial = Axial::new(0, -1);

/// A Y junction at the origin with house clusters on its east, south-west and north arms. The
/// east and south-west clusters own the world's four cars (ids 0 and 1 east, 2 and 3 south-west),
/// so the north one has three free spots.
fn y_junction(seed: u64) -> Simulation {
    let mut sim = Simulation::new(seed, quiet());
    place(&mut sim, Axial::new(0, 0), "road_junction", 0);
    place(&mut sim, EAST, "house_cluster", 3);
    place(&mut sim, SOUTH_WEST, "house_cluster", 5);
    sim.set_max_cars(4, 0);
    place(&mut sim, NORTH, "house_cluster", 1);
    sim
}

/// Sends car `x` north at tick 20 and car `y` north `d` ticks later (earlier when `d` is
/// negative), watching until both have parked.
fn merge_run(x: usize, y: usize, d: i64) -> Watch {
    let mut sim = y_junction(1);
    let ids: Vec<CarId> = parked_at(&sim, EAST)
        .into_iter()
        .chain(parked_at(&sim, SOUTH_WEST))
        .collect();
    let mut w = Watch::default();
    for t in 0..900i64 {
        if t == 20 {
            assert!(sim.send(ids[x], NORTH).is_some());
        }
        if t == 20 + d {
            assert!(sim.send(ids[y], NORTH).is_some());
        }
        w.tick(&mut sim);
    }
    assert!(sim.cars().is_empty(), "both trips finish");
    w
}

/// Two cars from different arms of a Y junction bound for the same arm: whatever their timing,
/// they never overlap and both arrive; at the same moment the lower id goes first (every turn at a
/// Y is 60°, so neither is through traffic) and the other stands on the road until it can follow
/// at the queue spacing.
#[test]
fn cars_yield_where_routes_merge_at_a_junction() {
    for d in -15..=15 {
        let w = merge_run(0, 2, d);
        assert!(w.overlaps.is_empty(), "d {d}: {:?}", w.overlaps);
        assert_eq!(w.arrivals, 2, "d {d}");
        assert!(w.road_stops.len() <= 1, "d {d}: only one car gives way");
    }
    let w = merge_run(0, 2, 0);
    assert!(!w.road_stops.contains_key(&0), "{:?}", w.road_stops);
    assert!(
        w.road_stops.get(&2).is_some_and(|n| *n >= 5),
        "{:?}",
        w.road_stops
    );
    // The same holds from the other spots.
    let w = merge_run(1, 3, 0);
    assert!(!w.road_stops.contains_key(&1), "{:?}", w.road_stops);
    assert!(
        w.road_stops.get(&3).is_some_and(|n| *n >= 5),
        "{:?}",
        w.road_stops
    );
}

const BAKERY: Axial = Axial::new(0, 0);
const WEST_HOUSES: Axial = Axial::new(-1, 0);
const EAST_HOUSES: Axial = Axial::new(1, 0);

/// The bakery on a straight road between two house clusters, west and east; the west one owns the
/// world's two cars (ids 0 and 1).
fn bakery_road(seed: u64) -> Simulation {
    let mut sim = Simulation::new(seed, quiet());
    place(&mut sim, WEST_HOUSES, "house_cluster", 0);
    sim.set_max_cars(2, 0);
    place(&mut sim, BAKERY, "bakery", 0);
    place(&mut sim, EAST_HOUSES, "house_cluster", 3);
    sim
}

/// Parks car 0 at the bakery, then sends it on east 6 s later while car 1 drives from the west
/// houses to the east ones past the bakery, sent `d` ticks after the visitor leaves.
fn driveway_run(d: i64) -> Watch {
    let mut sim = bakery_road(1);
    let west = parked_at(&sim, WEST_HOUSES);
    let (visitor, through) = (west[0], west[1]);
    sim.send(visitor, BAKERY).unwrap();
    while !parked_at(&sim, BAKERY).contains(&visitor) {
        sim.tick();
    }
    let t0 = sim.ticks() as i64 + 120;
    let mut w = Watch::default();
    for _ in 0..900 {
        let t = sim.ticks() as i64;
        if t == t0 {
            sim.send(visitor, EAST_HOUSES).unwrap();
        }
        if t == t0 + d {
            sim.send(through, EAST_HOUSES).unwrap();
        }
        w.tick(&mut sim);
    }
    assert_eq!(
        parked_at(&sim, EAST_HOUSES),
        vec![visitor, through],
        "d {d}: {:?} {:?}",
        w.longest_stop,
        sim.cars()
            .iter()
            .map(|c| (c.id(), c.phase(), c.distance(), c.to_spot()))
            .collect::<Vec<_>>()
    );
    w
}

/// A visitor leaving the bakery joins the through road: through traffic goes first, so with a car
/// coming along the road the visitor waits on its way out until the car has passed, and the car
/// never stops. Whatever the timing, the two never overlap.
#[test]
fn a_driveway_join_gives_way_to_road_traffic() {
    for d in (-110..=10).step_by(3) {
        let w = driveway_run(d);
        assert!(w.overlaps.is_empty(), "d {d}: {:?}", w.overlaps);
        assert!(
            !w.road_stops.contains_key(&1),
            "d {d}: the through car never stops"
        );
    }
    let w = driveway_run(-50);
    assert!(
        w.longest_stop.1 == 0 && w.longest_stop.0 >= 5 && w.road_stops.is_empty(),
        "the visitor waits on its way out for the through car: {:?}",
        w.longest_stop
    );
}

/// The five-tile line of `tests/crossing_queue.rs` with its shuttling train, the road crossing it at
/// the origin. Just south of the crossing a junction at (0, 1) turns off east to a house cluster
/// with no cars; behind it two more junctions feed three house clusters that own the world's six
/// cars. North of the crossing a junction leads to two clusters with no cars.
fn crossing_and_junction(seed: u64) -> Simulation {
    let mut sim = Simulation::new(seed, quiet());
    place(&mut sim, Axial::new(0, 0), "rail_road_crossing", 0);
    for q in [-2, -1, 1, 2] {
        place(&mut sim, Axial::new(q, 0), "rail_straight", 0);
    }
    place(&mut sim, Axial::new(0, 1), "road_junction", 0);
    place(&mut sim, Axial::new(-1, 2), "road_junction", 1);
    place(&mut sim, Axial::new(-2, 2), "road_junction", 0);
    place(&mut sim, Axial::new(-1, 3), "house_cluster", 4);
    place(&mut sim, Axial::new(-3, 3), "house_cluster", 5);
    place(&mut sim, Axial::new(-2, 1), "house_cluster", 1);
    sim.set_max_cars(6, 0);
    place(&mut sim, Axial::new(1, 1), "house_cluster", 3);
    place(&mut sim, Axial::new(0, -1), "road_junction", 1);
    place(&mut sim, Axial::new(-1, -1), "house_cluster", 0);
    place(&mut sim, Axial::new(1, -2), "house_cluster", 2);
    sim
}

/// F2: four cars queue at the closed crossing (timed, as in `tests/crossing_queue.rs`, to the
/// shuttle's first closure at tick 388 with seed 3) and the queue reaches back into the junction
/// south of it. A fifth car on the same lane, turning off at that junction and never crossing,
/// stops behind the queue instead of driving through its last car, and goes on once the queue
/// pulls away.
#[test]
fn a_crossing_queue_past_a_junction_holds_the_cars_behind_it() {
    let mut sim = crossing_and_junction(3);
    let ids: Vec<CarId> = sim.parked().iter().map(|c| c.id()).collect();
    let turner = ids[4];
    let mut w = Watch::default();
    let mut turner_held = 0;
    let mut tail_in_junction = false;
    let junction = geom::cell_center(Axial::new(0, 1));
    for _ in 0..1200u64 {
        let t = sim.ticks();
        for (k, id) in ids.iter().enumerate().take(4) {
            if t == 300 + 25 * k as u64 {
                let to = if k % 2 == 0 {
                    Axial::new(-1, -1)
                } else {
                    Axial::new(1, -2)
                };
                assert!(sim.send(*id, to).is_some());
            }
        }
        if t == 400 {
            assert!(sim.send(turner, Axial::new(1, 1)).is_some());
        }
        let was = sim
            .cars()
            .iter()
            .find(|c| c.id() == turner)
            .map(|c| c.distance());
        w.tick(&mut sim);
        let closed = sim.rails().crossings()[0].closed();
        let cars = sim.cars();
        if let Some(c) = cars.iter().find(|c| c.id() == turner)
            && closed
            && was == Some(c.distance())
            && c.phase() == CarPhase::Road
        {
            turner_held += 1;
        }
        if closed
            && cars
                .iter()
                .any(|c| c.id() == ids[3] && geom::dist(c.pose_at(1.0).pos, junction) < 3.0)
        {
            tail_in_junction = true;
        }
    }
    assert!(tail_in_junction, "the queue reaches back into the junction");
    assert!(
        turner_held >= 5,
        "the turning car waits behind the queue ({turner_held} ticks)"
    );
    assert!(w.overlaps.is_empty(), "{:?}", w.overlaps);
    assert_eq!(w.arrivals, 5, "every trip finishes");
    assert_eq!(parked_at(&sim, Axial::new(1, 1)), vec![turner]);
}

/// F3: both cars of one cluster sent off on the same tick never overlap: the higher id waits in its
/// spot while the lower id backs out and drives off, then follows it at the queue spacing.
#[test]
fn two_departures_from_one_cluster_never_overlap() {
    for (a, b) in [(0, 1), (2, 3)] {
        let mut sim = y_junction(1);
        let from = if a == 0 { EAST } else { SOUTH_WEST };
        let ids = parked_at(&sim, from);
        assert_eq!((ids[0].0, ids[1].0), (a, b));
        assert!(sim.send(ids[0], NORTH).is_some());
        assert!(sim.send(ids[1], NORTH).is_some());
        let mut w = Watch::default();
        for _ in 0..900 {
            w.tick(&mut sim);
        }
        assert!(w.overlaps.is_empty(), "{:?}", w.overlaps);
        assert_eq!(w.arrivals, 2);
        assert!(!w.spot_waits.contains_key(&a), "{:?}", w.spot_waits);
        assert!(
            w.spot_waits.get(&b).is_some_and(|n| *n >= 5),
            "{:?}",
            w.spot_waits
        );
    }
}

/// Merges that feed each other never deadlock: two and six junctions on a ring, each merge's
/// traffic driving on to the next, under ten times the default departure rate, and the 300-car,
/// 1,000-junction zigzag of `tests/cars.rs`. No car stands still for 30 s and trips keep finishing.
/// Bodies stay apart but for a few ticks in the busiest worlds: in a Y junction's tight turns a
/// car may pass within a body of a car waiting behind its leader at the junction's mouth, their
/// paths more than [`hex_sim::traffic::LANE_CLEAR`] apart (lane geometry, PRD §7.2); the count is
/// bounded here so a regression shows.
#[test]
fn merges_feeding_each_other_never_deadlock() {
    const SWING_TICKS: usize = 60;
    for (name, mut sim, ticks) in [
        (
            "two merges",
            Scene::with(5, busy(), &ring(&[0, 3])).sim,
            12_000,
        ),
        (
            "six merges",
            Scene::with(5, busy(), &ring(&[0, 1, 2, 3, 4, 5])).sim,
            12_000,
        ),
        ("zigzag", zigzag(99), 1_500),
    ] {
        let mut w = Watch::default();
        let mut last_half = 0;
        for t in 0..ticks {
            w.tick(&mut sim);
            if t == ticks / 2 {
                last_half = w.arrivals;
            }
        }
        println!(
            "{name}: {} overlapping pair-ticks {:?}, longest stop {:?}, arrivals {}",
            w.overlaps.len(),
            w.overlaps.first(),
            w.longest_stop,
            w.arrivals
        );
        assert!(
            w.overlaps.len() <= SWING_TICKS,
            "{name}: {:?}",
            &w.overlaps[..1]
        );
        assert!(
            w.longest_stop.0 < 600,
            "{name}: car {} stood still for {} ticks",
            w.longest_stop.1,
            w.longest_stop.0
        );
        if name != "zigzag" {
            assert!(
                w.arrivals - last_half > 20,
                "{name}: trips keep finishing ({} then {})",
                last_half,
                w.arrivals
            );
        }
    }
}

/// Evidence for `docs/checkpoints/car-yield/`: writes `car-yield.hexworld` (seed 1, the six-junction
/// ring of `ring`, camera and settings from the crossing-queue save) and prints every tick, within
/// the first five minutes, at which a car stands still on the road within 14 m of a ring junction
/// while another car drives within 12 m of it. Run with
/// `cargo test -p hex-sim --test yielding car_yield_evidence -- --ignored --nocapture`.
#[test]
#[ignore]
fn car_yield_evidence() {
    let cat = catalog();
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/checkpoints");
    let base = std::fs::read(format!("{dir}/crossing-queue/crossing-queue.hexworld")).unwrap();
    let mut save = hex_core::SaveData::decode(&base, &cat).unwrap();
    save.world = Scene::with(1, SimConfig::default(), &ring(&[0, 1, 2, 3, 4, 5])).world;
    std::fs::create_dir_all(format!("{dir}/car-yield")).unwrap();
    std::fs::write(format!("{dir}/car-yield/car-yield.hexworld"), save.encode()).unwrap();
    let mut sim = Simulation::from_world(&save.world, &cat, SimConfig::default());
    for _ in 0..6000 {
        let before: BTreeMap<CarId, f32> =
            sim.cars().iter().map(|c| (c.id(), c.distance())).collect();
        sim.tick();
        for c in sim.cars() {
            if c.phase() != CarPhase::Road || before.get(&c.id()) != Some(&c.distance()) {
                continue;
            }
            let p = c.pose_at(1.0).pos;
            let Some(d) = (0..6u8).find(|&d| geom::dist(p, geom::cell_center(ring_cell(d))) < 14.0)
            else {
                continue;
            };
            for o in sim.cars() {
                let q = o.pose_at(1.0).pos;
                if o.id() != c.id()
                    && before.get(&o.id()).is_some_and(|s| *s != o.distance())
                    && geom::dist(p, q) < 12.0
                {
                    println!(
                        "tick {} junction {d}: car {} waits at ({:.1}, {:.1}), car {} passes at ({:.1}, {:.1})",
                        sim.ticks(),
                        c.id().0,
                        p[0],
                        p[1],
                        o.id().0,
                        q[0],
                        q[1]
                    );
                }
            }
        }
    }
}

/// The same seed gives the same traffic, tick for tick, with merges, following and the yard rule
/// all in play.
#[test]
fn the_same_seed_gives_the_same_traffic() {
    let run = |seed: u64| {
        let mut sim = Scene::with(seed, busy(), &ring(&[0, 1, 2, 3, 4, 5])).sim;
        let mut w = Watch::default();
        for _ in 0..3000 {
            w.tick(&mut sim);
        }
        assert!(!w.road_stops.is_empty(), "cars gave way on the road");
        w.trace
    };
    assert_eq!(run(11), run(11));
}

/// The 2,000-tile zigzag of `tests/cars.rs`: 1,000 junctions, each with a house on its free arm.
fn zigzag(seed: u64) -> Simulation {
    let cat = catalog();
    let mut sim = Simulation::new(
        seed,
        SimConfig {
            spawn_per_destination_hz: 0.2,
            ..Default::default()
        },
    );
    let junction = cat.get("road_junction").unwrap();
    let house = cat.get("house_cluster").unwrap();
    for i in 0..500i32 {
        let even = Axial::new(2 * i, -i);
        sim.apply_place(even, junction, 0);
        sim.apply_place(even.neighbor(4), house, 1);
        let odd = Axial::new(2 * i + 1, -i);
        sim.apply_place(odd, junction, 1);
        sim.apply_place(odd.neighbor(1), house, 4);
    }
    sim
}
