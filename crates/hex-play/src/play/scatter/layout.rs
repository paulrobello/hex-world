//! The seeded layout of a tile: its prepared ground masks, the dart throw, the noises and the placed
//! instances. A pure function of the definition, the seed and the ground.
use std::f32::consts::TAU;

use bevy::prelude::*;
use hex_core::{
    Axial, Endpoint, HEX_RADIUS, NetworkKind, ScatterRule, TileCatalog, TileDef, cell_seed,
};

use super::super::APOTHEM;
use crate::water::tiles::Coastline;

use super::bands::{
    BANK_REACH, BeachBand, DRIFT_YAW_JITTER, SEABED_BORDER, SEABED_Y, WATER_REST, bank_band,
    beach_keep, coast_band, dressing_cap, floor_ok, floor_slots, keep_dressing, keep_water, lily,
    off_beach_band, off_water, ring_band, seabed, spring_nodes, verge_band, water_prop,
    water_stand,
};
use super::surface::{Surface, Tri, member_tris};
use super::tables::{
    DEFAULT_FOOTPRINT, FENCE_HALF, Footprint, LANDFORMS, fence, fence_lines, footprint,
    group_footprint, pick_variant, variant_piece,
};

/// Area of the hex cap in m² (pointy-top, circumradius [`HEX_RADIUS`]): what density counts per.
pub const CAP_AREA: f32 = 1.5 * 1.732_050_8 * HEX_RADIUS * HEX_RADIUS;

/// Instances of one piece keep this fraction of their mean spacing (sqrt(area / count)) apart.
pub const BLUE_NOISE: f32 = 0.6;

/// Trees of one piece keep at least this fraction of their crown radius apart.
pub const CROWN_SPACING: f32 = 0.7;

/// A crown may hang this far (m) over the hex border, into the neighbour.
pub const CROWN_OVERHANG: f32 = 1.0;

/// A tree keeps this fraction of a hero tree's crown radius, plus its own trunk, from the hero.
pub const HERO_CROWN: f32 = 0.85;

/// Gap (m) between footprints of a hero and an instance.
pub const HERO_GAP: f32 = 0.25;

/// Gap (m) between a lane's edge and an instance's footprint.
pub const LANE_GAP: f32 = 1.0;

/// Gap (m) between a footprint and the hex border.
pub const BORDER_GAP: f32 = 0.3;

/// Share of a drift rule's ground (its highest-noise cells) its instances grow on.
pub const DRIFT_SHARE: f32 = 0.25;

/// Same-piece instances in a drift keep this many ground reaches apart (2 when spread evenly), so
/// their clumps may overlap a little.
pub const DRIFT_PACK: f32 = 1.4;

/// Lattice spacing (m) of the drift noise's coarse octave; the fine octave is half of it.
pub const DRIFT_CELL: f32 = 7.0;

/// Dart throws per wanted instance before a rule gives up.
const ATTEMPTS: u32 = 60;

/// Raster step (m) of the ground the dart thrower samples, and its half extent (m).
pub(super) const RASTER: f32 = 0.25;

pub(super) const EXTENT: f32 = 16.5;

pub(super) const CELLS: usize = 132;

/// An instance's yaw from its yaw draw `u` in [0, 1): free (`u` × TAU), or for a piece laid along
/// the coast the coast's direction at `p`, either way round (u < 0.5 or not) and turned by up to
/// [`DRIFT_YAW_JITTER`] (from the same draw, so the dart stream is unchanged).
fn instance_yaw(band: Option<BeachBand>, coast: Option<Coastline>, p: Vec2, u: f32) -> f32 {
    let (Some(band), Some(coast)) = (band, coast) else {
        return u * TAU;
    };
    if !band.along {
        return u * TAU;
    }
    let t = match coast {
        Coastline::Straight { sea } => sea.perp(),
        Coastline::Arc { centre, .. } => (p - centre).perp(),
    };
    along_yaw(t, u)
}

/// A yaw from draw `u` in [0, 1) laying the piece's +x along `t`, either way round, within
/// [`DRIFT_YAW_JITTER`].
fn along_yaw(t: Vec2, u: f32) -> f32 {
    let flip = if u < 0.5 { 0.0 } else { std::f32::consts::PI };
    let jitter = ((u * 2.0).fract() * 2.0 - 1.0) * DRIFT_YAW_JITTER;
    // Quat::from_rotation_y(yaw) turns the piece's +x to (cos yaw, -sin yaw) in (x, z).
    portable_atan2(-t.y, t.x) + flip + jitter
}

