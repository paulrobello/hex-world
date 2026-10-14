//! The fixed-tick car simulation (PRD §7.2): seeded spawning between connected destinations,
//! A* routes and lane following, deterministic for a given seed and sequence of world edits.

use hex_core::{Axial, NetworkKind, TileCatalog, TileDef, World};

use crate::geom::{LanePath, Pose};
use crate::network::{LinkId, Network, NodeId};

/// Simulation ticks per second (PRD §7.2), independent of the render frame rate.
pub const TICK_HZ: u32 = 20;

/// Simulated seconds per tick.
pub const TICK_SECONDS: f32 = 1.0 / TICK_HZ as f32;

/// Vehicle cap at the High quality tier (PRD §7.2 reference target).
pub const HIGH_TIER_CAR_CAP: usize = 300;

/// Largest distance a car drives right of the road centerline: the middle of the right half of the
/// 6 m road lane (plan F3).
pub const MAX_LANE_OFFSET: f32 = 1.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SimConfig {
    /// Most cars alive at once.
    pub max_cars: usize,
    /// Cars spawned per second for each connected destination.
    pub spawn_per_destination_hz: f32,
    /// Driving speed in m/s.
    pub car_speed: f32,
    /// Right-hand distance from the road centerline in meters, clamped to [`MAX_LANE_OFFSET`].
    pub lane_offset: f32,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            max_cars: HIGH_TIER_CAR_CAP,
            spawn_per_destination_hz: 0.05,
            car_speed: 8.0,
            lane_offset: MAX_LANE_OFFSET,
        }
    }
}

/// Car id; ids count up from 0 in spawn order and are never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CarId(pub u64);

/// A car driving its lane from one destination to another.
#[derive(Clone, Debug, PartialEq)]
pub struct Car {
    id: CarId,
    origin: NodeId,
    destination: NodeId,
    links: Vec<LinkId>,
    path: LanePath,
    /// Distance along `path` at the previous tick.
    prev: f32,
    /// Distance along `path` now.
    s: f32,
}

impl Car {
    pub fn id(&self) -> CarId {
        self.id
    }

    pub fn origin(&self) -> NodeId {
        self.origin
    }

    pub fn destination(&self) -> NodeId {
        self.destination
    }

    /// Road links of the route, in travel order.
    pub fn links(&self) -> &[LinkId] {
        &self.links
    }

    /// The lane the car follows, already offset to the right of the centerline.
    pub fn path(&self) -> &LanePath {
        &self.path
    }

    /// Meters driven along the lane path.
    pub fn distance(&self) -> f32 {
        self.s
    }

    /// The pose between the previous tick (`alpha = 0`) and the latest one (`alpha = 1`), for
    /// render interpolation. Interpolates distance along the lane, so it follows curves.
    pub fn pose_at(&self, alpha: f32) -> Pose {
        let a = alpha.clamp(0.0, 1.0);
        self.path
            .pose_at_distance(self.prev + (self.s - self.prev) * a)
    }

    pub fn arrived(&self) -> bool {
        self.s >= self.path.length()
    }
}

