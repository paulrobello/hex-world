//! GPU-instanced settled kit parts (card 01a14675's D1 pivot): settled play-world kit parts draw
//! through one storage buffer of 48-byte records per world rebuild instead of one entity per
//! part. Chunk merging duplicated every part's vertices and transiently exceeded wasm32's 4 GiB
//! during the merge wave; instancing keeps memory at unique-piece size plus 48 B per instance.
//! The draw machinery adapts the gate's `instanced.rs` to real kit materials: slot entities
//! carry the `KitMaterial` binding (group 3) and are binned as `NonMesh` items, group 2 binds
//! the record + id buffers (`hex_render::kit_instances`), and the kit shaders' `KIT_INSTANCED`
//! def replaces the per-mesh uniform with the record.
use std::{collections::HashMap, ops::Range, sync::Arc};

use bevy::{
    camera::primitives::{Aabb, Frustum},
    core_pipeline::{
        core_3d::{
            AlphaMask3d, CORE_3D_DEPTH_FORMAT, Opaque3d, Opaque3dBatchSetKey, Opaque3dBinKey,
        },
        prepass::{
            AlphaMask3dPrepass, Opaque3dPrepass, OpaqueNoLightmap3dBatchSetKey,
            OpaqueNoLightmap3dBinKey, prepass_target_descriptors,
        },
    },
    ecs::system::{
        SystemParamItem,
        lifetimeless::{Read, SRes},
    },
    material::RenderPhaseType,
    math::Affine3A,
    pbr::{
        ExtractedDirectionalLight, LightEntity, MATERIAL_BIND_GROUP_INDEX, MeshPipeline,
        MeshPipelineKey, PreparedMaterial, PrepassPipeline, SetMeshViewBindGroup,
        SetMeshViewBindingArrayBindGroup, SetPrepassViewBindGroup, SetPrepassViewEmptyBindGroup,
        Shadow, ShadowBatchSetKey, ShadowBinKey, ViewKeyCache, ViewLightEntities,
        alpha_mode_pipeline_key,
    },
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        camera::ExtractedCamera,
        erased_render_asset::ErasedRenderAssets,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        material_bind_groups::MaterialBindGroupAllocators,
        mesh::allocator::{MeshAllocator, MeshSlabs},
        mesh::{MeshVertexBufferLayoutRef, RenderMesh, RenderMeshBufferInfo},
        render_asset::RenderAssets,
        render_phase::{
            AddRenderCommand, BinnedPhaseItem, BinnedRenderPhaseType, DrawFunctions,
            InputUniformIndex, PhaseItem, RenderCommand, RenderCommandResult, SetItemPipeline,
            TrackedRenderPass, ViewBinnedRenderPhases,
        },
        render_resource::{
            binding_types::{storage_buffer_read_only_sized, uniform_buffer_sized},
            *,
        },
        renderer::{RenderDevice, RenderQueue},
        settings::WgpuFeatures,
        sync_world::MainEntity,
        view::ExtractedView,
    },
    shader::ShaderDefVal,
    shape::ViewFrustum,
};
use bytemuck::{Pod, Zeroable};

use crate::kit_material::{KitMaterial, PREPASS_SHADER, SHADER, VERTEX_SHADER};

/// One settled kit part on the GPU: `world = translation · rotation · scale`. Layout matches
/// `hex_render::kit_instances`' `GpuKitInstance` (48 bytes, translation at 16, tag at 28).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct GpuKitInstance {
    pub rotation: [f32; 4],
    pub translation: [f32; 3],
    pub tag: u32,
    pub scale: [f32; 3],
    pub _pad: u32,
}

/// One slot's mesh metadata (decompress AABB + UV ranges), matching `hex_render::kit_instances`'
/// `KitMeshMeta` (64 bytes). One entry per (slot, LOD) draw, selected by a dynamic offset.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct KitGpuMeta {
    pub center: [f32; 3],
    pub _pad0: u32,
    pub half: [f32; 3],
    pub _pad1: u32,
    pub uv_a: [f32; 4],
    pub uv_b: [f32; 4],
}

/// Per-chunk culling data: the chunk hex centre, the world-space bounds of every instanced part
/// in it, and the instance range of each slot (`ranges[i]` is slot `i`'s).
#[derive(Clone, Debug)]
pub struct KitChunk {
    pub centre: Vec3,
    pub aabb: Aabb,
    pub ranges: Vec<Range<u32>>,
}

/// One draw slot: a piece mesh family × material. `mesh[l]` is the family's LOD-`l` mesh and
/// `key` carries the `StandardMaterialKey` bits the pipeline defs read.
#[derive(Clone, Debug)]
pub struct KitSlot {
    pub mesh: [Option<AssetId<Mesh>>; 3],
    /// Indices per LOD for the census/report.
    pub tris: [u32; 3],
    pub material: AssetId<KitMaterial>,
    /// `StandardMaterialKey` bits (cull mode, normal map, depth bias, ...).
    pub key: u64,
}

/// The whole world's instance data, rebuilt on each settle wave and swapped as one `Arc`.
#[derive(Clone, Debug, Default)]
pub struct KitInstanceSetData {
    pub pieces: Vec<GpuKitInstance>,
    pub slots: Vec<KitSlot>,
    pub chunks: Vec<KitChunk>,
    /// One `KitGpuMeta` per (slot, LOD) draw, in `slot * 3 + lod` order.
    pub meta: Vec<KitGpuMeta>,
    pub version: u64,
}

/// The main world's instance set, extracted to the render world by `Arc`.
#[derive(Resource, Clone, ExtractResource)]
#[extract_app(RenderApp)]
pub struct KitInstanceSet(pub Arc<KitInstanceSetData>);

/// One (slot, LOD) draw entity. Carries the `KitMaterial` binding; hex-play spawns them.
#[derive(Component, Clone, Copy, Debug, ExtractComponent)]
#[extract_app(RenderApp)]
pub struct KitSlotDraw {
    pub slot: u32,
    pub lod: u8,
}

impl KitSlotDraw {
    fn slot_index(self) -> usize {
        self.slot as usize * 3 + self.lod as usize
    }
}

/// The debug LOD overlays for the instanced path, which has no per-part entities for the
/// `VisibilityRange` force and the material-swap tint to act on: `force` pins every chunk to one
/// LOD, `tint` paints each draw in its LOD's debug colour. hex-play mirrors its overlays here.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, ExtractResource)]
#[extract_app(RenderApp)]
pub struct KitLodDebug {
    pub force: Option<u8>,
    pub tint: bool,
}

/// Culling/LOD thresholds, mirroring the per-entity `VisibilityRange` bands (`scene::lod_range`).
pub const LOD_NEAR: f32 = 150.0;
pub const LOD_FAR: f32 = 400.0;

/// LOD of a chunk `distance` metres from the camera.
pub fn kit_chunk_lod(distance: f32) -> usize {
    if distance < LOD_NEAR {
        0
    } else if distance < LOD_FAR {
        1
    } else {
        2
    }
}

/// Appends the visible instance ids to `ids`, grouped by draw slot (`slot * 3 + lod`), and
/// returns the non-empty slots' ranges into `ids`. `lods[ci]` is chunk `ci`'s LOD from the main
/// camera; shadow views reuse it, as `VisibilityRange` does.
pub fn kit_view_ranges(
    set: &KitInstanceSetData,
    lods: &[usize],
    frustum: &Frustum,
    shadow: bool,
    ids: &mut Vec<u32>,
) -> Vec<(usize, Range<u32>)> {
    let visible: Vec<usize> = (0..set.chunks.len())
        .filter(|&ci| {
            // Shadow casters in front of a cascade's near plane still cast into it.
            frustum.intersects_obb(&set.chunks[ci].aabb, &Affine3A::IDENTITY, !shadow, true)
        })
        .collect();
    let mut out = vec![];
    for slot in 0..set.slots.len() {
        for lod in 0..3usize {
            let first = ids.len() as u32;
            for &ci in &visible {
                if lods[ci] == lod
                    && let Some(r) = set.chunks[ci].ranges.get(slot)
                {
                    ids.extend(r.clone());
                }
            }
            let last = ids.len() as u32;
            if last > first {
                out.push((slot * 3 + lod, first..last));
            }
        }
    }
    out
}

