//! The fixed-tick car simulation (PRD §7.1, §7.2): every house cluster's houses own cars that wait
//! in its parking spots, pull out on seeded departures, follow A* routes on their lanes and park in
//! a free spot at another cluster or a shop. Shops own no cars; a visitor parks for
//! [`SimConfig::shop_stay_seconds`] and drives home. Deterministic for a given seed and sequence of
//! world edits.

use std::collections::BTreeMap;

use hex_core::{Axial, Destination, NetworkKind, TileCatalog, TileDef, World};

use crate::geom::{self, LanePath, Pose, Vec2};
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
    /// Most cars in the world, parked or driving.
    pub max_cars: usize,
    /// Departures per second for each connected destination.
    pub spawn_per_destination_hz: f32,
    /// Driving speed in m/s.
    pub car_speed: f32,
    /// Right-hand distance from the road centerline in meters, clamped to [`MAX_LANE_OFFSET`].
    pub lane_offset: f32,
    /// How long a visitor stays parked at a shop before it drives home, in seconds.
    pub shop_stay_seconds: f32,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            max_cars: HIGH_TIER_CAR_CAP,
            spawn_per_destination_hz: 0.05,
            car_speed: 8.0,
            lane_offset: MAX_LANE_OFFSET,
            shop_stay_seconds: 20.0,
        }
    }
}

/// Car id; ids count up from 0 in spawn order and are never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CarId(pub u64);

/// The house that owns a car: hero prop `house` of the cluster tile at `cell`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Home {
    pub cell: Axial,
    pub house: u16,
}

/// Parking spot `index` (in [`TileDef::parking`] order) of the house cluster or shop at `cell`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpotRef {
    pub cell: Axial,
    pub index: usize,
}

/// A car standing in a parking spot.
#[derive(Clone, Debug, PartialEq)]
pub struct ParkedCar {
    id: CarId,
    home: Home,
    spot: SpotRef,
    pose: Pose,
    /// The tick the car parked on.
    since: u64,
}

impl ParkedCar {
    /// The tick the car parked on.
    pub fn since(&self) -> u64 {
        self.since
    }

    pub fn id(&self) -> CarId {
        self.id
    }

    pub fn home(&self) -> Home {
        self.home
    }

    pub fn spot(&self) -> SpotRef {
        self.spot
    }

    /// The spot's pose: the car's center and the way its nose points.
    pub fn pose(&self) -> Pose {
        self.pose
    }
}

/// A car driving from one parking spot to another.
#[derive(Clone, Debug, PartialEq)]
pub struct Car {
    id: CarId,
    home: Home,
    from: SpotRef,
    to: SpotRef,
    origin: NodeId,
    destination: NodeId,
    links: Vec<LinkId>,
    /// The whole trip: out of the spot, along the lanes, into the spot.
    path: LanePath,
    /// The first `reverse` meters of `path` are driven backwards (the pull-out).
    reverse: f32,
    /// Where the trip leaves the origin's driveway for the road lanes, and where it rejoins the
    /// destination's driveway, in meters along `path`.
    lanes: (f32, f32),
    /// Distance along `path` at the previous tick.
    prev: f32,
    /// Distance along `path` now.
    s: f32,
    /// Level crossings on the trip: the crossing cell and the distance along `path` where the car
    /// holds while the crossing is closed (P3).
    crossings: Vec<(Axial, f32)>,
}

impl Car {
    pub fn id(&self) -> CarId {
        self.id
    }

    pub fn home(&self) -> Home {
        self.home
    }

    /// The spot the car pulled out of.
    pub fn from_spot(&self) -> SpotRef {
        self.from
    }

