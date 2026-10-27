//! Body clearance through junctions (PRD §7.2): at every rotation of the Y junction, a car body
//! ([`CAR_LENGTH`] by [`CAR_WIDTH`]) swept along each rounded lane with its heading keeps clear of
//! the bodies swept along every lane the follow and merge rules never pair it with (two lanes that
//! never come within [`LANE_CLEAR`] at any angle but head-on), so two cars on such lanes can pass
//! each other in the junction at any timing without touching. The bakery's drive, the other
//! three-way road node, is checked on the cars' own trip paths, counting only the trips whose cars
//! can be there at the same time under the follow, yard and forecourt rules.

mod common;

use common::*;
use hex_core::Axial;
use hex_sim::geom::{self, LanePath, Pose, Vec2};
use hex_sim::rail::CAR_LENGTH;
use hex_sim::traffic::{CAR_WIDTH, LANE_CLEAR};
use hex_sim::{CarId, Network, NodeId, SimConfig, Simulation};

/// `traffic.rs`'s head-on cosine: closer than this to opposite, two paths never conflict.
const HEAD_ON: f32 = -0.97;

/// Spacing of the poses swept along each lane (m).
const STEP: f32 = 0.1;

/// Only poses within this distance of the junction tile's centre are swept (m): the junction and
/// the start of each arm.
const REACH: f32 = 16.0;

/// The least gap between two car bodies on lanes the rules never pair (m): what the right turns'
/// swing buys within the deck ([`junction_lanes_keep_car_bodies_on_the_deck`]).
const MIN_GAP: f32 = 0.25;

/// A junction tile with a straight road on the neighbour across each arm, and the rounded lanes
/// between the far seams of every two arms.
struct Junction {
    centre: Vec2,
    lanes: Vec<(String, LanePath)>,
}

fn junction(tile: &str, rot: u8) -> Junction {
    let cat = catalog();
    let mut net = Network::new(hex_core::NetworkKind::Road);
    put(&mut net, &cat, Axial::ORIGIN, tile, rot);
    let arms: Vec<u8> = (0..6u8)
        .filter(|&e| net.seam_node(Axial::ORIGIN, e).is_some())
        .collect();
    for &e in &arms {
        put(
            &mut net,
            &cat,
            Axial::ORIGIN.neighbor(e),
            "road_straight",
            e % 3,
        );
    }
    let ends: Vec<(String, NodeId)> = arms
        .iter()
        .map(|&e| {
            let far = net.seam_node(Axial::ORIGIN.neighbor(e), e).unwrap();
            (format!("e{e}"), far)
        })
        .collect();
    let offset = SimConfig::default().lane_offset;
    let mut lanes = vec![];
    for (na, a) in &ends {
        for (nb, b) in &ends {
            if a == b {
                continue;
            }
            let route = net.route(*a, *b).unwrap();
            let lane = LanePath::rounded(&route.centerline(&net), offset);
            lanes.push((format!("{na}->{nb}"), lane));
        }
    }
    Junction {
        centre: geom::cell_center(Axial::ORIGIN),
        lanes,
    }
}

/// Whether the follow and merge rules can pair cars on paths `a` and `b` near the node, given
/// their poses there (`sa`, `sb`): somewhere one comes within [`LANE_CLEAR`] of the other at any
/// angle but head-on (`traffic::meets`).
fn paired(sa: &[Pose], a: &LanePath, sb: &[Pose], b: &LanePath) -> bool {
    let meets = |poses: &[Pose], other: &LanePath| {
        poses.iter().any(|p| {
            let (d, s) = hex_sim::rail::closest_distance(other.points(), p.pos);
            d <= LANE_CLEAR && geom::dot(p.dir, other.pose_at_distance(s).dir) > HEAD_ON
        })
    };
    meets(sa, b) || meets(sb, a)
}

/// The four corners of a car body centred on `p`, facing along its direction.
fn corners(p: Pose) -> [Vec2; 4] {
    let (f, r) = (
        geom::scale(p.dir, 0.5 * CAR_LENGTH),
        geom::scale(geom::right_of(p.dir), 0.5 * CAR_WIDTH),
    );
    [
        geom::add(geom::add(p.pos, f), r),
        geom::add(geom::sub(p.pos, f), r),
        geom::sub(geom::sub(p.pos, f), r),
        geom::sub(geom::add(p.pos, f), r),
    ]
}