/// The view depth (`-z` in the camera's view space) range of every point a caster inside
/// `lo..hi` can shadow, under a directional light shining along `dir`: the bounds grown by
/// `margin` and swept along `dir` down to their grown floor (light falls downward, so nothing
/// above the bounds or past the sweep is shadowed by them). `None` when the light is so near the
/// horizon that the sweep is unbounded.
pub fn shadowed_depth_range(
    lo: Vec3,
    hi: Vec3,
    dir: Vec3,
    view_from_world: &Mat4,
    margin: f32,
) -> Option<(f32, f32)> {
    if dir.y > -0.05 {
        return None;
    }
    let (lo, hi) = (lo - Vec3::splat(margin), hi + Vec3::splat(margin));
    let sweep = dir * ((hi.y - lo.y) / -dir.y);
    let (mut near, mut far) = (f32::MAX, f32::MIN);
    for i in 0..8 {
        let corner = Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
        for p in [corner, corner + sweep] {
            let depth = -view_from_world.transform_point3(p).z;
            near = near.min(depth);
            far = far.max(depth);
        }
    }
    Some((near, far))
}

/// Whether a fragment at a view depth inside `range` samples cascade `index`, by bevy_pbr's
/// `fetch_directional_shadow`: a fragment samples the first cascade whose far bound lies past
/// it, and inside the last `overlap` of that cascade it also blends in the next one. So cascade
/// `i` is read from `(1 - overlap) * bounds[i - 1]` (0 for the first) up to `bounds[i]`.
pub fn cascade_sampled(bounds: &[f32], overlap: f32, index: usize, range: (f32, f32)) -> bool {
    let Some(&far) = bounds.get(index) else {
        return true;
    };
    let near = match index {
        0 => f32::MIN,
        i => (1.0 - overlap) * bounds[i - 1],
    };
    range.1 >= near && range.0 < far
}

/// The shadow filter's reach past a caster's edge, in shadow-map texels of the cascade, plus a
/// fixed allowance (m) for the depth and normal biases.
const SHADOW_REACH_TEXELS: f32 = 8.0;
const SHADOW_REACH_M: f32 = 2.0;
// ---------------------------------------------------------------------------------------------
// Render world.

pub struct KitInstancesPlugin;

impl Plugin for KitInstancesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KitLodDebug>().add_plugins((
            ExtractResourcePlugin::<KitInstanceSet>::default(),
            ExtractResourcePlugin::<KitLodDebug>::default(),
            ExtractComponentPlugin::<KitSlotDraw>::default(),
        ));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<KitGpu>()
            .init_resource::<SpecializedMeshPipelines<KitMainPipeline>>()
            .init_resource::<SpecializedMeshPipelines<KitPrepassPipeline>>()
            .add_render_command::<Opaque3d, DrawKitMain>()
            .add_render_command::<AlphaMask3d, DrawKitMain>()
            .add_render_command::<Opaque3dPrepass, DrawKitPrepass>()
            .add_render_command::<AlphaMask3dPrepass, DrawKitPrepass>()
            .add_render_command::<Shadow, DrawKitPrepass>()
            .add_systems(
                Render,
                (
                    prepare_kit.in_set(RenderSystems::PrepareResources),
                    prepare_kit_bind_group.in_set(RenderSystems::PrepareBindGroups),
                    // The slot entities carry `MeshMaterial3d` only to resolve their bind group, so
                    // bevy's own queue systems treat a material edited this frame (the wet band,
                    // every frame) as a changed renderable and `remove` it from each phase. Queue
                    // after them, or a removal can follow this frame's `add` and drop the slot.
                    queue_kit
                        .in_set(RenderSystems::Queue)
                        .after(RenderSystems::QueueMeshes),
                ),
            );
    }
}

/// GPU state: the record buffer plus this frame's per-view draw ranges.
#[derive(Resource)]
struct KitGpu {
    scene: Option<Arc<KitInstanceSetData>>,
    pieces: Option<Buffer>,
    meta: Option<Buffer>,
    /// Byte stride between (slot, LOD) meta entries (device alignment).
    meta_stride: u32,
    /// The LOD tint the meta buffer was written with.
    meta_tint: bool,
    ids: RawBufferVec<u32>,
    // (`ids` holds the per-view record ids; it is bound as an instanced vertex buffer, not
    // storage — see kit_instances.wesl for why.)
    /// One static bind group per meta entry (no dynamic offsets).
    bind_groups: Vec<BindGroup>,
    /// (view render entity, slot index `slot * 3 + lod`) -> instance range into `ids`.
    draws: HashMap<(Entity, usize), Range<u32>>,
    /// Slot draws binned last frame. Bevy's binned phases retain items across frames until
    /// `remove`d, so each queue first unbins these or culled and despawned slots stay binned.
    queued: Vec<MainEntity>,
}

impl Default for KitGpu {
    fn default() -> Self {
        Self {
            scene: None,
            pieces: None,
            meta: None,
            meta_stride: 0,
            meta_tint: false,
            ids: RawBufferVec::new(BufferUsages::VERTEX),
            bind_groups: Vec::new(),
            draws: HashMap::default(),
            queued: Vec::new(),
        }
    }
}

