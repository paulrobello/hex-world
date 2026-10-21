//! Cars in the play scenario (PRD §7.2): a [`hex_sim::Simulation`] mirrored from the game world,
//! ticked at a fixed 20 Hz in `FixedUpdate` and drawn by `hex_play::cars` at the poses
//! interpolated with the fixed clock's overstep.
use std::collections::BTreeMap;

use bevy::prelude::*;
use hex_core::{Axial, PlacedTile, TileCatalog, World};
use hex_play::{
    ActiveScenario, AgentsPaused, Scenario,
    cars::{CarKind, CarPose, CarsView},
    play::{PlaySystems, rise::PlayRises},
    trains::{StockKind, StockPose, TrainsView},
};
use hex_sim::{SimConfig, Simulation, TICK_HZ};
use serde_json::{Value, json};

use crate::game::{GameEvent, GameState};

/// The 20 Hz tick. Nothing else in the app runs in `FixedUpdate`, so `Time<Fixed>` is the sim's.
///
/// It also orders the render's play systems between the game's view writer and the car mirror, so
/// a placement starts its rise in the frame it lands and the mirror's agents gate reads that frame's
/// rise state (frame-exact under `--clock fixed`), and publishes the rise cues after them.
pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(f64::from(TICK_HZ)))
            .add_systems(FixedUpdate, tick)
            .configure_sets(
                Update,
                PlaySystems
                    .after(crate::game::sync_view)
                    .before(mirror_world),
            )
            .add_systems(
                Update,
                crate::game::publish_rise_events
                    .after(PlaySystems)
                    .before(crate::control::publish_state),
            );
    }
}

/// What the state JSON reports about the cars, refreshed after every tick and world edit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SimStats {
    /// Cars driving.
    pub cars: usize,
    /// Cars standing in parking spots (PRD §7.1).
    pub parked_cars: usize,
    pub connected_destinations: usize,
    pub spawn_rate_hz: f32,
    /// Every driving car: id, trip phase and position (for scripted checkpoints).
    pub car_list: Vec<(u64, &'static str, [f32; 2])>,
    /// Every train (P3).
    pub train_list: Vec<TrainStat>,
    /// Every junction: cell, the leg its switch is set to and the legs facing trains took.
    pub junctions: Vec<(Axial, &'static str, Vec<&'static str>)>,
    /// Level crossings closed to cars.
    pub closed_crossings: Vec<Axial>,
}

/// A train as the state JSON reports it.
#[derive(Clone, Debug, PartialEq)]
pub struct TrainStat {
    pub id: u64,
    /// The front's world XZ.
    pub front: [f32; 2],
    /// m/s.
    pub speed: f32,
    /// What it waits for ([`blocker_name`]).
    pub waiting: String,
    /// The cell of the station it is heading for.
    pub destination: Option<Axial>,
    /// Meters of track along its route to that station's stop.
    pub route: Option<f32>,
}

#[derive(Resource)]
pub struct CarSim {
    sim: Simulation,
    /// The world's tiles as last mirrored into `sim`.
    mirrored: BTreeMap<Axial, PlacedTile>,
    stats: SimStats,
    /// Fixed ticks run so far (photo mode's pause holds it still).
    ticks: u64,
    /// Trains that left a station stop on the last tick: they start moving on this one, and their
    /// whistle has already gone.
    left_station: std::collections::BTreeSet<u64>,
}

impl CarSim {
    /// A simulation of `world`, seeded with the world seed.
    pub fn new(world: &World, catalog: &TileCatalog) -> Self {
        let mut s = Self {
            sim: Simulation::from_world(world, catalog, SimConfig::default()),
            mirrored: world.tiles().clone(),
            stats: SimStats::default(),
            ticks: 0,
            left_station: Default::default(),
        };
        s.refresh();
        s
    }

    /// Brings the sim up to `world` by diffing its tiles against the last mirrored ones: a tile
    /// that is gone is a bulldoze, a new or changed tile a place. That covers every path that
    /// edits the world (place, bulldoze, the undo of either, redo, a wholesale replacement), since
    /// undoing a place removes the tile and undoing a bulldoze restores the stored one. Tiles
    /// missing from `catalog` are left out, as `Simulation::from_world` does. Returns whether the
    /// world had changed.
    pub fn mirror(&mut self, world: &World, catalog: &TileCatalog) -> bool {
        self.mirror_tiles(world.tiles(), catalog)
    }

    /// [`CarSim::mirror`] with an agents gate: a tile not mirrored yet joins the sim only once
    /// `ready` says its staged rise reached the agents stage (PRD §4.4 step 5), so cars never use a
    /// road that is still rising. Tiles already mirrored stay; removals apply at once.
    pub fn mirror_gated(
        &mut self,
        world: &World,
        catalog: &TileCatalog,
        ready: impl Fn(Axial) -> bool,
    ) -> bool {
        let now: BTreeMap<Axial, PlacedTile> = world
            .tiles()
            .iter()
            .filter(|(c, t)| self.mirrored.get(c) == Some(t) || ready(**c))
            .map(|(c, t)| (*c, t.clone()))
            .collect();
        self.mirror_tiles(&now, catalog)
    }

