//! Trains on the rail network (PRD §7.1, §7.2, P3): the stitched rail graph, the switch at every
//! turnout toe, level crossings and the trains themselves, on the fixed 20 Hz tick.
//!
//! - **Graph.** One [`Network`] of [`NetworkKind::Rail`], stitched at seams like the roads. A
//!   turnout's three segments meet at its toe node ([`hex_core::SwitchDef`]); a train running in on
//!   the toe segment leaves on the leg the switch is set to, and from either leg it trails through
//!   onto the toe segment. No route joins the two legs.
//! - **Trains.** An engine, a carriage and a brake third ([`CONSIST`]), [`TRAIN_LENGTH`] meters
//!   over the buffers. A train follows the track, never a route: there are no stations yet, so at each facing
//!   toe it takes the leg the junction offers next, alternating between the two, and the switch is
//!   thrown for it when it reserves the junction. At a line end (a seam facing an empty cell; rail
//!   meets only rail, so a line can end nowhere else) it stops inside the last tile, waits
//!   [`DWELL_SECONDS`] and runs back the other way, the tank engine propelling its carriages (a
//!   push-pull "autotrain": tank engines work both ways without a turntable). On a loop it loops.
//! - **Junctions.** A train may enter a junction's fouling zone (from the toe border to
//!   [`hex_core::SwitchDef::fouling`] along both legs) only with the junction reserved, and the
//!   switch moves only when a train reserves an empty zone, so it never moves under a train. A
//!   train that cannot reserve stands [`ZONE_MARGIN`] short of the zone: beyond the fouling point on
//!   a leg, short of the toe border on the toe side.
//! - **Spacing.** A train brakes to stand [`TRAIN_GAP`] behind any other train on its path. Two
//!   trains stopped nose to nose for [`STUCK_SECONDS`] resolve it: the higher id runs back.
//! - **Crossings.** A level crossing is closed while a train body is on its deck or a train's front
//!   is within [`CROSSING_WARN`] of it, and cars hold short of a closed crossing ([`crate::Car`]).
//! - **Spawning.** Trains are derived, never saved: on each tick after the graph changed, every
//!   connected stretch of at least [`MIN_TRACK`] meters gets one train, plus one per further
//!   [`TRACK_PER_TRAIN`], up to [`MAX_TRAINS`], placed where the whole train fits clear of others
//!   and of junction zones. A world edit removes the trains standing on the edited cell.

use std::collections::BTreeSet;

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
/// How far inside the last tile a train stops at a line end (m), so the engine's 2.47 m overhang
/// never reaches the empty (or still rising) cell beyond.
pub const END_MARGIN: f32 = 1.0;
/// Wait at a line end before running back (s).
pub const DWELL_SECONDS: f32 = 3.0;
/// Two trains stopped facing each other this long (s): the higher id runs back.
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
    /// The brake third (`train_carriage_b`): the guard's veranda and roof lookout at its rear, so it
    /// runs last. Its length and axles are the carriage's.
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

/// The engine (8.94 m, axles ±2.0 m), a carriage and the brake third last (both 7.14 m, axles
/// ±1.5 m), 0.5 m apart.
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
    prev: Vec<VehiclePose>,
    now: Vec<VehiclePose>,
    /// Facing toes passed, as (junction cell, leg taken), oldest first; the last 16.
    routes: Vec<(Axial, Leg)>,
    reversals: u32,
}

impl Train {
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
    trains: Vec<Train>,
    next_train: u64,
    /// The graph changed (or a line still wants a train): run the spawner on the next tick.
    dirty: bool,
    ticks: u64,
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
        Self {
            net: Network::new(NetworkKind::Rail),
            junctions: vec![],
            crossings: vec![],
            trains: vec![],
            next_train: 0,
            dirty: false,
            ticks: 0,
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
    }

    /// Mirrors a bulldozed tile.
    pub fn remove_tile(&mut self, cell: Axial) {
        self.drop_trains_on(cell);
        self.forget_cell(cell);
        if self.net.remove_tile(cell) {
            self.dirty = true;
        }
    }