/// Removes `stale` from every view's `phases`.
fn unqueue<BPI: BinnedPhaseItem>(phases: &mut ViewBinnedRenderPhases<BPI>, stale: &[MainEntity]) {
    for phase in phases.values_mut() {
        for &me in stale {
            phase.remove(me);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Pipelines.

/// The `StandardMaterial` shader defs my pipelines must reproduce (the per-entity path gets them
/// from `StandardMaterial::specialize`); read off the pinned bevy_pbr-0.20.0 `pbr_material.rs`.
const MATERIAL_DEFS: &[(u64, &str)] = &[
    (
        0x000004, // StandardMaterialKey::NORMAL_MAP
        "STANDARD_MATERIAL_NORMAL_MAP",
    ),
    (
        0x000008, // RELIEF_MAPPING
        "RELIEF_MAPPING",
    ),
    (
        0x000010, // DIFFUSE_TRANSMISSION
        "STANDARD_MATERIAL_DIFFUSE_TRANSMISSION",
    ),
    (
        0x000020, // SPECULAR_TRANSMISSION
        "STANDARD_MATERIAL_SPECULAR_TRANSMISSION",
    ),
    (0x000040, "STANDARD_MATERIAL_CLEARCOAT"),
    (0x000080, "STANDARD_MATERIAL_CLEARCOAT_NORMAL_MAP"),
    (0x000100, "STANDARD_MATERIAL_ANISOTROPY"),
    (0x000200, "STANDARD_MATERIAL_BASE_COLOR_UV_B"),
    (0x000400, "STANDARD_MATERIAL_EMISSIVE_UV_B"),
    (0x000800, "STANDARD_MATERIAL_METALLIC_ROUGHNESS_UV_B"),
    (0x001000, "STANDARD_MATERIAL_OCCLUSION_UV_B"),
    (0x002000, "STANDARD_MATERIAL_SPECULAR_TRANSMISSION_UV_B"),
    (0x004000, "STANDARD_MATERIAL_THICKNESS_UV_B"),
    (0x008000, "STANDARD_MATERIAL_DIFFUSE_TRANSMISSION_UV_B"),
    (0x0000_1000 << 4, "STANDARD_MATERIAL_NORMAL_MAP_UV_B"),
    (0x0000_2000 << 4, "STANDARD_MATERIAL_ANISOTROPY_UV_B"),
    (0x0000_4000 << 4, "STANDARD_MATERIAL_CLEARCOAT_UV_B"),
    (
        0x0000_8000 << 4,
        "STANDARD_MATERIAL_CLEARCOAT_ROUGHNESS_UV_B",
    ),
    (0x0001_0000 << 4, "STANDARD_MATERIAL_CLEARCOAT_NORMAL_UV_B"),
    (0x0002_0000 << 4, "STANDARD_MATERIAL_SPECULAR_UV_B"),
    (0x0004_0000 << 4, "STANDARD_MATERIAL_SPECULAR_TINT_UV_B"),
];

fn push_material_defs(defs: &mut Vec<ShaderDefVal>, key: u64, mesh_key: MeshPipelineKey) {
    for &(flags, def) in MATERIAL_DEFS {
        if matches!(
            flags,
            0x000020 | 0x002000 // SPECULAR_TRANSMISSION, SPECULAR_TRANSMISSION_UV
        ) && !mesh_key.contains(MeshPipelineKey::VIEW_TRANSMISSION_TEXTURE)
        {
            continue;
        }
        if key & flags != 0 {
            defs.push(def.into());
        }
    }
}

fn cull_mode(key: u64) -> Option<Face> {
    match key & 0b11 {
        0b01 => Some(Face::Front),
        0b10 => Some(Face::Back),
        _ => None,
    }
}

fn depth_bias_of(key: u64) -> i32 {
    (key >> 32) as u32 as i32
}

/// Pipeline key: the mesh key (view + mesh + material alpha bits) plus the material key bits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct KitKey {
    mesh: MeshPipelineKey,
    mat: u64,
}

#[derive(Resource, Clone)]
struct KitMainPipeline {
    mesh: MeshPipeline,
    instances: BindGroupLayoutDescriptor,
    material: BindGroupLayoutDescriptor,
    vertex: Handle<Shader>,
    fragment: Handle<Shader>,
}

impl SpecializedMeshPipeline for KitMainPipeline {
    type Key = KitKey;

    fn specialize(
        &self,
        key: Self::Key,
        layout: &MeshVertexBufferLayoutRef,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let mut d = self.mesh.specialize(key.mesh, layout)?;
        push_material_defs(&mut d.vertex.shader_defs, key.mat, key.mesh);
        if let Some(f) = d.fragment.as_mut() {
            push_material_defs(&mut f.shader_defs, key.mat, key.mesh);
            f.shader_defs.push(ShaderDefVal::UInt(
                "MATERIAL_BIND_GROUP".into(),
                MATERIAL_BIND_GROUP_INDEX as u32,
            ));
            f.shader_defs.push("KIT_INSTANCED".into());
            f.shader_defs.push("VERTEX_OUTPUT_INSTANCE_INDEX".into());
        }
        d.vertex.shader_defs.push(ShaderDefVal::UInt(
            "MATERIAL_BIND_GROUP".into(),
            MATERIAL_BIND_GROUP_INDEX as u32,
        ));
        d.vertex.shader_defs.push("KIT_INSTANCED".into());
        d.vertex
            .shader_defs
            .push("VERTEX_OUTPUT_INSTANCE_INDEX".into());
        d.vertex.shader = self.vertex.clone();
        d.vertex.entry_point = Some("vertex".into());
        let f = d.fragment.as_mut().expect("main pass has a fragment");
        f.shader = self.fragment.clone();
        f.entry_point = Some("fragment".into());
        d.layout[2] = self.instances.clone();
        d.layout.insert(3, self.material.clone());
        // The instanced u32 record-id buffer: one u32 per instance, sliced per draw. Vertex
        // attributes carry no instance-index semantics, so the record lookup cannot drift
        // between Metal's absolute `[[instance_id]]` and WebGPU's 0-based `instance_index`.
        let mut record = d.vertex.buffers[0].clone();
        record.array_stride = 4;
        record.step_mode = VertexStepMode::Instance;
        let mut attr = record.attributes[0];
        attr.format = VertexFormat::Uint32;
        attr.offset = 0;
        attr.shader_location = 6;
        record.attributes = vec![attr];
        d.vertex.buffers.push(record);
        d.primitive.cull_mode = cull_mode(key.mat);
        if let Some(ds) = d.depth_stencil.as_mut() {
            ds.bias.constant = depth_bias_of(key.mat);
        }
        d.label = Some("kit_instances_main".into());
        Ok(d)
    }
}
#[derive(Resource, Clone)]
struct KitPrepassPipeline {
    prepass: PrepassPipeline,
    instances: BindGroupLayoutDescriptor,
    material: BindGroupLayoutDescriptor,
    shader: Handle<Shader>,
    depth_clip_control: bool,
}

impl SpecializedMeshPipeline for KitPrepassPipeline {
    type Key = KitKey;

    /// Mirrors bevy_pbr-0.20.0 `PrepassPipeline::specialize` for an opaque/masked, non-skinned,
    /// non-morphed, non-lightmapped, non-deferred, non-compressed kit mesh, with the mesh-uniform
    /// group replaced by the instance buffers. Pinned upstream, so this cannot drift silently.
    fn specialize(
        &self,
        key: Self::Key,
        layout: &MeshVertexBufferLayoutRef,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let mesh_key = key.mesh;
        let normal = mesh_key.contains(MeshPipelineKey::NORMAL_PREPASS);
        let motion = mesh_key.contains(MeshPipelineKey::MOTION_VECTOR_PREPASS);
        let depth_only = mesh_key.contains(MeshPipelineKey::DEPTH_PREPASS);
        let ortho_unclipped = mesh_key.contains(MeshPipelineKey::UNCLIPPED_DEPTH_ORTHO);
        let emulate = ortho_unclipped && !self.depth_clip_control;
        let mut defs: Vec<ShaderDefVal> = vec![
            "PREPASS_PIPELINE".into(),
            ShaderDefVal::UInt(
                "MATERIAL_BIND_GROUP".into(),
                MATERIAL_BIND_GROUP_INDEX as u32,
            ),
            "KIT_INSTANCED".into(),
            "VERTEX_OUTPUT_INSTANCE_INDEX".into(),
        ];
        push_material_defs(&mut defs, key.mat, mesh_key);
        let vp = mesh_key.intersection(MeshPipelineKey::VIEW_PROJECTION_RESERVED_BITS);
        if vp == MeshPipelineKey::VIEW_PROJECTION_NONSTANDARD {
            defs.push("VIEW_PROJECTION_NONSTANDARD".into());
        } else if vp == MeshPipelineKey::VIEW_PROJECTION_PERSPECTIVE {
            defs.push("VIEW_PROJECTION_PERSPECTIVE".into());
        } else if vp == MeshPipelineKey::VIEW_PROJECTION_ORTHOGRAPHIC {
            defs.push("VIEW_PROJECTION_ORTHOGRAPHIC".into());
        }
        if depth_only {
            defs.push("DEPTH_PREPASS".into());
        }
        if mesh_key.contains(MeshPipelineKey::MAY_DISCARD) {
            defs.push("MAY_DISCARD".into());
        }
        if normal {
            defs.push("NORMAL_PREPASS".into());
            defs.push("NORMAL_PREPASS_OR_DEFERRED_PREPASS".into());
        }
        // Attribute-driven defs + the vertex buffer layout, in bevy's location order.
        let mut attributes = vec![];
        if layout.0.contains(Mesh::ATTRIBUTE_POSITION) {
            defs.push("VERTEX_POSITIONS".into());
            attributes.push(Mesh::ATTRIBUTE_POSITION.at_shader_location(0));
        }
        if layout.0.contains(Mesh::ATTRIBUTE_UV_0) {
            defs.push("VERTEX_UVS".into());
            defs.push("VERTEX_UVS_A".into());
            attributes.push(Mesh::ATTRIBUTE_UV_0.at_shader_location(1));
        }
        if layout.0.contains(Mesh::ATTRIBUTE_UV_1) {
            defs.push("VERTEX_UVS".into());
            defs.push("VERTEX_UVS_B".into());
            attributes.push(Mesh::ATTRIBUTE_UV_1.at_shader_location(2));
        }
        if normal && layout.0.contains(Mesh::ATTRIBUTE_NORMAL) {
            defs.push("VERTEX_NORMALS".into());
            attributes.push(Mesh::ATTRIBUTE_NORMAL.at_shader_location(3));
        }
        if normal && layout.0.contains(Mesh::ATTRIBUTE_TANGENT) {
            defs.push("VERTEX_TANGENTS".into());
            attributes.push(Mesh::ATTRIBUTE_TANGENT.at_shader_location(4));
        }
        if layout.0.contains(Mesh::ATTRIBUTE_COLOR) {
            defs.push("VERTEX_COLORS".into());
            attributes.push(Mesh::ATTRIBUTE_COLOR.at_shader_location(7));
        }
        if motion {
            defs.push("MOTION_VECTOR_PREPASS".into());
            defs.push("MOTION_VECTOR_PREPASS_OR_DEFERRED_PREPASS".into());
        }
        if normal || motion {
            defs.push("PREPASS_FRAGMENT".into());
        }
        if emulate {
            defs.push("UNCLIPPED_DEPTH_ORTHO_EMULATION".into());
            defs.push("PREPASS_FRAGMENT".into());
        }
        let vertex_layout = layout.0.get_layout(&attributes)?;
        let mut targets = prepass_target_descriptors(normal, motion, false);
        if targets.iter().all(Option::is_none) {
            targets.clear();
        }
        let fragment_needed =
            !targets.is_empty() || emulate || mesh_key.contains(MeshPipelineKey::MAY_DISCARD);
        let fragment = fragment_needed.then(|| FragmentState {
            shader: self.shader.clone(),
            shader_defs: defs.clone(),
            entry_point: Some("fragment".into()),
            targets,
            ..default()
        });
        let d = RenderPipelineDescriptor {
            label: Some("kit_instances_prepass".into()),
            layout: vec![
                if motion {
                    self.prepass.view_layout_motion_vectors.clone()
                } else {
                    self.prepass.view_layout_no_motion_vectors.clone()
                },
                self.prepass.empty_layout.clone(),
                self.instances.clone(),
                self.material.clone(),
            ],
            vertex: VertexState {
                shader: self.shader.clone(),
                shader_defs: defs,
                entry_point: Some("vertex".into()),
                // Instanced u32 record-id buffer; see the main-pass specialize for why.
                buffers: {
                    let mut record = vertex_layout.clone();
                    record.array_stride = 4;
                    record.step_mode = VertexStepMode::Instance;
                    let mut attr = record.attributes[0];
                    attr.format = VertexFormat::Uint32;
                    attr.offset = 0;
                    attr.shader_location = 6;
                    record.attributes = vec![attr];
                    vec![vertex_layout, record]
                },
                ..default()
            },
            fragment,
            primitive: PrimitiveState {
                topology: mesh_key.primitive_topology(),
                strip_index_format: mesh_key.strip_index_format(),
                unclipped_depth: ortho_unclipped && self.depth_clip_control,
                cull_mode: cull_mode(key.mat),
                ..default()
            },
            depth_stencil: Some(DepthStencilState {
                format: CORE_3D_DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(CompareFunction::GreaterEqual),
                stencil: StencilState::default(),
                bias: DepthBiasState {
                    constant: depth_bias_of(key.mat),
                    ..default()
                },
            }),
            multisample: MultisampleState {
                count: mesh_key.msaa_samples(),
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            ..default()
        };
        Ok(d)
    }
}
// ---------------------------------------------------------------------------------------------
// Prepare systems.

/// Builds the pipelines and uploads the record buffer the first time a set arrives (and whenever
/// its `Arc` swaps). Reads the kit material's layout from its first prepared instance.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn prepare_kit(
    mut commands: Commands,
    scene: Option<Res<KitInstanceSet>>,
    mut gpu: ResMut<KitGpu>,
    mesh_pipeline: Option<Res<MeshPipeline>>,
    prepass_pipeline: Option<Res<PrepassPipeline>>,
    server: Res<AssetServer>,
    device: Res<RenderDevice>,
    materials: Option<Res<ErasedRenderAssets<PreparedMaterial>>>,
    debug: Option<Res<KitLodDebug>>,
) {
    let Some(scene) = scene else {
        return;
    };
    let tint = debug.is_some_and(|d| d.tint);
    let changed = gpu.scene.as_ref().is_none_or(|s| !Arc::ptr_eq(s, &scene.0));
    if !changed {
        if tint != gpu.meta_tint && gpu.meta.is_some() {
            gpu.meta = Some(meta_buffer(&device, &scene.0.meta, gpu.meta_stride, tint));
            gpu.meta_tint = tint;
        }
        return;
    }
    gpu.scene = Some(scene.0.clone());
    let Some(slot) = scene.0.slots.first() else {
        return;
    };
    let Some(materials) = materials else {
        return;
    };
    let Some(prepared) = materials.get(slot.material.untyped()) else {
        return;
    };
    let instances = BindGroupLayoutDescriptor::new(
        "kit_instances_group2",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::VERTEX_FRAGMENT,
            (
                storage_buffer_read_only_sized(false, None),
                uniform_buffer_sized(false, Some(std::num::NonZeroU64::new(64).unwrap())),
            ),
        ),
    );
    let material_layout = prepared
        .properties
        .material_layout
        .as_ref()
        .expect("kit layout");
    commands.insert_resource(KitMainPipeline {
        mesh: mesh_pipeline.expect("MeshPipeline").clone(),
        instances: instances.clone(),
        material: material_layout.clone(),
        vertex: server.load(VERTEX_SHADER),
        fragment: server.load(SHADER),
    });
    commands.insert_resource(KitPrepassPipeline {
        prepass: prepass_pipeline.expect("PrepassPipeline").clone(),
        instances,
        material: material_layout.clone(),
        shader: server.load(PREPASS_SHADER),
        depth_clip_control: device.features().contains(WgpuFeatures::DEPTH_CLIP_CONTROL),
    });
    // The record buffer is rebuilt whenever the set swaps. The floor keeps the binding at or
    // above the shader's runtime-array minimum on near-empty worlds (wgpu validates the bound
    // size, and this app treats validation errors as fatal).
    let mut records = bytemuck::cast_slice(&scene.0.pieces).to_vec();
    records.resize(records.len().max(256), 0);
    gpu.pieces = Some(device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("kit_instances_records"),
        contents: &records,
        usage: BufferUsages::STORAGE,
    }));
    // One metadata entry per (slot, LOD), padded to the device's dynamic-offset alignment.
    let align = device.limits().min_uniform_buffer_offset_alignment;
    gpu.meta_stride = 64u32.max(align);
    gpu.meta = Some(meta_buffer(&device, &scene.0.meta, gpu.meta_stride, tint));
    gpu.meta_tint = tint;
}

