//! Grass blades on grass-covered ground near the camera (owner request 2026-10-14: a grass shader
//! for the high detail LOD), High and Ultra tiers only, toggled by `grass=on|off`.
//!
//! - **Ground.** Blades grow on the topmost LOD0 surface of a tile's ground pieces (scatter's
//!   [`Surface`], from [`PlayScatter::ground`]) where its slot is a grass slot: a manifest slot whose
//!   tileable set is `grass` and that is not foliage ([`grass_slots`]: `meadow_base_cap`,
//!   `forest_base_rim` and `_edge`, `road_verge`, `river_grass` and `beach_grass`; the hedge is
//!   foliage). Scatter's own slot filter ([`scatter::excluded_slot`]) is not used: it drops every
//!   `road_`, `river_` and `beach_` slot and every rim and edge band, which are exactly the verges,
//!   banks and dune grass that should carry blades. Roads, sand, river beds and pads are not grass
//!   slots, so they never do.
//! - **Exclusions.** Scatter's footprints: no blade within a hero prop's ground footprint (houses,
//!   the oak's trunk) or a scatter instance's (rocks, spruce trunks, flower clumps, every planned
//!   instance of the tile's layout, so a late pack never moves the grass), nor below the water level
//!   ([`WATER_MARGIN`]). The hex border is not an exclusion (scatter's border gap keeps crowns and
//!   footprints inside the hex, which would thin grass into a seam): the border has no width, each
//!   tile owns the blades on its side of it, and its jittered grid keeps the density up to the edge,
//!   so two grass tiles read as one field.
//! - **Placement.** A jittered grid at [`DENSITY`] blades per m² of grass ground (thinned per slot
//!   by its [`Style`]), seeded from `cell_seed(world seed, cell)` with its own salt, so a tile always
//!   grows the same blades and scatter's stream is untouched. Arithmetic and `sqrt` only, so native
//!   and wasm agree.
//! - **Drawing.** One mesh per tile, a [`GrassGroup`] child of the tile root: it rises and sinks with
//!   the tile and goes with it on bulldoze, undo, replace and Start over, and the ghost never draws
//!   it. Every vertex of a blade carries the blade's root; `grass_common.wesl` poses it in the vertex
//!   stage (the shared wind of `wind.wesl`, bent by the square of the height along the blade) and
//!   shrinks it to nothing by [`FADE_END`] m from the camera, after thinning it by rank from
//!   [`THIN_START`]. Only tiles within [`BUILD_RADIUS`] of the camera hold a mesh, at most
//!   [`BUILDS_PER_FRAME`] new ones a frame, nearest first; beyond [`DROP_RADIUS`] they are dropped.
//! - **Material.** [`GrassMaterial`] binds one uniform and no texture: the pipeline samples only the
//!   view's textures, fewer than the kit forward pipeline. Blades write the depth, normal and motion
//!   prepass (tilt-shift, refraction, SSAO and TAA see them), receive shadows and cast none.
//! - **Colour.** A blade's base colour is its slot's tint times the grass set's mean linear albedo
//!   ([`GRASS_ALBEDO_MEAN`]), so blades blend into the ground at the root; the shader lightens the
//!   tip and takes the §9.4 season tint from the entity's `MeshTag`.
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use bevy::{
    asset::RenderAssetUsages,
    camera::{primitives::Aabb, visibility::NoAutoAabb},
    gltf::{Gltf, GltfMesh},
    light::NotShadowCaster,
    mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology},
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};
use hex_assets::KitManifest;
use hex_core::{Axial, HEX_RADIUS, TileCatalog, TileDef, cell_seed};

use super::{
    APOTHEM, PlayCatalog, PlayKit, PlaySpawned, PlayView,
    packs::PlayPacks,
    scatter::{self, Instance, PlayScatter, Rng, ScatterLayouts, Surface},
};
use crate::{ActiveScenario, Scenario, SceneEntity, post::QualityTier, workload::GateConfig};