    fn forget_cell(&mut self, cell: Axial) {
        self.junctions.retain(|j| j.cell != cell);
        self.crossings.retain(|c| c.cell != cell);
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
    /// toe the holder follows the switch and anyone else plans on the junction's next leg.
    fn next_step(&self, node: NodeId, from: LinkId, train: TrainId) -> Option<Step> {
        let net = self.net();
        let n = net.node(node)?;
        let link = if let Some(j) = self.junction_at(node).map(|i| &self.junctions[i]) {
            if from == j.toe {
                let leg = if j.holder == Some(train) {
                    j.set
                } else {
                    j.next
                };
                j.leg(leg)
            } else {
                j.toe
            }
        } else {
            *n.links().iter().find(|&&l| l != from)?
        };
        let forward = link_of(net, link).ends().0 == node;
        Some(Step { link, forward })
    }

    /// The track ahead of a train's front, up to `dist` meters, and the distance to a line end if
    /// one comes first.
    fn ahead(&self, t: &Train, dist: f32) -> (Vec<Span>, Option<f32>) {
        let net = self.net();
        let mut spans = vec![];
        let mut step = t.steps[0];
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
            match self.next_step(node, step.link, t.id) {
                Some(next) => {
                    step = next;
                    into = 0.0;
                }
                None => return (spans, Some(at)),
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

    fn poses(&self, t: &Train) -> Vec<VehiclePose> {
        CONSIST
            .iter()
            .map(|v| {
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
                    kind: v.kind,
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

    fn overlaps(body: &[(LinkId, f32, f32)], link: LinkId, lo: f32, hi: f32) -> bool {
        body.iter()
            .any(|&(l, a, b)| l == link && a <= hi && b >= lo)
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
        if let Some(d) = line_end {
            let d = d - END_MARGIN;
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
        // Junctions: reserve the first zone ahead that this train does not hold, or stand short.
        let mut seen: BTreeSet<usize> = BTreeSet::new();
        for sp in &spans {
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
                if j.holder == Some(id) || d >= limit {
                    continue;
                }
                if j.holder.is_none() && !self.zone_taken(j, id) {
                    let j = &mut self.junctions[k];
                    j.holder = Some(id);
                    let leg = match j.leg_of(sp.link) {
                        Some(trailing) => trailing,
                        None => {
                            let leg = j.next;
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
        self.advance(i, ds);
        let t = &mut self.trains[i];
        t.speed = ds / TICK_SECONDS;
        if ds < 1e-4 {
            t.stopped += 1;
            t.blocker = blocker;
        } else {
            t.stopped = 0;
            t.blocker = Blocker::Clear;
        }
        let dwell = (DWELL_SECONDS / TICK_SECONDS) as u32;
        let give_up = (GIVE_UP_SECONDS / TICK_SECONDS) as u32;
        let reverse = match t.blocker {
            Blocker::LineEnd => t.stopped >= dwell,
            Blocker::Train(_) | Blocker::Junction(_) => t.stopped >= give_up,
            Blocker::Clear => false,
        };
        if reverse {
            self.reverse(i);
        }
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
                self.reverse(i);
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
    fn advance(&mut self, i: usize, ds: f32) {
        let id = self.trains[i].id;
        self.trains[i].head += ds;
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
            let Some(next) = self.next_step(node, step.link, id) else {
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
    }

    /// Drops the links behind train `i`'s tail.
    fn trim(&mut self, i: usize) {
        trim(&self.net, &mut self.trains[i]);
    }

    /// Turns train `i` round where it stands: its tail becomes its front.
    fn reverse(&mut self, i: usize) {
        let mut t = turned(&self.net, &self.trains[i]);
        t.reversals += 1;
        let poses = self.poses(&t);
        t.now = poses.clone();
        t.prev = poses;
        self.trains[i] = t;
    }

    /// Free track behind train `t`: how far it could run back before a line end or another train.
    fn room_behind(&self, t: &Train) -> f32 {
        let back = turned(&self.net, t);
        let (spans, end) = self.ahead(&back, LOOKAHEAD);
        let mut room = end.unwrap_or(LOOKAHEAD);
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

    fn spawn(&mut self) {
        self.dirty = false;
        let lines = self.lines();
        for (links, len) in lines {
            if self.trains.len() >= MAX_TRAINS || len < MIN_TRACK {
                continue;
            }
            let want =
                (1 + ((len - MIN_TRACK) / TRACK_PER_TRAIN) as usize).min(MAX_TRAINS_PER_LINE);
            let set: BTreeSet<LinkId> = links.iter().copied().collect();
            let have = self
                .trains
                .iter()
                .filter(|t| set.contains(&t.steps[0].link))
                .count();
            for _ in have..want {
                if self.trains.len() >= MAX_TRAINS || !self.spawn_on(&links) {
                    break;
                }
            }
        }
    }
    /// Places a new train on the first link of `links` (in id order) where its whole body fits
    /// clear of other trains and of every junction zone. Returns whether one was placed.
    fn spawn_on(&mut self, links: &[LinkId]) -> bool {
        let id = TrainId(self.next_train);
        for &start in links {
            let step = Step {
                link: start,
                forward: true,
            };
            // The tail just inside the link's `a` end, the front a train length on, with clear
            // track for the gap at both ends.
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
            prev: vec![],
            now: vec![],
            routes: vec![],
            reversals: 0,
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
            let next = self.next_step(node, step.link, id)?;
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
    }
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
