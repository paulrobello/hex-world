//! `layout=instanced`: the shipping static-piece path (PRD §6).
//!
//! Every static piece is one 48-byte record in a single GPU instance buffer, grouped by kit
//! mesh and, within a mesh, by chunk, so each chunk owns one contiguous range per mesh. Each
//! frame, for every view (camera and each shadow cascade), the CPU culls chunks against the
//! view frustum, picks each chunk's LOD from the camera distance and writes the surviving
//! ranges into a per-frame id list. Each view then issues at most one draw per (kit mesh, LOD)
//! through one pipeline per pass; the material comes from a 40-entry palette indexed per
//! instance.
//!
//! Bevy 0.20's `custom_shader_instancing` example only queues into `Transparent3d`. Here the
//! draws are `NonMesh` items in the binned `Opaque3d`, `Opaque3dPrepass` and `Shadow` phases,
//! so the pieces write depth, normals and motion vectors for SSAO and TAA and cast shadows into
//! every cascade. The main pass uses Bevy's `MeshPipeline` layout and `apply_pbr_lighting`, so
//! they receive shadows like `StandardMaterial` meshes; SSAO is sampled in the fragment shader
//! the way `pbr_input_from_standard_material` does, since `apply_pbr_lighting` only consumes it.
use std::{
    collections::HashMap,
    ops::Range,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use bevy::{
    asset::{UntypedAssetId, uuid::Uuid},
    camera::primitives::{Aabb, Frustum},
    core_pipeline::{
        core_3d::{CORE_3D_DEPTH_FORMAT, Opaque3d, Opaque3dBatchSetKey, Opaque3dBinKey},
        prepass::{
            DepthPrepass, MotionVectorPrepass, NormalPrepass, Opaque3dPrepass,
            OpaqueNoLightmap3dBatchSetKey, OpaqueNoLightmap3dBinKey, prepass_target_descriptors,
        },
    },
    ecs::system::{
        SystemParamItem,
        lifetimeless::{Read, SRes},
    },
    math::Affine3A,
    mesh::MeshVertexBufferLayoutRef,
    pbr::{
        LightEntity, MeshPipeline, MeshPipelineKey, PrepassPipeline, SetMeshViewBindGroup,
        SetMeshViewBindingArrayBindGroup, SetPrepassViewBindGroup, SetPrepassViewEmptyBindGroup,
        Shadow, ShadowBatchSetKey, ShadowBinKey, ViewKeyCache,
    },
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        camera::ExtractedCamera,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        mesh::{
            RenderMesh, RenderMeshBufferInfo,
            allocator::{MeshAllocator, MeshSlabs},
        },
        render_asset::RenderAssets,
        render_phase::{
            AddRenderCommand, BinnedRenderPhaseType, DrawFunctions, InputUniformIndex, PhaseItem,
            RenderCommand, RenderCommandResult, SetItemPipeline, TrackedRenderPass,
            ViewBinnedRenderPhases,
        },
        render_resource::{binding_types::storage_buffer_read_only_sized, *},
        renderer::{RenderDevice, RenderQueue},
        settings::WgpuFeatures,
        sync_world::MainEntity,
        view::ExtractedView,
    },
    shader::ShaderDefVal,
    shape::ViewFrustum,
};
use bytemuck::{Pod, Zeroable};
use hex_core::{Axial, HEX_RADIUS};

use crate::{
    chunks::chunk_tiles,
    scene::{PieceMeshes, SceneEntity, casts_shadow, gate_material, tile_pieces},
    workload::{GATE, ShadowCasters, piece_material},
};

/// Kit meshes: base, edge, center, prop sphere, prop cylinder.
pub const KINDS: usize = 5;
/// LOD switch distances, measured from the camera to the chunk centroid (PRD §10).
pub const LOD_NEAR: f32 = 150.0;
pub const LOD_FAR: f32 = 400.0;

/// One static piece on the GPU: `world = translation + rotation · (scale · local)`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct GpuPiece {
    pub rotation: [f32; 4],
    pub translation: [f32; 3],
    /// Palette index, `0..GATE.materials`.
    pub material: u32,
    pub scale: [f32; 3],
    pub _pad: u32,
}

/// One palette entry: the `StandardMaterial` fields the gate materials set.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuMaterial {
    /// Linear RGBA.
    pub base_color: [f32; 4],
    /// Linear RGB, alpha unused.
    pub emissive: [f32; 4],
    pub roughness: f32,
    pub metallic: f32,
    pub reflectance: f32,
    pub _pad: f32,
}

/// One chunk: where it is, what it spans, and its instance range per kit mesh.
#[derive(Clone, Debug)]
pub struct ChunkInfo {
    /// Centroid of its tiles at ground level; LOD is chosen from the camera distance to it.
    pub centroid: Vec3,
    /// World-space bounds of every piece in the chunk at LOD0.
    pub aabb: Aabb,
    pub tiles: u32,
    pub ranges: [Range<u32>; KINDS],
}

/// The whole static gate as one instance buffer.
#[derive(Clone, Debug, Default)]
pub struct InstanceSet {
    pub pieces: Vec<GpuPiece>,
    pub chunks: Vec<ChunkInfo>,
}

impl InstanceSet {
    /// Instances of kit mesh `kind` (identical at every LOD: a LOD swaps the mesh only).
    pub fn kind_count(&self, kind: usize) -> u32 {
        self.chunks
            .iter()
            .map(|c| c.ranges[kind].len() as u32)
            .sum()
    }
}

/// LOD of a chunk whose centroid is `distance` metres from the camera.
pub fn chunk_lod(distance: f32) -> usize {
    if distance < LOD_NEAR {
        0
    } else if distance < LOD_FAR {
        1
    } else {
        2
    }
}