/// The bank's contour direction at `p`: across the surface's slope (central differences 0.5 m
/// apart), or `None` where the ground is level or missing.
pub(super) fn contour(surface: &Surface, p: Vec2) -> Option<Vec2> {
    let h = |dx: f32, dz: f32| surface.at(p.x + dx, p.y + dz).map(|(y, _)| y);
    let g = Vec2::new(h(0.5, 0.0)? - h(-0.5, 0.0)?, h(0.0, 0.5)? - h(0.0, -0.5)?);
    (g.length_squared() > 1e-6).then(|| g.perp())
}

/// `y.atan2(x)` from `+ - * /` alone (a degree-11 odd polynomial on [0, 1], max error
/// about 2.4e-5 rad), so native and wasm lay a beach out bit for bit: libm's `atan2` may differ by an
/// ulp between targets, which changes the layout fingerprint.
pub(super) fn portable_atan2(y: f32, x: f32) -> f32 {
    use std::f32::consts::{FRAC_PI_2, PI};
    let (ax, ay) = (x.abs(), y.abs());
    if ax == 0.0 && ay == 0.0 {
        return 0.0;
    }
    let a = ax.min(ay) / ax.max(ay);
    let s = a * a;
    let mut r = ((((-0.011_721_2 * s + 0.052_653_3) * s - 0.116_432_2) * s + 0.193_543_5) * s
        - 0.332_623_9)
        * s
        * a
        + a;
    if ay > ax {
        r = FRAC_PI_2 - r;
    }
    if x < 0.0 {
        r = PI - r;
    }
    if y < 0.0 { -r } else { r }
}

/// Scale jitter (± fraction): trees vary less than rocks and flowers.
fn scale_jitter(f: Footprint) -> f32 {
    if f.crown > 0.0 { 0.12 } else { 0.15 }
}

/// Where one rule's piece may stand: raster cells (row-major from (-EXTENT, -EXTENT)) whose
/// centre clears every rule. `area` is their total in m².
#[derive(Clone, Debug, Default)]
pub struct Mask {
    pub(super) cells: Vec<u32>,
    pub area: f32,
}

/// A tile definition's ground, ready to scatter on: its surface and one mask per scatter rule. A
/// footprint tile's are its centre cell's, and `ring` holds each ring member (in the tile frame),
/// its offset from the centre and its ground in its own frame, since a [`Surface`] spans one hex.
/// A ring cell's `share` scales each rule's planned count by its mask area over the same rule's on
/// a [`RING_GROUND`] cell, so it places as many per m² of free ground (`meadow_share`), and so
/// does a single-tile landform's ([`LANDFORMS`]); every other single tile's is empty.
pub struct Prepared {
    pub surface: Surface,
    pub masks: Vec<Mask>,
    pub ring: Vec<(Axial, Vec2, Prepared)>,
    pub share: Vec<f32>,
}

/// The tile a footprint tile's ring cells scatter like: their density per m² of free ground is
/// its (owner, visual standard: indistinguishable from the Meadow beside it).
pub const RING_GROUND: &str = "meadow";

pub(super) fn cell_centre(i: usize) -> f32 {
    -EXTENT + (i as f32 + 0.5) * RASTER
}

/// The unit direction of local edge `k`, `k` × 60° from +x toward +z. Built from `sqrt` alone,
/// which IEEE rounds exactly on every target, where `sin`/`cos` may differ by an ulp between the
/// native libm and wasm's: one flipped raster cell would reshuffle a whole layout.
pub fn hex_dir(k: u8) -> Vec2 {
    let h = 3f32.sqrt() / 2.0;
    [
        (1.0, 0.0),
        (0.5, h),
        (-0.5, h),
        (-1.0, 0.0),
        (-0.5, -h),
        (0.5, -h),
    ]
    .map(|(x, z)| Vec2::new(x, z))[usize::from(k % 6)]
}

/// The 16 unit directions `k` × 22.5° from +x toward +z, from `sqrt` alone ([`hex_dir`]).
pub(super) fn ring16() -> [Vec2; 16] {
    let r = std::f32::consts::FRAC_1_SQRT_2;
    let c = (2.0 + 2f32.sqrt()).sqrt() / 2.0;
    let s = (2.0 - 2f32.sqrt()).sqrt() / 2.0;
    let q = [
        Vec2::new(1.0, 0.0),
        Vec2::new(c, s),
        Vec2::new(r, r),
        Vec2::new(s, c),
    ];
    std::array::from_fn(|i| {
        let v = q[i % 4];
        match i / 4 {
            0 => v,
            1 => Vec2::new(-v.y, v.x),
            2 => -v,
            _ => Vec2::new(v.y, -v.x),
        }
    })
}

