//! Cars: fixed tick, interpolation, right-hand lanes, departure scaling and cap, parking on
//! arrival, removal rules, determinism, and never leaving the road graph or the driveways.

mod common;

use common::*;
use hex_core::{Axial, NetworkKind};
use hex_sim::geom;
use hex_sim::{
    CAR_FADE_TICKS, CarId, HIGH_TIER_CAR_CAP, MAX_LANE_OFFSET, SimConfig, Simulation, TICK_HZ,
    TICK_SECONDS,
};

fn busy() -> SimConfig {
    SimConfig {
        spawn_per_destination_hz: 0.5,
        ..Default::default()
    }
}

/// Every car's pose, bit for bit, at three interpolation points; parked cars repeat their spot.
fn snapshot(sim: &Simulation) -> Vec<(CarId, [u32; 12])> {
    let parked = sim.parked().iter().map(|c| {
        let p = c.pose();
        let mut bits = [0u32; 12];
        for i in 0..3 {
            bits[i * 4] = p.pos[0].to_bits();
            bits[i * 4 + 1] = p.pos[1].to_bits();
            bits[i * 4 + 2] = p.dir[0].to_bits();
            bits[i * 4 + 3] = p.dir[1].to_bits();
        }
        (c.id(), bits)
    });
    sim.cars()
        .iter()
        .map(|c| {
            let mut bits = [0u32; 12];
            for (i, alpha) in [0.0, 0.5, 1.0].into_iter().enumerate() {
                let p = c.pose_at(alpha);
                bits[i * 4] = p.pos[0].to_bits();
                bits[i * 4 + 1] = p.pos[1].to_bits();
                bits[i * 4 + 2] = p.dir[0].to_bits();
                bits[i * 4 + 3] = p.dir[1].to_bits();
            }
            (c.id(), bits)
        })
        .chain(parked)
        .collect()
}

/// Edits applied at fixed ticks during a 1,000-tick run.
fn scripted_edits() -> Vec<(u64, Edit)> {
    vec![
        (200, Edit::Bulldoze(ring_cell(1))),
        (400, Edit::Place(ring_cell(1), "road_bend_120", 3)),
        (600, Edit::Bulldoze(Axial::new(5, 0))),
        (800, Edit::Place(Axial::new(5, 0), "house_cluster", 3)),
    ]
}

fn run_scripted(seed: u64) -> Vec<Vec<(CarId, [u32; 12])>> {
    let mut scene = Scene::with(seed, busy(), &ring(&[0, 2]));
    scene.apply_all(&[
        Edit::Place(Axial::new(3, 0), "meadow", 0),
        Edit::Place(Axial::new(4, 0), "house_cluster", 0),
        Edit::Place(Axial::new(5, 0), "house_cluster", 3),
    ]);
    let edits = scripted_edits();
    let mut frames = Vec::with_capacity(1000);
    for t in 0..1000u64 {
        for (_, edit) in edits.iter().filter(|(at, _)| *at == t) {
            scene.apply(edit);
        }
        scene.sim.tick();
        frames.push(snapshot(&scene.sim));
    }
    frames
}

#[test]
fn tick_constants_are_20_hz() {
    assert_eq!(TICK_HZ, 20);
    assert_eq!(TICK_SECONDS, 0.05);
    assert_eq!(HIGH_TIER_CAR_CAP, 300);
    assert_eq!(SimConfig::default().max_cars, 300);
}

#[test]
fn same_seed_and_edits_give_identical_poses_over_1000_ticks() {
    let a = run_scripted(42);
    let b = run_scripted(42);
    assert_eq!(a.len(), 1000);
    let cars: usize = a.iter().map(Vec::len).sum();
    assert!(cars > 1000, "too few car-ticks to prove anything: {cars}");
    assert_eq!(a, b);
    assert_ne!(
        a,
        run_scripted(43),
        "the seed must drive destination choice"
    );
}