pub const SHADER: &str = "shaders/grass.wesl";
pub const PREPASS_SHADER: &str = "shaders/grass_prepass.wesl";

/// Mean linear albedo of the `grass` tileable set (`p1/materials/grass/grass.albedo.png`, sRGB
/// decoded per texel, as the GPU samples it). `albedo_mean_matches_the_png` holds it to the file.
pub const GRASS_ALBEDO_MEAN: [f32; 3] = [0.0993, 0.2135, 0.0310];
/// Blades per m² of grass ground at [`Density::Normal`], before a slot's own thinning.
pub const DENSITY: f32 = 14.0;
/// Camera distance (m) where blades start to thin by rank, and where only [`THIN_KEEP`] are left.
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
/// New tile meshes per frame, nearest first.
pub const BUILDS_PER_FRAME: usize = 3;
/// Scale on the shared wind's push for grass.
pub const WIND_SCALE: f32 = 1.0;
/// Blades keep this far (m) above the water level (`water::tiles::WATER_Y`).
pub const WATER_MARGIN: f32 = 0.3;
/// Gap (m) between a blade's root and a prop's ground footprint.
pub const PROP_GAP: f32 = 0.05;
const SALT: u64 = 0x6A55_B1AD_E5C0_FFEE;
/// Vertices per blade: two at the root, two at mid height, one at the tip.
pub const BLADE_VERTS: usize = 5;
const BLADE_TRIS: [u16; 9] = [0, 1, 2, 2, 1, 3, 2, 3, 4];

/// The grass density (Settings → Graphics → Grass, the `grass` key, `set_grass`) as a share of
/// [`DENSITY`]. Every tile seeds the Lush set once; a density draws the blades whose `keep` is under
/// its share of Lush, so Sparse is a subset of Normal and Normal of Lush, and a change only rebuilds
/// meshes from the cached set (never the scene).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Density {
    Off,
    Sparse,
    #[default]
    Normal,
    Lush,
}

impl Density {
    pub const ALL: [Density; 4] = [
        Density::Off,
        Density::Sparse,
        Density::Normal,
        Density::Lush,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Density::Off => "off",
            Density::Sparse => "sparse",
            Density::Normal => "normal",
            Density::Lush => "lush",
        }
    }

    /// `off|sparse|normal|lush`; `on` is Normal.
    pub fn parse(s: &str) -> Option<Density> {
        match s {
            "on" => Some(Density::Normal),
            _ => Density::ALL.into_iter().find(|d| d.name() == s),
        }
    }

    /// Share of [`DENSITY`].
    pub fn factor(self) -> f32 {
        match self {
            Density::Off => 0.0,
            Density::Sparse => 0.5,
            Density::Normal => 1.0,
            Density::Lush => LUSH,
        }
    }

    /// Whether a blade with this `keep` draws at this density.
    pub fn draws(self, keep: f32) -> bool {
        keep * LUSH < self.factor()
    }
}

/// The Lush share of [`DENSITY`]: the seeded set every density draws from.
pub const LUSH: f32 = 1.5;

/// How blades grow on one kind of grass slot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    /// Share of [`DENSITY`] kept.
    pub density: f32,
    /// Blade height range (m).
    pub height: [f32; 2],
    /// Blade width at the root (m).
    pub width: [f32; 2],
}

