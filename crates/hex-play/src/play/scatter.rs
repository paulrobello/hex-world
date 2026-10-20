//! Seeded prop scatter in play (PRD §5.3, §6): the pine forest's spruces and rocks, the oak tile's
//! rocks and flowers on the litter, and the meadow's wildflowers and rocks, from each tile's
//! `props.scatter` rules.
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
//! - **Drifts.** A rule with `drift` (the meadow's wildflowers, A18) grows only on the [`DRIFT_SHARE`]
//!   of its ground where the tile's seeded value noise ([`drift_noise`]) is highest, so its
//!   instances gather in a few clustered drifts. The noise is tile-local and seeded from the tile
//!   alone, so a tile needs no neighbour to scatter and redraws identically.
//! - **Variants.** A rule names a piece; each instance draws one piece of its group ([`VARIANTS`]:
//!   the piece and its `_b`, `_c`, … variants, PRD §6 Variety), picked by [`pick_variant`] from a
//!   hash of the tile seed, the rule and the instance's place in it, so the dart stream is
//!   untouched and the same seed draws the same mix. The rule's ground mask and spacing keep the
//!   group's largest footprint clear ([`group_footprint`]); each instance stands and clears others
//!   by its own. A rule draws once every piece of its group is in the kit.
//! - **Vary.** A rule with `vary` (the meadow's single wildflowers) spreads evenly but keeps a dart
//!   with probability 1 - vary × (1 - [`vary_noise`]), so its density varies gently over the cap
//!   while its count stays exact.
//! - **Ground.** An instance stands only on its piece's floor slots ([`floor_slots`]) of the topmost
//!   LOD0 surface of the tile's base, centre and lane edge pieces, sampled the way the kit gate's
//!   `surface_at` does: trees on `forest_base_floor` (the litter), rocks and wildflowers on either
//!   that and `meadow_base_cap`. Its whole ground footprint must lie on that floor, so nothing
//!   stands on a road, a river bed or bank, a house pad or driveway, a beach, a rim, a skirt or water
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
const RASTER: f32 = 0.25;
const EXTENT: f32 = 16.5;
const CELLS: usize = 132;
/// Bucket size (m) of the surface's triangle grid.
const BUCKET: f32 = 1.0;
const BUCKETS: usize = 33;

/// The slots a scatter piece may stand on (the topmost LOD0 surface under its whole footprint).
/// A piece not listed has none, so it is never placed.
pub fn floor_slots(piece: &str) -> &'static [&'static str] {
    match family(piece) {
        "tree_forest" | "tree_oak" | "fern_clump" | "log_fallen" => &["forest_base_floor"],
        "rock_small" | "wildflower_clump" => &["forest_base_floor", "meadow_base_cap"],
        "wildflower_single" => &["meadow_base_cap"],
        _ => &[],
    }
}

/// Variants of a scatter piece (PRD §6 Variety), after the piece itself: a rule names the first
/// and each of its instances draws one of the group, picked by [`pick_variant`].
pub const VARIANTS: [(&str, &[&str]); 5] = [
    ("tree_forest", &["tree_forest_b", "tree_forest_c"]),
    ("tree_oak", &["tree_oak_b"]),
    ("rock_small", &["rock_small_b", "rock_small_c"]),
    (
        "wildflower_clump",
        &["wildflower_clump_b", "wildflower_clump_c"],
    ),
    (
        "wildflower_single",
        &[
            "wildflower_single_b",
            "wildflower_single_c",
            "wildflower_single_d",
        ],
    ),
];

fn extra_variants(piece: &str) -> &'static [&'static str] {
    VARIANTS
        .iter()
        .find(|(id, _)| *id == piece)
        .map_or(&[], |(_, v)| *v)
}

/// The piece a rule naming `piece` draws as variant `v` (0 is `piece` itself).
pub fn variant_piece(piece: &str, v: u8) -> &str {
    match v {
        0 => piece,
        _ => extra_variants(piece)
            .get(usize::from(v) - 1)
            .copied()
            .unwrap_or(piece),
    }
}

