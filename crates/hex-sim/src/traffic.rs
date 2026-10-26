//! Car following and giving way (PRD §7.2). Each tick every driving car looks [`LOOK_AHEAD`]
//! meters along its own path; where another car's path (from half a car behind its centre) comes
//! within [`LANE_CLEAR`] of it at any angle but head-on, the two conflict from that point on: one
//! lane they share, a merge, or a crossing. For each such pair the sim picks a leader and caps the
//! follower so that, by the distance each has to the meeting point, it stays at least
//! [`CAR_QUEUE_SPACING`] behind the leader's position at the start of the tick:
//!
//! - **Following.** A car already on the other's path leads, so a car keeps the spacing behind any
//!   car ahead on its lane, stopped or slower, wherever its trip goes after (a car turning off
//!   close behind a queue's last car counts too).
//! - **Merging and crossing.** When neither car is on the other's path yet (a junction entry, a
//!   driveway joining a lane, two departures from one cluster, paths crossing in a junction), the
//!   one with priority goes first: a car still on its driveway gives way to one on the road, a
//!   turning car to one going straight (turns within [`TURN_TIE_DEG`] tie), and the lower id wins
//!   a tie. The other goes first only when the priority car is still at least the spacing behind
//!   it; otherwise it waits at its hold line: [`MERGE_HOLD`] short of the meeting point, and
//!   farther back while its body there would come within [`BODY_GAP`] of any nearby car's body
//!   along that car's look-ahead, so a waiting car never stands in another's way.
//! - **Overlapping.** Two cars each on the other's path (side by side, or already overlapping) let
//!   the one farther along lead, the lower id on a tie, and the other waits until the gap opens.
//! - **Starving.** Once either car of a pair has been kept waiting [`STARVE_TICKS`], the one that
//!   has waited longer goes first, if the other can stand clear of its path, so busy traffic
//!   cannot hold a car indefinitely; waiting time orders every car alike, a safeguard against cars
//!   giving way to each other in a circle.
//!
//! A car that has not left its spot never leads: it stands where a parked car would. No car moves
//! backwards. Every pair yields one leader, and the caps use positions from the start of the tick,
//! so the result does not depend on the order the cars are visited in.

use crate::geom::{self, LanePath, Pose, Vec2};
use crate::rail::{CAR_LENGTH, CAR_QUEUE_GAP, CAR_QUEUE_SPACING};

/// Meters of its own path each car checks for other cars and merges.
pub const LOOK_AHEAD: f32 = 18.0;

/// Centre distance under which two paths conflict (m): two 2 m wide bodies side by side with 0.2 m
/// to spare. Opposite lanes run 3 m apart, so they never meet.
pub const LANE_CLEAR: f32 = 2.2;

/// How far short of the merge point a yielding car's centre holds (m): half a car and half the
/// queue gap, so its nose stays off the lane it is joining.
pub const MERGE_HOLD: f32 = 0.5 * CAR_LENGTH + 0.5 * CAR_QUEUE_GAP;

/// The width of the car body the sim sizes for (m), with [`CAR_LENGTH`]: the pickup box of the
/// parking clearance sweep (PRD §7.1).
pub const CAR_WIDTH: f32 = 2.0;

/// Least gap between a waiting car's body and any nearby car's body anywhere along that car's
/// look-ahead (m): the queue gap.
pub const BODY_GAP: f32 = CAR_QUEUE_GAP;

/// Ticks a car may be kept waiting at a merge before it goes first (10 s).
pub const STARVE_TICKS: u32 = 200;

/// Turns at a merge that differ by less than this tie, so the lower id goes first (degrees).
pub const TURN_TIE_DEG: f32 = 15.0;

/// Spacing of the points sampled along a look-ahead (m).
const SAMPLE: f32 = 0.5;

/// A car counts as on the other's path when the lanes meet within this distance of it (m): more
/// than one [`SAMPLE`].
const ON_LANE: f32 = 0.75;