/// Mown verges, dune grass, bank grass, the forest margin and the meadow.
pub fn style(slot: &str) -> Style {
    match slot {
        "road_verge" => Style {
            density: 0.9,
            height: [0.12, 0.24],
            width: [0.05, 0.08],
        },
        "beach_grass" => Style {
            density: 0.55,
            height: [0.3, 0.62],
            width: [0.04, 0.07],
        },
        "river_grass" => Style {
            density: 0.85,
            height: [0.24, 0.5],
            width: [0.05, 0.08],
        },
        s if s.starts_with("forest_base") => Style {
            density: 0.75,
            height: [0.18, 0.38],
            width: [0.05, 0.08],
        },
        _ => Style {
            density: 1.0,
            height: [0.22, 0.5],
            width: [0.04, 0.075],
        },
    }
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

/// Discs (centre, radius) no blade may root in: hero props and every scatter instance.
pub fn exclusions(def: &TileDef, instances: &[Instance], pieces: &[String]) -> Vec<(Vec2, f32)> {
    let mut out: Vec<(Vec2, f32)> = def
        .props
        .hero
        .iter()
        .map(|h| {
            let f = scatter::footprint(h.piece.as_str());
            (Vec2::from(h.at), f.ground + PROP_GAP)
        })
        .collect();
    out.extend(instances.iter().filter_map(|i| {
        let piece = pieces.get(i.rule as usize)?;
        Some((
            i.pos.xz(),
            scatter::footprint(piece).ground * i.scale + PROP_GAP,
        ))
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
    excluded: &[(Vec2, f32)],
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
            let r = [(); 10].map(|_| rng.unit());
            let p = Vec2::new(
                -HEX_RADIUS + (i as f32 + r[0]) * step,
                -HEX_RADIUS + (j as f32 + r[1]) * step,
            );
            if scatter::hex_reach(p) >= APOTHEM
                || excluded
                    .iter()
                    .any(|(c, rad)| p.distance_squared(*c) < rad * rad)
            {
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
            let shade = 0.82 + 0.3 * r[6];
            let warm = 1.0 + 0.16 * (r[7] - 0.5);
            out.blades.push(Blade {
                root: Vec3::new(p.x, y, p.y),
                yaw_deg: ((r[3] * 360.0) as u16).min(359),
                rank: 0.9 * r[4],
                height: lerp(st.height, r[5]),
                width: lerp(st.width, r[8]),
                color: [
                    tint[0] * GRASS_ALBEDO_MEAN[0] * shade * warm,
                    tint[1] * GRASS_ALBEDO_MEAN[1] * shade,
                    tint[2] * GRASS_ALBEDO_MEAN[2] * shade / warm,
                ],
                keep: r[9],
            });
        }
    }
    out.area_m2 = ground as f32 * step * step;
    out
}

/// The mesh of the blades a density draws (render world only), its bounds with room for the bend,
/// and its blade count; `None` when it draws none.
pub fn blade_mesh(layout: &GrassLayout, density: Density) -> Option<(Mesh, Aabb, usize)> {
    let blades: Vec<&Blade> = layout
        .blades
        .iter()
        .filter(|b| density.draws(b.keep))
        .collect();
    if blades.is_empty() {
        return None;
    }
    let n = blades.len() * BLADE_VERTS;
    let mut pos = Vec::with_capacity(n);
    let mut col = Vec::with_capacity(n);
    let mut uv0 = Vec::with_capacity(n);
    let mut uv1 = Vec::with_capacity(n);
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    let mut tallest: f32 = 0.0;
    for b in &blades {
        let w = 0.5 * b.width;
        let packed = f32::from(b.yaw_deg) + b.rank;
        for (across, along) in [
            (-w, 0.0),
            (w, 0.0),
            (-0.6 * w, 0.5),
            (0.6 * w, 0.5),
            (0.0, 1.0),
        ] {
            pos.push(b.root.to_array());
            col.push(b.color);
            uv0.push([across, along]);
            uv1.push([packed, b.height]);
        }
        lo = lo.min(b.root);
        hi = hi.max(b.root);
        tallest = tallest.max(b.height);
    }
    let indices = if n <= usize::from(u16::MAX) + 1 {
        Indices::U16(
            (0..blades.len())
                .flat_map(|k| BLADE_TRIS.map(|t| (k * BLADE_VERTS) as u16 + t))
                .collect(),
        )
    } else {
        Indices::U32(
            (0..blades.len())
                .flat_map(|k| BLADE_TRIS.map(|t| (k * BLADE_VERTS) as u32 + u32::from(t)))
                .collect(),
        )
    };
    let mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, col)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv0)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, uv1)
    .with_inserted_indices(indices);
    // The pose bends a tip at most 1.1 heights sideways and the patch noise grows blades by 1.3.
    let reach = 1.3 * tallest;
    let aabb = Aabb::from_min_max(
        lo - Vec3::new(1.2 * reach, 0.0, 1.2 * reach),
        hi + Vec3::new(1.2 * reach, reach, 1.2 * reach),
    );
    Some((mesh, aabb, blades.len()))
}

