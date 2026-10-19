//! House parking (PRD §7.1): owned cars wait in their spots, pull out by reversing out of the yard,
//! park in a free spot at the destination, full clusters refuse or redirect, the tile's car cap
//! holds, bulldoze and undo remove and restore a cluster's cars, and the parking paths keep a
//! pickup-sized car clear of the houses, doorsteps, hedges and the other spots.

mod common;

use std::collections::BTreeSet;

use common::*;
use hex_core::{Axial, HEX_RADIUS};
use hex_sim::geom::{self, Vec2};
use hex_sim::{Car, CarId, SimConfig, Simulation, SpotRef, TICK_SECONDS};

const A: Axial = Axial::new(1, 0);
const B: Axial = Axial::new(3, 0);

fn busy() -> SimConfig {
    SimConfig {
        spawn_per_destination_hz: 0.5,
        ..Default::default()
    }
}

/// No departures unless a test sends one.
fn quiet() -> SimConfig {
    SimConfig {
        spawn_per_destination_hz: 0.0,
        ..Default::default()
    }
}

fn parked_at(sim: &Simulation, cell: Axial) -> Vec<(CarId, usize)> {
    sim.parked()
        .iter()
        .filter(|c| c.spot().cell == cell)
        .map(|c| (c.id(), c.spot().index))
        .collect()
}

fn drive_until_parked(sim: &mut Simulation, id: CarId) -> Car {
    let mut last = None;
    for _ in 0..2000 {
        if let Some(c) = sim.cars().iter().find(|c| c.id() == id) {
            last = Some(c.clone());
        } else if sim.parked().iter().any(|c| c.id() == id) {
            return last.expect("the car drove before it parked");
        }
        sim.tick();
    }
    panic!("car {id:?} never parked");
}

fn angle_between(a: Vec2, b: Vec2) -> f32 {
    geom::dot(a, b).clamp(-1.0, 1.0).acos().to_degrees()
}

#[test]
fn each_house_owns_a_parked_car_up_to_the_tile_cap() {
    let scene = Scene::with(1, quiet(), &house_chain());
    let def = scene.catalog.get("house_cluster").unwrap();
    assert_eq!(def.car_cap(), 2);
    assert_eq!(def.parking.len(), 3);
    for cell in [A, B] {
        assert_eq!(scene.sim.owned_by(cell), 2);
        // Houses 0 and 1 own the cars, each parked in its own spot; house 2's spot is free.
        let mut homes: Vec<(u16, usize)> = scene
            .sim
            .parked()
            .iter()
            .filter(|c| c.home().cell == cell)
            .map(|c| (c.home().house, c.spot().index))
            .collect();
        homes.sort();
        assert_eq!(homes, vec![(0, 0), (1, 1)]);
        let spots = scene.sim.spots(cell);
        assert_eq!(spots.len(), 3);
        assert!(spots[2].1.is_none());
        for c in scene.sim.parked().iter().filter(|c| c.spot().cell == cell) {
            assert_eq!(Some(c.pose()), scene.sim.spot_pose(c.spot()));
        }
    }
    assert!(scene.sim.cars().is_empty());
}

#[test]
fn spot_poses_follow_the_tile_rotation() {
    let scene = Scene::with(1, quiet(), &house_chain());
    // A is unrotated: park-0 at tile-local (-1, -2.6) facing -x. B is turned three sixths, so the
    // same spot sits mirrored through B's centre and faces +x.
    let a = scene.sim.spot_pose(SpotRef { cell: A, index: 0 }).unwrap();
    let ca = A.to_world(HEX_RADIUS);
    assert!(geom::dist(a.pos, geom::add(ca, [-1.0, -2.6])) < 1e-4);
    assert!(geom::dist(a.dir, [-1.0, 0.0]) < 1e-4);
    let b = scene.sim.spot_pose(SpotRef { cell: B, index: 0 }).unwrap();
    let cb = B.to_world(HEX_RADIUS);
    assert!(geom::dist(b.pos, geom::add(cb, [1.0, 2.6])) < 1e-4);
    assert!(geom::dist(b.dir, [1.0, 0.0]) < 1e-4);
}

