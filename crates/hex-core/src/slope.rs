//! The P4 slope height fields and the hillside ground mesh.
//!
//! Every field is in metres above the tile's level, tile-local and unrotated: corner heights in
//! local corner order, corner `k` at angle 30° + 60°·k on radius [`HEX_RADIUS`] ([`crate::prism`]).
//! [`height_at`] is the edge blend: each edge carries the smoothstep-eased ramp between its two
//! corner heights, and the interior blends the six edges by inverse squared distance to their
//! lines. Every seam is C1 (equal height and slope along the edge, zero slope across it), so
//! neighbouring caps shade alike. The tunnel-portal Blender generator
//! (`scripts/blender/kit/_tunnel_portal.py`, `cut_height_at`) computes the Cutting field in Python,
//! and its self-check and `fields_match_python_reference` share the committed
//! `scripts/blender/kit/slope_samples.json`. Callers go through [`field_at`].

use std::sync::LazyLock;

use crate::axial::{HEX_RADIUS, LEVEL_STEP};
use crate::prism::PrismMesh;
use crate::tile::Ground;

/// Road corridor half width and the outer edge of its blend into the bank (|z|, metres).
const CORRIDOR: f64 = 4.0;
const BLEND: f64 = 8.0;
/// Length of the road ramp's parabolic transition at each end (metres along x).
const VERTICAL_CURVE: f64 = 4.0;
/// Within this distance of the border (or outside it) the field takes the edge value.
const BORDER_EPS: f64 = 1e-5;
const SKIRT_UV: [f32; 2] = [0.005, 0.005];

/// A tile's ground: the cap (smooth analytic normals, meadow's planar UVs) and the skirt walls
/// from the cap boundary down to `-LEVEL_STEP`, one per edge.
pub struct SlopeMesh {
    pub cap: PrismMesh,
    pub skirt: PrismMesh,
}

/// A field value and its gradient `(d/dx, d/dz)`.
type Sample = (f64, [f64; 2]);

/// The hexagon, computed once: corner `k` at 30° + 60°·k; edge `e` faces 60°·e and runs from its
/// start corner `(e + 5) % 6` to its end corner `e` (`tile::edge_corners`).
struct Hex {
    corners: [[f64; 2]; 6],
    /// Outward unit normal of edge `e`.
    normals: [[f64; 2]; 6],
    /// Unit direction of edge `e`, start corner to end corner.
    dirs: [[f64; 2]; 6],
    /// Centre-to-edge distance, `HEX_RADIUS · √3 / 2`.
    apothem: f64,
}

static HEX: LazyLock<Hex> = LazyLock::new(|| {
    let r = HEX_RADIUS as f64;
    let corners: [[f64; 2]; 6] = std::array::from_fn(|k| {
        let a = (30.0 + 60.0 * k as f64).to_radians();
        [r * a.cos(), r * a.sin()]
    });
    Hex {
        corners,
        normals: std::array::from_fn(|e| {
            let a = (60.0 * e as f64).to_radians();
            [a.cos(), a.sin()]
        }),
        dirs: std::array::from_fn(|e| {
            let (s, t) = (corners[(e + 5) % 6], corners[e]);
            [(t[0] - s[0]) / r, (t[1] - s[1]) / r]
        }),
        apothem: r * 3f64.sqrt() / 2.0,
    }
});

fn start(e: usize) -> usize {
    (e + 5) % 6
}

/// Unclamped position of `p` along edge `e`'s line: 0 at its start corner, 1 at its end corner.
fn edge_t(e: usize, p: [f64; 2]) -> f64 {
    let (c, d) = (HEX.corners[start(e)], HEX.dirs[e]);
    ((p[0] - c[0]) * d[0] + (p[1] - c[1]) * d[1]) / HEX_RADIUS as f64
}

/// Edge `e`'s eased ramp `g_e(t) = h_start + (h_end − h_start) · smoothstep01(clamp(t))` and its
/// gradient in the plane. The ramp's slope vanishes at both corners, so the clamp is C1.
fn edge_ramp(h: &[f64; 6], e: usize, p: [f64; 2]) -> Sample {
    let t = edge_t(e, p).clamp(0.0, 1.0);
    let (h0, h1) = (h[start(e)], h[e]);
    let dg = (h1 - h0) * 6.0 * t * (1.0 - t) / HEX_RADIUS as f64;
    let d = HEX.dirs[e];
    (
        h0 + (h1 - h0) * t * t * (3.0 - 2.0 * t),
        [dg * d[0], dg * d[1]],
    )
}

/// Distance from `p` to edge `e`'s line, positive inside.
fn edge_distance(e: usize, p: [f64; 2]) -> f64 {
    let n = HEX.normals[e];
    HEX.apothem - (p[0] * n[0] + p[1] * n[1])
}

/// The edge nearest `p` (by distance to the edge segment) and the closest point on it.
fn nearest_edge(p: [f64; 2]) -> (usize, [f64; 2]) {
    let mut best = (f64::MAX, 0, p);
    for e in 0..6 {
        let (c, d) = (HEX.corners[start(e)], HEX.dirs[e]);
        let s = edge_t(e, p).clamp(0.0, 1.0) * HEX_RADIUS as f64;
        let q = [c[0] + s * d[0], c[1] + s * d[1]];
        let dist = (q[0] - p[0]).hypot(q[1] - p[1]);
        if dist < best.0 {
            best = (dist, e, q);
        }
    }
    (best.1, best.2)
}

/// The plain field with its gradient: `Σ w_e g_e(t_e)` with `w_e ∝ 1 / d_e²`. On or outside the
/// border it is the nearest edge's ramp at the closest boundary point, whose gradient runs along
/// that edge.
fn plain(corners: [u8; 6], p: [f64; 2]) -> Sample {
    let h: [f64; 6] = corners.map(|c| c as f64 * LEVEL_STEP as f64);
    let d: [f64; 6] = std::array::from_fn(|e| edge_distance(e, p));
    if d.iter().any(|&de| de <= BORDER_EPS) {
        let (e, q) = nearest_edge(p);
        return edge_ramp(&h, e, q);
    }
    let (mut s, mut gs) = (0.0, [0.0; 2]);
    let (mut num, mut gnum) = (0.0, [0.0; 2]);
    for (e, &de) in d.iter().enumerate() {
        // q = d⁻², and ∇d = −n, so ∇q = 2 n / d³.
        let q = 1.0 / (de * de);
        let k = 2.0 / (de * de * de);
        let gq = [k * HEX.normals[e][0], k * HEX.normals[e][1]];
        let (g, gg) = edge_ramp(&h, e, p);
        s += q;
        num += q * g;
        for i in 0..2 {
            gs[i] += gq[i];
            gnum[i] += gq[i] * g + q * gg[i];
        }
    }
    let v = num / s;
    (v, [(gnum[0] - v * gs[0]) / s, (gnum[1] - v * gs[1]) / s])
}