/// `p` turned `k` × 60° about +Y the way `piece_rotation(k)` turns it (+x toward +z), exactly.
pub fn turn(k: u8, p: [f32; 3]) -> [f32; 3] {
    let d = hex_dir(k);
    [p[0] * d.x - p[2] * d.y, p[1], p[0] * d.y + p[2] * d.x]
}

/// How far `p` reaches toward the nearest hex edge: the apothem on the border.
pub fn hex_reach(p: Vec2) -> f32 {
    (0..3).map(|e| p.dot(hex_dir(e)).abs()).fold(0.0, f32::max)
}

/// Road and river centrelines of a tile as polylines in tile-local metres, with half widths.
pub fn lanes(def: &TileDef) -> Vec<(Vec<Vec2>, f32)> {
    let point = |end: &Endpoint| -> Option<(Vec2, Option<f32>)> {
        match end {
            Endpoint::Edge(k) => {
                let lane = def.edges.get(*k as usize)?.lane?;
                let dir = hex_dir(*k);
                let tangent = Vec2::new(-dir.y, dir.x);
                Some((
                    dir * APOTHEM + tangent * lane.offset,
                    Some(lane.width / 2.0),
                ))
            }
            Endpoint::Node(id) => def
                .nodes
                .iter()
                .find(|n| n.id == *id)
                .map(|n| (Vec2::from(n.at), None)),
        }
    };
    def.networks
        .iter()
        .filter_map(|s| {
            let (a, wa) = point(&s.from)?;
            let (b, wb) = point(&s.to)?;
            let mut line = vec![a];
            line.extend(s.points.iter().map(|p| Vec2::from(*p)));
            line.push(b);
            let half = [wa, wb]
                .into_iter()
                .flatten()
                .reduce(f32::max)
                .unwrap_or(3.0);
            Some((line, half))
        })
        .collect()
}

pub(crate) fn polyline_distance(p: Vec2, line: &[Vec2]) -> f32 {
    line.windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
            p.distance(a + ab * t)
        })
        .fold(f32::MAX, f32::min)
}

/// The footprint hero slot `slot` of `def` keeps clear: the widest of every piece it may draw
/// (`TileDef::hero_options`). A tile's ground and masks are prepared once per tile id, not per
/// cell, so they clear every variant a cell may pick.
pub fn hero_footprint(def: &TileDef, slot: usize) -> Footprint {
    def.hero_options(slot)
        .map(footprint)
        .reduce(|a, b| Footprint {
            ground: a.ground.max(b.ground),
            crown: a.crown.max(b.crown),
        })
        .unwrap_or(DEFAULT_FOOTPRINT)
}

/// Least distance between a hero prop's centre and an instance's.
pub fn hero_clearance(hero: Footprint, inst: Footprint) -> f32 {
    let ground = hero.ground + inst.ground + HERO_GAP;
    if hero.crown > 0.0 && inst.crown > 0.0 {
        ground.max(HERO_CROWN * hero.crown + inst.ground)
    } else {
        ground
    }
}

/// Least distance between a fence line and the centre of an instance of ground reach `reach`.
pub fn fence_clearance(reach: f32) -> f32 {
    FENCE_HALF + reach
}

/// Least distance between a lane's centreline and an instance's centre.
pub fn lane_clearance(rule: &ScatterRule, half_width: f32, inst: Footprint) -> f32 {
    rule.lane_clearance.max(half_width) + LANE_GAP + inst.ground.max(inst.crown)
}

/// Largest [`hex_reach`] of an instance's centre.
pub fn border_reach(inst: Footprint) -> f32 {
    APOTHEM - (inst.ground + BORDER_GAP).max(inst.crown - CROWN_OVERHANG)
}

