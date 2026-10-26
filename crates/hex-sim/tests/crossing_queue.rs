//! Cars queue at a closed level crossing (owner report, 2026-10-21): each held car stands at
//! least a car length and a gap behind the car ahead on its approach, and the queue pulls away in
//! order when the crossing opens, without overlapping.

mod common;

use common::*;
use hex_core::Axial;
use hex_sim::rail::{CAR_CROSSING_STOP, CAR_LENGTH, CAR_QUEUE_SPACING};
use hex_sim::{CarPhase, SimConfig, Simulation, geom};

/// The five-tile line of `tests/rail.rs` with its shuttling train, the road crossing it at the
/// origin. South of the crossing a road junction feeds two house clusters (four cars, the world's
/// whole cap); north of it a junction leads to two more clusters with no cars, so all six of their
/// spots are free. No seeded departures: the test sends the cars itself.
fn layout(seed: u64) -> Simulation {
    let c = catalog();
    let mut sim = Simulation::new(
        seed,
        SimConfig {
            spawn_per_destination_hz: 0.0,
            ..SimConfig::default()
        },
    );
    let put = |sim: &mut Simulation, cell: Axial, id: &str, rot: u8| {
        sim.apply_place(cell, c.get(id).unwrap(), rot)
    };
    put(&mut sim, Axial::new(0, 0), "rail_road_crossing", 0);
    for q in [-2, -1, 1, 2] {
        put(&mut sim, Axial::new(q, 0), "rail_straight", 0);
    }
    put(&mut sim, Axial::new(0, 1), "road_straight", 1);
    put(&mut sim, Axial::new(0, 2), "road_junction", 0);
    put(&mut sim, Axial::new(1, 2), "house_cluster", 3);
    put(&mut sim, Axial::new(-1, 3), "house_cluster", 5);
    sim.set_max_cars(4, 0);
    put(&mut sim, Axial::new(0, -1), "road_junction", 1);
    put(&mut sim, Axial::new(-1, -1), "house_cluster", 0);
    put(&mut sim, Axial::new(1, -2), "house_cluster", 2);
    sim
}

/// One run's trace: every driving car's `(tick, id, distance)` each tick.
type Trace = Vec<(u64, u64, u32)>;

/// What a run saw: the most cars held at once, the order cars were first held in, the order they
/// crossed, and the trace.
struct Run {
    most_held: usize,
    held_order: Vec<u64>,
    cross_order: Vec<u64>,
    trace: Trace,
}

/// Sends the four southern cars north, 1.25 s apart, timed (from the shuttle's first closure at
/// tick 388, seed 3) to reach the crossing while it is closed, and checks the queue every tick.
fn run(seed: u64) -> Run {
    let mut sim = layout(seed);
    let ids: Vec<_> = sim.parked().iter().map(|c| c.id()).collect();
    assert_eq!(ids.len(), 4, "the southern clusters own four cars");
    let mut out = Run {
        most_held: 0,
        held_order: vec![],
        cross_order: vec![],
        trace: vec![],
    };
    let mut closed_ticks = 0;
    for tick in 0..900u64 {
        if let Some(k) = (tick >= 300 && (tick - 300) % 25 == 0)
            .then(|| ((tick - 300) / 25) as usize)
            .filter(|k| *k < ids.len())
        {
            let to = if k % 2 == 0 {
                Axial::new(-1, -1)
            } else {
                Axial::new(1, -2)
            };
            assert!(sim.send(ids[k], to).is_some(), "car {k} found no spot");
        }
        let before: Vec<(u64, f32)> = sim
            .cars()
            .iter()
            .map(|c| (c.id().0, c.distance()))
            .collect();
        sim.tick();
        let closed = sim.rails().crossings()[0].closed();
        closed_ticks += usize::from(closed);
        let cars = sim.cars();
        for car in cars {
            out.trace
                .push((sim.ticks(), car.id().0, car.distance().to_bits()));
        }

        // Held: standing still short of the closed crossing.
        let held: Vec<u64> = cars
            .iter()
            .filter(|c| {
                let was = before.iter().find(|b| b.0 == c.id().0).map(|b| b.1);
                closed && was == Some(c.distance())
            })
            .map(|c| c.id().0)
            .collect();
        out.most_held = out.most_held.max(held.len());
        for id in &held {
            if !out.held_order.contains(id) {
                out.held_order.push(*id);
            }
        }
        for car in cars {
            let &(_, stop) = &car.crossings()[0];
            let at = stop + CAR_CROSSING_STOP;
            let was = before.iter().find(|b| b.0 == car.id().0).map(|b| b.1);
            if was.is_some_and(|w| w < at) && car.distance() >= at {
                assert!(
                    !closed,
                    "tick {tick}: car {} crossed while closed",
                    car.id().0
                );
                out.cross_order.push(car.id().0);
            }
        }

        // Never overlapping: two cars heading the same way on the road stand at least a car
        // length apart, centre to centre, and two held on one approach at least the queue
        // spacing (the approach is straight, so the chord is the distance along the lane).
        for (i, a) in cars.iter().enumerate() {
            for b in &cars[i + 1..] {
                if a.phase() != CarPhase::Road || b.phase() != CarPhase::Road {
                    continue;
                }
                let (pa, pb) = (a.pose_at(1.0), b.pose_at(1.0));
                if geom::dot(pa.dir, pb.dir) < 0.7 {
                    continue;
                }
                let d = geom::dist(pa.pos, pb.pos);
                assert!(
                    d >= CAR_LENGTH,
                    "tick {tick}: cars {} and {} overlap, {d:.2} m apart",
                    a.id().0,
                    b.id().0
                );
                if held.contains(&a.id().0) && held.contains(&b.id().0) {
                    assert!(
                        d >= CAR_QUEUE_SPACING - 0.01,
                        "tick {tick}: held cars {} and {} only {d:.2} m apart",
                        a.id().0,
                        b.id().0
                    );
                }
            }
        }
    }
    assert!(closed_ticks > 0, "the train never closed the crossing");
    out
}

#[test]
fn cars_queue_at_a_closed_crossing_and_leave_in_order() {
    let a = run(3);
    assert_eq!(a.most_held, 4, "all four cars queue at once");
    assert_eq!(a.held_order.len(), 4, "{:?}", a.held_order);
    assert_eq!(
        a.cross_order, a.held_order,
        "the queue pulls away in the order it formed"
    );
    let b = run(3);
    assert_eq!(a.trace, b.trace, "the same seed gives the same queue");
}