/// Cosine under which two paths run head-on (about 166°): oncoming traffic on its own lane, never
/// a conflict.
const HEAD_ON: f32 = -0.97;

/// Whether two paths `d2` (squared) apart, the cosine `cos` between their directions, conflict:
/// within [`LANE_CLEAR`] at any angle but head-on.
fn meets(cos: f32, d2: f32) -> bool {
    cos > HEAD_ON && d2 <= LANE_CLEAR * LANE_CLEAR
}

/// Two look-aheads whose start-to-end chords run head-on ([`HEAD_ON`]) are oncoming traffic and
/// are not checked against each other.
const OPPOSITE: f32 = HEAD_ON;

/// A driving car as the follower rules see it.
pub(crate) struct Mover<'a> {
    pub id: u64,
    /// Meters along `path`.
    pub s: f32,
    pub path: &'a LanePath,
    /// Where the trip leaves its driveway for the road lanes, in meters along `path`.
    pub lane_start: f32,
    /// Ticks it has stood still in a row.
    pub waiting: u32,
}

/// A car's look-ahead: its path from the back of its body (half a car behind its centre, or the
/// start of the trip) to [`LOOK_AHEAD`] past its centre, with its bounding box grown by
/// [`LANE_CLEAR`]. Distances along it are measured from the car's centre, so the back of the body
/// lies at negative distances and another car passing close behind it counts as on its lane.
struct Window {
    pts: Vec<Vec2>,
    cum: Vec<f32>,
    /// Each step of `pts`: its unit direction, length and bounding box grown by [`LANE_CLEAR`].
    segs: Vec<Seg>,
    /// Meters of the window behind the car's centre.
    back: f32,
    /// Unit chord from the window's start to its end.
    chord: Vec2,
    /// The car's pose every [`SAMPLE`] from its centre to the window's end: where its body will be.
    samples: Vec<Pose>,
    lo: Vec2,
    hi: Vec2,
}

/// One step of a [`Window`].
struct Seg {
    dir: Vec2,
    len: f32,
    lo: Vec2,
    hi: Vec2,
}

impl Window {
    fn new(path: &LanePath, s: f32) -> Self {
        let back = s.min(0.5 * CAR_LENGTH);
        let pts = path.slice(s - back, s + LOOK_AHEAD);
        let mut cum = Vec::with_capacity(pts.len());
        let mut acc = 0.0;
        cum.push(0.0);
        for w in pts.windows(2) {
            acc += geom::dist(w[0], w[1]);
            cum.push(acc);
        }
        let (mut lo, mut hi) = (pts[0], pts[0]);
        for p in &pts {
            lo = [lo[0].min(p[0]), lo[1].min(p[1])];
            hi = [hi[0].max(p[0]), hi[1].max(p[1])];
        }
        let segs: Vec<Seg> = pts
            .windows(2)
            .map(|w| {
                let d = geom::sub(w[1], w[0]);
                let len = geom::len(d);
                Seg {
                    dir: if len > 1e-5 {
                        geom::scale(d, 1.0 / len)
                    } else {
                        [0.0, 0.0]
                    },
                    len,
                    lo: [
                        w[0][0].min(w[1][0]) - LANE_CLEAR,
                        w[0][1].min(w[1][1]) - LANE_CLEAR,
                    ],
                    hi: [
                        w[0][0].max(w[1][0]) + LANE_CLEAR,
                        w[0][1].max(w[1][1]) + LANE_CLEAR,
                    ],
                }
            })
            .collect();
        let span = geom::sub(pts[pts.len() - 1], pts[0]);
        let span_len = geom::len(span);
        let chord = if span_len > 1e-5 {
            geom::scale(span, 1.0 / span_len)
        } else {
            [0.0, 0.0]
        };
        let total = cum.last().copied().unwrap_or(0.0);
        let mut samples = Vec::with_capacity(((total - back) / SAMPLE) as usize + 1);
        let mut k = 0;
        let mut at = back;
        while at <= total && pts.len() > 1 {
            while k + 2 < pts.len() && cum[k + 1] < at {
                k += 1;
            }
            let seg: &Seg = &segs[k];
            if seg.len > 1e-5 {
                let t = (at - cum[k]).clamp(0.0, seg.len);
                samples.push(Pose {
                    pos: geom::add(pts[k], geom::scale(seg.dir, t)),
                    dir: seg.dir,
                });
            }
            at += SAMPLE;
        }
        Self {
            pts,
            cum,
            segs,
            back,
            chord,
            samples,
            lo: [lo[0] - LANE_CLEAR, lo[1] - LANE_CLEAR],
            hi: [hi[0] + LANE_CLEAR, hi[1] + LANE_CLEAR],
        }
    }