#[test]
fn cars_never_leave_the_road_graph() {
    // Seeds whose random walks carry enough traffic for the car-tick floor below; the walk draws
    // from the catalog, so a new tile changes every world.
    for seed in [1u64, 3, 4] {
        let mut scene = Scene::new(seed, busy());
        let mut rng = TestRng(seed * 31);
        for _ in 0..200 {
            if let Some(edit) = random_edit(&scene.world, &scene.catalog, &mut rng) {
                scene.apply(&edit);
            }
        }
        let mut seen = 0;
        for t in 0..1000 {
            if t % 50 == 25
                && let Some(edit) = random_edit(&scene.world, &scene.catalog, &mut rng)
            {
                scene.apply(&edit);
            }
            scene.sim.tick();
            let net = scene.sim.network(NetworkKind::Road);
            for car in scene.sim.cars() {
                let lines: Vec<_> = car
                    .links()
                    .iter()
                    .map(|l| net.link(*l).expect("car route uses a live link").points())
                    .collect();
                let (lane_start, lane_end) = car.lane_span();
                let at = car.distance();
                if at < lane_start || at > lane_end {
                    // On a driveway: inside the cluster it leaves or the one it parks in.
                    let p = car.pose_at(1.0).pos;
                    let near = [car.from_spot().cell, car.to_spot().cell]
                        .into_iter()
                        .any(|c| {
                            geom::dist(p, c.to_world(hex_core::HEX_RADIUS)) <= hex_core::HEX_RADIUS
                        });
                    assert!(
                        near,
                        "seed {seed} tick {t}: car {:?} left its driveways",
                        car.id()
                    );
                    seen += 1;
                    continue;
                }
                {
                    let p = car.pose_at(1.0).pos;
                    let d = lines
                        .iter()
                        .map(|pts| geom::distance_to_polyline(p, pts))
                        .fold(f32::INFINITY, f32::min);
                    // A destination spur's inner corner may swing wider (geom::INNER_SWING).
                    let spur = [car.from_spot().cell, car.to_spot().cell]
                        .into_iter()
                        .any(|c| {
                            geom::dist(p, c.to_world(hex_core::HEX_RADIUS)) <= hex_core::HEX_RADIUS
                        });
                    // So may a right turn through a junction node (geom::TURN_SWING).
                    let junction = car.links().iter().any(|l| {
                        let (a, b) = net.link(*l).expect("live link").ends();
                        [a, b].into_iter().filter_map(|n| net.node(n)).any(|n| {
                            n.links().len() >= 3 && geom::dist(p, n.pos()) <= hex_core::HEX_RADIUS
                        })
                    });
                    let band = MAX_LANE_OFFSET
                        + if spur { geom::INNER_SWING } else { 0.0 }
                        + if junction { geom::TURN_SWING } else { 0.0 };
                    assert!(
                        d <= band + 1e-3,
                        "seed {seed} tick {t}: car {:?} is {d} m off its road",
                        car.id()
                    );
                }
                seen += 1;
            }
        }
        assert!(seen > 500, "seed {seed}: only {seen} car-ticks");
    }
}

#[test]
fn interpolation_runs_from_the_previous_tick_to_the_latest() {
    let mut scene = Scene::with(9, busy(), &ring_and_pair());
    let mut last: Vec<(CarId, geom::Vec2)> = vec![];
    let mut checked = 0;
    for _ in 0..400 {
        scene.sim.tick();
        for car in scene.sim.cars() {
            if let Some((_, p)) = last.iter().find(|(id, _)| *id == car.id()) {
                assert_eq!(car.pose_at(0.0).pos, *p, "alpha 0 is the previous tick");
                checked += 1;
            }
            let (p0, p1) = (car.pose_at(0.0).pos, car.pose_at(1.0).pos);
            let mid = car.pose_at(0.5).pos;
            // Half a tick of travel each way; straight-line distances, so a reversing cusp only
            // shortens them.
            let half = scene.sim.config().car_speed * TICK_SECONDS / 2.0;
            assert!(geom::dist(p0, p1) <= 2.0 * half + 1e-3);
            assert!(geom::dist(p0, mid) <= half + 1e-3 && geom::dist(mid, p1) <= half + 1e-3);
            assert_eq!(car.pose_at(-1.0), car.pose_at(0.0));
            assert_eq!(car.pose_at(2.0), car.pose_at(1.0));
        }
        last = scene
            .sim
            .cars()
            .iter()
            .map(|c| (c.id(), c.pose_at(1.0).pos))
            .collect();
    }
    assert!(checked > 100);
}