#[test]
fn a_car_pulls_out_in_reverse_then_drives_forward() {
    let mut scene = Scene::with(1, quiet(), &house_chain());
    let (id, index) = parked_at(&scene.sim, A)[0];
    let spot = SpotRef { cell: A, index };
    let start = scene.sim.spot_pose(spot).unwrap();
    assert!(scene.sim.send(id, B).is_some());
    let car = scene
        .sim
        .cars()
        .iter()
        .find(|c| c.id() == id)
        .unwrap()
        .clone();
    assert_eq!(car.from_spot(), spot);
    // No pop: the trip starts exactly where and how the car stood.
    let p0 = car.pose_at(0.0);
    assert!(geom::dist(p0.pos, start.pos) < 1e-4);
    assert!(angle_between(p0.dir, start.dir) < 1.0);
    // The pull-out reverses: the car travels against its facing until the cusp.
    let rev = car.reverse_length();
    assert!(rev > 5.0, "reverse leg {rev} m");
    for i in 1..20 {
        let d = rev * i as f32 / 20.0;
        let (a, b) = (
            car.pose_at_distance(d - 0.05),
            car.pose_at_distance(d + 0.05),
        );
        let travel = geom::sub(b.pos, a.pos);
        assert!(geom::dot(travel, a.dir) < 0.0, "not reversing at {d} m");
    }
    // After the cusp it drives forward, and the facing turns smoothly through the cusp.
    let after = car.pose_at_distance(rev + 1.0);
    let travel = geom::sub(car.pose_at_distance(rev + 1.1).pos, after.pos);
    assert!(geom::dot(travel, after.dir) > 0.0);
    let (before, at) = (
        car.pose_at_distance(rev - 0.05),
        car.pose_at_distance(rev + 0.05),
    );
    assert!(
        angle_between(before.dir, at.dir) < 10.0,
        "{before:?} {at:?} rev {rev}"
    );
    // The facing never jumps between samples a tick apart anywhere on the trip.
    let step = scene.sim.config().car_speed * TICK_SECONDS;
    let mut d = 0.0;
    while d + step < car.path().length() {
        let turn = angle_between(
            car.pose_at_distance(d).dir,
            car.pose_at_distance(d + step).dir,
        );
        assert!(turn < 20.0, "facing turns {turn} deg in one tick at {d} m");
        d += step;
    }
    // Then it joins the road: the lanes start right of the road node, heading out.
    let (lane_start, _) = car.lane_span();
    let lane = car.pose_at_distance(lane_start + 0.01);
    let node = scene.roads().node(car.origin()).unwrap().pos();
    assert!(geom::dist(lane.pos, node) < 1.6);
    assert!(geom::dot(lane.dir, geom::sub(node, A.to_world(HEX_RADIUS))) > 0.0);
}

#[test]
fn a_car_parks_in_a_free_spot_without_popping() {
    let mut scene = Scene::with(1, quiet(), &house_chain());
    let (id, _) = parked_at(&scene.sim, A)[0];
    let to = scene.sim.send(id, B).expect("B has a free spot");
    assert_eq!(to, SpotRef { cell: B, index: 2 });
    assert_eq!(scene.sim.spots(B)[2].1, Some(id), "reserved while driving");
    let last = drive_until_parked(&mut scene.sim, id);
    let parked = scene.sim.parked().iter().find(|c| c.id() == id).unwrap();
    assert_eq!(parked.spot(), to);
    let want = scene.sim.spot_pose(to).unwrap();
    assert_eq!(parked.pose(), want);
    // The last driven pose is the spot, nose the right way.
    let end = last.pose_at(1.0);
    assert!(geom::dist(end.pos, want.pos) < 1e-3);
    assert!(angle_between(end.dir, want.dir) < 2.0);
    assert_eq!(
        scene.sim.owned_by(A),
        2,
        "a visiting car still belongs to its house"
    );
}