fn mesh_aabb(mesh: &Mesh) -> Aabb {
    let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("kit mesh lacks Float32x3 positions");
    };
    let (lo, hi) = p.iter().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(lo, hi), v| (lo.min(Vec3::from(*v)), hi.max(Vec3::from(*v))),
    );
    Aabb::from_min_max(lo, hi)
}

fn affine_aabb(local: &Aabb, t: &Transform) -> (Vec3, Vec3) {
    let (c, h) = (Vec3::from(local.center), Vec3::from(local.half_extents));
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for i in 0..8 {
        let s = Vec3::new(
            if i & 1 == 0 { -1.0 } else { 1.0 },
            if i & 2 == 0 { -1.0 } else { 1.0 },
            if i & 4 == 0 { -1.0 } else { 1.0 },
        );
        let p = t.transform_point(c + h * s);
        lo = lo.min(p);
        hi = hi.max(p);
    }
    (lo, hi)
}

/// Builds the instance buffer for `cells` in chunks of `chunk` hexes. Transforms and material
/// indices are exactly what the entities layout spawns.
pub(crate) fn build_instances(
    m: &PieceMeshes,
    meshes: &Assets<Mesh>,
    cells: &[Axial],
    chunk: u32,
) -> InstanceSet {
    let kinds = m.kinds();
    let local_aabbs: Vec<Aabb> = kinds
        .iter()
        .map(|lods| meshes.get(&lods[0]).map(mesh_aabb).expect("kit mesh"))
        .collect();
    let groups = chunk_tiles(cells, chunk);
    let mut per_kind: [Vec<GpuPiece>; KINDS] = Default::default();
    let mut chunks = Vec::with_capacity(groups.len());
    for (_, members) in &groups {
        let mut starts = [0u32; KINDS];
        for (k, v) in per_kind.iter().enumerate() {
            starts[k] = v.len() as u32;
        }
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        let mut centroid = Vec3::ZERO;
        for &ti in members {
            let [x, z] = cells[ti].to_world(HEX_RADIUS);
            centroid += Vec3::new(x, 0.0, z);
            for (pi, (lods, t)) in tile_pieces(m, ti).into_iter().enumerate() {
                let kind = kinds
                    .iter()
                    .position(|k| k[0] == lods[0])
                    .expect("piece uses a kit mesh");
                let world = Transform::from_xyz(x, 0.0, z) * t;
                let (a, b) = affine_aabb(&local_aabbs[kind], &world);
                lo = lo.min(a);
                hi = hi.max(b);
                per_kind[kind].push(GpuPiece {
                    rotation: world.rotation.to_array(),
                    translation: world.translation.to_array(),
                    material: piece_material(ti, pi) as u32,
                    scale: world.scale.to_array(),
                    _pad: 0,
                });
            }
        }
        let ranges = std::array::from_fn(|k| starts[k]..per_kind[k].len() as u32);
        chunks.push(ChunkInfo {
            centroid: centroid / members.len() as f32,
            aabb: Aabb::from_min_max(lo, hi),
            tiles: members.len() as u32,
            ranges,
        });
    }
    // Concatenate the kinds: kind k's ranges shift by the instances of the kinds before it.
    let mut pieces = Vec::with_capacity(per_kind.iter().map(Vec::len).sum());
    let mut base = [0u32; KINDS];
    for (k, v) in per_kind.iter().enumerate() {
        base[k] = pieces.len() as u32;
        pieces.extend_from_slice(v);
    }
    for c in &mut chunks {
        for (k, r) in c.ranges.iter_mut().enumerate() {
            *r = r.start + base[k]..r.end + base[k];
        }
    }
    InstanceSet { pieces, chunks }
}

/// Culls `set`'s chunks against one view and appends the visible instance ids to `ids`, grouped
/// by draw slot (`kind * 3 + lod`; a chunk draws at `lods[chunk]`). Shadow views skip slots
/// that do not cast and ignore the near plane. Returns the non-empty slots' ranges into `ids`;
/// `drawn` receives the chunks that contributed at least one instance.
pub(crate) fn view_ranges(
    set: &InstanceSet,
    lods: &[usize],
    casts: &[[bool; 3]; KINDS],
    frustum: &Frustum,
    shadow: bool,
    ids: &mut Vec<u32>,
    drawn: &mut Vec<usize>,
) -> Vec<(usize, Range<u32>)> {
    let visible: Vec<usize> = (0..set.chunks.len())
        .filter(|&ci| {
            // Shadow casters in front of a cascade's near plane still cast into it.
            frustum.intersects_obb(&set.chunks[ci].aabb, &Affine3A::IDENTITY, !shadow, true)
        })
        .collect();
    let mut seen = vec![false; set.chunks.len()];
    let mut out = vec![];
    for (kind, kind_casts) in casts.iter().enumerate() {
        for (lod, &cast) in kind_casts.iter().enumerate() {
            if shadow && !cast {
                continue;
            }
            let first = ids.len() as u32;
            for &ci in &visible {
                let r = set.chunks[ci].ranges[kind].clone();
                if lods[ci] == lod && !r.is_empty() {
                    ids.extend(r);
                    seen[ci] = true;
                }
            }
            let last = ids.len() as u32;
            if last > first {
                out.push((kind * 3 + lod, first..last));
            }
        }
    }
    drawn.clear();
    drawn.extend((0..set.chunks.len()).filter(|&ci| seen[ci]));
    out
}

