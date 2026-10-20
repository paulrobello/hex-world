//! Grass blades on grass-covered ground near the camera (owner request 2026-10-14: a grass shader
//! for the high detail LOD), High and Ultra tiers only, at the [`Density`] of the `grass` key,
//! `set_grass` and the Settings → Graphics → Grass density slider (0-150% of [`DENSITY`]).
//!
//! - **Ground.** Blades grow on the topmost LOD0 surface of a tile's ground pieces (scatter's
//!   [`Surface`], from [`PlayScatter::ground`]) where its slot is a grass slot: a manifest slot whose
//!   tileable set is `grass` and that is not foliage ([`grass_slots`]: `meadow_base_cap`,
//!   `forest_base_rim` and `_edge`, `road_verge`, `river_grass` and `beach_grass`; the hedge is
//!   foliage). Scatter's own slot filter ([`scatter::excluded_slot`]) is not used: it drops every
//!   `road_`, `river_` and `beach_` slot and every rim and edge band, which are exactly the verges,
//!   banks and dune grass that should carry blades. Roads, sand, river beds and pads are not grass
//!   slots, so they never do.
//! - **Exclusions.** Scatter's footprints: no blade within a hero prop's ground footprint (a
//!   rectangular building's collider box turned by its yaw plus [`WALL_GAP`], so blades grow
//!   along its long walls; a disc for the windmill's tower, the oak's trunk, bales and animals;
//!   a disc over each post of a fence, [`scatter::FENCES`], so blades grow under its rails) or
//!   a scatter instance's (a box over a rock's base outline, [`ROCK_BASES`], so blades come right
//!   up to the stone; spruce trunks; every planned instance of the
//!   tile's layout, so a late pack never moves the grass; flower clumps keep only their centre
//!   clear, [`GROWS_THROUGH`], so grass grows in among them), nor below the water level
//!   ([`WATER_MARGIN`]). The hex border is not an exclusion (scatter's border gap keeps crowns and
//!   footprints inside the hex, which would thin grass into a seam): the border has no width, each
//!   tile owns the blades on its side of it, and its jittered grid keeps the density up to the edge,
//!   so two grass tiles read as one field.
//! - **Placement.** A jittered grid at [`LUSH`] times [`DENSITY`] blades per m² of grass ground
//!   (thinned per slot by its [`Style`]), seeded from `cell_seed(world seed, cell)` with its own
//!   salt, so a tile always grows the same blades and scatter's stream is untouched. Every tile
//!   seeds this 150% set once; a density draws the blades whose `keep` is under its share of it, so
//!   every lower value is a subset of every higher one and a slider move only changes how many
//!   instances draw, never the scene. Each blade has a random scale ([`SCALE`]), yaw, lean and tilt,
//!   and a hue and lightness [`jitter`]. Arithmetic and `sqrt` only, so native and wasm agree.
//! - **Drawing.** GPU instancing ([`super::grass_gpu`]): one shared blade mesh ([`BLADE_CORNERS`],
//!   two crossed tapered planes, so a blade holds a silhouette from any side) and, per tile, one
//!   instance buffer of 32-byte [`GpuBlade`] records holding the tile's whole Lush set ordered by
//!   `keep`, uploaded once. A density draws a prefix of that buffer ([`GrassInstances::drawn`]), so
//!   a slider move changes only the instance count: no mesh is rebuilt and nothing is re-uploaded.
//!   The tile's [`GrassGroup`] is a child of the tile root: it rises and sinks with the tile and goes
//!   with it on bulldoze, undo, replace and Start over, and the ghost never draws it.
//!   `grass_common.wesl` poses each blade in the vertex stage (a world-space clump noise on the
//!   height, the shared wind of `wind.wesl`, bent by the square of the height along the blade) and
//!   shrinks it to nothing by [`FADE_END`] m from the camera, after thinning it by rank from
//!   [`THIN_START`]. Only tiles within [`BUILD_RADIUS`] of the camera hold a group, at most
//!   [`BUILDS_PER_FRAME`] new ones a frame within [`BUILD_BUDGET_MS`], nearest first; beyond
//!   [`DROP_RADIUS`] they are dropped. Those are the Near distances: the [`GrassDistance`]
//!   (`grass_distance`, `set_grass_distance`, Settings → Graphics → Grass distance) multiplies all
//!   six ([`Distances`]), so a farther stop builds the newly reached tiles under the same per-frame
//!   budget and a nearer one drops the tiles past its drop radius.
//! - **Pipelines.** Group 2 binds two uniforms (the wind clock and tuning, and per tile its
//!   transform now and one frame ago) and no texture: the pipelines sample only the view's
//!   textures, fewer than the kit forward pipeline. Blades write the depth, normal and motion
//!   prepass (tilt-shift, refraction, SSAO and TAA see them), receive shadows and cast none.
//! - **Colour.** A blade's base colour is its slot's tint times the grass set's mean linear albedo
//!   ([`GRASS_ALBEDO_MEAN`]) with its jitter, so blades blend into the ground at the root; the
//!   shader mixes toward a lighter, less saturated tip and takes the §9.4 season tint from the
//!   group's `MeshTag`.
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use bevy::{
    asset::RenderAssetUsages,
    camera::primitives::Aabb,
    gltf::{Gltf, GltfMesh},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::{render_resource::ShaderType, sync_world::SyncToRenderWorld},
};
use bytemuck::{Pod, Zeroable};
use hex_assets::KitManifest;
use hex_core::{Axial, HEX_RADIUS, TileCatalog, TileDef, cell_seed};

use super::{
    APOTHEM, PlayCatalog, PlayKit, PlaySpawned, PlayView,
    packs::PlayPacks,
    scatter::{self, Instance, PlayScatter, Rng, ScatterLayouts, Surface},
};
use crate::{ActiveScenario, Scenario, SceneEntity, post::QualityTier, workload::GateConfig};

/// Mean linear albedo of the `grass` tileable set (`p1/materials/grass/grass.albedo.png`, sRGB
/// decoded per texel, as the GPU samples it). `albedo_mean_matches_the_png` holds it to the file.
pub const GRASS_ALBEDO_MEAN: [f32; 3] = [0.0993, 0.2135, 0.0310];
/// Blades per m² of grass ground at [`Density::NORMAL`], before a slot's own thinning (owner
/// 2026-10-14: doubled from 14).
pub const DENSITY: f32 = 28.0;
/// Camera distance (m) where blades start to thin by rank, and where only [`THIN_KEEP`] are left,
/// at [`GrassDistance::NEAR`]: every grass distance here is the Near value, scaled at runtime by
/// the `grass_distance` key ([`Distances`]).
pub const THIN_START: f32 = 16.0;
pub const THIN_END: f32 = 46.0;
pub const THIN_KEEP: f32 = 0.35;
/// Camera distance (m) where the kept blades start to shrink, and where they are gone.
pub const FADE_START: f32 = 34.0;
pub const FADE_END: f32 = 58.0;
/// Tiles whose nearest point is within this of the camera hold a mesh; beyond [`DROP_RADIUS`]
/// they drop it.
pub const BUILD_RADIUS: f32 = FADE_END + 2.0;
pub const DROP_RADIUS: f32 = BUILD_RADIUS + 12.0;
/// New tile groups per frame, nearest first, and the frame's build time (ms) after which no further
/// one starts (the first always does), so walking into new tiles never hitches a frame.
pub const BUILDS_PER_FRAME: usize = 3;
pub const BUILD_BUDGET_MS: f32 = 4.0;
/// Scale on the shared wind's push for grass.
pub const WIND_SCALE: f32 = 1.0;
/// Blades keep this far (m) above the water level (`water::tiles::WATER_Y`).
pub const WATER_MARGIN: f32 = 0.3;
/// Gap (m) between a blade's root and a prop's ground footprint.
pub const PROP_GAP: f32 = 0.05;
/// Gap (m) between a blade's root and a rectangular building's wall ([`Clear::Box`]): a blade
/// is up to 0.11 m wide at the root and leans, so it keeps clear of the wall's face.
pub const WALL_GAP: f32 = 0.15;
/// Soft scatter pieces grass grows in among (owner 2026-10-14: wildflowers had a bare ring around
/// them), with the radius (m, before the instance's scale) of the dense centre it keeps clear.
/// Single-stem wildflowers keep no centre: grass grows right up to the stem.
pub const GROWS_THROUGH: &[(&str, f32)] = &[
    ("wildflower_clump", 0.2),
    ("wildflower_clump_b", 0.2),
    ("wildflower_clump_c", 0.2),
    ("wildflower_single", 0.0),
    ("wildflower_single_b", 0.0),
    ("wildflower_single_c", 0.0),
    ("wildflower_single_d", 0.0),
];
/// A rock's base outline where it meets the ground (owner 2026-10-19: grass stopped short of the
/// stone, held off by its spacing footprint): a convex 12-gon, counter-clockwise in the piece's
/// own (x, z) before its yaw and scale, around the y = 0 slice of its LOD0 and collider.
pub type RockBase = [[f32; 2]; 12];