#[test]
fn a_full_cluster_refuses_or_redirects() {
    // Refuse: A's second car cannot go to B once B's one free spot is taken, and there is nowhere
    // else to go.
    let mut scene = Scene::with(1, quiet(), &house_chain());
    let a_cars = parked_at(&scene.sim, A);
    assert!(scene.sim.send(a_cars[0].0, B).is_some());
    assert_eq!(scene.sim.send(a_cars[1].0, B), None);
    assert!(
        scene.sim.parked().iter().any(|c| c.id() == a_cars[1].0),
        "the refused car stays"
    );

    // Redirect: three clusters on the ring. The second car sent to a full cluster goes to the
    // other one with room.
    let mut scene = Scene::with(1, quiet(), &ring(&[0, 2, 4]));
    let (h0, h2, h4) = (ring_house(0), ring_house(2), ring_house(4));
    let cars = parked_at(&scene.sim, h0);
    assert_eq!(scene.sim.send(cars[0].0, h2).map(|s| s.cell), Some(h2));
    assert_eq!(scene.sim.send(cars[1].0, h2).map(|s| s.cell), Some(h4));
    assert!(scene.sim.spots(h2).iter().all(|(_, c)| c.is_some()));
}

#[test]
fn the_car_cap_holds_and_no_spot_is_double_booked() {
    let mut scene = Scene::with(3, busy(), &ring_and_pair());
    let clusters: Vec<Axial> = [
        ring_house(0),
        ring_house(2),
        ring_house(4),
        Axial::new(4, 0),
        Axial::new(5, 0),
    ]
    .into();
    for _ in 0..3000 {
        scene.sim.tick();
        assert_eq!(scene.sim.car_count(), 10);
        let mut holders = BTreeSet::new();
        for &cell in &clusters {
            assert_eq!(scene.sim.owned_by(cell), 2);
            for (_, car) in scene.sim.spots(cell) {
                if let Some(id) = car {
                    assert!(holders.insert(id), "{id:?} holds two spots");
                }
            }
        }
        // Every car holds exactly one spot: where it is parked, or where it is driving to.
        assert_eq!(holders.len(), 10);
        for c in scene.sim.parked() {
            assert_eq!(
                scene.sim.spots(c.spot().cell)[c.spot().index].1,
                Some(c.id())
            );
        }
        for c in scene.sim.cars() {
            assert_eq!(
                scene.sim.spots(c.to_spot().cell)[c.to_spot().index].1,
                Some(c.id())
            );
        }
    }
    assert!(scene.sim.departures() > 50);
}

fn run(seed: u64) -> Vec<Vec<(u64, [u32; 4])>> {
    let mut scene = Scene::with(seed, busy(), &house_chain_with_meadows());
    let mut frames = vec![];
    for t in 0..1500u64 {
        match t {
            300 => scene.apply(&Edit::Bulldoze(B)),
            600 => scene.apply(&Edit::Place(B, "house_cluster", 3)),
            _ => {}
        }
        scene.sim.tick();
        let pose = |id: CarId, p: hex_sim::Pose| {
            (
                id.0,
                [
                    p.pos[0].to_bits(),
                    p.pos[1].to_bits(),
                    p.dir[0].to_bits(),
                    p.dir[1].to_bits(),
                ],
            )
        };
        frames.push(
            scene
                .sim
                .cars()
                .iter()
                .map(|c| pose(c.id(), c.pose_at(0.5)))
                .chain(scene.sim.parked().iter().map(|c| pose(c.id(), c.pose())))
                .collect(),
        );
    }
    frames
}

#[test]
fn parking_is_deterministic() {
    let a = run(7);
    assert_eq!(a, run(7));
    assert_ne!(a, run(8));
}