/// Bytes the GPU holds for a mesh of `blades` blades (vertices and indices).
pub fn mesh_bytes(blades: usize) -> usize {
    blades * BLADE_VERTS * (12 + 12 + 8 + 8) + blades * BLADE_TRIS.len() * 2
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
    fn new() -> Self {
        Self {
            time: 0.0,
            prev_time: 0.0,
            fade_start: FADE_START,
            fade_end: FADE_END,
            thin_start: THIN_START,
            thin_end: THIN_END,
            thin_keep: THIN_KEEP,
            wind_scale: WIND_SCALE,
        }
    }
}

/// One uniform, no texture (see the module docs for the sampled-texture budget).
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct GrassMaterial {
    #[uniform(100)]
    pub params: GrassParams,
}

impl Material for GrassMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    fn prepass_vertex_shader() -> ShaderRef {
        PREPASS_SHADER.into()
    }

    fn enable_shadows() -> bool {
        false
    }

    /// Blades are single quads seen from both sides.
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// The entity drawing one tile's blades, a child of the tile root.
#[derive(Component)]
pub struct GrassGroup;

#[derive(Clone, Debug, PartialEq)]
struct Key {
    tile: String,
    seed: u64,
}

struct Built {
    key: Key,
    cell: Axial,
    /// The tile's Lush set, kept so a density change only rebuilds the mesh.
    layout: Arc<GrassLayout>,
    density: Density,
    /// `None` when the density draws no blade on the tile (no grass ground).
    group: Option<Entity>,
    blades: usize,
}

/// The shared material, the grass slots per tile id and the group drawn under each tile root.
#[derive(Resource, Default)]
pub struct PlayGrass {
    material: Option<Handle<GrassMaterial>>,
    slots: HashMap<String, Arc<HashMap<String, [f32; 3]>>>,
    built: HashMap<Entity, Built>,
    /// The slowest single tile build (seeded set and mesh, ms) in the latest frame that built one.
    last_build_ms: f32,
}

/// `state().grass` as JSON.
#[derive(Resource, Clone, Debug, Default)]
pub struct GrassReport(pub String);

/// Why grass draws or not: the density, the tier, the scenario.
pub fn status(active: Scenario, config: &GateConfig) -> &'static str {
    if config.grass == Density::Off {
        "off"
    } else if !matches!(config.tier, QualityTier::High | QualityTier::Ultra) {
        "tier"
    } else if active != Scenario::Play {
        "scenario"
    } else {
        "on"
    }
}

