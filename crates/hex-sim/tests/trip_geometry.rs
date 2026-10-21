//! Trip geometry against a recorded baseline (PRD §7.1): every trip between the parking spots of
//! the house and bakery layouts below turns no more per tick than it did when the baseline in
//! `fixtures/trip_turns.txt` was written, so a change to lane rounding or parking data cannot make
//! a trip sharper unnoticed. Turns compare within [`TOLERANCE`], never as exact text, so libm
//! differences between platforms do not fail it. `HEXW_BLESS_TRIPS=1` rewrites the baseline from
//! the current code; bless only an intended change.

mod common;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use common::*;
use hex_core::Axial;
use hex_sim::{Car, CarId, SimConfig, Simulation, TICK_SECONDS, geom};

/// How far a trip's turn may differ from the baseline's and still count as the same, in degrees
/// per tick.
const TOLERANCE: f32 = 0.1;

/// The 20° per tick rule (PRD §7.1).
const TURN_RULE: f32 = 20.0;

/// No departures or visitor returns unless the enumeration sends one.
fn quiet() -> SimConfig {
    SimConfig {
        spawn_per_destination_hz: 0.0,
        shop_stay_seconds: 1e9,
        ..Default::default()
    }
}

/// A ring with junctions at `d` and `d + 2`: a house outside the second, the bakery outside the
/// first with its edge 3 (`flip`: edge 0) facing the junction.
fn bakery_ring(d: u8, flip: bool) -> Vec<Edit> {
    let mut edits = vec![];
    for k in 0..6u8 {
        if k == d || k == (d + 2) % 6 {
            edits.push(Edit::Place(ring_cell(k), "road_junction", k));
        } else {
            edits.push(Edit::Place(ring_cell(k), "road_bend_120", (k + 2) % 6));
        }
    }
    edits.push(Edit::Place(
        ring_house((d + 2) % 6),
        "house_cluster",
        (d + 5) % 6,
    ));
    let rot = if flip { d } else { (d + 3) % 6 };
    edits.push(Edit::Place(ring_house(d), "bakery", rot));
    edits
}

fn scenes() -> Vec<(String, Vec<Edit>)> {
    let mut scenes: Vec<(String, Vec<Edit>)> = vec![
        ("house_chain".into(), house_chain()),
        ("ring024".into(), ring(&[0, 2, 4])),
        ("ring_and_pair".into(), ring_and_pair()),
    ];
    for rot in [0u8, 3] {
        scenes.push((
            format!("bakery_chain{rot}"),
            vec![
                Edit::Place(Axial::new(1, 0), "house_cluster", 0),
                Edit::Place(Axial::new(2, 0), "road_straight", 0),
                Edit::Place(Axial::new(3, 0), "bakery", rot),
            ],
        ));
    }
    for d in 0..6u8 {
        for flip in [false, true] {
            scenes.push((format!("bakery_ring{d}{flip}"), bakery_ring(d, flip)));
        }
    }
    scenes
}

/// The most the facing turns in one tick on the parking legs and on the lanes.
fn turns(car: &Car) -> (f32, f32) {
    let step = SimConfig::default().car_speed * TICK_SECONDS;
    let (l0, l1) = car.lane_span();
    let (mut legs, mut lanes) = (0.0f32, 0.0f32);
    let mut d = 0.0;
    while d + step < car.path().length() {
        let (a, b) = (
            car.pose_at_distance(d).dir,
            car.pose_at_distance(d + step).dir,
        );
        let t = geom::dot(a, b).clamp(-1.0, 1.0).acos().to_degrees();
        if d + step < l0 || d > l1 {
            legs = legs.max(t);
        } else {
            lanes = lanes.max(t);
        }
        d += 0.02;
    }
    (legs, lanes)
}

type Trips = BTreeMap<String, (f32, f32)>;

fn spot(s: hex_sim::SpotRef) -> String {
    format!("{},{}#{}", s.cell.q, s.cell.r, s.index)
}

/// Records the trip `id` just started, if it did.
fn record(scene: &str, sim: &Simulation, id: CarId, trips: &mut Trips) -> bool {
    let Some(car) = sim.cars().iter().find(|c| c.id() == id) else {
        return false;
    };
    let key = format!("{scene} {} {}", spot(car.from_spot()), spot(car.to_spot()));
    trips.insert(key, turns(car));
    true
}

fn park(sim: &mut Simulation, id: CarId) {
    for _ in 0..3000 {
        if sim.parked().iter().any(|c| c.id() == id) {
            return;
        }
        sim.tick();
    }
    panic!("{id:?} never parked");
}

