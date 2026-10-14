//! Cars in the play scenario (PRD §7.2): a [`hex_sim::Simulation`] mirrored from the game world,
//! ticked at a fixed 20 Hz in `FixedUpdate` and drawn by `hex_render::cars` at the poses
//! interpolated with the fixed clock's overstep.
use std::collections::BTreeMap;

use bevy::prelude::*;
use hex_core::{Axial, PlacedTile, TileCatalog, World};
use hex_render::{
    ActiveScenario, Scenario,
    cars::{CarPose, CarsView},
    play::{PlaySystems, rise::PlayRises},
};
use hex_sim::{SimConfig, Simulation, TICK_HZ};
use serde_json::{Value, json};

use crate::game::GameState;

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
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SimStats {
    pub cars: usize,
    pub connected_destinations: usize,
    pub spawn_rate_hz: f32,
}

#[derive(Resource)]
pub struct CarSim {
    sim: Simulation,
    /// The world's tiles as last mirrored into `sim`.
    mirrored: BTreeMap<Axial, PlacedTile>,
    stats: SimStats,
}

impl CarSim {
    /// A simulation of `world`, seeded with the world seed.
    pub fn new(world: &World, catalog: &TileCatalog) -> Self {
        let mut s = Self {
            sim: Simulation::from_world(world, catalog, SimConfig::default()),
            mirrored: world.tiles().clone(),
            stats: SimStats::default(),
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

    /// One fixed tick of `hex_sim::TICK_SECONDS`.
    pub fn tick(&mut self) {
        self.sim.tick();
        self.refresh();
    }

    fn refresh(&mut self) {
        self.stats = SimStats {
            cars: self.sim.cars().len(),
            connected_destinations: self.sim.connected_destinations(),
            spawn_rate_hz: self.sim.spawn_rate_hz(),
        };
    }

    /// Adds the car fields to the `play` object of the state JSON.
    pub fn extend_state(&self, play: &mut Value) {
        let Value::Object(map) = play else {
            return;
        };
        let rate = (f64::from(self.stats.spawn_rate_hz) * 1e6).round() / 1e6;
        map.insert("cars".into(), json!(self.stats.cars));
        map.insert(
            "connected_destinations".into(),
            json!(self.stats.connected_destinations),
        );
        map.insert("spawn_rate_hz".into(), json!(rate));
    }
}

/// Mirrors the world into the sim whenever the game state or a rise changed (after
/// `game::apply_actions` and the play systems). In the play scenario a placed tile waits for its
/// rise's agents stage ([`CarSim::mirror_gated`]).
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

/// The fixed tick; the sim runs only while the play scenario is shown.
pub fn tick(mut sim: ResMut<CarSim>, active: Res<ActiveScenario>) {
    if active.0 == Scenario::Play {
        sim.tick();
    }
}

/// Writes each car's pose between the last two ticks, at the fraction of a tick the fixed clock
/// has accumulated since the latest one.
pub fn publish_cars(sim: Res<CarSim>, fixed: Res<Time<Fixed>>, mut view: ResMut<CarsView>) {
    let cars = sim.sim.cars();
    if cars.is_empty() && view.cars.is_empty() {
        return;
    }
    let alpha = fixed.overstep_fraction();
    view.cars.clear();
    view.cars.extend(cars.iter().map(|c| {
        let pose = c.pose_at(alpha);
        CarPose {
            id: c.id().0,
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
        for kind in [NetworkKind::Road, NetworkKind::River] {
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

    const ROADS: [&str; 5] = [
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
                    dumps.insert(name, sim.stats);
                }
                _ => {}
            }
        }
        let rate = SimConfig::default().spawn_per_destination_hz * 2.0;
        let driving = dumps["cars-02-driving"];
        assert_eq!(driving.connected_destinations, 2);
        assert_eq!(driving.spawn_rate_hz, rate);
        assert!(driving.cars >= 1, "{driving:?}");
        let refused = dumps["cars-03-bulldoze-refused"];
        assert_eq!(refused.connected_destinations, 2, "the middle road stays");
        let cut = dumps["cars-04-disconnected"];
        assert_eq!(
            cut,
            SimStats {
                cars: 0,
                connected_destinations: 0,
                spawn_rate_hz: 0.0
            },
            "every car routed over the bulldozed road despawned"
        );
        let undone = dumps["cars-05-undo"];
        assert_eq!(undone.connected_destinations, 2);
        assert_eq!(undone.spawn_rate_hz, rate);
    }
}