/// The gap between two car bodies, negative when they overlap (the separating axis test).
fn body_gap(a: Pose, b: Pose) -> f32 {
    let (ca, cb) = (corners(a), corners(b));
    let axes = [a.dir, geom::right_of(a.dir), b.dir, geom::right_of(b.dir)];
    // The least overlap of the two boxes' spans over the four axes: positive when they overlap.
    let depth = axes
        .iter()
        .map(|&ax| {
            let span = |c: &[Vec2; 4]| {
                c.iter()
                    .map(|&p| geom::dot(p, ax))
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), x| {
                        (lo.min(x), hi.max(x))
                    })
            };
            let ((la, ha), (lb, hb)) = (span(&ca), span(&cb));
            ha.min(hb) - la.max(lb)
        })
        .fold(f32::INFINITY, f32::min);
    if depth >= 0.0 {
        return -depth;
    }
    // Disjoint convex boxes: the nearest points lie on a corner of one and an edge of the other.
    let edge_gap = |c: &[Vec2; 4], o: &[Vec2; 4]| {
        c.iter()
            .map(|&p| {
                (0..4)
                    .map(|k| geom::distance_to_polyline(p, &[o[k], o[(k + 1) % 4]]))
                    .fold(f32::INFINITY, f32::min)
            })
            .fold(f32::INFINITY, f32::min)
    };
    edge_gap(&ca, &cb).min(edge_gap(&cb, &ca))
}

/// The poses every [`STEP`] along `lane` within [`REACH`] of `centre`.
fn sweep(lane: &LanePath, centre: Vec2) -> Vec<Pose> {
    let mut out = vec![];
    let mut s = 0.0;
    while s <= lane.length() {
        let p = lane.pose_at_distance(s);
        if geom::dist(p.pos, centre) <= REACH {
            out.push(p);
        }
        s += STEP;
    }
    out
}

/// The least gap between bodies swept along two lanes, and where: `(gap, pose on a, pose on b)`.
fn least_gap(a: &[Pose], b: &[Pose]) -> (f32, Pose, Pose) {
    let mut best = (f32::INFINITY, a[0], b[0]);
    let reach = CAR_LENGTH + CAR_WIDTH + MIN_GAP;
    for &pa in a {
        for &pb in b {
            if geom::dist(pa.pos, pb.pos) > reach {
                continue;
            }
            let g = body_gap(pa, pb);
            if g < best.0 {
                best = (g, pa, pb);
            }
        }
    }
    best
}

/// Where the `road_junction_center` kerbs start (`scripts/blender/kit/_road_bend.py`): each arm
/// runs straight in to [`MOUTH`] from the centre, 3 m either side of its centreline, and between two
/// arms the kerb sweeps round a fillet of radius [`RHO_F`] tangent to both arms' kerb lines there.
const MOUTH: f32 = 4.0;
const HALF_ROAD: f32 = 3.0;
const RHO_F: f32 = (MOUTH - HALF_ROAD / 1.732_050_8) * 1.732_050_8;

/// How far `p` (relative to the junction centre) lies inside the paved deck of a Y junction whose
/// arms run along the unit directions `arms`, negative outside it.
fn deck_margin(p: Vec2, arms: &[Vec2]) -> f32 {
    let mut best = f32::NEG_INFINITY;
    for (k, &u) in arms.iter().enumerate() {
        let along = geom::dot(p, u).max(0.0);
        best = best.max(HALF_ROAD - geom::dist(p, geom::scale(u, along)));
        for &v in &arms[k + 1..] {
            let b = geom::add(u, v);
            let b = geom::scale(b, 1.0 / geom::len(b));
            let half = 0.5 * geom::dot(u, v).clamp(-1.0, 1.0).acos();
            let inside = geom::dot(p, b) >= geom::len(p) * half.cos()
                && geom::dot(p, u) <= MOUTH
                && geom::dot(p, v) <= MOUTH;
            if inside {
                let c = geom::scale(b, MOUTH / half.cos());
                best = best.max(geom::dist(p, c) - RHO_F);
            }
        }
    }
    best
}

