//! Trains on the rail network (PRD §7.1, §7.2, P3): the stitched rail graph, the switch at every
//! turnout toe, level crossings and the trains themselves, on the fixed 20 Hz tick.
//!
//! - **Graph.** One [`Network`] of [`NetworkKind::Rail`], stitched at seams like the roads. A
//!   turnout's three segments meet at its toe node ([`hex_core::SwitchDef`]); a train running in on
//!   the toe segment leaves on the leg the switch is set to, and from either leg it trails through
//!   onto the toe segment. No route joins the two legs.
//! - **Trains.** An engine, a carriage and a brake third ([`CONSIST`]), [`TRAIN_LENGTH`] meters
//!   over the buffers, the engine at one end and the two carriages in an order seeded per train
//!   (owner A13-Q2, [`consist_for`]). At a line end (a seam facing an empty cell; rail meets only
//!   rail, so a line can end nowhere else) a buffer stop stands [`hex_core::rail_end::STOP_U`]
//!   inside the last tile ([`Railway::buffer_stops`]); a train stops with its leading buffers at
//!   the stop's ([`Railway::line_end_arrivals`] names it that tick), waits [`DWELL_SECONDS`] and
//!   runs back the other way, the tank engine propelling its carriages (a push-pull "autotrain": tank
//!   engines work both ways without a turntable). On a loop it loops.
//! - **Routing.** A train with a reachable station has a destination, one station stop, and a
//!   route to it: an A* search over directed links ([`Railway::route_length`]). Its first
//!   destination is the stop at a seeded index ([`Railway::with_seed`], mixed with the train id),
//!   and on arriving it takes the next reachable stop in stop order, so it visits the stations in
//!   turn. A route may turn the train round only where a train stands anyway: at a line end and at
//!   a station stop, each turn costing [`REVERSE_COST`], and never at a station on a stretch with
//!   no line end, so trains on a loop keep lapping the same way. At each facing toe the switch is
//!   thrown to the leg its route needs when it reserves the junction. A train with no reachable
//!   station follows the track as before: at each facing toe the leg the junction offers next,
//!   alternating. Routes and destinations are derived, never saved: replanned on every world edit.
//! - **Stations.** A station tile's stop ([`hex_core::StopDef`]) is its platform's level section.
//!   A train running toward one brakes to stand with its outer axles ([`axle_span`], from
//!   [`CONSIST`], so a longer consist is centred the same way) centred on the section, waits
//!   [`STATION_DWELL_SECONDS`], and runs on ([`Railway::departures`] names it that tick). It stops
//!   at every station it passes, its destination or not, once per pass: the stop re-arms when its
//!   body leaves the station's link or it reverses. Leaving a station it turns round there when
//!   its route back is shorter by more than [`REVERSE_COST`].
//! - **Junctions.** A train may enter a junction's fouling zone (from the toe border to
//!   [`hex_core::SwitchDef::fouling`] along both legs) only with the junction reserved, and the
//!   switch moves only when a train reserves an empty zone, so it never moves under a train. A
//!   train that cannot reserve stands [`ZONE_MARGIN`] short of the zone: beyond the fouling point on
//!   a leg, short of the toe border on the toe side.
//! - **Spacing.** A train brakes to stand [`TRAIN_GAP`] behind any other train on its path. Two
//!   trains stopped nose to nose for [`STUCK_SECONDS`] resolve it: the one with more free track
//!   behind it (the higher id on a tie) runs back and gives way, and a train blocked by anything
//!   else for [`GIVE_UP_SECONDS`] does the same. Giving way it keeps its destination and replans,
//!   which clears a shared stretch (a Y's toe arm) for the other; giving way a second time before
//!   it arrives, it takes the nearest station by route from its new heading, so two trains that
//!   can never pass (a single line) still each reach stations instead of turning each other back.
//! - **Crossings.** A level crossing is closed while a train body is on its deck or a train's front
//!   is within [`CROSSING_WARN`] of it, and cars hold short of a closed crossing ([`crate::Car`]).
//! - **Spawning.** Trains are derived, never saved: on each tick after the graph changed, every
//!   connected stretch of at least [`MIN_TRACK`] meters gets one train, plus one per further
//!   [`TRACK_PER_TRAIN`], up to [`MAX_TRAINS`], placed where the whole train fits clear of others
//!   and of junction zones, all running the same way along one walk of the stretch (on a loop they
//!   lap the same way round). A world edit removes the trains standing on the edited cell.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use hex_core::{Axial, NetworkKind, TileDef};

use crate::geom::{self, Pose, Vec2};
use crate::network::{LinkId, Network, NodeId};
use crate::sim::TICK_SECONDS;

/// Line speed (m/s): a light railway's pace in the diorama, a little under the cars' 8 m/s.
pub const TRAIN_SPEED: f32 = 6.0;
/// Acceleration and braking (m/s²).
pub const ACCEL: f32 = 0.5;
pub const BRAKE: f32 = 1.0;
/// How far ahead a train looks for trains, line ends, junctions and crossings (m): its braking
/// distance at line speed (18 m) with room to spare.
pub const LOOKAHEAD: f32 = 50.0;
/// Clear space a train keeps behind another train's body (m).
pub const TRAIN_GAP: f32 = 4.0;
/// How far short of a junction zone a waiting train stands (m).
pub const ZONE_MARGIN: f32 = 1.0;
/// How far inside the last tile a train's front stops at a line end on a straight (m): at the
/// buffer stop's contact plane (owner D2-Q2, [`hex_core::rail_end::STOP_U`]). Spawning keeps a
/// train's tail this far in too. On a curve the train stops by its body instead: its foremost
/// leading buffer head meets the stop's contact plane.
pub const END_MARGIN: f32 = hex_core::rail_end::STOP_U;
/// The buffer heads every vehicle shares with the stop: 0.85 m either side of the centreline,
/// discs of 0.20 m radius (`docs/checkpoints/rail_buffer_stop/runtime-notes.md` section 2).
pub const BUFFER_SIDE: f32 = 0.85;
pub const BUFFER_RADIUS: f32 = 0.20;
/// Overlap (m) below which a body only touches a stretch of track ([`Railway::body`] tests).
const TOUCH: f32 = 0.05;
/// Salt for the consist order's seed, so it does not follow a train's first destination.
const CONSIST_SALT: u64 = 0x00C0_5157;
/// Wait at a line end before running back (s).
pub const DWELL_SECONDS: f32 = 3.0;
/// Wait at a station stop before running on (s).
pub const STATION_DWELL_SECONDS: f32 = 8.0;
/// How far (m) a stop target may lie behind the train's mark and still hold it there.
const STOP_EPS: f32 = 0.05;
/// What turning round costs a route (m of running): about a minute at line speed for the stand,
/// the brake and the restart. Long enough that a train on a stretch with a line end turns at a
/// station only to save real running.
pub const REVERSE_COST: f32 = 360.0;
/// Two trains stopped facing each other this long (s): one runs back.
pub const STUCK_SECONDS: f32 = 5.0;
/// A train blocked this long by anything else runs back (s), so no layout deadlocks for good.
pub const GIVE_UP_SECONDS: f32 = 30.0;
/// Shortest connected stretch of track (m) that gets a train: two tiles and a bit.
pub const MIN_TRACK: f32 = 55.0;
/// Track per extra train (m).
pub const TRACK_PER_TRAIN: f32 = 140.0;
/// Most trains in one connected stretch and in the world.
pub const MAX_TRAINS_PER_LINE: usize = 4;
pub const MAX_TRAINS: usize = 12;
/// A crossing closes while a train's front is this close (m): about 5 s at line speed, long
/// enough for a car already past the stop line to clear the deck.
pub const CROSSING_WARN: f32 = 30.0;
/// Half the level crossing's road deck measured along the track (m): the 6.5 m deck crossed at
/// 60 degrees spans 3.75 m each side of the crossing point.
pub const CROSSING_HALF: f32 = 4.0;
/// Where a car's centre holds short of a closed crossing, measured back along its path from the
/// crossing point (m): its front stays outside the train's 2.5 m swept half width with 1 m spare at
/// 60 degrees, for a 5.4 m pickup.
pub const CAR_CROSSING_STOP: f32 = 7.0;

/// Train id; ids count up from 0 in spawn order and are never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TrainId(pub u64);

/// One of a turnout's two legs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Leg {
    Straight,
    Branch,
}

impl Leg {
    pub fn other(self) -> Leg {
        match self {
            Leg::Straight => Leg::Branch,
            Leg::Branch => Leg::Straight,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Leg::Straight => "straight",
            Leg::Branch => "branch",
        }
    }
}

/// A rolling-stock type, one kit piece each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VehicleKind {
    Engine,
    Carriage,
    /// The brake third (`train_carriage_b`): the guard's veranda and roof lookout at its rear. Its
    /// length and axles are the carriage's, so it runs second or last (owner A13-Q2: a mix per
    /// train, [`consist_for`]).
    BrakeCarriage,
}

/// A vehicle of the consist: its centre's distance from the engine end of the train and half its
/// outer axle spacing (the train pieces' bake reports: `axlesX`, `lengthM`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleSpec {
    pub kind: VehicleKind,
    pub centre: f32,
    pub half_wheelbase: f32,
}

/// The engine (8.94 m, axles ±2.0 m), a carriage and the brake third (both 7.14 m, axles ±1.5 m),
/// 0.5 m apart. The two carriage places share one envelope, so a train runs its carriages in
/// either order ([`consist_for`]); `kind` here is one of the two.
pub const CONSIST: [VehicleSpec; 3] = [
    VehicleSpec {
        kind: VehicleKind::Engine,
        centre: 4.47,
        half_wheelbase: 2.0,
    },
    VehicleSpec {
        kind: VehicleKind::Carriage,
        centre: 13.01,
        half_wheelbase: 1.5,
    },
    VehicleSpec {
        kind: VehicleKind::BrakeCarriage,
        centre: 20.65,
        half_wheelbase: 1.5,
    },
];