/// Every trip from each car's home spot to each destination, on from there to each destination,
/// and the same for a second car taking the next free spot there.
fn trips() -> Trips {
    let mut trips = Trips::new();
    for (name, edits) in scenes() {
        let scene = Scene::with(11, quiet(), &edits);
        let cells: Vec<Axial> = edits
            .iter()
            .filter_map(|e| match e {
                Edit::Place(c, id, _) if *id == "house_cluster" || *id == "bakery" => Some(*c),
                _ => None,
            })
            .collect();
        let ids: Vec<CarId> = scene.sim.parked().iter().map(|c| c.id()).collect();
        let sends = |sim: &Simulation, id: CarId, trips: &mut Trips| {
            for &y in &cells {
                let mut s = sim.clone();
                if s.send(id, y).is_some_and(|t| t.cell == y) {
                    record(&name, &s, id, trips);
                }
            }
        };
        for &id in &ids {
            for &x in &cells {
                let mut s = scene.sim.clone();
                if s.send(id, x).is_none_or(|t| t.cell != x) || !record(&name, &s, id, &mut trips) {
                    continue;
                }
                park(&mut s, id);
                sends(&s, id, &mut trips);
                for &id2 in &ids {
                    let mut s2 = s.clone();
                    if id2 == id
                        || s2.send(id2, x).is_none_or(|t| t.cell != x)
                        || !record(&name, &s2, id2, &mut trips)
                    {
                        continue;
                    }
                    park(&mut s2, id2);
                    sends(&s2, id2, &mut trips);
                }
            }
        }
    }
    trips
}

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/trip_turns.txt");

#[test]
fn trips_turn_no_sharper_than_the_baseline() {
    let now = trips();
    if std::env::var_os("HEXW_BLESS_TRIPS").is_some() {
        let text = std::fs::read_to_string(FIXTURE).unwrap_or_default();
        let mut out: String = text
            .lines()
            .filter(|l| l.starts_with('#'))
            .map(|l| format!("{l}\n"))
            .collect();
        for (k, (legs, lanes)) in &now {
            writeln!(out, "{k} {legs:.2} {lanes:.2}").unwrap();
        }
        std::fs::write(FIXTURE, out).unwrap();
        return;
    }
    let base: Trips = std::fs::read_to_string(FIXTURE)
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            (
                format!("{} {} {}", f[0], f[1], f[2]),
                (f[3].parse().unwrap(), f[4].parse().unwrap()),
            )
        })
        .collect();
    assert_eq!(
        now.keys().collect::<Vec<_>>(),
        base.keys().collect::<Vec<_>>(),
        "the enumerated trips match the baseline's"
    );
    let mut failures = vec![];
    let (mut same, mut smoother) = (0, 0);
    for (k, &(legs, lanes)) in &now {
        let (bl, bn) = base[k];
        if legs > bl + TOLERANCE || lanes > bn + TOLERANCE {
            failures.push(format!(
                "{k}: legs {bl:.2} -> {legs:.2}, lanes {bn:.2} -> {lanes:.2}"
            ));
        } else if legs.max(lanes) < bl.max(bn) - TOLERANCE {
            smoother += 1;
        } else {
            same += 1;
        }
    }
    println!("{} trips: {same} as before, {smoother} smoother", now.len());
    assert!(
        failures.is_empty(),
        "sharper than the baseline:\n{}",
        failures.join("\n")
    );
}

/// Every enumerated trip, at the bakery at all six rotations (`bakery_chain`, `bakery_ring`) and
/// between houses, keeps its facing within 20° per tick on the sliding maximum (a 0.4 m tick
/// window slid 2 cm at a time, so a peak between per-tick samples counts), on the parking legs
/// (the reverse, its cusp, the swing onto the road, the way in) and on the lanes.
#[test]
fn every_trip_turns_at_most_20_deg_per_tick_on_the_sliding_maximum() {
    let now = trips();
    let rotations: std::collections::BTreeSet<&str> = now
        .keys()
        .filter_map(|k| k.split_whitespace().next())
        .filter(|s| s.starts_with("bakery"))
        .collect();
    assert_eq!(rotations.len(), 14, "bakery layouts: {rotations:?}");
    let over: Vec<String> = now
        .iter()
        .filter(|(_, (legs, lanes))| legs.max(*lanes) > TURN_RULE)
        .map(|(k, (legs, lanes))| format!("{k}: legs {legs:.2}, lanes {lanes:.2}"))
        .collect();
    let worst = now.values().map(|(l, n)| l.max(*n)).fold(0.0f32, f32::max);
    println!("{} trips, worst {worst:.2} deg per tick", now.len());
    assert!(
        over.is_empty(),
        "over 20 deg per tick:\n{}",
        over.join("\n")
    );
}