/// `state().grass`: the density and why it draws or not, live groups and blades, the tuning, and a
/// row per tile holding a mesh.
pub fn report(state: &PlayGrass, density: Density, status: &str, groups: usize) -> String {
    let build_ms = (state.last_build_ms * 100.0).round() / 100.0;
    let mut rows: BTreeMap<(i32, i32), String> = BTreeMap::new();
    let mut blades = 0usize;
    for b in state.built.values() {
        blades += b.blades;
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
    format!(
        "{{\"density\":\"{}\",\"status\":\"{status}\",\"groups\":{groups},\"blades\":{blades},\"mesh_bytes\":{},\"per_m2\":{},\"thin\":[{THIN_START},{THIN_END},{THIN_KEEP}],\"fade\":[{FADE_START},{FADE_END}],\"build_radius\":{BUILD_RADIUS},\"last_build_ms\":{build_ms},\"tiles\":[{}]}}",
        density.name(),
        mesh_bytes(blades),
        DENSITY * density.factor(),
        rows.join(",")
    )
}

/// Spawns the group drawing `layout` at `density` under `root`.
fn spawn_group(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: &Handle<GrassMaterial>,
    root: Entity,
    layout: &GrassLayout,
    density: Density,
) -> (Option<Entity>, usize) {
    let Some((mesh, aabb, n)) = blade_mesh(layout, density) else {
        return (None, 0);
    };
    let group = commands
        .spawn((
            GrassGroup,
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material.clone()),
            Transform::IDENTITY,
            aabb,
            NoAutoAabb,
            NotShadowCaster,
            SceneEntity,
        ))
        .id();
    commands.entity(root).add_child(group);
    (Some(group), n)
}