    fn mirror_tiles(&mut self, now: &BTreeMap<Axial, PlacedTile>, catalog: &TileCatalog) -> bool {
        let removed: Vec<Axial> = self
            .mirrored
            .keys()
            .filter(|c| !now.contains_key(c))
            .copied()
            .collect();
        let placed: Vec<(Axial, &PlacedTile)> = now
            .iter()
            .filter(|(c, t)| self.mirrored.get(c) != Some(t))
            .map(|(c, t)| (*c, t))
            .collect();
        if removed.is_empty() && placed.is_empty() {
            return false;
        }
        for cell in removed {
            self.sim.apply_bulldoze(cell);
            self.mirrored.remove(&cell);
        }
        for (cell, tile) in placed {
            match catalog.get(&tile.def) {
                Some(def) => self.sim.apply_place(cell, def, tile.rotation),
                None => self.sim.apply_bulldoze(cell),
            }
            self.mirrored.insert(cell, tile.clone());
        }
        self.refresh();
        true
    }

    /// One fixed tick of `hex_sim::TICK_SECONDS`. Returns the trips that ended and began on it, in
    /// that order: every car that had arrived parks on this tick, and a car departing on it starts
    /// at distance 0 (a car may do both). Cars removed by a world edit are neither.
    pub fn tick(&mut self) -> Vec<GameEvent> {
        let mut events: Vec<GameEvent> = self
            .sim
            .cars()
            .iter()
            .filter(|c| c.arrived())
            .map(|c| car_event(c.id().0, c.to_spot().cell, c.pose_at(1.0).pos, true))
            .collect();
        let moving: BTreeMap<u64, bool> = self
            .sim
            .rails()
            .trains()
            .iter()
            .map(|t| (t.id().0, t.speed() > 0.0))
            .collect();
        self.sim.tick();
        self.ticks += 1;
        self.refresh();
        events.extend(
            self.sim
                .cars()
                .iter()
                .filter(|c| c.distance() <= 0.0)
                .map(|c| car_event(c.id().0, c.from_spot().cell, c.pose_at(0.0).pos, false)),
        );
        // A departure is a train leaving a station stop on this tick (`Railway::departures`, its
        // dwell over, still at rest), or a train moving off from any other stand. A train spawned
        // on this tick stood still before it. A train that left a station last tick moves off now
        // without a second departure.
        let rails = self.sim.rails();
        let stations: std::collections::BTreeSet<u64> =
            rails.departures().iter().map(|(id, _)| id.0).collect();
        let cm = |v: f32| (v * 100.0).round() / 100.0;
        events.extend(
            rails
                .trains()
                .iter()
                .filter(|t| {
                    let id = t.id().0;
                    stations.contains(&id)
                        || (t.speed() > 0.0
                            && moving.get(&id) != Some(&true)
                            && !self.left_station.contains(&id))
                })
                .map(|t| {
                    let p = t.front();
                    GameEvent::TrainDeparted {
                        id: t.id().0,
                        x: cm(p[0]),
                        z: cm(p[1]),
                    }
                }),
        );
        self.left_station = stations;
        events.extend(
            rails
                .line_end_arrivals()
                .iter()
                .map(|&(id, p)| GameEvent::TrainAtLineEnd {
                    id: id.0,
                    x: cm(p[0]),
                    z: cm(p[1]),
                }),
        );
        events
    }

    /// The road or river routing graph as last mirrored.
    pub fn network(&self, kind: hex_core::NetworkKind) -> &hex_sim::network::Network {
        self.sim.network(kind)
    }

    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    fn refresh(&mut self) {
        self.stats = SimStats {
            cars: self.sim.cars().len(),
            parked_cars: self.sim.parked().len(),
            connected_destinations: self.sim.connected_destinations(),
            spawn_rate_hz: self.sim.spawn_rate_hz(),
            car_list: self
                .sim
                .cars()
                .iter()
                .map(|c| (c.id().0, c.phase().as_str(), c.pose_at(1.0).pos))
                .collect(),
            train_list: self
                .sim
                .rails()
                .trains()
                .iter()
                .map(|t| TrainStat {
                    id: t.id().0,
                    front: t.front(),
                    speed: t.speed(),
                    waiting: blocker_name(t.blocker()),
                    destination: t.destination(),
                    route: self.sim.rails().route_length(t),
                })
                .collect(),
            junctions: self
                .sim
                .rails()
                .junctions()
                .iter()
                .map(|j| {
                    (
                        j.cell(),
                        j.set().as_str(),
                        j.facing().iter().map(|l| l.as_str()).collect(),
                    )
                })
                .collect(),
            closed_crossings: self.sim.rails().closed_crossings(),
        };
    }