/// The (slot, LOD) metadata entries at `stride`. Entry `slot * 3 + lod` carries the debug LOD
/// tint in its pad words: `_pad0` 1 when tinting, `_pad1` the LOD.
fn meta_buffer(device: &RenderDevice, meta: &[KitGpuMeta], stride: u32, tint: bool) -> Buffer {
    let stride = stride as usize;
    let mut bytes = vec![0u8; stride * meta.len()];
    for (i, m) in meta.iter().enumerate() {
        let mut m = *m;
        m._pad0 = u32::from(tint);
        m._pad1 = (i % 3) as u32;
        let src = bytemuck::bytes_of(&m);
        bytes[i * stride..i * stride + src.len()].copy_from_slice(src);
    }
    device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("kit_instances_meta"),
        contents: &bytes,
        usage: BufferUsages::UNIFORM,
    })
}
// ---------------------------------------------------------------------------------------------
// Queue.

/// Binds the record + id buffers at group 2.
fn prepare_kit_bind_group(
    mut gpu: ResMut<KitGpu>,
    main: Option<Res<KitMainPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
) {
    let (Some(pipeline), Some(pieces), Some(meta)) = (main, gpu.pieces.as_ref(), gpu.meta.as_ref())
    else {
        return;
    };
    let layout = pipeline_cache.get_bind_group_layout(&pipeline.instances);
    let entries = |offset: u64| {
        [
            BindGroupEntry {
                binding: 0,
                resource: pieces.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Buffer(BufferBinding {
                    buffer: meta,
                    offset,
                    size: Some(std::num::NonZeroU64::new(64).unwrap()),
                }),
            },
        ]
    };
    let count = (meta.size() / gpu.meta_stride as u64) as usize;
    gpu.bind_groups = (0..count)
        .map(|i| {
            device.create_bind_group(
                "kit_instances_group2",
                &layout,
                &entries(i as u64 * gpu.meta_stride as u64),
            )
        })
        .collect();
}
// ---------------------------------------------------------------------------------------------
// Queue.

