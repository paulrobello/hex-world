//! Waterfalls in scatter (rivers-at-levels, Task 12): nothing stands within [`FALL_CLEAR`] of a
//! fall's seam on either side, and the lower tile's plunge pool is ringed by 0 to
//! [`FALL_ROCKS_MAX`] stone clusters ([`fall_rocks`]).
//!
//! A fall is applied to a finished [`Layout`] ([`with_falls`]), after the darts, the way a field
//! boundary is, so the darts' stream is untouched and a tile with no fall keeps its layout bit for
//! bit. Lily clusters and reed beds keep out of the pool disc ([`PLUNGE_R`](crate::water::tiles::PLUNGE_R)) because the disc lies
//! inside the clear zone.

use bevy::prelude::*;
use hex_core::{HEX_RADIUS, TileDef};

use super::super::APOTHEM;
use super::bands::{bank_band, floor_slots};
use super::layout::{Instance, Layout, Prepared, Rng, along_yaw, contour, scale_jitter, stand};
use super::tables::{family, footprint, group_footprint, pick_variant, variant_piece};

/// No scatter instance stands within this (m) of a waterfall's seam line, either side. Covers the
/// pool disc ([`PLUNGE_R`](crate::water::tiles::PLUNGE_R)) and the three-level fall's rock, set out 3 m into the lower tile.
pub const FALL_CLEAR: f32 = 6.0;

/// Most stone clusters ringing a plunge pool (first value pending the owner's look).
pub const FALL_ROCKS_MAX: usize = 2;

/// The ring (m from the seam's midpoint, on the lower tile) a pool's stone clusters stand in.
pub const FALL_ROCK_RING: (f32, f32) = (5.0, 6.5);

/// A waterfall seam on a tile, in the tile's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FallSeam {
    /// The seam's midpoint on the hex edge.
    pub mid: Vec2,
    /// The edge's outward unit normal.
    pub out: Vec2,
    /// Whether this tile is the lower side (the pool).
    pub lower: bool,
}

impl FallSeam {
    /// The seam on local edge `edge` of a tile.
    pub fn on_edge(edge: u8, lower: bool) -> Self {
        let a = (60.0 * f32::from(edge % 6)).to_radians();
        let out = Vec2::new(a.cos(), a.sin());
        FallSeam {
            mid: out * APOTHEM,
            out,
            lower,
        }
    }

    /// Distance (m) from `p` to the seam line, the hex edge's full length.
    pub fn distance(&self, p: Vec2) -> f32 {
        let half = self.out.perp() * (HEX_RADIUS * 0.5);
        let (a, b) = (self.mid - half, self.mid + half);
        let ab = b - a;
        let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
        p.distance(a + ab * t)
    }
}

/// The seams of a tile turned `rotation` steps, from its falls as (world edge, is the upper side).
pub fn fall_seams(falls: &[(u8, bool)], rotation: u8) -> Vec<FallSeam> {
    falls
        .iter()
        .map(|&(world, upper)| FallSeam::on_edge((world % 6 + 6 - rotation % 6) % 6, !upper))
        .collect()
}

/// A fingerprint of `seams` for a redraw key: 0 for none, else a hash of each local edge and side.
pub fn seams_sig(seams: &[FallSeam]) -> u64 {
    let mut h: u64 = 0;
    for s in seams {
        let k = ((s.out.y.atan2(s.out.x) * 1000.0).round() as i64 as u64) << 1 | u64::from(s.lower);
        h = (h ^ k.wrapping_add(1)).wrapping_mul(0x0100_0000_01B3);
    }
    h
}