/// Every car body swept along every lane through `road_junction` at every rotation stays on the
/// paved deck, modelled from the generator's kerb lines and fillets ([`deck_margin`]): the wide
/// right turns swing out over the kerb fillets, never past them.
#[test]
fn junction_lanes_keep_car_bodies_on_the_deck() {
    let mut worst = (f32::INFINITY, String::new());
    for rot in 0..6u8 {
        let j = junction("road_junction", rot);
        let arms: Vec<Vec2> = [0u8, 2, 4]
            .iter()
            .map(|&e| {
                let p = geom::sub(
                    geom::lane_point(Axial::ORIGIN, (e + rot) % 6, 0.0),
                    j.centre,
                );
                geom::scale(p, 1.0 / geom::len(p))
            })
            .collect();
        for (name, lane) in &j.lanes {
            for pose in sweep(lane, j.centre) {
                for c in corners(pose) {
                    let m = deck_margin(geom::sub(c, j.centre), &arms);
                    if m < worst.0 {
                        worst = (m, format!("rot {rot} {name} at ({:.2}, {:.2})", c[0], c[1]));
                    }
                }
            }
        }
    }
    println!("least deck margin {:.2} m, {}", worst.0, worst.1);
    assert!(worst.0 >= 0.0, "a body corner leaves the deck: {worst:?}");
}