/// After `scatter::sync`: keeps a blade mesh under every drawn tile root near the camera at the
/// current density, drops the far ones, and keeps the wind clock and the report current. A density
/// change swaps each mesh in place from the tile's cached Lush set, nearest first, so no tile is
/// bare between the two.
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
    (gltfs, gltf_meshes, mut meshes): (
        Res<Assets<Gltf>>,
        Res<Assets<GltfMesh>>,
        ResMut<Assets<Mesh>>,
    ),
    (mut ground, layouts, mut state, mut out): (
        ResMut<PlayScatter>,
        Res<ScatterLayouts>,
        ResMut<PlayGrass>,
        ResMut<GrassReport>,
    ),
    mut materials: ResMut<Assets<GrassMaterial>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    groups: Query<(), With<GrassGroup>>,
) {
    let status = status(active.0, &config);
    let density = config.grass;
    let state = &mut *state;
    // Off keeps each tile's seeded set (its group despawned) so turning grass back on swaps meshes
    // only; Off never seeds a new tile.
    let drawable = matches!(status, "on" | "off")
        && active.0 == Scenario::Play
        && matches!(config.tier, QualityTier::High | QualityTier::Ultra);
    let ready = drawable
        .then_some(())
        .and(spawned.as_deref().zip(kit.as_deref()).zip(packs.as_deref()));
    let Some(((spawned, PlayKit::Ready(kit)), packs)) = ready else {
        for (_, b) in state.built.drain() {
            if let Some(g) = b.group {
                commands.entity(g).try_despawn();
            }
        }
        let json = report(state, density, status, groups.iter().count());
        if out.0 != json {
            out.0 = json;
        }
        return;
    };
    let t = time.elapsed_secs_wrapped();
    let params = GrassParams {
        time: t,
        prev_time: t - time.delta_secs(),
        ..GrassParams::new()
    };
    let material = match &state.material {
        Some(h) => {
            if let Some(mut m) = materials.get_mut(h) {
                m.params = params;
            }
            h.clone()
        }
        None => {
            let h = materials.add(GrassMaterial { params });
            state.material = Some(h.clone());
            h
        }
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
            && reach(*cell) <= DROP_RADIUS
            && b.group.is_none_or(|g| groups.contains(g));
        if !keep && let Some(g) = b.group {
            commands.entity(g).try_despawn();
        }
        keep
    });
    let mut builds = 0;
    // Density changes first, nearest first: the old mesh stays until its replacement is spawned.
    let mut swaps: Vec<(f32, Entity)> = state
        .built
        .iter()
        .filter(|(_, b)| b.density != density)
        .map(|(root, b)| (reach(b.cell), *root))
        .collect();
    swaps.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    for (_, root) in swaps.into_iter().take(BUILDS_PER_FRAME) {
        let b = state.built.get_mut(&root).expect("listed above");
        let (group, n) = spawn_group(
            &mut commands,
            &mut meshes,
            &material,
            root,
            &b.layout,
            density,
        );
        if let Some(g) = b.group {
            commands.entity(g).try_despawn();
        }
        (b.group, b.blades, b.density) = (group, n, density);
        builds += 1;
    }
    let mut todo: Vec<(f32, Entity, Axial, Key)> = wanted
        .iter()
        .filter(|(root, _)| !state.built.contains_key(root))
        .map(|(root, (cell, key))| (reach(*cell), *root, *cell, key.clone()))
        .filter(|(d, ..)| *d <= BUILD_RADIUS)
        .collect();
    todo.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then((a.2.q, a.2.r).cmp(&(b.2.q, b.2.r)))
    });
    let mut slowest: Option<f32> = None;
    for (_, root, cell, key) in todo {
        if builds >= BUILDS_PER_FRAME || density == Density::Off {
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
        let excluded = exclusions(def, &instances, &pieces);
        let started = bevy::platform::time::Instant::now();
        let l = Arc::new(layout(key.seed, &prepared.surface, &slots, &excluded));
        let (group, blades) = spawn_group(&mut commands, &mut meshes, &material, root, &l, density);
        let ms = started.elapsed().as_secs_f32() * 1000.0;
        slowest = Some(slowest.map_or(ms, |m: f32| m.max(ms)));
        builds += 1;
        state.built.insert(
            root,
            Built {
                key,
                cell,
                layout: l,
                density,
                group,
                blades,
            },
        );
    }
    if let Some(ms) = slowest {
        state.last_build_ms = ms;
    }
    let json = report(state, density, status, groups.iter().count());
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
    fn mesh_bytes_count_vertices_and_indices() {
        assert_eq!(mesh_bytes(1), 5 * 40 + 9 * 2);
        let l = GrassLayout {
            blades: vec![
                Blade {
                    root: Vec3::ZERO,
                    yaw_deg: 10,
                    rank: 0.5,
                    height: 0.3,
                    width: 0.06,
                    color: [0.1, 0.2, 0.03],
                    keep: 0.5,
                };
                3
            ],
            area_m2: 1.0,
        };
        let (mesh, aabb, n) = blade_mesh(&l, Density::Lush).unwrap();
        assert_eq!(n, 3);
        assert!(
            blade_mesh(&l, Density::Sparse).is_none(),
            "keep 0.5 is over Sparse's third"
        );
        assert_eq!(mesh.count_vertices(), 15);
        assert_eq!(mesh.indices().unwrap().len(), 27);
        assert!(aabb.half_extents.y > 0.0);
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
                .filter(|b| Density::Normal.draws(b.keep))
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

    #[test]
    fn densities_parse_and_nest() {
        for d in Density::ALL {
            assert_eq!(Density::parse(d.name()), Some(d));
        }
        assert_eq!(Density::parse("on"), Some(Density::Normal));
        assert_eq!(Density::parse("thick"), None);
        assert_eq!(Density::default(), Density::Normal);
        let mut rng = Rng(7);
        for _ in 0..10_000 {
            let k = rng.unit();
            assert!(!Density::Off.draws(k));
            for w in Density::ALL.windows(2) {
                assert!(
                    !w[0].draws(k) || w[1].draws(k),
                    "{:?} draws a blade {:?} does not",
                    w[0],
                    w[1]
                );
            }
            assert!(Density::Lush.draws(k));
        }
    }

    #[test]
    fn status_follows_flag_tier_and_scenario() {
        let mut c = GateConfig::default();
        assert_eq!(status(Scenario::Play, &c), "on");
        assert_eq!(status(Scenario::Kit, &c), "scenario");
        c.tier = QualityTier::Medium;
        assert_eq!(status(Scenario::Play, &c), "tier");
        c.tier = QualityTier::Ultra;
        c.grass = Density::Off;
        assert_eq!(status(Scenario::Play, &c), "off");
    }
}
