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
        LightEntity, MATERIAL_BIND_GROUP_INDEX, MeshPipeline, MeshPipelineKey, PreparedMaterial,
        PrepassPipeline, SetMaterialBindGroup, SetMeshViewBindGroup,
        SetMeshViewBindingArrayBindGroup, SetPrepassViewBindGroup, SetPrepassViewEmptyBindGroup,
        Shadow, ShadowBatchSetKey, ShadowBinKey, ViewKeyCache,
    },
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        camera::ExtractedCamera,
        erased_render_asset::ErasedRenderAssets,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        mesh::allocator::{MeshAllocator, MeshSlabs},
        mesh::{MeshVertexBufferLayoutRef, RenderMesh, RenderMeshBufferInfo},
        render_asset::RenderAssets,
        render_phase::{
            AddRenderCommand, BinnedRenderPhaseType, DrawFunctions, InputUniformIndex, PhaseItem,
            RenderCommand, RenderCommandResult, SetItemPipeline, TrackedRenderPass,
            ViewBinnedRenderPhases,
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
// ---------------------------------------------------------------------------------------------
// Render world.

pub struct KitInstancesPlugin;

impl Plugin for KitInstancesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractResourcePlugin::<KitInstanceSet>::default(),
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
                    queue_kit.in_set(RenderSystems::Queue),
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
    ids: RawBufferVec<u32>,
    bind_group: Option<BindGroup>,
    /// (view render entity, slot index `slot * 3 + lod`) -> instance range into `ids`.
    draws: HashMap<(Entity, usize), Range<u32>>,
}

impl Default for KitGpu {
    fn default() -> Self {
        Self {
            scene: None,
            pieces: None,
            meta: None,
            meta_stride: 0,
            ids: RawBufferVec::new(BufferUsages::STORAGE),
            bind_group: None,
            draws: HashMap::default(),
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
                buffers: vec![vertex_layout],
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
) {
    let Some(scene) = scene else {
        return;
    };
    let changed = gpu.scene.as_ref().is_none_or(|s| !Arc::ptr_eq(s, &scene.0));
    if !changed {
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
                storage_buffer_read_only_sized(false, None),
                uniform_buffer_sized(true, Some(std::num::NonZeroU64::new(64).unwrap())),
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
    // The record buffer is rebuilt whenever the set swaps.
    gpu.pieces = Some(device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("kit_instances_records"),
        contents: bytemuck::cast_slice(&scene.0.pieces),
        usage: BufferUsages::STORAGE,
    }));
    // One metadata entry per (slot, LOD), padded to the device's dynamic-offset alignment.
    let align = device.limits().min_uniform_buffer_offset_alignment as usize;
    let stride = 64usize.max(align);
    let mut bytes = vec![0u8; stride * scene.0.meta.len()];
    for (i, m) in scene.0.meta.iter().enumerate() {
        let src = bytemuck::bytes_of(m);
        bytes[i * stride..i * stride + src.len()].copy_from_slice(src);
    }
    gpu.meta_stride = stride as u32;
    gpu.meta = Some(device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("kit_instances_meta"),
        contents: &bytes,
        usage: BufferUsages::UNIFORM,
    }));
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
    let (Some(pipeline), Some(pieces), Some(ids), Some(meta)) = (
        main,
        gpu.pieces.as_ref(),
        gpu.ids.buffer(),
        gpu.meta.as_ref(),
    ) else {
        return;
    };
    gpu.bind_group = Some(device.create_bind_group(
        "kit_instances_group2",
        &pipeline_cache.get_bind_group_layout(&pipeline.instances),
        &[
            BindGroupEntry {
                binding: 0,
                resource: pieces.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: ids.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::Buffer(BufferBinding {
                    buffer: meta,
                    offset: 0,
                    size: Some(std::num::NonZeroU64::new(64).unwrap()),
                }),
            },
        ],
    ));
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
    (opaque_draw, mask_draw, prepass_draw, shadow_draw): (
        Res<DrawFunctions<Opaque3d>>,
        Res<DrawFunctions<AlphaMask3d>>,
        Res<DrawFunctions<Opaque3dPrepass>>,
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
    cameras: Query<(Entity, &ExtractedView, &Msaa), With<ExtractedCamera>>,
    lights: Query<(Entity, &ExtractedView, &LightEntity)>,
    (device, queue): (Res<RenderDevice>, Res<RenderQueue>),
    mut gpu: ResMut<KitGpu>,
) {
    let (Some(scene), Some(main), Some(prepass)) = (scene, main, prepass) else {
        return;
    };
    let draw_main = opaque_draw.read().id::<DrawKitMain>();
    let draw_mask = mask_draw.read().id::<DrawKitMain>();
    let draw_prepass = prepass_draw.read().id::<DrawKitPrepass>();
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
        let bits = prepared.properties.mesh_pipeline_key_bits.downcast();
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
    let eye = camera.1.world_from_view.translation();
    let lods: Vec<usize> = d
        .chunks
        .iter()
        .map(|c| kit_chunk_lod(eye.distance(c.centre)))
        .collect();
    for (ve, view, msaa) in &cameras {
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
    }
    for (ve, view, light) in &lights {
        let LightEntity::Directional { .. } = light else {
            continue;
        };
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
    SetMaterialBindGroup<MATERIAL_BIND_GROUP_INDEX>,
    DrawKitInstances,
);

type DrawKitPrepass = (
    SetItemPipeline,
    SetPrepassViewBindGroup<0>,
    SetPrepassViewEmptyBindGroup<1>,
    SetKitInstancesBindGroup,
    SetMaterialBindGroup<MATERIAL_BIND_GROUP_INDEX>,
    DrawKitInstances,
);

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
        let Some(bind_group) = gpu.bind_group.as_ref() else {
            return RenderCommandResult::Skip;
        };
        let offset = group.slot_index() as u32 * gpu.meta_stride;
        pass.set_bind_group(2, bind_group, &[offset]);
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
        let (Some(group), Some(scene), Some(_bind_group)) =
            (group, gpu.scene.as_ref(), gpu.bind_group.as_ref())
        else {
            return RenderCommandResult::Skip;
        };
        let Some(range) = gpu.draws.get(&(view, group.slot_index())) else {
            return RenderCommandResult::Skip;
        };
        let id = scene.slots[group.slot as usize].mesh[group.lod as usize].expect("slotted mesh");
        let (Some(mesh), Some(vertices)) = (meshes.get(id), allocator.mesh_vertex_slice(&id))
        else {
            return RenderCommandResult::Skip;
        };
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