fn mask(
    def: &TileDef,
    surface: &Surface,
    raster: &[Option<(f32, u16)>],
    rule: &ScatterRule,
    at: Vec2,
) -> Mask {
    let piece = rule.piece.as_str();
    let ring = ring_band(def, piece);
    let fp = group_footprint(piece);
    let floors = floor_slots(piece);
    let ok_slot: Vec<bool> = surface.slots.iter().map(|s| floor_ok(floors, s)).collect();
    let beach = coast_band(def, piece).map(|b| (b, Coastline::of(def, 0)));
    let on_floor = |k: usize| raster[k].is_some_and(|(_, s)| ok_slot[s as usize]);
    // The footprint, the half diagonal a dart may land from its cell centre, and the half diagonal
    // from a footprint point to the nearest sample. `stand` then checks the footprint exactly.
    let r = fp.ground + RASTER * std::f32::consts::SQRT_2;
    let n = (r / RASTER).ceil() as i32;
    let disc: Vec<(i32, i32)> = (-n..=n)
        .flat_map(|j| (-n..=n).map(move |i| (i, j)))
        .filter(|&(i, j)| ((i * i + j * j) as f32).sqrt() * RASTER <= r)
        .collect();
    let border = border_reach(fp);
    // A fence hero is kept clear by its line after the darts (`layout`), not here: a mask change
    // would reshuffle every dart of the tile, the ones far from the fence too.
    let heroes: Vec<(Vec2, f32)> = def
        .props
        .hero
        .iter()
        .enumerate()
        .filter(|(_, h)| fence(h.piece.as_str()).is_none())
        .map(|(i, h)| {
            let c = hero_clearance(hero_footprint(def, i), fp);
            (Vec2::from(h.at), c)
        })
        .collect();
    let bank = bank_band(piece);
    let (rivers, lanes): (Vec<_>, Vec<_>) = def
        .networks
        .iter()
        .zip(lanes(def))
        .map(|(n, (line, half))| {
            let river = n.kind == NetworkKind::River;
            (river, (line, lane_clearance(rule, half, fp)))
        })
        .partition(|(river, _)| (bank.is_some() || water_prop(piece)) && *river);
    // A water prop: its own band, border and spring reach, at its largest scale from anywhere in
    // its cell, and a lily cluster's whole footprint over open water.
    let springs = spring_nodes(def);
    let water_reach = fp.ground * (1.0 + scale_jitter(fp));
    let off_water_cell = |k: usize, p: Vec2| {
        water_prop(piece)
            && raster[k].is_none_or(|(h, _)| {
                off_water(
                    lily(piece),
                    &springs,
                    p,
                    water_reach,
                    h,
                    RASTER * std::f32::consts::FRAC_1_SQRT_2,
                )
            })
    };
    let wet = |k: usize| !lily(piece) || raster[k].is_some_and(|(h, _)| h < WATER_REST);
    let rivers: Vec<Vec<Vec2>> = rivers.into_iter().map(|(_, (l, _))| l).collect();
    let lanes: Vec<(Vec<Vec2>, f32)> = lanes.into_iter().map(|(_, l)| l).collect();
    let off_bank = |k: usize, p: Vec2| {
        bank.is_some_and(|(low, high)| {
            let near = rivers.iter().any(|l| polyline_distance(p, l) <= BANK_REACH);
            !near || raster[k].is_none_or(|(h, _)| h < low || h > high)
        })
    };
    let band = verge_band(piece);
    // At its largest scale, from anywhere in its cell (a dart lands up to half a cell diagonal off).
    let seabed_border = seabed(piece).then_some(
        APOTHEM
            - SEABED_BORDER
            - fp.ground * (1.0 + scale_jitter(fp))
            - RASTER * std::f32::consts::FRAC_1_SQRT_2,
    );
    // A dart lands up to half a cell diagonal off its cell's centre.
    let slop = RASTER * std::f32::consts::FRAC_1_SQRT_2;
    let mut cells = vec![];
    for j in 0..CELLS {
        for i in 0..CELLS {
            let k = j * CELLS + i;
            if !on_floor(k) {
                continue;
            }
            let p = Vec2::new(cell_centre(i), cell_centre(j));
            if hex_reach(p) > border
                || heroes.iter().any(|(h, c)| p.distance(*h) < *c)
                || lanes.iter().any(|(l, c)| polyline_distance(p, l) < *c)
                || band.is_some_and(|b| lanes.iter().all(|(l, _)| polyline_distance(p, l) > b))
                || beach.is_some_and(|(b, c)| off_beach_band(b, c, p, fp.ground))
                || off_bank(k, p)
                || off_water_cell(k, p)
                || seabed_border.is_some_and(|r| hex_reach(p) > r)
                || ring
                    .is_some_and(|(lo, hi)| !(lo + slop..=hi - slop).contains(&(p + at).length()))
            {
                continue;
            }
            let covered = disc.iter().all(|&(di, dj)| {
                let (ii, jj) = (i as i32 + di, j as i32 + dj);
                (0..CELLS as i32).contains(&ii)
                    && (0..CELLS as i32).contains(&jj)
                    && on_floor(jj as usize * CELLS + ii as usize)
                    && wet(jj as usize * CELLS + ii as usize)
            });
            if covered {
                cells.push(k as u32);
            }
        }
    }
    let area = cells.len() as f32 * RASTER * RASTER;
    Mask { cells, area }
}

