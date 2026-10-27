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

/// Ticks an excess car fades out for before a lowered car cap removes it (PRD §7.2): the
/// Legend's 240 ms motion token, rounded up to whole 20 Hz ticks (250 ms).
pub const CAR_FADE_TICKS: u64 = 5;

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
    /// Ticks the car has stood still in a row (giving way, queueing, waiting to pull out).
    waiting: u32,
    /// Where a pull-out from a forecourt on a through road ends and where a turn into one starts,
    /// in meters along `path` ([`drive_holds`]): `(0, ∞)` for a trip with neither.
    drives: (f32, f32),
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

    /// Where a pull-out from a forecourt on a through road ends and where a turn into one starts,
    /// in meters along the path: `(0, ∞)` for a trip with neither (PRD §7.2, the forecourt rule).
    pub fn drives(&self) -> (f32, f32) {
        self.drives
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

/// Least gap a car turning right off or onto a lane keeps between its body and the body of a car
/// on the oncoming lane, in meters (PRD §7.2): the follow rules never pair the two, since their
/// paths stay more than `traffic::LANE_CLEAR` apart.
const PASS_GAP: f32 = 0.25;

/// How far the body of a car driving `pts` reaches past the near side of a car body on the
/// lane oncoming to `along` (a pose on a straight stretch of the car's own lane), less
/// [`PASS_GAP`], else 0. The oncoming lane runs `2 · offset` left of `along`.
fn oncoming_reach(pts: &[Vec2], along: Pose, offset: f32) -> f32 {
    let path = LanePath::new(pts, 0.0);
    let left = geom::scale(geom::right_of(along.dir), -1.0);
    let limit = 2.0 * offset - 0.5 * crate::traffic::CAR_WIDTH - PASS_GAP;
    let (f, w) = (
        0.5 * crate::rail::CAR_LENGTH,
        0.5 * crate::traffic::CAR_WIDTH,
    );
    let mut worst = 0.0f32;
    let mut d = 0.0;
    while d <= path.length() {
        let p = path.pose_at_distance(d);
        let r = geom::right_of(p.dir);
        for (a, b) in [(f, w), (f, -w), (-f, w), (-f, -w)] {
            let c = geom::add(p.pos, geom::add(geom::scale(p.dir, a), geom::scale(r, b)));
            worst = worst.max(geom::dot(geom::sub(c, along.pos), left) - limit);
        }
        d += 0.1;
    }
    worst
}

/// Whether `pts` turns at least [`OFF_TURN_DEG`] right of `along` (toward the lane offset side): a
/// turn off the road into a forecourt, not a house cluster's gentle pull-in.
fn turns_off(pts: &[Vec2], along: Pose, offset: f32) -> bool {
    let path = LanePath::new(pts, 0.0);
    let end = path.pose_at_distance(path.length()).dir;
    let turn = geom::dot(along.dir, end)
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees();
    turn >= OFF_TURN_DEG && geom::cross(along.dir, end) * offset > 0.0
}

/// Turns off the lane at least this sharp, in degrees, are checked against the oncoming lane
/// ([`oncoming_reach`]).
const OFF_TURN_DEG: f32 = 45.0;

/// A leg's score for [`pick_join`], lowest first: a gentle leg (turning at most
/// [`TURN_LIMIT_DEG`] per tick) clear of the oncoming lane ([`oncoming_reach`]) by its turn, then a
/// gentle one reaching toward it by how far, then any other by its turn.
fn reach_score(turn: f32, reach: f32) -> f32 {
    match (turn <= TURN_LIMIT_DEG, reach > 0.0) {
        (true, false) => turn,
        (true, true) => TURN_LIMIT_DEG + 1e-3 + reach.min(10.0),
        (false, _) => TURN_LIMIT_DEG + 20.0 + turn,
    }
}

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

/// The farthest each car (in `cars` order) may drive this tick, `f32::INFINITY` when nothing holds
/// it: short of each closed crossing on its trip, cars that have not passed the hold queue on
/// their approach (the way they face at the hold) in order of the distance left to it, the front
/// car stopping at the hold and each car behind it at least [`crate::rail::CAR_QUEUE_SPACING`]
/// behind the car ahead's new position. No car is moved backwards, so one already too close waits
/// where it is. Ties go to the lower id, so the queue is deterministic.
fn crossing_holds(cars: &[Car], closed: &[Axial], step: f32) -> Vec<f32> {
    let mut holds = vec![f32::INFINITY; cars.len()];
    for &cell in closed {
        // (car, its hold on the path, distance left to it, its heading there)
        let mut waiting: Vec<(usize, f32, f32, Vec2)> = cars
            .iter()
            .enumerate()
            .filter_map(|(i, car)| {
                let &(_, stop) = car.crossings.iter().find(|(c, _)| *c == cell)?;
                let left = stop - car.s;
                (left >= -1e-3).then(|| (i, stop, left, car.path.pose_at_distance(stop).dir))
            })
            .collect();
        let Some(&(_, _, _, reference)) = waiting.first() else {
            continue;
        };
        waiting.sort_by(|a, b| a.2.total_cmp(&b.2).then(cars[a.0].id.cmp(&cars[b.0].id)));
        for side in [true, false] {
            let mut floor = 0.0_f32;
            for &(i, stop, left, _) in waiting
                .iter()
                .filter(|w| (geom::dot(w.3, reference) > 0.0) == side)
            {
                let next_left = (left - step).max(floor).min(left);
                holds[i] = holds[i].min(stop - next_left);
                floor = next_left + crate::rail::CAR_QUEUE_SPACING;
            }
        }
    }
    holds
}

/// Meters short of the end of its lanes an arriving car starts to keep its destination's yard clear
/// of new departures: about four seconds out at the default speed.
const YARD_WATCH: f32 = 30.0;

/// The farthest each car (in `cars` order) may drive this tick under the yard rule, one trip at a
/// time through a parking tile's yard, `f32::INFINITY` when it does not hold the car:
/// - a car that has not started its pull-out stays in its spot while a car bound for the same tile
///   is within [`YARD_WATCH`] of its lanes' end or parking, or while another departure from the tile
///   is still on its driveway (the one that started first, or the lower id when both are waiting);
/// - an arriving car stops at the end of its lanes while a departure from its destination tile is
///   backing out or on its driveway.
fn yard_holds(cars: &[Car]) -> Vec<f32> {
    let mut holds = vec![f32::INFINITY; cars.len()];
    let leaving = |c: &Car| c.s < c.lanes.0;
    for (i, car) in cars.iter().enumerate() {
        if car.s <= 0.0 {
            let busy = cars.iter().any(|o| {
                o.id != car.id
                    && ((o.to.cell == car.from.cell
                        && o.s >= o.lanes.1 - YARD_WATCH
                        && !o.arrived())
                        || (o.from.cell == car.from.cell
                            && leaving(o)
                            && (o.s > 0.0 || o.id < car.id)))
            });
            if busy {
                holds[i] = 0.0;
            }
        } else if car.s <= car.lanes.1 {
            let busy = cars
                .iter()
                .any(|o| o.from.cell == car.to.cell && o.s > 0.0 && leaving(o));
            if busy {
                holds[i] = car.lanes.1;
            }
        }
    }
    holds
}

/// Whether car `j`, its body first meeting `sweep` `d` meters ahead, would stop inside it: a car that
/// stood still last tick (another road car, or one in its spot) stands on `j`'s path between `d`
/// and a queue spacing past the last of `look` poses ahead where `j`'s body meets the sweep.
fn queued(cars: &[Car], j: usize, d: f32, look: usize, sweep: &[Pose]) -> bool {
    let o = &cars[j];
    let last = (0..look)
        .rev()
        .map(|k| k as f32 * DRIVE_STEP)
        .find(|e| meets_any(o.path.pose_at_distance(o.s + e), sweep))
        .unwrap_or(d);
    let reach = last + crate::rail::CAR_QUEUE_SPACING;
    let line: Vec<Pose> = (0..)
        .map(|k| d + k as f32 * DRIVE_STEP * 2.0)
        .take_while(|e| *e <= reach)
        .map(|e| o.path.pose_at_distance(o.s + e))
        .collect();
    cars.iter().enumerate().any(|(k, other)| {
        if k == j || other.waiting == 0 || other.s <= 0.0 {
            return false;
        }
        let p = other.path.pose_at_distance(other.s);
        line.iter()
            .any(|q| geom::dist(p.pos, q.pos) <= 1.5 && geom::dot(p.dir, q.dir) > 0.5)
    })
}

/// A forecourt manoeuvre about to start or under way ([`drive_holds`]).
struct Manoeuvre {
    /// Where the car holds before it starts, in meters along its path.
    hold_at: f32,
    started: bool,
    /// The car's heading on its lane where the manoeuvre leaves or joins it.
    dir: Vec2,
    /// The car's poses along the rest of the manoeuvre.
    sweep: Vec<Pose>,
}

/// `car`'s pull-out until it ends, or its turn-in from [`crate::traffic::LOOK_AHEAD`] before it
/// starts.
fn manoeuvre(car: &Car) -> Option<Manoeuvre> {
    let (out_end, in_start) = car.drives;
    let look = crate::traffic::LOOK_AHEAD;
    if car.s < out_end {
        Some(Manoeuvre {
            hold_at: 0.0,
            started: car.s > 0.0,
            dir: car.path.pose_at_distance(out_end).dir,
            sweep: drive_sweep(&car.path, car.s.max(0.0), out_end),
        })
    } else if in_start.is_finite() && car.s >= in_start - look {
        Some(Manoeuvre {
            hold_at: in_start,
            // An arrival kept waiting [`crate::traffic::STARVE_TICKS`] turns in anyway, and the
            // cars driving the other way stop short of it, so a wait that a queue behind it feeds
            // back into (that queue held by a car giving way to this one) cannot last.
            started: car.s > in_start || car.waiting >= crate::traffic::STARVE_TICKS,
            dir: car.path.pose_at_distance(in_start).dir,
            sweep: drive_sweep(&car.path, car.s.max(in_start), car.path.length()),
        })
    } else {
        None
    }
}

/// Spacing of the poses [`drive_holds`] sweeps, in meters.
const DRIVE_STEP: f32 = 0.25;

/// The poses every [`DRIVE_STEP`] along `path` from `from` to `to`.
fn drive_sweep(path: &LanePath, from: f32, to: f32) -> Vec<Pose> {
    let mut out = vec![];
    let mut d = from;
    while d <= to {
        out.push(path.pose_at_distance(d));
        d += DRIVE_STEP;
    }
    out
}

/// Whether a car body at `p` comes within [`PASS_GAP`] of one at any pose of `sweep`.
fn meets_any(p: Pose, sweep: &[Pose]) -> bool {
    let reach = (crate::rail::CAR_LENGTH + crate::traffic::CAR_WIDTH + PASS_GAP).powi(2);
    sweep.iter().any(|q| {
        let d = geom::sub(q.pos, p.pos);
        geom::dot(d, d) <= reach && crate::traffic::bodies_meet(p, *q, PASS_GAP)
    })
}

/// The farthest each car (in `cars` order) may drive this tick under the forecourt rule (PRD §7.2),
/// `f32::INFINITY` when it does not hold the car. A pull-out from a forecourt on a through road
/// (the bakery's) swings the car's body out over the oncoming lane, and so may the turn into one;
/// the follow rules never pair those cars, since their paths stay more than
/// `traffic::LANE_CLEAR` apart. For each such manoeuvre (`Car::drives`) and each car driving the
/// other way on the road:
/// - a manoeuvre not yet started waits (in the spot, or at the end of the arrival's lane) while the
///   other car's body anywhere along its [`crate::traffic::LOOK_AHEAD`], from where it stands, would
///   come within [`PASS_GAP`] of the car's body anywhere along the manoeuvre and the other car can
///   drive through; a car in its spot waits so for road cars going either way, since its reverse
///   swings across both lanes;
/// - a pull-out stopped on its way out by another rule checks again, as if in its spot, before it
///   moves on;
/// - keep clear: a road car that stood still last tick, or would stop inside a turn-in behind a
///   car standing still ahead of it ([`queued`]), instead stops short of it and the turn-in goes
///   first, so no queue stands inside another forecourt's turn-in;
/// - once the manoeuvre has started, the other car stops short of the first point of its
///   look-ahead where its body would come that close to the rest of the manoeuvre, unless its body
///   already stands that close, when it drives on clear of it and the manoeuvre waits instead;
/// - paths the follow rules pair (a turn across the other car's lane) are left to them, and so are
///   cars bound for or leaving the same forecourt (with the yard rule); of two manoeuvres each in
///   the other's way, the lower id goes first and the other waits;
/// - an arrival kept waiting [`crate::traffic::STARVE_TICKS`] counts as started.
fn drive_holds(cars: &[Car]) -> Vec<f32> {
    let mut holds = vec![f32::INFINITY; cars.len()];
    let look = crate::traffic::LOOK_AHEAD;
    let manoeuvres: Vec<Option<Manoeuvre>> = cars.iter().map(manoeuvre).collect();
    for (i, c) in cars.iter().enumerate() {
        let Some(m) = &manoeuvres[i] else {
            continue;
        };
        let (out_end, hold_at, started, dir) = (c.drives.0, m.hold_at, m.started, m.dir);
        let sweep: &[Pose] = &m.sweep;
        let here = c.path.pose_at_distance(c.s.max(0.0)).pos;
        // A car still in its spot waits for every road car, either way, whose body would meet its
        // pull-out: its reverse swings its tail across the near lane as well as the far one.
        // So does a pull-out stopped on its way out (held by another rule) before it moves on.
        let in_spot = out_end > 0.0 && (c.s <= 0.0 || (c.s < c.lanes.0 && c.waiting > 0));
        let forecourt = if c.s < out_end {
            c.from.cell
        } else {
            c.to.cell
        };
        for (j, o) in cars.iter().enumerate() {
            // Cars bound for or leaving the same forecourt are the yard and merge rules' to order.
            let yard = [o.from.cell, o.to.cell].contains(&forecourt);
            if j == i || o.s < o.lanes.0 || o.s > o.lanes.1 || yard {
                continue;
            }
            let at = o.path.pose_at_distance(o.s);
            if geom::dist(at.pos, here) > 2.0 * look || (!in_spot && geom::dot(at.dir, dir) > -0.5)
            {
                continue;
            }
            let ahead: Vec<Pose> = (0..)
                .map(|k| k as f32 * DRIVE_STEP)
                .take_while(|d| *d <= look)
                .map(|d| o.path.pose_at_distance(o.s + d))
                .collect();
            // Paths that come within `LANE_CLEAR` at any angle but head-on (a turn across the
            // other's lane, or into the drive behind it) are the follow and merge rules' to order.
            let clear = crate::traffic::LANE_CLEAR;
            let paired = ahead.iter().any(|p| {
                sweep
                    .iter()
                    .any(|q| geom::dist(p.pos, q.pos) <= clear && geom::dot(p.dir, q.dir) > -0.97)
            });
            if paired && !in_spot {
                continue;
            }
            let first = ahead
                .iter()
                .position(|p| meets_any(*p, sweep))
                .map(|k| k as f32 * DRIVE_STEP);
            // Two manoeuvres each in the other's way: the lower id goes first, the other waits.
            let mutual = first.is_some()
                && manoeuvres[j].as_ref().is_some_and(|om| {
                    (0..)
                        .map(|k| k as f32 * DRIVE_STEP)
                        .take_while(|d| *d <= look)
                        .any(|d| meets_any(c.path.pose_at_distance(c.s + d), &om.sweep))
                });
            if mutual {
                if c.id > o.id {
                    holds[i] = holds[i].min(if started { c.s } else { hold_at });
                }
                continue;
            }
            if in_spot {
                // On its driveway the car gives way: the merge rule leaves the road car its way.
                if first.is_some() {
                    holds[i] = holds[i].min(c.s.max(0.0));
                }
                continue;
            }
            match first {
                None => {}
                Some(d) if d <= 0.0 => holds[i] = holds[i].min(if started { c.s } else { hold_at }),
                // Keep clear: a car that has stopped, or would stop inside the manoeuvre behind a
                // stopped car ahead, waits short of it and lets it go first.
                Some(d)
                    if !started && o.waiting == 0 && !queued(cars, j, d, ahead.len(), sweep) =>
                {
                    holds[i] = holds[i].min(hold_at);
                }
                Some(d) => holds[j] = holds[j].min(o.s + d - DRIVE_STEP),
            }
        }
    }
    holds
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
    /// A car that left for a trip but still stands in the spot or is backing out of it (it may be
    /// waiting to merge), so no other car is sent here yet.
    vacating: Option<CarId>,
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
                vacating: None,
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
        let free = |i: &usize| self.spots[*i].taken.is_none() && self.spots[*i].vacating.is_none();
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
    /// Cars over a lowered cap, fading out: the tick each started fading and the tick it goes.
    leaving: BTreeMap<CarId, (u64, u64)>,
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
            leaving: BTreeMap::new(),
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

    /// Changes the world's car cap (a quality-tier change, PRD §7.2). Lowering it below the car
    /// count marks the newest cars, parked or driving, as leaving: they keep driving but start no
    /// new trip, fade out for `fade_ticks` ([`CAR_FADE_TICKS`], 0 under reduced motion) and are
    /// removed after it, freeing their spots; with `fade_ticks` 0 they go at once. Raising it
    /// gives the clusters their missing cars, parked, and keeps any car no longer over the cap.
    /// Deterministic: ids decide which cars go.
    pub fn set_max_cars(&mut self, max_cars: usize, fade_ticks: u64) {
        self.config.max_cars = max_cars;
        let mut ids: Vec<CarId> = self
            .cars
            .iter()
            .map(|c| c.id)
            .chain(self.parked.iter().map(|c| c.id))
            .collect();
        ids.sort_unstable();
        let excess = ids.len().saturating_sub(max_cars);
        let going: std::collections::BTreeSet<CarId> =
            ids.split_off(ids.len() - excess).into_iter().collect();
        self.leaving.retain(|id, _| going.contains(id));
        if fade_ticks == 0 {
            self.remove_cars(&going);
        } else {
            let span = (self.ticks, self.ticks + fade_ticks);
            for id in going {
                self.leaving.entry(id).or_insert(span);
            }
        }
        self.refill();
    }

    /// How far car `id` has faded out, from 0 (not leaving) to 1 (gone), `alpha` of the way from
    /// the previous tick to the latest one (render interpolation, as [`Car::pose_at`]).
    pub fn fade(&self, id: CarId, alpha: f32) -> f32 {
        self.leaving.get(&id).map_or(0.0, |&(start, end)| {
            let t = (self.ticks - start) as f32 - 1.0 + alpha.clamp(0.0, 1.0);
            (t / (end - start) as f32).clamp(0.0, 1.0)
        })
    }

    /// Cars fading out over a lowered cap, by id, with the tick each started and the tick it goes.
    pub fn leaving(&self) -> &BTreeMap<CarId, (u64, u64)> {
        &self.leaving
    }

    /// Removes the cars in `gone`, parked or driving, freeing the spots they held.
    fn remove_cars(&mut self, gone: &std::collections::BTreeSet<CarId>) {
        if gone.is_empty() {
            return;
        }
        let freed: Vec<(SpotRef, CarId)> = self
            .cars
            .iter()
            .filter(|c| gone.contains(&c.id))
            .map(|c| (c.to, c.id))
            .chain(
                self.parked
                    .iter()
                    .filter(|c| gone.contains(&c.id))
                    .map(|c| (c.spot, c.id)),
            )
            .collect();
        self.cars.retain(|c| !gone.contains(&c.id));
        self.parked.retain(|c| !gone.contains(&c.id));
        for (spot, id) in freed {
            self.release(spot, id);
        }
        self.leaving.retain(|id, _| !gone.contains(id));
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
    /// they near), then every driving car, queueing short of a closed crossing, keeping the spacing
    /// behind the car ahead and giving way at merges ([`crate::traffic`]) and taking turns through
    /// a parking yard; parks the cars that had already arrived (so the final approach still
    /// renders), then starts departures.
    pub fn tick(&mut self) {
        self.ticks += 1;
        let gone: std::collections::BTreeSet<CarId> = self
            .leaving
            .iter()
            .filter(|(_, (_, end))| *end < self.ticks)
            .map(|(id, _)| *id)
            .collect();
        self.remove_cars(&gone);
        self.rails.tick();
        let step = self.config.car_speed * TICK_SECONDS;
        let holds = crossing_holds(&self.cars, &self.rails.closed_crossings(), step);
        let caps = crate::traffic::follow_caps(
            &self
                .cars
                .iter()
                .map(|c| crate::traffic::Mover {
                    id: c.id.0,
                    s: c.s,
                    path: &c.path,
                    lane_start: c.lanes.0,
                    waiting: c.waiting,
                })
                .collect::<Vec<_>>(),
        );
        let yards = yard_holds(&self.cars);
        let drives = drive_holds(&self.cars);
        let mut arrived = vec![];
        let mut k = 0;
        self.cars.retain_mut(|car| {
            let (hold, cap) = (holds[k].min(yards[k]).min(drives[k]), caps[k]);
            k += 1;
            if car.s >= car.path.length() {
                arrived.push((car.id, car.home, car.to));
                return false;
            }
            car.prev = car.s;
            car.s = (car.s + step)
                .min(car.path.length())
                .min(hold)
                .min(cap)
                .max(car.s);
            car.waiting = if car.s == car.prev {
                car.waiting.saturating_add(1)
            } else {
                0
            };
            true
        });
        self.mark_vacating();
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
        if self.leaving.contains_key(&id) {
            return None;
        }
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

        // A drive whose spur meets a through road at a junction node (the bakery's): departures
        // keep their bodies clear of the oncoming lane there ([`oncoming_reach`]), measured from
        // the outbound lane's straight past every join.
        let through = route.links.first().is_some_and(|l| {
            self.roads.link(*l).is_some_and(|link| {
                let (a, b) = link.ends();
                let next = if a == origin { b } else { a };
                self.roads.node(next).is_some_and(|n| n.links().len() >= 3)
            })
        });
        let outbound = lane.pose_at_distance(reach + JOIN_TAIL);
        let into_through = route.links.last().is_some_and(|l| {
            self.roads.link(*l).is_some_and(|link| {
                let (a, b) = link.ends();
                let next = if a == destination { b } else { a };
                self.roads.node(next).is_some_and(|n| n.links().len() >= 3)
            })
        });

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
                let reach = if through {
                    oncoming_reach(&whole, outbound, self.config.lane_offset)
                } else {
                    0.0
                };
                (reach_score(turn, reach), leg)
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

        // Into the spot: off the lane at the node when the turn there is gentle, else at the first
        // distance before it that is (the bakery's arrivals from the east turn right off the through
        // lane short of its spur; its enter poses are laid out for both directions).
        // The lane's straight approach, short of where any arrival leg may leave it.
        let approach = lane.pose_at_distance(lane.length() - reach - JOIN_TAIL);
        let into = pick_join(joins(), |e| {
            let s = lane.length() - e;
            let mut poses = vec![lane.pose_at_distance(s)];
            poses.extend(dest.enter.iter().copied());
            poses.push(dest.pose);
            let leg = geom::hermite_leg(&poses, false);
            let mut whole = lane.slice(s - JOIN_TAIL, s);
            whole.extend(leg.iter().skip(1));
            let turn = LanePath::new(&whole, 0.0).max_turn_deg(step);
            let offset = self.config.lane_offset;
            let reach = if turns_off(&whole, approach, offset) {
                oncoming_reach(&whole, approach, offset)
            } else {
                0.0
            };
            (reach_score(turn, reach), leg)
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
            s.vacating = Some(id);
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
            waiting: 0,
            drives: (
                if through {
                    lane_start + crate::rail::CAR_LENGTH
                } else {
                    0.0
                },
                if into_through {
                    lane_end - crate::rail::CAR_LENGTH
                } else {
                    f32::INFINITY
                },
            ),
        };
        let at = self.cars.partition_point(|c| c.id < id);
        self.cars.insert(at, new);
        self.departures += 1;
        Some(to)
    }

    /// Marks every spot whose car has left on a trip but has not backed out of it yet.
    fn mark_vacating(&mut self) {
        for cluster in self.clusters.values_mut() {
            for spot in &mut cluster.spots {
                spot.vacating = None;
            }
        }
        let still: Vec<(SpotRef, CarId)> = self
            .cars
            .iter()
            .filter(|c| c.s < c.reverse)
            .map(|c| (c.from, c.id))
            .collect();
        for (spot, id) in still {
            if let Some(s) = self.spot_mut(spot) {
                s.vacating = Some(id);
            }
        }
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
                    && !self.leaving.contains_key(&c.id)
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
                .filter(|c| !self.clusters[&c.spot.cell].shop && !self.leaving.contains_key(&c.id))
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