/// Which (kit mesh, LOD) slots cast shadows under `casters`, derived from the per-piece rule
/// `scene::casts_shadow` through each kind's first piece index in a tile.
pub(crate) fn kind_casts(casters: ShadowCasters) -> [[bool; 3]; KINDS] {
    std::array::from_fn(|kind| {
        std::array::from_fn(|lod| casts_shadow(casters, KIND_FIRST_PIECE[kind], lod))
    })
}

/// Index within `tile_pieces` of the first piece of each kind: base, edge, center, sphere, cylinder.
const KIND_FIRST_PIECE: [usize; KINDS] = [0, 1, 7, 8, 9];

// ---------------------------------------------------------------------------------------------
// Main world: what the scene spawns.

/// The gate's instance data and kit meshes, extracted to the render world.
#[derive(Resource, Clone, ExtractResource)]
#[extract_app(RenderApp)]
pub struct InstancedScene(pub Arc<InstancedSceneData>);

pub struct InstancedSceneData {
    pub set: InstanceSet,
    /// Strong handles: nothing else keeps the kit meshes alive in this layout.
    pub handles: Vec<Handle<Mesh>>,
    pub meshes: [[AssetId<Mesh>; 3]; KINDS],
    pub tris: [[u32; 3]; KINDS],
    /// Whether (kind, LOD) is drawn into shadow maps.
    pub casts: [[bool; 3]; KINDS],
    pub palette: Vec<GpuMaterial>,
}

/// One draw slot: kit mesh `kind` at `lod`. Each view draws each slot at most once.
#[derive(Component, Clone, Copy, ExtractComponent)]
#[extract_app(RenderApp)]
pub struct PieceGroup {
    pub kind: u8,
    pub lod: u8,
}

impl PieceGroup {
    fn slot(self) -> usize {
        self.kind as usize * 3 + self.lod as usize
    }
}

fn palette() -> Vec<GpuMaterial> {
    (0..GATE.materials)
        .map(|i| {
            let m = gate_material(i);
            GpuMaterial {
                base_color: m.base_color.to_linear().to_f32_array(),
                // Alpha is StandardMaterial's `emissive_exposure_weight` (default 0).
                emissive: [m.emissive.red, m.emissive.green, m.emissive.blue, 0.0],
                roughness: m.perceptual_roughness,
                metallic: m.metallic,
                reflectance: m.reflectance,
                _pad: 0.0,
            }
        })
        .collect()
}

/// Builds the instance buffer, inserts `InstancedScene` and spawns the 15 draw-slot entities.
/// Returns the chunk count.
pub(crate) fn spawn(
    commands: &mut Commands,
    meshes: &Assets<Mesh>,
    m: &PieceMeshes,
    cells: &[Axial],
    chunk: u32,
    casters: ShadowCasters,
) -> usize {
    let set = build_instances(m, meshes, cells, chunk);
    let kinds = m.kinds();
    let tri = |h: &Handle<Mesh>| {
        meshes
            .get(h)
            .map_or(0, |m| m.indices().map_or(0, |i| i.len() / 3)) as u32
    };
    let data = InstancedSceneData {
        meshes: kinds.map(|k| [k[0].id(), k[1].id(), k[2].id()]),
        tris: kinds.map(|k| [tri(&k[0]), tri(&k[1]), tri(&k[2])]),
        casts: kind_casts(casters),
        palette: palette(),
        handles: kinds.iter().flat_map(|k| k.iter().cloned()).collect(),
        set,
    };
    let chunks = data.set.chunks.len();
    for kind in 0..KINDS as u8 {
        for lod in 0..3u8 {
            commands.spawn((PieceGroup { kind, lod }, SceneEntity));
        }
    }
    commands.insert_resource(InstancedScene(Arc::new(data)));
    chunks
}

// ---------------------------------------------------------------------------------------------
// Diagnostics: what the latest census frame drew.

/// One view's instanced draws in a census frame.
#[derive(Clone, Debug, Default)]
pub struct InstancedViewCensus {
    /// `main` or `cascade<i>`.
    pub view: String,
    /// Non-empty (kit mesh, LOD) draws this view issued.
    pub draws: u32,
    pub instances: u32,
    pub tris: u64,
}

/// Instanced census across views, plus the union over views (comparable to the entities
/// layout's `ViewVisibility` union).
#[derive(Clone, Debug, Default)]
pub struct InstancedCensus {
    pub views: Vec<InstancedViewCensus>,
    pub union_instances_by_lod: [u32; 3],
    pub union_tris: u64,
}

static CENSUS: Mutex<Option<InstancedCensus>> = Mutex::new(None);
static CENSUS_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Asks the render world to record an instanced census on its next frame.
pub fn request_census() {
    CENSUS_REQUESTED.store(true, Ordering::Relaxed);
}

/// The latest instanced census, if one has been recorded since the scene was built.
pub fn latest_census() -> Option<InstancedCensus> {
    CENSUS.lock().unwrap().clone()
}

// ---------------------------------------------------------------------------------------------
// Render world.

pub struct InstancedPlugin;

impl Plugin for InstancedPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractResourcePlugin::<InstancedScene>::default(),
            ExtractComponentPlugin::<PieceGroup>::default(),
        ));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<InstancedGpu>()
            .init_resource::<SpecializedMeshPipelines<InstancedMainPipeline>>()
            .init_resource::<SpecializedMeshPipelines<InstancedPrepassPipeline>>()
            .add_render_command::<Opaque3d, DrawPiecesMain>()
            .add_render_command::<Opaque3dPrepass, DrawPiecesPrepass>()
            .add_render_command::<Shadow, DrawPiecesPrepass>()
            .add_systems(
                Render,
                (
                    init_pipelines
                        .in_set(RenderSystems::PrepareViews)
                        .run_if(not(resource_exists::<InstancedMainPipeline>)),
                    queue_pieces.in_set(RenderSystems::Queue),
                    prepare_pieces.in_set(RenderSystems::PrepareResources),
                    prepare_pieces_bind_group.in_set(RenderSystems::PrepareBindGroups),
                ),
            );
    }
}