/// At every rotation of `road_junction`, each pair of lanes the rules never pair keeps [`MIN_GAP`]
/// between the bodies swept along them, whatever the two cars' timing.
#[test]
fn junction_lanes_keep_car_bodies_clear_of_unpaired_lanes() {
    let mut failures = vec![];
    let (mut worst, mut unpaired) = (f32::INFINITY, 0);
    for rot in 0..6u8 {
        let j = junction("road_junction", rot);
        let swept: Vec<Vec<Pose>> = j.lanes.iter().map(|(_, l)| sweep(l, j.centre)).collect();
        for a in 0..j.lanes.len() {
            for b in a + 1..j.lanes.len() {
                if paired(&swept[a], &j.lanes[a].1, &swept[b], &j.lanes[b].1) {
                    continue;
                }
                unpaired += 1;
                let (gap, pa, pb) = least_gap(&swept[a], &swept[b]);
                worst = worst.min(gap);
                if gap < MIN_GAP {
                    let at = |p: Pose| geom::sub(p.pos, j.centre);
                    failures.push(format!(
                        "rot {rot} {} vs {}: {gap:.2} m at {:?} and {:?}",
                        j.lanes[a].0,
                        j.lanes[b].0,
                        at(pa),
                        at(pb)
                    ));
                }
            }
        }
    }
    println!("{unpaired} unpaired lane pairs, least gap {worst:.2} m");
    assert!(unpaired >= 6 * 6, "the oncoming turns are among them");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// One trip through the bakery's three-way node.
struct Trip {
    name: String,
    /// Leaves from a bakery spot.
    departs: bool,
    /// Parks at the bakery or leaves it.
    yard: bool,
    path: LanePath,
    /// The pull-out and the turn-in the forecourt rule governs (`Car::drives`).
    drives: (f32, f32),
}

/// The bakery at `rot` with a straight road and a house cluster beyond each of its two road arms,
/// and every trip through its three-way node: through both ways, each house's cars to each spot,
/// and each spot back to each house.
fn bakery_trips(rot: u8) -> Vec<Trip> {
    let cat = catalog();
    let mut sim = Simulation::new(
        3,
        SimConfig {
            spawn_per_destination_hz: 0.0,
            shop_stay_seconds: 1e9,
            ..Default::default()
        },
    );
    sim.apply_place(Axial::ORIGIN, cat.get("bakery").unwrap(), rot);
    let mut houses = vec![];
    for e in [rot, (rot + 3) % 6] {
        let road = Axial::ORIGIN.neighbor(e);
        sim.apply_place(road, cat.get("road_straight").unwrap(), e % 3);
        let house = road.neighbor(e);
        sim.apply_place(house, cat.get("house_cluster").unwrap(), (e + 3) % 6);
        houses.push(house);
    }
    let trip = |s: &Simulation, id: CarId| {
        let car = s.cars().iter().find(|c| c.id() == id).unwrap();
        let (from, to) = (car.from_spot(), car.to_spot());
        Trip {
            name: format!(
                "{:?}#{} -> {:?}#{}",
                from.cell, from.index, to.cell, to.index
            ),
            departs: from.cell == Axial::ORIGIN,
            yard: from.cell == Axial::ORIGIN || to.cell == Axial::ORIGIN,
            path: car.path().clone(),
            drives: car.drives(),
        }
    };
    let mut trips = vec![];
    let ids: Vec<CarId> = sim.parked().iter().map(|c| c.id()).collect();
    for &id in &ids {
        for &to in houses.iter().chain([&Axial::ORIGIN]) {
            for second in [false, true] {
                // A second car to the bakery takes its other spot.
                let mut s = sim.clone();
                if second {
                    let Some(&other) = ids.iter().find(|&&o| o != id) else {
                        continue;
                    };
                    if to != Axial::ORIGIN || s.send(other, to).is_none() {
                        continue;
                    }
                }
                if s.send(id, to).is_none_or(|t| t.cell != to) {
                    continue;
                }
                trips.push(trip(&s, id));
                if to != Axial::ORIGIN {
                    continue;
                }
                for _ in 0..3000 {
                    if s.parked().iter().any(|c| c.id() == id) {
                        break;
                    }
                    s.tick();
                }
                for &home in &houses {
                    let mut back = s.clone();
                    if back.send(id, home).is_some_and(|t| t.cell == home) {
                        trips.push(trip(&back, id));
                    }
                }
            }
        }
    }
    trips
}

/// The poses of `t` within [`REACH`] of `centre`, without its forecourt manoeuvres when `other`
/// drives the other way past them: the forecourt rule never lets the two be there at once.
fn bakery_sweep(t: &Trip, other: &Trip, centre: Vec2) -> Vec<Pose> {
    let (out_end, in_start) = t.drives;
    let lane_dir = |s: f32| t.path.pose_at_distance(s).dir;
    let near = |p: &LanePath| {
        sweep(p, centre)
            .into_iter()
            .min_by(|a, b| geom::dist(a.pos, centre).total_cmp(&geom::dist(b.pos, centre)))
    };
    let oncoming = |dir: Vec2| near(&other.path).is_some_and(|o| geom::dot(o.dir, dir) < -0.5);
    let skip_out = out_end > 0.0 && oncoming(lane_dir(out_end));
    let skip_in = in_start.is_finite() && oncoming(lane_dir(in_start));
    let mut out = vec![];
    let mut s = 0.0;
    while s <= t.path.length() {
        let p = t.path.pose_at_distance(s);
        let held = (skip_out && s < out_end) || (skip_in && s >= in_start);
        if !held && geom::dist(p.pos, centre) <= REACH {
            out.push(p);
        }
        s += STEP;
    }
    out
}

/// The bakery's drive, the other three-way road node: at every rotation each pair of trips whose
/// cars can be there at the same time keeps [`MIN_GAP`] between the bodies swept along them, on the
/// cars' own paths (lanes and parking legs), whatever their timing. Pairs that cannot be there at
/// once: two trips the follow and merge rules pair near the node ([`paired`]); a departure from the
/// bakery and any other trip parking there or leaving it (the yard rule, `sim::yard_holds`); and a
/// forecourt manoeuvre (`Car::drives`) and a car driving the other way past it (the forecourt
/// rule, `sim::drive_holds`), so those poses are left out of the pair ([`bakery_sweep`]).
#[test]
fn bakery_trips_keep_car_bodies_clear_when_they_can_meet() {
    let mut failures = vec![];
    let (mut worst, mut checked) = (f32::INFINITY, 0);
    for rot in 0..6u8 {
        let trips = bakery_trips(rot);
        assert!(trips.len() >= 12, "rot {rot}: {} trips", trips.len());
        let centre = geom::cell_center(Axial::ORIGIN);
        let swept: Vec<Vec<Pose>> = trips.iter().map(|t| sweep(&t.path, centre)).collect();
        for a in 0..trips.len() {
            for b in a + 1..trips.len() {
                let (ta, tb) = (&trips[a], &trips[b]);
                if (ta.yard && tb.yard && (ta.departs || tb.departs))
                    || ta.name == tb.name
                    || paired(&swept[a], &ta.path, &swept[b], &tb.path)
                {
                    continue;
                }
                let (sa, sb) = (bakery_sweep(ta, tb, centre), bakery_sweep(tb, ta, centre));
                if sa.is_empty() || sb.is_empty() {
                    continue;
                }
                checked += 1;
                let (gap, pa, pb) = least_gap(&sa, &sb);
                worst = worst.min(gap);
                if gap < MIN_GAP {
                    let at = |p: Pose| geom::sub(p.pos, centre);
                    failures.push(format!(
                        "rot {rot} {} vs {}: {gap:.2} m at {:?} and {:?}",
                        ta.name,
                        tb.name,
                        at(pa),
                        at(pb)
                    ));
                }
            }
        }
    }
    println!("bakery: {checked} trip pairs that can meet, least gap {worst:.2} m");
    assert!(checked > 0, "the through trips both ways are among them");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