#[test]
fn cars_drive_on_the_right() {
    let mut scene = Scene::with(11, busy(), &house_chain());
    let road_z = Axial::new(2, 0).to_world(hex_core::HEX_RADIUS)[1];
    let mut east = 0;
    let mut west = 0;
    for _ in 0..600 {
        scene.sim.tick();
        for car in scene.sim.cars() {
            let pose = car.pose_at(1.0);
            // Only the straight middle tile, away from the house spurs.
            if (pose.pos[0] - Axial::new(2, 0).to_world(hex_core::HEX_RADIUS)[0]).abs() > 10.0 {
                continue;
            }
            if pose.dir[0] > 0.99 {
                assert!((pose.pos[1] - (road_z + 1.5)).abs() < 1e-3, "{pose:?}");
                east += 1;
            } else if pose.dir[0] < -0.99 {
                assert!((pose.pos[1] - (road_z - 1.5)).abs() < 1e-3, "{pose:?}");
                west += 1;
            }
        }
    }
    assert!(east > 0 && west > 0, "east {east} west {west}");
}

#[test]
fn lane_offset_is_capped_at_one_and_a_half_meters() {
    let sim = Simulation::new(
        0,
        SimConfig {
            lane_offset: 4.0,
            ..Default::default()
        },
    );
    assert_eq!(sim.config().lane_offset, MAX_LANE_OFFSET);
}

#[test]
fn departure_rate_scales_with_connected_destinations() {
    let lone = [Edit::Place(Axial::new(1, 0), "house_cluster", 0)];
    let cases: [(&[Edit], usize); 3] = [(&lone, 0), (&house_chain(), 2), (&ring_and_pair(), 5)];
    let config = busy();
    for (edits, want) in cases {
        let mut scene = Scene::with(5, config, edits);
        assert_eq!(scene.sim.connected_destinations(), want);
        assert_eq!(
            scene.sim.spawn_rate_hz(),
            want as f32 * config.spawn_per_destination_hz
        );
        for _ in 0..400 {
            scene.sim.tick();
        }
        // A drawn departure starts a trip, is refused for want of a free spot, or finds every
        // car already on the road.
        let expected = want as f32 * config.spawn_per_destination_hz * 400.0 * TICK_SECONDS;
        let drawn = (scene.sim.departures() + scene.sim.refused()) as f32;
        assert!(
            drawn <= expected + 1.0 && drawn >= expected / 2.0,
            "{want} destinations: {drawn} departures drawn, expected about {expected}"
        );
        assert_eq!(scene.sim.departures() > 0, want > 0);
    }
}

#[test]
fn car_count_never_exceeds_the_cap() {
    let config = SimConfig {
        max_cars: 3,
        spawn_per_destination_hz: 5.0,
        ..Default::default()
    };
    let mut scene = Scene::with(13, config, &ring_and_pair());
    assert_eq!(scene.sim.car_count(), 3);
    let mut peak = 0;
    for _ in 0..1000 {
        scene.sim.tick();
        assert_eq!(scene.sim.car_count(), 3);
        peak = peak.max(scene.sim.cars().len());
    }
    assert!(peak > 0);
}

/// A quality-tier change under reduced motion (PRD §7.2): lowering the cap with no fade removes
/// the newest cars at once, parked or driving, and frees their spots; raising it parks the missing
/// cars again. Same seed, same result.
#[test]
fn lowering_the_cap_removes_the_newest_cars_and_raising_it_refills() {
    let run = || {
        let mut scene = Scene::with(13, busy(), &ring_and_pair());
        let full = scene.sim.car_count();
        while scene.sim.cars().len() < 2 {
            scene.sim.tick();
        }
        let mut before: Vec<CarId> = scene
            .sim
            .cars()
            .iter()
            .map(|c| c.id())
            .chain(scene.sim.parked().iter().map(|c| c.id()))
            .collect();
        before.sort();
        scene.sim.set_max_cars(2, 0);
        assert_eq!(scene.sim.config().max_cars, 2);
        let mut kept: Vec<CarId> = scene
            .sim
            .cars()
            .iter()
            .map(|c| c.id())
            .chain(scene.sim.parked().iter().map(|c| c.id()))
            .collect();
        kept.sort();
        assert_eq!(kept, before[..2], "the two oldest cars stay");
        // No spot is held by a car that is gone.
        let clusters = [
            ring_house(0),
            ring_house(2),
            ring_house(4),
            Axial::new(4, 0),
            Axial::new(5, 0),
        ];
        for cell in clusters {
            assert!(!scene.sim.spots(cell).is_empty());
            for (_, holder) in scene.sim.spots(cell) {
                assert!(holder.is_none_or(|id| kept.contains(&id)), "{holder:?}");
            }
        }
        for _ in 0..400 {
            scene.sim.tick();
            assert!(scene.sim.car_count() <= 2);
        }
        let low = snapshot(&scene.sim);
        scene.sim.set_max_cars(HIGH_TIER_CAR_CAP, 0);
        assert_eq!(scene.sim.car_count(), full, "every house has its car again");
        (kept, low, snapshot(&scene.sim))
    };
    assert_eq!(run(), run());
}