#[test]
fn bulldozing_a_cluster_removes_its_cars_and_undo_restores_them() {
    let mut scene = Scene::with(5, busy(), &house_chain_with_meadows());
    let before: Vec<(u16, usize, hex_sim::Pose)> = scene
        .sim
        .parked()
        .iter()
        .filter(|c| c.home().cell == B)
        .map(|c| (c.home().house, c.spot().index, c.pose()))
        .collect();
    // Let cars visit, so B holds a visitor or has a car away when it goes.
    for _ in 0..200 {
        scene.sim.tick();
    }
    scene.apply(&Edit::Bulldoze(B));
    assert_eq!(scene.sim.owned_by(B), 0);
    assert!(scene.sim.spots(B).is_empty());
    assert!(
        scene
            .sim
            .parked()
            .iter()
            .all(|c| c.home().cell != B && c.spot().cell != B)
    );
    assert!(
        scene
            .sim
            .cars()
            .iter()
            .all(|c| c.home().cell != B && c.to_spot().cell != B)
    );
    // A keeps its two cars: one lost in B (parked there or headed there) comes back at home.
    assert_eq!(scene.sim.owned_by(A), 2);
    assert!(scene.sim.parked().iter().all(|c| c.spot().cell == A));
    for _ in 0..100 {
        scene.sim.tick();
    }

    // Undo: the same tile back, the same cars back in their houses' spots.
    scene.apply(&Edit::Place(B, "house_cluster", 3));
    let after: Vec<(u16, usize, hex_sim::Pose)> = scene
        .sim
        .parked()
        .iter()
        .filter(|c| c.home().cell == B)
        .map(|c| (c.home().house, c.spot().index, c.pose()))
        .collect();
    assert_eq!(after, before);
    assert_eq!(scene.sim.owned_by(B), 2);
}

// Clearance: obstacles of house_cluster in tile-local (x, z) meters, as min/max boxes. Houses from
// the house pieces' footprints at their hero placements (data/tiles/house_cluster.ron); doorsteps
// and hedges from scripts/blender/kit/house_pad.py (DOORSTEPS, HEDGES with HEDGE_W 0.65).
const HOUSE_CLUSTER: [(&str, [f32; 4]); 12] = [
    ("cottage 0", [-3.7, -9.2, 2.7, -4.8]),
    ("cottage 1", [-3.7, 4.8, 2.7, 9.2]),
    ("two-storey", [-10.0, -3.6, -5.0, 3.6]),
    ("doorstep 0", [-1.2, -4.8, 0.2, -3.9]),
    ("doorstep 1", [-1.2, 3.9, 0.2, 4.8]),
    ("doorstep 2", [-5.0, -0.8, -4.1, 0.8]),
    ("driveway hedge -", [3.1, -5.78, 7.6, -5.12]),
    ("driveway hedge +", [3.1, 5.12, 7.6, 5.78]),
    ("back hedge -", [-4.0, -10.93, 3.0, -10.27]),
    ("back hedge +", [-4.0, 10.27, 3.0, 10.93]),
    ("garden hedge -", [-11.6, -5.33, -5.7, -4.67]),
    ("garden hedge +", [-11.6, 4.67, -5.7, 5.33]),
];
const CAR: [f32; 2] = [5.4, 2.0];

/// The clearance obstacles of every destination tile with parking (PRD §7.1). A new parking tile
/// must declare its buildings, kerbs and hedges here and get a sweep below.
fn obstacles(tile: &str) -> Option<&'static [(&'static str, [f32; 4])]> {
    match tile {
        "house_cluster" => Some(&HOUSE_CLUSTER),
        _ => None,
    }
}

/// Every catalog tile with parking declares its clearance obstacles, so its parking paths are
/// swept like house_cluster's.
#[test]
fn every_parking_tile_declares_its_clearance_obstacles() {
    let missing: Vec<String> = catalog()
        .tiles()
        .iter()
        .filter(|d| !d.parking.is_empty() && obstacles(&d.id).is_none())
        .map(|d| d.id.clone())
        .collect();
    assert!(
        missing.is_empty(),
        "parking tiles without clearance obstacles: {missing:?}"
    );
}

fn corners(pos: Vec2, dir: Vec2) -> [Vec2; 4] {
    let r = [-dir[1], dir[0]];
    let (hl, hw) = (CAR[0] / 2.0, CAR[1] / 2.0);
    [(hl, hw), (hl, -hw), (-hl, -hw), (-hl, hw)]
        .map(|(a, b)| geom::add(pos, geom::add(geom::scale(dir, a), geom::scale(r, b))))
}