/// Which gradient a sample carries: the field's true slope, or the slope the cap shades with.
#[derive(Clone, Copy, PartialEq)]
enum Slope {
    True,
    /// The meadow relief's slope under its border falloff, leaving out the falloff's own rise to
    /// the ring: `_terrain.slope_under`, which the baked meadow cap's normals use.
    Shade,
}

/// The meadow relief with its gradient: [`meadow_relief`] in f64. Under [`Slope::Shade`] the
/// gradient leaves out the border falloff's slope, as `_terrain.meadow_shade_slope` does.
fn relief(p: [f64; 2], slope: Slope) -> Sample {
    // Blender's frame: (x, -z).
    let (bx, by) = (p[0], -p[1]);
    let apothem = HEX_RADIUS as f64 * 3f64.sqrt() / 2.0;
    let (h, k) = (0.5, 3f64.sqrt() / 2.0);
    // The border distance's governing line and its gradient (in Blender's frame). On a ray to a
    // corner two lines tie and the distance has a kink: take the mean of their gradients, the
    // value a central difference sees.
    let lines = [(1.0, 0.0), (h, k), (h, -k)].map(|(a, b): (f64, f64)| {
        let v = a * bx + b * by;
        (v.abs(), [a * v.signum(), b * v.signum()])
    });
    let reach = lines.iter().map(|l| l.0).fold(f64::MIN, f64::max);
    let tied: Vec<[f64; 2]> = lines
        .iter()
        .filter(|l| reach - l.0 < 1e-9)
        .map(|l| l.1)
        .collect();
    let dreach: [f64; 2] =
        std::array::from_fn(|i| tied.iter().map(|g| g[i]).sum::<f64>() / tied.len() as f64);
    let t = (apothem - reach) / 3.0;
    let (falloff, dfall) = if t <= 0.0 || t >= 1.0 {
        (
            t.clamp(0.0, 1.0).powi(2) * (3.0 - 2.0 * t.clamp(0.0, 1.0)),
            [0.0; 2],
        )
    } else {
        let d = -6.0 * t * (1.0 - t) / 3.0;
        (t * t * (3.0 - 2.0 * t), [d * dreach[0], d * dreach[1]])
    };
    let (a1, a2, a3) = (
        0.31 * bx + 1.7,
        0.27 * by - 0.4,
        0.53 * bx - 0.45 * by + 0.9,
    );
    let swell = 0.6 * a1.sin() * a2.cos() + 0.4 * a3.sin();
    let (hollow, dswell) = if swell <= 0.0 || swell >= 1.0 {
        (swell.clamp(0.0, 1.0), [0.0; 2])
    } else {
        (
            swell,
            [
                0.6 * 0.31 * a1.cos() * a2.cos() + 0.4 * 0.53 * a3.cos(),
                -0.6 * 0.27 * a1.sin() * a2.sin() - 0.4 * 0.45 * a3.cos(),
            ],
        )
    };
    let v = -0.25 * falloff * hollow * hollow;
    let dfall = if slope == Slope::Shade {
        [0.0; 2]
    } else {
        dfall
    };
    let g: [f64; 2] = std::array::from_fn(|i| {
        -0.25 * (dfall[i] * hollow * hollow + falloff * 2.0 * hollow * dswell[i])
    });
    // Back to the tile frame: d/dz = −d/dby.
    (v, [g[0], -g[1]])
}

/// The plain field plus the meadow relief: a hillside's ground.
fn plain_ground(corners: [u8; 6], p: [f64; 2], slope: Slope) -> Sample {
    let (v, g) = plain(corners, p);
    let (r, gr) = relief(p, slope);
    (v + r, [g[0] + gr[0], g[1] + gr[1]])
}

/// `inside` across the road corridor `|z| <= 4`, blended to the plain ground (with its relief) by
/// smoothstep over `4 < |z| < 8`, the plain ground beyond: the corridor stays clean and the banks
/// get the hollows.
fn corridor(corners: [u8; 6], p: [f64; 2], inside: Sample, slope: Slope) -> Sample {
    let az = p[1].abs();
    if az <= CORRIDOR {
        return inside;
    }
    let bank = plain_ground(corners, p, slope);
    if az >= BLEND {
        return bank;
    }
    let u = (az - CORRIDOR) / (BLEND - CORRIDOR);
    let t = u * u * (3.0 - 2.0 * u);
    let dt = 6.0 * u * (1.0 - u) / (BLEND - CORRIDOR) * p[1].signum();
    (
        inside.0 * (1.0 - t) + bank.0 * t,
        [
            inside.1[0] * (1.0 - t) + bank.1[0] * t,
            inside.1[1] * (1.0 - t) + bank.1[1] * t + (bank.0 - inside.0) * dt,
        ],
    )
}

/// The road ramp's vertical curve along x: with `s = x + a` clamped to `[0, 2a]`, grade 0 at both
/// edges, parabolic over [`VERTICAL_CURVE`] metres at each end and the constant grade
/// `g = 4 / (2a − L)` between, rising [`LEVEL_STEP`] in all. C1, so it meets flat road without a
/// crease.
fn road_profile(x: f64) -> Sample {
    let (a, l, rise) = (HEX.apothem, VERTICAL_CURVE, LEVEL_STEP as f64);
    let g = rise / (2.0 * a - l);
    let s = (x + a).clamp(0.0, 2.0 * a);
    let (h, dh) = if s < l {
        (g * s * s / (2.0 * l), g * s / l)
    } else if s > 2.0 * a - l {
        let r = 2.0 * a - s;
        (rise - g * r * r / (2.0 * l), g * r / l)
    } else {
        (g * (s - l / 2.0), g)
    };
    (h, [dh, 0.0])
}

fn sample(ground: Ground, corners: [u8; 6], p: [f64; 2]) -> Sample {
    sample_slope(ground, corners, p, Slope::True)
}

