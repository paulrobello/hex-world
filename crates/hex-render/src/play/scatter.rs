//! Seeded prop scatter in play (PRD §5.3, §6): the forest's spruces and rocks and the meadow's
//! wildflowers and rocks, from each tile's `props.scatter` rules.
//!
//! - **Seed.** A tile scatters from `hex_core::cell_seed(world seed, cell)`, which is exactly its
//!   `PlacedTile::seed`. [`layout`] is a pure function of the definition, that seed and the tile's
//!   ground, so a world always looks the same, and undo, redo, a reload or a late pack redraw a tile
//!   as it was.
//! - **Density.** `ScatterRule::density` is the mean count per 100 m² of the hex cap ([`CAP_AREA`],
//!   665 m²), so the forest's 3.0 is 20 trees a tile. The count is rounded stochastically from the
//!   seed, which keeps its mean over cells exact. Exclusions move instances onto the ground that is
//!   left and do not thin the count; where blue-noise spacing cannot fit it, the report's `placed`
//!   falls short of `planned`.
//! - **Ground.** An instance stands only on its piece's floor slots ([`floor_slots`]) of the topmost
//!   LOD0 surface of the tile's base, centre and lane edge pieces, sampled the way the kit gate's
//!   `surface_at` does: trees on `forest_base_floor` (the litter), wildflowers on `meadow_base_cap`,
//!   rocks on either. Its whole ground footprint must lie on that floor, so nothing stands on a road,
//!   a river bed or bank, a house pad or driveway, a beach, a rim, a skirt or water
//!   ([`excluded_slot`]). It keeps clear of hero props (a tree clears the oak's canopy), of road and
//!   river centrelines (`lane_clearance`, at least the lane's half width plus [`LANE_GAP`]) and of
//!   the hex border (a crown overhangs it by at most [`CROWN_OVERHANG`]), and stands upright at the
//!   surface height.
//! - **Spacing.** Dart throwing over the valid ground (Poisson-disc, never a grid): instances of one
//!   piece keep [`BLUE_NOISE`] of their mean spacing apart (trees at least [`CROWN_SPACING`] of a
//!   crown), and different pieces never share a footprint. Each gets a random yaw and a small scale
//!   jitter, both carried by its transform.
//! - **Drawing.** Instances are children of the tile root, which carries the tile's rotation, built
//!   from the kit's own parts (`spawn_kit_parts`): shared meshes and kit materials, so Bevy draws each
//!   (piece, LOD, slot) as one instanced batch per pass within the kit's LOD ranges, and no sampled
//!   texture is added. They grow in the rise's props stage in a ripple from the centre outward, and
//!   sink with the tile.
//! - **Streaming.** A tile scatters once its ground pieces are in the kit (the forest pack brings
//!   `forest_base`), and each rule draws once its piece is (the props pack brings rocks and
//!   wildflowers, the forest pack spruces). Until then the tile shows its placeholder or bare cap.
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    f32::consts::TAU,
    sync::Arc,
};

use bevy::{
    gltf::{Gltf, GltfMesh},
    prelude::*,
};
use hex_core::{
    Axial, Endpoint, HEX_RADIUS, ScatterRule, SocketKind, TileCatalog, TileDef, cell_seed,
};

use super::{
    APOTHEM, KitPart, PlayCatalog, PlayKit, PlaySpawned, PlayView,
    packs::PlayPacks,
    rise::{PropKind, RiseRole},
    spawn_kit_parts,
};
use crate::{ActiveScenario, Scenario, SceneEntity};

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
/// Dart throws per wanted instance before a rule gives up.
const ATTEMPTS: u32 = 60;
/// Raster step (m) of the ground the dart thrower samples, and its half extent (m).
const RASTER: f32 = 0.25;
const EXTENT: f32 = 16.5;
const CELLS: usize = 132;
/// Bucket size (m) of the surface's triangle grid.
const BUCKET: f32 = 1.0;
const BUCKETS: usize = 33;

/// The slots a scatter piece may stand on (the topmost LOD0 surface under its whole footprint).
/// A piece not listed has none, so it is never placed.
pub fn floor_slots(piece: &str) -> &'static [&'static str] {
    match piece {
        "tree_forest" => &["forest_base_floor"],
        "rock_small" => &["forest_base_floor", "meadow_base_cap"],
        "wildflower_clump" => &["meadow_base_cap"],
        _ => &[],
    }
}

/// Slots no scatter may stand on whatever [`floor_slots`] says: roads, river beds and banks,
/// house pads and driveways, beaches and the coast, bridges, and every base's rim, mossy edge band
/// and skirt.
pub fn excluded_slot(slot: &str) -> bool {
    const PREFIXES: [&str; 6] = [
        "road_",
        "river_",
        "house_pad_",
        "beach_",
        "coast_",
        "bridge_",
    ];
    const SUFFIXES: [&str; 3] = ["_rim", "_edge", "_skirt"];
    PREFIXES.iter().any(|p| slot.starts_with(p)) || SUFFIXES.iter().any(|s| slot.ends_with(s))
}

/// Horizontal reach of a piece in metres: `ground` is its collider's (what it stands on and what
/// others must not overlap) and `crown` its LOD0 canopy's (0 for low props).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Footprint {
    pub ground: f32,
    pub crown: f32,
}

