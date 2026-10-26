//! Network graph: seam stitching for every rotation, segment shapes, and incremental updates.

mod common;

use common::*;
use hex_core::{Axial, Destination, HEX_RADIUS, NetworkKind};
use hex_sim::geom::{self, Vec2};
use hex_sim::{Link, Network};

fn close(a: Vec2, b: Vec2) -> bool {
    geom::dist(a, b) < 1e-3
}

/// The seam midpoint from cell centers alone, independent of the rotation math under test.
fn seam_mid(a: Axial, b: Axial) -> Vec2 {
    geom::scale(
        geom::add(a.to_world(HEX_RADIUS), b.to_world(HEX_RADIUS)),
        0.5,
    )
}

fn has_end_at(link: &Link, p: Vec2) -> bool {
    let pts = link.points();
    close(pts[0], p) || close(pts[pts.len() - 1], p)
}

fn only_link(net: &Network, cell: Axial) -> &Link {
    let links: Vec<_> = net.links_of(cell).collect();
    assert_eq!(links.len(), 1, "{cell:?}");
    links[0].1
}

#[test]
fn road_straight_pair_stitches_across_the_seam_at_every_rotation() {
    let cat = catalog();
    for k in 0..6u8 {
        let a = Axial::ORIGIN;
        let b = a.neighbor(k);
        for rot_b in [k % 3, k % 3 + 3] {
            let mut net = Network::new(NetworkKind::Road);
            put(&mut net, &cat, a, "road_straight", k);
            put(&mut net, &cat, b, "road_straight", rot_b);

            assert_eq!(net.nodes().count(), 3, "k{k} rot_b{rot_b}");
            assert_eq!(net.links().count(), 2);
            let seam = net.seam_node(a, k).expect("seam node");
            assert_eq!(net.seam_node(b, (k + 3) % 6), Some(seam));
            assert_eq!(net.node(seam).unwrap().links().len(), 2);

            let mid = seam_mid(a, b);
            assert!(close(net.node(seam).unwrap().pos(), mid));
            assert!(
                has_end_at(only_link(&net, a), mid),
                "k{k}: A misses the seam"
            );
            assert!(
                has_end_at(only_link(&net, b), mid),
                "k{k}: B misses the seam"
            );

            // The far ends sit on the far seams, and the stitched road is two tiles long.
            let far_a = net.seam_node(a, (k + 3) % 6).expect("far end of A");
            let far_b = net.seam_node(b, k).expect("far end of B");
            assert!(close(
                net.node(far_a).unwrap().pos(),
                seam_mid(a, a.neighbor((k + 3) % 6))
            ));
            assert!(close(
                net.node(far_b).unwrap().pos(),
                seam_mid(b, b.neighbor(k))
            ));
            let route = net.route(far_a, far_b).expect("stitched route");
            assert!((route.length - 4.0 * APOTHEM).abs() < 1e-3);
            assert_eq!(route.links.len(), 2);
        }
    }
}

#[test]
fn road_ends_that_do_not_face_each_other_stay_apart() {
    let cat = catalog();
    let mut net = Network::new(NetworkKind::Road);
    put(&mut net, &cat, Axial::ORIGIN, "road_straight", 0);
    // Neighbor across edge 0 with its road on edges 1 and 4: nothing meets at the seam.
    put(
        &mut net,
        &cat,
        Axial::ORIGIN.neighbor(0),
        "road_straight",
        1,
    );
    assert_eq!(net.nodes().count(), 4);
    assert_eq!(net.components().count(), 2);
    let seam = net.seam_node(Axial::ORIGIN, 0).unwrap();
    assert_eq!(net.node(seam).unwrap().links().len(), 1);
}

/// Centre-line length of a road bend whose lane rays leave the hex center `alpha` apart: the two
/// straight edge stubs (`road_straight_edge`; road_bend_60's trimmed `road_bend_60_edge_e0`/`_e1`
/// share its centre line) from each edge midpoint in to r = 6, joined by the arc
/// tangent to both, radius 6 tan(alpha / 2) turning through pi - alpha. The tile polyline samples
/// that arc every 10 degrees, so its chords fall short by at most rho (pi - alpha) d^2 / 24
/// (d = 10 degrees): 0.009 m and 0.014 m, inside the 0.02 m tolerance.
fn stub_tangent_bend_length(alpha: f32) -> f32 {
    let rho = 6.0 * (alpha / 2.0).tan();
    2.0 * (APOTHEM - 6.0) + rho * (std::f32::consts::PI - alpha)
}