fn sample_slope(ground: Ground, corners: [u8; 6], p: [f64; 2], slope: Slope) -> Sample {
    match ground {
        Ground::Plain => plain_ground(corners, p, slope),
        Ground::RoadRamp => corridor(corners, p, road_profile(p[0]), slope),
        Ground::Cutting => corridor(corners, p, (0.0, [0.0; 2]), slope),
    }
}

/// The gradient `(d/dx, d/dz)` the runtime cap shades with: the field's slope with the meadow
/// relief's slope taken under its border falloff, leaving out the falloff's own rise to the ring
/// (`_terrain.slope_under`, the baked meadow cap's normals). It equals the field's true gradient
/// wherever the falloff is 1 or the relief is 0, including on every border, so seams shade alike.
pub fn shade_gradient(ground: Ground, corners: [u8; 6], x: f32, z: f32) -> [f32; 2] {
    sample_slope(ground, corners, [x as f64, z as f64], Slope::Shade)
        .1
        .map(|g| g as f32)
}

/// The meadow relief under tile-frame `(x, z)`: `meadow_relief()` in scripts/blender/kit/_terrain.py
/// at Blender `(x, -z)`. Hollows on a plane level with the border ring: 0 wherever the undulation
/// `0.5 + 0.5 * swell` is under one half, down to -0.25 m where it is 1, faded to 0 over the last
/// 3 m of the border. [`field_at`] adds it to a hillside's ground (fully on `Ground::Plain`, on the
/// banks of a ramp or cutting), so the baked meadow masks AO lands on matching hollows.
pub fn meadow_relief(x: f32, z: f32) -> f32 {
    relief([x as f64, z as f64], Slope::True).0 as f32
}

/// The edge blend `Σ_e w_e(p) · g_e(t_e(p))` of `corners` × [`LEVEL_STEP`], without the meadow
/// relief ([`field_at`] adds it). Edge `e`'s ramp is
/// `g_e(t) = h_start + (h_end − h_start) · smoothstep01(t)` from its start corner to its end
/// corner, `t_e` is `p` projected onto the edge's line (0 at the start, 1 at the end) and clamped
/// to [0, 1], and `w_e ∝ 1 / d_e²` over the six edges with `d_e` the distance to the edge's line.
/// On the border it is the edge's own ramp; outside the hexagon, the value at the nearest boundary
/// point.
pub fn height_at(corners: [u8; 6], x: f32, z: f32) -> f32 {
    plain(corners, [x as f64, z as f64]).0 as f32
}

/// The `Ground::RoadRamp` field (road along +x from edge 3 to edge 0): across the corridor
/// `|z| <= 4`, a vertical curve from 0 at `x = −a` to 4 at `x = a` (`a` the apothem `16 √3 / 2`),
/// grade 0 at both edges, parabolic over 4 m at each end and `4 / (2a − 4)` between; blended to
/// [`height_at`] plus [`meadow_relief`] by smoothstep over `4 < |z| < 8`, that beyond.
pub fn road_ramp_height_at(corners: [u8; 6], x: f32, z: f32) -> f32 {
    sample(Ground::RoadRamp, corners, [x as f64, z as f64]).0 as f32
}

/// The `Ground::Cutting` field (tunnel portal): 0 across the corridor `|z| <= 4`, blended to
/// [`height_at`] plus [`meadow_relief`] by smoothstep over `4 < |z| < 8`, that beyond.
pub fn cutting_height_at(corners: [u8; 6], x: f32, z: f32) -> f32 {
    sample(Ground::Cutting, corners, [x as f64, z as f64]).0 as f32
}

/// The ground field of a tile with `ground` and local `corners`, in metres above its level: the
/// slope field plus [`meadow_relief`] (weighted out of a road corridor).
pub fn field_at(ground: Ground, corners: [u8; 6], x: f32, z: f32) -> f32 {
    sample(ground, corners, [x as f64, z as f64]).0 as f32
}

/// Cap vertices in `rings` concentric hex rings: the centre, then ring `r` (1..=rings) with `6 r`
/// vertices, side `k` running from corner `k` toward corner `k + 1`.
fn cap_points(rings: u32) -> Vec<[f64; 2]> {
    let cs = HEX.corners;
    let n = rings as f64;
    let mut pts = vec![[0.0, 0.0]];
    for r in 1..=rings {
        let s = r as f64 / n;
        for k in 0..6 {
            let (a, b) = (cs[k], cs[(k + 1) % 6]);
            for j in 0..r {
                let t = j as f64 / r as f64;
                pts.push([
                    s * (a[0] + (b[0] - a[0]) * t),
                    s * (a[1] + (b[1] - a[1]) * t),
                ]);
            }
        }
    }
    pts
}

/// Index of vertex `j` on side `k` of ring `r` (wrapping into the next side).
fn ring_index(r: u32, k: u32, j: u32) -> u32 {
    if r == 0 {
        0
    } else {
        1 + 3 * r * (r - 1) + (k * r + j) % (6 * r)
    }
}

/// Cap triangle indices, wound CCW viewed from +Y like the prism top: (inner, next, current).
fn cap_indices(rings: u32) -> Vec<u32> {
    let mut idx = Vec::with_capacity(18 * (rings * rings) as usize);
    for r in 1..=rings {
        for k in 0..6 {
            for j in 0..r {
                let (a, b) = (ring_index(r, k, j), ring_index(r, k, j + 1));
                idx.extend_from_slice(&[ring_index(r - 1, k, j), b, a]);
            }
            for j in 0..r - 1 {
                let (c, d) = (ring_index(r - 1, k, j), ring_index(r - 1, k, j + 1));
                idx.extend_from_slice(&[c, d, ring_index(r, k, j + 1)]);
            }
        }
    }
    idx
}

fn unit_normal(g: [f64; 2]) -> [f32; 3] {
    let len = (g[0] * g[0] + 1.0 + g[1] * g[1]).sqrt();
    [
        (-g[0] / len) as f32,
        (1.0 / len) as f32,
        (-g[1] / len) as f32,
    ]
}

