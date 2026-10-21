//! Tile segment shapes on the XZ plane (tile-local or world meters): the polyline a network segment
//! runs along. `hex-sim` builds its graph links from it, and the buffer stops ([`crate::rail_end`])
//! measure along the same centreline.

type Vec2 = [f32; 2];

/// Largest angle one arc sample may span, in radians (5°).
pub const ARC_STEP: f32 = std::f32::consts::PI / 36.0;

fn add(a: Vec2, b: Vec2) -> Vec2 {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub(a: Vec2, b: Vec2) -> Vec2 {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot(a: Vec2, b: Vec2) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: Vec2, b: Vec2) -> f32 {
    a[0] * b[1] - a[1] * b[0]
}

fn dist(a: Vec2, b: Vec2) -> f32 {
    dot(sub(a, b), sub(a, b)).sqrt()
}

/// The sampled shape of a segment from `from` to `to` through `interior` control points.
///
/// No control point gives a straight line. One gives the circular arc through all three points,
/// since tile data places a single point at the arc midpoint. More give the polyline through them.
pub fn segment_shape(from: Vec2, interior: &[Vec2], to: Vec2) -> Vec<Vec2> {
    match interior {
        [] => vec![from, to],
        [mid] => arc_through(from, *mid, to),
        _ => {
            let mut points = Vec::with_capacity(interior.len() + 2);
            points.push(from);
            points.extend_from_slice(interior);
            points.push(to);
            points
        }
    }
}

/// Samples the circle through `a`, `m` and `b` from `a` to `b` via `m`, at most [`ARC_STEP`] apart.
/// Collinear points give the polyline `a, m, b`.
fn arc_through(a: Vec2, m: Vec2, b: Vec2) -> Vec<Vec2> {
    let turn = cross(sub(m, a), sub(b, m));
    let d = 2.0 * cross(sub(m, a), sub(b, a));
    if turn.abs() <= 1e-6 * dist(a, b).powi(2) || d == 0.0 {
        return vec![a, m, b];
    }
    // Circumcenter relative to `a`.
    let (u, v) = (sub(m, a), sub(b, a));
    let (uu, vv) = (dot(u, u), dot(v, v));
    let center = add(
        a,
        [(v[1] * uu - u[1] * vv) / d, (u[0] * vv - v[0] * uu) / d],
    );
    let radius = dist(a, center);
    let angle = |p: Vec2| (p[1] - center[1]).atan2(p[0] - center[0]);
    let start = angle(a);
    let tau = std::f32::consts::TAU;
    let mut sweep = (angle(b) - start).rem_euclid(tau);
    if turn < 0.0 {
        sweep -= tau;
    }
    let steps = ((sweep.abs() / ARC_STEP).ceil() as usize).max(2);
    let mut points = Vec::with_capacity(steps + 1);
    points.push(a);
    for i in 1..steps {
        let t = start + sweep * i as f32 / steps as f32;
        points.push([center[0] + radius * t.cos(), center[1] + radius * t.sin()]);
    }
    points.push(b);
    points
}