#[test]
fn sixty_degree_bend_is_a_stub_tangent_arc_between_adjacent_edges() {
    let cat = catalog();
    // 2 (8 sqrt 3 - 6) + 2 sqrt 3 * 2 pi / 3 = 15.713 + 7.255 = 22.968 m.
    let want = stub_tangent_bend_length(std::f32::consts::FRAC_PI_3);
    assert!((want - 22.968).abs() < 1e-3, "{want}");
    for k in 0..6u8 {
        let mut net = Network::new(NetworkKind::Road);
        put(&mut net, &cat, Axial::ORIGIN, "road_bend_60", k);
        let link = only_link(&net, Axial::ORIGIN);
        assert!((link.length() - want).abs() < 0.02, "{}", link.length());
        for e in [k, (k + 1) % 6] {
            let mid = seam_mid(Axial::ORIGIN, Axial::ORIGIN.neighbor(e));
            assert!(has_end_at(link, mid), "k{k} edge {e}");
        }
    }
}

#[test]
fn hundred_twenty_degree_bend_is_a_stub_tangent_arc_two_edges_apart() {
    let cat = catalog();
    // 2 (8 sqrt 3 - 6) + 6 sqrt 3 * pi / 3 = 15.713 + 10.883 = 26.596 m.
    let want = stub_tangent_bend_length(2.0 * std::f32::consts::FRAC_PI_3);
    assert!((want - 26.596).abs() < 1e-3, "{want}");
    for k in 0..6u8 {
        let mut net = Network::new(NetworkKind::Road);
        put(&mut net, &cat, Axial::ORIGIN, "road_bend_120", k);
        let link = only_link(&net, Axial::ORIGIN);
        assert!((link.length() - want).abs() < 0.02, "{}", link.length());
        for e in [k, (k + 2) % 6] {
            let mid = seam_mid(Axial::ORIGIN, Axial::ORIGIN.neighbor(e));
            assert!(has_end_at(link, mid), "k{k} edge {e}");
        }
    }
}

/// The wide rail bend (`rail_bend_120_center`) has no edge stub: it meets both lane edges square-on
/// at their midpoints, so it is the arc about the point where the two edge lines meet, the centre of
/// the neighbour across the edge between them, radius APOTHEM tan 60 degrees = 24 m, turning pi / 3
/// over 8 pi = 25.133 m. The tile samples it every 2.5 degrees, so its chords fall short by at most
/// R (pi / 3) d^2 / 24 = 0.002 m and stand off the arc by at most R d^2 / 8 = 0.006 m.
#[test]
fn wide_rail_bend_is_the_24_m_arc_two_edges_apart() {
    let cat = catalog();
    let radius = APOTHEM * 3f32.sqrt();
    let want = radius * std::f32::consts::FRAC_PI_3;
    assert!((want - 25.133).abs() < 1e-3, "{want}");
    for k in 0..6u8 {
        let mut net = Network::new(NetworkKind::Rail);
        put(&mut net, &cat, Axial::ORIGIN, "rail_bend_120", k);
        let link = only_link(&net, Axial::ORIGIN);
        assert!((link.length() - want).abs() < 0.02, "{}", link.length());
        for e in [k, (k + 2) % 6] {
            let mid = seam_mid(Axial::ORIGIN, Axial::ORIGIN.neighbor(e));
            assert!(has_end_at(link, mid), "k{k} edge {e}");
        }
        let centre = Axial::ORIGIN.neighbor((k + 1) % 6).to_world(HEX_RADIUS);
        for p in link.points() {
            let off = geom::dist(*p, centre) - radius;
            assert!(off.abs() < 0.002, "k{k}: {p:?} {off} m off the arc");
        }
    }
}

#[test]
fn bend_continues_a_straight_road_at_both_seams() {
    let cat = catalog();
    let mut net = Network::new(NetworkKind::Road);
    put(&mut net, &cat, Axial::ORIGIN, "road_bend_60", 0);
    put(
        &mut net,
        &cat,
        Axial::ORIGIN.neighbor(0),
        "road_straight",
        0,
    );
    put(
        &mut net,
        &cat,
        Axial::ORIGIN.neighbor(1),
        "road_straight",
        1,
    );
    let from = net.seam_node(Axial::ORIGIN.neighbor(0), 0).unwrap();
    let to = net.seam_node(Axial::ORIGIN.neighbor(1), 1).unwrap();
    let route = net.route(from, to).expect("bend joins the straights");
    let want = 4.0 * APOTHEM + stub_tangent_bend_length(std::f32::consts::FRAC_PI_3);
    assert!((route.length - want).abs() < 0.05);
}

