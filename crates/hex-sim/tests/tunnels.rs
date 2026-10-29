//! Cars through tunnels (P4): the hidden segment between two facing portals' arches, routing over
//! it, the fade inside, and the segment following every world edit including undo and redo.

mod common;

use common::*;
use hex_core::Axial;
use hex_sim::route::find_route;
use hex_sim::{LinkId, Network, NodeId, SimConfig, geom};

fn hidden_links(net: &Network) -> Vec<LinkId> {
    net.links()
        .filter(|(_, l)| l.hidden())
        .map(|(id, _)| id)
        .collect()
}

fn arch(net: &Network, cell: Axial) -> NodeId {
    net.local_node(cell, "arch").expect("portal arch node")
}

fn houses(net: &Network, cell: Axial) -> NodeId {
    net.local_node(cell, "houses").expect("house node")
}

/// Ticks `scene` until some driving car is routed over `link`, at most `max` ticks.
fn tick_until_car_on(scene: &mut Scene, link: LinkId, max: usize) -> bool {
    for _ in 0..max {
        scene.sim.tick();
        if scene.sim.cars().iter().any(|c| c.links().contains(&link)) {
            return true;
        }
    }
    false
}

#[test]
fn route_runs_through_a_tunnel() {
    let scene = Scene::with(1, SimConfig::default(), &tunnel_layout());
    let roads = scene.roads();
    let hidden = hidden_links(roads);
    assert_eq!(hidden.len(), 1, "one tunnel, one hidden segment");
    let link = roads.link(hidden[0]).expect("listed");
    let (a, b) = (arch(roads, PORTAL_A), arch(roads, PORTAL_B));
    let ends = link.ends();
    assert!(ends == (a, b) || ends == (b, a), "arch to arch: {ends:?}");
    // A straight 2D segment between the arches.
    assert_eq!(link.points().len(), 2);
    let (pa, pb) = (roads.node(a).unwrap().pos(), roads.node(b).unwrap().pos());
    assert!((link.length() - geom::dist(pa, pb)).abs() < 1e-4);

    let (from, to) = (houses(roads, TUNNEL_HOUSE_A), houses(roads, TUNNEL_HOUSE_B));
    for (x, y) in [(from, to), (to, from)] {
        let route = find_route(roads, x, y).expect("the tunnel joins the houses");
        assert!(route.links.contains(&hidden[0]), "{route:?}");
    }
}

#[test]
fn open_portal_has_no_through_route() {
    // Without the last cover meadow, both portals are open.
    let mut edits = tunnel_layout();
    edits.pop();
    let mut scene = Scene::with(1, SimConfig::default(), &edits);
    let roads = scene.roads();
    assert!(hidden_links(roads).is_empty());
    for portal in [PORTAL_A, PORTAL_B] {
        let node = roads.node(arch(roads, portal)).unwrap();
        assert_eq!(node.links().len(), 1, "the arch is a dead end");
        assert!(node.destination().is_none(), "the arch is no destination");
    }
    let (from, to) = (houses(roads, TUNNEL_HOUSE_A), houses(roads, TUNNEL_HOUSE_B));
    assert!(find_route(roads, from, to).is_none());
    assert_eq!(scene.sim.connected_destinations(), 0);
    for _ in 0..400 {
        scene.sim.tick();
        assert!(scene.sim.cars().is_empty(), "no trip has anywhere to go");
    }
}

#[test]
fn cars_are_hidden_inside() {
    let mut scene = Scene::with(1, SimConfig::default(), &tunnel_layout());
    let link = hidden_links(scene.roads())[0];
    assert!(
        tick_until_car_on(&mut scene, link, 4000),
        "no car took the tunnel"
    );
    let car = scene
        .sim
        .cars()
        .iter()
        .find(|c| c.links().contains(&link))
        .expect("found above")
        .clone();
    let spans = car.hidden();
    assert_eq!(spans.len(), 1, "{spans:?}");
    let (s_in, s_out) = spans[0];
    assert!(s_in < s_out);
    // The span runs from one arch to the other, a lane offset to the side.
    let roads = scene.roads();
    let arches =
        [arch(roads, PORTAL_A), arch(roads, PORTAL_B)].map(|n| roads.node(n).unwrap().pos());
    let near = |s: f32| {
        let p = car.pose_at_distance(s).pos;
        arches
            .iter()
            .map(|a| geom::dist(*a, p))
            .fold(f32::INFINITY, f32::min)
    };
    let off = scene.sim.config().lane_offset + 0.05;
    assert!(
        near(s_in) <= off && near(s_out) <= off,
        "{} {}",
        near(s_in),
        near(s_out)
    );

    let mid = 0.5 * (s_in + s_out);
    assert_eq!(car.opacity_at_distance(mid), 0.0);
    assert_eq!(car.opacity_at_distance(s_in - 5.0), 1.0);
    assert_eq!(car.opacity_at_distance(s_out + 5.0), 1.0);
    // Linear over the last 3 m before the arch and the first 3 m after it.
    assert!((car.opacity_at_distance(s_in - 1.5) - 0.5).abs() < 1e-4);
    assert!((car.opacity_at_distance(s_out + 1.5) - 0.5).abs() < 1e-4);
    assert_eq!(car.opacity_at_distance(s_in), 0.0);

    // The live car, driven to the middle of the tunnel, is drawn at opacity 0.
    let id = car.id();
    let mut seen = false;
    for _ in 0..4000 {
        scene.sim.tick();
        let Some(c) = scene.sim.cars().iter().find(|c| c.id() == id) else {
            break;
        };
        if (c.distance() - mid).abs() < 4.0 {
            assert_eq!(c.opacity_at(1.0), 0.0);
            seen = true;
            break;
        }
    }
    assert!(seen, "the car never reached the tunnel midpoint");
}

#[test]
fn undo_of_cover_breaks_the_route() {
    let mut scene = Scene::with(1, SimConfig::default(), &tunnel_layout());
    let link = hidden_links(scene.roads())[0];
    assert!(
        tick_until_car_on(&mut scene, link, 4000),
        "no car took the tunnel"
    );
    let owned = (
        scene.sim.owned_by(TUNNEL_HOUSE_A),
        scene.sim.owned_by(TUNNEL_HOUSE_B),
    );

    // Undo the cover meadow that completed the tunnel.
    scene.undo();
    scene.sim.tick();
    let roads = scene.roads();
    assert!(hidden_links(roads).is_empty(), "the hidden segment went");
    for car in scene.sim.cars() {
        assert!(
            car.links().iter().all(|l| roads.link(*l).is_some()),
            "car {:?} still routed over a removed link",
            car.id()
        );
        assert!(!car.links().contains(&link));
    }
    let (from, to) = (houses(roads, TUNNEL_HOUSE_A), houses(roads, TUNNEL_HOUSE_B));
    assert!(find_route(roads, from, to).is_none());
    // The houses keep their cars: the dropped ones come back parked.
    assert_eq!(
        (
            scene.sim.owned_by(TUNNEL_HOUSE_A),
            scene.sim.owned_by(TUNNEL_HOUSE_B),
        ),
        owned
    );

    // Redo brings the tunnel back.
    scene.redo();
    assert_eq!(hidden_links(scene.roads()).len(), 1);
}