/// `rock_bases_match_the_kit` checks these against the GLBs (to 2 cm).
pub const ROCK_BASES: [(&str, RockBase); 3] = [
    (
        "rock_small",
        [
            [-0.487, 0.151],
            [-0.465, -0.051],
            [-0.413, -0.188],
            [-0.333, -0.291],
            [-0.127, -0.469],
            [0.282, -0.359],
            [0.341, -0.31],
            [0.529, 0.058],
            [0.378, 0.308],
            [0.263, 0.429],
            [-0.33, 0.351],
            [-0.449, 0.237],
        ],
    ),
    (
        "rock_small_b",
        [
            [-0.959, 0.139],
            [-0.77, -0.237],
            [-0.543, -0.533],
            [0.016, -0.79],
            [0.16, -0.751],
            [0.666, -0.419],
            [0.846, -0.137],
            [0.605, 0.42],
            [0.405, 0.731],
            [0.269, 0.76],
            [0.095, 0.743],
            [-0.894, 0.344],
        ],
    ),
    (
        "rock_small_c",
        [
            [-0.86, -0.244],
            [-0.84, -0.443],
            [-0.574, -0.538],
            [-0.14, -0.445],
            [0.249, -0.328],
            [0.461, -0.143],
            [0.563, 0.208],
            [0.63, 0.597],
            [0.569, 0.775],
            [-0.315, 0.7],
            [-0.392, 0.671],
            [-0.829, -0.138],
        ],
    ),
];
const SALT: u64 = 0x6A55_B1AD_E5C0_FFEE;
/// The shared blade mesh: two crossed planes (an X from above, so the blade holds a silhouette from
/// any side), each two corners at the root, two at mid height (the 1 - t² taper) and one at the tip,
/// as (offset across in widths, height fraction along, plane). `grass_common` turns the second
/// plane 90° about the root.
pub const BLADE_CORNERS: [[f32; 3]; 10] = [
    [-0.5, 0.0, 0.0],
    [0.5, 0.0, 0.0],
    [-0.375, 0.5, 0.0],
    [0.375, 0.5, 0.0],
    [0.0, 1.0, 0.0],
    [-0.5, 0.0, 1.0],
    [0.5, 0.0, 1.0],
    [-0.375, 0.5, 1.0],
    [0.375, 0.5, 1.0],
    [0.0, 1.0, 1.0],
];
pub const BLADE_INDICES: [u16; 18] = [0, 1, 2, 2, 1, 3, 2, 3, 4, 5, 6, 7, 7, 6, 8, 7, 8, 9];
/// Triangles per blade.
pub const BLADE_TRIANGLES: usize = BLADE_INDICES.len() / 3;
/// GPU bytes of the shared blade mesh: 12-byte corners and 16-bit indices.
pub const BLADE_MESH_BYTES: usize = BLADE_CORNERS.len() * 12 + BLADE_INDICES.len() * 2;
/// Per-blade random scale on height and width, and per-blade hue (turns) and lightness jitter.
const SCALE: [f32; 2] = [0.7, 1.4];
const HUE_JITTER: f32 = 0.04;
const LIGHT_JITTER: f32 = 0.05;

pub use hex_play_state::{Density, GrassDistance, LUSH};

/// The grass distances (m) in use at a [`GrassDistance`]: the Near constants times its multiplier.
/// The shader reads the thinning and fade from [`GrassParams`], `sync` the build and drop radii, so
/// both always draw with the same values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Distances {
    pub thin_start: f32,
    pub thin_end: f32,
    pub fade_start: f32,
    pub fade_end: f32,
    pub build_radius: f32,
    pub drop_radius: f32,
}

impl Distances {
    pub fn new(distance: GrassDistance) -> Self {
        let s = distance.scale();
        Self {
            thin_start: THIN_START * s,
            thin_end: THIN_END * s,
            fade_start: FADE_START * s,
            fade_end: FADE_END * s,
            build_radius: BUILD_RADIUS * s,
            drop_radius: DROP_RADIUS * s,
        }
    }
}

/// How blades grow on one kind of grass slot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    /// Share of [`DENSITY`] kept.
    pub density: f32,
    /// Blade height range (m), before the per-blade [`SCALE`] and the shader's clump noise.
    pub height: [f32; 2],
    /// Blade width at the root (m), before the per-blade [`SCALE`]: about 0.3 of the height, so the
    /// crossed pair reads chunky (owner 2026-10-14, after small-world's grass).
    pub width: [f32; 2],
}

/// Mown verges, dune grass, bank grass, the forest margin and the meadow.
pub fn style(slot: &str) -> Style {
    match slot {
        "road_verge" => Style {
            density: 0.9,
            height: [0.1, 0.18],
            width: [0.035, 0.055],
        },
        "beach_grass" => Style {
            density: 0.55,
            height: [0.26, 0.5],
            width: [0.045, 0.075],
        },
        "river_grass" => Style {
            density: 0.85,
            height: [0.2, 0.38],
            width: [0.06, 0.1],
        },
        s if s.starts_with("forest_base") => Style {
            density: 0.75,
            height: [0.15, 0.3],
            width: [0.05, 0.085],
        },
        _ => Style {
            density: 1.0,
            height: [0.2, 0.36],
            width: [0.07, 0.11],
        },
    }
}

/// Shifts a linear colour's hue (turns) and lightness in a gamma-2 space close to sRGB, with
/// arithmetic and `sqrt` only so native and wasm agree.
pub fn jitter(c: [f32; 3], dh: f32, dl: f32) -> [f32; 3] {
    let [r, g, b] = c.map(|x| x.max(0.0).sqrt());
    let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
    let l = 0.5 * (hi + lo);
    let d = hi - lo;
    let (h, s) = if d <= 1e-6 {
        (0.0, 0.0)
    } else {
        let s = (d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6)).min(1.0);
        let h = if hi == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if hi == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        (h / 6.0, s)
    };
    let h = (h + dh).rem_euclid(1.0) * 6.0;
    let l = (l + dl).clamp(0.0, 1.0);
    let chroma = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = chroma * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = l - 0.5 * chroma;
    [r + m, g + m, b + m].map(|v| v * v)
}

/// The grass slots of a tile's ground pieces with their linear tint.
pub fn grass_slots(
    manifest: &KitManifest,
    def: &TileDef,
    catalog: &TileCatalog,
) -> HashMap<String, [f32; 3]> {
    let mut out = HashMap::new();
    for (id, _) in scatter::ground_pieces(def, catalog) {
        let Some(entry) = manifest.pieces.iter().find(|e| e.id == id) else {
            continue;
        };
        for (name, slot) in &entry.slots {
            if slot.tileable.as_deref() == Some("grass") && !slot.foliage {
                out.insert(name.clone(), slot.color.unwrap_or([1.0, 1.0, 1.0]));
            }
        }
    }
    out
}

/// One blade, tile-local.
#[derive(Clone, Debug, PartialEq)]
pub struct Blade {
    pub root: Vec3,
    /// Facing in whole degrees.
    pub yaw_deg: u16,
    /// Thinning rank in [0, 0.9): far away the highest ranks go first.
    pub rank: f32,
    pub height: f32,
    pub width: f32,
    /// Linear base colour.
    pub color: [f32; 3],
    /// In [0, 1): the blade draws at a density whose share of Lush is above it ([`Density::draws`]).
    pub keep: f32,
}

/// A tile's blades and the grass ground they cover.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GrassLayout {
    pub blades: Vec<Blade>,
    pub area_m2: f32,
}

impl GrassLayout {
    /// FNV-1a over every blade's bits: equal layouts, equal value.
    pub fn fingerprint(&self) -> u64 {
        let mut h: u64 = 0xCBF2_9CE4_8422_2325;
        for b in &self.blades {
            let words = [
                b.root.x.to_bits(),
                b.root.y.to_bits(),
                b.root.z.to_bits(),
                u32::from(b.yaw_deg),
                b.rank.to_bits(),
                b.height.to_bits(),
                b.width.to_bits(),
                b.keep.to_bits(),
            ];
            for x in words.iter().flat_map(|w| w.to_le_bytes()) {
                h = (h ^ u64::from(x)).wrapping_mul(0x0100_0000_01B3);
            }
        }
        h
    }
}

/// Tile-local ground (x, z) no blade may root in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Clear {
    Disc {
        centre: Vec2,
        radius: f32,
    },
    /// A rectangle: its centre, its half extents along its own x and z, and its own +x axis (a
    /// unit vector; its +z is `axis.perp()`).
    Box {
        centre: Vec2,
        half: Vec2,
        axis: Vec2,
    },
    /// A rock's base ([`ROCK_BASES`]) at `centre`, its own +x along `axis`, scaled by `scale`,
    /// grown outward by [`PROP_GAP`].
    Rock {
        centre: Vec2,
        axis: Vec2,
        scale: f32,
        outline: &'static RockBase,
    },
}

impl Clear {
    pub fn contains(&self, p: Vec2) -> bool {
        match *self {
            Clear::Disc { centre, radius } => p.distance_squared(centre) < radius * radius,
            Clear::Box { centre, half, axis } => {
                let d = p - centre;
                d.dot(axis).abs() < half.x && d.dot(axis.perp()).abs() < half.y
            }
            Clear::Rock {
                centre,
                axis,
                scale,
                outline,
            } => {
                let d = p - centre;
                let q = Vec2::new(d.dot(axis), d.dot(axis.perp())) / scale;
                let gap = PROP_GAP / scale;
                (0..outline.len()).all(|k| {
                    let a = Vec2::from(outline[k]);
                    let e = Vec2::from(outline[(k + 1) % outline.len()]) - a;
                    e.perp_dot(q - a) > -gap * e.length()
                })
            }
        }
    }
}

/// `(cos, sin)` of `deg` degrees from `+`, `*`, `/` and `round` alone: `sin`/`cos` may differ by
/// an ulp between native libm and wasm's, and a tile's blades must be the same on both.
pub fn cos_sin_deg(deg: f32) -> Vec2 {
    let d = deg.rem_euclid(360.0);
    let q = (d / 90.0).round();
    let x = (d - 90.0 * q) * (std::f32::consts::PI / 180.0);
    let x2 = x * x;
    let s = x * (1.0 - x2 / 6.0 * (1.0 - x2 / 20.0 * (1.0 - x2 / 42.0 * (1.0 - x2 / 72.0))));
    let c = 1.0 - x2 / 2.0 * (1.0 - x2 / 12.0 * (1.0 - x2 / 30.0 * (1.0 - x2 / 56.0)));
    match q as u8 % 4 {
        0 => Vec2::new(c, s),
        1 => Vec2::new(-s, c),
        2 => Vec2::new(-c, -s),
        _ => Vec2::new(s, -c),
    }
}