    /// The spot reserved for the car at its destination.
    pub fn to_spot(&self) -> SpotRef {
        self.to
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

    /// The trip path: the parking legs at both ends and the lanes (offset right of the road
    /// centerline) between them.
    pub fn path(&self) -> &LanePath {
        &self.path
    }

    /// Meters at the start of the path driven in reverse.
    pub fn reverse_length(&self) -> f32 {
        self.reverse
    }

    /// The stretch of the path on road lanes, in meters from its start; the rest is driveway.
    pub fn lane_span(&self) -> (f32, f32) {
        self.lanes
    }

    /// Meters driven along the path.
    pub fn distance(&self) -> f32 {
        self.s
    }

    /// The pose `d` meters along the trip, facing backwards on the reversing pull-out.
    pub fn pose_at_distance(&self, d: f32) -> Pose {
        let mut pose = self.path.pose_at_distance(d);
        if d < self.reverse {
            pose.dir = geom::scale(pose.dir, -1.0);
        }
        pose
    }

    /// The pose between the previous tick (`alpha = 0`) and the latest one (`alpha = 1`), for
    /// render interpolation. Interpolates distance along the path, so it follows curves.
    pub fn pose_at(&self, alpha: f32) -> Pose {
        let a = alpha.clamp(0.0, 1.0);
        self.pose_at_distance(self.prev + (self.s - self.prev) * a)
    }

    /// The level crossings on the trip: each crossing cell and the distance along the path where
    /// the car holds while it is closed.
    pub fn crossings(&self) -> &[(Axial, f32)] {
        &self.crossings
    }

    pub fn arrived(&self) -> bool {
        self.s >= self.path.length()
    }

    /// Which part of the trip the car is on now.
    pub fn phase(&self) -> CarPhase {
        if self.s < self.reverse {
            CarPhase::PullingOut
        } else if self.s < self.lanes.0 {
            CarPhase::Leaving
        } else if self.s <= self.lanes.1 {
            CarPhase::Road
        } else {
            CarPhase::Parking
        }
    }
}

/// The parts of a trip (PRD §7.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CarPhase {
    /// Reversing out of the spot.
    PullingOut,
    /// Driving forward from the turn onto the outbound lane.
    Leaving,
    /// On the road lanes.
    Road,
    /// Driving from the inbound lane into the reserved spot.
    Parking,
}

impl CarPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            CarPhase::PullingOut => "pulling_out",
            CarPhase::Leaving => "leaving",
            CarPhase::Road => "road",
            CarPhase::Parking => "parking",
        }
    }
}

/// The most a car's facing may turn in one tick on a parking leg, in degrees, with a margin under
/// the 20° rule (PRD §7.1) for render-time sampling.
const TURN_LIMIT_DEG: f32 = 18.0;

/// Spacing of the candidate points where a parking leg may join or leave the lane, in meters.
const JOIN_STEP: f32 = 0.5;

/// Farthest a parking leg may join or leave the lane from the node, in meters, so the leg stays
/// on the tile.
const JOIN_REACH: f32 = 10.0;

/// Meters of lane scored with a parking leg, so the turn where they meet counts too.
const JOIN_TAIL: f32 = 2.0;