#[test]
fn y_junction_joins_three_roads_at_its_center_node() {
    let cat = catalog();
    for k in [0u8, 1] {
        let mut net = Network::new(NetworkKind::Road);
        put(&mut net, &cat, Axial::ORIGIN, "road_junction", k);
        let arms: Vec<u8> = vec![k, (k + 2) % 6, (k + 4) % 6];
        for &e in &arms {
            put(
                &mut net,
                &cat,
                Axial::ORIGIN.neighbor(e),
                "road_straight",
                e % 3,
            );
        }
        let center = net.local_node(Axial::ORIGIN, "junction").unwrap();
        assert!(close(net.node(center).unwrap().pos(), [0.0, 0.0]));
        assert_eq!(net.node(center).unwrap().links().len(), 3);
        let ends: Vec<_> = arms
            .iter()
            .map(|&e| net.seam_node(Axial::ORIGIN.neighbor(e), e).unwrap())
            .collect();
        for i in 0..3 {
            for j in 0..3 {
                if i == j {
                    continue;
                }
                let r = net.route(ends[i], ends[j]).expect("all arms connect");
                assert!((r.length - 6.0 * APOTHEM).abs() < 1e-3);
                assert!(r.nodes.contains(&center));
            }
        }
    }
}

#[test]
fn house_cluster_chain_exposes_house_destinations() {
    let scene = Scene::with(1, Default::default(), &house_chain());
    let net = scene.roads();
    let houses: Vec<_> = net.destinations().collect();
    assert_eq!(houses.len(), 2);
    for (cell, rot) in [(Axial::new(1, 0), 0u8), (Axial::new(3, 0), 3)] {
        let id = net.local_node(cell, "houses").unwrap();
        assert!(houses.contains(&id));
        let node = net.node(id).unwrap();
        assert_eq!(node.destination(), Some(Destination::House));
        let at = geom::add(cell.to_world(HEX_RADIUS), geom::rotate([12.5, 0.0], rot));
        assert!(close(node.pos(), at));
    }
    // Rivers are a separate graph; this layout has none.
    assert_eq!(scene.sim.network(NetworkKind::River).nodes().count(), 0);
}

#[test]
fn bridge_feeds_road_and_river_graphs_separately() {
    let cat = catalog();
    let mut roads = Network::new(NetworkKind::Road);
    let mut rivers = Network::new(NetworkKind::River);
    for net in [&mut roads, &mut rivers] {
        put(net, &cat, Axial::ORIGIN, "river_road_bridge", 0);
        put(net, &cat, Axial::ORIGIN.neighbor(0), "river_straight", 0);
        put(net, &cat, Axial::ORIGIN.neighbor(1), "road_straight", 1);
    }
    assert_eq!(roads.links().count(), 2);
    assert_eq!(rivers.links().count(), 2);
    assert!(roads.seam_node(Axial::ORIGIN, 1).is_some());
    assert!(roads.seam_node(Axial::ORIGIN, 0).is_none());
    assert_eq!(
        rivers
            .node(rivers.seam_node(Axial::ORIGIN, 0).unwrap())
            .unwrap()
            .links()
            .len(),
        2
    );
}

/// The river fork's three arms meet at one local node of degree 3, and each arm stitches to the
/// river tile across its seam, so the river graph is one connected piece through the fork.
#[test]
fn river_fork_joins_three_river_neighbours_at_one_node() {
    let cat = catalog();
    for rot in [0u8, 1] {
        let mut rivers = Network::new(NetworkKind::River);
        let o = Axial::ORIGIN;
        put(&mut rivers, &cat, o, "river_fork", rot);
        for k in [0u8, 2, 4] {
            let e = (k + rot) % 6;
            let back = (e + 3) % 6;
            put(&mut rivers, &cat, o.neighbor(e), "river_straight", back % 3);
        }
        assert_eq!(rivers.links().count(), 6, "r{rot}");
        let hubs: Vec<_> = rivers
            .nodes()
            .filter(|(_, n)| n.links().len() == 3)
            .collect();
        assert_eq!(hubs.len(), 1, "r{rot}: one junction");
        assert!(close(hubs[0].1.pos(), o.to_world(HEX_RADIUS)));
        for k in [0u8, 2, 4] {
            let seam = rivers.seam_node(o, (k + rot) % 6).expect("arm seam");
            assert_eq!(
                rivers.node(seam).unwrap().links().len(),
                2,
                "r{rot} arm {k}"
            );
        }
    }
}

/// The tight river bend joins two straights into one river graph: its link runs seam to seam along
/// the 1 m stubs and the 7.42 m arc between them (2 m + 7.4226 m x 2 pi / 3 = 17.54 m).
#[test]
fn tight_river_bend_links_its_two_seams() {
    let cat = catalog();
    let mut rivers = Network::new(NetworkKind::River);
    let a = Axial::ORIGIN;
    let b = a.neighbor(0);
    let c = b.neighbor(4);
    put(&mut rivers, &cat, a, "river_straight", 0);
    put(&mut rivers, &cat, b, "river_bend_60", 3);
    put(&mut rivers, &cat, c, "river_straight", 1);
    assert_eq!(rivers.links().count(), 3);
    let bend = only_link(&rivers, b);
    assert!(has_end_at(bend, seam_mid(a, b)) && has_end_at(bend, seam_mid(b, c)));
    let arc = 2.0 + 12.856_406 * (30f32).to_radians().tan() * 2.0 * std::f32::consts::PI / 3.0;
    assert!(
        (bend.length() - arc).abs() < 0.01,
        "{} vs {arc}",
        bend.length()
    );
    let from = rivers.seam_node(a, 3).unwrap();
    let to = rivers.seam_node(c, 1).unwrap();
    assert!(rivers.route(from, to).is_some());
}