/// The pieces scatter places or keeps clear of (`footprints_match_the_kit` checks them against
/// the GLBs).
pub const FOOTPRINTS: [(&str, Footprint); 6] = [
    (
        "tree_forest",
        Footprint {
            ground: 0.51,
            crown: 4.1,
        },
    ),
    (
        "rock_small",
        Footprint {
            ground: 0.62,
            crown: 0.0,
        },
    ),
    (
        "wildflower_clump",
        Footprint {
            ground: 0.9,
            crown: 0.0,
        },
    ),
    (
        "tree_hero_oak",
        Footprint {
            ground: 2.05,
            crown: 7.1,
        },
    ),
    (
        "house_cottage",
        Footprint {
            ground: 4.3,
            crown: 0.0,
        },
    ),
    (
        "house_twostorey",
        Footprint {
            ground: 4.75,
            crown: 0.0,
        },
    ),
];

/// A piece missing from [`FOOTPRINTS`].
const DEFAULT_FOOTPRINT: Footprint = Footprint {
    ground: 1.0,
    crown: 0.0,
};

pub fn footprint(piece: &str) -> Footprint {
    FOOTPRINTS
        .iter()
        .find(|(id, _)| *id == piece)
        .map_or(DEFAULT_FOOTPRINT, |(_, f)| *f)
}

/// Scale jitter (± fraction): trees vary less than rocks and flowers.
fn scale_jitter(f: Footprint) -> f32 {
    if f.crown > 0.0 { 0.12 } else { 0.15 }
}

/// A triangle's corners in the tile frame (runtime axes: Y up).
pub type Tri = [[f32; 3]; 3];

/// A tile's ground: the topmost upward triangles over each (x, z), with their slot names,
/// bucketed on a 1 m grid.
pub struct Surface {
    slots: Vec<String>,
    tris: Vec<(Tri, u16)>,
    buckets: Vec<Vec<u32>>,
}

fn bucket(v: f32) -> usize {
    (((v + EXTENT) / BUCKET).floor().max(0.0) as usize).min(BUCKETS - 1)
}

impl Surface {
    /// Vertical (skirt) and degenerate triangles carry no ground and are dropped.
    pub fn new(parts: impl IntoIterator<Item = (Tri, String)>) -> Self {
        let mut slots: Vec<String> = vec![];
        let mut tris = vec![];
        let mut buckets = vec![vec![]; BUCKETS * BUCKETS];
        for (t, name) in parts {
            let [a, b, c] = t;
            let d = (b[0] - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (b[2] - a[2]);
            if d.abs() < 1e-9 {
                continue;
            }
            let slot = match slots.iter().position(|s| *s == name) {
                Some(i) => i,
                None => {
                    slots.push(name);
                    slots.len() - 1
                }
            } as u16;
            let i = tris.len() as u32;
            tris.push((t, slot));
            let lo = |k: usize| bucket(t.iter().map(|p| p[k]).fold(f32::MAX, f32::min));
            let hi = |k: usize| bucket(t.iter().map(|p| p[k]).fold(f32::MIN, f32::max));
            for bz in lo(2)..=hi(2) {
                for bx in lo(0)..=hi(0) {
                    buckets[bz * BUCKETS + bx].push(i);
                }
            }
        }
        Self {
            slots,
            tris,
            buckets,
        }
    }

    fn top(&self, x: f32, z: f32) -> Option<(f32, u16)> {
        if x.abs() >= EXTENT || z.abs() >= EXTENT {
            return None;
        }
        let mut best: Option<(f32, u16)> = None;
        for &i in &self.buckets[bucket(z) * BUCKETS + bucket(x)] {
            let ([a, b, c], slot) = self.tris[i as usize];
            let d = (b[0] - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (b[2] - a[2]);
            let u = ((x - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (z - a[2])) / d;
            let v = ((b[0] - a[0]) * (z - a[2]) - (x - a[0]) * (b[2] - a[2])) / d;
            if u < -1e-5 || v < -1e-5 || u + v > 1.0 + 1e-5 {
                continue;
            }
            let y = a[1] + u * (b[1] - a[1]) + v * (c[1] - a[1]);
            if best.is_none_or(|(h, _)| y > h) {
                best = Some((y, slot));
            }
        }
        best
    }

    /// Height and slot of the topmost upward triangle over (x, z), if any.
    pub fn at(&self, x: f32, z: f32) -> Option<(f32, &str)> {
        self.top(x, z)
            .map(|(h, s)| (h, self.slots[s as usize].as_str()))
    }
}

/// The ground pieces of a tile and their turn about +Y in the tile frame ([`turn`] steps, as
/// `tile_children` draws them with `piece_rotation`): the base, and with a centre piece the centre
/// and its lanes' edge pieces.
pub fn ground_pieces<'a>(def: &'a TileDef, catalog: &'a TileCatalog) -> Vec<(&'a str, u8)> {
    let mut v = vec![(def.base.as_str(), 0)];
    if let Some(c) = &def.center {
        v.push((c.as_str(), 0));
        let pieces = def.edge_pieces(catalog.sockets());
        for e in 0..6u8 {
            let s = &def.edges[e as usize];
            if s.lane.is_some()
                && matches!(s.kind, SocketKind::Road | SocketKind::River)
                && let Some(p) = pieces[e as usize]
            {
                v.push((p.as_str(), e));
            }
        }
    }
    v
}

/// Where one rule's piece may stand: raster cells (row-major from (-EXTENT, -EXTENT)) whose
/// centre clears every rule. `area` is their total in m².
#[derive(Clone, Debug, Default)]
pub struct Mask {
    cells: Vec<u32>,
    pub area: f32,
}

/// A tile definition's ground, ready to scatter on: its surface and one mask per scatter rule.
pub struct Prepared {
    pub surface: Surface,
    pub masks: Vec<Mask>,
}

fn cell_centre(i: usize) -> f32 {
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
fn ring16() -> [Vec2; 16] {
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

fn polyline_distance(p: Vec2, line: &[Vec2]) -> f32 {
    line.windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
            p.distance(a + ab * t)
        })
        .fold(f32::MAX, f32::min)
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
) -> Mask {
    let piece = rule.piece.as_str();
    let fp = footprint(piece);
    let floors = floor_slots(piece);
    let ok_slot: Vec<bool> = surface
        .slots
        .iter()
        .map(|s| floors.contains(&s.as_str()) && !excluded_slot(s))
        .collect();
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
    let heroes: Vec<(Vec2, f32)> = def
        .props
        .hero
        .iter()
        .map(|h| {
            let c = hero_clearance(footprint(h.piece.as_str()), fp);
            (Vec2::from(h.at), c)
        })
        .collect();
    let lanes: Vec<(Vec<Vec2>, f32)> = lanes(def)
        .into_iter()
        .map(|(line, half)| (line, lane_clearance(rule, half, fp)))
        .collect();
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
            {
                continue;
            }
            let covered = disc.iter().all(|&(di, dj)| {
                let (ii, jj) = (i as i32 + di, j as i32 + dj);
                (0..CELLS as i32).contains(&ii)
                    && (0..CELLS as i32).contains(&jj)
                    && on_floor(jj as usize * CELLS + ii as usize)
            });
            if covered {
                cells.push(k as u32);
            }
        }
    }
    let area = cells.len() as f32 * RASTER * RASTER;
    Mask { cells, area }
}