const SHADER: &str = "shaders/instanced_piece.wesl";

fn pieces_layout() -> BindGroupLayoutDescriptor {
    BindGroupLayoutDescriptor::new(
        "instanced_pieces",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::VERTEX_FRAGMENT,
            (
                storage_buffer_read_only_sized(false, None),
                storage_buffer_read_only_sized(false, None),
                storage_buffer_read_only_sized(false, None),
            ),
        ),
    )
}

#[derive(Resource, Clone)]
struct InstancedMainPipeline {
    mesh: MeshPipeline,
    pieces: BindGroupLayoutDescriptor,
    shader: Handle<Shader>,
}

impl SpecializedMeshPipeline for InstancedMainPipeline {
    type Key = MeshPipelineKey;

    fn specialize(
        &self,
        key: Self::Key,
        layout: &MeshVertexBufferLayoutRef,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let mut d = self.mesh.specialize(key, layout)?;
        // Group 2 is Bevy's per-mesh uniform group; the pieces read their own buffers there.
        d.layout[2] = self.pieces.clone();
        d.layout.truncate(3);
        d.label = Some("instanced_pieces_main".into());
        d.vertex.shader = self.shader.clone();
        d.vertex.entry_point = Some("vertex_main".into());
        let f = d.fragment.as_mut().expect("main pass has a fragment stage");
        f.shader = self.shader.clone();
        f.entry_point = Some("fragment_main".into());
        d.primitive.cull_mode = Some(Face::Back);
        Ok(d)
    }
}

#[derive(Resource, Clone)]
struct InstancedPrepassPipeline {
    view_motion_vectors: BindGroupLayoutDescriptor,
    view_no_motion_vectors: BindGroupLayoutDescriptor,
    empty: BindGroupLayoutDescriptor,
    pieces: BindGroupLayoutDescriptor,
    shader: Handle<Shader>,
    depth_clip_control: bool,
}

impl SpecializedMeshPipeline for InstancedPrepassPipeline {
    type Key = MeshPipelineKey;