/// Buffer to buffer (m).
pub const TRAIN_LENGTH: f32 = 24.22;

/// The consist's first and last axle, measured from the engine end of the train (m): every wheel
/// of the train stands between them.
pub fn axle_span() -> (f32, f32) {
    CONSIST.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| {
        (
            lo.min(v.centre - v.half_wheelbase),
            hi.max(v.centre + v.half_wheelbase),
        )
    })
}

/// A link travelled in one direction: `forward` runs from the link's `a` end to its `b` end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Step {
    pub link: LinkId,
    pub forward: bool,
}

/// One vehicle's pose: centre on the chord between its outer axles, facing the engine end of the
/// train (so a vehicle never turns round when the train reverses).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehiclePose {
    pub kind: VehicleKind,
    pub pose: Pose,
}

/// What a train is waiting for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocker {
    Clear,
    LineEnd,
    Train(TrainId),
    Junction(Axial),
    /// Standing at the station stop on this cell.
    Station(Axial),
}

/// A train on the track.
#[derive(Clone, Debug, PartialEq)]
pub struct Train {
    id: TrainId,
    /// The links under the train, front first; the front is `head` meters into `steps[0]`.
    steps: Vec<Step>,
    head: f32,
    speed: f32,
    /// Whether the engine is at the front (else it propels from the back).
    engine_leads: bool,
    blocker: Blocker,
    /// Ticks stopped in a row.
    stopped: u32,
    /// The vehicle in each [`CONSIST`] place ([`consist_for`]).
    kinds: [VehicleKind; 3],
    prev: Vec<VehiclePose>,
    now: Vec<VehiclePose>,
    /// Facing toes passed, as (junction cell, leg taken), oldest first; the last 16.
    routes: Vec<(Axial, Leg)>,
    reversals: u32,
    /// The link of the stop this train last waited at, until it leaves that link or reverses.
    served: Option<LinkId>,
    /// Station stops made.
    stops: u32,
    /// The link of the station stop this train is heading for.
    dest: Option<LinkId>,
    /// The planned path to `dest`, front link first (`route[0] == steps[0]` while it lines up); a
    /// link repeated with its direction flipped is a turn round there.
    route: Vec<Step>,
    /// Destinations reached, oldest first (the last 16), as the stops' cells.
    arrivals: Vec<Axial>,
    /// Times this train gave way since it last reached a destination.
    yields: u32,
}

impl Train {
    /// The cell of the station stop this train is heading for, if any.
    pub fn destination(&self) -> Option<Axial> {
        self.dest.map(|l| l.cell)
    }

    /// The link of the station stop this train is heading for, if any.
    pub fn destination_link(&self) -> Option<LinkId> {
        self.dest
    }

    /// The planned path to the destination, front link first (empty without one).
    pub fn route(&self) -> &[Step] {
        &self.route
    }

    /// Destinations this train reached, oldest first (the last 16).
    pub fn arrivals(&self) -> &[Axial] {
        &self.arrivals
    }

    /// Station stops this train has made (each counted when it leaves).
    pub fn stops(&self) -> u32 {
        self.stops
    }

    pub fn id(&self) -> TrainId {
        self.id
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }

    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    pub fn engine_leads(&self) -> bool {
        self.engine_leads
    }

    pub fn blocker(&self) -> Blocker {
        self.blocker
    }

    /// Facing toes this train went through, with the leg it took, oldest first (the last 16).
    pub fn routes(&self) -> &[(Axial, Leg)] {
        &self.routes
    }

    pub fn reversals(&self) -> u32 {
        self.reversals
    }

    /// The vehicle in each [`CONSIST`] place, engine end first.
    pub fn consist(&self) -> [VehicleKind; 3] {
        self.kinds
    }

    /// Each vehicle between the previous tick (`alpha = 0`) and the latest (`alpha = 1`).
    pub fn vehicles(&self, alpha: f32) -> Vec<VehiclePose> {
        let a = alpha.clamp(0.0, 1.0);
        self.prev
            .iter()
            .zip(&self.now)
            .map(|(p, n)| {
                let pos = geom::lerp(p.pose.pos, n.pose.pos, a);
                let (y0, y1) = (p.pose.yaw(), n.pose.yaw());
                let mut d = y1 - y0;
                while d > std::f32::consts::PI {
                    d -= std::f32::consts::TAU;
                }
                while d < -std::f32::consts::PI {
                    d += std::f32::consts::TAU;
                }
                let yaw = y0 + d * a;
                VehiclePose {
                    kind: n.kind,
                    pose: Pose {
                        pos,
                        dir: [yaw.cos(), yaw.sin()],
                    },
                }
            })
            .collect()
    }

    /// The world XZ of the train's front.
    pub fn front(&self) -> Vec2 {
        self.now.first().map_or([0.0, 0.0], |v| v.pose.pos)
    }
}

/// A turnout in the stitched graph.
#[derive(Clone, Debug, PartialEq)]
pub struct Junction {
    cell: Axial,
    node: NodeId,
    toe: LinkId,
    straight: LinkId,
    branch: LinkId,
    /// The fouling distance measured from the toe node along each leg.
    reach: f32,
    set: Leg,
    /// The leg the next facing train takes.
    next: Leg,
    holder: Option<TrainId>,
    throws: u32,
    /// The legs facing trains took, oldest first (the last 16).
    facing: Vec<Leg>,
}

impl Junction {
    pub fn cell(&self) -> Axial {
        self.cell
    }

    /// The leg the switch is set to.
    pub fn set(&self) -> Leg {
        self.set
    }

    /// The train holding the junction, if any.
    pub fn holder(&self) -> Option<TrainId> {
        self.holder
    }

    /// How often the switch moved.
    /// The legs facing trains took here, oldest first (the last 16).
    pub fn facing(&self) -> &[Leg] {
        &self.facing
    }

    pub fn throws(&self) -> u32 {
        self.throws
    }

    fn leg(&self, leg: Leg) -> LinkId {
        match leg {
            Leg::Straight => self.straight,
            Leg::Branch => self.branch,
        }
    }

    fn leg_of(&self, link: LinkId) -> Option<Leg> {
        if link == self.straight {
            Some(Leg::Straight)
        } else if link == self.branch {
            Some(Leg::Branch)
        } else {
            None
        }
    }
}

/// A level crossing: where a rail link crosses a road on one tile.
#[derive(Clone, Debug, PartialEq)]
pub struct Crossing {
    cell: Axial,
    rail: LinkId,
    at: Vec2,
    /// The crossing point's distance along the rail link from its `a` end.
    s: f32,
    closed: bool,
}

impl Crossing {
    pub fn cell(&self) -> Axial {
        self.cell
    }

    /// The crossing point, where the rail and road centrelines meet.
    pub fn at(&self) -> Vec2 {
        self.at
    }

    pub fn closed(&self) -> bool {
        self.closed
    }
}

/// A station stop: a platform's level section along one rail link.
#[derive(Clone, Debug, PartialEq)]
pub struct Stop {
    cell: Axial,
    link: LinkId,
    /// The level section in link coordinates (`a` end = 0).
    level: (f32, f32),
}

impl Stop {
    pub fn cell(&self) -> Axial {
        self.cell
    }

    pub fn link(&self) -> LinkId {
        self.link
    }

    /// The level section along the link from its `a` end (m).
    pub fn level(&self) -> (f32, f32) {
        self.level
    }
}

/// A stretch of a link ahead of a train: from `from` to `to` in the link's own coordinates (`a`
/// end = 0), starting `at` meters ahead of the train's front.
#[derive(Clone, Copy, Debug)]
struct Span {
    link: LinkId,
    from: f32,
    to: f32,
    at: f32,
}

impl Span {
    fn forward(&self) -> bool {
        self.to >= self.from
    }

    /// Distance from the train's front to the first point of `[lo, hi]` (link coordinates) on this
    /// span, or `None` when the interval is behind the span's start or past its end.
    fn reach(&self, lo: f32, hi: f32) -> Option<f32> {
        if self.forward() {
            (hi >= self.from && lo <= self.to).then(|| self.at + (lo - self.from).max(0.0))
        } else {
            (lo <= self.from && hi >= self.to).then(|| self.at + (self.from - hi).max(0.0))
        }
    }
}

/// The rail graph, its junctions and crossings, and the trains.
#[derive(Clone, Debug)]
pub struct Railway {
    net: Network,
    junctions: Vec<Junction>,
    crossings: Vec<Crossing>,
    stops: Vec<Stop>,
    /// Trains that left a station stop on the last tick, with the stop's cell.
    departures: Vec<(TrainId, Axial)>,
    /// Trains that came to rest at a buffer stop on the last tick, with the stop's pivot.
    arrivals: Vec<(TrainId, Vec2)>,
    /// A buffer stop for every rail lane end of the placed tiles, open or not.
    ends: Vec<EndStop>,
    trains: Vec<Train>,
    next_train: u64,
    /// The graph changed (or a line still wants a train): run the spawner on the next tick.
    dirty: bool,
    /// The graph changed since the spawner last ran (a line may have closed into a loop).
    changed: bool,
    ticks: u64,
    /// The world seed: picks each train's first destination.
    seed: u64,
}

/// How a train picks a new destination among the station stops.
#[derive(Clone, Copy, Debug)]
enum Pick {
    /// The stop at a seeded index, or the first reachable after it in stop order.
    First,
    /// The first reachable stop after this one in stop order, this one last.
    After(LinkId),
    /// The reachable stop with the cheapest route.
    Nearest,
}

/// A buffer stop at a rail lane end of a placed tile: the end's seam node and the stop's pose.
#[derive(Clone, Copy, Debug, PartialEq)]
struct EndStop {
    cell: Axial,
    edge: u8,
    node: NodeId,
    pose: Pose,
}