/// The ground mesh of a tile: the cap subdivided in `rings` concentric hex rings (12 in play) with
/// y from [`field_at`] and smooth normals from its analytic shading gradient ([`shade_gradient`],
/// the baked meadow cap's shading), UVs on meadow's planar cap mapping `u = (x + 16) / 32`,
/// `v = (16 - z) / 32`, and a skirt wall per edge from the cap boundary down to `-LEVEL_STEP`,
/// sharing the cap's boundary positions exactly.
pub fn ground_mesh(ground: Ground, corners: [u8; 6], rings: u32) -> SlopeMesh {
    let rings = rings.max(1);
    let mut cap = PrismMesh {
        positions: vec![],
        normals: vec![],
        uvs: vec![],
        indices: cap_indices(rings),
    };
    for p in cap_points(rings) {
        let y = sample(ground, corners, p).0;
        let g = sample_slope(ground, corners, p, Slope::Shade).1;
        let (x, z) = (p[0] as f32, p[1] as f32);
        cap.positions.push([x, y as f32, z]);
        cap.normals.push(unit_normal(g));
        cap.uvs.push([(x + 16.0) / 32.0, (16.0 - z) / 32.0]);
    }

    let mut skirt = PrismMesh {
        positions: vec![],
        normals: vec![],
        uvs: vec![],
        indices: vec![],
    };
    for k in 0..6u32 {
        // Side k of the outer ring is edge k + 1, whose outward normal is at 60° · (k + 1).
        let a = (60.0 * (k + 1) as f32).to_radians();
        let n = [a.cos(), 0.0, a.sin()];
        let base = skirt.positions.len() as u32;
        for j in 0..=rings {
            let top = cap.positions[ring_index(rings, k, j) as usize];
            skirt.positions.push(top);
            skirt.positions.push([top[0], -LEVEL_STEP, top[2]]);
            skirt.normals.extend_from_slice(&[n, n]);
            skirt.uvs.extend_from_slice(&[SKIRT_UV, SKIRT_UV]);
        }
        for j in 0..rings {
            let (t0, b0) = (base + 2 * j, base + 2 * j + 1);
            let (t1, b1) = (t0 + 2, b0 + 2);
            skirt.indices.extend_from_slice(&[b0, t1, b1, b0, t0, t1]);
        }
    }
    SlopeMesh { cap, skirt }
}