/// SplitMix64: small, fast and stable across releases, so seeds replay identically.
#[derive(Clone, Debug)]
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n`; `n` must be positive.
    fn below(&mut self, n: usize) -> usize {
        ((u128::from(self.next_u64()) * n as u128) >> 64) as usize
    }
}

/// Road and river graphs plus the cars on the roads.
#[derive(Clone, Debug)]
pub struct Simulation {
    config: SimConfig,
    rng: Rng,
    roads: Network,
    rivers: Network,
    cars: Vec<Car>,
    next_car: u64,
    spawn_credit: f32,
    ticks: u64,
}

impl Simulation {
    pub fn new(seed: u64, config: SimConfig) -> Self {
        Self {
            config: SimConfig {
                lane_offset: config.lane_offset.clamp(0.0, MAX_LANE_OFFSET),
                ..config
            },
            rng: Rng(seed),
            roads: Network::new(NetworkKind::Road),
            rivers: Network::new(NetworkKind::River),
            cars: vec![],
            next_car: 0,
            spawn_credit: 0.0,
            ticks: 0,
        }
    }

    /// A simulation of every tile in `world`, in cell order, seeded with the world seed. Tiles
    /// whose definition is missing from `catalog` are skipped.
    pub fn from_world(world: &World, catalog: &TileCatalog, config: SimConfig) -> Self {
        let mut sim = Self::new(world.seed(), config);
        for (cell, tile) in world.tiles() {
            if let Some(def) = catalog.get(&tile.def) {
                sim.apply_place(*cell, def, tile.rotation);
            }
        }
        sim
    }

    pub fn config(&self) -> &SimConfig {
        &self.config
    }

    /// Mirrors a tile placed at `cell`. Replacing a tile despawns the cars routed over it.
    pub fn apply_place(&mut self, cell: Axial, def: &TileDef, rotation: u8) {
        self.drop_cars_on(cell);
        self.roads.insert_tile(cell, def, rotation);
        self.rivers.insert_tile(cell, def, rotation);
    }

    /// Mirrors a bulldozed tile, despawning every car whose route crosses it.
    pub fn apply_bulldoze(&mut self, cell: Axial) {
        self.drop_cars_on(cell);
        self.roads.remove_tile(cell);
        self.rivers.remove_tile(cell);
    }

    /// Advances one fixed tick of [`TICK_SECONDS`]: moves every car, despawns the cars that had
    /// already arrived (so the final approach still renders), then spawns new ones.
    pub fn tick(&mut self) {
        self.ticks += 1;
        let step = self.config.car_speed * TICK_SECONDS;
        for car in &mut self.cars {
            car.prev = car.s;
            car.s = (car.s + step).min(car.path.length());
        }
        self.cars.retain(|c| c.prev < c.path.length());
        self.spawn();
    }

    /// Ticks run so far.
    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// Live cars in spawn order.
    pub fn cars(&self) -> &[Car] {
        &self.cars
    }

    /// Cars spawned so far, including despawned ones.
    pub fn spawned(&self) -> u64 {
        self.next_car
    }

    pub fn network(&self, kind: NetworkKind) -> &Network {
        match kind {
            NetworkKind::Road => &self.roads,
            NetworkKind::River => &self.rivers,
        }
    }

    /// Destinations that can reach at least one other destination by road.
    pub fn connected_destinations(&mut self) -> usize {
        self.roads.components().connected_destinations().len()
    }

    /// Cars spawned per second while under the cap.
    pub fn spawn_rate_hz(&mut self) -> f32 {
        self.connected_destinations() as f32 * self.config.spawn_per_destination_hz
    }

    fn drop_cars_on(&mut self, cell: Axial) {
        if self.roads.links_of(cell).next().is_some() {
            self.cars
                .retain(|c| !c.links.iter().any(|link| link.cell == cell));
        }
    }

    fn spawn(&mut self) {
        let rate = self.spawn_rate_hz();
        if rate <= 0.0 {
            self.spawn_credit = 0.0;
            return;
        }
        self.spawn_credit += rate * TICK_SECONDS;
        while self.spawn_credit >= 1.0 && self.cars.len() < self.config.max_cars {
            self.spawn_credit -= 1.0;
            self.spawn_one();
        }
        self.spawn_credit = self.spawn_credit.min(1.0);
    }

    /// Spawns one car between a random pair of connected destinations.
    fn spawn_one(&mut self) {
        let (origin, destination) = {
            let comps = self.roads.components();
            let starts = comps.connected_destinations();
            if starts.is_empty() {
                return;
            }
            let origin = starts[self.rng.below(starts.len())];
            let Some(comp) = comps.of(origin) else {
                return;
            };
            let group = comps.destinations(comp);
            let Ok(skip) = group.binary_search(&origin) else {
                return;
            };
            let pick = self.rng.below(group.len() - 1);
            (origin, group[if pick >= skip { pick + 1 } else { pick }])
        };
        let Some(route) = self.roads.route(origin, destination) else {
            return;
        };
        let path = LanePath::new(&route.centerline(&self.roads), self.config.lane_offset);
        self.cars.push(Car {
            id: CarId(self.next_car),
            origin,
            destination,
            links: route.links.clone(),
            path,
            prev: 0.0,
            s: 0.0,
        });
        self.next_car += 1;
    }
}