fn ids(sim: &Simulation) -> Vec<CarId> {
    let mut ids: Vec<CarId> = sim
        .cars()
        .iter()
        .map(|c| c.id())
        .chain(sim.parked().iter().map(|c| c.id()))
        .collect();
    ids.sort();
    ids
}

/// A quality-tier drop (PRD §7.2): the newest cars over the new cap fade out over CAR_FADE_TICKS
/// instead of vanishing. While fading they keep driving (a parked one starts no trip), their fade
/// rises from 0 to 1 tick by tick, no car is added, and the tick after the fade they are gone with
/// their spots free. Raising the cap mid-fade keeps the cars back under it. Same seed, same result.
#[test]
fn lowering_the_cap_fades_the_newest_cars_out_then_removes_them() {
    let run = || {
        let mut scene = Scene::with(13, busy(), &ring_and_pair());
        while scene.sim.cars().len() < 3 {
            scene.sim.tick();
        }
        let before = ids(&scene.sim);
        scene.sim.set_max_cars(2, CAR_FADE_TICKS);
        let going: Vec<CarId> = before[2..].to_vec();
        assert_eq!(
            scene.sim.leaving().keys().copied().collect::<Vec<_>>(),
            going,
            "the newest cars fade, chosen by id"
        );
        assert_eq!(ids(&scene.sim), before, "nothing vanishes at once");
        let driving: Vec<CarId> = scene
            .sim
            .cars()
            .iter()
            .map(|c| c.id())
            .filter(|id| going.contains(id))
            .collect();
        assert!(!driving.is_empty(), "a driving car is among them");
        let mut fades = vec![];
        for alpha in [0.0, 1.0] {
            fades.push(scene.sim.fade(going[0], alpha));
        }
        for _ in 0..CAR_FADE_TICKS {
            let dist: Vec<f32> = driving
                .iter()
                .map(|id| {
                    scene
                        .sim
                        .cars()
                        .iter()
                        .find(|c| c.id() == *id)
                        .map_or(0.0, |c| c.distance())
                })
                .collect();
            scene.sim.tick();
            assert_eq!(
                ids(&scene.sim),
                before,
                "still fading, still drawn, none added"
            );
            for (id, d) in driving.iter().zip(dist) {
                if let Some(c) = scene.sim.cars().iter().find(|c| c.id() == *id) {
                    assert!(c.distance() >= d, "a fading car keeps driving");
                }
            }
            for alpha in [0.0, 0.5, 1.0] {
                fades.push(scene.sim.fade(going[0], alpha));
            }
        }
        assert!(fades.windows(2).all(|w| w[1] >= w[0]), "{fades:?}");
        assert_eq!((fades[0], *fades.last().unwrap()), (0.0, 1.0));
        scene.sim.tick();
        assert_eq!(ids(&scene.sim), before[..2], "gone the tick after the fade");
        assert!(scene.sim.leaving().is_empty());
        for cell in [
            ring_house(0),
            ring_house(2),
            ring_house(4),
            Axial::new(4, 0),
            Axial::new(5, 0),
        ] {
            for (_, holder) in scene.sim.spots(cell) {
                assert!(
                    holder.is_none_or(|id| before[..2].contains(&id)),
                    "{holder:?}"
                );
            }
        }
        // Raising the cap mid-fade: the cars back under it stay, opaque.
        scene.sim.set_max_cars(HIGH_TIER_CAR_CAP, 0);
        let full = ids(&scene.sim);
        scene.sim.set_max_cars(3, CAR_FADE_TICKS);
        scene.sim.tick();
        scene.sim.set_max_cars(4, CAR_FADE_TICKS);
        assert_eq!(scene.sim.leaving().len(), full.len() - 4);
        assert_eq!(scene.sim.fade(full[3], 1.0), 0.0, "back under the cap");
        for _ in 0..=CAR_FADE_TICKS {
            scene.sim.tick();
        }
        assert_eq!(ids(&scene.sim), full[..4]);
        (before, fades, snapshot(&scene.sim))
    };
    assert_eq!(run(), run());
}

