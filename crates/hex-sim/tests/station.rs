//! Rail stations (P3): a train stops at the station tile's platform with every axle on its level
//! section, waits, and runs on, in both directions of a shuttle.

mod common;

use common::*;
use hex_core::{Axial, TileCatalog};
use hex_sim::TICK_HZ;
use hex_sim::rail::{Blocker, CONSIST, Railway, STATION_DWELL_SECONDS, axle_span};

fn place(r: &mut Railway, c: &TileCatalog, cell: Axial, id: &str, rot: u8) {
    r.insert_tile(cell, c.get(id).unwrap_or_else(|| panic!("{id}")), rot);
}

/// One station visit: whether the engine led, the ticks it stood, and its axles' x in the
/// station's frame while it stood.
#[derive(Debug)]
struct Visit {
    engine_leads: bool,
    ticks: u32,
    axles: Vec<f32>,
}

/// Station criterion 4: on a line of two straights, the station and two straights (rot 0, the
/// track along x through the station at the origin), the shuttle stops at the platform each way
/// with every outer axle of every vehicle on the level section (|x| <= 10 m, `rail_station.ron`),
/// stands there [`STATION_DWELL_SECONDS`], then runs on and is named in the departures that tick.
#[test]
fn a_train_stops_with_every_axle_on_the_level_section_then_runs_on() {
    let c = catalog();
    let mut r = Railway::new();
    place(&mut r, &c, Axial::ORIGIN, "rail_station", 0);
    for q in [-2, -1, 1, 2] {
        place(&mut r, &c, Axial::new(q, 0), "rail_straight", 0);
    }
    assert_eq!(r.stops().len(), 1);
    let (lo, hi) = r.stops()[0].level();
    assert!((hi - lo - 20.0).abs() < 0.01, "a 20 m level section");
    let (a, b) = axle_span();
    assert!(
        b - a < 20.0,
        "the consist's axles fit the level section: {}",
        b - a
    );

    let mut visits: Vec<Visit> = vec![];
    let mut standing = false;
    let mut departed: u32 = 0;
    for _ in 0..300 * TICK_HZ {
        r.tick();
        let [t] = r.trains() else {
            panic!("one train on the line: {}", r.trains().len())
        };
        let at_station = t.blocker() == Blocker::Station(Axial::ORIGIN) && t.speed() == 0.0;
        if at_station {
            if !standing {
                visits.push(Visit {
                    engine_leads: t.engine_leads(),
                    ticks: 0,
                    axles: vec![],
                });
            }
            let v = visits.last_mut().unwrap();
            v.ticks += 1;
            v.axles.clear();
            // Each vehicle in consist order with its own wheelbase: centre ± half along its facing.
            for (pose, spec) in t.vehicles(1.0).iter().zip(CONSIST.iter()) {
                assert_eq!(pose.kind, spec.kind);
                for s in [-1.0, 1.0] {
                    let x = pose.pose.pos[0] + s * spec.half_wheelbase * pose.pose.dir[0];
                    let z = pose.pose.pos[1] + s * spec.half_wheelbase * pose.pose.dir[1];
                    assert!(z.abs() < 0.01, "on the track centreline: {z}");
                    v.axles.push(x);
                }
            }
        }
        if r.departures()
            .iter()
            .any(|&(id, cell)| id == t.id() && cell == Axial::ORIGIN)
        {
            assert!(standing, "a departure follows a stand");
            departed += 1;
        }
        standing = at_station;
    }
    assert!(visits.len() >= 3, "{visits:?}");
    assert!(
        visits.iter().any(|v| v.engine_leads) && visits.iter().any(|v| !v.engine_leads),
        "stops both ways: {visits:?}"
    );
    let dwell = (STATION_DWELL_SECONDS * TICK_HZ as f32) as u32;
    let last = visits.len() - 1;
    for (k, v) in visits.iter().enumerate() {
        for &x in &v.axles {
            assert!(
                x.abs() <= 10.0,
                "visit {k}: an axle at x {x} off the level section"
            );
        }
        // Centred: the first and last axle stand equally far inside the ends.
        let min = v.axles.iter().copied().fold(f32::MAX, f32::min);
        let max = v.axles.iter().copied().fold(f32::MIN, f32::max);
        assert!((min + max).abs() < 0.02, "visit {k} centred: {min}..{max}");
        // The last visit may still be standing when the run ends.
        if k < last {
            assert!(
                v.ticks.abs_diff(dwell) <= 2,
                "visit {k} stood {} ticks, want {dwell}",
                v.ticks
            );
        }
    }
    assert!(
        departed as usize >= last,
        "{departed} departures for {} visits",
        visits.len()
    );
    assert_eq!(r.trains()[0].stops(), departed);
}

/// A stop re-arms once the train has left the station's link: the shuttle stops on every pass,
/// not only the first, and never twice in one pass (between two line-end reversals).
#[test]
fn the_shuttle_stops_once_per_pass() {
    let c = catalog();
    let mut r = Railway::new();
    place(&mut r, &c, Axial::ORIGIN, "rail_station", 0);
    for q in [-2, -1, 1, 2] {
        place(&mut r, &c, Axial::new(q, 0), "rail_straight", 0);
    }
    let mut per_pass: Vec<u32> = vec![0];
    let mut reversals = 0;
    for _ in 0..400 * TICK_HZ {
        r.tick();
        let t = &r.trains()[0];
        if t.reversals() != reversals {
            reversals = t.reversals();
            per_pass.push(0);
        }
        if !r.departures().is_empty() {
            *per_pass.last_mut().unwrap() += 1;
        }
    }
    // Every complete pass (between two reversals) stops exactly once.
    let full = &per_pass[1..per_pass.len() - 1];
    assert!(full.len() >= 4, "{per_pass:?}");
    assert!(full.iter().all(|&n| n == 1), "{per_pass:?}");
}