/// Builds a definition's ground from its ground pieces' LOD0 triangles (`tris_of`, tile-local
/// before the piece's turn), or `None` while one is missing.
pub fn prepare(
    def: &TileDef,
    catalog: &TileCatalog,
    tris_of: &mut dyn FnMut(&str) -> Option<Vec<(Tri, String)>>,
) -> Option<Prepared> {
    let mut all = vec![];
    for (id, k) in ground_pieces(def, catalog) {
        for (t, s) in tris_of(id)? {
            all.push((t.map(|p| turn(k, p)), s));
        }
    }
    let surface = Surface::new(all);
    let raster: Vec<Option<(f32, u16)>> = (0..CELLS * CELLS)
        .map(|k| surface.top(cell_centre(k % CELLS), cell_centre(k / CELLS)))
        .collect();
    let masks = def
        .props
        .scatter
        .iter()
        .map(|r| mask(def, &surface, &raster, r))
        .collect();
    Some(Prepared { surface, masks })
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
}

impl Instance {
    pub fn rotation(&self) -> Quat {
        Quat::from_rotation_y(self.yaw)
    }

    pub fn transform(&self) -> Transform {
        Transform::from_translation(self.pos)
            .with_rotation(self.rotation())
            .with_scale(Vec3::splat(self.scale))
    }
}

/// A tile's scatter. `planned` and `area` are per rule.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub instances: Vec<Instance>,
    pub planned: Vec<u32>,
    pub area: Vec<f32>,
}

impl Layout {
    pub fn placed(&self, rule: usize) -> usize {
        self.instances
            .iter()
            .filter(|i| i.rule as usize == rule)
            .count()
    }
}

/// SplitMix64.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Lowest surface height under the centre and a 0.3 m ring around `p`, if `p` stands on `floors`.
fn stand(surface: &Surface, p: Vec2, floors: &[&str], reach: f32) -> Option<f32> {
    let on_floor = |q: Vec2| {
        surface
            .at(q.x, q.y)
            .filter(|(_, s)| floors.contains(s) && !excluded_slot(s))
    };
    let (h, _) = on_floor(p)?;
    // The footprint's rim and half-rim, exactly (the mask only eroded the sampled ground).
    let ring16 = ring16();
    for d in ring16 {
        on_floor(p + reach * d)?;
    }
    for d in ring16.iter().step_by(2) {
        on_floor(p + 0.5 * reach * *d)?;
    }
    let ring = (0..6).filter_map(|k| {
        let q = p + 0.3 * hex_dir(k);
        surface.at(q.x, q.y).map(|(y, _)| y)
    });
    Some(ring.fold(h, f32::min))
}