/// Where no blade may root: hero props (a box over a rectangular building's collider,
/// [`scatter::GROUND_BOXES`], else a disc of its ground reach) and every scatter instance (a box
/// over a rock's base, [`ROCK_BASES`], else a disc). Each hero is the piece its slot draws at
/// `cell` (`hex_core::hero_piece`).
pub fn exclusions(
    def: &TileDef,
    cell: Axial,
    instances: &[Instance],
    pieces: &[String],
) -> Vec<Clear> {
    let mut out: Vec<Clear> = def
        .hero_pieces(cell)
        .filter(|(_, piece)| scatter::fence(piece).is_none())
        .map(|(h, piece)| {
            let at = Vec2::from(h.at);
            let Some(b) = scatter::ground_box(piece) else {
                return Clear::Disc {
                    centre: at,
                    radius: scatter::footprint(piece).ground + PROP_GAP,
                };
            };
            // The hero's turn as walk and the kit scene place it: `Quat::from_rotation_y(-yaw)`
            // takes the piece's +x to (cos, sin) in (x, z).
            let axis = cos_sin_deg(h.yaw_deg);
            let (min, max) = (Vec2::from(b.min), Vec2::from(b.max));
            let mid = (min + max) / 2.0;
            Clear::Box {
                centre: at + axis * mid.x + axis.perp() * mid.y,
                half: (max - min) / 2.0 + WALL_GAP,
                axis,
            }
        })
        .collect();
    // A hero fence clears only its posts (a disc over each square post's corners), so blades grow
    // under the rails and across the yard it encloses.
    for (h, piece) in def.hero_pieces(cell) {
        let Some(f) = scatter::fence(piece) else {
            continue;
        };
        let axis = cos_sin_deg(h.yaw_deg);
        for (at, half) in f.posts {
            out.push(Clear::Disc {
                centre: Vec2::from(h.at) + axis * at[0] + axis.perp() * at[1],
                radius: half * std::f32::consts::SQRT_2 + PROP_GAP,
            });
        }
    }
    out.extend(instances.iter().filter_map(|i| {
        let piece = i.piece(pieces.get(i.rule as usize)?);
        if let Some((_, outline)) = ROCK_BASES.iter().find(|(id, _)| *id == piece) {
            // `Instance::rotation` (`Quat::from_rotation_y(yaw)`) takes +x to (cos, -sin) in (x, z).
            return Some(Clear::Rock {
                centre: i.pos.xz(),
                axis: cos_sin_deg(-i.yaw.to_degrees()),
                scale: i.scale,
                outline,
            });
        }
        let radius = GROWS_THROUGH
            .iter()
            .find(|(id, _)| *id == piece)
            .map_or_else(|| scatter::footprint(piece).ground, |(_, core)| *core);
        Some(Clear::Disc {
            centre: i.pos.xz(),
            radius: radius * i.scale + PROP_GAP,
        })
    }));
    out
}

fn lerp(r: [f32; 2], t: f32) -> f32 {
    r[0] + (r[1] - r[0]) * t
}

/// The Lush blades of a tile with seed `seed` on its ground.
pub fn layout(
    seed: u64,
    surface: &Surface,
    slots: &HashMap<String, [f32; 3]>,
    excluded: &[Clear],
) -> GrassLayout {
    let mut rng = Rng(seed ^ SALT);
    let step = 1.0 / (DENSITY * LUSH).sqrt();
    let n = (2.0 * HEX_RADIUS / step).ceil() as i32;
    let water = crate::water::tiles::WATER_Y + WATER_MARGIN;
    let mut out = GrassLayout::default();
    let mut ground = 0u32;
    for j in 0..n {
        for i in 0..n {
            // Every grid point draws the same numbers whether or not it grows a blade.
            let r = [(); 11].map(|_| rng.unit());
            let p = Vec2::new(
                -HEX_RADIUS + (i as f32 + r[0]) * step,
                -HEX_RADIUS + (j as f32 + r[1]) * step,
            );
            if scatter::hex_reach(p) >= APOTHEM || excluded.iter().any(|c| c.contains(p)) {
                continue;
            }
            let Some((y, slot)) = surface.at(p.x, p.y) else {
                continue;
            };
            let Some(tint) = slots.get(slot) else {
                continue;
            };
            if y < water {
                continue;
            }
            ground += 1;
            let st = style(slot);
            if r[2] >= st.density {
                continue;
            }
            let scale = lerp(SCALE, r[10]);
            let base = [0, 1, 2].map(|k| tint[k] * GRASS_ALBEDO_MEAN[k]);
            out.blades.push(Blade {
                root: Vec3::new(p.x, y, p.y),
                yaw_deg: ((r[3] * 360.0) as u16).min(359),
                rank: 0.9 * r[4],
                height: lerp(st.height, r[5]) * scale,
                width: lerp(st.width, r[8]) * scale,
                color: jitter(
                    base,
                    HUE_JITTER * (2.0 * r[6] - 1.0),
                    LIGHT_JITTER * (2.0 * r[7] - 1.0),
                ),
                keep: r[9],
            });
        }
    }
    out.area_m2 = ground as f32 * step * step;
    out
}

/// The shared blade mesh (render world only): [`BLADE_CORNERS`] as its positions.
pub fn blade_mesh() -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, BLADE_CORNERS.to_vec())
    .with_inserted_indices(Indices::U16(BLADE_INDICES.to_vec()))
}

/// One blade's instance record (32 bytes), tile-local.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct GpuBlade {
    pub root: [f32; 3],
    /// Yaw in whole degrees plus the thinning rank in [0, 0.9).
    pub yaw_rank: f32,
    /// Height and root width (m).
    pub size: [f32; 2],
    /// Linear base colour as unorm16 (each channel is under 1), alpha 0.
    pub color: [u16; 4],
}

/// GPU bytes per blade instance.
pub const BLADE_BYTES: usize = size_of::<GpuBlade>();

/// A tile's Lush set as GPU instances, ordered by `keep` (ties by generation order) so the blades
/// any density draws are a prefix, and their bounds with room for the bend.
#[derive(Debug)]
pub struct GrassInstances {
    pub blades: Vec<GpuBlade>,
    /// Each blade's `keep`, ascending.
    keeps: Vec<f32>,
    pub aabb: Aabb,
}

impl GrassInstances {
    /// `None` when the layout has no blade.
    pub fn new(layout: &GrassLayout) -> Option<Self> {
        if layout.blades.is_empty() {
            return None;
        }
        let mut order: Vec<&Blade> = layout.blades.iter().collect();
        order.sort_by(|a, b| a.keep.total_cmp(&b.keep));
        let unorm = |c: f32| (c.clamp(0.0, 1.0) * 65535.0).round() as u16;
        let blades = order
            .iter()
            .map(|b| GpuBlade {
                root: b.root.to_array(),
                yaw_rank: f32::from(b.yaw_deg) + b.rank,
                size: [b.height, b.width],
                color: [unorm(b.color[0]), unorm(b.color[1]), unorm(b.color[2]), 0],
            })
            .collect();
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        let mut tallest: f32 = 0.0;
        for b in &layout.blades {
            lo = lo.min(b.root);
            hi = hi.max(b.root);
            tallest = tallest.max(b.height);
        }
        // The pose bends a tip at most 1.1 heights sideways, the clump noise grows blades by up to
        // 1.45 and far blades widen by up to 2.5.
        let reach = 1.45 * tallest;
        let side = 1.2 * reach + 0.2;
        Some(Self {
            blades,
            keeps: order.iter().map(|b| b.keep).collect(),
            aabb: Aabb::from_min_max(
                lo - Vec3::new(side, 0.0, side),
                hi + Vec3::new(side, reach, side),
            ),
        })
    }

    /// How many blades `density` draws: the prefix whose `keep` it draws.
    pub fn drawn(&self, density: Density) -> u32 {
        self.keeps.partition_point(|&k| density.draws(k)) as u32
    }

    /// GPU bytes of the instance buffer.
    pub fn bytes(&self) -> usize {
        self.blades.len() * BLADE_BYTES
    }
}

#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct GrassParams {
    /// The wind clock (s) now and one frame ago: Bevy's `Time`, 1/60 s a frame under
    /// `--clock fixed`.
    pub time: f32,
    pub prev_time: f32,
    pub fade_start: f32,
    pub fade_end: f32,
    pub thin_start: f32,
    pub thin_end: f32,
    pub thin_keep: f32,
    pub wind_scale: f32,
}

impl GrassParams {
    fn new(d: Distances) -> Self {
        Self {
            time: 0.0,
            prev_time: 0.0,
            fade_start: d.fade_start,
            fade_end: d.fade_end,
            thin_start: d.thin_start,
            thin_end: d.thin_end,
            thin_keep: THIN_KEEP,
            wind_scale: WIND_SCALE,
        }
    }
}

/// The wind clock and tuning every group draws with this frame (extracted to the render world).
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct GrassClock(pub GrassParams);

/// The entity drawing one tile's blades, a child of the tile root: its instances (uploaded once)
/// and how many of them the density draws.
#[derive(Component, Clone)]
pub struct GrassGroup {
    pub instances: Arc<GrassInstances>,
    pub drawn: u32,
}

#[derive(Clone, Debug, PartialEq)]
struct Key {
    tile: String,
    seed: u64,
}