/// Queues the slot draws: culls chunks per view, writes the per-view id ranges, and bins every
/// non-empty (slot, LOD) into the view's opaque or alpha-mask phase, its prepass, and the shadow
/// cascades. Masked slots go to `AlphaMask3d`/`AlphaMask3dPrepass`, as `RenderPhaseType` decides.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn queue_kit(
    groups: Query<(Entity, &MainEntity, &KitSlotDraw)>,
    scene: Option<Res<KitInstanceSet>>,
    main: Option<Res<KitMainPipeline>>,
    prepass: Option<Res<KitPrepassPipeline>>,
    mut main_pipelines: ResMut<SpecializedMeshPipelines<KitMainPipeline>>,
    mut prepass_pipelines: ResMut<SpecializedMeshPipelines<KitPrepassPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    render_meshes: Res<RenderAssets<RenderMesh>>,
    materials: Res<ErasedRenderAssets<PreparedMaterial>>,
    view_key_cache: Res<ViewKeyCache>,
    (opaque_draw, mask_draw, prepass_draw, mask_prepass_draw, shadow_draw): (
        Res<DrawFunctions<Opaque3d>>,
        Res<DrawFunctions<AlphaMask3d>>,
        Res<DrawFunctions<Opaque3dPrepass>>,
        Res<DrawFunctions<AlphaMask3dPrepass>>,
        Res<DrawFunctions<Shadow>>,
    ),
    (
        mut opaque_phases,
        mut mask_phases,
        mut prepass_phases,
        mut mask_prepass_phases,
        mut shadow_phases,
    ): (
        ResMut<ViewBinnedRenderPhases<Opaque3d>>,
        ResMut<ViewBinnedRenderPhases<AlphaMask3d>>,
        ResMut<ViewBinnedRenderPhases<Opaque3dPrepass>>,
        ResMut<ViewBinnedRenderPhases<AlphaMask3dPrepass>>,
        ResMut<ViewBinnedRenderPhases<Shadow>>,
    ),
    cameras: Query<
        (Entity, &ExtractedView, &Msaa, Option<&ViewLightEntities>),
        With<ExtractedCamera>,
    >,
    (lights, sun): (
        Query<(Entity, &ExtractedView, &LightEntity)>,
        Query<&ExtractedDirectionalLight>,
    ),
    (device, queue, debug): (
        Res<RenderDevice>,
        Res<RenderQueue>,
        Option<Res<KitLodDebug>>,
    ),
    mut gpu: ResMut<KitGpu>,
) {
    let stale = std::mem::take(&mut gpu.queued);
    unqueue(&mut opaque_phases, &stale);
    unqueue(&mut mask_phases, &stale);
    unqueue(&mut prepass_phases, &stale);
    unqueue(&mut mask_prepass_phases, &stale);
    unqueue(&mut shadow_phases, &stale);
    let (Some(scene), Some(main), Some(prepass)) = (scene, main, prepass) else {
        return;
    };
    let draw_main = opaque_draw.read().id::<DrawKitMain>();
    let draw_mask = mask_draw.read().id::<DrawKitMain>();
    // Draw-function ids index each phase's own registry: an `Opaque3dPrepass` id queued into
    // `AlphaMask3dPrepass` names some other function there, and the masked slots never draw.
    let draw_prepass = prepass_draw.read().id::<DrawKitPrepass>();
    let draw_mask_prepass = mask_prepass_draw.read().id::<DrawKitPrepass>();
    let draw_shadow = shadow_draw.read().id::<DrawKitPrepass>();
    // One queue-side record per (slot, LOD) draw: the render mesh for the key/topology and the
    // material's pipeline key bits.
    struct Draw {
        entity: Entity,
        main_entity: MainEntity,
        slot_idx: usize,
        mesh: AssetId<Mesh>,
        topo: MeshPipelineKey,
        bits: MeshPipelineKey,
        key: u64,
        mask: bool,
    }
    let mut draws: Vec<Draw> = vec![];
    for (e, me, g) in &groups {
        let Some(slot) = scene.0.slots.get(g.slot as usize) else {
            continue;
        };
        let Some(mesh) = slot.mesh[g.lod as usize] else {
            continue;
        };
        let Some(mesh) = render_meshes.get(mesh) else {
            continue;
        };
        let Some(prepared) = materials.get(slot.material.untyped()) else {
            continue;
        };
        // As bevy's `queue_material_meshes` does: the alpha mode's key (MAY_DISCARD for Mask, so
        // the alpha-cutoff foliage discards) is added per draw, not carried in the material bits.
        let mut bits: MeshPipelineKey = prepared.properties.mesh_pipeline_key_bits.downcast();
        bits.insert(alpha_mode_pipeline_key(
            prepared.properties.alpha_mode,
            &Msaa::Off,
        ));
        draws.push(Draw {
            entity: e,
            main_entity: *me,
            slot_idx: g.slot as usize * 3 + g.lod as usize,
            mesh: slot.mesh[g.lod as usize].expect("checked above"),
            topo: MeshPipelineKey::from_primitive_topology_and_strip_index(
                mesh.primitive_topology(),
                mesh.index_format(),
            ),
            bits,
            key: slot.key,
            mask: matches!(
                prepared.properties.render_phase_type,
                RenderPhaseType::AlphaMask
            ),
        });
    }
    if draws.is_empty() {
        return;
    }
    let d = scene.0.clone();
    gpu.draws.clear();
    gpu.ids.clear();
    let Some(camera) = cameras.iter().next() else {
        return;
    };
    // LOD is per chunk from the main camera, in every view (as `VisibilityRange` did per entity).
    // A forced debug LOD pins every chunk, as it pins every per-entity part's range.
    let eye = camera.1.world_from_view.translation();
    let force = debug.and_then(|d| d.force).map(|l| (l as usize).min(2));
    let lods: Vec<usize> = d
        .chunks
        .iter()
        .map(|c| force.unwrap_or_else(|| kit_chunk_lod(eye.distance(c.centre))))
        .collect();
    // Shadow cascades whose shadow map no fragment the kit can shadow reads: the kit draws
    // nothing into them, which leaves every shaded pixel as it was (a strategic camera's world
    // lies past the first cascades and short of the last).
    let mut unsampled: Vec<Entity> = vec![];
    let bounds = d
        .chunks
        .iter()
        .filter(|c| c.aabb.half_extents.cmpge(Vec3A::ZERO).all())
        .map(|c| (c.aabb.min(), c.aabb.max()))
        .reduce(|(lo, hi), (l, h)| (lo.min(l), hi.max(h)));
    for (ve, view, _, cascade_views) in &cameras {
        let (Some((lo, hi)), Some(cascade_views)) = (bounds, cascade_views) else {
            continue;
        };
        let view_from_world = view.world_from_view.to_matrix().inverse();
        for &cascade_view in &cascade_views.lights {
            let Ok((
                _,
                _,
                LightEntity::Directional {
                    light_entity,
                    cascade_index,
                },
            )) = lights.get(cascade_view)
            else {
                continue;
            };
            // Volumetric lighting samples the cascades along each view ray, not by surface depth.
            let Ok(light) = sun.get(*light_entity) else {
                continue;
            };
            if light.volumetric {
                continue;
            }
            let Some(cascade) = light.cascades.get(&ve).and_then(|c| c.get(*cascade_index)) else {
                continue;
            };
            let margin = SHADOW_REACH_M + SHADOW_REACH_TEXELS * cascade.texel_size;
            let dir = light.transform.forward().as_vec3();
            let Some(range) =
                shadowed_depth_range(lo.into(), hi.into(), dir, &view_from_world, margin)
            else {
                continue;
            };
            let config = &light.cascade_shadow_config;
            if !cascade_sampled(
                &config.bounds,
                config.overlap_proportion,
                *cascade_index,
                range,
            ) {
                unsampled.push(cascade_view);
            }
        }
    }
    for (ve, view, msaa, _) in &cameras {
        let Some(&view_key) = view_key_cache.get(&view.retained_view_entity) else {
            continue;
        };
        // Chunk LOD replaces per-instance visibility ranges on this path.
        let view_key = view_key - MeshPipelineKey::VISIBILITY_RANGE_DITHER;
        let clip_from_world = view.clip_from_view * view.world_from_view.to_matrix().inverse();
        let frustum = Frustum(ViewFrustum::from_clip_from_world(&clip_from_world));
        let ranges = kit_view_ranges(&d, &lods, &frustum, false, gpu.ids.values_mut());
        for (slot_idx, r) in &ranges {
            gpu.draws.insert((ve, *slot_idx), r.clone());
        }
        // Main pass: opaque and alpha-mask phases.
        if let Some(phase) = opaque_phases.get_mut(&view.retained_view_entity) {
            for dr in &draws {
                if !dr.mask && gpu.draws.contains_key(&(ve, dr.slot_idx)) {
                    let Ok(pipeline) = main_pipelines.specialize(
                        &pipeline_cache,
                        &main,
                        KitKey {
                            mesh: view_key | dr.topo | dr.bits,
                            mat: dr.key,
                        },
                        &render_meshes.get(dr.mesh).expect("mesh").layout,
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
                        Opaque3dBinKey {
                            asset_id: dr.mesh.untyped(),
                        },
                        (dr.entity, dr.main_entity),
                        InputUniformIndex::default(),
                        BinnedRenderPhaseType::NonMesh,
                    );
                }
            }
        }
        if let Some(phase) = mask_phases.get_mut(&view.retained_view_entity) {
            for dr in &draws {
                if dr.mask && gpu.draws.contains_key(&(ve, dr.slot_idx)) {
                    let Ok(pipeline) = main_pipelines.specialize(
                        &pipeline_cache,
                        &main,
                        KitKey {
                            mesh: view_key | dr.topo | dr.bits,
                            mat: dr.key,
                        },
                        &render_meshes.get(dr.mesh).expect("mesh").layout,
                    ) else {
                        continue;
                    };
                    phase.add(
                        OpaqueNoLightmap3dBatchSetKey {
                            pipeline,
                            draw_function: draw_mask,
                            material_bind_group_index: None,
                            slabs: MeshSlabs::default(),
                        },
                        OpaqueNoLightmap3dBinKey {
                            asset_id: dr.mesh.untyped(),
                        },
                        (dr.entity, dr.main_entity),
                        InputUniformIndex::default(),
                        BinnedRenderPhaseType::NonMesh,
                    );
                }
            }
        }
        // Prepass phases for cameras (normals/motion/depth).
        if let Some(phase) = prepass_phases.get_mut(&view.retained_view_entity) {
            for dr in &draws {
                if !dr.mask && gpu.draws.contains_key(&(ve, dr.slot_idx)) {
                    let Ok(pipeline) = prepass_pipelines.specialize(
                        &pipeline_cache,
                        &prepass,
                        KitKey {
                            mesh: MeshPipelineKey::from_msaa_samples(msaa.samples())
                                | (view_key & MeshPipelineKey::ALL_PREPASS_BITS)
                                | dr.topo
                                | dr.bits,
                            mat: dr.key,
                        },
                        &render_meshes.get(dr.mesh).expect("mesh").layout,
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
                        OpaqueNoLightmap3dBinKey {
                            asset_id: dr.mesh.untyped(),
                        },
                        (dr.entity, dr.main_entity),
                        InputUniformIndex::default(),
                        BinnedRenderPhaseType::NonMesh,
                    );
                }
            }
        }
        // Alpha-mask prepass: the same prepass phase machinery, mask phase only.
        if let Some(phase) = mask_prepass_phases.get_mut(&view.retained_view_entity) {
            for dr in &draws {
                if dr.mask && gpu.draws.contains_key(&(ve, dr.slot_idx)) {
                    let Ok(pipeline) = prepass_pipelines.specialize(
                        &pipeline_cache,
                        &prepass,
                        KitKey {
                            mesh: MeshPipelineKey::from_msaa_samples(msaa.samples())
                                | (view_key & MeshPipelineKey::ALL_PREPASS_BITS)
                                | dr.topo
                                | dr.bits,
                            mat: dr.key,
                        },
                        &render_meshes.get(dr.mesh).expect("mesh").layout,
                    ) else {
                        continue;
                    };
                    phase.add(
                        OpaqueNoLightmap3dBatchSetKey {
                            pipeline,
                            draw_function: draw_mask_prepass,
                            material_bind_group_index: None,
                            slabs: MeshSlabs::default(),
                        },
                        OpaqueNoLightmap3dBinKey {
                            asset_id: dr.mesh.untyped(),
                        },
                        (dr.entity, dr.main_entity),
                        InputUniformIndex::default(),
                        BinnedRenderPhaseType::NonMesh,
                    );
                }
            }
        }
    }
    for (ve, view, light) in &lights {
        let LightEntity::Directional { .. } = light else {
            continue;
        };
        if unsampled.contains(&ve) {
            continue;
        }
        let Some(phase) = shadow_phases.get_mut(&view.retained_view_entity) else {
            continue;
        };
        // Shadow views cull in their own frustum; cascade LODs follow the main camera's.
        let clip_from_world = view.clip_from_view * view.world_from_view.to_matrix().inverse();
        let frustum = Frustum(ViewFrustum::from_clip_from_world(&clip_from_world));
        let ranges = kit_view_ranges(&d, &lods, &frustum, true, gpu.ids.values_mut());
        for (slot_idx, r) in &ranges {
            gpu.draws.insert((ve, *slot_idx), r.clone());
        }
        let key = MeshPipelineKey::DEPTH_PREPASS | MeshPipelineKey::UNCLIPPED_DEPTH_ORTHO;
        for dr in &draws {
            if !gpu.draws.contains_key(&(ve, dr.slot_idx)) {
                continue;
            }
            let Ok(pipeline) = prepass_pipelines.specialize(
                &pipeline_cache,
                &prepass,
                KitKey {
                    mesh: key | dr.topo | dr.bits,
                    mat: dr.key,
                },
                &render_meshes.get(dr.mesh).expect("mesh").layout,
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
                ShadowBinKey {
                    asset_id: dr.mesh.untyped(),
                },
                (dr.entity, dr.main_entity),
                InputUniformIndex::default(),
                BinnedRenderPhaseType::NonMesh,
            );
        }
    }
    gpu.queued = draws.iter().map(|dr| dr.main_entity).collect();
    if gpu.ids.is_empty() {
        gpu.ids.push(0);
    }
    gpu.ids.write_buffer(&device, &queue);
}
// ---------------------------------------------------------------------------------------------
// Draw commands.