/// `layout` as a tile with `seams` draws it: every instance within [`FALL_CLEAR`] of a seam
/// dropped (not replaced, so a tile may place fewer than planned), then each pool ringed with
/// [`fall_rocks`]. A tile with no seam is returned as it is.
pub fn with_falls(
    mut layout: Layout,
    def: &TileDef,
    seed: u64,
    ground: &Prepared,
    seams: &[FallSeam],
) -> Layout {
    if seams.is_empty() {
        return layout;
    }
    layout.instances.retain(|i| {
        let p = i.pos.xz();
        seams.iter().all(|s| s.distance(p) >= FALL_CLEAR)
    });
    let rocks = fall_rocks(def, seed, ground, seams, &layout.instances);
    if let Some(r) = rocks.first().map(|i| i.rule as usize) {
        layout.planned[r] += rocks.len() as u32;
    }
    layout.instances.extend(rocks);
    let mut by_distance: Vec<usize> = (0..layout.instances.len()).collect();
    by_distance.sort_by(|&a, &b| {
        let d = |i: usize| layout.instances[i].pos.xz().length();
        d(a).total_cmp(&d(b)).then(a.cmp(&b))
    });
    for (rank, i) in by_distance.into_iter().enumerate() {
        layout.instances[i].order = rank.min(u8::MAX as usize) as u8;
    }
    layout
}

/// The stone clusters ringing each lower seam's pool: 0 to [`FALL_ROCKS_MAX`] per pool, each
/// [`FALL_ROCK_RING`] from the seam's midpoint on a bank floor, clear of `placed` and of each
/// other. Drawn from a stream of its own, so nothing else moves. Empty on a tile that scatters no
/// `river_stones`.
pub fn fall_rocks(
    def: &TileDef,
    seed: u64,
    ground: &Prepared,
    seams: &[FallSeam],
    placed: &[Instance],
) -> Vec<Instance> {
    let Some(ri) = def
        .props
        .scatter
        .iter()
        .position(|r| family(r.piece.as_str()) == "river_stones")
    else {
        return vec![];
    };
    let piece = def.props.scatter[ri].piece.as_str();
    let (low, high) = bank_band(piece).expect("a bank piece");
    let floors = floor_slots(piece);
    let jitter = scale_jitter(group_footprint(piece));
    let mut rng = Rng(seed ^ 0xFA11_0C45_7D3B_92E1);
    let mut standing: Vec<(Vec2, f32)> = placed
        .iter()
        .map(|i| {
            let rule_piece = def.props.scatter[i.rule as usize].piece.as_str();
            (i.pos.xz(), footprint(i.piece(rule_piece)).ground * i.scale)
        })
        .collect();
    let mut out = vec![];
    for seam in seams.iter().filter(|s| s.lower) {
        let want = rng.below(FALL_ROCKS_MAX + 1);
        let mut n = 0;
        for _ in 0..40 * want {
            if n == want {
                break;
            }
            let k = out.len() as u32;
            let variant = pick_variant(def, piece, seed, ri, 1000 + k);
            let reach = footprint(variant_piece(piece, variant)).ground;
            let turn = (rng.unit() - 0.5) * std::f32::consts::PI;
            let r = FALL_ROCK_RING.0 + rng.unit() * (FALL_ROCK_RING.1 - FALL_ROCK_RING.0);
            let p = seam.mid + Vec2::from_angle(turn).rotate(-seam.out) * r;
            let (u, s) = (rng.unit(), rng.unit());
            if !standing.iter().all(|&(q, g)| p.distance(q) >= g + reach) {
                continue;
            }
            let Some(y) = stand(&ground.surface, p, floors, reach, false) else {
                continue;
            };
            if y < low || y > high {
                continue;
            }
            let yaw =
                contour(&ground.surface, p).map_or(u * std::f32::consts::TAU, |t| along_yaw(t, u));
            let scale = 1.0 + jitter * (2.0 * s - 1.0);
            standing.push((p, reach * scale));
            out.push(Instance {
                rule: ri as u8,
                pos: Vec3::new(p.x, y, p.y),
                yaw,
                scale,
                order: 0,
                variant,
            });
            n += 1;
        }
    }
    out
}