    /// Mirrors `PrepassPipeline::specialize` for an opaque, non-skinned, non-deferred mesh.
    fn specialize(
        &self,
        key: Self::Key,
        layout: &MeshVertexBufferLayoutRef,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let normal = key.contains(MeshPipelineKey::NORMAL_PREPASS);
        let motion = key.contains(MeshPipelineKey::MOTION_VECTOR_PREPASS);
        let ortho_unclipped = key.contains(MeshPipelineKey::UNCLIPPED_DEPTH_ORTHO);
        let emulate = ortho_unclipped && !self.depth_clip_control;
        let mut defs: Vec<ShaderDefVal> = vec!["INSTANCED_PREPASS".into()];
        if normal {
            defs.push("NORMAL_PREPASS".into());
        }
        if motion {
            defs.push("MOTION_VECTOR_PREPASS".into());
        }
        if emulate {
            defs.push("UNCLIPPED_DEPTH_ORTHO_EMULATION".into());
        }
        let mut targets = prepass_target_descriptors(normal, motion, false);
        if targets.iter().all(Option::is_none) {
            targets.clear();
        }
        if !targets.is_empty() || emulate {
            defs.push("PREPASS_FRAGMENT".into());
        }
        let fragment = (!targets.is_empty() || emulate).then(|| FragmentState {
            shader: self.shader.clone(),
            shader_defs: defs.clone(),
            entry_point: Some("fragment_prepass".into()),
            targets,
            ..default()
        });
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
        ])?;
        Ok(RenderPipelineDescriptor {
            label: Some("instanced_pieces_prepass".into()),
            layout: vec![
                if motion {
                    self.view_motion_vectors.clone()
                } else {
                    self.view_no_motion_vectors.clone()
                },
                self.empty.clone(),
                self.pieces.clone(),
            ],
            vertex: VertexState {
                shader: self.shader.clone(),
                shader_defs: defs,
                entry_point: Some("vertex_prepass".into()),
                buffers: vec![vertex_layout],
                ..default()
            },
            fragment,
            primitive: PrimitiveState {
                topology: key.primitive_topology(),
                cull_mode: Some(Face::Back),
                unclipped_depth: ortho_unclipped && self.depth_clip_control,
                ..default()
            },
            depth_stencil: Some(DepthStencilState {
                format: CORE_3D_DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(CompareFunction::GreaterEqual),
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState {
                count: key.msaa_samples(),
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            ..default()
        })
    }
}

fn init_pipelines(
    mut commands: Commands,
    mesh: Res<MeshPipeline>,
    prepass: Option<Res<PrepassPipeline>>,
    asset_server: Res<AssetServer>,
    device: Res<RenderDevice>,
) {
    let Some(prepass) = prepass else {
        return;
    };
    let shader = asset_server.load(SHADER);
    let pieces = pieces_layout();
    commands.insert_resource(InstancedMainPipeline {
        mesh: mesh.clone(),
        pieces: pieces.clone(),
        shader: shader.clone(),
    });
    commands.insert_resource(InstancedPrepassPipeline {
        view_motion_vectors: prepass.view_layout_motion_vectors.clone(),
        view_no_motion_vectors: prepass.view_layout_no_motion_vectors.clone(),
        empty: prepass.empty_layout.clone(),
        pieces,
        shader,
        depth_clip_control: device.features().contains(WgpuFeatures::DEPTH_CLIP_CONTROL),
    });
}

/// GPU state for the pieces plus this frame's per-view draw ranges.
#[derive(Resource)]
struct InstancedGpu {
    scene: Option<Arc<InstancedSceneData>>,
    pieces: Option<Buffer>,
    palette: Option<Buffer>,
    ids: RawBufferVec<u32>,
    bind_group: Option<BindGroup>,
    /// (view render entity, slot) -> instance range into `ids`.
    draws: HashMap<(Entity, usize), Range<u32>>,
}

impl Default for InstancedGpu {
    fn default() -> Self {
        Self {
            scene: None,
            pieces: None,
            palette: None,
            ids: RawBufferVec::new(BufferUsages::STORAGE),
            bind_group: None,
            draws: HashMap::default(),
        }
    }
}

/// Uploads the instance buffer when the scene changes, then culls chunks per view and writes
/// each view's visible ranges, grouped by draw slot, into the per-frame id list.
#[allow(clippy::too_many_arguments)]
fn prepare_pieces(
    scene: Option<Res<InstancedScene>>,
    mut gpu: ResMut<InstancedGpu>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    cameras: Query<&ExtractedView, With<ExtractedCamera>>,
    views: Query<(Entity, &ExtractedView, Option<&LightEntity>)>,
    camera_views: Query<(), With<ExtractedCamera>>,
) {
    let gpu = &mut *gpu;
    gpu.draws.clear();
    gpu.ids.clear();
    let Some(scene) = scene else {
        gpu.scene = None;
        return;
    };
    if gpu.scene.as_ref().is_none_or(|s| !Arc::ptr_eq(s, &scene.0)) {
        let d = &scene.0;
        gpu.pieces = Some(device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("instanced_pieces"),
            contents: bytemuck::cast_slice(&d.set.pieces),
            usage: BufferUsages::STORAGE,
        }));
        gpu.palette = Some(device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("instanced_palette"),
            contents: bytemuck::cast_slice(&d.palette),
            usage: BufferUsages::STORAGE,
        }));
        gpu.scene = Some(scene.0.clone());
        *CENSUS.lock().unwrap() = None;
    }
    let d = scene.0.clone();
    let Some(camera) = cameras.iter().next() else {
        return;
    };
    // LOD is per chunk from the main camera, in every view (as `VisibilityRange` does).
    let eye = camera.world_from_view.translation();
    let lods: Vec<usize> = d
        .set
        .chunks
        .iter()
        .map(|c| chunk_lod(eye.distance(c.centroid)))
        .collect();

    let census = CENSUS_REQUESTED.swap(false, Ordering::Relaxed);
    let mut views_census = vec![];
    let mut union_seen = vec![false; d.set.chunks.len()];
    let mut visible: Vec<usize> = Vec::with_capacity(d.set.chunks.len());
    for (entity, view, light) in &views {
        let shadow = match light {
            Some(LightEntity::Directional { .. }) => true,
            Some(_) => continue,
            None if camera_views.contains(entity) => false,
            None => continue,
        };
        let clip_from_world = view.clip_from_view * view.world_from_view.to_matrix().inverse();
        let frustum = Frustum(ViewFrustum::from_clip_from_world(&clip_from_world));
        let ranges = view_ranges(
            &d.set,
            &lods,
            &d.casts,
            &frustum,
            shadow,
            gpu.ids.values_mut(),
            &mut visible,
        );
        let mut vc = InstancedViewCensus {
            view: match light {
                Some(LightEntity::Directional { cascade_index, .. }) => {
                    format!("cascade{cascade_index}")
                }
                _ => "main".into(),
            },
            ..default()
        };
        for (slot, r) in ranges {
            let (kind, lod) = (slot / 3, slot % 3);
            vc.draws += 1;
            vc.instances += r.len() as u32;
            vc.tris += r.len() as u64 * d.tris[kind][lod] as u64;
            gpu.draws.insert((entity, slot), r);
        }
        if census {
            for &ci in &visible {
                union_seen[ci] = true;
            }
        }
        if census {
            views_census.push(vc);
        }
    }
    if census {
        let mut c = InstancedCensus {
            views: views_census,
            ..default()
        };
        c.views.sort_by(|a, b| a.view.cmp(&b.view));
        for (ci, chunk) in d.set.chunks.iter().enumerate() {
            if union_seen[ci] {
                for kind in 0..KINDS {
                    let n = chunk.ranges[kind].len() as u32;
                    c.union_instances_by_lod[lods[ci]] += n;
                    c.union_tris += n as u64 * d.tris[kind][lods[ci]] as u64;
                }
            }
        }
        *CENSUS.lock().unwrap() = Some(c);
    }
    if gpu.ids.is_empty() {
        gpu.ids.push(0);
    }
    gpu.ids.write_buffer(&device, &queue);
}

fn prepare_pieces_bind_group(
    mut gpu: ResMut<InstancedGpu>,
    pipeline: Option<Res<InstancedMainPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
) {
    let gpu = &mut *gpu;
    gpu.bind_group = None;
    let (Some(pipeline), Some(pieces), Some(palette), Some(ids)) = (
        pipeline,
        gpu.pieces.as_ref(),
        gpu.palette.as_ref(),
        gpu.ids.buffer(),
    ) else {
        return;
    };
    gpu.bind_group = Some(device.create_bind_group(
        "instanced_pieces",
        &pipeline_cache.get_bind_group_layout(&pipeline.pieces),
        &BindGroupEntries::sequential((
            pieces.as_entire_binding(),
            palette.as_entire_binding(),
            ids.as_entire_binding(),
        )),
    ));
}

