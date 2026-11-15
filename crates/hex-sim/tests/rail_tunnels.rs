//! Trains through rail tunnels (P4, rail at levels): the hidden link between two facing rail
//! portals' arches, a train running over it, the vehicles fading at the arches, and the link
//! following the cover being bulldozed.

mod common;

use common::*;
use hex_core::{Axial, NetworkKind};
use hex_sim::{LinkId, SimConfig};

const PORTAL_A: Axial = Axial::new(1, 2);
const PORTAL_B: Axial = Axial::new(1, 5);

/// Rail down the q = 1 column (world edges 1 and 4) beside the start meadow, which the first
/// straight's grass edge faces: straights at (1,0) and (1,1), a portal at (1,2) facing +r,
/// level-1 meadow cover at (1,3) and (1,4), a portal at (1,5) facing back, straights at (1,6)
/// and (1,7).
fn rail_tunnel_layout() -> Vec<Edit> {
    vec![
        Edit::Place(Axial::new(1, 0), "rail_straight", 1),
        Edit::Place(Axial::new(1, 1), "rail_straight", 1),
        Edit::Place(PORTAL_A, "rail_tunnel_portal", 1),
        Edit::PlaceAt(Axial::new(1, 3), "meadow", 0, 1),
        Edit::PlaceAt(Axial::new(1, 4), "meadow", 0, 1),
        Edit::Place(PORTAL_B, "rail_tunnel_portal", 4),
        Edit::Place(Axial::new(1, 6), "rail_straight", 1),
        Edit::Place(Axial::new(1, 7), "rail_straight", 1),
    ]
}

fn hidden_rail_links(scene: &Scene) -> Vec<LinkId> {
    scene
        .sim
        .network(NetworkKind::Rail)
        .links()
        .filter(|(_, l)| l.hidden())
        .map(|(id, _)| id)
        .collect()
}

#[test]
fn two_facing_rail_portals_get_one_hidden_rail_link_and_no_road_link() {
    let scene = Scene::with(1, SimConfig::default(), &rail_tunnel_layout());
    assert_eq!(hidden_rail_links(&scene).len(), 1);
    assert!(scene.roads().links().all(|(_, l)| !l.hidden()));
}

#[test]
fn a_train_runs_the_tunnel_with_its_lamps_ramping_at_the_arches() {
    let mut scene = Scene::with(1, SimConfig::default(), &rail_tunnel_layout());
    let hidden = hidden_rail_links(&scene)[0];
    let (mut through, mut seen_open, mut seen_gone, mut mid_fade) = (false, false, false, false);
    for _ in 0..3000 * hex_sim::TICK_HZ {
        scene.sim.tick();
        for t in scene.sim.rails().trains() {
            through |= t.steps().iter().any(|s| s.link == hidden);
            for v in t.vehicles(1.0) {
                seen_open |= v.lamps <= 0.0;
                seen_gone |= v.lamps >= 1.0;
                mid_fade |= v.lamps > 0.0 && v.lamps < 1.0;
            }
        }
        if through && seen_open && seen_gone && mid_fade {
            break;
        }
    }
    assert!(through, "no train routed over the tunnel link");
    assert!(seen_open, "lamps are off in the open");
    assert!(seen_gone, "lamps are fully lit inside the tunnel");
    assert!(mid_fade, "lamps ramp up at an arch");
}

/// Bulldozing the far portal (after the track beyond it, which the world keeps connected) takes
/// the link away; trains on it go, the rest carry on.
#[test]
fn bulldozing_the_far_portal_removes_the_rail_link_without_a_panic() {
    let mut scene = Scene::with(1, SimConfig::default(), &rail_tunnel_layout());
    for _ in 0..40 * hex_sim::TICK_HZ {
        scene.sim.tick();
    }
    assert!(
        !scene.sim.rails().trains().is_empty(),
        "the line has a train"
    );
    assert_eq!(hidden_rail_links(&scene).len(), 1);
    for cell in [Axial::new(1, 7), Axial::new(1, 6), PORTAL_B] {
        scene.apply(&Edit::Bulldoze(cell));
    }
    assert!(hidden_rail_links(&scene).is_empty());
    for _ in 0..60 * hex_sim::TICK_HZ {
        scene.sim.tick();
    }
    for t in scene.sim.rails().trains() {
        for s in t.steps() {
            assert!(
                scene.sim.network(NetworkKind::Rail).link(s.link).is_some(),
                "{s:?} names a link that is gone"
            );
        }
    }
}