/// The scatter of a tile with definition `def` and seed `seed` on its prepared ground.
pub fn layout(def: &TileDef, seed: u64, ground: &Prepared) -> Layout {
    let mut rng = Rng(seed ^ 0x5CA7_7E25_D3A1_0B07);
    let mut out = Layout::default();
    // (centre, ground reach, rule) of everything placed so far.
    let mut placed: Vec<(Vec2, f32, usize)> = vec![];
    for (ri, rule) in def.props.scatter.iter().enumerate() {
        let piece = rule.piece.as_str();
        let fp = footprint(piece);
        let mask = &ground.masks[ri];
        let want = (rule.density.max(0.0) * CAP_AREA / 100.0 + rng.unit()).floor() as u32;
        out.planned.push(want);
        out.area.push(mask.area);
        if want == 0 || mask.cells.is_empty() {
            continue;
        }
        let spacing = (BLUE_NOISE * (mask.area / want as f32).sqrt())
            .max(2.0 * fp.ground)
            .max(CROWN_SPACING * fp.crown);
        let jitter = scale_jitter(fp);
        let floors = floor_slots(piece);
        let mut n = 0;
        for _ in 0..ATTEMPTS * want + 64 {
            if n == want {
                break;
            }
            let k = mask.cells[rng.below(mask.cells.len())] as usize;
            let p = Vec2::new(
                cell_centre(k % CELLS) + (rng.unit() - 0.5) * RASTER,
                cell_centre(k / CELLS) + (rng.unit() - 0.5) * RASTER,
            );
            let clear = placed.iter().all(|&(q, g, r)| {
                let d = p.distance(q);
                if r == ri {
                    d >= spacing
                } else {
                    d >= g + fp.ground
                }
            });
            if !clear {
                continue;
            }
            let Some(y) = stand(&ground.surface, p, floors, fp.ground) else {
                continue;
            };
            let yaw = rng.unit() * TAU;
            let scale = 1.0 + jitter * (2.0 * rng.unit() - 1.0);
            placed.push((p, fp.ground, ri));
            out.instances.push(Instance {
                rule: ri as u8,
                pos: Vec3::new(p.x, y, p.y),
                yaw,
                scale,
                order: 0,
            });
            n += 1;
        }
    }
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

// ---------------------------------------------------------------------------------------------
// Play: drawing, streaming, the shared layouts and the report

/// The entity holding one tile's scatter, a child of the tile root.
#[derive(Component)]
pub struct ScatterGroup;

/// What a tile's group was drawn from: its tile, seed and which rules' pieces the kit had.
#[derive(Clone, Debug, PartialEq)]
struct Key {
    tile: String,
    seed: u64,
    drawn: Vec<bool>,
}

/// Prepared grounds by tile id, layouts by cell, and the group drawn under each tile root.
#[derive(Resource, Default)]
pub struct PlayScatter {
    prepared: HashMap<String, Arc<Prepared>>,
    layouts: HashMap<Axial, (String, u64, Arc<Layout>)>,
    roots: HashMap<Entity, (Key, Option<Entity>)>,
}

/// One drawn tile's scatter.
#[derive(Clone, Debug)]
pub struct TileScatter {
    pub tile: String,
    pub seed: u64,
    pub layout: Arc<Layout>,
    /// Per rule: its piece is in the kit and its instances are drawn.
    pub drawn: Vec<bool>,
    /// Per rule: its piece.
    pub pieces: Vec<String>,
}

/// The scatter of every drawn tile, for the boids' roosts; `revision` changes with it.
#[derive(Resource, Default)]
pub struct ScatterLayouts {
    pub revision: u64,
    pub tiles: BTreeMap<Axial, TileScatter>,
}

impl ScatterLayouts {
    /// Every drawn instance with its cell and piece.
    pub fn drawn(&self) -> impl Iterator<Item = (Axial, &str, &Instance)> {
        self.tiles.iter().flat_map(|(c, t)| {
            t.layout
                .instances
                .iter()
                .filter(|i| t.drawn[i.rule as usize])
                .map(move |i| (*c, t.pieces[i.rule as usize].as_str(), i))
        })
    }
}

/// `hexWorld.state().scatter` as JSON: per tile, the planned, placed and drawn count of each piece.
#[derive(Resource, Clone, Debug, Default)]
pub struct ScatterReport(pub String);

/// LOD0 triangles of a loaded kit piece with their slot names (`build_piece`'s slot walk).
fn kit_tris(
    id: &str,
    packs: &PlayPacks,
    (gltfs, gltf_meshes, meshes): (&Assets<Gltf>, &Assets<GltfMesh>, &Assets<Mesh>),
) -> Option<Vec<(Tri, String)>> {
    let g = gltfs.get(packs.glb(id)?)?;
    let gm = gltf_meshes.get(g.named_meshes.get(format!("{id}_lod0").as_str())?)?;
    let mut out = vec![];
    for prim in &gm.primitives {
        let name = prim
            .material
            .as_ref()
            .and_then(|mh| {
                g.named_materials
                    .iter()
                    .find(|(_, v)| *v == mh)
                    .map(|(k, _)| k.to_string())
            })
            .unwrap_or_default();
        let mesh = meshes.get(&prim.mesh)?;
        let pos = mesh.attribute(Mesh::ATTRIBUTE_POSITION)?.as_float3()?;
        let idx: Vec<usize> = mesh.indices()?.iter().collect();
        for t in idx.as_chunks::<3>().0 {
            out.push(([pos[t[0]], pos[t[1]], pos[t[2]]], name.clone()));
        }
    }
    Some(out)
}

/// Spawns the drawn instances of `layout` under `root`, growing in the props stage of `cell`'s rise.
fn spawn_group(
    commands: &mut Commands,
    (root, cell): (Entity, Axial),
    kit: &HashMap<String, Vec<KitPart>>,
    def: &TileDef,
    layout: &Layout,
    drawn: &[bool],
) -> Option<Entity> {
    if !layout.instances.iter().any(|i| drawn[i.rule as usize]) {
        return None;
    }
    let count = layout.instances.len().min(u8::MAX as usize) as u8;
    let group = commands
        .spawn((
            ScatterGroup,
            Transform::IDENTITY,
            Visibility::default(),
            SceneEntity,
        ))
        .id();
    commands.entity(root).add_child(group);
    commands.entity(group).with_children(|p| {
        for inst in layout.instances.iter().filter(|i| drawn[i.rule as usize]) {
            let role = RiseRole::Prop {
                order: inst.order,
                count,
                kind: PropKind::Tree,
            };
            let piece = def.props.scatter[inst.rule as usize].piece.as_str();
            spawn_kit_parts(p, kit, piece, inst.transform(), Some((cell, role)));
        }
    });
    Some(group)
}

fn json_counts(pieces: &[String], n: impl Fn(usize) -> usize) -> String {
    let fields: Vec<String> = pieces
        .iter()
        .enumerate()
        .map(|(i, p)| format!("\"{p}\":{}", n(i)))
        .collect();
    format!("{{{}}}", fields.join(","))
}

fn report(
    world_seed: u64,
    tiles: &BTreeMap<Axial, TileScatter>,
    waiting: &[(Axial, String)],
    kit: &HashMap<String, Vec<KitPart>>,
) -> String {
    let mut rows = vec![];
    let mut drawn_by_piece: BTreeMap<&str, usize> = BTreeMap::new();
    let mut placed_total = 0;
    for (c, t) in tiles {
        let l = &t.layout;
        let drawn = |i: usize| if t.drawn[i] { l.placed(i) } else { 0 };
        for (i, p) in t.pieces.iter().enumerate() {
            *drawn_by_piece.entry(p.as_str()).or_default() += drawn(i);
        }
        placed_total += l.instances.len();
        rows.push(format!(
            "{{\"q\":{},\"r\":{},\"tile\":\"{}\",\"planned\":{},\"placed\":{},\"drawn\":{},\"area_m2\":{}}}",
            c.q,
            c.r,
            t.tile,
            json_counts(&t.pieces, |i| l.planned[i] as usize),
            json_counts(&t.pieces, |i| l.placed(i)),
            json_counts(&t.pieces, drawn),
            json_counts(&t.pieces, |i| l.area[i].round() as usize),
        ));
    }
    let entities: usize = drawn_by_piece
        .iter()
        .map(|(p, n)| n * kit.get(*p).map_or(0, Vec::len))
        .sum();
    let pairs: HashSet<_> = drawn_by_piece
        .iter()
        .filter(|(_, n)| **n > 0)
        .flat_map(|(p, _)| kit.get(*p).into_iter().flatten())
        .map(|part| (part.mesh.id(), part.material.id()))
        .collect();
    let drawn_total: usize = drawn_by_piece.values().sum();
    let by_piece: Vec<String> = drawn_by_piece
        .iter()
        .map(|(p, n)| format!("\"{p}\":{n}"))
        .collect();
    let waiting: Vec<String> = waiting
        .iter()
        .map(|(c, t)| format!("{{\"q\":{},\"r\":{},\"tile\":\"{t}\"}}", c.q, c.r))
        .collect();
    format!(
        "{{\"world_seed\":{world_seed},\"tiles\":[{}],\"waiting\":[{}],\"placed\":{placed_total},\"drawn\":{drawn_total},\"drawn_by_piece\":{{{}}},\"entities\":{entities},\"mesh_material_pairs\":{}}}",
        rows.join(","),
        waiting.join(","),
        by_piece.join(","),
        pairs.len()
    )
}

/// After `play::sync`: draws each placed tile's scatter under its root once its ground is in the
/// kit, redraws it when a rule's piece arrives or a pack refresh rebuilt the tile's children, and
/// publishes the layouts and the report.
#[allow(clippy::too_many_arguments)]
pub fn sync(
    mut commands: Commands,
    active: Res<ActiveScenario>,
    view: Res<PlayView>,
    (spawned, kit, packs, catalog): (
        Option<Res<PlaySpawned>>,
        Option<Res<PlayKit>>,
        Option<Res<PlayPacks>>,
        Res<PlayCatalog>,
    ),
    (gltfs, gltf_meshes, meshes): (Res<Assets<Gltf>>, Res<Assets<GltfMesh>>, Res<Assets<Mesh>>),
    (mut state, mut layouts, mut out): (
        ResMut<PlayScatter>,
        ResMut<ScatterLayouts>,
        ResMut<ScatterReport>,
    ),
    groups: Query<(), With<ScatterGroup>>,
    mut removed: RemovedComponents<ScatterGroup>,
) {
    let gone = removed.read().count() > 0;
    let (Scenario::Play, Some(spawned), Some(kit_res), Some(packs)) =
        (active.0, spawned, kit, packs)
    else {
        return;
    };
    let PlayKit::Ready(kit) = &*kit_res else {
        return;
    };
    if !(view.is_changed() || spawned.is_changed() || kit_res.is_changed() || gone) {
        return;
    }
    let state = &mut *state;
    let assets = (&*gltfs, &*gltf_meshes, &*meshes);
    let mut roots = HashMap::new();
    let mut next_layouts = HashMap::new();
    let mut tiles = BTreeMap::new();
    let mut waiting = vec![];
    let mut cells: Vec<_> = spawned.tiles.iter().collect();
    cells.sort_by_key(|(c, _)| (c.q, c.r));
    for (&cell, (t, root)) in cells {
        let Some(def) = catalog
            .0
            .get(&t.tile)
            .filter(|d| !d.props.scatter.is_empty())
        else {
            continue;
        };
        let ground_ready = ground_pieces(def, &catalog.0)
            .iter()
            .all(|(id, _)| kit.contains_key(*id));
        let prepared = match state.prepared.get(&def.id) {
            Some(p) => Some(p.clone()),
            None if ground_ready => {
                prepare(def, &catalog.0, &mut |id| kit_tris(id, &packs, assets)).map(|p| {
                    let p = Arc::new(p);
                    state.prepared.insert(def.id.clone(), p.clone());
                    p
                })
            }
            None => None,
        };
        let Some(prepared) = prepared.filter(|_| ground_ready) else {
            waiting.push((cell, def.id.clone()));
            continue;
        };
        let seed = cell_seed(view.world_seed, cell);
        let layout = match state.layouts.get(&cell) {
            Some((id, s, l)) if *id == def.id && *s == seed => l.clone(),
            _ => Arc::new(layout(def, seed, &prepared)),
        };
        let drawn: Vec<bool> = def
            .props
            .scatter
            .iter()
            .map(|r| kit.contains_key(r.piece.as_str()))
            .collect();
        let key = Key {
            tile: def.id.clone(),
            seed,
            drawn: drawn.clone(),
        };
        let group = match state.roots.get(root) {
            Some((k, g)) if *k == key && g.is_none_or(|g| groups.contains(g)) => *g,
            old => {
                if let Some((_, Some(g))) = old {
                    commands.entity(*g).try_despawn();
                }
                spawn_group(&mut commands, (*root, cell), kit, def, &layout, &drawn)
            }
        };
        roots.insert(*root, (key, group));
        next_layouts.insert(cell, (def.id.clone(), seed, layout.clone()));
        tiles.insert(
            cell,
            TileScatter {
                tile: def.id.clone(),
                seed,
                layout,
                drawn,
                pieces: def
                    .props
                    .scatter
                    .iter()
                    .map(|r| r.piece.as_str().to_owned())
                    .collect(),
            },
        );
    }
    state.roots = roots;
    state.layouts = next_layouts;
    let sig = |m: &BTreeMap<Axial, TileScatter>| -> Vec<(Axial, String, u64, Vec<bool>)> {
        m.iter()
            .map(|(c, t)| (*c, t.tile.clone(), t.seed, t.drawn.clone()))
            .collect()
    };
    if sig(&layouts.tiles) != sig(&tiles) {
        layouts.revision += 1;
        layouts.tiles = tiles;
    }
    let json = report(view.world_seed, &layouts.tiles, &waiting, kit);
    if out.0 != json {
        out.0 = json;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use hex_assets::parse_manifest;
    use hex_core::{AgentKind, World};

    use super::*;
    use crate::kit_scene::{MANIFEST, piece_rotation};

    type Tris = Option<Vec<(Tri, String)>>;

    fn catalog() -> TileCatalog {
        TileCatalog::builtin().unwrap()
    }

    fn glb(id: &str) -> Option<(gltf::Document, Vec<gltf::buffer::Data>)> {
        let m = parse_manifest(MANIFEST).unwrap();
        let e = m.pieces.iter().find(|p| p.id == id)?;
        let path = format!("{}/../../assets/kit/{}", env!("CARGO_MANIFEST_DIR"), e.glb);
        let bytes = std::fs::read(path).ok()?;
        let plain = hex_assets::rewrite_glb(&bytes).ok()?;
        let (doc, buffers, _) = gltf::import_slice(&plain).ok()?;
        Some((doc, buffers))
    }

    /// Triangles of mesh `<id>_<mesh>` from the piece's GLB, with slot names (the kit gate's
    /// `triangles_with_slot`).
    fn glb_mesh_tris(id: &str, mesh: &str) -> Tris {
        let (doc, buffers) = glb(id)?;
        let name = format!("{id}_{mesh}");
        let m = doc.meshes().find(|m| m.name() == Some(name.as_str()))?;
        let mut out = vec![];
        for prim in m.primitives() {
            let slot = prim.material().name().unwrap_or_default().to_string();
            let r = prim.reader(|b| Some(&buffers[b.index()]));
            let pos: Vec<[f32; 3]> = r.read_positions()?.collect();
            let idx: Vec<u32> = r.read_indices()?.into_u32().collect();
            for t in idx.as_chunks::<3>().0 {
                out.push((
                    [pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]],
                    slot.clone(),
                ));
            }
        }
        Some(out)
    }

    /// LOD0 triangles, read once per piece for the whole test binary.
    fn lod0(id: &str) -> Tris {
        static CACHE: OnceLock<Mutex<HashMap<String, Tris>>> = OnceLock::new();
        let cache = CACHE.get_or_init(Default::default);
        if let Some(t) = cache.lock().unwrap().get(id) {
            return t.clone();
        }
        let t = glb_mesh_tris(id, "lod0");
        cache.lock().unwrap().insert(id.to_owned(), t.clone());
        t
    }

    fn prepared(def: &TileDef) -> Option<Prepared> {
        prepare(def, &catalog(), &mut |id| lod0(id))
    }

    fn rule(piece: &str, density: f32) -> ScatterRule {
        ScatterRule {
            piece: piece.into(),
            density,
            lane_clearance: 0.0,
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_instances() {
        let c = catalog();
        for id in ["forest", "meadow"] {
            let def = c.get(id).unwrap();
            let g = prepared(def).expect("ground pieces in the kit");
            let a = layout(def, 7, &g);
            assert!(!a.instances.is_empty(), "{id}");
            assert_eq!(a, layout(def, 7, &g), "{id}: same seed");
            // A second preparation of the same ground scatters identically too.
            assert_eq!(a, layout(def, 7, &prepared(def).unwrap()), "{id}");
            let b = layout(def, 8, &g);
            assert_ne!(a.instances, b.instances, "{id}: another seed");
        }
        // Seeded from the world seed and the cell: `PlacedTile::seed`.
        let w = World::new(3);
        let origin = w.tiles()[&Axial::ORIGIN].seed;
        assert_eq!(origin, cell_seed(3, Axial::ORIGIN));
        assert_ne!(
            cell_seed(3, Axial::new(1, 0)),
            cell_seed(4, Axial::new(1, 0))
        );
    }

    /// Every MVP tile, every rotation: rules that declare no scatter get one rule per scatter piece,
    /// so roads, rivers, beaches, the ocean and the house pad are exercised too. Each instance's
    /// centre and footprint ring, turned with the tile, stand on its piece's floor in the tile's
    /// turned ground, never on an excluded slot, clear of heroes, lanes and the border.
    #[test]
    fn no_instance_on_an_excluded_slot_for_every_mvp_tile_and_rotation() {
        let c = catalog();
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for base in c.tiles() {
            let mut def = base.clone();
            if def.props.scatter.is_empty() {
                def.props.scatter = vec![
                    rule("tree_forest", 2.0),
                    rule("rock_small", 1.0),
                    rule("wildflower_clump", 3.0),
                ];
            }
            let g = prepared(&def).unwrap_or_else(|| panic!("{}: ground pieces missing", def.id));
            let local: Vec<(Tri, String)> = ground_pieces(&def, &c)
                .into_iter()
                .flat_map(|(id, k)| {
                    lod0(id)
                        .unwrap()
                        .into_iter()
                        .map(move |(t, s)| (t.map(|p| turn(k, p)), s))
                })
                .collect();
            let lanes = lanes(&def);
            for k in 0..6u8 {
                let rot = piece_rotation(k);
                let world = Surface::new(
                    local
                        .iter()
                        .map(|(t, s)| (t.map(|p| (rot * Vec3::from(p)).to_array()), s.clone())),
                );
                for seed in 0..4u64 {
                    let l = layout(&def, seed * 7919 + u64::from(k), &g);
                    *counts.entry(def.id.clone()).or_default() += l.instances.len();
                    for inst in &l.instances {
                        let r = &def.props.scatter[inst.rule as usize];
                        let piece = r.piece.as_str();
                        let fp = footprint(piece);
                        let floors = floor_slots(piece);
                        let mut probes = vec![Vec3::ZERO];
                        probes.extend(
                            ring16()
                                .iter()
                                .step_by(2)
                                .map(|d| Vec3::new(d.x, 0.0, d.y) * fp.ground),
                        );
                        for off in probes {
                            let p = rot * (inst.pos + off);
                            let (h, slot) = world.at(p.x, p.z).unwrap_or_else(|| {
                                panic!("{} k{k}: {piece} over no ground at {p}", def.id)
                            });
                            assert!(
                                floors.contains(&slot) && !excluded_slot(slot),
                                "{} k{k} seed {seed}: {piece} at {} over {slot}",
                                def.id,
                                inst.pos
                            );
                            if off == Vec3::ZERO {
                                assert!(
                                    inst.pos.y <= h + 1e-4 && inst.pos.y > h - 0.1,
                                    "{} {piece}: y {} vs surface {h}",
                                    def.id,
                                    inst.pos.y
                                );
                            }
                        }
                        let p = inst.pos.xz();
                        assert!(hex_reach(p) <= border_reach(fp) + 0.2, "{} border", def.id);
                        for h in &def.props.hero {
                            let need = hero_clearance(footprint(h.piece.as_str()), fp);
                            assert!(
                                p.distance(Vec2::from(h.at)) >= need - 0.2,
                                "{} hero",
                                def.id
                            );
                        }
                        for (line, half) in &lanes {
                            let need = lane_clearance(r, *half, fp);
                            assert!(polyline_distance(p, line) >= need - 0.2, "{} lane", def.id);
                        }
                    }
                }
            }
        }
        assert_eq!(counts.len(), 14, "every MVP tile ran");
        for id in ["forest", "meadow", "road_straight", "house_cluster"] {
            assert!(counts[id] > 0, "{id} scatters: {counts:?}");
        }
        for id in [
            "ocean",
            "river_straight",
            "river_bend",
            "beach",
            "beach_inner",
            "beach_outer",
        ] {
            assert_eq!(counts[id], 0, "{id} has no scatter floor: {counts:?}");
        }
    }

    /// Density is per 100 m² of the hex cap: the mean count over cells matches it within 3%, every
    /// tile places its whole planned count, and same-piece neighbours keep the blue-noise spacing.
    #[test]
    fn density_is_per_100_m2_of_cap_and_spacing_is_blue_noise() {
        let c = catalog();
        for id in ["forest", "meadow"] {
            let def = c.get(id).unwrap();
            let g = prepared(def).unwrap();
            let seeds = 64u64;
            let mut sums = vec![0usize; def.props.scatter.len()];
            for seed in 0..seeds {
                let l = layout(
                    def,
                    cell_seed(1, Axial::new(seed as i32, -(seed as i32))),
                    &g,
                );
                for (ri, r) in def.props.scatter.iter().enumerate() {
                    let want = r.density * CAP_AREA / 100.0;
                    let n = l.placed(ri);
                    assert_eq!(
                        n as u32,
                        l.planned[ri],
                        "{id} {}: shortfall",
                        r.piece.as_str()
                    );
                    assert!((n as f32 - want).abs() < 1.0, "{id}: {n} vs {want}");
                    sums[ri] += n;
                    let fp = footprint(r.piece.as_str());
                    let pts: Vec<Vec2> = l
                        .instances
                        .iter()
                        .filter(|i| i.rule as usize == ri)
                        .map(|i| i.pos.xz())
                        .collect();
                    let spacing = (BLUE_NOISE * (l.area[ri] / n.max(1) as f32).sqrt())
                        .max(2.0 * fp.ground)
                        .max(CROWN_SPACING * fp.crown);
                    for (a, p) in pts.iter().enumerate() {
                        for q in &pts[a + 1..] {
                            assert!(p.distance(*q) >= spacing - 1e-4, "{id}: too close");
                        }
                    }
                }
            }
            for (ri, r) in def.props.scatter.iter().enumerate() {
                let mean = sums[ri] as f32 / seeds as f32 / CAP_AREA * 100.0;
                assert!(
                    (mean - r.density).abs() <= 0.03 * r.density.max(1.0),
                    "{id} {}: {mean} per 100 m² vs {}",
                    r.piece.as_str(),
                    r.density
                );
            }
        }
    }

    /// The nearest-neighbour distances of a forest's trees vary: Poisson-disc, not a lattice.
    #[test]
    fn spacing_is_not_a_grid() {
        let c = catalog();
        let def = c.get("forest").unwrap();
        let l = layout(def, 11, &prepared(def).unwrap());
        let trees: Vec<Vec2> = l
            .instances
            .iter()
            .filter(|i| i.rule == 0)
            .map(|i| i.pos.xz())
            .collect();
        let nn: Vec<f32> = trees
            .iter()
            .enumerate()
            .map(|(a, p)| {
                trees
                    .iter()
                    .enumerate()
                    .filter(|(b, _)| *b != a)
                    .map(|(_, q)| p.distance(*q))
                    .fold(f32::MAX, f32::min)
            })
            .collect();
        let mean = nn.iter().sum::<f32>() / nn.len() as f32;
        let sd = (nn.iter().map(|d| (d - mean).powi(2)).sum::<f32>() / nn.len() as f32).sqrt();
        // A lattice spreads ~0%; dense Poisson-disc packing (20 spruces around the oak's clearance)
        // still spreads ~7%.
        assert!(
            sd > 0.05 * mean,
            "nearest-neighbour spread {sd} of mean {mean}"
        );
        let yaws: HashSet<u32> = l.instances.iter().map(|i| i.yaw.to_bits()).collect();
        assert_eq!(
            yaws.len(),
            l.instances.len(),
            "every instance has its own yaw"
        );
        assert!(l.instances.iter().all(|i| (0.85..=1.15).contains(&i.scale)));
    }

    /// Undo and redo (and bulldoze then undo) bring a tile back with the seed it had, so it
    /// scatters identically.
    #[test]
    fn undo_and_redo_restore_an_identical_scatter() {
        let c = catalog();
        let def = c.get("forest").unwrap();
        let g = prepared(def).unwrap();
        let mut w = World::new(42);
        let cell = Axial::new(1, 0);
        let k = w
            .auto_rotate(&c, cell, def)
            .expect("a forest fits next to the meadow");
        w.place(&c, cell, "forest", k).unwrap();
        let scatter = |w: &World| {
            let t = &w.tiles()[&cell];
            assert_eq!(t.seed, cell_seed(w.seed(), cell));
            layout(c.get(&t.def).unwrap(), cell_seed(w.seed(), cell), &g)
        };
        let first = scatter(&w);
        assert!(w.undo());
        assert!(!w.tiles().contains_key(&cell));
        assert!(w.redo());
        assert_eq!(scatter(&w), first, "redo");
        w.bulldoze(cell).unwrap();
        assert!(w.undo());
        assert_eq!(scatter(&w), first, "undo of a bulldoze");
    }

    /// The footprint table matches the kit: ground reach is the collider's, crown reach the widest
    /// LOD's (within 0.1 m, never under).
    #[test]
    fn footprints_match_the_kit() {
        let reach = |tris: &[(Tri, String)]| {
            tris.iter()
                .flat_map(|(t, _)| t.iter())
                .map(|p| Vec2::new(p[0], p[2]).length())
                .fold(0.0, f32::max)
        };
        for (id, f) in FOOTPRINTS {
            let ground = reach(&glb_mesh_tris(id, "collider").unwrap());
            assert!(
                f.ground >= ground && f.ground <= ground + 0.1,
                "{id} ground {ground}"
            );
            if f.crown > 0.0 {
                let crown = (0..3)
                    .map(|l| reach(&glb_mesh_tris(id, &format!("lod{l}")).unwrap()))
                    .fold(0.0, f32::max);
                assert!(
                    f.crown >= crown && f.crown <= crown + 0.1,
                    "{id} crown {crown}"
                );
            }
        }
    }

    /// The sqrt-built directions and turns agree with the trig and `piece_rotation` the renderer
    /// uses, so the exact layout frame is the drawn one.
    #[test]
    fn exact_directions_match_trig_and_piece_rotation() {
        for k in 0..6u8 {
            let a = (60.0 * f32::from(k)).to_radians();
            assert!(
                hex_dir(k).distance(Vec2::new(a.cos(), a.sin())) < 1e-6,
                "edge {k}"
            );
            let p = [3.0, 1.5, -7.0];
            let want = piece_rotation(k) * Vec3::from(p);
            assert!(Vec3::from(turn(k, p)).distance(want) < 1e-5, "turn {k}");
        }
        for (k, d) in ring16().iter().enumerate() {
            let a = TAU * k as f32 / 16.0;
            assert!(d.distance(Vec2::new(a.cos(), a.sin())) < 1e-6, "ring {k}");
        }
    }

    /// The forest's spruces leave the oak's canopy clear, so its perches stay open.
    #[test]
    fn spruces_clear_the_oak_canopy() {
        let c = catalog();
        let def = c.get("forest").unwrap();
        assert!(def.agents.iter().any(|a| a.agent == AgentKind::Bird));
        let g = prepared(def).unwrap();
        for seed in 0..16 {
            for i in layout(def, seed, &g)
                .instances
                .iter()
                .filter(|i| i.rule == 0)
            {
                assert!(
                    i.pos.xz().length() >= HERO_CROWN * 7.1 + 0.5 - 0.2,
                    "{}",
                    i.pos
                );
            }
        }
    }
}