/// Separating-axis test of a car rectangle against an axis-aligned box.
fn overlaps(car: &[Vec2; 4], b: [f32; 4]) -> bool {
    let boxc = [[b[0], b[1]], [b[2], b[1]], [b[2], b[3]], [b[0], b[3]]];
    for poly in [&car[..], &boxc[..]] {
        for i in 0..4 {
            let e = geom::sub(poly[(i + 1) % 4], poly[i]);
            let n = [e[1], -e[0]];
            let span = |pts: &[Vec2]| {
                let v: Vec<f32> = pts.iter().map(|p| geom::dot(*p, n)).collect();
                (
                    v.iter().copied().fold(f32::INFINITY, f32::min),
                    v.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                )
            };
            let (a0, a1) = span(car);
            let (b0, b1) = span(&boxc);
            if a1 < b0 + 1e-3 || b1 < a0 + 1e-3 {
                return false;
            }
        }
    }
    true
}

fn to_local(p: Vec2, cell: Axial, rotation: u8) -> Vec2 {
    geom::rotate(
        geom::sub(p, cell.to_world(HEX_RADIUS)),
        (6 - rotation % 6) % 6,
    )
}

#[test]
fn parking_paths_clear_houses_hedges_and_other_spots() {
    let scene = Scene::with(11, busy(), &house_chain());
    sweep(scene, "house_cluster", |cell| if cell == A { 0 } else { 3 });
}

/// Drives `scene` for 6000 ticks and checks every trip's path, near each end's `tile`, keeps a
/// pickup-sized car clear of the tile's obstacles and of every other spot, and that every spot saw
/// both a departure and an arrival. `rotation` gives each `tile` cell's placed rotation.
fn sweep(mut scene: Scene, tile: &str, rotation: impl Fn(Axial) -> u8) {
    let def = scene.catalog.get(tile).unwrap().clone();
    let obstacles = obstacles(tile).expect("declared above");
    let spot_boxes: Vec<[Vec2; 4]> = def
        .parking
        .iter()
        .map(|s| corners(s.at, geom::heading(s.yaw_deg)))
        .collect();
    let (mut froms, mut tos) = (BTreeSet::new(), BTreeSet::new());
    let mut seen = BTreeSet::new();
    for _ in 0..6000 {
        scene.sim.tick();
        for car in scene.sim.cars() {
            if !seen.insert((car.id(), car.from_spot(), car.to_spot())) {
                continue;
            }
            froms.insert(car.from_spot().index);
            tos.insert(car.to_spot().index);
            let mut d = 0.0;
            while d <= car.path().length() {
                let pose = car.pose_at_distance(d);
                d += 0.1;
                for (end, own) in [
                    (car.from_spot(), car.from_spot().index),
                    (car.to_spot(), car.to_spot().index),
                ] {
                    if geom::dist(pose.pos, end.cell.to_world(HEX_RADIUS)) > HEX_RADIUS {
                        continue;
                    }
                    let r = rotation(end.cell);
                    let p = to_local(pose.pos, end.cell, r);
                    let dir = geom::rotate(pose.dir, (6 - r) % 6);
                    let c = corners(p, dir);
                    for (name, b) in obstacles {
                        assert!(!overlaps(&c, *b), "car at {p:?} facing {dir:?} hits {name}");
                    }
                    for (i, s) in spot_boxes.iter().enumerate() {
                        if i == own {
                            continue;
                        }
                        let xs = s.map(|q| q[0]);
                        let zs = s.map(|q| q[1]);
                        let b = [
                            xs.iter().copied().fold(f32::INFINITY, f32::min),
                            zs.iter().copied().fold(f32::INFINITY, f32::min),
                            xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                            zs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                        ];
                        assert!(
                            !overlaps(&c, b),
                            "car at {p:?} facing {dir:?} crosses spot {i}"
                        );
                    }
                }
            }
        }
    }
    let all: BTreeSet<usize> = (0..def.parking.len()).collect();
    assert_eq!(froms, all, "{tile}: departures from every spot");
    assert_eq!(tos, all, "{tile}: arrivals in every spot");
}