type DrawKitMain = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshViewBindingArrayBindGroup<1>,
    SetKitInstancesBindGroup,
    SetKitMaterialBindGroup,
    DrawKitInstances,
);

type DrawKitPrepass = (
    SetItemPipeline,
    SetPrepassViewBindGroup<0>,
    SetPrepassViewEmptyBindGroup<1>,
    SetKitInstancesBindGroup,
    SetKitMaterialBindGroup,
    DrawKitInstances,
);

/// Binds the slot's kit material at the material bind group. Bevy's `SetMaterialBindGroup` looks
/// the material up in `RenderMaterialInstances`, which extraction only fills for `Mesh3d`
/// entities; the kit draw slots have no `Mesh3d`, so it would Skip and — tuples abort on Skip —
/// silently drop the whole draw.
struct SetKitMaterialBindGroup;

impl<P: PhaseItem> RenderCommand<P> for SetKitMaterialBindGroup {
    type Param = (
        SRes<KitGpu>,
        SRes<ErasedRenderAssets<PreparedMaterial>>,
        SRes<MaterialBindGroupAllocators>,
    );
    type ViewQuery = ();
    type ItemQuery = Read<KitSlotDraw>;

    fn render<'w>(
        _item: &P,
        _view: (),
        group: Option<&'w KitSlotDraw>,
        (gpu, materials, allocators): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let (Some(group), Some(scene)) = (group, gpu.into_inner().scene.as_ref()) else {
            return RenderCommandResult::Skip;
        };
        let Some(slot) = scene.slots.get(group.slot as usize) else {
            return RenderCommandResult::Skip;
        };
        let (materials, allocators) = (materials.into_inner(), allocators.into_inner());
        let Some(prepared) = materials.get(slot.material.untyped()) else {
            return RenderCommandResult::Skip;
        };
        let Some(bind_group) = allocators
            .get(&slot.material.untyped().type_id())
            .and_then(|allocator| allocator.get(prepared.binding.group))
            .and_then(|bind_group| bind_group.bind_group())
        else {
            return RenderCommandResult::Skip;
        };
        pass.set_bind_group(MATERIAL_BIND_GROUP_INDEX, bind_group, &[]);
        RenderCommandResult::Success
    }
}