/// Each of `def`'s rules' share of its planned count on ground whose masks are `masks`: the mask's
/// area over the same rule's mask on a [`RING_GROUND`] cell (`reference`), so a rule places as many
/// instances per m² of its free ground as on a Meadow; 1 for a banded rule (the pines), which has no
/// Meadow counterpart, and for a rule with no Meadow ground.
fn meadow_share(def: &TileDef, masks: &[Mask], reference: &[Mask]) -> Vec<f32> {
    def.props
        .scatter
        .iter()
        .zip(masks.iter().zip(reference))
        .map(|(r, (m, base))| {
            if ring_band(def, r.piece.as_str()).is_some() || base.area <= 0.0 {
                1.0
            } else {
                m.area / base.area
            }
        })
        .collect()
}

/// Builds a definition's ground from its ground pieces' LOD0 triangles (`tris_of`, tile-local
/// before the piece's turn), or `None` while one is missing. A sloped definition stands on its
/// runtime ground ([`super::super::slope_ground::tile_ground_tris`]).
pub fn prepare(
    def: &TileDef,
    catalog: &TileCatalog,
    tris_of: &mut dyn FnMut(&str) -> Option<Vec<(Tri, String)>>,
) -> Option<Prepared> {
    let all = super::super::slope_ground::tile_ground_tris(def, catalog, tris_of)?;
    if !def.is_mega() {
        let mut out = prepare_cell(def, all, Vec2::ZERO);
        if LANDFORMS.contains(&def.id.as_str()) {
            let meadow = catalog.get(RING_GROUND)?;
            let meadow_tris =
                super::super::slope_ground::tile_ground_tris(meadow, catalog, tris_of)?;
            let reference = prepare_cell(def, meadow_tris, Vec2::ZERO);
            out.share = meadow_share(def, &out.masks, &reference.masks);
        }
        return Some(out);
    }
    let cells = hex_core::footprint::members(Axial::ORIGIN).map(|m| {
        let at = Vec2::from(m.to_world(HEX_RADIUS));
        (m, at, prepare_cell(def, member_tris(&all, at), at))
    });
    let centre = cells.iter().position(|(m, ..)| *m == Axial::ORIGIN)?;
    let mut ring: Vec<(Axial, Vec2, Prepared)> = cells.into_iter().collect();
    let (_, _, mut out) = ring.remove(centre);
    let meadow = catalog.get(RING_GROUND)?;
    let meadow_tris = super::super::slope_ground::tile_ground_tris(meadow, catalog, tris_of)?;
    let reference = prepare_cell(def, meadow_tris, Vec2::ZERO);
    for (_, _, cell) in &mut ring {
        cell.share = meadow_share(def, &cell.masks, &reference.masks);
    }
    out.ring = ring;
    Some(out)
}

/// One cell's ground from its triangles in its own frame, `at` from its tile's centre.
fn prepare_cell(def: &TileDef, tris: Vec<(Tri, String)>, at: Vec2) -> Prepared {
    let surface = Surface::new(tris);
    let raster: Vec<Option<(f32, u16)>> = (0..CELLS * CELLS)
        .map(|k| surface.top(cell_centre(k % CELLS), cell_centre(k / CELLS)))
        .collect();
    let masks = def
        .props
        .scatter
        .iter()
        .map(|r| mask(def, &surface, &raster, r, at))
        .collect();
    Prepared {
        surface,
        masks,
        ring: vec![],
        share: vec![],
    }
}

/// One placed prop, tile-local.
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    /// Index of its rule in `props.scatter`.
    pub rule: u8,
    /// Centre at the surface height under it.
    pub pos: Vec3,
    /// Turn about +Y (radians).
    pub yaw: f32,
    pub scale: f32,
    /// Rank by distance from the tile centre: the rise grows instances in this order.
    pub order: u8,
    /// Which piece of its rule's group it draws ([`variant_piece`]; 0 is the rule's own piece).
    pub variant: u8,
}

impl Instance {
    /// The piece it draws, for a rule naming `rule_piece`.
    pub fn piece<'a>(&self, rule_piece: &'a str) -> &'a str {
        variant_piece(rule_piece, self.variant)
    }

    pub fn rotation(&self) -> Quat {
        Quat::from_rotation_y(self.yaw)
    }

    pub fn transform(&self) -> Transform {
        Transform::from_translation(self.pos)
            .with_rotation(self.rotation())
            .with_scale(Vec3::splat(self.scale))
    }
}