    /// Adds the car fields to the `play` object of the state JSON.
    pub fn extend_state(&self, play: &mut Value) {
        let Value::Object(map) = play else {
            return;
        };
        let rate = (f64::from(self.stats.spawn_rate_hz) * 1e6).round() / 1e6;
        map.insert("cars".into(), json!(self.stats.cars));
        map.insert("parked_cars".into(), json!(self.stats.parked_cars));
        let round = |v: f32| (f64::from(v) * 100.0).round() / 100.0;
        map.insert(
            "car_list".into(),
            json!(
                self.stats
                    .car_list
                    .iter()
                    .map(|(id, phase, p)| json!({
                        "id": id, "phase": phase, "x": round(p[0]), "z": round(p[1])
                    }))
                    .collect::<Vec<_>>()
            ),
        );
        map.insert(
            "connected_destinations".into(),
            json!(self.stats.connected_destinations),
        );
        map.insert("spawn_rate_hz".into(), json!(rate));
        map.insert("trains".into(), json!(self.stats.train_list.len()));
        map.insert(
            "train_list".into(),
            json!(
                self.stats
                    .train_list
                    .iter()
                    .map(|t| json!({
                        "id": t.id, "x": round(t.front[0]), "z": round(t.front[1]),
                        "speed": round(t.speed), "waiting": t.waiting,
                        "destination": t.destination.map(|c| json!({"q": c.q, "r": c.r})),
                        "route": t.route.map(|m| (f64::from(m) * 10.0).round() / 10.0),
                    }))
                    .collect::<Vec<_>>()
            ),
        );
        map.insert(
            "junctions".into(),
            json!(
                self.stats
                    .junctions
                    .iter()
                    .map(|(c, set, facing)| json!({
                        "q": c.q, "r": c.r, "set": set, "facing": facing
                    }))
                    .collect::<Vec<_>>()
            ),
        );
        map.insert(
            "closed_crossings".into(),
            json!(
                self.stats
                    .closed_crossings
                    .iter()
                    .map(|c| json!({"q": c.q, "r": c.r}))
                    .collect::<Vec<_>>()
            ),
        );
    }
}

/// What a train waits for, as the state JSON names it.
fn blocker_name(b: hex_sim::rail::Blocker) -> String {
    use hex_sim::rail::Blocker;
    match b {
        Blocker::Clear => "none".into(),
        Blocker::LineEnd => "line_end".into(),
        Blocker::Train(_) => "train".into(),
        Blocker::Junction(_) => "junction".into(),
        Blocker::Station(_) => "station".into(),
    }
}

/// A trip's end (`parked`) or start at a spot in `cell`, with the spot's world XZ to the centimetre.
fn car_event(id: u64, cell: Axial, pos: [f32; 2], parked: bool) -> GameEvent {
    let cm = |v: f32| (v * 100.0).round() / 100.0;
    let (q, r, x, z) = (cell.q, cell.r, cm(pos[0]), cm(pos[1]));
    if parked {
        GameEvent::CarParked { id, q, r, x, z }
    } else {
        GameEvent::CarPulledOut { id, q, r, x, z }
    }
}

/// Mirrors the world into the sim whenever the game state or a rise changed (after
/// `game::apply_actions` and the play systems). In the play scenario a placed tile waits for its
/// rise's agents stage ([`CarSim::mirror_gated`]).
/// The `networks` debug overlay: every road, river and rail link's centerline in its socket colour, each
/// node as a small ring, and destinations (car spawns and sinks) as a larger one. Gated by a run
/// condition, so it does nothing while the overlay is off.
pub fn draw_networks(
    sim: Res<CarSim>,
    mut gizmos: Gizmos<hex_play::debug::DebugGizmos>,
    mut report: ResMut<hex_play::debug::DebugReport>,
) {
    use hex_core::{NetworkKind, SocketKind};
    use hex_play::debug::{LINE_Y, ring};
    report.work += 1;
    let y = LINE_Y + 0.15;
    for (kind, socket) in [
        (NetworkKind::Road, SocketKind::Road),
        (NetworkKind::River, SocketKind::River),
        (NetworkKind::Rail, SocketKind::Rail),
    ] {
        let [r, g, b] = hex_play::play::affordances::socket_rgb(socket);
        let color = Color::srgb_u8(r, g, b);
        let net = sim.network(kind);
        for (_, link) in net.links() {
            gizmos.linestrip(
                link.points().iter().map(|p| Vec3::new(p[0], y, p[1])),
                color,
            );
        }
        for (_, node) in net.nodes() {
            let p = node.pos();
            let big = node.destination().is_some();
            ring(
                &mut gizmos,
                Vec3::new(p[0], y, p[1]),
                if big { 2.2 } else { 0.9 },
                if big { Color::WHITE } else { color },
            );
        }
    }
}

pub fn mirror_world(
    game: Res<GameState>,
    active: Res<ActiveScenario>,
    rises: Res<PlayRises>,
    mut sim: ResMut<CarSim>,
) {
    if !game.is_changed() && !rises.is_changed() {
        return;
    }
    if active.0 == Scenario::Play {
        sim.mirror_gated(&game.world, &game.catalog, |c| rises.agents_ready(c));
    } else {
        sim.mirror(&game.world, &game.catalog);
    }
}