    /// Meters of the window ahead of the car's centre.
    fn ahead(&self) -> f32 {
        self.cum.last().copied().unwrap_or(0.0) - self.back
    }

    fn overlaps(&self, o: &Self) -> bool {
        self.lo[0] <= o.hi[0]
            && o.lo[0] <= self.hi[0]
            && self.lo[1] <= o.hi[1]
            && o.lo[1] <= self.hi[1]
    }

    /// The closest point of the window to `p` when it lies within [`LANE_CLEAR`] and `p` is
    /// abreast of the window, not behind its start or past its end: the distance along it from the
    /// car's centre and its direction there.
    fn near(&self, p: Vec2) -> Option<(f32, Vec2, f32)> {
        if p[0] < self.lo[0] || p[0] > self.hi[0] || p[1] < self.lo[1] || p[1] > self.hi[1] {
            return None;
        }
        let last = self.segs.len().checked_sub(1)?;
        // (squared distance, along, direction, off either end). Steps whose grown box misses `p`
        // lie farther than LANE_CLEAR from it, so they cannot change the answer.
        let mut best: Option<(f32, f32, Vec2, bool)> = None;
        for (k, seg) in self.segs.iter().enumerate() {
            if seg.len <= 1e-5
                || p[0] < seg.lo[0]
                || p[0] > seg.hi[0]
                || p[1] < seg.lo[1]
                || p[1] > seg.hi[1]
            {
                continue;
            }
            let raw = geom::dot(geom::sub(p, self.pts[k]), seg.dir);
            let along = raw.clamp(0.0, seg.len);
            let q = geom::add(self.pts[k], geom::scale(seg.dir, along));
            let e = geom::sub(p, q);
            let d2 = geom::dot(e, e);
            if best.is_none_or(|b| d2 < b.0) {
                let off = (k == 0 && raw < 0.0) || (k == last && raw > seg.len);
                best = Some((d2, self.cum[k] + along - self.back, seg.dir, off));
            }
        }
        let (d2, along, dir, off) = best?;
        (!off && d2 <= LANE_CLEAR * LANE_CLEAR).then_some((along, dir, d2))
    }
}

/// Where look-ahead `a` first runs onto window `b`: the distance from each car's centre to that
/// point, sampled every [`SAMPLE`] along `a` from its car's centre.
fn touch(a: &Window, b: &Window) -> Option<(f32, f32)> {
    a.samples.iter().enumerate().find_map(|(k, pose)| {
        let (along, other, d2) = b.near(pose.pos)?;
        meets(geom::dot(pose.dir, other), d2).then_some((k as f32 * SAMPLE, along))
    })
}