/// A tile's scatter. `planned`, `area` (the rule's whole ground) and `spacing` (the least distance
/// between two of its instances) are per rule.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub instances: Vec<Instance>,
    pub planned: Vec<u32>,
    pub area: Vec<f32>,
    pub spacing: Vec<f32>,
}

impl Layout {
    pub fn placed(&self, rule: usize) -> usize {
        self.instances
            .iter()
            .filter(|i| i.rule as usize == rule)
            .count()
    }

    /// FNV-1a over every instance's rule, position, yaw and scale bits: equal layouts, equal value.
    pub fn fingerprint(&self) -> u64 {
        let mut h: u64 = 0xCBF2_9CE4_8422_2325;
        for i in &self.instances {
            let words = [
                u32::from(i.rule) | (u32::from(i.variant) << 8),
                i.pos.x.to_bits(),
                i.pos.y.to_bits(),
                i.pos.z.to_bits(),
                i.yaw.to_bits(),
                i.scale.to_bits(),
            ];
            for b in words.iter().flat_map(|w| w.to_le_bytes()) {
                h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01B3);
            }
        }
        h
    }
}

/// SplitMix64.
pub(crate) struct Rng(pub(crate) u64);

impl Rng {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub(crate) fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// The `unit` the `k`th next draw (from 0) would return, without drawing: SplitMix64's state
    /// steps by a constant, so a draw can be made out of order.
    pub(crate) fn unit_at(&self, k: u64) -> f32 {
        Rng(self.0.wrapping_add(k.wrapping_mul(0x9E37_79B9_7F4A_7C15))).unit()
    }