#[test]
fn car_parks_the_tick_after_it_arrives() {
    let mut scene = Scene::with(17, busy(), &house_chain());
    while scene.sim.cars().is_empty() {
        scene.sim.tick();
    }
    let car = scene.sim.cars()[0].clone();
    let length = car.path().length();
    let ticks = (length / (scene.sim.config().car_speed * TICK_SECONDS)).ceil() as u64;
    for _ in 0..ticks {
        scene.sim.tick();
    }
    let at_end = scene
        .sim
        .cars()
        .iter()
        .find(|c| c.id() == car.id())
        .expect("still drawn on its arrival tick");
    assert!(at_end.arrived());
    let end = car.path().points().last().copied().unwrap();
    assert_eq!(at_end.pose_at(1.0).pos, end);
    scene.sim.tick();
    assert!(scene.sim.cars().iter().all(|c| c.id() != car.id()));
    let parked = scene
        .sim
        .parked()
        .iter()
        .find(|c| c.id() == car.id())
        .expect("parked on the next tick");
    assert_eq!(parked.spot(), car.to_spot());
    assert!(geom::dist(parked.pose().pos, end) < 1e-3);
}

#[test]
fn bulldoze_despawns_only_cars_routed_over_the_tile() {
    let mut scene = Scene::with(19, busy(), &ring_and_pair());
    for _ in 0..200 {
        scene.sim.tick();
    }
    let cut = ring_cell(1);
    let before: Vec<_> = scene.sim.cars().to_vec();
    let crossing = before
        .iter()
        .filter(|c| c.links().iter().any(|l| l.cell == cut))
        .count();
    assert!(crossing > 0 && crossing < before.len());
    scene.apply(&Edit::Bulldoze(cut));
    let after = scene.sim.cars();
    assert_eq!(after.len(), before.len() - crossing);
    assert!(
        after
            .iter()
            .all(|c| c.links().iter().all(|l| l.cell != cut))
    );
}

/// 2,000 tiles: a 1,000-junction road zigzagging east and north-east, each junction's free branch
/// ending at a house. One component holding 1,000 destinations.
fn zigzag(seed: u64) -> Simulation {
    let cat = catalog();
    let mut sim = Simulation::new(
        seed,
        SimConfig {
            spawn_per_destination_hz: 0.2,
            ..Default::default()
        },
    );
    let junction = cat.get("road_junction").unwrap();
    let house = cat.get("house_cluster").unwrap();
    for i in 0..500i32 {
        let even = Axial::new(2 * i, -i);
        sim.apply_place(even, junction, 0);
        sim.apply_place(even.neighbor(4), house, 1);
        let odd = Axial::new(2 * i + 1, -i);
        sim.apply_place(odd, junction, 1);
        sim.apply_place(odd.neighbor(1), house, 4);
    }
    sim
}

#[test]
fn two_thousand_tile_world_fills_to_the_high_tier_cap() {
    let mut sim = zigzag(99);
    assert_eq!(sim.connected_destinations(), 1000);
    assert_eq!(sim.car_count(), HIGH_TIER_CAR_CAP);
    for _ in 0..200 {
        sim.tick();
        assert_eq!(sim.car_count(), HIGH_TIER_CAR_CAP);
    }
    assert!(!sim.cars().is_empty());
}

/// Tick-cost sanity bound against the 4 ms main-thread budget (PRD §10). A timing measurement:
/// run it only with the host load guard satisfied (`cargo test -p hex-sim --release -- --ignored`).
#[test]
#[ignore]
fn bench_tick_stays_within_budget_at_the_car_cap() {
    let mut sim = zigzag(99);
    for _ in 0..400 {
        sim.tick();
    }
    let ticks = 1000;
    let start = std::time::Instant::now();
    for _ in 0..ticks {
        sim.tick();
    }
    let mean_ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(ticks);
    println!("mean tick {mean_ms:.4} ms with {} cars", sim.cars().len());
    assert!(
        mean_ms <= 4.0,
        "mean tick {mean_ms} ms over the 4 ms budget"
    );
}