/// How far ahead of its centre a car giving way at a merge `merge` meters ahead may drive: up to
/// [`MERGE_HOLD`] short of the merge point, and farther back while its body there would come
/// within [`BODY_GAP`] of any nearby car's body along that car's look-ahead (a sharp merge, or a
/// car turning across the junction, swings close well before the paths meet), so a waiting car
/// never stands in another's way. Never behind where it stands.
fn hold_line(car: &Mover, merge: f32, others: &[&Mover]) -> f32 {
    let sweep: Vec<Pose> = others
        .iter()
        .filter(|o| o.s > 0.0)
        .flat_map(|o| {
            (0..)
                .map(|k| k as f32 * SAMPLE)
                .take_while(|d| *d <= LOOK_AHEAD)
                .map(|d| o.path.pose_at_distance(o.s + d))
        })
        .collect();
    let reach = (CAR_LENGTH + CAR_WIDTH + BODY_GAP).powi(2);
    let mut h = merge - MERGE_HOLD;
    while h > 0.0 {
        let pose = car.path.pose_at_distance(car.s + h);
        let blocked = sweep.iter().any(|o| {
            let d = geom::sub(o.pos, pose.pos);
            geom::dot(d, d) <= reach && bodies_meet(pose, *o, BODY_GAP)
        });
        if !blocked {
            return h;
        }
        h -= SAMPLE;
    }
    0.0
}

/// Whether two car bodies ([`CAR_LENGTH`] by [`CAR_WIDTH`], centred on their poses), each grown
/// by half of `gap`, overlap: the separating axis test on the two boxes. The pose's direction is
/// the path's, which is the body's axis whichever way the car drives.
fn bodies_meet(a: Pose, b: Pose, gap: f32) -> bool {
    let half = [0.5 * (CAR_LENGTH + gap), 0.5 * (CAR_WIDTH + gap)];
    let axes = |d: Vec2| [d, [-d[1], d[0]]];
    let (ua, ub) = (axes(a.dir), axes(b.dir));
    let radius = |u: &[Vec2; 2], axis: Vec2| {
        half[0] * geom::dot(u[0], axis).abs() + half[1] * geom::dot(u[1], axis).abs()
    };
    let d = geom::sub(b.pos, a.pos);
    !ua.iter()
        .chain(ub.iter())
        .any(|&axis| geom::dot(d, axis).abs() > radius(&ua, axis) + radius(&ub, axis))
}

/// Merge priority at `m` meters along the car's path: `(still on its driveway, turn in degrees)`,
/// the turn measured from 6 m before the merge point to 3 m past it.
fn approach(car: &Mover, m: f32) -> (bool, f32) {
    let (a, b) = (
        car.path.pose_at_distance(m - 6.0).dir,
        car.path.pose_at_distance(m + 3.0).dir,
    );
    let turn = geom::dot(a, b).clamp(-1.0, 1.0).acos().to_degrees();
    (m < car.lane_start + 1.0, turn)
}

/// Whether car `i` goes before car `j` at a merge `ai` and `aj` meters ahead of them: by
/// [`has_priority`], except that once either has been kept waiting [`STARVE_TICKS`], the one that
/// has waited longer (the lower id on a tie) goes first when the other can still stand clear of
/// its path. Waiting time orders every car the same way, so among cars waiting on each other in a
/// circle the longest waiter goes.
fn goes_first(i: &Mover, ai: f32, j: &Mover, aj: f32) -> bool {
    if i.waiting.max(j.waiting) >= STARVE_TICKS {
        let i_first = (i.waiting, std::cmp::Reverse(i.id)) > (j.waiting, std::cmp::Reverse(j.id));
        let (first, other) = if i_first { (i, j) } else { (j, i) };
        if stands_clear(other, first) {
            return i_first;
        }
    }
    has_priority(i, ai, j, aj)
}

/// Whether `car`'s body where it stands keeps [`BODY_GAP`] from `other`'s body all along
/// `other`'s look-ahead.
fn stands_clear(car: &Mover, other: &Mover) -> bool {
    let here = car.path.pose_at_distance(car.s);
    let reach = (CAR_LENGTH + CAR_WIDTH + BODY_GAP).powi(2);
    (0..)
        .map(|k| k as f32 * SAMPLE)
        .take_while(|d| *d <= LOOK_AHEAD)
        .map(|d| other.path.pose_at_distance(other.s + d))
        .all(|o| {
            let d = geom::sub(o.pos, here.pos);
            geom::dot(d, d) > reach || !bodies_meet(here, o, BODY_GAP)
        })
}