struct Built {
    key: Key,
    cell: Axial,
    /// The tile's Lush set, kept so a density change only changes the drawn count.
    layout: Arc<GrassLayout>,
    /// The group and its instances; `None` when the tile has no blade (no grass ground).
    group: Option<(Entity, Arc<GrassInstances>)>,
    /// Blades drawn at the current density.
    blades: usize,
}

/// The grass slots per tile id and the group drawn under each tile root.
#[derive(Resource, Default)]
pub struct PlayGrass {
    slots: HashMap<String, Arc<HashMap<String, [f32; 3]>>>,
    built: HashMap<Entity, Built>,
    /// The density the groups' drawn counts are set for.
    density: Option<Density>,
    /// The slowest single tile build (seeded set and instances, ms) in the latest frame that built
    /// one.
    last_build_ms: f32,
    /// The slowest single density change (a group's drawn count, ms) in the latest frame that
    /// changed one.
    last_swap_ms: f32,
}

/// `state().grass` as JSON.
#[derive(Resource, Clone, Debug, Default)]
pub struct GrassReport(pub String);

/// Why grass draws or not: the density, the tier, the scenario.
pub fn status(active: Scenario, config: &GateConfig) -> &'static str {
    if config.grass.is_off() {
        "off"
    } else if !matches!(config.tier, QualityTier::High | QualityTier::Ultra) {
        "tier"
    } else if active != Scenario::Play {
        "scenario"
    } else {
        "on"
    }
}

/// `state().grass`: the density (share of [`DENSITY`]), the distance multiplier, and why it draws or
/// not, live groups, drawn blades and triangles, the instances held on the GPU and their bytes with
/// the shared blade mesh, instance buffers uploaded since start, the tuning at that distance, and a
/// row per tile holding a group.
pub fn report(
    state: &PlayGrass,
    density: Density,
    distance: GrassDistance,
    status: &str,
    groups: usize,
) -> String {
    let d = Distances::new(distance);
    let build_ms = (state.last_build_ms * 100.0).round() / 100.0;
    let swap_ms = (state.last_swap_ms * 100.0).round() / 100.0;
    let mut rows: BTreeMap<(i32, i32), String> = BTreeMap::new();
    let (mut blades, mut instances, mut bytes) = (0usize, 0usize, 0usize);
    for b in state.built.values() {
        blades += b.blades;
        if let Some((_, i)) = &b.group {
            instances += i.blades.len();
            bytes += i.bytes();
        }
        rows.insert(
            (b.cell.q, b.cell.r),
            format!(
                "{{\"q\":{},\"r\":{},\"tile\":\"{}\",\"blades\":{},\"area_m2\":{},\"layout\":\"{:016x}\"}}",
                b.cell.q,
                b.cell.r,
                b.key.tile,
                b.blades,
                b.layout.area_m2.round(),
                b.layout.fingerprint()
            ),
        );
    }
    let rows: Vec<String> = rows.into_values().collect();
    if instances > 0 {
        bytes += BLADE_MESH_BYTES;
    }
    format!(
        "{{\"density\":{},\"distance\":{},\"status\":\"{status}\",\"groups\":{groups},\"blades\":{blades},\"triangles\":{},\"instances\":{instances},\"mesh_bytes\":{bytes},\"uploads\":{},\"per_m2\":{},\"thin\":[{},{},{THIN_KEEP}],\"fade\":[{},{}],\"build_radius\":{},\"last_build_ms\":{build_ms},\"last_swap_ms\":{swap_ms},\"tiles\":[{}]}}",
        f64::from(density.percent()) / 100.0,
        f64::from(distance.hundredths()) / 100.0,
        blades * BLADE_TRIANGLES,
        super::grass_gpu::uploads(),
        f64::from(DENSITY) * f64::from(density.percent()) / 100.0,
        d.thin_start,
        d.thin_end,
        d.fade_start,
        d.fade_end,
        d.build_radius,
        rows.join(",")
    )
}

/// Spawns the group drawing `layout` at `density` under `root`; `None` when it has no blade.
fn spawn_group(
    commands: &mut Commands,
    root: Entity,
    layout: &GrassLayout,
    density: Density,
) -> (Option<(Entity, Arc<GrassInstances>)>, usize) {
    let Some(instances) = GrassInstances::new(layout).map(Arc::new) else {
        return (None, 0);
    };
    let drawn = instances.drawn(density);
    let group = commands
        .spawn((
            GrassGroup {
                instances: instances.clone(),
                drawn,
            },
            Transform::IDENTITY,
            Visibility::Inherited,
            SyncToRenderWorld::default(),
            SceneEntity,
        ))
        .id();
    commands.entity(root).add_child(group);
    (Some((group, instances)), drawn as usize)
}

/// After `scatter::sync`: keeps a blade group under every drawn tile root near the camera, drops
/// the far ones, sets every group's drawn count for the current density (no rebuild, no upload),
/// and keeps the wind clock and the report current.
#[allow(clippy::too_many_arguments)]
pub fn sync(
    mut commands: Commands,
    (active, config, time, view): (
        Res<ActiveScenario>,
        Res<GateConfig>,
        Res<Time>,
        Res<PlayView>,
    ),
    (spawned, kit, packs, catalog): (
        Option<Res<PlaySpawned>>,
        Option<Res<PlayKit>>,
        Option<Res<PlayPacks>>,
        Res<PlayCatalog>,
    ),
    (gltfs, gltf_meshes, meshes): (Res<Assets<Gltf>>, Res<Assets<GltfMesh>>, Res<Assets<Mesh>>),
    (mut ground, layouts, mut state, mut out): (
        ResMut<PlayScatter>,
        Res<ScatterLayouts>,
        ResMut<PlayGrass>,
        ResMut<GrassReport>,
    ),
    mut clock: ResMut<GrassClock>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut groups: Query<&mut GrassGroup>,
) {
    let status = status(active.0, &config);
    let density = config.grass;
    let distance = config.grass_distance;
    let reach_m = Distances::new(distance);
    let state = &mut *state;
    // Off keeps each tile's group at a drawn count of 0, so turning grass back on changes counts
    // only; Off never seeds a new tile.
    let drawable = matches!(status, "on" | "off")
        && active.0 == Scenario::Play
        && matches!(config.tier, QualityTier::High | QualityTier::Ultra);
    let ready = drawable
        .then_some(())
        .and(spawned.as_deref().zip(kit.as_deref()).zip(packs.as_deref()));
    let Some(((spawned, PlayKit::Ready(kit)), packs)) = ready else {
        for (_, b) in state.built.drain() {
            if let Some((g, _)) = b.group {
                commands.entity(g).try_despawn();
            }
        }
        let json = report(state, density, distance, status, groups.iter().count());
        if out.0 != json {
            out.0 = json;
        }
        return;
    };
    let t = time.elapsed_secs_wrapped();
    clock.0 = GrassParams {
        time: t,
        prev_time: t - time.delta_secs(),
        ..GrassParams::new(reach_m)
    };
    let camera = cameras
        .iter()
        .find(|(c, _)| c.is_active)
        .map(|(_, g)| g.translation());
    // A tile's nearest point from the camera (m), at rest.
    let reach = |cell: Axial| {
        camera.map_or(f32::MAX, |c| {
            let [x, z] = cell.to_world(HEX_RADIUS);
            let flat = (Vec2::new(c.x - x, c.z - z).length() - HEX_RADIUS).max(0.0);
            Vec2::new(flat, c.y).length()
        })
    };
    let wanted: HashMap<Entity, (Axial, Key)> = spawned
        .tiles
        .iter()
        .filter(|(c, _)| !spawned.hidden.contains_key(c))
        .map(|(c, (t, root))| {
            let key = Key {
                tile: t.tile.clone(),
                seed: cell_seed(view.world_seed, *c),
            };
            (*root, (*c, key))
        })
        .collect();
    // A root gone from the view (sinking, replaced, rebuilt) keeps its group as a child until it
    // despawns; a far, hidden or changed tile, or one whose group a pack refresh despawned, drops it.
    state.built.retain(|root, b| {
        let Some((cell, key)) = wanted.get(root) else {
            return false;
        };
        let keep = *key == b.key
            && reach(*cell) <= reach_m.drop_radius
            && b.group.as_ref().is_none_or(|(g, _)| groups.contains(*g));
        if !keep && let Some((g, _)) = b.group {
            commands.entity(g).try_despawn();
        }
        keep
    });
    // A density change sets every group's drawn count at once: the instances are already on the GPU.
    if state.density != Some(density) {
        state.density = Some(density);
        let mut slowest_swap: Option<f32> = None;
        for b in state.built.values_mut() {
            let started = bevy::platform::time::Instant::now();
            let Some((g, instances)) = &b.group else {
                continue;
            };
            let drawn = instances.drawn(density);
            if let Ok(mut group) = groups.get_mut(*g) {
                group.drawn = drawn;
            }
            b.blades = drawn as usize;
            let ms = started.elapsed().as_secs_f32() * 1000.0;
            slowest_swap = Some(slowest_swap.map_or(ms, |m: f32| m.max(ms)));
        }
        if let Some(ms) = slowest_swap {
            state.last_swap_ms = ms;
        }
    }
    let mut builds = 0;
    let frame = bevy::platform::time::Instant::now();
    let spent = |builds: usize| {
        builds >= BUILDS_PER_FRAME
            || (builds > 0 && frame.elapsed().as_secs_f32() * 1000.0 >= BUILD_BUDGET_MS)
    };
    let mut todo: Vec<(f32, Entity, Axial, Key)> = wanted
        .iter()
        .filter(|(root, _)| !state.built.contains_key(root))
        .map(|(root, (cell, key))| (reach(*cell), *root, *cell, key.clone()))
        .filter(|(d, ..)| *d <= reach_m.build_radius)
        .collect();
    todo.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then((a.2.q, a.2.r).cmp(&(b.2.q, b.2.r)))
    });
    let mut slowest: Option<f32> = None;
    for (_, root, cell, key) in todo {
        if spent(builds) || density.is_off() {
            break;
        }
        let Some(def) = catalog.0.get(&key.tile) else {
            continue;
        };
        let Some(prepared) = ground.ground(def, &catalog.0, kit, &mut |id| {
            scatter::kit_tris(id, packs, (&*gltfs, &*gltf_meshes, &*meshes))
        }) else {
            continue;
        };
        // Scatter publishes its layout in the same frame its ground is ready.
        let (instances, pieces) = if def.props.scatter.is_empty() {
            (vec![], vec![])
        } else {
            let Some(s) = layouts.tiles.get(&cell).filter(|s| s.seed == key.seed) else {
                continue;
            };
            (s.layout.instances.clone(), s.pieces.clone())
        };
        let slots = state
            .slots
            .entry(def.id.clone())
            .or_insert_with(|| Arc::new(grass_slots(&packs.manifest, def, &catalog.0)))
            .clone();
        let excluded = exclusions(def, cell, &instances, &pieces);
        let started = bevy::platform::time::Instant::now();
        let l = Arc::new(layout(key.seed, &prepared.surface, &slots, &excluded));
        let (group, blades) = spawn_group(&mut commands, root, &l, density);
        let ms = started.elapsed().as_secs_f32() * 1000.0;
        slowest = Some(slowest.map_or(ms, |m: f32| m.max(ms)));
        builds += 1;
        state.built.insert(
            root,
            Built {
                key,
                cell,
                layout: l,
                group,
                blades,
            },
        );
    }
    if let Some(ms) = slowest {
        state.last_build_ms = ms;
    }
    let json = report(state, density, distance, status, groups.iter().count());
    if out.0 != json {
        out.0 = json;
    }
}