/// `piece` and its variants.
pub fn group(piece: &str) -> impl Iterator<Item = &str> {
    std::iter::once(piece).chain(extra_variants(piece).iter().copied())
}

/// The piece a variant belongs to (itself when it is not a variant).
pub fn family(piece: &str) -> &str {
    VARIANTS
        .iter()
        .find(|(_, v)| v.contains(&piece))
        .map_or(piece, |(id, _)| *id)
}

/// The largest ground and crown reach over `piece`'s group: what a rule's ground mask and spacing
/// keep clear, so any variant fits wherever the rule may place one.
pub fn group_footprint(piece: &str) -> Footprint {
    group(piece).map(footprint).fold(
        Footprint {
            ground: 0.0,
            crown: 0.0,
        },
        |a, f| Footprint {
            ground: a.ground.max(f.ground),
            crown: a.crown.max(f.crown),
        },
    )
}

/// The variant of the `n`-th instance placed by rule `rule` on a tile with seed `seed`, among
/// `count`: a hash of its own, so picking it never moves the placement stream's darts, and the
/// same seed always draws the same mix.
pub fn pick_variant(seed: u64, rule: usize, n: u32, count: usize) -> u8 {
    if count <= 1 {
        return 0;
    }
    let key = ((rule as u64) << 32) | u64::from(n);
    let mut r = Rng(seed ^ 0x7A21_C3E9_55B0_4D1F ^ key.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    r.below(count) as u8
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
pub const FOOTPRINTS: [(&str, Footprint); 26] = [
    (
        "tree_forest",
        Footprint {
            ground: 0.51,
            crown: 4.1,
        },
    ),
    (
        "tree_forest_b",
        Footprint {
            ground: 0.6,
            crown: 3.2,
        },
    ),
    (
        "tree_forest_c",
        Footprint {
            ground: 0.27,
            crown: 2.85,
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
        "rock_small_b",
        Footprint {
            ground: 1.03,
            crown: 0.0,
        },
    ),
    (
        "rock_small_c",
        Footprint {
            ground: 1.07,
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
        "wildflower_clump_b",
        Footprint {
            ground: 0.96,
            crown: 0.0,
        },
    ),
    (
        "wildflower_clump_c",
        Footprint {
            ground: 0.96,
            crown: 0.0,
        },
    ),
    (
        "wildflower_single",
        Footprint {
            ground: 0.25,
            crown: 0.0,
        },
    ),
    (
        "wildflower_single_b",
        Footprint {
            ground: 0.25,
            crown: 0.0,
        },
    ),
    (
        "wildflower_single_c",
        Footprint {
            ground: 0.25,
            crown: 0.0,
        },
    ),
    (
        "wildflower_single_d",
        Footprint {
            ground: 0.25,
            crown: 0.0,
        },
    ),
    (
        "tree_hero_oak",
        Footprint {
            ground: 2.5,
            crown: 8.55,
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
    // The tower trunk only: the sails turn overhead, from 3.7 m up.
    (
        "windmill",
        Footprint {
            ground: 2.9,
            crown: 0.0,
        },
    ),
    // The farm tile's heroes: the barn (10 x 7 m, so its reach is its corner) and the static
    // bales and animals.
    (
        "barn_red",
        Footprint {
            ground: 6.6,
            crown: 0.0,
        },
    ),
    (
        "hay_bale_round",
        Footprint {
            ground: 0.9,
            crown: 0.0,
        },
    ),
    (
        "hay_bale_stack",
        Footprint {
            ground: 0.9,
            crown: 0.0,
        },
    ),
    (
        "animal_cow",
        Footprint {
            ground: 1.5,
            crown: 0.0,
        },
    ),
    (
        "animal_sheep",
        Footprint {
            ground: 0.8,
            crown: 0.0,
        },
    ),
    (
        "fern_clump",
        Footprint {
            ground: 0.3,
            crown: 0.0,
        },
    ),
    (
        "log_fallen",
        Footprint {
            ground: 1.9,
            crown: 0.0,
        },
    ),
    (
        "tree_oak",
        Footprint {
            ground: 0.59,
            crown: 4.86,
        },
    ),
    (
        "tree_oak_b",
        Footprint {
            ground: 0.35,
            crown: 3.35,
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

/// A rectangular piece's collider bounds in its own frame (x, z), before its yaw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundBox {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

/// The buildings whose collider is a rectangle (`ground_boxes_match_the_kit` checks them against
/// the GLBs). Every other piece, the windmill's round tower included, is a disc of its
/// [`Footprint::ground`].
pub const GROUND_BOXES: [(&str, GroundBox); 4] = [
    (
        "house_cottage",
        GroundBox {
            min: [-2.5, -3.85],
            max: [2.5, 3.45],
        },
    ),
    (
        "house_twostorey",
        GroundBox {
            min: [-2.8, -3.8],
            max: [2.8, 4.25],
        },
    ),
    (
        "barn_red",
        GroundBox {
            min: [-5.3, -3.85],
            max: [5.3, 3.85],
        },
    ),
    (
        "bakery_shop",
        GroundBox {
            min: [-2.8, -4.0],
            max: [3.15, 4.0],
        },
    ),
];

pub fn ground_box(piece: &str) -> Option<GroundBox> {
    GROUND_BOXES
        .iter()
        .find(|(id, _)| *id == piece)
        .map(|(_, b)| *b)
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

/// Slots of ground pieces that stand on the ground rather than being it: a crop's leaning row cards
/// on a centre piece. Nothing stands on them and the walker walks through them.
pub const NOT_GROUND: [&str; 1] = ["wheat_field_wheat"];

impl Surface {
    /// Vertical (skirt) and degenerate triangles carry no ground and are dropped, as are the
    /// [`NOT_GROUND`] slots.
    pub fn new(parts: impl IntoIterator<Item = (Tri, String)>) -> Self {
        let mut slots: Vec<String> = vec![];
        let mut tris = vec![];
        let mut buckets = vec![vec![]; BUCKETS * BUCKETS];
        for (t, name) in parts {
            if NOT_GROUND.contains(&name.as_str()) {
                continue;
            }
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
    let fp = group_footprint(piece);
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

    fn below(&mut self, n: usize) -> usize {
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
        let fp = group_footprint(piece);
        let variants = group(piece).count();
        let mask = &ground.masks[ri];
        let want = (rule.density.max(0.0) * CAP_AREA / 100.0 + rng.unit()).floor() as u32;
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
        let mut n = 0;
        for _ in 0..ATTEMPTS * want + 64 {
            if n == want {
                break;
            }
            let variant = pick_variant(seed, ri, n, variants);
            let reach = footprint(variant_piece(piece, variant)).ground;
            let k = cells[rng.below(cells.len())] as usize;
            let p = Vec2::new(
                cell_centre(k % CELLS) + (rng.unit() - 0.5) * RASTER,
                cell_centre(k / CELLS) + (rng.unit() - 0.5) * RASTER,
            );
            if vary > 0.0 && rng.unit() > 1.0 - vary * (1.0 - vary_noise(seed, p)) {
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
            let Some(y) = stand(&ground.surface, p, floors, reach) else {
                continue;
            };
            let yaw = rng.unit() * TAU;
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

impl PlayScatter {
    /// The definition's prepared ground, built once its ground pieces are in the kit and shared by
    /// every placed tile of that definition and the ghost.
    pub(crate) fn ground(
        &mut self,
        def: &TileDef,
        catalog: &TileCatalog,
        kit: &HashMap<String, Vec<KitPart>>,
        tris_of: &mut dyn FnMut(&str) -> Option<Vec<(Tri, String)>>,
    ) -> Option<Arc<Prepared>> {
        if !ground_pieces(def, catalog)
            .iter()
            .all(|(id, _)| kit.contains_key(*id))
        {
            return None;
        }
        if let Some(p) = self.prepared.get(&def.id) {
            return Some(p.clone());
        }
        let p = Arc::new(prepare(def, catalog, tris_of)?);
        self.prepared.insert(def.id.clone(), p.clone());
        Some(p)
    }
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

impl TileScatter {
    /// The piece instance `i` draws: its rule's piece or one of its variants.
    pub fn piece_of(&self, i: &Instance) -> &str {
        i.piece(self.pieces[i.rule as usize].as_str())
    }
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
                .map(move |i| (*c, t.piece_of(i), i))
        })
    }
}

/// `hexWorld.state().scatter` as JSON: per tile, the planned, placed and drawn count of each piece.
#[derive(Resource, Clone, Debug, Default)]
pub struct ScatterReport(pub String);

/// LOD0 triangles of a loaded kit piece with their slot names (`build_piece`'s slot walk).
pub(crate) fn kit_tris(
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
            let piece = inst.piece(def.props.scatter[inst.rule as usize].piece.as_str());
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
        for p in &t.pieces {
            drawn_by_piece.entry(p.as_str()).or_default();
        }
        for i in l.instances.iter().filter(|i| t.drawn[i.rule as usize]) {
            *drawn_by_piece.entry(t.piece_of(i)).or_default() += 1;
        }
        placed_total += l.instances.len();
        rows.push(format!(
            "{{\"q\":{},\"r\":{},\"tile\":\"{}\",\"layout\":\"{:016x}\",\"planned\":{},\"placed\":{},\"drawn\":{},\"area_m2\":{}}}",
            c.q,
            c.r,
            t.tile,
            l.fingerprint(),
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
        let Some(prepared) =
            state.ground(def, &catalog.0, kit, &mut |id| kit_tris(id, &packs, assets))
        else {
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
            .map(|r| group(r.piece.as_str()).all(|p| kit.contains_key(p)))
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
            drift: false,
            vary: 0.0,
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_instances() {
        let c = catalog();
        for id in ["forest", "oak", "meadow"] {
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
                        let fp = footprint(inst.piece(piece));
                        let floors = floor_slots(inst.piece(piece));
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
        assert_eq!(counts.len(), 22, "every inventory tile ran");
        for id in [
            "forest",
            "oak",
            "windmill",
            "farm",
            "wheat_field",
            "meadow",
            "road_straight",
            "house_cluster",
        ] {
            assert!(counts[id] > 0, "{id} scatters: {counts:?}");
        }
        for id in [
            "ocean",
            "river_straight",
            "river_bend",
            "river_spring",
            "beach",
            "beach_inner",
            "beach_outer",
            "river_mouth",
            "river_mouth_l",
        ] {
            assert_eq!(counts[id], 0, "{id} has no scatter floor: {counts:?}");
        }
    }

    /// Density is per 100 m² of the hex cap: the mean count over cells matches it within 3%, every
    /// tile places its whole planned count, and same-piece neighbours keep the rule's spacing: the
    /// blue-noise spacing over its ground (a drift's share of it), never under two ground reaches
    /// ([`DRIFT_PACK`] in a drift).
    #[test]
    fn density_is_per_100_m2_of_cap_and_spacing_is_blue_noise() {
        let c = catalog();
        for id in ["forest", "oak", "meadow"] {
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
                    let fp = group_footprint(r.piece.as_str());
                    let pts: Vec<Vec2> = l
                        .instances
                        .iter()
                        .filter(|i| i.rule as usize == ri)
                        .map(|i| i.pos.xz())
                        .collect();
                    let share = if r.drift { DRIFT_SHARE } else { 1.0 };
                    let even = BLUE_NOISE * (share * l.area[ri] / n.max(1) as f32).sqrt();
                    let pack = if r.drift { DRIFT_PACK } else { 2.0 };
                    let want = even.max(pack * fp.ground).max(CROWN_SPACING * fp.crown);
                    assert!(
                        (l.spacing[ri] - want).abs() < 0.05,
                        "{id}: spacing {} vs {want}",
                        l.spacing[ri]
                    );
                    let spacing = l.spacing[ri];
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
        // A lattice spreads ~0%; dense Poisson-disc packing (20 spruces on the litter) still spreads
        // several per cent.
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

    /// PRD §6 Variety: a rule's instances draw every piece of its group over a few tiles, the same
    /// seed draws the same mix, and every variant is a known piece of its family with a footprint.
    #[test]
    fn instances_draw_every_variant_of_their_rule() {
        let c = catalog();
        for (id, rule) in [("forest", 0), ("forest", 1), ("meadow", 0), ("meadow", 2)] {
            let def = c.get(id).unwrap();
            let g = prepared(def).unwrap();
            let piece = def.props.scatter[rule].piece.as_str();
            let mut seen = std::collections::BTreeSet::new();
            for seed in 0..12u64 {
                let l = layout(def, seed, &g);
                assert_eq!(l, layout(def, seed, &g), "{id}: same seed");
                for i in l.instances.iter().filter(|i| i.rule as usize == rule) {
                    seen.insert(i.piece(piece).to_owned());
                }
            }
            let want: std::collections::BTreeSet<String> =
                group(piece).map(str::to_owned).collect();
            assert_eq!(seen, want, "{id} {piece}");
        }
        for (id, variants) in VARIANTS {
            for v in variants {
                assert_eq!(family(v), id);
                assert!(
                    FOOTPRINTS.iter().any(|(f, _)| f == v),
                    "{v} has no footprint"
                );
                assert_eq!(floor_slots(v), floor_slots(id));
            }
        }
    }

    /// A `vary` rule (the meadow's single wildflowers) stands where its seeded noise is high more
    /// often than its ground does on average, so the field's density varies gently, never in a grid.
    #[test]
    fn single_flowers_thin_by_their_seeded_noise() {
        let c = catalog();
        let def = c.get("meadow").unwrap();
        let (ri, r) = def
            .props
            .scatter
            .iter()
            .enumerate()
            .find(|(_, r)| r.piece.as_str() == "wildflower_single")
            .unwrap();
        assert!(r.vary > 0.0 && !r.drift);
        let g = prepared(def).unwrap();
        let (mut at, mut ground) = (0.0, 0.0);
        for seed in 0..16u64 {
            let l = layout(def, seed, &g);
            let pts: Vec<Vec2> = l
                .instances
                .iter()
                .filter(|i| i.rule as usize == ri)
                .map(|i| i.pos.xz())
                .collect();
            assert_eq!(pts.len() as u32, l.planned[ri]);
            at += pts.iter().map(|p| vary_noise(seed, *p)).sum::<f32>() / pts.len() as f32;
            let cells = &g.masks[ri].cells;
            ground += cells
                .iter()
                .map(|&k| {
                    let k = k as usize;
                    vary_noise(
                        seed,
                        Vec2::new(cell_centre(k % CELLS), cell_centre(k / CELLS)),
                    )
                })
                .sum::<f32>()
                / cells.len() as f32;
        }
        let (at, ground) = (at / 16.0, ground / 16.0);
        assert!(
            at > ground + 0.03,
            "noise at flowers {at} vs ground {ground}"
        );
    }

    /// The footprint table matches the kit: ground reach is the collider's, crown reach the widest
    /// LOD's (within 0.1 m, never under).
    /// The wheat field's ground (scatter, grass and the walker all stand on it) is its ploughed
    /// earth inside the field and the meadow cap on the grass margin; the wheat cards are never
    /// ground (`NOT_GROUND`), and the meadow cap never pokes through the earth.
    #[test]
    fn the_wheat_field_ground_is_its_earth_not_its_wheat() {
        let def = catalog().get("wheat_field").unwrap().clone();
        let g = prepared(&def).expect("wheat_field ground in the kit");
        let field = |p: Vec2| (0..3).map(|e| p.dot(hex_dir(e)).abs()).fold(0.0, f32::max);
        let (mut earth, mut cap) = (0, 0);
        for j in -60..=60 {
            for i in -60..=60 {
                let p = Vec2::new(i as f32 * 0.25, j as f32 * 0.25);
                let reach = field(p);
                let Some((_, slot)) = g.surface.at(p.x, p.y) else {
                    continue;
                };
                assert_ne!(slot, "wheat_field_wheat", "wheat is ground at {p}");
                if reach < 10.3 {
                    assert_eq!(slot, "wheat_field_soil", "at {p}");
                    earth += 1;
                } else if reach > 10.9 && reach < 13.5 {
                    assert_eq!(slot, "meadow_base_cap", "at {p}");
                    cap += 1;
                }
            }
        }
        assert!(earth > 4000 && cap > 1500, "{earth} {cap}");
        // LOD1 and LOD2 draw from 150 m and 400 m, base and centre switching together (one
        // origin): the meadow cap never pokes through the field's earth there either.
        for lod in ["lod1", "lod2"] {
            let mut all = vec![];
            for (id, k) in ground_pieces(&def, &catalog()) {
                for (t, s) in glb_mesh_tris(id, lod).unwrap() {
                    all.push((t.map(|p| turn(k, p)), s));
                }
            }
            let s = Surface::new(all);
            for j in -40..=40 {
                for i in -40..=40 {
                    let p = Vec2::new(i as f32 * 0.25, j as f32 * 0.25);
                    if field(p) < 10.3 {
                        let (_, slot) = s.at(p.x, p.y).unwrap();
                        assert_eq!(slot, "wheat_field_soil", "{lod} at {p}");
                    }
                }
            }
        }
        // The wildflowers and rocks stand on the margin only.
        let l = layout(&def, 1, &g);
        assert!(!l.instances.is_empty());
        for inst in &l.instances {
            assert!(field(inst.pos.xz()) > 10.8, "{:?}", inst.pos);
        }
    }

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

    /// Each [`GROUND_BOXES`] entry is its collider's bounds (to 1 cm) and the collider fills
    /// them (a rectangle with at most chamfered corners), so a box clears no more than the piece.
    #[test]
    fn ground_boxes_match_the_kit() {
        for (id, b) in GROUND_BOXES {
            let pts: Vec<Vec2> = glb_mesh_tris(id, "collider")
                .unwrap()
                .iter()
                .flat_map(|(t, _)| t.iter().map(|p| Vec2::new(p[0], p[2])))
                .collect();
            let lo = pts.iter().fold(Vec2::MAX, |a, p| a.min(*p));
            let hi = pts.iter().fold(Vec2::MIN, |a, p| a.max(*p));
            assert!(
                lo.distance(Vec2::from(b.min)) < 0.01 && hi.distance(Vec2::from(b.max)) < 0.01,
                "{id} collider bounds {lo}..{hi}"
            );
            let h = crate::walk::hull(&pts);
            let area = (0..h.len())
                .map(|i| h[i].perp_dot(h[(i + 1) % h.len()]))
                .sum::<f32>()
                / 2.0;
            let size = hi - lo;
            assert!(
                area >= 0.95 * size.x * size.y,
                "{id} fills {area} of its {size} box"
            );
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

    /// Spruces scattered round a hero oak leave its canopy clear, so its perches stay open. No
    /// shipped tile pairs them since the split (U3), so the pine forest is given the oak tile's oak.
    #[test]
    fn spruces_clear_the_oak_canopy() {
        let c = catalog();
        let oak = c.get("oak").unwrap();
        assert!(oak.agents.iter().any(|a| a.agent == AgentKind::Bird));
        let mut def = c.get("forest").unwrap().clone();
        def.props.hero = oak.props.hero.clone();
        let g = prepared(&def).unwrap();
        for seed in 0..16 {
            for i in layout(&def, seed, &g)
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

    /// The split (U3): the pine forest is spruces and rocks with no hero and no bird roost; the oak
    /// tile is the hero oak, rocks and a few flowers on forest_base's litter, and the roost. Both
    /// have Forest on every edge.
    #[test]
    fn pine_and_oak_tiles_split_the_forest() {
        let c = catalog();
        let pine = c.get("forest").unwrap();
        let oak = c.get("oak").unwrap();
        for d in [pine, oak] {
            assert_eq!(d.base.as_str(), "forest_base", "{}", d.id);
            assert!(
                d.edges.iter().all(|e| e.kind == SocketKind::Forest),
                "{}",
                d.id
            );
        }
        let pieces = |d: &TileDef| -> Vec<String> {
            d.props
                .scatter
                .iter()
                .map(|r| r.piece.as_str().to_owned())
                .collect()
        };
        assert_eq!(
            pieces(pine),
            ["tree_forest", "rock_small", "fern_clump", "log_fallen"]
        );
        assert!(pine.props.hero.is_empty() && pine.agents.is_empty());
        assert_eq!(pieces(oak), ["rock_small", "wildflower_clump", "tree_oak"]);
        assert_eq!(oak.props.hero.len(), 1);
        assert_eq!(oak.props.hero[0].piece.as_str(), "tree_hero_oak");
        let g = prepared(oak).unwrap();
        let mut flowers = 0;
        for seed in 0..32 {
            let l = layout(oak, seed, &g);
            assert_eq!(l.placed(0), l.planned[0] as usize, "rocks");
            assert_eq!(l.placed(1), l.planned[1] as usize, "flowers");
            flowers += l.placed(1);
            for i in l.instances.iter().filter(|i| i.rule == 1) {
                let (_, slot) = g.surface.at(i.pos.x, i.pos.z).unwrap();
                assert_eq!(slot, "forest_base_floor", "a flower on the litter");
            }
        }
        let mean = flowers as f32 / 32.0;
        assert!((4.0..=8.0).contains(&mean), "a few flowers: {mean}");
    }

    /// Drift rules cluster: over 4 m quadrats of the ground, the meadow's flowers have an index of
    /// dispersion (variance over mean count) well above 1, where the same rule spread evenly stays
    /// below it. The drifts follow the tile's seed, not a fixed pattern.
    #[test]
    fn meadow_flowers_grow_in_clustered_drifts() {
        let c = catalog();
        let meadow = c.get("meadow").unwrap();
        assert!(meadow.props.scatter[0].drift, "the meadow's flowers drift");
        assert!(!meadow.props.scatter[1].drift, "its rocks do not");
        let mut even = meadow.clone();
        even.props.scatter[0].drift = false;
        let g = prepared(meadow).unwrap();
        let quad = |x: f32, z: f32| ((x / 4.0).floor() as i32, (z / 4.0).floor() as i32);
        let dispersion = |def: &TileDef, seed: u64| {
            let l = layout(def, seed, &g);
            let mut quads: BTreeMap<(i32, i32), u32> = BTreeMap::new();
            for &k in &g.masks[0].cells {
                let k = k as usize;
                quads
                    .entry(quad(cell_centre(k % CELLS), cell_centre(k / CELLS)))
                    .or_default();
            }
            for i in l.instances.iter().filter(|i| i.rule == 0) {
                *quads.entry(quad(i.pos.x, i.pos.z)).or_default() += 1;
            }
            let n = quads.len() as f32;
            let mean = quads.values().map(|&v| v as f32).sum::<f32>() / n;
            let var = quads
                .values()
                .map(|&v| (v as f32 - mean).powi(2))
                .sum::<f32>()
                / n;
            var / mean
        };
        let seeds: Vec<u64> = (0..16)
            .map(|k| cell_seed(5, Axial::new(k, 1 - k)))
            .collect();
        let drift = seeds.iter().map(|&s| dispersion(meadow, s)).sum::<f32>() / 16.0;
        let spread = seeds.iter().map(|&s| dispersion(&even, s)).sum::<f32>() / 16.0;
        assert!(
            drift > 1.6,
            "drifts cluster: dispersion {drift} (even {spread})"
        );
        assert!(spread < 1.0, "an even spread does not: dispersion {spread}");
        let (a, b) = (seeds[0], seeds[1]);
        let p = Vec2::new(3.0, -4.0);
        assert_ne!(drift_noise(a, p), drift_noise(b, p), "seeded per tile");
        for k in 0..200 {
            let q = Vec2::new(k as f32 * 0.37 - 30.0, 17.0 - k as f32 * 0.21);
            assert!((0.0..1.0).contains(&drift_noise(a, q)), "{q}");
        }
    }
}