    /// Steps past the next `n` draws, as `n` calls to `next` would.
    pub(crate) fn skip(&mut self, n: u64) {
        self.0 = self.0.wrapping_add(n.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    }

    pub(super) fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// A lattice value in [0, 1) of the drift noise: SplitMix64 of the seed and the lattice point.
fn lattice(seed: u64, i: i32, j: i32) -> f32 {
    let key = (u64::from(i as u32) << 32) | u64::from(j as u32);
    Rng(seed ^ key.wrapping_mul(0xD6E8_FEB8_6659_FD93)).unit()
}

/// Value noise at `p` (tile-local metres) on a lattice `cell` metres apart, smoothstep-blended.
/// Arithmetic and `floor` alone, so native and wasm agree bit for bit.
fn value_noise(seed: u64, p: Vec2, cell: f32) -> f32 {
    let g = p / cell;
    let (fx, fz) = (g.x.floor(), g.y.floor());
    let (i, j) = (fx as i32, fz as i32);
    let ease = |t: f32| t * t * (3.0 - 2.0 * t);
    let (u, v) = (ease(g.x - fx), ease(g.y - fz));
    let a = lattice(seed, i, j) + u * (lattice(seed, i + 1, j) - lattice(seed, i, j));
    let b = lattice(seed, i, j + 1) + u * (lattice(seed, i + 1, j + 1) - lattice(seed, i, j + 1));
    a + v * (b - a)
}

/// The drift noise of a tile with seed `seed` at `p`: two octaves of value noise.
pub fn drift_noise(seed: u64, p: Vec2) -> f32 {
    let s = seed ^ 0xD71F_7A55_0C3E_91B5;
    (value_noise(s, p, DRIFT_CELL) + 0.35 * value_noise(s.rotate_left(17), p, 0.5 * DRIFT_CELL))
        / 1.35
}

/// The noise a `vary` rule thins its even spread by (`ScatterRule::vary`), in [0, 1]: the drift
/// noise on a seed of its own (single flowers do not just follow the clumps' drifts), stretched
/// by a smoothstep over its middle, where the two smoothed octaves spend most of their values.
pub fn vary_noise(seed: u64, p: Vec2) -> f32 {
    let n = drift_noise(seed.rotate_left(29) ^ 0x51F1_E5A3_0C77_9B2D, p);
    let t = ((n - 0.3) / 0.4).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The cells of `mask` a drift rule grows on: the [`DRIFT_SHARE`] with the highest drift noise.
fn drift_cells(seed: u64, mask: &Mask) -> Vec<u32> {
    let mut scored: Vec<(f32, u32)> = mask
        .cells
        .iter()
        .map(|&k| {
            let k_ = k as usize;
            let p = Vec2::new(cell_centre(k_ % CELLS), cell_centre(k_ / CELLS));
            (drift_noise(seed, p), k)
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let keep = ((scored.len() as f32 * DRIFT_SHARE).ceil() as usize).min(scored.len());
    let mut cells: Vec<u32> = scored[..keep].iter().map(|&(_, k)| k).collect();
    cells.sort_unstable();
    cells
}

/// Lowest surface height under the centre and a 0.3 m ring around `p` (the centre's own height
/// when `level` is false: a bank prop, laid along the bank's contour), if `p` stands on `floors`.
fn stand(surface: &Surface, p: Vec2, floors: &[&str], reach: f32, level: bool) -> Option<f32> {
    let on_floor = |q: Vec2| surface.at(q.x, q.y).filter(|(_, s)| floor_ok(floors, s));
    let (h, _) = on_floor(p)?;
    // The footprint's rim and half-rim, exactly (the mask only eroded the sampled ground).
    let ring16 = ring16();
    for d in ring16 {
        on_floor(p + reach * d)?;
    }
    for d in ring16.iter().step_by(2) {
        on_floor(p + 0.5 * reach * *d)?;
    }
    if !level {
        return Some(h);
    }
    let ring = (0..6).filter_map(|k| {
        let q = p + 0.3 * hex_dir(k);
        surface.at(q.x, q.y).map(|(y, _)| y)
    });
    Some(ring.fold(h, f32::min))
}

/// A reed bed's yaw from draw `u`: its -z (Blender +y) up the slope at `p` (central differences
/// 0.5 m apart), within [`DRIFT_YAW_JITTER`]; `None` where the ground is level or missing.
fn uphill_yaw(surface: &Surface, p: Vec2, u: f32) -> Option<f32> {
    let h = |dx: f32, dz: f32| surface.at(p.x + dx, p.y + dz).map(|(y, _)| y);
    let n = Vec2::new(h(0.5, 0.0)? - h(-0.5, 0.0)?, h(0.0, 0.5)? - h(0.0, -0.5)?);
    // Quat::from_rotation_y(yaw) turns the piece's -z to (-sin yaw, -cos yaw) in (x, z).
    (n.length_squared() > 1e-6)
        .then(|| portable_atan2(-n.x, -n.y) + (2.0 * u - 1.0) * DRIFT_YAW_JITTER)
}

/// Every instance the darts place for a tile before a capped tile keeps its [`dressing_cap`], in
/// dart order and not yet ranked.
pub(super) fn every_dart(def: &TileDef, seed: u64, ground: &Prepared) -> Layout {
    let mut rng = Rng(seed ^ 0x5CA7_7E25_D3A1_0B07);
    let mut out = Layout::default();
    // (centre, ground reach, rule) of everything placed so far.
    let mut placed: Vec<(Vec2, f32, usize)> = vec![];
    let coast = Coastline::of(def, 0);
    let springs = spring_nodes(def);
    for (ri, rule) in def.props.scatter.iter().enumerate() {
        let piece = rule.piece.as_str();
        let fp = group_footprint(piece);
        let mask = &ground.masks[ri];
        let mean = rule.density.max(0.0) * CAP_AREA / 100.0;
        let mean = match ground.share.get(ri) {
            Some(share) => mean * share,
            None => mean,
        };
        let want = (mean + rng.unit()).floor() as u32;
        out.planned.push(want);
        out.area.push(mask.area);
        let drift;
        let cells = if rule.drift {
            drift = drift_cells(seed, mask);
            &drift
        } else {
            &mask.cells
        };
        let grows_on = cells.len() as f32 * RASTER * RASTER;
        let pack = if rule.drift { DRIFT_PACK } else { 2.0 };
        let spacing = (BLUE_NOISE * (grows_on / want.max(1) as f32).sqrt())
            .max(pack * fp.ground)
            .max(CROWN_SPACING * fp.crown);
        out.spacing.push(spacing);
        if want == 0 || cells.is_empty() {
            continue;
        }
        let jitter = scale_jitter(fp);
        let floors = floor_slots(piece);
        let vary = rule.vary.clamp(0.0, 1.0);
        let beach = coast_band(def, piece);
        let bank = bank_band(piece);
        let water = water_prop(piece);
        let mut n = 0;
        for _ in 0..ATTEMPTS * want + 64 {
            if n == want {
                break;
            }
            let variant = pick_variant(def, piece, seed, ri, n);
            let reach = footprint(variant_piece(piece, variant)).ground;
            let k = cells[rng.below(cells.len())] as usize;
            let p = Vec2::new(
                cell_centre(k % CELLS) + (rng.unit() - 0.5) * RASTER,
                cell_centre(k / CELLS) + (rng.unit() - 0.5) * RASTER,
            );
            if vary > 0.0 && rng.unit() > 1.0 - vary * (1.0 - vary_noise(seed, p)) {
                continue;
            }
            if let (Some(b), Some(c)) = (beach, coast)
                && b.thin > 0.0
                && rng.unit() > beach_keep(b, c, p)
            {
                continue;
            }
            let clear = placed.iter().all(|&(q, g, r)| {
                let d = p.distance(q);
                if r == ri {
                    d >= spacing
                } else {
                    d >= g + reach
                }
            });
            if !clear {
                continue;
            }
            let level = bank.is_none() && !water;
            let Some(y) = stand(&ground.surface, p, floors, reach, level) else {
                continue;
            };
            // The mask checks the band at its cell centres; the dart, off-centre, exactly.
            if bank.is_some_and(|(low, high)| y < low || y > high) {
                continue;
            }
            let y = match water.then(|| {
                let r = reach * (1.0 + jitter);
                water_stand(&ground.surface, piece, &springs, p, r, y)
            }) {
                Some(None) => continue,
                Some(Some(w)) => w,
                None => y,
            };
            let y = if seabed(piece) { SEABED_Y } else { y };
            let u = rng.unit();
            let yaw = match bank.and_then(|_| contour(&ground.surface, p)) {
                Some(t) => along_yaw(t, u),
                None if water && !lily(piece) => {
                    uphill_yaw(&ground.surface, p, u).unwrap_or(u * TAU)
                }
                None => instance_yaw(beach, coast, p, u),
            };
            let scale = 1.0 + jitter * (2.0 * rng.unit() - 1.0);
            placed.push((p, reach, ri));
            out.instances.push(Instance {
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
    // Hero fences: nothing stands on a fence line. Dropped after the darts, so every instance
    // clear of the line keeps the place it has without the fence; a dropped one is not replaced,
    // so a fenced tile may place fewer than planned.
    let fences = fence_lines(def);
    if !fences.is_empty() {
        out.instances.retain(|i| {
            let piece = i.piece(def.props.scatter[i.rule as usize].piece.as_str());
            let need = fence_clearance(footprint(piece).ground * i.scale);
            fences
                .iter()
                .all(|l| polyline_distance(i.pos.xz(), l) >= need)
        });
    }
    out
}

/// The scatter of a tile with definition `def` and seed `seed` on its prepared ground. A footprint
/// tile scatters on its ring cells only, each as a single tile of its rules on its own ground and
/// seed ([`ring_seed`]); its centre is the rock.
pub fn layout(def: &TileDef, seed: u64, ground: &Prepared) -> Layout {
    let mut out = if def.is_mega() {
        let mut out = Layout::default();
        for (m, at, g) in &ground.ring {
            let l = cell_layout(def, ring_seed(seed, *m), g);
            if out.planned.is_empty() {
                out.planned = vec![0; l.planned.len()];
                out.area = vec![0.0; l.area.len()];
                out.spacing = vec![0.0; l.spacing.len()];
            }
            for ri in 0..l.planned.len() {
                out.planned[ri] += l.planned[ri];
                out.area[ri] += l.area[ri];
                out.spacing[ri] = out.spacing[ri].max(l.spacing[ri]);
            }
            out.instances.extend(l.instances.into_iter().map(|mut i| {
                i.pos += Vec3::new(at.x, 0.0, at.y);
                i
            }));
        }
        out
    } else {
        cell_layout(def, seed, ground)
    };
    let mut by_distance: Vec<usize> = (0..out.instances.len()).collect();
    by_distance.sort_by(|&a, &b| {
        let d = |i: usize| out.instances[i].pos.xz().length();
        d(a).total_cmp(&d(b)).then(a.cmp(&b))
    });
    for (rank, i) in by_distance.into_iter().enumerate() {
        out.instances[i].order = rank.min(u8::MAX as usize) as u8;
    }
    out
}

/// The seed a footprint tile with seed `seed` scatters and grows grass on its member `m` (tile
/// frame) with; the centre keeps the tile's.
pub fn ring_seed(seed: u64, m: Axial) -> u64 {
    if m == Axial::ORIGIN {
        seed
    } else {
        cell_seed(seed, m)
    }
}

/// One cell's scatter: its darts, then a capped tile's [`dressing_cap`] and its water props' keep.
pub(super) fn cell_layout(def: &TileDef, seed: u64, ground: &Prepared) -> Layout {
    let mut out = every_dart(def, seed, ground);
    let water: Vec<bool> = def
        .props
        .scatter
        .iter()
        .map(|r| water_prop(r.piece.as_str()))
        .collect();
    if let Some(cap) = dressing_cap(def, seed) {
        keep_dressing(&mut out, seed, cap, &water);
    }
    if water.contains(&true) {
        keep_water(&mut out, seed, &water);
    }
    out
}