/// Binds the record + id buffers and the draw's metadata entry at group 2.
struct SetKitInstancesBindGroup;

impl<P: PhaseItem> RenderCommand<P> for SetKitInstancesBindGroup {
    type Param = SRes<KitGpu>;
    type ViewQuery = ();
    type ItemQuery = Read<KitSlotDraw>;

    fn render<'w>(
        _item: &P,
        _view: (),
        group: Option<&'w KitSlotDraw>,
        gpu: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(group) = group else {
            return RenderCommandResult::Skip;
        };
        let gpu = gpu.into_inner();
        let Some(bind_group) = gpu.bind_groups.get(group.slot_index()) else {
            return RenderCommandResult::Skip;
        };
        pass.set_bind_group(2, bind_group, &[]);
        RenderCommandResult::Success
    }
}

/// Binds the slot's mesh at group 2's vertex slots and draws the view's instance range of it.
struct DrawKitInstances;

impl<P: PhaseItem> RenderCommand<P> for DrawKitInstances {
    type Param = (
        SRes<KitGpu>,
        SRes<RenderAssets<RenderMesh>>,
        SRes<MeshAllocator>,
    );
    type ViewQuery = Entity;
    type ItemQuery = Read<KitSlotDraw>;

    fn render<'w>(
        _item: &P,
        view: Entity,
        group: Option<&'w KitSlotDraw>,
        (gpu, meshes, allocator): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let (gpu, meshes, allocator) = (
            gpu.into_inner(),
            meshes.into_inner(),
            allocator.into_inner(),
        );
        let (Some(group), Some(scene)) = (group, gpu.scene.as_ref()) else {
            return RenderCommandResult::Skip;
        };
        let Some(range) = gpu.draws.get(&(view, group.slot_index())) else {
            return RenderCommandResult::Skip;
        };
        let instances = range.end - range.start;
        let id = scene.slots[group.slot as usize].mesh[group.lod as usize].expect("slotted mesh");
        let (Some(mesh), Some(vertices)) = (meshes.get(id), allocator.mesh_vertex_slice(&id))
        else {
            return RenderCommandResult::Skip;
        };
        pass.set_vertex_buffer(0, vertices.buffer.slice(..));
        // The record ids for this draw as an instanced attribute: one u32 per instance,
        // starting at 0 for every draw (WebGPU's instance_index is 0-based per draw; Metal's
        // [[instance_id]] is absolute — attributes sidestep the divergence).
        pass.set_vertex_buffer(
            1,
            gpu.ids
                .buffer()
                .expect("ids buffer")
                .slice((range.start * 4) as u64..(range.end * 4) as u64),
        );
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
                    0..instances,
                );
            }
            RenderMeshBufferInfo::NonIndexed => {
                pass.draw(vertices.range, 0..instances);
            }
        }
        RenderCommandResult::Success
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Binned phases keep last frame's items: a slot culled from a cascade (or despawned by a
    /// rebuild) stays binned, and drawn as a skipped item, until it is removed from every view.
    #[test]
    fn unqueue_removes_last_frames_slots_from_every_view() {
        use bevy::render::batching::gpu_preprocessing::GpuPreprocessingMode;
        use bevy::{
            asset::AssetIndex, material::labels::DrawFunctionId, render::view::RetainedViewEntity,
        };
        let mut phases = ViewBinnedRenderPhases::<Shadow>::default();
        let views = [0, 1].map(|i| {
            RetainedViewEntity::new(
                MainEntity::from(Entity::from_raw_u32(100 + i).unwrap()),
                None,
                i,
            )
        });
        let slots = [1, 2].map(|i| {
            let e = Entity::from_raw_u32(i).unwrap();
            (e, MainEntity::from(e))
        });
        for v in views {
            phases.prepare_for_new_frame(v, GpuPreprocessingMode::Culling);
            let phase = phases.get_mut(&v).unwrap();
            for (i, s) in slots.into_iter().enumerate() {
                phase.add(
                    ShadowBatchSetKey {
                        pipeline: CachedRenderPipelineId::INVALID,
                        draw_function: DrawFunctionId(0),
                        material_bind_group_index: None,
                        slabs: MeshSlabs::default(),
                    },
                    ShadowBinKey {
                        asset_id: AssetId::<Mesh>::from(AssetIndex::from_bits(i as u64)).untyped(),
                    },
                    s,
                    InputUniformIndex::default(),
                    BinnedRenderPhaseType::NonMesh,
                );
            }
        }
        let count = |phases: &ViewBinnedRenderPhases<Shadow>| -> Vec<usize> {
            views
                .iter()
                .map(|v| {
                    phases[v]
                        .non_mesh_items
                        .values()
                        .map(|b| b.entities.len())
                        .sum()
                })
                .collect()
        };
        assert_eq!(count(&phases), [2, 2]);
        unqueue(&mut phases, &[slots[0].1]);
        assert_eq!(count(&phases), [1, 1]);
        unqueue(&mut phases, &[slots[1].1]);
        assert_eq!(count(&phases), [0, 0]);
        assert!(views.iter().all(|v| phases[v].is_empty()));
    }

    fn view_from_world_at(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
        Transform::from_translation(eye)
            .looking_at(target, up)
            .to_matrix()
            .inverse()
    }

    /// The scatter checkpoint's strategic camera over its 20-tile world under the kit sun: the
    /// world's view depths lie past cascade 1 and short of cascade 3, so neither is read.
    #[test]
    fn strategic_camera_reads_only_the_cascades_over_the_world() {
        let view_from_world = view_from_world_at(
            Vec3::new(20.0, 160.0, 150.0),
            Vec3::new(20.0, 0.0, -5.0),
            Vec3::Y,
        );
        let dir = -crate::lighting::KIT_SUN_DIR.normalize();
        let range = shadowed_depth_range(
            Vec3::new(-40.0, -3.0, -60.0),
            Vec3::new(80.0, 20.0, 60.0),
            dir,
            &view_from_world,
            4.0,
        )
        .expect("the sun is high");
        // Play's cascades: 4 out to 2,000 m, the first ending at 25 m.
        let config = bevy::light::CascadeShadowConfigBuilder {
            num_cascades: 4,
            maximum_distance: 2000.0,
            first_cascade_far_bound: crate::lighting::KIT_FIRST_CASCADE_FAR_BOUND,
            ..default()
        }
        .build();
        let read: Vec<bool> = (0..4)
            .map(|i| cascade_sampled(&config.bounds, config.overlap_proportion, i, range))
            .collect();
        assert_eq!(read, [false, false, true, false], "{range:?} {config:?}");
    }

    /// A camera inside the world (walk mode) reads its near cascades; the far ones stay unread
    /// only when the swept world ends short of them.
    #[test]
    fn camera_in_the_world_reads_the_near_cascades() {
        let view_from_world =
            view_from_world_at(Vec3::new(0.0, 1.7, 0.0), Vec3::new(10.0, 1.7, 0.0), Vec3::Y);
        let dir = -crate::lighting::KIT_SUN_DIR.normalize();
        let range = shadowed_depth_range(
            Vec3::new(-40.0, -3.0, -60.0),
            Vec3::new(80.0, 20.0, 60.0),
            dir,
            &view_from_world,
            4.0,
        )
        .unwrap();
        let bounds = [25.0, 108.0, 464.0, 2000.0];
        assert!(cascade_sampled(&bounds, 0.2, 0, range));
        assert!(cascade_sampled(&bounds, 0.2, 1, range));
    }

    /// A cascade is read from the overlap band before its near bound, and not at its far bound.
    #[test]
    fn cascade_sampled_follows_bevys_selection_and_blend() {
        let bounds = [25.0, 108.0, 464.0];
        // 0.8 * 25 = 20: a fragment at 21 m blends cascade 1 into cascade 0.
        assert!(cascade_sampled(&bounds, 0.2, 1, (21.0, 21.0)));
        assert!(!cascade_sampled(&bounds, 0.2, 1, (19.0, 19.9)));
        assert!(!cascade_sampled(&bounds, 0.2, 0, (25.0, 30.0)));
        assert!(cascade_sampled(&bounds, 0.2, 0, (-5.0, 0.0)));
        // Past the last bound no cascade is read; an unknown cascade is kept.
        assert!(!cascade_sampled(&bounds, 0.2, 2, (464.0, 900.0)));
        assert!(cascade_sampled(&bounds, 0.2, 3, (0.0, 1.0)));
    }

    /// The sweep follows the light down to the floor, so a low sun reaches farther; a sun at the
    /// horizon gives no bound and nothing is skipped.
    #[test]
    fn shadowed_range_sweeps_along_the_light() {
        let view_from_world = view_from_world_at(Vec3::ZERO, Vec3::NEG_Z, Vec3::Y);
        let (lo, hi) = (Vec3::new(-1.0, 0.0, -11.0), Vec3::new(1.0, 10.0, -9.0));
        let high = shadowed_depth_range(lo, hi, Vec3::NEG_Y, &view_from_world, 0.0).unwrap();
        assert_eq!(high, (9.0, 11.0));
        let low = shadowed_depth_range(
            lo,
            hi,
            Vec3::new(0.0, -1.0, -1.0).normalize(),
            &view_from_world,
            0.0,
        )
        .unwrap();
        assert!((low.1 - 21.0).abs() < 1e-4, "{low:?}");
        assert!(shadowed_depth_range(lo, hi, Vec3::NEG_Z, &view_from_world, 0.0).is_none());
    }

    #[test]
    fn chunk_lod_switches_at_150_and_400_m() {
        assert_eq!(kit_chunk_lod(0.0), 0);
        assert_eq!(kit_chunk_lod(149.9), 0);
        assert_eq!(kit_chunk_lod(150.0), 1);
        assert_eq!(kit_chunk_lod(399.9), 1);
        assert_eq!(kit_chunk_lod(400.0), 2);
    }

    /// One chunk, one slot, two instances: visible views draw both exactly once; a frustum
    /// missing the chunk draws nothing; shadow views use the same ranges.
    #[test]
    fn view_ranges_draw_each_visible_instance_once() {
        let set = KitInstanceSetData {
            pieces: vec![
                GpuKitInstance {
                    rotation: Quat::IDENTITY.to_array(),
                    translation: [0.0, 0.0, 0.0],
                    tag: 0,
                    scale: [1.0; 3],
                    _pad: 0,
                },
                GpuKitInstance {
                    rotation: Quat::IDENTITY.to_array(),
                    translation: [1.0, 0.0, 0.0],
                    tag: 0,
                    scale: [1.0; 3],
                    _pad: 0,
                },
            ],
            slots: vec![KitSlot {
                mesh: [None, None, None],
                tris: [12; 3],
                material: AssetId::<KitMaterial>::default().untyped().typed(),
                key: 0,
            }],
            chunks: vec![KitChunk {
                centre: Vec3::ZERO,
                aabb: Aabb::from_min_max(Vec3::new(-2.0, -1.0, -2.0), Vec3::new(2.0, 1.0, 2.0)),
                ranges: vec![Range { start: 0, end: 2 }],
            }],
            meta: vec![],
            version: 1,
        };
        let (mut ids, mut seen) = (vec![], vec![]);
        let enclosing = enclosing_frustum();
        let ranges = kit_view_ranges(&set, &[0], &enclosing, false, &mut ids);
        assert_eq!(ranges, vec![(0, 0..2)]);
        assert_eq!(ids.len(), 2);
        kit_view_ranges(&set, &[0], &enclosing, true, &mut seen);
        assert_eq!(seen.len(), 2);
        let (mut ids, mut miss) = (vec![], vec![]);
        let other = frustum_around(Vec3::new(1_000_000.0, 0.0, 0.0));
        assert!(kit_view_ranges(&set, &[0], &other, false, &mut ids).is_empty());
        assert!(ids.is_empty());
        kit_view_ranges(&set, &[0], &other, true, &mut miss);
        assert!(miss.is_empty());
    }

    fn enclosing_frustum() -> Frustum {
        // Orthographic box of ±5 km with clip z in 0..1, enclosing the chunk.
        let clip = Mat4::from_translation(Vec3::new(0.0, 0.0, 0.5))
            * Mat4::from_scale(Vec3::new(1.0 / 5_000.0, 1.0 / 5_000.0, 1.0 / 10_000.0));
        Frustum(ViewFrustum::from_clip_from_world(&clip))
    }

    fn frustum_around(centre: Vec3) -> Frustum {
        let clip = Mat4::from_translation(centre + Vec3::new(0.0, 0.0, 0.5))
            * Mat4::from_scale(Vec3::new(1.0 / 10.0, 1.0 / 10.0, 1.0 / 20.0));
        Frustum(ViewFrustum::from_clip_from_world(&clip))
    }
}
