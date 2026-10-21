//! World-space geometry on the XZ plane, in meters: tile transforms, segment shapes and lane paths.
//!
//! Directions follow `hex_core::axial`: edge 0 faces +x and edges run clockwise viewed from +Y,
//! so edge `e` points at angle `60° · e` measured from +x toward +z.

use hex_core::{Axial, HEX_RADIUS};

/// `sqrt(3) / 2`, the sine of 60°.
const HALF_SQRT3: f32 = 0.866_025_4;

/// Outward unit normals of edges 0..6; entry `k` is also the `(cos, sin)` of rotation `k`.
const EDGE_DIR: [[f32; 2]; 6] = [
    [1.0, 0.0],
    [0.5, HALF_SQRT3],
    [-0.5, HALF_SQRT3],
    [-1.0, 0.0],
    [-0.5, -HALF_SQRT3],
    [0.5, -HALF_SQRT3],
];

/// Distance from a cell center to each edge midpoint.
pub const APOTHEM: f32 = HEX_RADIUS * HALF_SQRT3;

use hex_core::path::ARC_STEP;

/// Shorter steps are dropped when a lane path is built, so every step has a direction.
const MIN_STEP: f32 = 1e-4;

pub type Vec2 = [f32; 2];

pub fn add(a: Vec2, b: Vec2) -> Vec2 {
    [a[0] + b[0], a[1] + b[1]]
}

pub fn sub(a: Vec2, b: Vec2) -> Vec2 {
    [a[0] - b[0], a[1] - b[1]]
}

pub fn scale(a: Vec2, s: f32) -> Vec2 {
    [a[0] * s, a[1] * s]
}

pub fn dot(a: Vec2, b: Vec2) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

pub fn cross(a: Vec2, b: Vec2) -> f32 {
    a[0] * b[1] - a[1] * b[0]
}

pub fn len(a: Vec2) -> f32 {
    dot(a, a).sqrt()
}

pub fn dist(a: Vec2, b: Vec2) -> f32 {
    len(sub(a, b))
}

pub fn lerp(a: Vec2, b: Vec2, t: f32) -> Vec2 {
    add(a, scale(sub(b, a), t))
}

/// The driver's right for a heading `d` with +Y up: `(-d_z, d_x)`. Heading +x drives at +z.
pub fn right_of(d: Vec2) -> Vec2 {
    [-d[1], d[0]]
}

/// Rotates a tile-local point by `rotation` sixth-turns (local edge `e` lands on edge `e + k`).
pub fn rotate(p: Vec2, rotation: u8) -> Vec2 {
    let [c, s] = EDGE_DIR[(rotation % 6) as usize];
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}

pub fn cell_center(cell: Axial) -> Vec2 {
    cell.to_world(HEX_RADIUS)
}

/// World position of a lane crossing world edge `edge` of `cell`, `offset` meters along the edge
/// tangent `(-n_z, n_x)` from the midpoint (the `LaneSpec` convention in `hex_core::socket`).
pub fn lane_point(cell: Axial, edge: u8, offset: f32) -> Vec2 {
    let n = EDGE_DIR[(edge % 6) as usize];
    let tangent = [-n[1], n[0]];
    add(
        add(cell_center(cell), scale(n, APOTHEM)),
        scale(tangent, offset),
    )
}

/// Total length of a polyline.
pub fn polyline_length(points: &[Vec2]) -> f32 {
    points.windows(2).map(|w| dist(w[0], w[1])).sum()
}