/// The fixed tick; the sim runs only while the play scenario is shown and photo mode has not
/// paused the agents. Each trip's end and start goes out as an event (the audio layer's car cues).
pub fn tick(mut sim: ResMut<CarSim>, active: Res<ActiveScenario>, paused: Res<AgentsPaused>) {
    if active.0 == Scenario::Play && !paused.0 {
        for e in sim.tick() {
            crate::game::publish_event(&e);
        }
    }
}

/// Sends the boats' cues of the latest ticks (`hex_play::boats::BoatCues`) out as events: an oar
/// stroke and a sail flap for the audio layer.
pub fn publish_boat_cues(mut cues: ResMut<hex_play::boats::BoatCues>) {
    use hex_play::boats::plan::BoatCue;
    for c in cues.0.drain(..) {
        let e = match c {
            BoatCue::Stroke { id, at } => GameEvent::BoatStroke {
                id,
                x: at.x,
                z: at.y,
            },
            BoatCue::SailFlap { id, at } => GameEvent::SailFlap {
                id,
                x: at.x,
                z: at.y,
            },
        };
        crate::game::publish_event(&e);
    }
}

/// Writes each driving car's pose between the last two ticks, at the fraction of a tick the fixed
/// clock has accumulated since the latest one, and each parked car at its spot. A car keeps its id
/// from spot to spot, so the same entity parks, pulls out and parks again without popping. Paused,
/// the poses stay as last written: the fixed clock's overstep keeps cycling, and interpolating with
/// it would rock each car between two poses.
/// Writes every train vehicle's pose between the last two ticks (as `publish_cars` does for the
/// cars): each vehicle keeps its id (train id x 4 + place in the consist) for the train's life.
pub fn publish_trains(
    sim: Res<CarSim>,
    fixed: Res<Time<Fixed>>,
    paused: Res<AgentsPaused>,
    mut view: ResMut<TrainsView>,
) {
    if paused.0 {
        return;
    }
    let trains = sim.sim.rails().trains();
    if trains.is_empty() && view.vehicles.is_empty() {
        return;
    }
    let alpha = fixed.overstep_fraction();
    view.vehicles.clear();
    for t in trains {
        for (k, v) in t.vehicles(alpha).into_iter().enumerate() {
            view.vehicles.push(StockPose {
                id: t.id().0 * 4 + k as u64,
                train: t.id().0,
                kind: match v.kind {
                    hex_sim::VehicleKind::Engine => StockKind::Engine,
                    hex_sim::VehicleKind::Carriage => StockKind::Carriage,
                    hex_sim::VehicleKind::BrakeCarriage => StockKind::BrakeCarriage,
                },
                pos: v.pose.pos,
                yaw: v.pose.yaw(),
            });
        }
    }
}