#[test]
fn place_and_bulldoze_update_only_the_touched_tile() {
    let cat = catalog();
    let mut net = Network::new(NetworkKind::Road);
    let a = Axial::ORIGIN;
    let b = a.neighbor(0);
    put(&mut net, &cat, a, "road_straight", 0);
    let link_a = only_link(&net, a).clone();
    let seam = net.seam_node(a, 0).unwrap();
    let far_a = net.seam_node(a, 3).unwrap();
    let v0 = net.version();

    put(&mut net, &cat, b, "road_straight", 0);
    assert_eq!(net.version(), v0 + 1);
    assert_eq!(only_link(&net, a), &link_a, "placing B must not touch A");
    assert_eq!(net.seam_node(a, 0), Some(seam), "B reuses A's seam node");
    assert_eq!(net.seam_node(a, 3), Some(far_a));
    let far_b = net.seam_node(b, 0).unwrap();

    assert!(net.remove_tile(b));
    assert_eq!(net.version(), v0 + 2);
    assert_eq!(only_link(&net, a), &link_a);
    assert_eq!(net.seam_node(a, 0), Some(seam), "half-seam stays with A");
    assert_eq!(net.node(seam).unwrap().links().len(), 1);
    assert_eq!(net.node(far_b), None, "B's far end goes with B");
    assert_eq!(net.nodes().count(), 2);

    // Re-placing B allocates a fresh far-end id: ids are never reused.
    put(&mut net, &cat, b, "road_straight", 0);
    let far_b2 = net.seam_node(b, 0).unwrap();
    assert!(far_b2 > far_b);
    assert_eq!(net.seam_node(a, 0), Some(seam));

    // Removing an empty cell changes nothing.
    assert!(!net.remove_tile(Axial::new(9, 9)));
    assert_eq!(net.version(), v0 + 3);
}

#[test]
fn placing_over_an_occupied_cell_replaces_its_segments() {
    let cat = catalog();
    let mut net = Network::new(NetworkKind::Road);
    put(&mut net, &cat, Axial::ORIGIN, "road_straight", 0);
    put(&mut net, &cat, Axial::ORIGIN, "road_junction", 0);
    assert_eq!(net.links_of(Axial::ORIGIN).count(), 3);
    assert_eq!(net.seam_node(Axial::ORIGIN, 3), None);
    assert_eq!(net.nodes().count(), 4);
}

#[test]
fn bulldoze_splits_the_network() {
    let mut scene = Scene::with(3, Default::default(), &house_chain_with_meadows());
    let road = Axial::new(2, 0);
    let (h1, h2) = {
        let net = scene.roads();
        (
            net.local_node(Axial::new(1, 0), "houses").unwrap(),
            net.local_node(Axial::new(3, 0), "houses").unwrap(),
        )
    };
    assert_eq!(scene.sim.connected_destinations(), 2);

    scene.apply(&Edit::Bulldoze(road));
    let mut net = scene.roads().clone();
    assert_eq!(net.links_of(road).count(), 0);
    assert_eq!(net.components().count(), 2);
    assert!(!net.components().connected(h1, h2));
    assert!(net.route(h1, h2).is_none());
    assert_eq!(scene.sim.connected_destinations(), 0);

    scene.apply(&Edit::Place(road, "road_straight", 0));
    assert_eq!(scene.sim.connected_destinations(), 2);
}

#[test]
fn simulation_from_world_matches_incremental_edits() {
    let scene = Scene::with(5, Default::default(), &ring_and_pair());
    let rebuilt = hex_sim::Simulation::from_world(&scene.world, &scene.catalog, Default::default());
    let incremental = scene.roads();
    let bulk = rebuilt.network(NetworkKind::Road);
    let keys = |n: &Network| -> std::collections::BTreeSet<_> {
        n.nodes()
            .map(|(_, node)| (node.key().clone(), node.links().clone()))
            .collect()
    };
    assert_eq!(keys(incremental), keys(bulk));
    assert_eq!(
        incremental.links().map(|(id, _)| id).collect::<Vec<_>>(),
        bulk.links().map(|(id, _)| id).collect::<Vec<_>>()
    );
}