/// Whether car `i` goes before car `j` at a merge `ai` and `aj` meters ahead of them.
fn has_priority(i: &Mover, ai: f32, j: &Mover, aj: f32) -> bool {
    let ((drive_i, turn_i), (drive_j, turn_j)) = (approach(i, i.s + ai), approach(j, j.s + aj));
    if drive_i != drive_j {
        return !drive_i;
    }
    if (turn_i - turn_j).abs() >= TURN_TIE_DEG {
        return turn_i < turn_j;
    }
    i.id < j.id
}

/// The farthest each car (in `cars` order) may drive this tick under the following and merge rules,
/// `f32::INFINITY` when no car holds it. A cap below a car's distance means it waits where it is.
pub(crate) fn follow_caps(cars: &[Mover]) -> Vec<f32> {
    let mut caps = vec![f32::INFINITY; cars.len()];
    let windows: Vec<Option<Window>> = cars
        .iter()
        .map(|c| {
            let w = Window::new(c.path, c.s);
            (w.ahead() > 1e-3).then_some(w)
        })
        .collect();
    let reach = 2.0 * LOOK_AHEAD + 2.0 * LANE_CLEAR;
    // Cars giving way at a merge, `(car, distance to the merge point, cap behind the priority
    // car)`, and the cars whose windows overlap each car's.
    let mut waits: Vec<(usize, f32, f32)> = vec![];
    let mut near: Vec<Vec<usize>> = vec![vec![]; cars.len()];
    for i in 0..cars.len() {
        let Some(wi) = &windows[i] else { continue };
        for j in i + 1..cars.len() {
            let Some(wj) = &windows[j] else { continue };
            let gap = geom::sub(wi.pts[0], wj.pts[0]);
            if geom::dot(gap, gap) > reach * reach || !wi.overlaps(wj) {
                continue;
            }
            near[i].push(j);
            near[j].push(i);
            // Windows running opposite ways (oncoming traffic) cannot share a lane.
            if geom::dot(wi.chord, wj.chord) < OPPOSITE {
                continue;
            }
            // Sampled from i's look-ahead first, then from j's, which also catches a car whose
            // look-ahead passes close behind the other's centre.
            let Some((ai, aj)) = touch(wi, wj).or_else(|| touch(wj, wi).map(|(b, a)| (a, b)))
            else {
                continue;
            };
            let (on_i, on_j) = (ai <= ON_LANE, aj <= ON_LANE);
            let (lead, follow, a_lead, a_follow, yields) = if on_i != on_j {
                // One is already on the other's lane, so it is ahead.
                if on_i {
                    (i, j, ai, aj, false)
                } else {
                    (j, i, aj, ai, false)
                }
            } else if on_i {
                // Side by side or overlapping: the one farther along leads.
                if ai < aj || (ai == aj && cars[i].id < cars[j].id) {
                    (i, j, ai, aj, false)
                } else {
                    (j, i, aj, ai, false)
                }
            } else {
                // A merge: the car without priority enters first only with the spacing in hand.
                let (p, q, ap, aq) = if goes_first(&cars[i], ai, &cars[j], aj) {
                    (i, j, ai, aj)
                } else {
                    (j, i, aj, ai)
                };
                if ap - aq >= CAR_QUEUE_SPACING {
                    (q, p, aq, ap, false)
                } else {
                    (p, q, ap, aq, true)
                }
            };
            if cars[lead].s <= 0.0 && cars[follow].s > 0.0 {
                // A car that has not left its spot stands where a parked car would.
                continue;
            }
            let s = cars[follow].s;
            let cap = s + a_follow - a_lead - CAR_QUEUE_SPACING;
            if yields {
                waits.push((follow, a_follow, cap));
            } else {
                caps[follow] = caps[follow].min(cap);
            }
        }
    }
    for (car, merge, cap) in waits {
        let others: Vec<&Mover> = near[car].iter().map(|&o| &cars[o]).collect();
        let hold = cars[car].s + hold_line(&cars[car], merge, &others);
        caps[car] = caps[car].min(cap.max(hold));
    }
    caps
}