pub fn publish_cars(
    sim: Res<CarSim>,
    fixed: Res<Time<Fixed>>,
    paused: Res<AgentsPaused>,
    mut view: ResMut<CarsView>,
) {
    if paused.0 {
        return;
    }
    let (cars, parked) = (sim.sim.cars(), sim.sim.parked());
    if cars.is_empty() && parked.is_empty() && view.cars.is_empty() {
        return;
    }
    let alpha = fixed.overstep_fraction();
    view.cars.clear();
    view.cars.extend(cars.iter().map(|c| {
        let pose = c.pose_at(alpha);
        CarPose {
            id: c.id().0,
            // Seeded by the car id (never reused), so a car keeps its body type for its whole trip.
            kind: CarKind::from_seed(c.id().0),
            pos: pose.pos,
            yaw: pose.yaw(),
        }
    }));
    view.cars.extend(parked.iter().map(|c| {
        let pose = c.pose();
        CarPose {
            id: c.id().0,
            kind: CarKind::from_seed(c.id().0),
            pos: pose.pos,
            yaw: pose.yaw(),
        }
    }));
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::actions::Action;
    use crate::control::{Command, parse_script};
    use hex_core::{NetworkKind, RotateDir};
    use hex_sim::{Network, NodeKey};

    /// SplitMix64, so the random sequences replay identically.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    fn keys(net: &Network) -> (BTreeSet<NodeKey>, BTreeSet<hex_sim::LinkId>) {
        (
            net.nodes().map(|(_, n)| n.key().clone()).collect(),
            net.links().map(|(id, _)| id).collect(),
        )
    }

    /// Asserts the mirrored sim's networks equal a fresh `Simulation::from_world` of `world`:
    /// node and link counts (and keys) per network, and route existence between every pair of
    /// houses. Returns the number of connected house pairs.
    fn assert_matches_world(mirror: &CarSim, world: &World, catalog: &TileCatalog) -> usize {
        let fresh = Simulation::from_world(world, catalog, SimConfig::default());
        for kind in NetworkKind::ALL {
            let (a, b) = (mirror.sim.network(kind), fresh.network(kind));
            assert_eq!(a.nodes().count(), b.nodes().count(), "{kind:?} nodes");
            assert_eq!(a.links().count(), b.links().count(), "{kind:?} links");
            assert_eq!(keys(a), keys(b), "{kind:?} node and link keys");
        }
        let mut a = mirror.sim.network(NetworkKind::Road).clone();
        let mut b = fresh.network(NetworkKind::Road).clone();
        let houses: Vec<NodeKey> = b
            .destinations()
            .map(|id| b.node(id).expect("listed").key().clone())
            .collect();
        let mut connected = 0;
        for x in &houses {
            for y in &houses {
                if x == y {
                    continue;
                }
                let id = |n: &Network, k: &NodeKey| n.node_id(k).expect("same keys");
                let (ax, ay, bx, by) = (id(&a, x), id(&a, y), id(&b, x), id(&b, y));
                let routed = b.route(bx, by).is_some();
                assert_eq!(a.route(ax, ay).is_some(), routed, "route {x:?} -> {y:?}");
                connected += usize::from(routed);
            }
        }
        connected
    }

    const ROADS: [&str; 9] = [
        "rail_straight",
        "rail_bend_60",
        "rail_bend_120",
        "rail_junction",
        "road_straight",
        "road_bend_60",
        "road_bend_120",
        "road_junction",
        "house_cluster",
    ];

    /// One random game action: mostly placements (biased to roads and houses, at a random
    /// frontier slot and legal rotation), plus bulldozes, undos and redos.
    fn random_action(g: &mut GameState, rng: &mut Rng) {
        match rng.below(10) {
            0..=5 => {
                let frontier = g.world.frontier();
                let cell = frontier[rng.below(frontier.len())];
                let id = if rng.below(2) == 0 {
                    ROADS[rng.below(ROADS.len())].to_owned()
                } else {
                    let tiles = g.catalog.tiles();
                    tiles[rng.below(tiles.len())].id.clone()
                };
                g.apply(Action::SelectTile(id));
                g.apply(Action::Hover(Some(cell)));
                for _ in 0..rng.below(6) {
                    g.apply(Action::Rotate(RotateDir::Forward));
                }
                g.apply(Action::Place(cell));
            }
            6 | 7 => {
                let cells: Vec<Axial> = g.world.tiles().keys().copied().collect();
                g.apply(Action::Bulldoze(cells[rng.below(cells.len())]));
            }
            8 => {
                g.apply(Action::Undo);
            }
            _ => {
                g.apply(Action::Redo);
            }
        }
    }

    #[test]
    fn mirror_matches_from_world_after_random_edits() {
        let (mut undos, mut bulldozes, mut max_pairs, mut cars) = (0, 0, 0, 0);
        for seed in 0..24u64 {
            let mut g = GameState::new(seed);
            let mut sim = CarSim::new(&g.world, &g.catalog);
            let mut rng = Rng(seed * 7919 + 1);
            for _ in 0..160 {
                random_action(&mut g, &mut rng);
                match g.recent.back() {
                    Some(crate::game::GameEvent::Undone) => undos += 1,
                    Some(crate::game::GameEvent::TileBulldozed { .. }) => bulldozes += 1,
                    _ => {}
                }
                sim.mirror(&g.world, &g.catalog);
                // Cars alive while the world changes exercise the despawn-on-edit path too.
                for _ in 0..rng.below(40) {
                    sim.tick();
                }
                cars = cars.max(sim.stats.cars);
                max_pairs = max_pairs.max(assert_matches_world(&sim, &g.world, &g.catalog));
            }
        }
        assert!(
            undos > 50 && bulldozes > 50,
            "undos {undos} bulldozes {bulldozes}"
        );
        assert!(max_pairs > 0, "no sequence connected two houses");
        assert!(cars > 0, "no car ever spawned");
    }

    #[test]
    fn mirror_follows_a_wholesale_world_replacement() {
        let mut g = GameState::new(3);
        let mut rng = Rng(11);
        for _ in 0..80 {
            random_action(&mut g, &mut rng);
        }
        let mut sim = CarSim::new(&g.world, &g.catalog);
        g.world = World::new(3);
        assert!(sim.mirror(&g.world, &g.catalog));
        assert_matches_world(&sim, &g.world, &g.catalog);
        assert!(!sim.mirror(&g.world, &g.catalog), "nothing left to mirror");
    }

    /// The agents gate: a placed tile joins the sim only once it is ready (its rise reached the
    /// agents stage); tiles already mirrored stay, and a removal applies while others are held.
    #[test]
    fn gated_mirror_holds_tiles_until_ready() {
        let json = include_str!("../../../docs/checkpoints/cars/cars.json");
        let mut g = GameState::new(1);
        let mut sim = CarSim::new(&g.world, &g.catalog);
        let mut cmds = parse_script(json).unwrap().into_iter();
        for c in cmds.by_ref() {
            if matches!(&c, Command::DumpState { name } if name == "cars-01-connected") {
                break;
            }
            if let Some(a) = c.action() {
                g.apply(a);
            }
        }
        assert!(!sim.mirror_gated(&g.world, &g.catalog, |_| false));
        assert_eq!(sim.stats.connected_destinations, 0, "every new tile held");
        assert!(sim.mirror_gated(&g.world, &g.catalog, |_| true));
        assert_eq!(sim.stats.connected_destinations, 2);
        assert!(
            !sim.mirror_gated(&g.world, &g.catalog, |_| false),
            "mirrored tiles stay"
        );
        // The filler meadow is held while the bulldoze of the middle road applies at once.
        g.apply(Action::SelectTile("meadow".into()));
        g.apply(Action::Place(Axial::new(1, 0)));
        g.apply(Action::Bulldoze(Axial::new(0, 1)));
        assert!(g.world.tile(Axial::new(0, 1)).is_none());
        assert!(sim.mirror_gated(&g.world, &g.catalog, |c| c != Axial::new(1, 0)));
        assert_eq!(sim.stats.connected_destinations, 0);
        assert!(!sim.mirrored.contains_key(&Axial::new(1, 0)));
    }

    /// The trip events of the cars proof layout over two simulated minutes: every pull-out is at
    /// the car's spot as it was parked, every park at the spot the car was driving to, a car parks
    /// only after it pulled out, and the driving count is pull-outs minus parks throughout.
    #[test]
    fn ticks_report_each_pull_out_and_park_at_the_spot() {
        let json = include_str!("../../../docs/checkpoints/cars/cars.json");
        let mut g = GameState::new(1);
        let mut sim = CarSim::new(&g.world, &g.catalog);
        for c in parse_script(json).unwrap() {
            if matches!(&c, Command::DumpState { name } if name == "cars-01-connected") {
                break;
            }
            if let Some(a) = c.action() {
                g.apply(a);
            }
        }
        sim.mirror(&g.world, &g.catalog);
        let at = |cell: Axial, x: f32, z: f32, spots: &[(Axial, [f32; 2])]| {
            spots
                .iter()
                .any(|(c, p)| *c == cell && (p[0] - x).abs() < 0.01 && (p[1] - z).abs() < 0.01)
        };
        let (mut out, mut parked, mut away) = (0, 0, BTreeSet::new());
        for _ in 0..(120 * hex_sim::TICK_HZ) {
            let spots: Vec<(Axial, [f32; 2])> = sim
                .sim
                .parked()
                .iter()
                .map(|c| (c.spot().cell, c.pose().pos))
                .chain(sim.sim.cars().iter().map(|c| {
                    let p = sim.sim.spot_pose(c.to_spot()).unwrap().pos;
                    (c.to_spot().cell, p)
                }))
                .collect();
            for e in sim.tick() {
                match e {
                    GameEvent::CarPulledOut { id, q, r, x, z } => {
                        assert!(at(Axial::new(q, r), x, z, &spots), "{e:?} not at a spot");
                        assert!(away.insert(id), "{id} pulled out twice without parking");
                        out += 1;
                    }
                    GameEvent::CarParked { id, q, r, x, z } => {
                        assert!(at(Axial::new(q, r), x, z, &spots), "{e:?} not at its spot");
                        assert!(away.remove(&id), "{id} parked without pulling out");
                        parked += 1;
                    }
                    other => panic!("not a car event: {other:?}"),
                }
            }
            assert_eq!(sim.stats.cars, out - parked);
        }
        assert!(out >= 3 && parked >= 2, "{out} pull-outs, {parked} parks");
    }

    /// Replays the cars proof script (docs/checkpoints/cars/cars.json) headlessly with one tick
    /// every third frame and the ungated mirror. A native `--clock fixed` run gates each new tile
    /// on its staged rise (`mirror_gated`), so its dumps taken right after a placement
    /// (`cars-01-connected`, `cars-05-undo`) report fewer connected houses than this replay.
    #[test]
    fn checkpoint_sequence() {
        let json = include_str!("../../../docs/checkpoints/cars/cars.json");
        let mut g = GameState::new(1);
        let mut sim = CarSim::new(&g.world, &g.catalog);
        let mut frame = 0u32;
        let mut dumps: BTreeMap<String, SimStats> = BTreeMap::new();
        let step = |sim: &mut CarSim, frames: u32, frame: &mut u32| {
            for _ in 0..frames {
                *frame += 1;
                if frame.is_multiple_of(3) {
                    sim.tick();
                }
            }
        };
        for c in parse_script(json).unwrap() {
            step(&mut sim, 1, &mut frame);
            if let Some(a) = c.action() {
                g.apply(a);
                sim.mirror(&g.world, &g.catalog);
            }
            match c {
                Command::Wait { frames } => step(&mut sim, frames, &mut frame),
                Command::DumpState { name } => {
                    dumps.insert(name, sim.stats.clone());
                }
                _ => {}
            }
        }
        let rate = SimConfig::default().spawn_per_destination_hz * 2.0;
        let driving = &dumps["cars-02-driving"];
        assert_eq!(driving.connected_destinations, 2);
        assert_eq!(driving.spawn_rate_hz, rate);
        assert!(driving.cars >= 1, "{driving:?}");
        let refused = &dumps["cars-03-bulldoze-refused"];
        assert_eq!(refused.connected_destinations, 2, "the middle road stays");
        let cut = &dumps["cars-04-disconnected"];
        assert_eq!(
            (cut.cars, cut.connected_destinations, cut.spawn_rate_hz),
            (0, 0, 0.0),
            "every car routed over the bulldozed road is gone: {cut:?}"
        );
        assert_eq!(cut.parked_cars, 4, "each house's car is back in its spot");
        let undone = &dumps["cars-05-undo"];
        assert_eq!(undone.connected_destinations, 2);
        assert_eq!(undone.spawn_rate_hz, rate);
    }

    /// A shuttle through a station: every station departure (`Railway::departures`, the dwell
    /// over) is a `TrainDeparted` on that tick, and the train moving off on the next tick is not a
    /// second one; line-end departures still come from the train moving off.
    #[test]
    fn station_departures_whistle_once() {
        let c = TileCatalog::builtin().unwrap();
        let mut w = World::new(1);
        w.place(&c, Axial::new(0, -1), "rail_station", 0).unwrap();
        for q in [1, 2, -1, -2] {
            w.place(&c, Axial::new(q, -1), "rail_straight", 0).unwrap();
        }
        let mut sim = CarSim::new(&w, &c);
        let (mut at_station, mut other) = (0, 0);
        let mut last: BTreeMap<u64, u64> = BTreeMap::new();
        for tick in 0..300 * u64::from(hex_sim::TICK_HZ) {
            let events = sim.tick();
            let stations: BTreeSet<u64> = sim
                .sim
                .rails()
                .departures()
                .iter()
                .map(|(id, _)| id.0)
                .collect();
            let departed: Vec<u64> = events
                .iter()
                .filter_map(|e| match e {
                    GameEvent::TrainDeparted { id, .. } => Some(*id),
                    _ => None,
                })
                .collect();
            for id in &stations {
                assert!(
                    departed.contains(id),
                    "tick {tick}: train {id} left the station quietly"
                );
            }
            for id in departed {
                if let Some(t) = last.insert(id, tick) {
                    assert!(
                        tick - t > 1,
                        "tick {tick}: train {id} whistled on two ticks running"
                    );
                }
                if stations.contains(&id) {
                    at_station += 1;
                } else {
                    other += 1;
                }
            }
        }
        assert!(at_station >= 2, "{at_station} station departures");
        assert!(other >= 2, "{other} other departures (spawn, line ends)");
    }

    /// The two rail evidence worlds of docs/checkpoints/rail/rail-play.json, each replayed
    /// headlessly up to the first camera move after its placements. Returns the loop world and the
    /// Y world.
    fn rail_checkpoint_worlds() -> (GameState, GameState) {
        let json = include_str!("../../../docs/checkpoints/rail/rail-play.json");
        let (mut worlds, mut g) = (vec![], GameState::new(1));
        let mut building = true;
        for c in parse_script(json).unwrap() {
            match c {
                Command::NewWorld => {
                    g = GameState::new(1);
                    building = true;
                }
                Command::Camera { .. } if building && g.world.tiles().len() > 1 => {
                    worlds.push(std::mem::replace(&mut g, GameState::new(1)));
                    building = false;
                }
                c => {
                    if let Some(a) = c.action().filter(|_| building) {
                        g.apply(a);
                    }
                }
            }
        }
        assert_eq!(worlds.len(), 2);
        let y = worlds.pop().unwrap();
        (worlds.pop().unwrap(), y)
    }

    fn rot(g: &GameState, q: i32, r: i32) -> Option<(String, u8)> {
        g.world
            .tile(Axial::new(q, r))
            .map(|t| (t.def.to_string(), t.rotation))
    }

    /// The loop world: six straights and three bends close a 216 m triangle round the start meadow,
    /// and its two trains lap it the same way round from the start, never running back.
    #[test]
    fn rail_checkpoint_loop_runs_two_trains_the_same_way_round() {
        let (g, _) = rail_checkpoint_worlds();
        assert_eq!(g.world.tiles().len(), 10);
        assert_eq!(rot(&g, -1, -1), Some(("rail_bend_60".into(), 0)));
        let mut sim = CarSim::new(&g.world, &g.catalog);
        for _ in 0..180 * hex_sim::TICK_HZ {
            sim.tick();
        }
        let trains = sim.sim.rails().trains();
        assert_eq!(trains.len(), 2, "{:?}", sim.stats.train_list);
        for t in trains {
            assert_eq!(t.reversals(), 0, "{:?} ran back on the loop", t.id());
            assert!(t.speed() > 5.0, "{:?} at {} m/s", t.id(), t.speed());
        }
    }

    /// The Y world: every placement lands at the rotation the layout needs (auto-rotate has one
    /// legal orientation for each), and over ten simulated minutes its two trains take both legs of
    /// the turnout as facing moves, the branch train closes the level crossing, and a car on the
    /// crossing road holds short of it while it is closed.
    #[test]
    fn rail_checkpoint_y_runs_trains_both_ways_and_holds_cars_at_the_crossing() {
        let (_, g) = rail_checkpoint_worlds();
        assert_eq!(g.world.tiles().len(), 13);
        assert_eq!(rot(&g, 0, -1), Some(("rail_junction".into(), 3)));
        assert!(matches!(rot(&g, 0, -3), Some((d, 1 | 4)) if d == "rail_road_crossing"));
        assert!(matches!(rot(&g, 0, -2), Some((d, 1 | 4)) if d == "rail_straight"));
        for (q, r) in [(-1, -2), (1, -4)] {
            assert!(matches!(rot(&g, q, r), Some((d, _)) if d == "house_cluster"));
        }
        let mut sim = CarSim::new(&g.world, &g.catalog);
        let (mut closed, mut held) = (0, 0);
        let mut departed: BTreeMap<u64, u32> = BTreeMap::new();
        for _ in 0..600 * hex_sim::TICK_HZ {
            let before: BTreeMap<u64, f32> = sim
                .sim
                .cars()
                .iter()
                .map(|c| (c.id().0, c.distance()))
                .collect();
            for e in sim.tick() {
                if let GameEvent::TrainDeparted { id, x, z } = e {
                    let t = sim.sim.rails().trains().iter().find(|t| t.id().0 == id);
                    let front = t.expect("a departing train").front();
                    assert!((front[0] - x).abs() < 0.01 && (front[1] - z).abs() < 0.01);
                    *departed.entry(id).or_insert(0) += 1;
                }
            }
            let shut = !sim.stats.closed_crossings.is_empty();
            closed += usize::from(shut);
            for car in sim.sim.cars() {
                let still = before.get(&car.id().0) == Some(&car.distance());
                if shut
                    && still
                    && car
                        .crossings()
                        .iter()
                        .any(|&(_, stop)| (car.distance() - stop).abs() < 1e-3)
                {
                    held += 1;
                }
            }
        }
        assert_eq!(sim.stats.train_list.len(), 2, "{:?}", sim.stats.train_list);
        let j = &sim.sim.rails().junctions()[0];
        let facing: BTreeSet<&str> = j.facing().iter().map(|l| l.as_str()).collect();
        assert_eq!(facing, BTreeSet::from(["branch", "straight"]), "{facing:?}");
        assert!(closed > 0, "the crossing never closed");
        assert!(held > 0, "no car held short of the closed crossing");
        // Each train departs on its first move and again after every stand: a line end's dwell
        // comes round every lap of its shuttle, so ten minutes give each several departures.
        assert_eq!(departed.len(), 2, "{departed:?}");
        for (id, n) in &departed {
            let t = sim.sim.rails().trains().iter().find(|t| t.id().0 == *id);
            let reversals = t.unwrap().reversals();
            assert!(
                *n >= 3 && *n > reversals,
                "train {id}: {n} departures, {reversals} reversals"
            );
        }
    }

    /// The routing evidence world of docs/checkpoints/rail-routing/play.json, replayed headlessly:
    /// the turnout lands toe west with its branch north-west, the one train stands at each station
    /// in turn with that station as its destination, and the state JSON names each train's
    /// `destination` cell and `route` meters.
    #[test]
    fn rail_routing_checkpoint_stands_a_train_at_each_station_in_turn() {
        let json = include_str!("../../../docs/checkpoints/rail-routing/play.json");
        let mut g = GameState::new(1);
        for c in parse_script(json).unwrap() {
            if matches!(c, Command::Camera { .. }) {
                break;
            }
            if let Some(a) = c.action() {
                g.apply(a);
            }
        }
        assert_eq!(g.world.tiles().len(), 6);
        assert_eq!(rot(&g, 0, -1), Some(("rail_junction".into(), 3)));
        assert!(matches!(rot(&g, 1, -1), Some((d, 0 | 3)) if d == "rail_station"));
        assert!(matches!(rot(&g, 0, -2), Some((d, 1 | 4)) if d == "rail_station"));
        let mut sim = CarSim::new(&g.world, &g.catalog);
        let stations = [Axial::new(1, -1), Axial::new(0, -2)];
        let mut stood: Vec<Axial> = vec![];
        let mut keys_checked = false;
        for _ in 0..400 * hex_sim::TICK_HZ {
            sim.tick();
            assert_eq!(sim.stats.train_list.len(), 1, "{:?}", sim.stats.train_list);
            let t = &sim.stats.train_list[0];
            if t.waiting == "station" {
                let cell = t.destination.expect("a destination");
                assert!(stations.contains(&cell), "{t:?}");
                if stood.last() != Some(&cell) {
                    stood.push(cell);
                }
            }
            if !keys_checked && t.route.is_some() {
                let mut play = json!({});
                sim.extend_state(&mut play);
                let s = &play["train_list"][0];
                assert!(s["destination"]["q"].is_i64() && s["route"].is_f64(), "{s}");
                keys_checked = true;
            }
        }
        assert!(keys_checked);
        assert!(stood.len() >= 4, "{stood:?}");
        for w in stood.windows(2) {
            assert_ne!(w[0], w[1], "{stood:?}");
        }
    }
}