#[cfg(test)]
mod tests {
    use hex_assets::parse_manifest;

    use super::*;
    use crate::kit_scene::MANIFEST;

    #[test]
    fn grass_slots_are_the_grass_sets_minus_foliage() {
        let m = parse_manifest(MANIFEST).unwrap();
        let mut names: Vec<&str> = m
            .pieces
            .iter()
            .flat_map(|e| e.slots.iter())
            .filter(|(_, s)| s.tileable.as_deref() == Some("grass") && !s.foliage)
            .map(|(n, _)| n.as_str())
            .collect();
        names.sort();
        names.dedup();
        assert_eq!(
            names,
            [
                "beach_grass",
                "forest_base_edge",
                "forest_base_rim",
                "meadow_base_cap",
                "river_grass",
                "road_verge"
            ]
        );
        let c = TileCatalog::builtin().unwrap();
        let meadow = grass_slots(&m, c.get("meadow").unwrap(), &c);
        assert_eq!(meadow.len(), 1);
        assert_eq!(meadow.get("meadow_base_cap"), Some(&[1.0, 1.0, 1.0]));
    }

    #[test]
    fn one_shared_blade_mesh_and_32_byte_instances() {
        assert_eq!(BLADE_TRIANGLES, 6, "two crossed planes of three triangles");
        assert_eq!(BLADE_MESH_BYTES, 10 * 12 + 18 * 2);
        assert_eq!(BLADE_BYTES, 32);
        let mesh = blade_mesh();
        assert_eq!(mesh.count_vertices(), 10);
        assert_eq!(mesh.indices().unwrap().len(), 18);
        // The second plane's corners mirror the first's: grass_common turns it 90° about the root.
        for k in 0..5 {
            let [a, b] = [BLADE_CORNERS[k], BLADE_CORNERS[k + 5]];
            assert_eq!((a[0], a[1], a[2], b[2]), (b[0], b[1], 0.0, 1.0));
        }
        let blade = |keep: f32, color: [f32; 3]| Blade {
            root: Vec3::new(keep, 0.1, -keep),
            yaw_deg: 10,
            rank: 0.5,
            height: 0.3,
            width: 0.06,
            color,
            keep,
        };
        let l = GrassLayout {
            blades: vec![
                blade(0.5, [0.1, 0.2, 0.03]),
                blade(0.1, [0.0, 0.5, 1.0]),
                blade(0.9, [0.1, 0.2, 0.03]),
            ],
            area_m2: 1.0,
        };
        let i = GrassInstances::new(&l).unwrap();
        assert!(GrassInstances::new(&GrassLayout::default()).is_none());
        assert_eq!(i.bytes(), 3 * 32);
        // Ordered by keep: every density draws a prefix.
        assert_eq!(
            i.blades.iter().map(|b| b.root[0]).collect::<Vec<_>>(),
            [0.1, 0.5, 0.9]
        );
        assert_eq!(i.drawn(Density::OFF), 0);
        assert_eq!(i.drawn(Density::parse("sparse").unwrap()), 1, "0.5 > 1/3");
        assert_eq!(i.drawn(Density::parse("76%").unwrap()), 2);
        assert_eq!(i.drawn(Density::MAX), 3);
        let b = i.blades[0];
        assert_eq!(b.yaw_rank, 10.5);
        assert_eq!(b.size, [0.3, 0.06]);
        assert_eq!(b.color, [0, 32768, 65535, 0]);
        assert!(i.aabb.half_extents.y > 0.0);
    }

    /// Every grass slot's blade colour fits unorm16 without clamping, and the instances draw the
    /// same blades a density drew as a mesh: those whose `keep` it draws.
    #[test]
    fn instances_draw_each_densitys_blades() {
        let c = TileCatalog::builtin().unwrap();
        let m = parse_manifest(MANIFEST).unwrap();
        let slots = grass_slots(&m, c.get("meadow").unwrap(), &c);
        let l = layout(1, &meadow_surface(), &slots, &[]);
        assert!(
            l.blades
                .iter()
                .all(|b| b.color.iter().all(|&x| (0.0..1.0).contains(&x)))
        );
        let i = GrassInstances::new(&l).unwrap();
        assert_eq!(i.blades.len(), l.blades.len());
        for p in [1, 35, 50, 99, 100, 101, 149, 150] {
            let d = Density::from_share(p as f32 / 100.0).unwrap();
            let want = l.blades.iter().filter(|b| d.draws(b.keep)).count();
            assert_eq!(i.drawn(d) as usize, want, "{p}%");
        }
    }

    #[test]
    fn jitter_shifts_hue_and_lightness_only_as_asked() {
        let c = [0.0993, 0.2135, 0.0310];
        let same = jitter(c, 0.0, 0.0);
        for k in 0..3 {
            assert!((same[k] - c[k]).abs() < 1e-5, "{same:?} against {c:?}");
        }
        let light = jitter(c, 0.0, 0.05);
        let dark = jitter(c, 0.0, -0.05);
        assert!((0..3).all(|k| light[k] > c[k] && dark[k] < c[k]));
        let warm = jitter(c, -0.04, 0.0);
        assert!(
            warm[0] / warm[1] > c[0] / c[1],
            "a negative hue shift turns green toward yellow"
        );
    }

    /// Linear per-channel mean of a tileable set's albedo PNG, as the GPU samples it.
    fn set_mean(set: &str) -> [f64; 3] {
        use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
        let path = format!(
            "{}/../../assets/kit/p1/materials/{set}/{set}.albedo.png",
            env!("CARGO_MANIFEST_DIR")
        );
        let img = Image::from_buffer(
            &std::fs::read(path).unwrap(),
            ImageType::Extension("png"),
            CompressedImageFormats::NONE,
            false,
            ImageSampler::Default,
            RenderAssetUsages::default(),
        )
        .unwrap();
        let data = img.data.unwrap();
        let lin = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let n = (data.len() / 4) as f64;
        std::array::from_fn(|k| {
            data.as_chunks::<4>()
                .0
                .iter()
                .map(|p| lin(p[k]))
                .sum::<f64>()
                / n
        })
    }

    /// `DRY_SAND_OVER_SEABED` in kit_material.wesl is dry sand's mean (plaster set times the
    /// beach_sand tint) over the coast seabed's (gravel set times the beach_seabed tint), so the
    /// bed the swash bares reads as the sand beside it.
    #[test]
    fn dry_sand_over_seabed_matches_the_sets_and_tints() {
        let shader = include_str!("../../../../assets/shaders/kit_material.wesl");
        let line = shader
            .lines()
            .find(|l| l.starts_with("const DRY_SAND_OVER_SEABED"))
            .expect("constant");
        let inside = &line[line.find("vec3(").unwrap() + 5..line.rfind(')').unwrap()];
        let got: Vec<f64> = inside
            .split(',')
            .map(|v| v.trim().parse().unwrap())
            .collect();
        let m = parse_manifest(MANIFEST).unwrap();
        let beach = m.pieces.iter().find(|p| p.id == "beach_base").unwrap();
        let tint = |slot: &str| beach.slots[slot].color.unwrap().map(f64::from);
        let (sand, gravel) = (set_mean("plaster"), set_mean("gravel"));
        let (st, gt) = (tint("beach_sand"), tint("beach_seabed"));
        for k in 0..3 {
            let want = sand[k] * st[k] / (gravel[k] * gt[k]);
            assert!(
                (got[k] - want).abs() < 2e-3,
                "channel {k}: shader {} vs sets and tints {want:.4}",
                got[k]
            );
        }
        // Every coast base shares those tints.
        for p in m
            .pieces
            .iter()
            .filter(|p| p.slots.contains_key("beach_seabed"))
        {
            assert_eq!(
                p.slots["beach_seabed"].color,
                beach.slots["beach_seabed"].color
            );
            assert_eq!(p.slots["beach_sand"].color, beach.slots["beach_sand"].color);
        }
    }