/// Re-queues the 15 draw slots into every camera's opaque and prepass phases and every
/// directional cascade's shadow phase. Removing first keeps a slot from lingering under a
/// stale pipeline after a view's key changes.
#[allow(clippy::too_many_arguments)]
fn queue_pieces(
    groups: Query<(Entity, &MainEntity, &PieceGroup)>,
    scene: Option<Res<InstancedScene>>,
    main_pipeline: Option<Res<InstancedMainPipeline>>,
    prepass_pipeline: Option<Res<InstancedPrepassPipeline>>,
    mut main_pipelines: ResMut<SpecializedMeshPipelines<InstancedMainPipeline>>,
    mut prepass_pipelines: ResMut<SpecializedMeshPipelines<InstancedPrepassPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    render_meshes: Res<RenderAssets<RenderMesh>>,
    view_key_cache: Res<ViewKeyCache>,
    (opaque_draw, prepass_draw, shadow_draw): (
        Res<DrawFunctions<Opaque3d>>,
        Res<DrawFunctions<Opaque3dPrepass>>,
        Res<DrawFunctions<Shadow>>,
    ),
    (mut opaque_phases, mut prepass_phases, mut shadow_phases): (
        ResMut<ViewBinnedRenderPhases<Opaque3d>>,
        ResMut<ViewBinnedRenderPhases<Opaque3dPrepass>>,
        ResMut<ViewBinnedRenderPhases<Shadow>>,
    ),
    cameras: Query<
        (
            &ExtractedView,
            &Msaa,
            Has<NormalPrepass>,
            Has<MotionVectorPrepass>,
            Has<DepthPrepass>,
        ),
        With<ExtractedCamera>,
    >,
    lights: Query<(&ExtractedView, &LightEntity)>,
) {
    let (Some(scene), Some(main_pipeline), Some(prepass_pipeline)) =
        (scene, main_pipeline, prepass_pipeline)
    else {
        return;
    };
    let draw_main = opaque_draw.read().id::<DrawPiecesMain>();
    let draw_prepass = prepass_draw.read().id::<DrawPiecesPrepass>();
    let draw_shadow = shadow_draw.read().id::<DrawPiecesPrepass>();
    // Bins sort by key within a pipeline and the later draw wins an equal-depth pixel, so each
    // slot's bin is its (kind, LOD), not the mesh `AssetId`, whose index follows load order.
    let slots: Vec<(Entity, MainEntity, UntypedAssetId, &RenderMesh, PieceGroup)> = groups
        .iter()
        .filter_map(|(e, me, g)| {
            let id = scene.0.meshes[g.kind as usize][g.lod as usize];
            let bin = AssetId::<Mesh>::Uuid {
                uuid: Uuid::from_u128(g.slot() as u128),
            };
            render_meshes
                .get(id)
                .map(|m| (e, *me, bin.untyped(), m, *g))
        })
        .collect();
    let topo = |m: &RenderMesh| {
        MeshPipelineKey::from_primitive_topology_and_strip_index(
            m.primitive_topology(),
            m.index_format(),
        )
    };

    for (view, msaa, normal, motion, depth) in &cameras {
        let Some(&view_key) = view_key_cache.get(&view.retained_view_entity) else {
            continue;
        };
        if let Some(phase) = opaque_phases.get_mut(&view.retained_view_entity) {
            for &(e, me, bin, mesh, _) in &slots {
                phase.remove(me);
                let Ok(pipeline) = main_pipelines.specialize(
                    &pipeline_cache,
                    &main_pipeline,
                    view_key | topo(mesh),
                    &mesh.layout,
                ) else {
                    continue;
                };
                phase.add(
                    Opaque3dBatchSetKey {
                        pipeline,
                        draw_function: draw_main,
                        material_bind_group_index: None,
                        slabs: MeshSlabs::default(),
                        lightmap_slab: None,
                    },
                    Opaque3dBinKey { asset_id: bin },
                    (e, me),
                    InputUniformIndex::default(),
                    BinnedRenderPhaseType::NonMesh,
                );
            }
        }
        if let Some(phase) = prepass_phases.get_mut(&view.retained_view_entity) {
            let mut key = MeshPipelineKey::from_msaa_samples(msaa.samples());
            if normal {
                key |= MeshPipelineKey::NORMAL_PREPASS;
            }
            if motion {
                key |= MeshPipelineKey::MOTION_VECTOR_PREPASS;
            }
            if depth {
                key |= MeshPipelineKey::DEPTH_PREPASS;
            }
            for &(e, me, bin, mesh, _) in &slots {
                phase.remove(me);
                let Ok(pipeline) = prepass_pipelines.specialize(
                    &pipeline_cache,
                    &prepass_pipeline,
                    key | topo(mesh),
                    &mesh.layout,
                ) else {
                    continue;
                };
                phase.add(
                    OpaqueNoLightmap3dBatchSetKey {
                        pipeline,
                        draw_function: draw_prepass,
                        material_bind_group_index: None,
                        slabs: MeshSlabs::default(),
                    },
                    OpaqueNoLightmap3dBinKey { asset_id: bin },
                    (e, me),
                    InputUniformIndex::default(),
                    BinnedRenderPhaseType::NonMesh,
                );
            }
        }
    }

    for (view, light) in &lights {
        let LightEntity::Directional { .. } = light else {
            continue;
        };
        let Some(phase) = shadow_phases.get_mut(&view.retained_view_entity) else {
            continue;
        };
        let key = MeshPipelineKey::DEPTH_PREPASS | MeshPipelineKey::UNCLIPPED_DEPTH_ORTHO;
        for &(e, me, bin, mesh, g) in &slots {
            phase.remove(me);
            if !scene.0.casts[g.kind as usize][g.lod as usize] {
                continue;
            }
            let Ok(pipeline) = prepass_pipelines.specialize(
                &pipeline_cache,
                &prepass_pipeline,
                key | topo(mesh),
                &mesh.layout,
            ) else {
                continue;
            };
            phase.add(
                ShadowBatchSetKey {
                    pipeline,
                    draw_function: draw_shadow,
                    material_bind_group_index: None,
                    slabs: MeshSlabs::default(),
                },
                ShadowBinKey { asset_id: bin },
                (e, me),
                InputUniformIndex::default(),
                BinnedRenderPhaseType::NonMesh,
            );
        }
    }
}

