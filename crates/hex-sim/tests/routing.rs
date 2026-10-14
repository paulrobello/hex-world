//! Routing: existence, shortest routes against a Dijkstra oracle, disconnected destinations and
//! route-cache invalidation.

mod common;

use common::*;
use hex_core::Axial;
use hex_sim::Network;
use hex_sim::route::find_route;

fn house(net: &Network, cell: Axial) -> hex_sim::NodeId {
    net.local_node(cell, "houses").expect("house node")
}

#[test]
fn house_chain_route_runs_between_the_house_nodes() {
    let scene = Scene::with(1, Default::default(), &house_chain());
    let mut net = scene.roads().clone();
    let (a, b) = (house(&net, Axial::new(1, 0)), house(&net, Axial::new(3, 0)));
    let route = net.route(a, b).expect("connected houses");
    // Each house node sits 2 m from its cell center, so 4 apothems minus 2 × 2 m.
    assert!((route.length - (4.0 * APOTHEM - 4.0)).abs() < 1e-3);
    assert_eq!(route.nodes.first(), Some(&a));
    assert_eq!(route.nodes.last(), Some(&b));
    assert_eq!(route.links.len(), 3);

    let back = net.route(b, a).expect("routes are undirected");
    assert!((back.length - route.length).abs() < 1e-4);
    let line = back.centerline(&net);
    assert!(geom_close(line[0], net.node(b).unwrap().pos()));
    assert!(geom_close(
        *line.last().unwrap(),
        net.node(a).unwrap().pos()
    ));
}

fn geom_close(a: [f32; 2], b: [f32; 2]) -> bool {
    hex_sim::geom::dist(a, b) < 1e-3
}

#[test]
fn ring_route_takes_the_shorter_way_around() {
    let scene = Scene::with(2, Default::default(), &ring(&[0, 2]));
    let mut net = scene.roads().clone();
    let (a, b) = (house(&net, ring_house(0)), house(&net, ring_house(2)));
    let route = net.route(a, b).expect("ring is connected");
    let cells: Vec<Axial> = route.links.iter().map(|l| l.cell).collect();
    assert!(cells.contains(&ring_cell(1)), "{cells:?}");
    assert!(!cells.contains(&ring_cell(4)), "{cells:?}");
    let oracle = dijkstra(&net, a);
    assert!((route.length - oracle[&b]).abs() < 1e-3);
}

#[test]
fn a_star_matches_dijkstra_on_random_worlds() {
    for seed in 0..6u64 {
        let mut scene = Scene::new(seed, Default::default());
        let mut rng = TestRng(seed);
        for _ in 0..250 {
            if let Some(edit) = random_edit(&scene.world, &scene.catalog, &mut rng) {
                scene.apply(&edit);
            }
        }
        let net = scene.roads();
        let nodes: Vec<_> = net.nodes().map(|(id, _)| id).collect();
        assert!(nodes.len() > 20, "seed {seed}: only {} nodes", nodes.len());
        for (i, &from) in nodes.iter().enumerate().step_by(7) {
            let oracle = dijkstra(net, from);
            for &to in nodes.iter().skip(i % 5).step_by(5) {
                let got = find_route(net, from, to);
                match (oracle.get(&to), got) {
                    (Some(d), Some(r)) => {
                        assert!(
                            (r.length - d).abs() < 1e-2,
                            "seed {seed}: {} vs {d}",
                            r.length
                        );
                        let sum: f32 = r.links.iter().map(|l| net.link(*l).unwrap().length()).sum();
                        assert!((sum - r.length).abs() < 1e-2);
                    }
                    (None, None) => {}
                    (d, r) => panic!("seed {seed}: oracle {d:?}, A* {:?}", r.map(|r| r.length)),
                }
            }
        }
    }
}

#[test]
fn destination_in_another_component_has_no_route() {
    let scene = Scene::with(4, Default::default(), &ring_and_pair());
    let mut net = scene.roads().clone();
    let ring_home = house(&net, ring_house(0));
    let pair_home = house(&net, Axial::new(4, 0));
    assert!(net.route(ring_home, pair_home).is_none());
    assert!(find_route(&net, ring_home, pair_home).is_none());
    assert!(
        net.route(pair_home, house(&net, Axial::new(5, 0)))
            .is_some()
    );

    let comps = net.components();
    assert_eq!(comps.count(), 2);
    assert_eq!(comps.connected_destinations().len(), 5);
}

#[test]
fn lone_house_has_no_route_partner() {
    let scene = Scene::with(
        6,
        Default::default(),
        &[Edit::Place(Axial::new(1, 0), "house_cluster", 0)],
    );
    let mut net = scene.roads().clone();
    assert_eq!(net.destinations().count(), 1);
    assert!(net.components().connected_destinations().is_empty());
}

#[test]
fn route_cache_follows_bulldoze_and_rebuild() {
    let mut scene = Scene::with(7, Default::default(), &ring(&[0, 2]));
    let (a, b) = {
        let net = scene.roads();
        (house(net, ring_house(0)), house(net, ring_house(2)))
    };
    let mut net = scene.roads().clone();
    let short = net.route(a, b).unwrap();
    assert_eq!(net.cached_routes(), 1);
    assert!(std::sync::Arc::ptr_eq(&short, &net.route(a, b).unwrap()));

    // Bulldoze the one-tile arc between the junctions: the route must go the long way round.
    scene.apply(&Edit::Bulldoze(ring_cell(1)));
    let mut cut = scene.roads().clone();
    let long = cut.route(a, b).expect("the long way remains");
    assert!(long.length > short.length + 40.0);
    assert!(!long.links.iter().any(|l| l.cell == ring_cell(1)));

    // In place: a stale cached route through the bulldozed tile must not survive.
    let mut live = net.clone();
    let def = scene.catalog.get("road_bend_120").unwrap();
    live.remove_tile(ring_cell(1));
    assert_eq!(live.cached_routes(), 0);
    let rerouted = live.route(a, b).unwrap();
    assert!((rerouted.length - long.length).abs() < 1e-3);

    // A cached "no route" must not survive a placement that connects the destinations.
    let mut split = live.clone();
    split.remove_tile(ring_cell(3));
    assert!(split.route(a, b).is_none());
    assert_eq!(split.cached_routes(), 1);
    split.insert_tile(ring_cell(1), def, 3);
    assert_eq!(split.cached_routes(), 0);
    let restored = split.route(a, b).expect("re-placed arc reconnects");
    assert!((restored.length - short.length).abs() < 1e-3);
}