    #[test]
    fn albedo_mean_matches_the_png() {
        use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
        let path = format!(
            "{}/../../assets/kit/p1/materials/grass/grass.albedo.png",
            env!("CARGO_MANIFEST_DIR")
        );
        let img = Image::from_buffer(
            &std::fs::read(path).unwrap(),
            ImageType::Extension("png"),
            CompressedImageFormats::NONE,
            false,
            ImageSampler::Default,
            RenderAssetUsages::default(),
        )
        .unwrap();
        let data = img.data.unwrap();
        let lin = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let n = (data.len() / 4) as f64;
        for (k, want) in GRASS_ALBEDO_MEAN.iter().enumerate() {
            let mean = data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| lin(p[k]))
                .sum::<f64>()
                / n;
            assert!(
                (mean - f64::from(*want)).abs() < 5e-4,
                "channel {k}: the PNG's linear mean is {mean:.4}, the constant {want}"
            );
        }
    }

    /// The meadow base's LOD0 ground as scatter samples it.
    fn meadow_surface() -> Surface {
        let m = parse_manifest(MANIFEST).unwrap();
        let e = m.pieces.iter().find(|p| p.id == "meadow_base").unwrap();
        let path = format!("{}/../../assets/kit/{}", env!("CARGO_MANIFEST_DIR"), e.glb);
        let bytes = std::fs::read(path).unwrap();
        let plain = hex_assets::rewrite_glb(&bytes).unwrap();
        let (doc, buffers, _) = gltf::import_slice(&plain).unwrap();
        let mesh = doc
            .meshes()
            .find(|m| m.name() == Some("meadow_base_lod0"))
            .unwrap();
        let mut tris = vec![];
        for prim in mesh.primitives() {
            let slot = prim.material().name().unwrap_or_default().to_string();
            let r = prim.reader(|b| Some(&buffers[b.index()]));
            let pos: Vec<[f32; 3]> = r.read_positions().unwrap().collect();
            let idx: Vec<u32> = r.read_indices().unwrap().into_u32().collect();
            for t in idx.as_chunks::<3>().0 {
                let [a, b, c] = t.map(|i| pos[i as usize]);
                tris.push(([a, b, c], slot.clone()));
            }
        }
        Surface::new(tris)
    }

    /// Owner feedback 2026-10-14: no thinner strip along tile borders. Blades within 1 m of the
    /// border stand at the interior density (within 10%) and none crosses it, so two meadows meet
    /// with neither a gap nor a double strip.
    #[test]
    fn blades_reach_the_border_at_the_interior_density() {
        let c = TileCatalog::builtin().unwrap();
        let m = parse_manifest(MANIFEST).unwrap();
        let slots = grass_slots(&m, c.get("meadow").unwrap(), &c);
        let surface = meadow_surface();
        let hex_area = |a: f32| 2.0 * 3f32.sqrt() * a * a;
        for seed in [1u64, 2, 3] {
            let l = layout(seed, &surface, &slots, &[]);
            let reach = |b: &Blade| scatter::hex_reach(b.root.xz());
            assert!(l.blades.iter().all(|b| reach(b) < APOTHEM));
            let normal: Vec<&Blade> = l
                .blades
                .iter()
                .filter(|b| Density::NORMAL.draws(b.keep))
                .collect();
            let count = |lo: f32, hi: f32| {
                normal
                    .iter()
                    .filter(|b| (lo..hi).contains(&reach(b)))
                    .count() as f32
            };
            let edge =
                count(APOTHEM - 1.0, APOTHEM) / (hex_area(APOTHEM) - hex_area(APOTHEM - 1.0));
            let inner = count(0.0, APOTHEM - 4.0) / hex_area(APOTHEM - 4.0);
            println!(
                "seed {seed}: {edge:.2} blades/m² within 1 m of the border, {inner:.2} inside"
            );
            assert!(
                (edge / inner - 1.0).abs() < 0.1,
                "seed {seed}: border strip {edge:.2}/m² against interior {inner:.2}/m²"
            );
        }
    }

    /// Owner feedback 2026-10-14: wildflowers stood in a bare ring. Grass grows in among a flower
    /// clump up to its small centre, while a rock keeps its base clear.
    #[test]
    fn grass_grows_among_flowers_but_not_under_rocks() {
        let c = TileCatalog::builtin().unwrap();
        let m = parse_manifest(MANIFEST).unwrap();
        let def = c.get("meadow").unwrap();
        let slots = grass_slots(&m, def, &c);
        let pieces = vec!["wildflower_clump".to_string(), "rock_small".to_string()];
        let at = |rule: u8, x: f32| Instance {
            rule,
            pos: Vec3::new(x, 0.0, 0.0),
            yaw: 0.0,
            scale: 1.0,
            order: rule,
            variant: 0,
        };
        let rock_at = at(1, 4.0);
        let ex = exclusions(def, Axial::ORIGIN, &[at(0, -4.0), rock_at.clone()], &pieces);
        let [.., flower, rock] = ex.as_slice() else {
            unreachable!()
        };
        let flower = disc(flower);
        assert_eq!(flower.1, 0.2 + PROP_GAP);
        assert!(matches!(rock, Clear::Rock { .. }), "{rock:?}");
        let l = layout(1, &meadow_surface(), &slots, &ex);
        let around = |centre: Vec2, lo: f32, hi: f32| {
            l.blades
                .iter()
                .filter(|b| (lo..hi).contains(&b.root.xz().distance(centre)))
                .count()
        };
        assert!(
            around(flower.0, flower.1, 0.9) > 0,
            "no grass among the flowers"
        );
        let base = placed_base("rock_small", &rock_at);
        assert!(
            !l.blades.iter().any(|b| inside(&base, b.root.xz())),
            "grass under the rock"
        );
    }

    /// The y = 0 slice of a piece's LOD0 and collider: where it meets the ground.
    fn ground_slice(id: &str) -> Vec<Vec2> {
        let mut tris = mesh_tris(id, "lod0");
        tris.extend(mesh_tris(id, "collider"));
        let mut out = vec![];
        for t in &tris {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                if a[1] != b[1] && a[1] * b[1] <= 0.0 {
                    let s = a[1] / (a[1] - b[1]);
                    out.push(Vec2::new(
                        a[0] + (b[0] - a[0]) * s,
                        a[2] + (b[2] - a[2]) * s,
                    ));
                }
            }
        }
        out
    }

    /// A scatter rock's ground outline (a counter-clockwise hull) placed by its instance.
    fn placed_base(id: &str, i: &Instance) -> Vec<Vec2> {
        let t = i.transform();
        let pts: Vec<Vec2> = ground_slice(id)
            .iter()
            .map(|p| t.transform_point(Vec3::new(p.x, 0.0, p.y)).xz())
            .collect();
        crate::walk::hull(&pts)
    }

    fn inside(hull: &[Vec2], p: Vec2) -> bool {
        (0..hull.len()).all(|k| (hull[(k + 1) % hull.len()] - hull[k]).perp_dot(p - hull[k]) > 0.0)
    }

    fn outline_distance(hull: &[Vec2], p: Vec2) -> f32 {
        (0..hull.len())
            .map(|k| {
                let (a, b) = (hull[k], hull[(k + 1) % hull.len()]);
                let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
                p.distance(a + (b - a) * t)
            })
            .fold(f32::MAX, f32::min)
    }

    /// Each [`ROCK_BASES`] outline is convex and counter-clockwise, holds its rock's ground slice
    /// (to 1 mm), and every side lies within 2 cm of the slice's hull, so it clears no more than
    /// the stone's base.
    #[test]
    fn rock_bases_match_the_kit() {
        for (id, outline) in &ROCK_BASES {
            let poly: Vec<Vec2> = outline.iter().map(|p| Vec2::from(*p)).collect();
            let slice = ground_slice(id);
            let hull = crate::walk::hull(&slice);
            for k in 0..poly.len() {
                let a = poly[k];
                let e = (poly[(k + 1) % poly.len()] - a).normalize();
                let turn = e.perp_dot(poly[(k + 2) % poly.len()] - a);
                assert!(turn > 0.0, "{id} outline not convex and CCW at {k}");
                let reach = slice
                    .iter()
                    .map(|p| e.perp_dot(*p - a))
                    .fold(f32::MAX, f32::min);
                assert!(reach > -0.001, "{id} slice outside side {k} by {}", -reach);
                let touch = hull
                    .iter()
                    .map(|p| e.perp_dot(*p - a))
                    .fold(f32::MAX, f32::min);
                assert!(touch < 0.02, "{id} side {k} stands {touch} m off the stone");
            }
        }
    }

    /// Owner 2026-10-19: grass comes right up to each rock and only where the stone sits is bare:
    /// no blade roots inside its base outline, every blade the open field grows more than
    /// [`PROP_GAP`] + 2 cm (the outline's fit) off the stone still grows, and blades stand within
    /// 0.15 m of it.
    #[test]
    fn grass_grows_right_up_to_each_rock() {
        let c = TileCatalog::builtin().unwrap();
        let m = parse_manifest(MANIFEST).unwrap();
        let def = c.get("meadow").unwrap();
        let slots = grass_slots(&m, def, &c);
        let pieces = vec!["rock_small".to_string()];
        let rocks: Vec<Instance> = (0..3u8)
            .map(|v| Instance {
                rule: 0,
                pos: Vec3::new(4.0 * f32::from(v) - 4.0, 0.0, 3.0),
                yaw: 0.7 + f32::from(v),
                scale: 1.1,
                order: v,
                variant: v,
            })
            .collect();
        let ex = exclusions(def, Axial::ORIGIN, &rocks, &pieces);
        let surface = meadow_surface();
        let l = layout(1, &surface, &slots, &ex);
        let grown: std::collections::HashSet<[u32; 2]> = l
            .blades
            .iter()
            .map(|b| [b.root.x.to_bits(), b.root.z.to_bits()])
            .collect();
        let open = layout(1, &surface, &slots, &[]);
        for i in &rocks {
            let id = i.piece("rock_small");
            let base = placed_base(id, i);
            let mut near = 0;
            for b in &l.blades {
                let p = b.root.xz();
                assert!(
                    !inside(&base, p),
                    "{id}: a blade roots under the stone at {p}"
                );
                if outline_distance(&base, p) < 0.15 {
                    near += 1;
                }
            }
            assert!(near >= 10, "{id}: {near} blades within 0.15 m of the stone");
            for b in &open.blades {
                let p = b.root.xz();
                let d = outline_distance(&base, p);
                if !inside(&base, p) && d > PROP_GAP * i.scale + 0.02 && d < 0.5 {
                    assert!(
                        grown.contains(&[b.root.x.to_bits(), b.root.z.to_bits()]),
                        "{id}: the blade {d:.3} m off the stone at {p} was cleared"
                    );
                }
            }
        }
    }

    /// The windmill tile's grass keeps clear of the tower (its trunk footprint) and still grows
    /// right up to it.
    #[test]
    fn grass_clears_the_windmill_tower() {
        let c = TileCatalog::builtin().unwrap();
        let m = parse_manifest(MANIFEST).unwrap();
        let def = c.get("windmill").unwrap();
        let ex = exclusions(def, Axial::ORIGIN, &[], &[]);
        let tower = scatter::footprint("windmill").ground + PROP_GAP;
        assert_eq!(
            ex,
            vec![Clear::Disc {
                centre: Vec2::ZERO,
                radius: tower
            }]
        );
        let l = layout(1, &meadow_surface(), &grass_slots(&m, def, &c), &ex);
        let within = |lo: f32, hi: f32| {
            l.blades
                .iter()
                .filter(|b| (lo..hi).contains(&b.root.xz().length()))
                .count()
        };
        assert_eq!(within(0.0, tower), 0, "grass inside the tower");
        assert!(within(tower, tower + 1.0) > 0, "no grass at the tower foot");
    }

    fn disc(c: &Clear) -> (Vec2, f32) {
        match *c {
            Clear::Disc { centre, radius } => (centre, radius),
            Clear::Box { .. } | Clear::Rock { .. } => panic!("not a disc: {c:?}"),
        }
    }

    /// Mesh `<id>_<mesh>`'s triangles from the piece's kit GLB.
    fn mesh_tris(id: &str, mesh: &str) -> Vec<scatter::Tri> {
        let m = parse_manifest(MANIFEST).unwrap();
        let e = m.pieces.iter().find(|p| p.id == id).unwrap();
        let path = format!("{}/../../assets/kit/{}", env!("CARGO_MANIFEST_DIR"), e.glb);
        let bytes = std::fs::read(path).unwrap();
        let plain = hex_assets::rewrite_glb(&bytes).unwrap();
        let (doc, buffers, _) = gltf::import_slice(&plain).unwrap();
        let name = format!("{id}_{mesh}");
        let mesh = doc.meshes().find(|m| m.name() == Some(&name)).unwrap();
        let mut tris = vec![];
        for prim in mesh.primitives() {
            let r = prim.reader(|b| Some(&buffers[b.index()]));
            let pos: Vec<[f32; 3]> = r.read_positions().unwrap().collect();
            let idx: Vec<u32> = r.read_indices().unwrap().into_u32().collect();
            tris.extend(
                idx.as_chunks::<3>()
                    .0
                    .iter()
                    .map(|t| t.map(|i| pos[i as usize])),
            );
        }
        tris
    }

    /// A hero's kit collider on the tile's ground, placed as walk mode places it, and its own
    /// +x and +z there.
    fn placed_collider(h: &hex_core::HeroProp, piece: &str) -> (crate::walk::Obstacle, Vec2, Vec2) {
        let turn = Quat::from_rotation_y(-h.yaw_deg.to_radians());
        let at = Vec2::from(h.at);
        let tris = mesh_tris(piece, "collider");
        let pts: Vec<Vec2> = tris
            .iter()
            .flatten()
            .map(|p| at + (turn * Vec3::new(p[0], 0.0, p[2])).xz())
            .collect();
        let obstacle = crate::walk::Obstacle {
            piece: piece.to_owned(),
            poly: crate::walk::hull(&pts),
        };
        (obstacle, (turn * Vec3::X).xz(), (turn * Vec3::Z).xz())
    }

    /// Every hero's grass on a meadow cap: no blade within 0.1 m of a box's collider (0.04 m of
    /// a disc's), and for each rectangular building, the blades rooted in the band from
    /// [`WALL_GAP`] to 0.6 m outside each long wall, over its middle (by wall). Each hero is the
    /// piece its slot draws at `cell`.
    fn hero_grass(def: &TileDef, cell: Axial) -> Vec<(String, [usize; 2])> {
        let c = TileCatalog::builtin().unwrap();
        let m = parse_manifest(MANIFEST).unwrap();
        let ex = exclusions(def, cell, &[], &[]);
        let l = layout(1, &meadow_surface(), &grass_slots(&m, def, &c), &ex);
        let mut bands = vec![];
        // A fence's collider hull spans the yard it encloses; its posts are checked on their own
        // (`grass_grows_under_the_paddock_fence`).
        for (h, piece) in def
            .hero_pieces(cell)
            .filter(|(_, p)| scatter::fence(p).is_none())
        {
            let (collider, x, z) = placed_collider(h, piece);
            let boxed = scatter::ground_box(piece);
            let gap = if boxed.is_some() { 0.1 } else { 0.04 };
            for b in &l.blades {
                assert!(
                    !collider.hits(b.root.xz(), gap),
                    "a blade at {} in {piece}",
                    b.root.xz()
                );
            }
            let Some(gb) = boxed else {
                continue;
            };
            // Long walls run along the box's longer side.
            let (min, max) = (Vec2::from(gb.min), Vec2::from(gb.max));
            let (along, across, lo, hi, a0, a1) = if max.x - min.x > max.y - min.y {
                (x, z, min.y, max.y, min.x, max.x)
            } else {
                (z, x, min.x, max.x, min.y, max.y)
            };
            let mut walls = [0; 2];
            for b in &l.blades {
                let d = b.root.xz() - Vec2::from(h.at);
                let (u, v) = (d.dot(along), d.dot(across));
                if u < a0 + 0.5 || u > a1 - 0.5 {
                    continue;
                }
                if (lo - 0.6..lo - WALL_GAP).contains(&v) {
                    walls[0] += 1;
                } else if (hi + WALL_GAP..hi + 0.6).contains(&v) {
                    walls[1] += 1;
                }
            }
            bands.push((piece.to_owned(), walls));
        }
        bands
    }

    /// The farm tile's grass keeps clear of the barn, the bales and the animals (each hero's
    /// footprint: a box over the barn's collider, discs for the rest), and grows within 0.6 m
    /// of both of the barn's long walls (owner 2026-10-19: no bare band along them). The paddock
    /// fence clears its posts alone, after the other heroes.
    #[test]
    fn grass_clears_the_barn_and_the_farm_heroes() {
        let def = TileCatalog::builtin().unwrap().get("farm").unwrap().clone();
        let ex = exclusions(&def, Axial::ORIGIN, &[], &[]);
        // Each hero clears the piece it draws at the cell (the origin draws hay_bale_round_b in
        // the first round slot, `hex_core::hero_piece`).
        let heroes: Vec<_> = def
            .hero_pieces(Axial::ORIGIN)
            .filter(|(_, p)| scatter::fence(p).is_none())
            .collect();
        assert!(heroes.iter().any(|(_, p)| *p == "hay_bale_round_b"));
        let posts: usize = def
            .props
            .hero
            .iter()
            .filter_map(|h| scatter::fence(h.piece.as_str()))
            .map(|f| f.posts.len())
            .sum();
        assert_eq!(posts, 27, "the paddock's posts");
        assert_eq!(ex.len(), heroes.len() + posts);
        for ((h, piece), c) in heroes.into_iter().zip(&ex) {
            if piece == "barn_red" {
                assert!(matches!(c, Clear::Box { .. }), "{c:?}");
            } else {
                assert_eq!(
                    disc(c),
                    (
                        Vec2::from(h.at),
                        scatter::footprint(piece).ground + PROP_GAP
                    )
                );
            }
        }
        let bands = hero_grass(&def, Axial::ORIGIN);
        println!("{bands:?}");
        let [(barn, walls)] = bands.as_slice() else {
            panic!("{bands:?}")
        };
        assert_eq!(barn, "barn_red");
        assert!(walls.iter().all(|&n| n >= 50), "barn walls {walls:?}");
    }

    /// The paddock fence clears a disc over each post (0.142 m, gate posts 0.163 m), not a disc
    /// of its whole reach at the tile centre: grass grows in the yard at (0, 0), under the rails
    /// mid-span, and never on a post.
    #[test]
    fn grass_grows_under_the_paddock_fence() {
        let c = TileCatalog::builtin().unwrap();
        let def = c.get("farm").unwrap().clone();
        let fence = scatter::fence("fence_paddock").unwrap();
        let ex = exclusions(&def, Axial::ORIGIN, &[], &[]);
        let discs: Vec<(Vec2, f32)> = ex
            .iter()
            .filter_map(|c| match *c {
                Clear::Disc { centre, radius } => Some((centre, radius)),
                _ => None,
            })
            .filter(|(c, _)| fence.posts.iter().any(|(at, _)| *c == Vec2::from(*at)))
            .collect();
        assert_eq!(discs.len(), 27);
        for (at, half) in fence.posts {
            let want = if *half > 0.07 { 0.163 } else { 0.142 };
            let (_, r) = discs.iter().find(|(c, _)| *c == Vec2::from(*at)).unwrap();
            assert!((r - want).abs() < 1e-3, "post at {at:?}: {r}");
        }
        assert!(
            ex.iter().all(|c| !c.contains(Vec2::ZERO)),
            "the yard centre is not cleared"
        );
        let m = parse_manifest(MANIFEST).unwrap();
        let l = layout(1, &meadow_surface(), &grass_slots(&m, &def, &c), &ex);
        let near = |p: Vec2, r: f32| {
            l.blades
                .iter()
                .filter(|b| b.root.xz().distance(p) < r)
                .count()
        };
        // The old disc at the tile centre (1.05 m radius, the default 1 m reach plus the gap):
        // grown over at about the open meadow's density.
        let centre = near(Vec2::ZERO, 1.0);
        let open = near(Vec2::new(-4.0, -4.0), 1.0);
        assert!(
            centre * 2 > open && centre > 30,
            "{centre} blades vs {open} in the open"
        );
        // Under the rails mid-span: the north run between its posts at x -5.504 and -3.253, and
        // the north-west diagonal between (-8.171, 9.435) and (-6.128, 10.614).
        for mid in [Vec2::new(-4.38, 2.5), Vec2::new(-7.15, 10.02)] {
            assert!(near(mid, 0.3) > 0, "no grass under the rail at {mid}");
        }
        for (at, half) in fence.posts {
            let r = half * std::f32::consts::SQRT_2;
            assert_eq!(near(Vec2::from(*at), r), 0, "a blade in the post at {at:?}");
        }
    }

    /// The houses clear their collider rectangle, turned by each one's yaw, and grass grows within
    /// 0.6 m of every long wall: at the origin (both cottages) and at (1, 0), where house 1 draws
    /// the longhouse, so the exclusion box is the drawn piece's.
    #[test]
    fn grass_grows_along_house_walls() {
        let c = TileCatalog::builtin().unwrap();
        let mut def = c.get("meadow").unwrap().clone();
        def.props.hero = c.get("house_cluster").unwrap().props.hero.clone();
        for (cell, longhouses) in [(Axial::ORIGIN, 0), (Axial::new(1, 0), 1)] {
            let bands = hero_grass(&def, cell);
            println!("{bands:?}");
            assert_eq!(bands.len(), 3, "{bands:?}");
            assert_eq!(
                bands.iter().filter(|(p, _)| p == "house_longhouse").count(),
                longhouses,
                "{bands:?}"
            );
            for (piece, walls) in &bands {
                assert!(walls.iter().all(|&n| n >= 30), "{piece} walls {walls:?}");
            }
        }
    }

    /// The arithmetic `cos_sin_deg` agrees with std trig and with the hero's turn as walk places
    /// it (`Quat::from_rotation_y(-yaw)` takes +x to (cos, sin)).
    #[test]
    fn cos_sin_deg_matches_trig_and_the_hero_turn() {
        for k in -720..=720 {
            let deg = k as f32 * 0.5;
            let r = deg.to_radians();
            let v = cos_sin_deg(deg);
            assert!(v.distance(Vec2::new(r.cos(), r.sin())) < 1e-6, "{deg}: {v}");
            let q = (Quat::from_rotation_y(-r) * Vec3::X).xz();
            assert!(v.distance(q) < 1e-6, "{deg}: {v} against {q}");
        }
    }

    #[test]
    fn densities_parse_shares_percentages_and_aliases() {
        let p = |s: &str| Density::parse(s).map(Density::percent);
        // The stepped levels the slider replaced: off 0, sparse 50, normal 100, lush 150.
        assert_eq!(p("off"), Some(0));
        assert_eq!(p("sparse"), Some(50));
        assert_eq!(p("normal"), Some(100));
        assert_eq!(p("on"), Some(100));
        assert_eq!(p("lush"), Some(150));
        assert_eq!(p("1.2"), Some(120));
        assert_eq!(p("0.55"), Some(55));
        assert_eq!(p("120%"), Some(120));
        assert_eq!(p("35 %"), Some(35));
        assert_eq!(p("0"), Some(0));
        // Out of range clamps; a bare number is the share, so 100 is the top, not 100%.
        assert_eq!(p("100"), Some(150));
        assert_eq!(p("200%"), Some(150));
        assert_eq!(p("-1"), Some(0));
        assert_eq!(p("thick"), None);
        assert_eq!(p("NaN"), None);
        assert_eq!(p("inf"), None);
        assert_eq!(Density::from_share(f32::NAN), None);
        assert_eq!(Density::from_share(2.0), Some(Density::MAX));
        assert_eq!(Density::default(), Density::NORMAL);
        assert_eq!(Density::NORMAL.share(), 1.0);
        assert!(Density::OFF.is_off() && !Density::from_share(0.01).unwrap().is_off());
    }

    /// Every lower density draws a subset of every higher one, so a slider move only changes how
    /// many of the seeded Lush set's instances draw; 0 draws nothing and 150% everything.
    #[test]
    fn continuous_densities_nest() {
        let all: Vec<Density> = (0..=150)
            .map(|p| Density::from_share(p as f32 / 100.0).unwrap())
            .collect();
        assert_eq!(
            all.iter().map(|d| d.percent()).collect::<Vec<_>>(),
            (0..=150).collect::<Vec<u8>>()
        );
        let mut rng = Rng(7);
        let keeps: Vec<f32> = (0..5_000).map(|_| rng.unit()).collect();
        for &k in &keeps {
            assert!(!Density::OFF.draws(k));
            assert!(Density::MAX.draws(k));
            for w in all.windows(2) {
                assert!(
                    !w[0].draws(k) || w[1].draws(k),
                    "{:?} draws a blade {:?} does not",
                    w[0],
                    w[1]
                );
            }
        }
        // The drawn share tracks the slider: 100% draws two thirds of the Lush set.
        let normal = keeps.iter().filter(|&&k| Density::NORMAL.draws(k)).count() as f32;
        assert!((normal / keeps.len() as f32 - 2.0 / 3.0).abs() < 0.02);
    }

    /// Near reproduces the shipped tuning bit for bit; every stop scales the six distances (never
    /// the kept share) and keeps fade end < build radius < drop radius.
    #[test]
    fn distances_scale_from_the_near_tuning() {
        let near = Distances::new(GrassDistance::NEAR);
        assert_eq!(
            near,
            Distances {
                thin_start: THIN_START,
                thin_end: THIN_END,
                fade_start: FADE_START,
                fade_end: FADE_END,
                build_radius: BUILD_RADIUS,
                drop_radius: DROP_RADIUS,
            }
        );
        assert_eq!(GrassDistance::default(), GrassDistance::NEAR);
        let p = GrassParams::new(near);
        assert_eq!(
            (
                p.thin_start,
                p.thin_end,
                p.thin_keep,
                p.fade_start,
                p.fade_end
            ),
            (THIN_START, THIN_END, THIN_KEEP, FADE_START, FADE_END)
        );
        for (name, h) in GrassDistance::ALIASES {
            let g = GrassDistance::parse(name).unwrap();
            assert_eq!(g.hundredths(), h);
            let d = Distances::new(g);
            let s = g.scale();
            assert_eq!(d.fade_end, FADE_END * s, "{name}");
            assert_eq!(d.thin_end, THIN_END * s, "{name}");
            assert_eq!(d.drop_radius, DROP_RADIUS * s, "{name}");
            assert!(d.thin_start < d.thin_end, "{name}");
            assert!(d.fade_start < d.fade_end, "{name}");
            assert!(d.fade_end < d.build_radius, "{name}");
            assert!(d.build_radius < d.drop_radius, "{name}");
            assert_eq!(GrassParams::new(d).thin_keep, THIN_KEEP, "{name}");
        }
        let far = Distances::new(GrassDistance::parse("far").unwrap());
        assert_eq!(
            (far.fade_start, far.fade_end, far.thin_end),
            (68.0, 116.0, 92.0)
        );
    }

    #[test]
    fn grass_distances_parse_stops_and_multipliers() {
        let p = |s: &str| GrassDistance::parse(s).map(GrassDistance::hundredths);
        assert_eq!(p("near"), Some(100));
        assert_eq!(p("medium"), Some(150));
        assert_eq!(p("far"), Some(200));
        assert_eq!(p("very-far"), Some(300));
        assert_eq!(p("2"), Some(200));
        assert_eq!(p(" 1.25 "), Some(125));
        assert_eq!(p("0.5"), Some(100), "never nearer than the shipped tuning");
        assert_eq!(p("9"), Some(300));
        assert_eq!(p("NaN"), None);
        assert_eq!(p("farther"), None);
        assert_eq!(GrassDistance::from_scale(f32::INFINITY), None);
    }

    #[test]
    fn status_follows_flag_tier_and_scenario() {
        let mut c = GateConfig::default();
        assert_eq!(status(Scenario::Play, &c), "on");
        assert_eq!(status(Scenario::Kit, &c), "scenario");
        c.tier = QualityTier::Medium;
        assert_eq!(status(Scenario::Play, &c), "tier");
        c.tier = QualityTier::Ultra;
        c.grass = Density::OFF;
        assert_eq!(status(Scenario::Play, &c), "off");
    }
}