type DrawPiecesMain = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshViewBindingArrayBindGroup<1>,
    DrawPieces,
);

type DrawPiecesPrepass = (
    SetItemPipeline,
    SetPrepassViewBindGroup<0>,
    SetPrepassViewEmptyBindGroup<1>,
    DrawPieces,
);

/// Binds the piece buffers at group 2 and draws this view's instances of one (kit mesh, LOD).
struct DrawPieces;

impl<P: PhaseItem> RenderCommand<P> for DrawPieces {
    type Param = (
        SRes<InstancedGpu>,
        SRes<RenderAssets<RenderMesh>>,
        SRes<MeshAllocator>,
    );
    type ViewQuery = Entity;
    type ItemQuery = Read<PieceGroup>;

    fn render<'w>(
        _item: &P,
        view: Entity,
        group: Option<&'w PieceGroup>,
        (gpu, meshes, allocator): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let (gpu, meshes, allocator) = (
            gpu.into_inner(),
            meshes.into_inner(),
            allocator.into_inner(),
        );
        let (Some(group), Some(scene), Some(bind_group)) =
            (group, gpu.scene.as_ref(), gpu.bind_group.as_ref())
        else {
            return RenderCommandResult::Skip;
        };
        let Some(range) = gpu.draws.get(&(view, group.slot())) else {
            return RenderCommandResult::Skip;
        };
        let id = scene.meshes[group.kind as usize][group.lod as usize];
        let (Some(mesh), Some(vertices)) = (meshes.get(id), allocator.mesh_vertex_slice(&id))
        else {
            return RenderCommandResult::Skip;
        };
        pass.set_bind_group(2, bind_group, &[]);
        pass.set_vertex_buffer(0, vertices.buffer.slice(..));
        match &mesh.buffer_info {
            RenderMeshBufferInfo::Indexed {
                index_format,
                count,
            } => {
                let Some(indices) = allocator.mesh_index_slice(&id) else {
                    return RenderCommandResult::Skip;
                };
                pass.set_index_buffer(indices.buffer.slice(..), *index_format);
                pass.draw_indexed(
                    indices.range.start..indices.range.start + count,
                    vertices.range.start as i32,
                    range.clone(),
                );
            }
            RenderMeshBufferInfo::NonIndexed => {
                pass.draw(vertices.range, range.clone());
            }
        }
        RenderCommandResult::Success
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workload::GATE;
    use hex_core::spiral;

    fn tris(meshes: &Assets<Mesh>, h: &Handle<Mesh>) -> u64 {
        meshes
            .get(h)
            .map_or(0, |m| m.indices().map_or(0, |i| i.len() / 3)) as u64
    }

    #[test]
    fn instances_match_the_entities_layout_at_every_lod() {
        let mut assets = Assets::<Mesh>::default();
        let m = PieceMeshes::new(&mut assets);
        let cells = spiral(GATE.tiles);
        for chunk in [7, 19] {
            let set = build_instances(&m, &assets, &cells, chunk);
            let kinds = m.kinds();

            // 40,000 pieces, each drawable at each of the three LODs.
            assert_eq!(set.pieces.len(), GATE.tiles * GATE.pieces_per_tile);
            let per_kind: Vec<u32> = (0..KINDS).map(|k| set.kind_count(k)).collect();
            assert_eq!(per_kind, vec![2_000, 12_000, 2_000, 12_000, 12_000]);
            assert_eq!(per_kind.iter().sum::<u32>(), 40_000);

            // Triangles per LOD equal what the entities layout spawns for that LOD.
            for lod in 0..3 {
                let want: u64 = (0..cells.len())
                    .flat_map(|ti| tile_pieces(&m, ti))
                    .map(|(lods, _)| tris(&assets, &lods[lod]))
                    .sum();
                let got: u64 = (0..KINDS)
                    .map(|k| set.kind_count(k) as u64 * tris(&assets, &kinds[k][lod]))
                    .sum();
                assert_eq!(got, want, "chunk {chunk} LOD{lod} triangles");
            }

            // Same (material, position) multiset as the entities layout, palette in 0..40.
            let mut want: Vec<(u32, [i64; 3])> = vec![];
            for (ti, cell) in cells.iter().enumerate() {
                let [x, z] = cell.to_world(HEX_RADIUS);
                for (pi, (_, t)) in tile_pieces(&m, ti).into_iter().enumerate() {
                    let p = (Transform::from_xyz(x, 0.0, z) * t).translation;
                    want.push((piece_material(ti, pi) as u32, key(p)));
                }
            }
            let mut got: Vec<(u32, [i64; 3])> = set
                .pieces
                .iter()
                .map(|p| (p.material, key(Vec3::from(p.translation))))
                .collect();
            assert!(got.iter().all(|(mat, _)| (*mat as usize) < GATE.materials));
            want.sort();
            got.sort();
            assert_eq!(got, want);
        }
    }

