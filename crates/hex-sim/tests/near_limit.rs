//! The simulation on a world at the decoder's coordinate limit (ENH-004): a road street and a
//! rail ring translated so their cells sit at `q = CELL_LIMIT - 8` build both networks, and 600
//! ticks step without a panic or a non-finite pose. `f32` world positions lose integer precision
//! long before this bound, so rendering a loaded world at such coordinates is a separate, open
//! question (a lower `CELL_LIMIT` would trade reach away for render precision); this test is the
//! simulation's answer.

mod common;

use std::collections::{BTreeMap, VecDeque};

use common::*;
use hex_core::{Axial, NetworkKind, PlacedTile, World};
use hex_sim::{SimConfig, Simulation, TICK_HZ};

/// The decoder's cell bound (hex-core `save.rs`'s `CELL_LIMIT`).
const CELL_LIMIT: i32 = 1 << 24;

/// A rail ring (the rail tests' triangle, two straights per side) plus a road street with homes
/// hanging off it, all one connected world. Every edit is `hex_core`-legal or `Scene` panics.
fn origin_layout() -> Vec<Edit> {
    // The crossing_queue tests' proven layout, translated by (+2, +1) to clear the start tile:
    // a rail line through a road crossing, with a road branch and homes hanging off it. Every
    // edit is `hex_core`-legal or `Scene` panics.
    vec![
        Edit::Place(Axial::new(0, 1), "rail_straight", 0),
        Edit::Place(Axial::new(1, 1), "rail_straight", 0),
        Edit::Place(Axial::new(2, 1), "rail_road_crossing", 0),
        Edit::Place(Axial::new(3, 1), "rail_straight", 0),
        Edit::Place(Axial::new(4, 1), "rail_straight", 0),
        Edit::Place(Axial::new(2, 2), "road_straight", 1),
        Edit::Place(Axial::new(2, 3), "road_junction", 0),
        Edit::Place(Axial::new(3, 3), "house_cluster", 3),
        Edit::Place(Axial::new(1, 4), "house_cluster", 5),
        Edit::Place(Axial::new(2, 0), "road_junction", 1),
        Edit::Place(Axial::new(1, 0), "house_cluster", 0),
        Edit::Place(Axial::new(3, -1), "house_cluster", 2),
    ]
}

fn translated(world: &World, dq: i32) -> World {
    let tiles: BTreeMap<Axial, PlacedTile> = world
        .tiles()
        .iter()
        .map(|(&c, t)| (Axial::new(c.q + dq, c.r), t.clone()))
        .collect();
    World::from_parts(world.seed(), tiles, VecDeque::new(), Vec::new())
}

#[test]
fn the_simulation_runs_600_ticks_near_the_coordinate_limit() {
    let c = catalog();
    // Played at the origin first, so the layout is one the game could place.
    let origin = Scene::with(7, SimConfig::default(), &origin_layout()).world;
    let max_q = origin.tiles().keys().map(|c| c.q).max().unwrap_or(0);
    let near_limit = translated(&origin, CELL_LIMIT - 8 - max_q);
    assert!(
        near_limit
            .tiles()
            .keys()
            .all(|c| (-CELL_LIMIT..=CELL_LIMIT).contains(&c.q)),
        "every cell stays inside the decoder's bound"
    );

    let mut sim = Simulation::from_world(&near_limit, &c, SimConfig::default());
    sim.set_max_cars(4, 0);
    assert!(
        sim.network(NetworkKind::Road).nodes().next().is_some(),
        "the road network built"
    );
    assert!(
        sim.network(NetworkKind::Rail).nodes().next().is_some(),
        "the rail network built"
    );
    let ticks = 600 * TICK_HZ as u64;
    for _ in 0..ticks {
        sim.tick();
    }
    assert_eq!(sim.ticks(), ticks);
    // Finite everywhere a car or a node sits, whatever the f32 precision cost at this distance:
    // the simulation must never produce NaN or infinity.
    for car in sim.cars() {
        let pose = car.pose_at_distance(car.distance());
        assert!(
            pose.pos[0].is_finite() && pose.pos[1].is_finite(),
            "a driving car's pose stays finite"
        );
    }
    for kind in [NetworkKind::Road, NetworkKind::Rail] {
        for (_, node) in sim.network(kind).nodes() {
            let pos = node.pos();
            assert!(
                pos[0].is_finite() && pos[1].is_finite(),
                "a network node stays finite"
            );
        }
    }
}