/// The first candidate whose leg turns at most [`TURN_LIMIT_DEG`] per tick, else the one that
/// turns least: `(turn, candidate, leg)`. `leg` scores a candidate as `(turn, leg)`.
fn pick_join(
    candidates: impl Iterator<Item = f32>,
    mut leg: impl FnMut(f32) -> (f32, Vec<Vec2>),
) -> Option<(f32, f32, Vec<Vec2>)> {
    let mut best: Option<(f32, f32, Vec<Vec2>)> = None;
    for c in candidates {
        let (turn, pts) = leg(c);
        if best.as_ref().is_none_or(|b| turn < b.0) {
            best = Some((turn, c, pts));
        }
        if turn <= TURN_LIMIT_DEG {
            break;
        }
    }
    best
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

/// One parking spot in world space.
#[derive(Clone, Debug)]
struct Spot {
    house: u16,
    pose: Pose,
    enter: Vec<Pose>,
    back: Vec<Pose>,
    back_alt: Vec<Pose>,
    leave: Vec<Pose>,
    node: String,
    /// The car parked here or driving here.
    taken: Option<CarId>,
}

/// A placed tile with parking: its spots and its car cap (0 at a shop, which owns no cars).
#[derive(Clone, Debug)]
struct Cluster {
    spots: Vec<Spot>,
    cap: usize,
    shop: bool,
}

impl Cluster {
    fn new(cell: Axial, def: &TileDef, rotation: u8) -> Self {
        let center = geom::cell_center(cell);
        let turn = 60.0 * f32::from(rotation % 6);
        let pose = |p: hex_core::ParkPose| Pose {
            pos: geom::add(center, geom::rotate(p.at, rotation)),
            dir: geom::heading(p.yaw_deg + turn),
        };
        let spots = def
            .parking
            .iter()
            .map(|s| Spot {
                house: s.house,
                pose: pose(s.pose()),
                enter: s.enter.iter().copied().map(pose).collect(),
                back: s.back.iter().copied().map(pose).collect(),
                back_alt: s.back_alt.iter().copied().map(pose).collect(),
                leave: s.leave.iter().copied().map(pose).collect(),
                node: s.node.clone(),
                taken: None,
            })
            .collect();
        Self {
            spots,
            cap: def.car_cap(),
            shop: def.parking_destination() == Some(Destination::Shop),
        }
    }

    /// The owner's own spot when free, else the lowest free spot.
    fn free_spot(&self, house: Option<u16>) -> Option<usize> {
        let free = |i: &usize| self.spots[*i].taken.is_none();
        house
            .and_then(|h| (0..self.spots.len()).find(|&i| self.spots[i].house == h))
            .filter(free)
            .or_else(|| (0..self.spots.len()).find(free))
    }
}

/// Road and river graphs, the parking clusters and the cars.
#[derive(Clone, Debug)]
pub struct Simulation {
    config: SimConfig,
    rng: Rng,
    roads: Network,
    rivers: Network,
    rails: crate::rail::Railway,
    clusters: BTreeMap<Axial, Cluster>,
    /// Driving cars in id order.
    cars: Vec<Car>,
    /// Parked cars in id order.
    parked: Vec<ParkedCar>,
    next_car: u64,
    spawn_credit: f32,
    departures: u64,
    refused: u64,
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
            rails: crate::rail::Railway::with_seed(seed),
            clusters: BTreeMap::new(),
            cars: vec![],
            parked: vec![],
            next_car: 0,
            spawn_credit: 0.0,
            departures: 0,
            refused: 0,
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

    /// Mirrors a tile placed at `cell`. Replacing a tile removes the cars routed over it and the
    /// cars of the cluster it replaces. A tile with parking gets its owned cars, parked.
    pub fn apply_place(&mut self, cell: Axial, def: &TileDef, rotation: u8) {
        self.remove_cluster(cell);
        self.drop_cars_on(cell);
        self.roads.insert_tile(cell, def, rotation);
        self.rivers.insert_tile(cell, def, rotation);
        self.rails.insert_tile(cell, def, rotation);
        if !def.parking.is_empty() {
            self.clusters
                .insert(cell, Cluster::new(cell, def, rotation));
        }
        self.refill();
    }

    /// Mirrors a bulldozed tile: removes every car routed over it, parked in it or owned by its
    /// houses. Clusters that lost a car that way get one back in a free home spot.
    pub fn apply_bulldoze(&mut self, cell: Axial) {
        self.remove_cluster(cell);
        self.drop_cars_on(cell);
        self.roads.remove_tile(cell);
        self.rivers.remove_tile(cell);
        self.rails.remove_tile(cell);
        self.refill();
    }

    /// Advances one fixed tick of [`TICK_SECONDS`]: moves the trains (closing the level crossings
    /// they near), then every driving car, holding short of a closed crossing; parks the cars that
    /// had already arrived (so the final approach still renders), then starts departures.
    pub fn tick(&mut self) {
        self.ticks += 1;
        self.rails.tick();
        let step = self.config.car_speed * TICK_SECONDS;
        let closed = self.rails.closed_crossings();
        let mut arrived = vec![];
        self.cars.retain_mut(|car| {
            if car.s >= car.path.length() {
                arrived.push((car.id, car.home, car.to));
                return false;
            }
            car.prev = car.s;
            let mut next = (car.s + step).min(car.path.length());
            for &(cell, stop) in &car.crossings {
                if closed.contains(&cell) && car.s <= stop + 1e-3 && next > stop {
                    next = stop.max(car.s);
                }
            }
            car.s = next;
            true
        });
        for (id, home, spot) in arrived {
            let pose = self
                .spot(spot)
                .expect("reserved spots outlive their cars")
                .pose;
            self.park(ParkedCar {
                id,
                home,
                spot,
                pose,
                since: self.ticks,
            });
        }
        self.send_visitors_home();
        self.spawn();
    }

    /// Ticks run so far.
    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// Driving cars in id order.
    pub fn cars(&self) -> &[Car] {
        &self.cars
    }

    /// Parked cars in id order.
    pub fn parked(&self) -> &[ParkedCar] {
        &self.parked
    }

    /// Cars in the world, parked or driving.
    pub fn car_count(&self) -> usize {
        self.cars.len() + self.parked.len()
    }

    /// Cars created so far, including removed ones.
    pub fn spawned(&self) -> u64 {
        self.next_car
    }

    /// Trips started so far.
    pub fn departures(&self) -> u64 {
        self.departures
    }

    /// Departures refused because no other connected cluster had a free spot.
    pub fn refused(&self) -> u64 {
        self.refused
    }

    /// The rail network, its junctions, crossings and trains (P3).
    pub fn rails(&self) -> &crate::rail::Railway {
        &self.rails
    }

    pub fn network(&self, kind: NetworkKind) -> &Network {
        match kind {
            NetworkKind::Road => &self.roads,
            NetworkKind::River => &self.rivers,
            NetworkKind::Rail => self.rails.network(),
        }
    }

    /// The pose of a parking spot, if its cluster is placed.
    pub fn spot_pose(&self, spot: SpotRef) -> Option<Pose> {
        self.spot(spot).map(|s| s.pose)
    }

    /// Parking spots of the cluster at `cell`, and which car holds each (parked there or driving
    /// there). Empty when `cell` has no cluster.
    pub fn spots(&self, cell: Axial) -> Vec<(SpotRef, Option<CarId>)> {
        self.clusters.get(&cell).map_or(vec![], |c| {
            c.spots
                .iter()
                .enumerate()
                .map(|(index, s)| (SpotRef { cell, index }, s.taken))
                .collect()
        })
    }

    /// Cars owned by the houses of the cluster at `cell`, parked or driving.
    pub fn owned_by(&self, cell: Axial) -> usize {
        self.cars.iter().filter(|c| c.home.cell == cell).count()
            + self.parked.iter().filter(|c| c.home.cell == cell).count()
    }

    /// Destinations that can reach at least one other destination by road.
    pub fn connected_destinations(&mut self) -> usize {
        self.roads.components().connected_destinations().len()
    }

    /// Departures per second.
    pub fn spawn_rate_hz(&mut self) -> f32 {
        self.connected_destinations() as f32 * self.config.spawn_per_destination_hz
    }

    /// Sends the parked car `id` to a free spot at the cluster at `to`, or, when that cluster is
    /// full or unreachable, to the first other cluster with a free spot that is connected to the
    /// car's cluster, scanning clusters in cell order from `to`. Returns the reserved spot, or
    /// `None` when the car is not parked or no connected cluster has room (the car stays).
    pub fn send(&mut self, id: CarId, to: Axial) -> Option<SpotRef> {
        let car = self.parked.iter().find(|c| c.id == id)?.clone();
        let here = self.node_of(car.spot.cell)?;
        let mut order: Vec<(Axial, Option<NodeId>)> = self
            .clusters
            .keys()
            .map(|&c| (c, self.node_of(c)))
            .collect();
        let start = order.iter().position(|(c, _)| *c >= to).unwrap_or(0);
        order.rotate_left(start);
        let comps = self.roads.components();
        let reachable: Vec<Axial> = order
            .into_iter()
            .filter(|(c, n)| *c != car.spot.cell && n.is_some_and(|n| comps.connected(here, n)))
            .map(|(c, _)| c)
            .collect();
        let pick = reachable
            .into_iter()
            .find(|cell| self.clusters[cell].free_spot(None).is_some())?;
        self.depart(car, pick)
    }

    /// The road node of the cluster at `cell`.
    fn node_of(&self, cell: Axial) -> Option<NodeId> {
        let spot = self.clusters.get(&cell)?.spots.first()?;
        self.roads.local_node(cell, &spot.node)
    }

    fn spot(&self, spot: SpotRef) -> Option<&Spot> {
        self.clusters.get(&spot.cell)?.spots.get(spot.index)
    }

    fn spot_mut(&mut self, spot: SpotRef) -> Option<&mut Spot> {
        self.clusters.get_mut(&spot.cell)?.spots.get_mut(spot.index)
    }

    fn park(&mut self, car: ParkedCar) {
        if let Some(s) = self.spot_mut(car.spot) {
            s.taken = Some(car.id);
        }
        let at = self.parked.partition_point(|c| c.id < car.id);
        self.parked.insert(at, car);
    }

    /// Starts `car` from its spot to a free spot at `cell`: the owner's spot when it is going home.
    fn depart(&mut self, car: ParkedCar, cell: Axial) -> Option<SpotRef> {
        let house = (car.home.cell == cell).then_some(car.home.house);
        let index = self.clusters.get(&cell)?.free_spot(house)?;
        let to = SpotRef { cell, index };
        let (origin, destination) = (self.node_of(car.spot.cell)?, self.node_of(cell)?);
        let route = self.roads.route(origin, destination)?;
        let lane = LanePath::rounded(&route.centerline(&self.roads), self.config.lane_offset);
        if lane.points().len() < 2 {
            return None;
        }
        let (from, dest) = (self.spot(car.spot)?, self.spot(to)?);
        let step = self.config.car_speed * TICK_SECONDS;
        let reach = (0.45 * lane.length()).min(JOIN_REACH);
        let joins = || {
            (0..)
                .map(|i| i as f32 * JOIN_STEP)
                .take_while(move |s| *s <= reach)
        };

        // Out of the spot: each reverse the spot gives, joining the lane at the first distance
        // from the node that keeps the turn gentle (the node itself when it does).
        let mut best: Option<(f32, Vec<Vec2>, Vec<Vec2>, f32)> = None;
        let alts = std::iter::once(&from.back)
            .chain((!from.back_alt.is_empty()).then_some(&from.back_alt));
        for alt in alts {
            let mut back = vec![from.pose];
            back.extend(alt.iter().copied());
            let back_pts = if back.len() > 1 {
                geom::hermite_leg(&back, true)
            } else {
                vec![from.pose.pos]
            };
            let back_turn = LanePath::new(&back_pts, 0.0).max_turn_deg(step);
            let mut leave = vec![*back.last().expect("starts with the spot")];
            leave.extend(from.leave.iter().copied());
            let pick = pick_join(joins(), |s| {
                let mut poses = leave.clone();
                poses.push(lane.pose_at_distance(s));
                let leg = geom::hermite_leg(&poses, false);
                let mut whole = leg.clone();
                whole.extend(lane.slice(s, s + JOIN_TAIL).into_iter().skip(1));
                let turn = LanePath::new(&whole, 0.0).max_turn_deg(step).max(back_turn);
                (turn, leg)
            });
            if let Some((turn, s, leg)) = pick
                && best
                    .as_ref()
                    .is_none_or(|b| b.0 > TURN_LIMIT_DEG && turn < b.0)
            {
                best = Some((turn, back_pts, leg, s));
            }
        }
        let (_, back_pts, leave_pts, out) = best?;

        // Into the spot: off the lane at the node. Leaving the lane earlier cuts the mouth corner
        // into the kerbs at the bakery, which the enter poses are not laid out for.
        let into = pick_join(std::iter::once(0.0), |e| {
            let s = lane.length() - e;
            let mut poses = vec![lane.pose_at_distance(s)];
            poses.extend(dest.enter.iter().copied());
            poses.push(dest.pose);
            let leg = geom::hermite_leg(&poses, false);
            let mut whole = lane.slice(s - JOIN_TAIL, s);
            whole.extend(leg.iter().skip(1));
            (LanePath::new(&whole, 0.0).max_turn_deg(step), leg)
        });
        let (_, e, enter_pts) = into?;

        let mut points: Vec<Vec2> = back_pts;
        let reverse = geom::polyline_length(&points);
        points.extend(leave_pts.into_iter().skip(1));
        let lane_start = geom::polyline_length(&points);
        points.extend(lane.slice(out, lane.length() - e).into_iter().skip(1));
        let lane_end = geom::polyline_length(&points);
        points.extend(enter_pts.into_iter().skip(1));

        // Where the trip crosses a railway: the car's centre holds CAR_CROSSING_STOP short of the
        // crossing point while the crossing is closed.
        let cells: std::collections::BTreeSet<Axial> = route.links.iter().map(|l| l.cell).collect();
        let crossings: Vec<(Axial, f32)> = self
            .rails
            .crossings()
            .iter()
            .filter(|x| cells.contains(&x.cell()))
            .filter_map(|x| {
                let (d, s) = crate::rail::closest_distance(&points, x.at());
                (d < 3.0).then_some((x.cell(), (s - crate::rail::CAR_CROSSING_STOP).max(0.0)))
            })
            .collect();
        let id = car.id;
        self.parked.retain(|c| c.id != id);
        if let Some(s) = self.spot_mut(car.spot) {
            s.taken = None;
        }
        if let Some(s) = self.spot_mut(to) {
            s.taken = Some(id);
        }
        let new = Car {
            id,
            home: car.home,
            from: car.spot,
            to,
            origin,
            destination,
            links: route.links.clone(),
            path: LanePath::new(&points, 0.0),
            reverse,
            lanes: (lane_start, lane_end),
            prev: 0.0,
            s: 0.0,
            crossings,
        };
        let at = self.cars.partition_point(|c| c.id < id);
        self.cars.insert(at, new);
        self.departures += 1;
        Some(to)
    }

    /// Frees the spot a removed car held, unless the cluster went with it.
    fn release(&mut self, spot: SpotRef, id: CarId) {
        if let Some(s) = self.spot_mut(spot)
            && s.taken == Some(id)
        {
            s.taken = None;
        }
    }

    /// Removes the cluster at `cell` with every car owned by its houses, parked in it or driving
    /// to it.
    fn remove_cluster(&mut self, cell: Axial) {
        if !self.clusters.contains_key(&cell) {
            return;
        }
        let gone: Vec<(SpotRef, CarId)> = self
            .parked
            .iter()
            .filter(|c| c.home.cell == cell || c.spot.cell == cell)
            .map(|c| (c.spot, c.id))
            .chain(
                self.cars
                    .iter()
                    .filter(|c| c.home.cell == cell || c.to.cell == cell)
                    .map(|c| (c.to, c.id)),
            )
            .collect();
        self.parked
            .retain(|c| c.home.cell != cell && c.spot.cell != cell);
        self.cars
            .retain(|c| c.home.cell != cell && c.to.cell != cell);
        for (spot, id) in gone {
            self.release(spot, id);
        }
        self.clusters.remove(&cell);
    }

    /// Removes every driving car routed over `cell`, freeing its reserved spot.
    fn drop_cars_on(&mut self, cell: Axial) {
        if self.roads.links_of(cell).next().is_none() {
            return;
        }
        let gone: Vec<(SpotRef, CarId)> = self
            .cars
            .iter()
            .filter(|c| c.links.iter().any(|link| link.cell == cell))
            .map(|c| (c.to, c.id))
            .collect();
        self.cars
            .retain(|c| !c.links.iter().any(|link| link.cell == cell));
        for (spot, id) in gone {
            self.release(spot, id);
        }
    }

    /// Gives every cluster its owned cars (PRD §7.1): houses in spot order each own one car, up to
    /// the tile's car cap and the global [`SimConfig::max_cars`]. A missing car is created parked
    /// in its house's spot, or the cluster's lowest free spot.
    fn refill(&mut self) {
        let cells: Vec<Axial> = self.clusters.keys().copied().collect();
        for cell in cells {
            let cluster = &self.clusters[&cell];
            let (cap, houses): (usize, Vec<u16>) =
                (cluster.cap, cluster.spots.iter().map(|s| s.house).collect());
            for house in houses {
                if self.owned_by(cell) >= cap || self.car_count() >= self.config.max_cars {
                    break;
                }
                let home = Home { cell, house };
                let has_car = self.parked.iter().any(|c| c.home == home)
                    || self.cars.iter().any(|c| c.home == home);
                if has_car {
                    continue;
                }
                let Some(index) = self.clusters[&cell].free_spot(Some(house)) else {
                    break;
                };
                let spot = SpotRef { cell, index };
                let pose = self.clusters[&cell].spots[index].pose;
                let id = CarId(self.next_car);
                self.next_car += 1;
                self.park(ParkedCar {
                    id,
                    home,
                    spot,
                    pose,
                    since: self.ticks,
                });
            }
        }
    }

    /// Sends every visitor whose shop stay is over home, or, when home is full or cut off, to the
    /// first other connected house cluster with room in cell order. A visitor with nowhere to go
    /// waits and tries again next tick.
    fn send_visitors_home(&mut self) {
        let stay = (self.config.shop_stay_seconds / TICK_SECONDS).round() as u64;
        let due: Vec<ParkedCar> = self
            .parked
            .iter()
            .filter(|c| {
                self.ticks.saturating_sub(c.since) >= stay
                    && self.clusters.get(&c.spot.cell).is_some_and(|k| k.shop)
            })
            .cloned()
            .collect();
        for car in due {
            let home = car.home.cell;
            let houses: Vec<Axial> = std::iter::once(home)
                .chain(
                    self.clusters
                        .iter()
                        .filter(|(c, k)| !k.shop && **c != home)
                        .map(|(c, _)| *c),
                )
                .collect();
            for cell in houses {
                if self.depart(car.clone(), cell).is_some() {
                    break;
                }
            }
        }
    }

    fn spawn(&mut self) {
        let rate = self.spawn_rate_hz();
        if rate <= 0.0 {
            self.spawn_credit = 0.0;
            return;
        }
        self.spawn_credit += rate * TICK_SECONDS;
        while self.spawn_credit >= 1.0 {
            self.spawn_credit -= 1.0;
            self.depart_random();
        }
    }

    /// Starts one seeded departure: a random parked car in a connected cluster goes home when it
    /// is away and its home has room, else to a random other connected cluster (redirected to the
    /// next one with a free spot when that one is full). Counts a refusal when none has room.
    fn depart_random(&mut self) {
        let (candidates, groups) = {
            let nodes: Vec<(Axial, NodeId)> = self
                .clusters
                .keys()
                .filter_map(|&c| Some((c, self.node_of(c)?)))
                .collect();
            let comps = self.roads.components();
            let connected = comps.connected_destinations();
            let candidates: Vec<CarId> = self
                .parked
                .iter()
                .filter(|c| !self.clusters[&c.spot.cell].shop)
                .filter(|c| {
                    nodes
                        .iter()
                        .any(|(cell, n)| *cell == c.spot.cell && connected.binary_search(n).is_ok())
                })
                .map(|c| c.id)
                .collect();
            let groups: Vec<(Axial, Option<usize>)> =
                nodes.iter().map(|(c, n)| (*c, comps.of(*n))).collect();
            (candidates, groups)
        };
        if candidates.is_empty() {
            return;
        }
        let id = candidates[self.rng.below(candidates.len())];
        let car = self
            .parked
            .iter()
            .find(|c| c.id == id)
            .expect("candidate is parked")
            .clone();
        let comp = groups
            .iter()
            .find(|(c, _)| *c == car.spot.cell)
            .and_then(|(_, g)| *g);
        let others: Vec<Axial> = groups
            .iter()
            .filter(|(c, g)| *c != car.spot.cell && *g == comp)
            .map(|(c, _)| *c)
            .collect();
        if others.is_empty() {
            return;
        }
        let pick = others[self.rng.below(others.len())];
        let home = car.home.cell;
        let target = if home != car.spot.cell
            && others.contains(&home)
            && self.clusters[&home].free_spot(None).is_some()
        {
            home
        } else {
            pick
        };
        if self.send(id, target).is_none() {
            self.refused += 1;
        }
    }
}