/// The vehicle in each [`CONSIST`] place for train `id` in a world seeded `seed`: the engine first,
/// then the carriage and the brake third in a seeded order (owner A13-Q2: a mix per train).
pub fn consist_for(seed: u64, id: u64) -> [VehicleKind; 3] {
    if mix(seed ^ CONSIST_SALT, id) & 1 == 0 {
        [
            VehicleKind::Engine,
            VehicleKind::Carriage,
            VehicleKind::BrakeCarriage,
        ]
    } else {
        [
            VehicleKind::Engine,
            VehicleKind::BrakeCarriage,
            VehicleKind::Carriage,
        ]
    }
}

/// SplitMix64's finaliser: a train's seeded start index among the stops.
fn mix(seed: u64, id: u64) -> u64 {
    let mut z = seed ^ id.wrapping_add(1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn link_of(net: &Network, id: LinkId) -> &crate::network::Link {
    net.link(id).expect("rail links outlive the trains on them")
}

/// The point and unit direction `s` meters along a polyline.
fn along(points: &[Vec2], s: f32) -> (Vec2, Vec2) {
    let mut left = s.max(0.0);
    for w in points.windows(2) {
        let l = geom::dist(w[0], w[1]);
        if l <= 0.0 {
            continue;
        }
        if left <= l {
            let dir = geom::scale(geom::sub(w[1], w[0]), 1.0 / l);
            return (geom::lerp(w[0], w[1], left / l), dir);
        }
        left -= l;
    }
    let n = points.len();
    let dir = if n >= 2 {
        let d = geom::sub(points[n - 1], points[n - 2]);
        geom::scale(d, 1.0 / geom::len(d).max(1e-6))
    } else {
        [1.0, 0.0]
    };
    (points.last().copied().unwrap_or([0.0, 0.0]), dir)
}

/// Where two polylines first cross: the point and its distance along `a`.
fn crossing_point(a: &[Vec2], b: &[Vec2]) -> Option<(Vec2, f32)> {
    let mut acc = 0.0;
    for wa in a.windows(2) {
        let d = geom::sub(wa[1], wa[0]);
        for wb in b.windows(2) {
            let e = geom::sub(wb[1], wb[0]);
            let den = geom::cross(d, e);
            if den.abs() < 1e-9 {
                continue;
            }
            let w = geom::sub(wb[0], wa[0]);
            let t = geom::cross(w, e) / den;
            let u = geom::cross(w, d) / den;
            if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
                return Some((geom::lerp(wa[0], wa[1], t), acc + t * geom::len(d)));
            }
        }
        acc += geom::len(d);
    }
    None
}

impl Default for Railway {
    fn default() -> Self {
        Self::new()
    }
}

impl Railway {
    pub fn new() -> Self {
        Self::with_seed(0)
    }

    /// An empty railway whose trains pick their first destinations from `seed` (the world seed).
    pub fn with_seed(seed: u64) -> Self {
        Self {
            net: Network::new(NetworkKind::Rail),
            junctions: vec![],
            crossings: vec![],
            stops: vec![],
            departures: vec![],
            arrivals: vec![],
            ends: vec![],
            trains: vec![],
            next_train: 0,
            dirty: false,
            changed: false,
            ticks: 0,
            seed,
        }
    }

    /// The stitched rail graph.
    pub fn network(&self) -> &Network {
        &self.net
    }

    fn net(&self) -> &Network {
        self.network()
    }

    pub fn trains(&self) -> &[Train] {
        &self.trains
    }

    pub fn junctions(&self) -> &[Junction] {
        &self.junctions
    }

    pub fn crossings(&self) -> &[Crossing] {
        &self.crossings
    }

    pub fn stops(&self) -> &[Stop] {
        &self.stops
    }

    /// Trains that left a station stop on the last tick, with the stop's cell (a departure
    /// whistle's cue).
    pub fn departures(&self) -> &[(TrainId, Axial)] {
        &self.departures
    }

    /// Trains that came to rest against a buffer stop on the last tick, with the stop's pivot (the
    /// buffer clunk's cue).
    pub fn line_end_arrivals(&self) -> &[(TrainId, Vec2)] {
        &self.arrivals
    }

    /// The buffer stops at the open line ends (a rail seam facing no rail), as (cell, world edge,
    /// pivot and the piece's +X): [`hex_core::rail_end`]'s rule in world space.
    pub fn buffer_stops(&self) -> Vec<(Axial, u8, Pose)> {
        self.ends
            .iter()
            .filter(|e| self.open_end(e.node))
            .map(|e| (e.cell, e.edge, e.pose))
            .collect()
    }

    fn open_end(&self, node: NodeId) -> bool {
        self.net.node(node).is_some_and(|n| n.links().len() == 1)
    }

    /// Cells of the crossings closed to cars this tick.
    pub fn closed_crossings(&self) -> Vec<Axial> {
        self.crossings
            .iter()
            .filter(|c| c.closed)
            .map(|c| c.cell)
            .collect()
    }

    /// Trains created so far, including removed ones.
    pub fn spawned(&self) -> u64 {
        self.next_train
    }

    /// Mirrors a placed tile: its rail segments join the graph, its switches and crossings are
    /// registered, and any train standing on the cell is removed.
    pub fn insert_tile(&mut self, cell: Axial, def: &TileDef, rotation: u8) {
        self.drop_trains_on(cell);
        self.forget_cell(cell);
        if self.net.insert_tile(cell, def, rotation) {
            self.dirty = true;
            self.changed = true;
        }
        let net = &self.net;
        for sw in &def.switches {
            let Some(node) = net.local_node(cell, &sw.node) else {
                continue;
            };
            let link = |i: u16| LinkId { cell, segment: i };
            let Some(toe) = net.link(link(sw.toe)) else {
                continue;
            };
            self.junctions.push(Junction {
                cell,
                node,
                toe: link(sw.toe),
                straight: link(sw.straight),
                branch: link(sw.branch),
                reach: (sw.fouling - toe.length()).max(0.0),
                set: Leg::Straight,
                next: Leg::Straight,
                holder: None,
                throws: 0,
                facing: vec![],
            });
        }
        self.junctions.sort_by_key(|j| (j.cell, j.node));
        let mut roads = Network::new(NetworkKind::Road);
        if roads.insert_tile(cell, def, rotation) {
            for (rail, link) in net.links_of(cell) {
                for (_, road) in roads.links() {
                    if let Some((at, s)) = crossing_point(link.points(), road.points()) {
                        self.crossings.push(Crossing {
                            cell,
                            rail,
                            at,
                            s,
                            closed: false,
                        });
                    }
                }
            }
        }
        self.crossings.sort_by_key(|c| (c.cell, c.rail));
        for stop in &def.stops {
            let link = LinkId {
                cell,
                segment: stop.segment,
            };
            if self.net.link(link).is_some() {
                self.stops.push(Stop {
                    cell,
                    link,
                    level: (stop.level[0], stop.level[1]),
                });
            }
        }
        self.stops.sort_by_key(|s| (s.cell, s.link));
        let centre = geom::cell_center(cell);
        for s in hex_core::rail_end::buffer_stops(def) {
            let edge = (s.edge + rotation) % 6;
            if let Some(node) = self.net.seam_node(cell, edge) {
                self.ends.push(EndStop {
                    cell,
                    edge,
                    node,
                    pose: Pose {
                        pos: geom::add(centre, geom::rotate(s.pos, rotation)),
                        dir: geom::rotate(s.dir, rotation),
                    },
                });
            }
        }
        self.ends.sort_by_key(|e| (e.cell, e.edge));
        self.replan_all();
    }

    /// Mirrors a bulldozed tile.
    pub fn remove_tile(&mut self, cell: Axial) {
        self.drop_trains_on(cell);
        self.forget_cell(cell);
        if self.net.remove_tile(cell) {
            self.dirty = true;
            self.changed = true;
        }
        self.replan_all();
    }

    /// After a world edit: every train replans to its destination, or picks another when the edit
    /// removed it or cut it off, so no route names a link that is gone.
    fn replan_all(&mut self) {
        for i in 0..self.trains.len() {
            self.trains[i].route.clear();
            let pick = self.trains[i].dest.map_or(Pick::First, Pick::After);
            self.route_on(i, pick);
        }
    }

    fn forget_cell(&mut self, cell: Axial) {
        self.junctions.retain(|j| j.cell != cell);
        self.crossings.retain(|c| c.cell != cell);
        self.stops.retain(|s| s.cell != cell);
        self.ends.retain(|e| e.cell != cell);
    }

    fn drop_trains_on(&mut self, cell: Axial) {
        let before = self.trains.len();
        self.trains
            .retain(|t| t.steps.iter().all(|s| s.link.cell != cell));
        let alive: BTreeSet<TrainId> = self.trains.iter().map(|t| t.id).collect();
        for j in &mut self.junctions {
            if j.holder.is_some_and(|h| !alive.contains(&h)) {
                j.holder = None;
            }
        }
        if self.trains.len() != before {
            self.dirty = true;
        }
    }

    fn junction_at(&self, node: NodeId) -> Option<usize> {
        self.junctions.iter().position(|j| j.node == node)
    }

    /// The link a train leaves `node` on, having arrived on `from`; `None` at a line end. At a
    /// toe the holder follows the switch, and anyone else plans on the leg its route wants
    /// (`want`), else on the junction's next leg.
    fn next_step(
        &self,
        node: NodeId,
        from: LinkId,
        train: TrainId,
        want: Option<LinkId>,
    ) -> Option<Step> {
        let net = self.net();
        let n = net.node(node)?;
        let link = if let Some(j) = self.junction_at(node).map(|i| &self.junctions[i]) {
            if from == j.toe {
                if j.holder == Some(train) {
                    j.leg(j.set)
                } else {
                    match want {
                        Some(w) if j.leg_of(w).is_some() => w,
                        _ => j.leg(j.next),
                    }
                }
            } else {
                j.toe
            }
        } else {
            match want {
                Some(w) if w != from && n.links().contains(&w) => w,
                _ => *n.links().iter().find(|&&l| l != from)?,
            }
        };
        let forward = link_of(net, link).ends().0 == node;
        Some(Step { link, forward })
    }

    /// Train `t`'s front step's index in its route, when the route lines up with it.
    fn route_cursor(t: &Train) -> Option<usize> {
        (t.route.first() == t.steps.first()).then_some(0)
    }

    /// The step train `t` takes after `step`, arriving at `node`: its route's next step while the
    /// route lines up (`k`, `step`'s index in the route, follows along), else the track's choice.
    fn follow(&self, t: &Train, node: NodeId, step: Step, k: &mut Option<usize>) -> Option<Step> {
        let want = k
            .and_then(|k| t.route.get(k + 1))
            .filter(|s| s.link != step.link)
            .map(|s| s.link);
        let next = self.next_step(node, step.link, t.id, want)?;
        *k = k
            .filter(|&k| t.route.get(k + 1) == Some(&next))
            .map(|k| k + 1);
        Some(next)
    }

    /// The leg train `t`'s route takes after the toe link `toe`, if its route passes there.
    fn route_after(t: &Train, toe: LinkId) -> Option<LinkId> {
        let k = t.route.iter().position(|s| s.link == toe)?;
        t.route.get(k + 1).map(|s| s.link).filter(|&l| l != toe)
    }

    /// The track ahead of a train's front, up to `dist` meters, and the distance to a line end and
    /// its node if one comes first.
    fn ahead(&self, t: &Train, dist: f32) -> (Vec<Span>, Option<(f32, NodeId)>) {
        let net = self.net();
        let mut spans = vec![];
        let mut step = t.steps[0];
        let mut k = Self::route_cursor(t);
        let mut at = 0.0;
        let mut into = t.head;
        loop {
            let link = link_of(net, step.link);
            let len = link.length();
            let (from, to) = if step.forward {
                (into, len)
            } else {
                (len - into, 0.0)
            };
            spans.push(Span {
                link: step.link,
                from,
                to,
                at,
            });
            at += len - into;
            if at >= dist {
                return (spans, None);
            }
            let node = if step.forward {
                link.ends().1
            } else {
                link.ends().0
            };
            match self.follow(t, node, step, &mut k) {
                Some(next) => {
                    step = next;
                    into = 0.0;
                }
                None => return (spans, Some((at, node))),
            }
        }
    }

    /// The stretches of links under a train's body, as (link, lo, hi) in link coordinates.
    pub fn body(&self, t: &Train) -> Vec<(LinkId, f32, f32)> {
        let net = self.net();
        let mut out = vec![];
        let mut left = TRAIN_LENGTH;
        for (i, step) in t.steps.iter().enumerate() {
            let len = link_of(net, step.link).length();
            let behind = if i == 0 { t.head } else { len };
            let take = behind.min(left);
            // In travel coordinates the body covers [behind - take, behind] of this step.
            let (a, b) = (behind - take, behind);
            let (lo, hi) = if step.forward {
                (a, b)
            } else {
                (len - b, len - a)
            };
            out.push((step.link, lo, hi));
            left -= take;
            if left <= 1e-4 {
                break;
            }
        }
        out
    }

    /// Where the point `s` (link coordinates) of `link` lies along train `t`'s path, measured from
    /// its front (m): ahead on `spans` (`Railway::ahead`) is positive, under the body negative.
    /// `None` when it is on neither.
    fn offset_of(&self, t: &Train, spans: &[Span], link: LinkId, s: f32) -> Option<f32> {
        for sp in spans.iter().filter(|sp| sp.link == link) {
            let (lo, hi) = if sp.forward() {
                (sp.from, sp.to)
            } else {
                (sp.to, sp.from)
            };
            if (lo..=hi).contains(&s) {
                return Some(sp.at + (s - sp.from).abs());
            }
        }
        let net = self.net();
        let mut behind = 0.0;
        for (i, step) in t.steps.iter().enumerate() {
            let len = link_of(net, step.link).length();
            // The travel coordinate on this step nearest the front.
            let near = if i == 0 { t.head } else { len };
            if step.link == link {
                let travel = if step.forward { s } else { len - s };
                if travel <= near {
                    return Some(-(behind + near - travel));
                }
            }
            behind += near;
        }
        None
    }

    /// The point `d` meters behind a train's front.
    fn point_behind(&self, t: &Train, d: f32) -> Vec2 {
        let net = self.net();
        let mut left = d.max(0.0);
        let mut last = [0.0, 0.0];
        for (i, step) in t.steps.iter().enumerate() {
            let link = link_of(net, step.link);
            let len = link.length();
            let behind = if i == 0 { t.head } else { len };
            let travel = behind - left.min(behind);
            let s = if step.forward { travel } else { len - travel };
            last = along(link.points(), s).0;
            if left <= behind {
                return last;
            }
            left -= behind;
        }
        last
    }

    /// The point `a` meters ahead of train `t`'s front along its path: on `spans`
    /// ([`Railway::ahead`]) for `a >= 0`, under its body behind the front for `a < 0`.
    fn path_point(&self, t: &Train, spans: &[Span], a: f32) -> Vec2 {
        if a < 0.0 {
            return self.point_behind(t, -a);
        }
        let net = self.net();
        let mut last = self.point_behind(t, 0.0);
        for sp in spans {
            let len = (sp.to - sp.from).abs();
            let k = (a - sp.at).clamp(0.0, len);
            let s = if sp.forward() {
                sp.from + k
            } else {
                sp.from - k
            };
            last = along(link_of(net, sp.link).points(), s).0;
            if a <= sp.at + len {
                break;
            }
        }
        last
    }

    /// How far inside a line end `d` meters ahead train `t`'s front stands when its leading
    /// buffers meet the stop `stop` (pivot, +X toward the edge): the leading vehicle's axles on the
    /// track and its body on their chord, as [`Railway::poses`] places it. On a straight that is
    /// [`END_MARGIN`]; on a curve the skewed body meets the contact plane with its foremost buffer
    /// head first, a little farther in.
    fn end_margin(&self, t: &Train, spans: &[Span], d: f32, stop: Pose) -> f32 {
        let v = if t.engine_leads {
            CONSIST[0]
        } else {
            CONSIST[CONSIST.len() - 1]
        };
        // The leading vehicle's centre behind the train's front, which is also its half length.
        let o = if t.engine_leads {
            v.centre
        } else {
            TRAIN_LENGTH - v.centre
        };
        let past = |m: f32| lead_buffers_past(self, t, spans, d - m, o, v.half_wheelbase, stop);
        let (mut lo, mut hi) = (0.0f32, 2.0 * END_MARGIN + o);
        if past(hi) > 0.0 {
            return END_MARGIN;
        }
        for _ in 0..32 {
            let m = 0.5 * (lo + hi);
            if past(m) > 0.0 {
                lo = m;
            } else {
                hi = m;
            }
        }
        hi
    }

    fn poses(&self, t: &Train) -> Vec<VehiclePose> {
        CONSIST
            .iter()
            .zip(t.kinds)
            .map(|(v, kind)| {
                let at = |o: f32| {
                    let d = if t.engine_leads { o } else { TRAIN_LENGTH - o };
                    self.point_behind(t, d)
                };
                // Toward the engine end of the train.
                let front = at(v.centre - v.half_wheelbase);
                let back = at(v.centre + v.half_wheelbase);
                let d = geom::sub(front, back);
                let l = geom::len(d).max(1e-6);
                VehiclePose {
                    kind,
                    pose: Pose {
                        pos: geom::lerp(front, back, 0.5),
                        dir: geom::scale(d, 1.0 / l),
                    },
                }
            })
            .collect()
    }

    /// Junction zone on `link` in link coordinates, if the link is part of junction `j`'s zone.
    fn zone_span(&self, j: &Junction, link: LinkId) -> Option<(f32, f32)> {
        let l = link_of(self.net(), link);
        if link == j.toe {
            Some((0.0, l.length()))
        } else if j.leg_of(link).is_some() {
            if l.ends().0 == j.node {
                Some((0.0, j.reach.min(l.length())))
            } else {
                Some(((l.length() - j.reach).max(0.0), l.length()))
            }
        } else {
            None
        }
    }

    /// Whether the body covers more than [`TOUCH`] of `[lo, hi]` on `link`. A train standing at a
    /// straight tile's buffer stop ends 1 cm past the tile's far seam (3.5 m + 24.22 m against
    /// 27.71 m); that touch must not foul a junction toe or a crossing across it.
    fn overlaps(body: &[(LinkId, f32, f32)], link: LinkId, lo: f32, hi: f32) -> bool {
        body.iter()
            .any(|&(l, a, b)| l == link && a <= hi - TOUCH && b >= lo + TOUCH)
    }

    /// Whether a train other than `except` has its body in junction `j`'s zone.
    fn zone_taken(&self, j: &Junction, except: TrainId) -> bool {
        self.trains.iter().filter(|t| t.id != except).any(|t| {
            let body = self.body(t);
            [j.toe, j.straight, j.branch].into_iter().any(|l| {
                self.zone_span(j, l)
                    .is_some_and(|(lo, hi)| Self::overlaps(&body, l, lo, hi))
            })
        })
    }

    /// The legs of junction `j` whose part of the fouling zone train `t`'s body is on.
    pub fn legs_under(&self, j: &Junction, t: &Train) -> Vec<Leg> {
        let body = self.body(t);
        [Leg::Straight, Leg::Branch]
            .into_iter()
            .filter(|&leg| {
                let l = j.leg(leg);
                self.zone_span(j, l)
                    .is_some_and(|(lo, hi)| Self::overlaps(&body, l, lo, hi))
            })
            .collect()
    }

    pub fn in_zone(&self, j: &Junction, t: &Train) -> bool {
        let body = self.body(t);
        [j.toe, j.straight, j.branch].into_iter().any(|l| {
            self.zone_span(j, l)
                .is_some_and(|(lo, hi)| Self::overlaps(&body, l, lo, hi))
        })
    }

    /// Advances one fixed tick: spawns trains where lines lack them, moves every train in id order,
    /// releases junctions the trains have left and updates the crossings.
    pub fn tick(&mut self) {
        self.ticks += 1;
        self.departures.clear();
        self.arrivals.clear();
        if self.dirty || self.ticks.is_multiple_of(40) {
            self.spawn();
        }
        let mut warned: BTreeSet<usize> = BTreeSet::new();
        for i in 0..self.trains.len() {
            self.move_train(i, &mut warned);
        }
        self.resolve_stuck();
        self.release();
        for (k, c) in self.crossings.iter_mut().enumerate() {
            c.closed = warned.contains(&k);
        }
        let occupied: Vec<usize> = (0..self.crossings.len())
            .filter(|&k| {
                let c = &self.crossings[k];
                self.trains.iter().any(|t| {
                    Self::overlaps(
                        &self.body(t),
                        c.rail,
                        c.s - CROSSING_HALF,
                        c.s + CROSSING_HALF,
                    )
                })
            })
            .collect();
        for k in occupied {
            self.crossings[k].closed = true;
        }
    }

    fn move_train(&mut self, i: usize, warned: &mut BTreeSet<usize>) {
        let id = self.trains[i].id;
        let (spans, line_end) = self.ahead(&self.trains[i], LOOKAHEAD);
        let mut limit = LOOKAHEAD;
        let mut blocker = Blocker::Clear;
        let mut end_pivot = None;
        let mut end_limit = None;
        if let Some((d, node)) = line_end {
            let stop = self.ends.iter().find(|e| e.node == node).map(|e| e.pose);
            end_pivot = stop.map(|p| p.pos);
            let margin = stop.map_or(END_MARGIN, |p| {
                self.end_margin(&self.trains[i], &spans, d, p)
            });
            let d = d - margin;
            end_limit = Some(d.max(0.0));
            if d < limit {
                limit = d;
                blocker = Blocker::LineEnd;
            }
        }
        for u in self.trains.iter().filter(|u| u.id != id) {
            for (link, lo, hi) in self.body(u) {
                for sp in spans.iter().filter(|sp| sp.link == link) {
                    if let Some(d) = sp.reach(lo, hi) {
                        let d = d - TRAIN_GAP;
                        if d < limit {
                            limit = d;
                            blocker = Blocker::Train(u.id);
                        }
                    }
                }
            }
        }
        // Stations: brake to stand with the axles centred on the level section, unless this
        // pass already waited there. Before the junctions, so none is reserved past a stop.
        let t = &self.trains[i];
        if let Some(l) = t.served
            && !self.body(t).iter().any(|&(b, ..)| b == l)
        {
            self.trains[i].served = None;
        }
        let t = &self.trains[i];
        let (lo, hi) = axle_span();
        let mid = (lo + hi) / 2.0;
        let behind = if t.engine_leads {
            mid
        } else {
            TRAIN_LENGTH - mid
        };
        let mut stop_link = None;
        let mut station_at_end = false;
        for st in self.stops.iter().filter(|s| t.served != Some(s.link)) {
            let centre = (st.level.0 + st.level.1) / 2.0;
            let Some(at) = self.offset_of(t, &spans, st.link, centre) else {
                continue;
            };
            let mut go = at + behind;
            // A terminal station: the buffer stop holds the train short of centring on the level
            // section, so it stands at the stop while its axles are on the section there.
            let mut at_end = false;
            if let Some(end) = end_limit
                && go > end
                && let (Some(a), Some(b)) = (
                    self.offset_of(t, &spans, st.link, st.level.0),
                    self.offset_of(t, &spans, st.link, st.level.1),
                )
                && a.min(b) + behind <= end
            {
                go = end;
                at_end = true;
            }
            if go >= -STOP_EPS && (go.max(0.0) < limit || at_end && go <= limit + 1e-4) {
                limit = go.max(0.0);
                blocker = Blocker::Station(st.cell);
                stop_link = Some(st.link);
                station_at_end = at_end;
            }
        }
        // Junctions: reserve the first zone ahead that this train does not hold, or stand short.
        let mut seen: BTreeSet<usize> = BTreeSet::new();
        for (n, sp) in spans.iter().enumerate() {
            // The leg this train plans on past a facing toe here: the span after it (its lookahead
            // follows its route), else its route's, else none.
            let planned = spans
                .get(n + 1)
                .map(|s| s.link)
                .or_else(|| Self::route_after(&self.trains[i], sp.link));
            for k in 0..self.junctions.len() {
                if seen.contains(&k) {
                    continue;
                }
                let j = &self.junctions[k];
                let Some((lo, hi)) = self.zone_span(j, sp.link) else {
                    continue;
                };
                let Some(d) = sp.reach(lo, hi) else {
                    continue;
                };
                seen.insert(k);
                if j.holder == Some(id) {
                    // Held from an earlier approach (round a balloon loop shorter than the
                    // lookahead, or before a turn round): while the zone is clear of it, set the
                    // switch for the way it comes now, so it never runs through one set against it.
                    // Facing, its lookahead follows the switch it holds, so ask its route first.
                    let leg = j.leg_of(sp.link).or_else(|| {
                        Self::route_after(&self.trains[i], sp.link)
                            .or(planned)
                            .and_then(|l| j.leg_of(l))
                    });
                    if let Some(leg) = leg
                        && leg != j.set
                        && !self.in_zone(j, &self.trains[i])
                    {
                        let j = &mut self.junctions[k];
                        j.set = leg;
                        j.throws += 1;
                    }
                    continue;
                }
                if d >= limit {
                    continue;
                }
                if j.holder.is_none() && !self.zone_taken(j, id) {
                    let j = &mut self.junctions[k];
                    j.holder = Some(id);
                    let leg = match j.leg_of(sp.link) {
                        Some(trailing) => trailing,
                        None => {
                            let leg = planned.and_then(|l| j.leg_of(l)).unwrap_or(j.next);
                            j.facing.push(leg);
                            if j.facing.len() > 16 {
                                j.facing.remove(0);
                            }
                            j.next = leg.other();
                            let t = &mut self.trains[i];
                            t.routes.push((j.cell, leg));
                            if t.routes.len() > 16 {
                                t.routes.remove(0);
                            }
                            leg
                        }
                    };
                    if j.set != leg {
                        j.set = leg;
                        j.throws += 1;
                    }
                } else {
                    let d = d - ZONE_MARGIN;
                    if d < limit {
                        limit = d;
                        blocker = Blocker::Junction(self.junctions[k].cell);
                    }
                }
            }
        }
        for sp in &spans {
            for (k, c) in self.crossings.iter().enumerate() {
                if c.rail == sp.link
                    && sp
                        .reach(c.s - CROSSING_HALF, c.s + CROSSING_HALF)
                        .is_some_and(|d| d <= CROSSING_WARN)
                {
                    warned.insert(k);
                }
            }
        }

        let limit = limit.max(0.0);
        let t = &self.trains[i];
        let v = (t.speed + ACCEL * TICK_SECONDS)
            .min(TRAIN_SPEED)
            .min((2.0 * BRAKE * limit).sqrt());
        let ds = (v * TICK_SECONDS).min(limit);
        if !self.advance(i, ds) {
            // Off its route (a switch set another way under a holder, a stop it ran past): replan.
            let pick = self.trains[i].dest.map_or(Pick::First, Pick::After);
            self.route_on(i, pick);
        }
        let t = &mut self.trains[i];
        // Came to rest on this tick from running (not standing since a spawn or a reversal).
        let at_stop = blocker == Blocker::LineEnd || station_at_end;
        let arrived = ds < 1e-4 && t.stopped == 0 && t.speed * TICK_SECONDS >= 1e-4 && at_stop;
        t.speed = ds / TICK_SECONDS;
        if arrived {
            let at = end_pivot.unwrap_or_else(|| t.front());
            self.arrivals.push((id, at));
        }
        let t = &mut self.trains[i];
        if ds < 1e-4 {
            t.stopped += 1;
            t.blocker = blocker;
        } else {
            t.stopped = 0;
            t.blocker = Blocker::Clear;
        }
        let dwell = (DWELL_SECONDS / TICK_SECONDS) as u32;
        let give_up = (GIVE_UP_SECONDS / TICK_SECONDS) as u32;
        if let Blocker::Station(cell) = t.blocker
            && t.stopped >= (STATION_DWELL_SECONDS / TICK_SECONDS) as u32
        {
            t.served = stop_link;
            t.stops += 1;
            t.blocker = Blocker::Clear;
            t.stopped = 0;
            self.departures.push((id, cell));
            if let Some(stop) = stop_link {
                self.depart(i, stop);
            }
            return;
        }
        let (blocker, stopped) = (t.blocker, t.stopped);
        match blocker {
            Blocker::LineEnd if stopped >= dwell => {
                self.reverse(i);
                let pick = self.trains[i].dest.map_or(Pick::First, Pick::After);
                self.route_on(i, pick);
            }
            Blocker::Train(_) | Blocker::Junction(_) if stopped >= give_up => {
                self.give_way(i);
            }
            _ => {}
        }
    }

    /// Train `i` leaves the station stop on `stop`: at its destination it takes the next station
    /// in turn, and it turns round here when its route back is shorter by more than
    /// [`REVERSE_COST`] (never toward the stop it is leaving, nor on a stretch with no line end).
    fn depart(&mut self, i: usize, stop: LinkId) {
        let t = &self.trains[i];
        let arrived = t.dest.is_none_or(|d| d == stop);
        if arrived && t.dest.is_some() {
            let t = &mut self.trains[i];
            t.arrivals.push(stop.cell);
            t.yields = 0;
            if t.arrivals.len() > 16 {
                t.arrivals.remove(0);
            }
        }
        if arrived {
            self.retarget(i, Pick::After(stop), Some(stop));
        } else {
            self.route_with_turn(i, Some(stop));
        }
    }

    /// Train `i` turns round to give way (nose to nose, or blocked too long). The first time it
    /// keeps its destination and replans, which clears a shared stretch for the other train to
    /// pass; giving way again before it reaches its destination it takes the nearest station by
    /// route from its new heading instead, so two trains that can never pass each other (a single
    /// line) still each reach stations rather than turning each other back for good.
    fn give_way(&mut self, i: usize) {
        self.reverse(i);
        let t = &mut self.trains[i];
        t.yields += 1;
        if t.yields >= 2 {
            t.yields = 0;
            self.retarget(i, Pick::Nearest, None);
        } else {
            let pick = t.dest.map_or(Pick::First, Pick::After);
            self.route_on(i, pick);
        }
    }

    /// Plans train `i`'s route to its destination as it heads, or picks a new destination with
    /// `pick` when it has none or the destination is out of reach.
    fn route_on(&mut self, i: usize, pick: Pick) {
        let t = &self.trains[i];
        let turns = !self.on_closed_line(t);
        match t.dest.and_then(|d| self.plan(t.steps[0], t.head, d, turns)) {
            Some((route, _)) => self.trains[i].route = route,
            None => self.retarget(i, pick, None),
        }
    }

    /// [`Railway::route_on`] for a train standing at the stop on `stop`, which may also turn round
    /// there first.
    fn route_with_turn(&mut self, i: usize, stop: Option<LinkId>) {
        let t = &self.trains[i];
        let Some(dest) = t.dest else {
            return self.retarget(i, Pick::First, stop);
        };
        match self.best_plan(t, dest, stop) {
            Some((_, route, turn)) => self.take(i, dest, route, turn, stop),
            None => self.retarget(i, Pick::After(dest), stop),
        }
    }

    /// Sets train `i`'s destination and route, turning it round first when `turn` (at the stop on
    /// `stop`, which it then does not stop at again this pass).
    fn take(&mut self, i: usize, dest: LinkId, route: Vec<Step>, turn: bool, stop: Option<LinkId>) {
        if turn {
            self.reverse(i);
            self.trains[i].served = stop;
        }
        let t = &mut self.trains[i];
        t.dest = Some(dest);
        t.route = route;
    }

    /// Picks train `i`'s destination with `pick` among the reachable stops and plans its route;
    /// with none reachable it has no destination and follows the track. Standing at the stop on
    /// `stop`, it may turn round there toward any other stop.
    fn retarget(&mut self, i: usize, pick: Pick, stop: Option<LinkId>) {
        let n = self.stops.len();
        let t = &self.trains[i];
        let order: Vec<usize> = match pick {
            _ if n == 0 => vec![],
            Pick::First => {
                let start = (mix(self.seed, t.id.0) % n as u64) as usize;
                (0..n).map(|k| (start + k) % n).collect()
            }
            Pick::After(l) => match self.stops.iter().position(|s| s.link == l) {
                Some(at) => (1..=n).map(|k| (at + k) % n).collect(),
                None => {
                    let start = (mix(self.seed, t.id.0) % n as u64) as usize;
                    (0..n).map(|k| (start + k) % n).collect()
                }
            },
            // A train giving way for good takes the nearest station no other train stands at: on a
            // stretch of dead ends the train it yields to is often at the nearest one, and heading
            // there again only repeats the meeting (two trains swapping terminals across one
            // headshunt never pass). Only when every station is held does it take a held one.
            Pick::Nearest => {
                let held = |k: usize| {
                    let l = self.stops[k].link;
                    self.trains
                        .iter()
                        .filter(|u| u.id != t.id)
                        .any(|u| self.body(u).iter().any(|&(b, ..)| b == l))
                };
                let free: Vec<usize> = (0..n).filter(|&k| !held(k)).collect();
                if free.is_empty() {
                    (0..n).collect()
                } else {
                    free
                }
            }
        };
        let mut best: Option<(f32, LinkId, Vec<Step>, bool)> = None;
        for k in order {
            let dest = self.stops[k].link;
            let Some((cost, route, turn)) = self.best_plan(t, dest, stop) else {
                continue;
            };
            if best.as_ref().is_none_or(|b| cost < b.0) {
                best = Some((cost, dest, route, turn));
            }
            if !matches!(pick, Pick::Nearest) {
                break;
            }
        }
        match best {
            Some((_, dest, route, turn)) => self.take(i, dest, route, turn, stop),
            None => {
                let t = &mut self.trains[i];
                t.dest = None;
                t.route.clear();
            }
        }
    }

    /// Train `t`'s cheapest route to the stop on `dest`: (cost, route, turn round first). Standing
    /// at the stop on `stop`, it may turn round there unless `dest` is that stop or the stretch has
    /// no line end.
    fn best_plan(
        &self,
        t: &Train,
        dest: LinkId,
        stop: Option<LinkId>,
    ) -> Option<(f32, Vec<Step>, bool)> {
        let turns = !self.on_closed_line(t);
        let ahead = self
            .plan(t.steps[0], t.head, dest, turns)
            .map(|(r, c)| (c, r, false));
        let back = (turns && stop.is_some_and(|s| s != dest))
            .then(|| {
                let b = turned(&self.net, t);
                self.plan(b.steps[0], b.head, dest, turns)
                    .map(|(r, c)| (c + REVERSE_COST, r, true))
            })
            .flatten();
        match (ahead, back) {
            (Some(a), Some(b)) => Some(if b.0 < a.0 { b } else { a }),
            (a, b) => a.or(b),
        }
    }

    /// Whether train `t` runs on a stretch with no line end (a loop, or loops joined).
    fn on_closed_line(&self, t: &Train) -> bool {
        let net = self.net();
        let mut seen: BTreeSet<LinkId> = BTreeSet::new();
        let mut stack = vec![t.steps[0].link];
        seen.insert(t.steps[0].link);
        while let Some(l) = stack.pop() {
            let (a, b) = link_of(net, l).ends();
            for n in [a, b] {
                let Some(node) = net.node(n) else {
                    return false;
                };
                if node.links().len() < 2 {
                    return false;
                }
                for &m in node.links() {
                    if seen.insert(m) {
                        stack.push(m);
                    }
                }
            }
        }
        true
    }

    /// A* over directed links from a front `head` meters along `start` to the stop on `dest`:
    /// the route (front link first, ending on `dest`) and its cost in meters of running plus
    /// [`REVERSE_COST`] per turn. A route turns round at a line end, and at a station stop other
    /// than on the start link when `station_turns`; it arrives on entering `dest` from a node or
    /// turning onto it at a line end, never by starting on it. Costs are whole millimetres and
    /// ties break on the link order, so a plan is the same every run.
    pub fn plan(
        &self,
        start: Step,
        head: f32,
        dest: LinkId,
        station_turns: bool,
    ) -> Option<(Vec<Step>, f32)> {
        type Key = (LinkId, bool);
        let net = self.net();
        let stop = self.stops.iter().find(|s| s.link == dest)?;
        let centre = (stop.level.0 + stop.level.1) / 2.0;
        let dl = net.link(dest)?;
        let dlen = dl.length();
        let goals: Vec<Vec2> = [dl.ends().0, dl.ends().1]
            .iter()
            .filter_map(|&n| net.node(n).map(|n| n.pos()))
            .collect();
        let mm = |m: f32| (m.max(0.0) * 1000.0).round() as u64;
        let len = |l: LinkId| link_of(net, l).length();
        let exit = |k: Key| {
            let (a, b) = link_of(net, k.0).ends();
            if k.1 { b } else { a }
        };
        let h = |k: Key| {
            let p = net.node(exit(k)).map_or([0.0, 0.0], |n| n.pos());
            mm(goals
                .iter()
                .map(|&q| geom::dist(p, q))
                .fold(f32::MAX, f32::min))
        };
        let stop_at: BTreeMap<LinkId, f32> = self
            .stops
            .iter()
            .map(|s| (s.link, (s.level.0 + s.level.1) / 2.0))
            .collect();
        let first: Key = (start.link, start.forward);
        let mut best: BTreeMap<Key, u64> = BTreeMap::new();
        let mut from: BTreeMap<Key, Key> = BTreeMap::new();
        // Entries (f, g, state); `None` is the goal.
        let mut heap: BinaryHeap<Reverse<(u64, u64, Option<Key>)>> = BinaryHeap::new();
        let g0 = mm(len(start.link) - head);
        best.insert(first, g0);
        heap.push(Reverse((g0 + h(first), g0, Some(first))));
        let mut goal: Option<(u64, Key, bool)> = None;
        while let Some(Reverse((_, g, k))) = heap.pop() {
            let Some(k) = k else {
                break;
            };
            if best.get(&k).is_some_and(|&b| b < g) {
                continue;
            }
            let node = exit(k);
            let onward = self.onward(node, k.0);
            let mut next: Vec<(Key, u64)> = vec![];
            let mut arrive = |cost: u64, turned: bool, heap: &mut BinaryHeap<_>| {
                if goal.is_none_or(|(c, ..)| cost < c) {
                    goal = Some((cost, k, turned));
                    heap.push(Reverse((cost, cost, None)));
                }
            };
            if onward.is_empty() {
                // A line end: turn round and run back along the link.
                let back = (k.0, !k.1);
                if k.0 == dest {
                    let to = if back.1 { centre } else { dlen - centre };
                    arrive(g + mm(REVERSE_COST + to), true, &mut heap);
                } else {
                    next.push((back, g + mm(REVERSE_COST + len(k.0))));
                }
            }
            for m in onward {
                let fwd = link_of(net, m).ends().0 == node;
                if m == dest {
                    let to = if fwd { centre } else { dlen - centre };
                    arrive(g + mm(to), false, &mut heap);
                } else {
                    next.push(((m, fwd), g + mm(len(m))));
                }
            }
            if station_turns
                && k.0 != start.link
                && let Some(&c) = stop_at.get(&k.0)
            {
                // Turn round standing at the stop: back from the exit to the stop, then out the
                // other end.
                let l = len(k.0);
                let (behind, out) = if k.1 { (l - c, c) } else { (c, l - c) };
                let g2 = g.saturating_sub(mm(behind)) + mm(REVERSE_COST + out);
                next.push(((k.0, !k.1), g2));
            }
            for (k2, g2) in next {
                if best.get(&k2).is_none_or(|&b| g2 < b) {
                    best.insert(k2, g2);
                    from.insert(k2, k);
                    heap.push(Reverse((g2 + h(k2), g2, Some(k2))));
                }
            }
        }
        let (cost, last, turned) = goal?;
        let mut keys = vec![last];
        let mut at = last;
        while at != first {
            at = *from.get(&at)?;
            keys.push(at);
        }
        keys.reverse();
        let mut route: Vec<Step> = keys
            .into_iter()
            .map(|(link, forward)| Step { link, forward })
            .collect();
        let end = if turned {
            Step {
                link: dest,
                forward: !last.1,
            }
        } else {
            Step {
                link: dest,
                forward: link_of(net, dest).ends().0 == exit(last),
            }
        };
        route.push(end);
        Some((route, cost as f32 / 1000.0))
    }

    /// Meters of track along train `t`'s route from its front to its destination's stop, or
    /// `None` without a destination.
    pub fn route_length(&self, t: &Train) -> Option<f32> {
        let dest = t.dest?;
        let stop = self.stops.iter().find(|s| s.link == dest)?;
        let centre = (stop.level.0 + stop.level.1) / 2.0;
        let net = self.net();
        let n = t.route.len();
        if n == 0 || t.route[0] != t.steps[0] {
            return None;
        }
        let mut m = 0.0;
        for (k, s) in t.route.iter().enumerate() {
            let l = net.link(s.link)?.length();
            let (from, to) = (if k == 0 { t.head } else { 0.0 }, l);
            if k + 1 == n {
                let c = if s.forward { centre } else { l - centre };
                m += (c - from).max(0.0);
            } else {
                m += to - from;
            }
        }
        Some(m)
    }

    /// Two trains stopped nose to nose: the higher id runs back.
    fn resolve_stuck(&mut self) {
        let stuck = (STUCK_SECONDS / TICK_SECONDS) as u32;
        let mut back = vec![];
        for t in &self.trains {
            let Blocker::Train(other) = t.blocker else {
                continue;
            };
            if t.stopped < stuck {
                continue;
            }
            let Some(u) = self.trains.iter().find(|u| u.id == other) else {
                continue;
            };
            if u.blocker != Blocker::Train(t.id) || u.stopped < stuck || t.id > other {
                continue;
            }
            // Nose to nose: the train with more free track behind it runs back (the higher id on
            // a tie), so a train standing at a line end is never the one asked to back away.
            let (mine, theirs) = (self.room_behind(t), self.room_behind(u));
            back.push(if theirs >= mine { u.id } else { t.id });
        }
        for id in back {
            if let Some(i) = self.trains.iter().position(|t| t.id == id) {
                self.give_way(i);
            }
        }
    }

    /// Releases every junction whose holder has left its zone and is not heading into it.
    fn release(&mut self) {
        for k in 0..self.junctions.len() {
            let Some(h) = self.junctions[k].holder else {
                continue;
            };
            let keep = self.trains.iter().find(|t| t.id == h).is_some_and(|t| {
                let j = &self.junctions[k];
                self.in_zone(j, t)
                    || self.ahead(t, LOOKAHEAD).0.iter().any(|sp| {
                        self.zone_span(j, sp.link)
                            .is_some_and(|(lo, hi)| sp.reach(lo, hi).is_some())
                    })
            });
            if !keep {
                self.junctions[k].holder = None;
            }
        }
    }

    /// Moves train `i`'s front `ds` meters along the track and trims the links it has left.
    /// Returns whether the train is still on its route (always, without one).
    fn advance(&mut self, i: usize, ds: f32) -> bool {
        self.trains[i].head += ds;
        let mut k = Self::route_cursor(&self.trains[i]);
        loop {
            let t = &self.trains[i];
            let step = t.steps[0];
            let link = link_of(self.net(), step.link);
            let len = link.length();
            if t.head <= len {
                break;
            }
            let node = if step.forward {
                link.ends().1
            } else {
                link.ends().0
            };
            let Some(next) = self.follow(t, node, step, &mut k) else {
                self.trains[i].head = len;
                break;
            };
            let t = &mut self.trains[i];
            t.head -= len;
            t.steps.insert(0, next);
        }
        self.trim(i);
        let poses = self.poses(&self.trains[i]);
        let t = &mut self.trains[i];
        t.prev = std::mem::replace(&mut t.now, poses);
        match k {
            Some(k) => {
                t.route.drain(..k);
                true
            }
            None => t.dest.is_none(),
        }
    }

    /// Drops the links behind train `i`'s tail.
    fn trim(&mut self, i: usize) {
        trim(&self.net, &mut self.trains[i]);
    }

    /// Turns train `i` round where it stands: its tail becomes its front.
    fn reverse(&mut self, i: usize) {
        let mut t = turned(&self.net, &self.trains[i]);
        t.reversals += 1;
        t.served = None;
        let poses = self.poses(&t);
        t.now = poses.clone();
        t.prev = poses;
        self.trains[i] = t;
        // A junction it held ahead was reserved for the way it no longer runs: let it go, so the
        // switch is set again for its new path when it next reserves (one it stands in keeps).
        let t = &self.trains[i];
        let free: Vec<usize> = (0..self.junctions.len())
            .filter(|&k| {
                let j = &self.junctions[k];
                j.holder == Some(t.id) && !self.in_zone(j, t)
            })
            .collect();
        for k in free {
            self.junctions[k].holder = None;
        }
    }

    /// Free track behind train `t`: how far it could run back before a line end or another train.
    fn room_behind(&self, t: &Train) -> f32 {
        let back = turned(&self.net, t);
        let (spans, end) = self.ahead(&back, LOOKAHEAD);
        let mut room = end.map_or(LOOKAHEAD, |(d, _)| d);
        for u in self.trains.iter().filter(|u| u.id != t.id) {
            for (link, lo, hi) in self.body(u) {
                for sp in spans.iter().filter(|sp| sp.link == link) {
                    if let Some(d) = sp.reach(lo, hi) {
                        room = room.min(d);
                    }
                }
            }
        }
        room
    }

    /// Connected stretches of track: each component's links in id order and its length.
    fn lines(&self) -> Vec<(Vec<LinkId>, f32)> {
        let net = self.net();
        let mut seen: BTreeSet<LinkId> = BTreeSet::new();
        let mut out = vec![];
        for (start, _) in net.links() {
            if seen.contains(&start) {
                continue;
            }
            let mut comp = vec![];
            let mut stack = vec![start];
            seen.insert(start);
            while let Some(l) = stack.pop() {
                comp.push(l);
                let (a, b) = link_of(net, l).ends();
                for n in [a, b] {
                    for &m in net.node(n).map(|n| n.links()).into_iter().flatten() {
                        if seen.insert(m) {
                            stack.push(m);
                        }
                    }
                }
            }
            comp.sort();
            let len = comp.iter().map(|&l| link_of(net, l).length()).sum();
            out.push((comp, len));
        }
        out
    }

    /// The links a train arriving at `node` on `from` may run on next: the switch's two legs from
    /// the toe, the toe from either leg, and anywhere else every other link at the node.
    fn onward(&self, node: NodeId, from: LinkId) -> Vec<LinkId> {
        if let Some(j) = self.junction_at(node).map(|i| &self.junctions[i]) {
            if from == j.toe {
                vec![j.straight, j.branch]
            } else {
                vec![j.toe]
            }
        } else {
            self.net()
                .node(node)
                .map(|n| n.links().iter().copied().filter(|&l| l != from).collect())
                .unwrap_or_default()
        }
    }

    /// A running direction for every link of the connected stretch holding `start.link` (`true`
    /// from `a` to `b`), from one walk along it starting on `start`, so that trains laid that way
    /// all run the same way: on a loop they all lap the same way round, whatever way each link was
    /// stitched.
    fn heading(&self, start: Step) -> BTreeMap<LinkId, bool> {
        let net = self.net();
        let mut out = BTreeMap::new();
        out.insert(start.link, start.forward);
        let mut stack = vec![start.link];
        while let Some(l) = stack.pop() {
            let (a, b) = link_of(net, l).ends();
            let (exit, entry) = if out[&l] { (b, a) } else { (a, b) };
            // Leaving `l` at its exit, the next link runs away from that node; arriving at its
            // entry, the link before runs toward it.
            for (node, leaving) in [(exit, true), (entry, false)] {
                for m in self.onward(node, l) {
                    let away = link_of(net, m).ends().0 == node;
                    out.entry(m).or_insert_with(|| {
                        stack.push(m);
                        away == leaving
                    });
                }
            }
        }
        out
    }

    /// Whether a connected stretch has no line end: every node of its links joins two or more.
    fn closed(&self, links: &[LinkId]) -> bool {
        let net = self.net();
        links.iter().all(|&l| {
            let (a, b) = link_of(net, l).ends();
            [a, b]
                .into_iter()
                .all(|n| net.node(n).is_some_and(|n| n.links().len() >= 2))
        })
    }

    fn spawn(&mut self) {
        self.dirty = false;
        let changed = std::mem::take(&mut self.changed);
        let lines = self.lines();
        for (links, len) in lines {
            if self.trains.len() >= MAX_TRAINS || len < MIN_TRACK {
                continue;
            }
            let want =
                (1 + ((len - MIN_TRACK) / TRACK_PER_TRAIN) as usize).min(MAX_TRAINS_PER_LINE);
            let set: BTreeSet<LinkId> = links.iter().copied().collect();
            let on: Vec<usize> = (0..self.trains.len())
                .filter(|&i| set.contains(&self.trains[i].steps[0].link))
                .collect();
            let have = on.len();
            // The walk follows the oldest train already here, else starts forward on the first link.
            let start = on.first().map_or(
                Step {
                    link: links[0],
                    forward: true,
                },
                |&i| self.trains[i].steps[0],
            );
            let heading = self.heading(start);
            // Stretches that each had a train can close into a loop with their trains running
            // opposite ways round: on a line with no end, those running against the oldest turn
            // round where they stand. Only after a graph change, so a train that turned round to
            // give way keeps its new heading.
            if changed && self.closed(&links) {
                for &i in &on {
                    let s = self.trains[i].steps[0];
                    if heading.get(&s.link).is_some_and(|&f| f != s.forward) {
                        self.reverse(i);
                        let pick = self.trains[i].dest.map_or(Pick::First, Pick::After);
                        self.route_on(i, pick);
                    }
                }
            }
            for _ in have..want {
                if self.trains.len() >= MAX_TRAINS || !self.spawn_on(&links, &heading) {
                    break;
                }
            }
        }
    }
    /// Places a new train on the first link of `links` (in id order) where its whole body fits
    /// clear of other trains and of every junction zone, running the way `heading` gives that
    /// link. Returns whether one was placed.
    fn spawn_on(&mut self, links: &[LinkId], heading: &BTreeMap<LinkId, bool>) -> bool {
        let id = TrainId(self.next_train);
        for &start in links {
            let step = Step {
                link: start,
                forward: heading.get(&start).copied().unwrap_or(true),
            };
            // The tail just inside the link's end the train runs from, the front a train length
            // on, with clear track for the gap at both ends.
            let Some(mut t) = self.lay(id, step, END_MARGIN, TRAIN_LENGTH + 2.0 * TRAIN_GAP) else {
                continue;
            };
            t.head -= TRAIN_GAP;
            while t.head < 0.0 && t.steps.len() > 1 {
                t.steps.remove(0);
                t.head += link_of(&self.net, t.steps[0].link).length();
            }
            if t.head < 0.0 {
                continue;
            }
            trim(&self.net, &mut t);
            let mine: Vec<_> = self
                .body(&t)
                .iter()
                .map(|&(l, lo, hi)| (l, lo - TRAIN_GAP, hi + TRAIN_GAP))
                .collect();
            let clash = self.trains.iter().any(|u| {
                self.body(u)
                    .iter()
                    .any(|&(l, lo, hi)| Self::overlaps(&mine, l, lo, hi))
            }) || self.junctions.iter().any(|j| {
                [j.toe, j.straight, j.branch].into_iter().any(|l| {
                    self.zone_span(j, l)
                        .is_some_and(|(lo, hi)| Self::overlaps(&mine, l, lo, hi))
                })
            });
            if clash {
                continue;
            }
            self.add(t);
            return true;
        }
        false
    }

    /// Places a train with its front `front` meters along `link` (from its `a` end), heading toward
    /// `b` when `forward`, its body on the track behind. `None` when the track behind is too short.
    /// For scripted layouts and tests; the spawner places trains itself.
    pub fn spawn_at(&mut self, link: LinkId, forward: bool, front: f32) -> Option<TrainId> {
        let len = self.net.link(link)?.length();
        let id = TrainId(self.next_train);
        // Lay the train out the other way from the front, then turn it round.
        let back = Step {
            link,
            forward: !forward,
        };
        let into = if forward { len - front } else { front };
        let t = self.lay(id, back, into, TRAIN_LENGTH)?;
        let mut t = turned(&self.net, &t);
        t.engine_leads = true;
        self.add(t);
        self.dirty = false;
        Some(id)
    }

    /// Sends train `id` to the station stop on `cell` and plans its route there, as it heads.
    /// Returns whether the stop exists and is in reach. For scripted layouts and tests; trains
    /// pick their own destinations.
    pub fn send(&mut self, id: TrainId, cell: Axial) -> bool {
        let Some(i) = self.trains.iter().position(|t| t.id == id) else {
            return false;
        };
        let Some(dest) = self.stops.iter().find(|s| s.cell == cell).map(|s| s.link) else {
            return false;
        };
        let t = &self.trains[i];
        let turns = !self.on_closed_line(t);
        match self.plan(t.steps[0], t.head, dest, turns) {
            Some((route, _)) => {
                self.take(i, dest, route, false, None);
                true
            }
            None => false,
        }
    }

    /// A train at rest whose front starts `into` meters along `step` and walks `length` meters on
    /// (switches as set), or `None` when a line end comes first.
    fn lay(&self, id: TrainId, step: Step, into: f32, length: f32) -> Option<Train> {
        let net = &self.net;
        let mut t = Train {
            id,
            steps: vec![step],
            head: into.min(link_of(net, step.link).length()),
            speed: 0.0,
            engine_leads: true,
            blocker: Blocker::Clear,
            stopped: 0,
            kinds: consist_for(self.seed, id.0),
            prev: vec![],
            now: vec![],
            routes: vec![],
            reversals: 0,
            served: None,
            stops: 0,
            dest: None,
            route: vec![],
            arrivals: vec![],
            yields: 0,
        };
        let mut need = length;
        loop {
            let step = t.steps[0];
            let l = link_of(net, step.link);
            let room = l.length() - t.head;
            if room >= need {
                t.head += need;
                return Some(t);
            }
            need -= room;
            let node = if step.forward { l.ends().1 } else { l.ends().0 };
            let next = self.next_step(node, step.link, id, None)?;
            t.steps.insert(0, next);
            t.head = 0.0;
        }
    }

    fn add(&mut self, mut t: Train) {
        trim(&self.net, &mut t);
        let poses = self.poses(&t);
        t.prev = poses.clone();
        t.now = poses;
        self.trains.push(t);
        self.next_train += 1;
        self.retarget(self.trains.len() - 1, Pick::First, None);
    }
}

/// How far the leading vehicle's foremost buffer head stands past the stop's contact plane (m,
/// negative short of it), with the train's front `front` meters ahead along its path; the vehicle's
/// centre is `o` behind the front and its outer axles `hw` either side.
fn lead_buffers_past(
    r: &Railway,
    t: &Train,
    spans: &[Span],
    front: f32,
    o: f32,
    hw: f32,
    stop: Pose,
) -> f32 {
    let f = r.path_point(t, spans, front - (o - hw));
    let b = r.path_point(t, spans, front - (o + hw));
    let fwd = geom::sub(f, b);
    let fwd = geom::scale(fwd, 1.0 / geom::len(fwd).max(1e-6));
    let face = geom::add(geom::lerp(f, b, 0.5), geom::scale(fwd, o));
    let side = geom::scale(geom::right_of(fwd), BUFFER_SIDE);
    let c = geom::dot(fwd, stop.dir);
    let disc = BUFFER_RADIUS * (1.0 - c * c).max(0.0).sqrt();
    [geom::add(face, side), geom::sub(face, side)]
        .iter()
        .map(|p| geom::dot(geom::sub(*p, stop.pos), stop.dir))
        .fold(f32::MIN, f32::max)
        + disc
}

/// The distance along `points` closest to `p`.
pub fn closest_distance(points: &[Vec2], p: Vec2) -> (f32, f32) {
    let mut best = (f32::INFINITY, 0.0);
    let mut acc = 0.0;
    for w in points.windows(2) {
        let d = geom::sub(w[1], w[0]);
        let l2 = geom::dot(d, d);
        let l = l2.sqrt();
        let t = if l2 > 0.0 {
            (geom::dot(geom::sub(p, w[0]), d) / l2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let q = geom::lerp(w[0], w[1], t);
        let dist = geom::dist(p, q);
        if dist < best.0 {
            best = (dist, acc + t * l);
        }
        acc += l;
    }
    best
}

/// Train `t` turned round where it stands: its tail becomes its front, the engine changes ends,
/// and it is at rest. Poses and the reversal count are the caller's.
fn turned(net: &Network, t: &Train) -> Train {
    let mut steps = vec![];
    let mut left = TRAIN_LENGTH;
    let mut head = 0.0;
    for (k, step) in t.steps.iter().enumerate() {
        let len = link_of(net, step.link).length();
        let behind = if k == 0 { t.head } else { len };
        steps.push(Step {
            link: step.link,
            forward: !step.forward,
        });
        if left <= behind {
            // The tail is `behind - left` into this step; seen from the other way it is the rest
            // of the link.
            head = len - (behind - left);
            break;
        }
        left -= behind;
        head = len;
    }
    steps.reverse();
    let mut out = Train {
        steps,
        head,
        speed: 0.0,
        stopped: 0,
        blocker: Blocker::Clear,
        engine_leads: !t.engine_leads,
        route: vec![],
        ..t.clone()
    };
    trim(net, &mut out);
    out
}

/// Drops the links behind a train's tail.
fn trim(net: &Network, t: &mut Train) {
    let mut covered = t.head;
    let mut keep = 1;
    while covered < TRAIN_LENGTH && keep < t.steps.len() {
        covered += link_of(net, t.steps[keep].link).length();
        keep += 1;
    }
    t.steps.truncate(keep);
}
