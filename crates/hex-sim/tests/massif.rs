//! A massif (P4 mega tile, A52) carries no network: the sim gains no node or link from one placed
//! beside a road loop, and removing it from a ring cell removes nothing either.

mod common;

use common::*;
use hex_core::{Axial, NetworkKind, Orientation, World};
use hex_sim::Simulation;

const MASSIF: &str = "massif_granite";

/// Node and link counts of every network the sim builds.
fn counts(sim: &Simulation) -> Vec<(usize, usize)> {
    [NetworkKind::Road, NetworkKind::River, NetworkKind::Rail]
        .into_iter()
        .map(|k| {
            let net = sim.network(k);
            (net.nodes().count(), net.links().count())
        })
        .collect()
}

#[test]
fn massif_adds_no_nodes() {
    let loop_only = Scene::with(1, Default::default(), &ring(&[0, 2, 4]));
    let c = &loop_only.catalog;
    let def = c.get(MASSIF).unwrap();
    let (centre, legal) = loop_only.world.mega_slots(c, def).remove(0);
    let o: Orientation = legal[0];
    let roads = counts(&loop_only.sim);
    assert!(roads[0].0 > 0, "the loop has road nodes");

    let mut world: World = loop_only.world.clone();
    world.place(c, centre, MASSIF, o).unwrap();
    let built = Simulation::from_world(&world, c, Default::default());
    assert_eq!(
        counts(&built),
        roads,
        "built from a world holding the massif"
    );

    let mut scene = Scene::with(1, Default::default(), &ring(&[0, 2, 4]));
    scene.apply(&Edit::PlaceAt(centre, MASSIF, o.rotation, o.level));
    assert_eq!(counts(&scene.sim), roads, "placed into a running sim");
    let ring_cell: Axial = centre.neighbor(3);
    assert_eq!(scene.world.anchor_of(ring_cell), Some(centre));
    scene.apply(&Edit::Bulldoze(ring_cell));
    assert!(scene.world.tile(centre).is_none());
    assert_eq!(counts(&scene.sim), roads, "bulldozed from a ring cell");
}

/// No road reaches a massif (its outer sockets are grass), so no car drives or parks on a cell it
/// covers: the loop's cars run a few thousand ticks with the massif placed beside it.
#[test]
fn no_car_route_touches_a_massif() {
    let mut scene = Scene::with(1, Default::default(), &ring(&[0, 2, 4]));
    let def = scene.catalog.get(MASSIF).unwrap();
    let (centre, legal) = scene.world.mega_slots(&scene.catalog, def).remove(0);
    scene.apply(&Edit::PlaceAt(
        centre,
        MASSIF,
        legal[0].rotation,
        legal[0].level,
    ));
    let cover = hex_core::footprint::members(centre);
    let road = |c: Axial| {
        scene.world.tile(c).is_some_and(|t| {
            scene
                .catalog
                .get(&t.def)
                .is_some_and(|d| d.networks.iter().any(|n| n.kind == NetworkKind::Road))
        })
    };
    assert!(
        cover[1..]
            .iter()
            .any(|m| (0..6).any(|d| road(m.neighbor(d)))),
        "the massif's ring touches the road loop"
    );
    // A point lies in the cell whose centre is nearest; a cover cell is nearest among the cover
    // and its neighbours exactly when the point is on it.
    let candidates: Vec<Axial> = cover
        .iter()
        .flat_map(|c| std::iter::once(*c).chain((0..6).map(|d| c.neighbor(d))))
        .collect();
    let on_cover = |p: [f32; 2]| {
        let d2 = |c: &Axial| {
            let [x, z] = c.to_world(hex_core::HEX_RADIUS);
            (x - p[0]).powi(2) + (z - p[1]).powi(2)
        };
        let nearest = candidates
            .iter()
            .min_by(|a, b| d2(a).total_cmp(&d2(b)))
            .unwrap();
        cover.contains(nearest)
    };
    let mut seen = 0;
    for _ in 0..3000 {
        scene.sim.tick();
        for car in scene.sim.cars() {
            let p = car.pose_at(1.0).pos;
            assert!(!on_cover(p), "a car at {p:?} on the massif");
            seen += 1;
        }
        for parked in scene.sim.parked() {
            let p = parked.pose().pos;
            assert!(!on_cover(p), "a car parked at {p:?} on the massif");
        }
    }
    assert!(seen > 0, "cars drove the loop");
}