/// Shortest distance from `p` to a polyline.
pub fn distance_to_polyline(p: Vec2, points: &[Vec2]) -> f32 {
    match points {
        [] => f32::INFINITY,
        [only] => dist(p, *only),
        _ => points
            .windows(2)
            .map(|w| {
                let d = sub(w[1], w[0]);
                let l2 = dot(d, d);
                let t = if l2 > 0.0 {
                    (dot(sub(p, w[0]), d) / l2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                dist(p, lerp(w[0], w[1], t))
            })
            .fold(f32::INFINITY, f32::min),
    }
}

pub use hex_core::path::segment_shape;

/// Largest step between samples of a Hermite parking leg, in meters.
const LEG_STEP: f32 = 0.1;

/// The unit heading of a yaw in degrees from +x toward +z.
pub fn heading(yaw_deg: f32) -> Vec2 {
    let a = yaw_deg.to_radians();
    [a.cos(), a.sin()]
}

/// Samples a parking leg through `poses` (position and facing), joining each consecutive pair with
/// a cubic Hermite curve whose end tangents are the facings scaled by the chord length (negated
/// when `reverse`, since a reversing car travels against its facing). Shared points appear once.
pub fn hermite_leg(poses: &[Pose], reverse: bool) -> Vec<Vec2> {
    let sign = if reverse { -1.0 } else { 1.0 };
    let mut out: Vec<Vec2> = vec![];
    for w in poses.windows(2) {
        let (a, b) = (w[0], w[1]);
        let chord = dist(a.pos, b.pos);
        let (ta, tb) = (scale(a.dir, sign * chord), scale(b.dir, sign * chord));
        let n = ((chord / LEG_STEP).ceil() as usize).max(8);
        let first = usize::from(!out.is_empty());
        for i in first..=n {
            let s = i as f32 / n as f32;
            let (s2, s3) = (s * s, s * s * s);
            let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
            let h10 = s3 - 2.0 * s2 + s;
            let h01 = -2.0 * s3 + 3.0 * s2;
            let h11 = s3 - s2;
            out.push(add(
                add(scale(a.pos, h00), scale(ta, h10)),
                add(scale(b.pos, h01), scale(tb, h11)),
            ));
        }
    }
    if out.is_empty()
        && let Some(p) = poses.first()
    {
        out.push(p.pos);
    }
    out
}

/// Where a car is and which way it faces, in world XZ meters. `dir` is a unit vector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub pos: Vec2,
    pub dir: Vec2,
}

impl Pose {
    /// Heading as a yaw angle in radians from +x toward +z.
    pub fn yaw(&self) -> f32 {
        self.dir[1].atan2(self.dir[0])
    }
}

/// A route centerline shifted to the driver's right, parameterized by distance along it.
#[derive(Clone, Debug, PartialEq)]
pub struct LanePath {
    points: Vec<Vec2>,
    /// Distance from the start to each point.
    cum: Vec<f32>,
}

impl LanePath {
    /// Offsets every vertex of `centerline` by `offset` along the average of the right normals of
    /// its two steps, so the path stays continuous through corners and never strays farther than
    /// `offset` from the centerline.
    pub fn new(centerline: &[Vec2], offset: f32) -> Self {
        let line = dedup(centerline);
        Self::from_points(offset_points(&line, offset))
    }

    /// [`LanePath::new`] with every corner sharper than [`CORNER_MIN`] rounded by a circular arc
    /// of radius at most [`CORNER_RADIUS`], as a car steers through a junction node. Each arc is
    /// shrunk until it stays on its own side of `centerline`, between half of `offset` and
    /// `offset` from it, so the rounded lane never leaves the band an unrounded one keeps to.
    pub fn rounded(centerline: &[Vec2], offset: f32) -> Self {
        let line = dedup(centerline);
        let lane = offset_points(&line, offset);
        let (lo, hi) = (0.5 * offset, offset + 1e-4);
        let mut out: Vec<Vec2> = Vec::with_capacity(lane.len());
        for i in 0..lane.len() {
            let v = lane[i];
            let (Some(&p), Some(&n)) =
                (i.checked_sub(1).and_then(|j| lane.get(j)), lane.get(i + 1))
            else {
                out.push(v);
                continue;
            };
            let (a, b) = (sub(v, p), sub(n, v));
            let (la, lb) = (len(a), len(b));
            let (a, b) = (scale(a, 1.0 / la), scale(b, 1.0 / lb));
            let turn = dot(a, b).clamp(-1.0, 1.0).acos();
            if turn < CORNER_MIN || turn > CORNER_MAX {
                out.push(v);
                continue;
            }
            // Half of a step shared with another vertex, all of an end step.
            let avail = |l: f32, end: bool| if end { 0.95 * l } else { 0.5 * l };
            let t_max = avail(la, i == 1).min(avail(lb, i + 2 == lane.len()));
            let tan = (0.5 * turn).tan();
            let mut r = CORNER_RADIUS.min(t_max / tan);
            // The centerline steps around the corner; the arc cannot reach farther ones.
            let near = &line[i.saturating_sub(2)..(i + 3).min(line.len())];
            let arc = loop {
                if r < 0.05 {
                    break None;
                }
                let arc = fillet(v, a, b, turn, r * tan, r);
                // Between samples too: the distance to a polyline is not convex along a chord.
                let inside = arc.windows(2).all(|w| {
                    [0.0, 0.25, 0.5, 0.75, 1.0].iter().all(|&t| {
                        let d = distance_to_polyline(lerp(w[0], w[1], t), near);
                        (lo..=hi).contains(&d)
                    })
                });
                if inside {
                    break Some(arc);
                }
                r *= 0.85;
            };
            match arc {
                Some(arc) => out.extend(arc),
                None => out.push(v),
            }
        }
        Self::from_points(out)
    }

    fn from_points(points: Vec<Vec2>) -> Self {
        let mut cum = Vec::with_capacity(points.len());
        let mut total = 0.0;
        for (i, p) in points.iter().enumerate() {
            if i > 0 {
                total += dist(points[i - 1], *p);
            }
            cum.push(total);
        }
        Self { points, cum }
    }
}

/// Corners turning less than this are left alone: arc samples turn at most [`ARC_STEP`].
const CORNER_MIN: f32 = 1.5 * ARC_STEP;

/// Corners sharper than this (a U-turn) are left alone.
const CORNER_MAX: f32 = 170.0 * std::f32::consts::PI / 180.0;

/// Largest radius of a rounded lane corner, in meters.
pub const CORNER_RADIUS: f32 = 4.0;

/// Largest angle one rounded-corner sample may span, in radians (2°).
const FILLET_STEP: f32 = std::f32::consts::PI / 90.0;

/// The arc of radius `r` replacing corner `v` between unit headings `a` (in) and `b` (out) that
/// turn by `turn` radians, from `t` meters before `v` to `t` meters after it.
fn fillet(v: Vec2, a: Vec2, b: Vec2, turn: f32, t: f32, r: f32) -> Vec<Vec2> {
    let side = if cross(a, b) >= 0.0 { 1.0 } else { -1.0 };
    let start = sub(v, scale(a, t));
    // Unit normal of `a` toward the turn.
    let normal = scale([-a[1], a[0]], side);
    let center = add(start, scale(normal, r));
    let rel = sub(start, center);
    let steps = ((turn / FILLET_STEP).ceil() as usize).max(2);
    (0..=steps)
        .map(|k| {
            let phi = side * turn * k as f32 / steps as f32;
            let (s, c) = phi.sin_cos();
            add(center, [rel[0] * c - rel[1] * s, rel[0] * s + rel[1] * c])
        })
        .collect()
}

/// `centerline` without steps shorter than [`MIN_STEP`].
fn dedup(centerline: &[Vec2]) -> Vec<Vec2> {
    let mut line: Vec<Vec2> = Vec::with_capacity(centerline.len());
    for &p in centerline {
        if line.last().is_none_or(|&q| dist(p, q) > MIN_STEP) {
            line.push(p);
        }
    }
    line
}

/// Every vertex of `line` moved `offset` along the average of the right normals of its two steps.
fn offset_points(line: &[Vec2], offset: f32) -> Vec<Vec2> {
    let dirs: Vec<Vec2> = line
        .windows(2)
        .map(|w| {
            let d = sub(w[1], w[0]);
            scale(d, 1.0 / len(d))
        })
        .collect();
    (0..line.len())
        .map(|i| {
            let before = i.checked_sub(1).and_then(|j| dirs.get(j));
            let normal = match (before, dirs.get(i)) {
                (Some(&a), Some(&b)) => {
                    let sum = add(right_of(a), right_of(b));
                    let l = len(sum);
                    if l > 1e-3 {
                        scale(sum, 1.0 / l)
                    } else {
                        right_of(b)
                    }
                }
                (Some(&d), None) | (None, Some(&d)) => right_of(d),
                (None, None) => [0.0, 0.0],
            };
            add(line[i], scale(normal, offset))
        })
        .collect()
}

impl LanePath {
    pub fn length(&self) -> f32 {
        self.cum.last().copied().unwrap_or(0.0)
    }

    pub fn points(&self) -> &[Vec2] {
        &self.points
    }

    /// The path from `s0` to `s1` meters along it (clamped to its ends), as a polyline that starts
    /// and ends at those distances.
    pub fn slice(&self, s0: f32, s1: f32) -> Vec<Vec2> {
        let (s0, s1) = (s0.clamp(0.0, self.length()), s1.clamp(0.0, self.length()));
        let mut out = vec![self.pose_at_distance(s0).pos];
        for (p, c) in self.points.iter().zip(&self.cum) {
            if *c > s0 + MIN_STEP && *c < s1 - MIN_STEP {
                out.push(*p);
            }
        }
        out.push(self.pose_at_distance(s1).pos);
        out
    }

    /// The largest heading change between any two poses `step` meters apart, in degrees, sliding
    /// the pair along the path 5 cm at a time.
    pub fn max_turn_deg(&self, step: f32) -> f32 {
        let mut worst = 0.0f32;
        let mut d = 0.0;
        while d + step <= self.length() {
            let (a, b) = (
                self.pose_at_distance(d).dir,
                self.pose_at_distance(d + step).dir,
            );
            worst = worst.max(dot(a, b).clamp(-1.0, 1.0).acos().to_degrees());
            d += 0.05;
        }
        worst
    }

    /// The pose `s` meters along the path, clamped to its ends.
    pub fn pose_at_distance(&self, s: f32) -> Pose {
        match self.points.len() {
            0 => Pose {
                pos: [0.0, 0.0],
                dir: [1.0, 0.0],
            },
            1 => Pose {
                pos: self.points[0],
                dir: [1.0, 0.0],
            },
            n => {
                let s = s.clamp(0.0, self.length());
                let i = self.cum.partition_point(|&c| c <= s).clamp(1, n - 1);
                let (a, b) = (self.points[i - 1], self.points[i]);
                let step = self.cum[i] - self.cum[i - 1];
                let t = if step > 0.0 {
                    (s - self.cum[i - 1]) / step
                } else {
                    0.0
                };
                let d = sub(b, a);
                Pose {
                    pos: lerp(a, b, t),
                    dir: scale(d, 1.0 / len(d)),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec2, b: Vec2) -> bool {
        dist(a, b) < 1e-3
    }

    #[test]
    fn rotation_maps_edge_normals_onto_rotated_edges() {
        for k in 0..6u8 {
            for e in 0..6u8 {
                let got = rotate(EDGE_DIR[e as usize], k);
                assert!(close(got, EDGE_DIR[((e + k) % 6) as usize]), "e{e} k{k}");
            }
        }
    }

    #[test]
    fn lane_points_meet_the_neighbor_at_the_seam() {
        let cell = Axial::new(2, -1);
        for e in 0..6u8 {
            let n = cell.neighbor(e);
            let mid = scale(add(cell_center(cell), cell_center(n)), 0.5);
            assert!(close(lane_point(cell, e, 0.0), mid));
            assert!(close(
                lane_point(cell, e, 1.0),
                lane_point(n, (e + 3) % 6, -1.0)
            ));
        }
    }

    #[test]
    fn arc_through_three_points_has_circle_length() {
        // Quarter circle of radius 10 around the origin.
        let s = 10.0 * std::f32::consts::FRAC_1_SQRT_2;
        let pts = segment_shape([10.0, 0.0], &[[s, s]], [0.0, 10.0]);
        let want = 10.0 * std::f32::consts::FRAC_PI_2;
        assert!((polyline_length(&pts) - want).abs() < 0.01);
        assert!(pts.iter().all(|p| (len(*p) - 10.0).abs() < 1e-3));
        // Same arc, reversed: turns the other way.
        let back = segment_shape([0.0, 10.0], &[[s, s]], [10.0, 0.0]);
        assert!((polyline_length(&back) - want).abs() < 0.01);
    }

    #[test]
    fn collinear_control_point_stays_straight() {
        let pts = segment_shape([0.0, 0.0], &[[5.0, 0.0]], [10.0, 0.0]);
        assert_eq!(pts, vec![[0.0, 0.0], [5.0, 0.0], [10.0, 0.0]]);
    }

    #[test]
    fn heading_plus_x_drives_at_plus_z() {
        let path = LanePath::new(&[[0.0, 0.0], [10.0, 0.0]], 1.5);
        let pose = path.pose_at_distance(5.0);
        assert!(close(pose.pos, [5.0, 1.5]));
        assert!(close(pose.dir, [1.0, 0.0]));
    }

    #[test]
    fn lane_path_stays_within_offset_of_a_corner() {
        let line = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]];
        let path = LanePath::new(&line, 1.5);
        for i in 0..=100 {
            let p = path.pose_at_distance(path.length() * i as f32 / 100.0).pos;
            assert!(distance_to_polyline(p, &line) <= 1.5 + 1e-4);
        }
    }

    #[test]
    fn hermite_leg_meets_its_poses_with_their_headings() {
        let a = Pose {
            pos: [0.0, 0.0],
            dir: [1.0, 0.0],
        };
        let b = Pose {
            pos: [4.0, 4.0],
            dir: [0.0, 1.0],
        };
        let pts = hermite_leg(&[a, b], false);
        assert_eq!(pts[0], a.pos);
        assert!(close(*pts.last().unwrap(), b.pos));
        let first = sub(pts[1], pts[0]);
        let last = sub(pts[pts.len() - 1], pts[pts.len() - 2]);
        assert!(dot(scale(first, 1.0 / len(first)), a.dir) > 0.99);
        assert!(dot(scale(last, 1.0 / len(last)), b.dir) > 0.99);
        // Reversing travels against the facing.
        let back = hermite_leg(
            &[
                a,
                Pose {
                    pos: [-4.0, 4.0],
                    dir: [0.0, -1.0],
                },
            ],
            true,
        );
        assert!(back[1][0] < 0.0);
    }

    #[test]
    fn pose_clamps_to_path_ends() {
        let path = LanePath::new(&[[0.0, 0.0], [4.0, 0.0]], 0.0);
        assert_eq!(path.pose_at_distance(-1.0).pos, [0.0, 0.0]);
        assert_eq!(path.pose_at_distance(9.0).pos, [4.0, 0.0]);
    }
}
