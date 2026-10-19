//! Shop destinations (PRD §7.1): a shop owns no cars; house cars visit it, park in a free spot for
//! a short deterministic stay and drive home; bulldozing the shop returns its visitors home.
//!
//! The shop here is house_cluster with the bakery for its houses and a Shop node, so the tests do
//! not depend on the bakery tile's layout.

mod common;

use common::*;
use hex_core::{Axial, Destination, TileDef};
use hex_sim::{CarId, SimConfig, Simulation, TICK_SECONDS};

const HOUSE: Axial = Axial::new(1, 0);
const SHOP: Axial = Axial::new(3, 0);

fn shop_def() -> TileDef {
    let mut def = catalog().get("house_cluster").unwrap().clone();
    def.id = "test_shop".into();
    for h in &mut def.props.hero {
        h.piece = "bakery_shop".into();
    }
    def.nodes[0].destination = Some(Destination::Shop);
    def.agents.clear();
    def
}

/// A house cluster and a shop joined by a road_straight, as [`house_chain`] lays out two houses.
fn village(config: SimConfig) -> Simulation {
    let cat = catalog();
    let mut sim = Simulation::new(1, config);
    sim.apply_place(HOUSE, cat.get("house_cluster").unwrap(), 0);
    sim.apply_place(Axial::new(2, 0), cat.get("road_straight").unwrap(), 0);
    sim.apply_place(SHOP, &shop_def(), 3);
    sim
}

fn quiet() -> SimConfig {
    SimConfig {
        spawn_per_destination_hz: 0.0,
        ..Default::default()
    }
}

fn parked_at(sim: &Simulation, cell: Axial) -> Vec<CarId> {
    sim.parked()
        .iter()
        .filter(|c| c.spot().cell == cell)
        .map(|c| c.id())
        .collect()
}

#[test]
fn a_shop_tile_validates_and_owns_no_cars() {
    assert!(shop_def().validate(catalog().sockets()).is_empty());
    let sim = village(quiet());
    assert_eq!(sim.owned_by(SHOP), 0);
    assert!(parked_at(&sim, SHOP).is_empty());
    assert_eq!(sim.owned_by(HOUSE), 2);
}

#[test]
fn a_house_and_a_shop_start_traffic() {
    let mut sim = village(SimConfig::default());
    assert_eq!(sim.connected_destinations(), 2);
    assert!(sim.spawn_rate_hz() > 0.0);
}

#[test]
fn a_visitor_parks_stays_and_drives_home() {
    let config = quiet();
    let stay_ticks = (config.shop_stay_seconds / TICK_SECONDS).round() as u64;
    let mut sim = village(config);
    let id = parked_at(&sim, HOUSE)[0];
    let home_spot = sim.parked().iter().find(|c| c.id() == id).unwrap().spot();
    let to = sim.send(id, SHOP).expect("the shop has room");
    assert_eq!(to.cell, SHOP);

    let mut parked_on = None;
    for _ in 0..4000 {
        sim.tick();
        if let Some(c) = sim.parked().iter().find(|c| c.id() == id) {
            if c.spot().cell == SHOP && parked_on.is_none() {
                parked_on = Some(c.since());
            }
            if c.spot().cell == HOUSE {
                break;
            }
        }
    }
    let parked_on = parked_on.expect("the visitor parked at the shop");
    let back = sim
        .parked()
        .iter()
        .find(|c| c.id() == id)
        .expect("the visitor parked again");
    assert_eq!(back.spot(), home_spot, "home in its own spot");
    assert_eq!(sim.owned_by(HOUSE), 2);
    // It left the shop exactly when its stay ran out, then drove the trip home.
    assert!(back.since() > parked_on + stay_ticks);
}

#[test]
fn visitors_leave_on_time_and_shops_never_start_departures() {
    let config = SimConfig {
        spawn_per_destination_hz: 0.5,
        ..Default::default()
    };
    let stay_ticks = (config.shop_stay_seconds / TICK_SECONDS).round() as u64;
    let mut sim = village(config);
    let mut visits = 0;
    let mut at_shop: std::collections::BTreeMap<CarId, u64> = Default::default();
    for _ in 0..6000 {
        sim.tick();
        for c in sim.parked().iter().filter(|c| c.spot().cell == SHOP) {
            assert!(
                sim.ticks() - c.since() <= stay_ticks,
                "car {:?} overstayed",
                c.id()
            );
        }
        visits += sim
            .cars()
            .iter()
            .filter(|c| c.to_spot().cell == SHOP)
            .count();
        // A car only ever leaves the shop for a house, and only once its stay is over.
        for c in sim.cars().iter().filter(|c| c.from_spot().cell == SHOP) {
            assert_eq!(c.to_spot().cell, HOUSE);
            if let Some(since) = at_shop.remove(&c.id()) {
                assert!(
                    sim.ticks() - since >= stay_ticks,
                    "car {:?} left early",
                    c.id()
                );
            }
        }
        at_shop = sim
            .parked()
            .iter()
            .filter(|c| c.spot().cell == SHOP)
            .map(|c| (c.id(), c.since()))
            .collect();
    }
    assert!(visits > 0, "no car visited the shop");
    assert_eq!(sim.owned_by(HOUSE), 2);
}

#[test]
fn the_same_seed_replays_the_same_visits() {
    let run = || {
        let mut sim = village(SimConfig {
            spawn_per_destination_hz: 0.5,
            ..Default::default()
        });
        let mut trace = vec![];
        for _ in 0..3000 {
            sim.tick();
            trace.push(
                sim.parked()
                    .iter()
                    .map(|c| (c.id(), c.spot()))
                    .collect::<Vec<_>>(),
            );
        }
        trace
    };
    assert_eq!(run(), run());
}

#[test]
fn bulldozing_the_shop_sends_its_visitors_home() {
    for driving in [false, true] {
        let mut sim = village(quiet());
        let id = parked_at(&sim, HOUSE)[0];
        sim.send(id, SHOP).unwrap();
        if !driving {
            while !parked_at(&sim, SHOP).contains(&id) {
                sim.tick();
            }
        }
        sim.apply_bulldoze(SHOP);
        assert_eq!(sim.owned_by(HOUSE), 2, "driving {driving}");
        assert!(sim.cars().is_empty());
        assert_eq!(parked_at(&sim, HOUSE).len(), 2);
        // Undo puts the shop back, empty.
        sim.apply_place(SHOP, &shop_def(), 3);
        assert!(parked_at(&sim, SHOP).is_empty());
        assert_eq!(sim.owned_by(HOUSE), 2);
    }
}