/// The cap triangles of [`ground_mesh`], for scatter and walkers.
pub fn cap_tris(ground: Ground, corners: [u8; 6], rings: u32) -> Vec<[[f32; 3]; 3]> {
    let cap = ground_mesh(ground, corners, rings).cap;
    cap.indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| t.map(|i| cap.positions[i as usize]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::TileCatalog;
    use crate::prism::corner;
    use crate::socket::{EdgeProfile, HeightMeet, heights_meet};
    use crate::tile::{TileDef, edge_corners, local_edge};
    use crate::world::MAX_LEVEL;

    const RINGS: u32 = 12;

    fn apothem32() -> f32 {
        HEX_RADIUS * 3f32.sqrt() / 2.0
    }

    fn smoothstep01(t: f32) -> f32 {
        t * t * (3.0 - 2.0 * t)
    }

    fn catalog() -> TileCatalog {
        TileCatalog::builtin().unwrap()
    }

    /// The 12 hillside defs.
    fn hillsides(c: &TileCatalog) -> Vec<&TileDef> {
        let h: Vec<&TileDef> = c
            .tiles()
            .iter()
            .filter(|d| d.id.starts_with("hillside_"))
            .collect();
        assert_eq!(h.len(), 12);
        h
    }

    fn lerp2(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
    }

    /// Rotates a point by `60° · steps` (positive toward +z, the corner order).
    fn rotate(p: [f32; 2], steps: i32) -> [f32; 2] {
        let a = (60.0 * steps as f32).to_radians();
        let (s, c) = a.sin_cos();
        [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
    }

    fn inside(x: f32, z: f32, margin: f32) -> bool {
        (0..6).all(|e| {
            let a = (60.0 * e as f32).to_radians();
            x * a.cos() + z * a.sin() <= apothem32() - margin
        })
    }

    fn tri_normal(t: [[f32; 3]; 3]) -> [f32; 3] {
        let [a, b, c] = t;
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ]
    }

    fn mesh_tris(m: &PrismMesh) -> Vec<[[f32; 3]; 3]> {
        m.indices
            .chunks(3)
            .map(|t| [0, 1, 2].map(|k| m.positions[t[k] as usize]))
            .collect()
    }

    /// Whether `def`'s edge facing `world` under `rotation` is a bank edge whose cap must join its
    /// neighbour's: every edge of a plain tile, and the non-road edges 1, 2, 4 and 5 of the road
    /// ramp and the tunnel portal (the road edges 0 and 3 carry the ramp grade, and the portal's
    /// edge 0 dips into its cutting by design).
    fn bank_edge(def: &TileDef, world: u8, rotation: u8) -> bool {
        def.ground == Ground::Plain || ![0, 3].contains(&local_edge(world, rotation))
    }

    /// Every (hillside, meadow, road ramp or tunnel portal, rotation, level) pair that meets
    /// seamlessly across world edge 0 of tile A at the origin and world edge 3 of tile B east of
    /// it, on bank edges only.
    fn seamless_pairs(c: &TileCatalog) -> Vec<(&TileDef, u8, u8, &TileDef, u8, u8)> {
        let mut defs = hillsides(c);
        for id in ["meadow", "road_ramp", "road_tunnel_portal"] {
            defs.push(c.get(id).unwrap());
        }
        let mut out = vec![];
        for &a in &defs {
            for &b in &defs {
                for ra in 0..6u8 {
                    for rb in 0..6u8 {
                        if !bank_edge(a, 0, ra) || !bank_edge(b, 3, rb) {
                            continue;
                        }
                        for la in 0..=MAX_LEVEL - a.top() {
                            for lb in 0..=MAX_LEVEL - b.top() {
                                if heights_meet(a.edge(0, ra), la, b.edge(3, rb), lb)
                                    == HeightMeet::Seamless
                                {
                                    out.push((a, ra, la, b, rb, lb));
                                }
                            }
                        }
                    }
                }
            }
        }
        out
    }

    /// World edge 0 of the origin tile, from its start corner to its end corner, and the centre of
    /// the neighbour across it.
    fn shared_edge() -> ([f32; 2], [f32; 2], [f32; 2]) {
        let (s, e) = edge_corners(0);
        (
            corner(HEX_RADIUS, s as usize),
            corner(HEX_RADIUS, e as usize),
            [2.0 * apothem32(), 0.0],
        )
    }

    /// The world point `w` in tile A's and tile B's own unrotated local frames.
    fn local_points(w: [f32; 2], east: [f32; 2], ra: u8, rb: u8) -> ([f32; 2], [f32; 2]) {
        (
            rotate(w, -(ra as i32)),
            rotate([w[0] - east[0], w[1] - east[1]], -(rb as i32)),
        )
    }

    #[test]
    fn exact_at_corners() {
        let c = catalog();
        let mut patterns: Vec<[u8; 6]> = hillsides(&c).iter().map(|d| d.corners(0)).collect();
        patterns.push([0; 6]);
        patterns.push([1, 1, 0, 0, 0, 1]);
        for corners in patterns {
            for (k, &ck) in corners.iter().enumerate() {
                let [x, z] = corner(HEX_RADIUS, k);
                let h = height_at(corners, x, z);
                let want = ck as f32 * LEVEL_STEP;
                assert!(
                    (h - want).abs() < 1e-4,
                    "{corners:?} corner {k}: {h} vs {want}"
                );
            }
        }
    }

    #[test]
    fn eased_along_edges() {
        let corners = [1, 0, 0, 0, 0, 0];
        let (a, b) = (corner(HEX_RADIUS, 0), corner(HEX_RADIUS, 1));
        for t in [0.25, 0.5, 0.75] {
            let [x, z] = lerp2(a, b, t);
            let h = height_at(corners, x, z);
            let want = 4.0 * smoothstep01(1.0 - t);
            assert!((h - want).abs() < 1e-3, "t {t}: {h} vs {want}");
        }
    }

    /// No crease against flat ground: on every flat edge of every hillside (at h0 or h1) the cap's
    /// normal is straight up, in the field's gradient and on the mesh's boundary vertices.
    #[test]
    fn flat_edges_meet_flat_ground() {
        let c = catalog();
        let mut checked = 0;
        for def in hillsides(&c) {
            let corners = def.corners(0);
            let m = ground_mesh(def.ground, corners, RINGS);
            for (e, edge) in def.edges.iter().enumerate() {
                if edge.profile != EdgeProfile::Flat {
                    continue;
                }
                let (s, t) = edge_corners(e as u8);
                let (a, b) = (
                    corner(HEX_RADIUS, s as usize),
                    corner(HEX_RADIUS, t as usize),
                );
                for i in 0..=RINGS {
                    let [x, z] = lerp2(a, b, i as f32 / RINGS as f32);
                    let n = unit_normal(sample(def.ground, corners, [x as f64, z as f64]).1);
                    let v = vertex_normal(&m.cap, [x, z], 0).expect("boundary vertex");
                    for n in [n, v] {
                        assert!(
                            n[0].abs() < 1e-3 && (n[1] - 1.0).abs() < 1e-3 && n[2].abs() < 1e-3,
                            "{} edge {e} at ({x}, {z}): {n:?}",
                            def.id
                        );
                    }
                    checked += 1;
                }
            }
        }
        assert!(checked > 0);
    }

    #[test]
    fn flat_is_flat() {
        for level in 0..=MAX_LEVEL {
            let corners = [level; 6];
            let want = level as f32 * LEVEL_STEP;
            for i in -20..=20 {
                for j in -20..=20 {
                    let (x, z) = (i as f32, j as f32);
                    let h = height_at(corners, x, z);
                    assert!((h - want).abs() < 1e-5, "{level} at ({x}, {z}): {h}");
                }
            }
        }
    }

    #[test]
    fn rotation_into_local_frame_matches_world_corners() {
        let c = catalog();
        for d in hillsides(&c) {
            for r in 0..6u8 {
                for p in [[3.0, 1.0], [-5.0, 7.0], [10.0, -4.0]] {
                    let local = rotate(p, -(r as i32));
                    let a = height_at(d.corners(0), local[0], local[1]);
                    let b = height_at(d.corners(r), p[0], p[1]);
                    assert!((a - b).abs() < 1e-3, "{} r{r} {p:?}: {a} vs {b}", d.id);
                }
            }
        }
    }

    #[test]
    fn neighbours_join() {
        let c = catalog();
        let pairs = seamless_pairs(&c);
        assert!(pairs.len() > 100, "{} pairs", pairs.len());
        let (p0, p1, east) = shared_edge();
        for (a, ra, la, b, rb, lb) in pairs {
            for i in 0..=16 {
                let w = lerp2(p0, p1, i as f32 / 16.0);
                let (pa, pb) = local_points(w, east, ra, rb);
                let ha = la as f32 * LEVEL_STEP + field_at(a.ground, a.corners(0), pa[0], pa[1]);
                let hb = lb as f32 * LEVEL_STEP + field_at(b.ground, b.corners(0), pb[0], pb[1]);
                assert!(
                    (ha - hb).abs() < 1e-3,
                    "{} r{ra} l{la} vs {} r{rb} l{lb} at {w:?}: {ha} vs {hb}",
                    a.id,
                    b.id
                );
            }
        }
    }

    /// The world-frame normal of the cap vertex at local point `p`, if one is there.
    fn vertex_normal(cap: &PrismMesh, p: [f32; 2], rotation: u8) -> Option<[f32; 3]> {
        let i = cap
            .positions
            .iter()
            .position(|v| (v[0] - p[0]).abs() < 1e-3 && (v[2] - p[1]).abs() < 1e-3)?;
        let n = cap.normals[i];
        let [x, z] = rotate([n[0], n[2]], rotation as i32);
        Some([x, n[1], z])
    }

    /// The world-frame normal of the field (analytic gradient) at local point `p`.
    fn field_normal(def: &TileDef, p: [f32; 2], rotation: u8) -> [f32; 3] {
        let g = sample(def.ground, def.corners(0), [p[0] as f64, p[1] as f64]).1;
        let n = unit_normal(g);
        let [x, z] = rotate([n[0], n[2]], rotation as i32);
        [x, n[1], z]
    }

    /// Every seam is C1: across every seamless bank-edge pair the normals agree within 1e-3, both
    /// on the mesh's shared boundary vertices and in the field approached from 0.1 mm inside each
    /// tile (so the interior blend, not only the border branch, is held to it).
    #[test]
    fn shared_edge_normals() {
        let c = catalog();
        let (p0, p1, east) = shared_edge();
        let inset = 1e-4;
        let mut worst = 0.0f32;
        for (a, ra, _, b, rb, _) in seamless_pairs(&c) {
            let ma = ground_mesh(a.ground, a.corners(0), RINGS);
            let mb = ground_mesh(b.ground, b.corners(0), RINGS);
            for i in 0..=RINGS {
                let w = lerp2(p0, p1, i as f32 / RINGS as f32);
                let (pa, pb) = local_points(w, east, ra, rb);
                let na = vertex_normal(&ma.cap, pa, ra).expect("boundary vertex on A");
                let nb = vertex_normal(&mb.cap, pb, rb).expect("boundary vertex on B");
                // World edge 0 faces +x: A's interior is at -x, B's at +x.
                let (ia, _) = local_points([w[0] - inset, w[1]], east, ra, rb);
                let (_, ib) = local_points([w[0] + inset, w[1]], east, ra, rb);
                let fa = field_normal(a, ia, ra);
                let fb = field_normal(b, ib, rb);
                for k in 0..3 {
                    worst = worst.max((na[k] - nb[k]).abs()).max((fa[k] - fb[k]).abs());
                }
            }
        }
        assert!(worst < 1e-3, "seam normal mismatch {worst}");
    }

    /// The ramp's road runs a vertical curve: grade 0 and heights 0 and 4 at edges 3 and 0, the
    /// constant grade `4 / (2a − 4)` between the two 4 m parabolic transitions, C1 at both joins.
    #[test]
    fn ramp_grade() {
        let corners = [1, 1, 0, 0, 0, 1];
        let a = HEX.apothem;
        let mid = 4.0 / (2.0 * a - 4.0);
        let f = |x: f64| sample(Ground::RoadRamp, corners, [x, 0.0]).0;
        let slope = |x0: f64, x1: f64| (f(x1) - f(x0)) / (x1 - x0);
        let analytic = |x: f64| sample(Ground::RoadRamp, corners, [x, 0.0]).1[0];

        for i in 0..=40 {
            let x = -a + 4.1 + (2.0 * a - 8.2) * i as f64 / 40.0;
            let g = slope(x - 0.05, x + 0.05);
            assert!((g - mid).abs() < 1e-4, "middle grade {g} at x {x}");
            assert!((analytic(x) - mid).abs() < 1e-4);
        }
        let e = 1e-4;
        for (x, inward, want) in [(-a, e, 0.0), (a, -e, 4.0)] {
            let g = slope(x, x + inward);
            assert!(
                g.abs() < 1e-3 && analytic(x).abs() < 1e-3,
                "edge grade {g} at x {x}"
            );
            let h = road_ramp_height_at(corners, x as f32, 0.0);
            assert!((h - want).abs() < 1e-4, "edge height {h} at x {x}");
        }
        for x in [-a + 4.0, a - 4.0] {
            let (left, right) = (slope(x - e, x), slope(x, x + e));
            assert!(
                (left - right).abs() < 1e-3,
                "join at {x}: {left} vs {right}"
            );
            assert!((left - mid).abs() < 1e-3, "join at {x}: {left} vs {mid}");
        }

        let d = 0.01;
        for i in 0..=52 {
            let x = -13.0 + 0.5 * i as f32;
            for j in 0..=12 {
                let z = -3.0 + 0.5 * j as f32;
                let gz = (road_ramp_height_at(corners, x, z + d)
                    - road_ramp_height_at(corners, x, z - d))
                    / (2.0 * d);
                assert!(gz.abs() < 0.01, "cross slope {gz} at ({x}, {z})");
            }
        }
        // Edges 0 and 3 hold one height in both fields, so the blend joins neighbours.
        for e in [0u8, 3] {
            let (s, t) = edge_corners(e);
            let (a, b) = (
                corner(HEX_RADIUS, s as usize),
                corner(HEX_RADIUS, t as usize),
            );
            for i in 0..=32 {
                let [x, z] = lerp2(a, b, i as f32 / 32.0);
                let r = road_ramp_height_at(corners, x, z);
                let h = height_at(corners, x, z);
                assert!((r - h).abs() < 1e-4, "edge {e} at ({x}, {z}): {r} vs {h}");
            }
        }
        // So the cap's normals on both road edges are straight up, matching the flat road.
        let m = ground_mesh(Ground::RoadRamp, corners, RINGS);
        for (p, n) in m.cap.positions.iter().zip(&m.cap.normals) {
            if (p[0].abs() - apothem32()).abs() < 1e-3 {
                assert!(
                    n[0].abs() < 1e-3 && (n[1] - 1.0).abs() < 1e-3 && n[2].abs() < 1e-3,
                    "road edge normal {n:?} at {p:?}"
                );
            }
        }
    }

    /// Measured maximum with the meadow relief over the same sample, plus 0.02.
    const RELIEF_BOUND: f32 = 0.445;

    /// Measured maximum plus 0.02 over a 64×64 interior sample. Every pattern peaks at the eased
    /// edge ramp's own steepest slope, 1.5 × 4 / 16 = 0.375: 0.378 for the three with no lone
    /// corner (one differing from both neighbours) and 0.377 for the other nine, so one bound
    /// covers both classes.
    #[test]
    fn bank_grade_is_bounded() {
        let c = catalog();
        let d = 0.05;
        for def in hillsides(&c) {
            let corners = def.corners(0);
            let bound = 0.40;
            let mut worst = 0.0f32;
            for i in 0..64 {
                for j in 0..64 {
                    let x = -HEX_RADIUS + 2.0 * HEX_RADIUS * (i as f32 + 0.5) / 64.0;
                    let z = -HEX_RADIUS + 2.0 * HEX_RADIUS * (j as f32 + 0.5) / 64.0;
                    if !inside(x, z, 2.0 * d) {
                        continue;
                    }
                    let gx =
                        (height_at(corners, x + d, z) - height_at(corners, x - d, z)) / (2.0 * d);
                    let gz =
                        (height_at(corners, x, z + d) - height_at(corners, x, z - d)) / (2.0 * d);
                    worst = worst.max((gx * gx + gz * gz).sqrt());
                }
            }
            assert!(worst < bound, "{}: max grade {worst}", def.id);
            // The ground as drawn, with the meadow relief: its hollows steepen the banks.
            let mut drawn = 0.0f32;
            for i in 0..64 {
                for j in 0..64 {
                    let x = -HEX_RADIUS + 2.0 * HEX_RADIUS * (i as f32 + 0.5) / 64.0;
                    let z = -HEX_RADIUS + 2.0 * HEX_RADIUS * (j as f32 + 0.5) / 64.0;
                    if !inside(x, z, 2.0 * d) {
                        continue;
                    }
                    let f = |x: f32, z: f32| field_at(Ground::Plain, corners, x, z);
                    let gx = (f(x + d, z) - f(x - d, z)) / (2.0 * d);
                    let gz = (f(x, z + d) - f(x, z - d)) / (2.0 * d);
                    drawn = drawn.max((gx * gx + gz * gz).sqrt());
                }
            }
            assert!(
                drawn < RELIEF_BOUND,
                "{}: max grade with relief {drawn}",
                def.id
            );
        }
    }

    #[test]
    fn fields_match_python_reference() {
        // The samples are `scripts/blender/kit/slope_samples.json`, also read by the tunnel-portal
        // generator's `check_field` (`_tunnel_portal.py`, `cut_height_at`). They come from a
        // separate python3 implementation of the definition (with the meadow relief since Task 7
        // fix round 1, sampled where the relief carves): the edge blend with inverse squared
        // distance weights, eased edge ramps, the nearest-boundary clamp, the corridor 4 m blended
        // to 8 m, the ramp's vertical curve (4 m parabolic ends) and the apothem 16 √3 / 2. A
        // change to the Rust field fails here until the file and the generator are updated.
        let samples: serde_json::Value = serde_json::from_str(include_str!(
            "../../../scripts/blender/kit/slope_samples.json"
        ))
        .unwrap();
        let rows = |key: &str| -> Vec<(f32, f32, f32)> {
            samples[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| {
                    let v = |i: usize| r[i].as_f64().unwrap() as f32;
                    (v(0), v(1), v(2))
                })
                .collect()
        };
        let (cut, ramp, plain) = (rows("cutting"), rows("ramp"), rows("plain"));
        assert_eq!((cut.len(), ramp.len(), plain.len()), (12, 8, 12));
        let corners = [1, 1, 0, 0, 0, 1];
        assert_eq!(samples["corners"], serde_json::json!(corners));
        for (x, z, want) in cut {
            let h = cutting_height_at(corners, x, z);
            assert_eq!(h, field_at(Ground::Cutting, corners, x, z));
            assert!((h - want).abs() < 1e-4, "cut ({x}, {z}): {h} vs {want}");
        }
        for (x, z, want) in ramp {
            let h = road_ramp_height_at(corners, x, z);
            assert_eq!(h, field_at(Ground::RoadRamp, corners, x, z));
            assert!((h - want).abs() < 1e-4, "ramp ({x}, {z}): {h} vs {want}");
        }
        for (x, z, want) in plain {
            let h = field_at(Ground::Plain, corners, x, z);
            let bare = height_at(corners, x, z) + meadow_relief(x, z);
            assert!((h - bare).abs() < 1e-5, "plain ({x}, {z}): {h} vs {bare}");
            assert!((h - want).abs() < 1e-4, "plain ({x}, {z}): {h} vs {want}");
        }
    }

    #[test]
    fn mesh_has_no_cracks() {
        let c = catalog();
        let mut cases: Vec<(Ground, [u8; 6])> = hillsides(&c)
            .iter()
            .map(|d| (Ground::Plain, d.corners(0)))
            .collect();
        cases.push((Ground::RoadRamp, [1, 1, 0, 0, 0, 1]));
        cases.push((Ground::Cutting, [1, 1, 0, 0, 0, 1]));
        for (ground, corners) in cases {
            let m = ground_mesh(ground, corners, RINGS);
            let boundary: Vec<[f32; 3]> = m
                .cap
                .positions
                .iter()
                .copied()
                .filter(|p| !inside(p[0], p[2], 1e-3))
                .collect();
            assert_eq!(boundary.len(), 6 * RINGS as usize, "{corners:?}");
            for p in &boundary {
                let on_edge = (0..6).any(|e| {
                    let a = (60.0 * e as f32).to_radians();
                    (p[0] * a.cos() + p[2] * a.sin() - apothem32()).abs() < 1e-3
                });
                let r = (p[0] * p[0] + p[2] * p[2]).sqrt();
                assert!(
                    on_edge && r <= HEX_RADIUS + 1e-3,
                    "{p:?} is off the hexagon"
                );
            }
            for p in &m.cap.positions {
                let h = field_at(ground, corners, p[0], p[2]);
                assert!(
                    (p[1] - h).abs() < 1e-4,
                    "{ground:?} {corners:?} {p:?} vs {h}"
                );
            }
            // Every skirt top is exactly a cap boundary vertex; every bottom is at -LEVEL_STEP.
            let mut bottoms = 0;
            for p in &m.skirt.positions {
                if p[1] == -LEVEL_STEP {
                    bottoms += 1;
                } else {
                    assert!(
                        boundary.contains(p),
                        "{corners:?} skirt top {p:?} not on the cap"
                    );
                }
            }
            assert_eq!(bottoms * 2, m.skirt.positions.len());
        }
    }

    #[test]
    fn cap_faces_up_and_skirt_faces_out() {
        let corners = [0, 1, 1, 1, 1, 1];
        let m = ground_mesh(Ground::Plain, corners, RINGS);
        assert_eq!(m.cap.indices.len(), 3 * 6 * (RINGS * RINGS) as usize);
        for t in mesh_tris(&m.cap) {
            assert!(tri_normal(t)[1] > 0.0, "cap tri {t:?} faces down");
        }
        for t in mesh_tris(&m.skirt) {
            let n = tri_normal(t);
            let mid = [
                (t[0][0] + t[1][0] + t[2][0]) / 3.0,
                (t[0][2] + t[1][2] + t[2][2]) / 3.0,
            ];
            assert!(
                n[0] * mid[0] + n[2] * mid[1] > 0.0,
                "skirt tri {t:?} faces in"
            );
            assert!(
                n[1].abs() < 1e-3 * (n[0].abs() + n[2].abs()),
                "skirt tri {t:?} is not a wall"
            );
        }
        for (mesh, name) in [(&m.cap, "cap"), (&m.skirt, "skirt")] {
            assert_eq!(mesh.normals.len(), mesh.positions.len(), "{name}");
            assert_eq!(mesh.uvs.len(), mesh.positions.len(), "{name}");
        }
        assert_eq!(cap_tris(Ground::Plain, corners, RINGS), mesh_tris(&m.cap));
    }

    #[test]
    fn cap_uvs_follow_meadow_mapping() {
        let m = ground_mesh(Ground::Plain, [0, 0, 1, 1, 0, 0], RINGS);
        for (p, uv) in m.cap.positions.iter().zip(&m.cap.uvs) {
            assert_eq!(*uv, [(p[0] + 16.0) / 32.0, (16.0 - p[2]) / 32.0]);
        }
        assert!(m.skirt.uvs.iter().all(|uv| *uv == [0.005, 0.005]));
    }

    /// `_terrain.falloff` at tile-frame `(x, z)`: smoothstep(0, 3 m, distance to the border), in
    /// Blender's `(x, -z)`. Written from the Python, independent of `relief`.
    fn terrain_falloff(x: f64, z: f64) -> f64 {
        let (bx, by) = (x, -z);
        let reach = (0..6)
            .map(|k| {
                let a = (60.0 * k as f64).to_radians();
                bx * a.cos() + by * a.sin()
            })
            .fold(f64::MIN, f64::max);
        let t = ((HEX.apothem - reach) / 3.0).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// `_terrain.meadow_depth` at tile-frame `(x, z)`: 0.25 · hollow(undulation).
    fn terrain_depth(x: f64, z: f64) -> f64 {
        let (bx, by) = (x, -z);
        let und = 0.5
            + 0.5
                * (0.6 * (0.31 * bx + 1.7).sin() * (0.27 * by - 0.4).cos()
                    + 0.4 * (0.53 * bx - 0.45 * by + 0.9).sin());
        let u = ((und - 0.5) / 0.5).clamp(0.0, 1.0);
        0.25 * u * u
    }

    /// `_terrain.slope_under(meadow_depth, falloff)` in the tile frame: `-fall · ∇depth`, the
    /// baked meadow cap's shading slope (central differences, `H` = 1e-5 as in the Python).
    fn slope_under(x: f64, z: f64) -> [f64; 2] {
        let h = 1e-5;
        let f = terrain_falloff(x, z);
        [
            -f * (terrain_depth(x + h, z) - terrain_depth(x - h, z)) / (2.0 * h),
            -f * (terrain_depth(x, z + h) - terrain_depth(x, z - h)) / (2.0 * h),
        ]
    }

    /// Within the 3 m border band, where the falloff bends the relief, a hillside cap shades with
    /// the baked meadow's slope: the plain field's slope plus `slope_under`, the relief's slope
    /// without the falloff's own rise. So no shading band runs beside a hillside–meadow seam.
    #[test]
    fn hillside_shading_matches_the_meadow_shade_slope() {
        let c = catalog();
        let (mut n, mut worst) = (0, 0.0f64);
        for def in hillsides(&c) {
            let corners = def.corners(0);
            for i in -64..=64 {
                for j in -64..=64 {
                    let (x, z) = (i as f32 * 0.25, j as f32 * 0.25);
                    let (xd, zd) = (x as f64, z as f64);
                    let fall = terrain_falloff(xd, zd);
                    if !inside(x, z, 0.05) || fall >= 1.0 || terrain_depth(xd, zd) == 0.0 {
                        continue;
                    }
                    let base = plain(corners, [xd, zd]).1;
                    let under = slope_under(xd, zd);
                    let want = [base[0] + under[0], base[1] + under[1]];
                    let got = shade_gradient(Ground::Plain, corners, x, z);
                    for k in 0..2 {
                        let e = (got[k] as f64 - want[k]).abs();
                        worst = worst.max(e);
                        assert!(e < 1e-4, "{} at ({x}, {z}): {got:?} vs {want:?}", def.id);
                    }
                    n += 1;
                }
            }
        }
        println!("{n} band points, worst {worst:.2e}");
        assert!(n > 500, "the band holds hollows: {n} points");
        // On the border the falloff's slope is 0: shading and true gradients agree there.
        let corners = [0, 0, 0, 1, 1, 1];
        for k in 0..6 {
            for t in [0.2, 0.5, 0.8] {
                let (a, b) = (HEX.corners[k], HEX.corners[(k + 1) % 6]);
                let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                let tg = sample(Ground::Plain, corners, p).1;
                let sg = shade_gradient(Ground::Plain, corners, p[0] as f32, p[1] as f32);
                assert!((tg[0] - sg[0] as f64).abs() < 1e-5 && (tg[1] - sg[1] as f64).abs() < 1e-5);
            }
        }
    }

    /// The cap normals come from the analytic shading gradient ([`shade_gradient`]): central
    /// differences of the field's value (in f64) at each vertex, less the meadow relief's falloff
    /// slope (the bank's share of `−∇falloff · depth`), or 1e-4 m inside a boundary vertex so the
    /// stencil never crosses the boundary clamp.
    #[test]
    fn normals_match_field_gradient() {
        let cases = [
            (Ground::Plain, [0, 0, 0, 1, 1, 1]),
            (Ground::Plain, [1, 0, 1, 0, 1, 0]),
            (Ground::RoadRamp, [1, 1, 0, 0, 0, 1]),
            (Ground::Cutting, [1, 1, 0, 0, 0, 1]),
        ];
        let d = 1e-6f64;
        for (ground, corners) in cases {
            let f = |x: f64, z: f64| sample(ground, corners, [x, z]).0;
            // The relief's share of the field: all of it on plain ground, the bank's on a ramp.
            let bank = |z: f64| match ground {
                Ground::Plain => 1.0,
                _ => {
                    let u = ((z.abs() - CORRIDOR) / (BLEND - CORRIDOR)).clamp(0.0, 1.0);
                    u * u * (3.0 - 2.0 * u)
                }
            };
            let m = ground_mesh(ground, corners, RINGS);
            for (p, n) in m.cap.positions.iter().zip(&m.cap.normals) {
                let s = if inside(p[0], p[2], 1e-3) {
                    1.0
                } else {
                    1.0 - 1e-4 / (p[0] as f64).hypot(p[2] as f64)
                };
                let (x, z) = (p[0] as f64 * s, p[2] as f64 * s);
                let lift = bank(z) * terrain_depth(x, z);
                let gx = (f(x + d, z) - f(x - d, z)) / (2.0 * d)
                    + lift * (terrain_falloff(x + d, z) - terrain_falloff(x - d, z)) / (2.0 * d);
                let gz = (f(x, z + d) - f(x, z - d)) / (2.0 * d)
                    + lift * (terrain_falloff(x, z + d) - terrain_falloff(x, z - d)) / (2.0 * d);
                let len = (gx * gx + 1.0 + gz * gz).sqrt();
                let want = [-gx / len, 1.0 / len, -gz / len];
                for (k, w) in want.iter().enumerate() {
                    assert!(
                        (n[k] as f64 - w).abs() < 1e-3,
                        "{ground:?} {corners:?} at {p:?}: {n:?} vs {want:?}"
                    );
                }
                let unit = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                assert!((unit - 1.0).abs() < 1e-5);
            }
        }
    }
}