    fn key(p: Vec3) -> [i64; 3] {
        (p * 1000.0).round().as_i64vec3().to_array()
    }

    #[test]
    fn chunk_ranges_partition_each_kind_and_bound_their_pieces() {
        let mut assets = Assets::<Mesh>::default();
        let m = PieceMeshes::new(&mut assets);
        let cells = spiral(GATE.tiles);
        let set = build_instances(&m, &assets, &cells, 7);
        assert_eq!(set.chunks.iter().map(|c| c.tiles).sum::<u32>(), 2_000);
        let mut covered = vec![0u8; set.pieces.len()];
        for c in &set.chunks {
            let (lo, hi) = (Vec3::from(c.aabb.min()), Vec3::from(c.aabb.max()));
            for r in &c.ranges {
                for i in r.clone() {
                    covered[i as usize] += 1;
                    let p = Vec3::from(set.pieces[i as usize].translation);
                    assert!(p.cmpge(lo - 1e-3).all() && p.cmple(hi + 1e-3).all());
                }
            }
        }
        assert!(
            covered.iter().all(|&n| n == 1),
            "each piece in one chunk range"
        );
    }

    #[test]
    fn kind_casts_agree_with_the_per_piece_rule() {
        let mut assets = Assets::<Mesh>::default();
        let m = PieceMeshes::new(&mut assets);
        let kinds = m.kinds();
        for casters in [ShadowCasters::All, ShadowCasters::Large] {
            let table = kind_casts(casters);
            for ti in 0..5 {
                for (pi, (lods, _)) in tile_pieces(&m, ti).into_iter().enumerate() {
                    let kind = kinds.iter().position(|k| k[0] == lods[0]).unwrap();
                    for (lod, &cast) in table[kind].iter().enumerate() {
                        assert_eq!(
                            cast,
                            casts_shadow(casters, pi, lod),
                            "{casters:?} piece {pi} (kind {kind}) LOD{lod}"
                        );
                    }
                }
            }
        }
    }

    fn enclosing_frustum() -> Frustum {
        // Orthographic box of ±5 km with clip z in 0..1, enclosing the whole gate.
        let clip = Mat4::from_translation(Vec3::new(0.0, 0.0, 0.5))
            * Mat4::from_scale(Vec3::new(1.0 / 5_000.0, 1.0 / 5_000.0, 1.0 / 10_000.0));
        Frustum(ViewFrustum::from_clip_from_world(&clip))
    }

    #[test]
    fn view_ranges_draw_every_instance_once_split_by_chunk_lod() {
        let mut assets = Assets::<Mesh>::default();
        let m = PieceMeshes::new(&mut assets);
        let set = build_instances(&m, &assets, &spiral(GATE.tiles), 7);
        let lods: Vec<usize> = (0..set.chunks.len()).map(|i| i % 3).collect();
        let (mut ids, mut drawn) = (vec![], vec![]);
        let casts = kind_casts(ShadowCasters::All);
        let ranges = view_ranges(
            &set,
            &lods,
            &casts,
            &enclosing_frustum(),
            false,
            &mut ids,
            &mut drawn,
        );
        assert_eq!(ids.len(), 40_000);
        assert_eq!(drawn.len(), set.chunks.len());
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert!(
            sorted.iter().copied().eq(0..40_000u32),
            "each instance exactly once"
        );
        let mut by_lod = [0usize; 3];
        for (slot, r) in &ranges {
            by_lod[slot % 3] += r.len();
        }
        let mut want = [0usize; 3];
        for (ci, c) in set.chunks.iter().enumerate() {
            want[lods[ci]] += c.ranges.iter().map(|r| r.len()).sum::<usize>();
        }
        assert_eq!(by_lod, want);
        assert_eq!(by_lod.iter().sum::<usize>(), 40_000);
        assert!(ranges.len() <= KINDS * 3);
    }

    #[test]
    fn shadow_views_skip_non_casting_slots() {
        let mut assets = Assets::<Mesh>::default();
        let m = PieceMeshes::new(&mut assets);
        let set = build_instances(&m, &assets, &spiral(GATE.tiles), 7);
        let lods: Vec<usize> = (0..set.chunks.len()).map(|i| i % 3).collect();
        let casts = kind_casts(ShadowCasters::Large);
        let (mut ids, mut drawn) = (vec![], vec![]);
        let ranges = view_ranges(
            &set,
            &lods,
            &casts,
            &enclosing_frustum(),
            true,
            &mut ids,
            &mut drawn,
        );
        assert!(!ranges.is_empty());
        for (slot, _) in &ranges {
            assert!(casts[slot / 3][slot % 3], "slot {slot} does not cast");
        }
        let mut want = 0;
        for (ci, c) in set.chunks.iter().enumerate() {
            for (k, r) in c.ranges.iter().enumerate() {
                if casts[k][lods[ci]] {
                    want += r.len();
                }
            }
        }
        assert_eq!(ids.len(), want);
        assert!(want < 40_000);
        // The same view as a camera view draws everything.
        let (mut ids, mut drawn) = (vec![], vec![]);
        view_ranges(
            &set,
            &lods,
            &casts,
            &enclosing_frustum(),
            false,
            &mut ids,
            &mut drawn,
        );
        assert_eq!(ids.len(), 40_000);
    }

    #[test]
    fn chunk_lod_switches_at_150_and_400_m() {
        assert_eq!(chunk_lod(0.0), 0);
        assert_eq!(chunk_lod(149.9), 0);
        assert_eq!(chunk_lod(150.0), 1);
        assert_eq!(chunk_lod(399.9), 1);
        assert_eq!(chunk_lod(400.0), 2);
    }
}
