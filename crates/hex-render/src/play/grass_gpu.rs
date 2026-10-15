//! The grass blades' GPU instancing (render world) for [`super::grass`].
//!
//! One shared blade mesh ([`grass::BLADE_CORNERS`], 10 vertices and 18 indices) owned here, and per
//! tile group one instance vertex buffer of 32-byte [`GpuBlade`] records: the tile's whole Lush set,
//! ordered by `keep`, uploaded once when the group appears. A density is a prefix of that buffer, so
//! a slider move changes only the drawn instance count ([`GrassGroup::drawn`]).
//!
//! As in [`crate::instanced`], the draws are `NonMesh` items in the binned `Opaque3d` and
//! `Opaque3dPrepass` phases of every camera, culled per view against each group's Lush-set bounds
//! under its `GlobalTransform`. The main pass uses Bevy's `MeshPipeline` layout and
//! `apply_pbr_lighting` (blades receive shadows); the prepass writes depth, normals and motion
//! vectors from the same pose (`grass_common`). Blades cast no shadow, so there is no `Shadow`
//! draw. Group 2 holds the shared [`GrassParams`] and, at a dynamic offset per group, its transform
//! now and one frame ago (the motion vectors of a rising or sinking tile) and its season tag.
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use bevy::{
    asset::uuid::Uuid,
    camera::primitives::Frustum,
    core_pipeline::{
        core_3d::{CORE_3D_DEPTH_FORMAT, Opaque3d, Opaque3dBatchSetKey, Opaque3dBinKey},
        prepass::{
            DepthPrepass, MotionVectorPrepass, NormalPrepass, Opaque3dPrepass,
            OpaqueNoLightmap3dBatchSetKey, OpaqueNoLightmap3dBinKey, prepass_target_descriptors,
        },
    },
    ecs::system::{SystemParamItem, lifetimeless::SRes},
    math::Affine3A,
    mesh::{
        MeshVertexBufferLayoutRef, MeshVertexBufferLayouts, PrimitiveTopology, VertexBufferLayout,
    },
    pbr::{
        MeshPipeline, MeshPipelineKey, PrepassPipeline, SetMeshViewBindGroup,
        SetMeshViewBindingArrayBindGroup, SetPrepassViewBindGroup, SetPrepassViewEmptyBindGroup,
        ViewKeyCache,
    },
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
        camera::ExtractedCamera,
        mesh::allocator::MeshSlabs,
        render_phase::{
            AddRenderCommand, BinnedRenderPhaseType, DrawFunctions, InputUniformIndex, PhaseItem,
            RenderCommand, RenderCommandResult, SetItemPipeline, TrackedRenderPass,
            ViewBinnedRenderPhases,
        },
        render_resource::{binding_types::uniform_buffer, *},
        renderer::{RenderDevice, RenderQueue},
        settings::WgpuFeatures,
        sync_world::{MainEntity, MainEntityHashMap, RenderEntity},
        view::ExtractedView,
    },
    shader::ShaderDefVal,
    shape::ViewFrustum,
};

use super::grass::{self, GpuBlade, GrassClock, GrassGroup, GrassInstances, GrassParams};

/// Instance buffers created since start: one per group, never on a density change.
static UPLOADS: AtomicU32 = AtomicU32::new(0);

/// Instance buffers the render world has created since start (`state().grass.uploads`).
pub fn uploads() -> u32 {
    UPLOADS.load(Ordering::Relaxed)
}

/// The bin key of every grass draw: blades have no mesh asset.
const BIN: AssetId<Mesh> = AssetId::Uuid {
    uuid: Uuid::from_u128(0x6a55_b1ad_e5c0_4ffe_9a55_0000_0000_0001),
};

/// One group's transform now and one frame ago, and its season tag (`MeshTag`, 0 when none).
#[derive(Clone, Copy, Debug, Default, ShaderType)]
struct GrassTile {
    world_from_local: Mat4,
    previous_world_from_local: Mat4,
    tag: u32,
}

pub struct GrassGpuPlugin;

impl Plugin for GrassGpuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GrassClock>();
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<ExtractedGrass>()
            .init_resource::<GrassGpu>()
            .init_resource::<SpecializedMeshPipelines<GrassMainPipeline>>()
            .init_resource::<SpecializedMeshPipelines<GrassPrepassPipeline>>()
            .add_render_command::<Opaque3d, DrawGrassMain>()
            .add_render_command::<Opaque3dPrepass, DrawGrassPrepass>()
            .add_systems(ExtractSchedule, extract_grass)
            .add_systems(
                Render,
                (
                    init_pipelines
                        .in_set(RenderSystems::PrepareViews)
                        .run_if(not(resource_exists::<GrassPipelines>)),
                    queue_grass.in_set(RenderSystems::Queue),
                    prepare_grass.in_set(RenderSystems::PrepareResources),
                    prepare_bind_group.in_set(RenderSystems::PrepareBindGroups),
                ),
            );
    }
}

struct ExtractedGroup {
    entity: Entity,
    main: MainEntity,
    instances: Arc<GrassInstances>,
    drawn: u32,
    visible: bool,
    world_from_local: Affine3A,
    tag: u32,
}

#[derive(Resource, Default)]
struct ExtractedGrass {
    params: GrassParams,
    groups: Vec<ExtractedGroup>,
}

fn extract_grass(
    mut out: ResMut<ExtractedGrass>,
    clock: Extract<Res<GrassClock>>,
    groups: Extract<
        Query<(
            Entity,
            &RenderEntity,
            &GrassGroup,
            &GlobalTransform,
            &InheritedVisibility,
            Option<&bevy::mesh::MeshTag>,
        )>,
    >,
) {
    out.params = clock.0;
    out.groups.clear();
    out.groups
        .extend(groups.iter().map(|(e, re, g, t, v, tag)| ExtractedGroup {
            entity: re.id(),
            main: MainEntity::from(e),
            instances: g.instances.clone(),
            drawn: g.drawn,
            visible: v.get(),
            world_from_local: t.affine(),
            tag: tag.map_or(0, |t| t.value),
        }));
}

const SHADER: &str = "shaders/grass.wesl";
const PREPASS_SHADER: &str = "shaders/grass_prepass.wesl";

fn grass_layout() -> BindGroupLayoutDescriptor {
    BindGroupLayoutDescriptor::new(
        "grass",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::VERTEX,
            (
                uniform_buffer::<GrassParams>(false),
                uniform_buffer::<GrassTile>(true),
            ),
        ),
    )
}

/// The instance buffer: [`GpuBlade`] at shader locations 1-4 (the blade mesh's corner is 0).
fn instance_layout() -> VertexBufferLayout {
    VertexBufferLayout::from_vertex_formats(
        VertexStepMode::Instance,
        [
            VertexFormat::Float32x3,
            VertexFormat::Float32,
            VertexFormat::Float32x2,
            VertexFormat::Unorm16x4,
        ],
    )
    .offset_locations_by(1)
}

#[derive(Resource, Clone)]
struct GrassMainPipeline {
    mesh: MeshPipeline,
    grass: BindGroupLayoutDescriptor,
    shader: Handle<Shader>,
}

impl SpecializedMeshPipeline for GrassMainPipeline {
    type Key = MeshPipelineKey;

    fn specialize(
        &self,
        key: Self::Key,
        layout: &MeshVertexBufferLayoutRef,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let mut d = self.mesh.specialize(key, layout)?;
        // Group 2 is Bevy's per-mesh uniform group; the blades read their own uniforms there.
        d.layout[2] = self.grass.clone();
        d.layout.truncate(3);
        d.label = Some("grass_main".into());
        d.vertex.shader = self.shader.clone();
        d.vertex.entry_point = Some("vertex".into());
        d.vertex.buffers.push(instance_layout());
        let f = d.fragment.as_mut().expect("main pass has a fragment stage");
        f.shader = self.shader.clone();
        f.entry_point = Some("fragment".into());
        // Blades are single quads seen from both sides.
        d.primitive.cull_mode = None;
        Ok(d)
    }
}

#[derive(Resource, Clone)]
struct GrassPrepassPipeline {
    view_motion_vectors: BindGroupLayoutDescriptor,
    view_no_motion_vectors: BindGroupLayoutDescriptor,
    empty: BindGroupLayoutDescriptor,
    grass: BindGroupLayoutDescriptor,
    shader: Handle<Shader>,
    depth_clip_control: bool,
}

impl SpecializedMeshPipeline for GrassPrepassPipeline {
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
        let mut defs: Vec<ShaderDefVal> = vec![];
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
            entry_point: Some("fragment".into()),
            targets,
            ..default()
        });
        let corners = layout
            .0
            .get_layout(&[Mesh::ATTRIBUTE_POSITION.at_shader_location(0)])?;
        Ok(RenderPipelineDescriptor {
            label: Some("grass_prepass".into()),
            layout: vec![
                if motion {
                    self.view_motion_vectors.clone()
                } else {
                    self.view_no_motion_vectors.clone()
                },
                self.empty.clone(),
                self.grass.clone(),
            ],
            vertex: VertexState {
                shader: self.shader.clone(),
                shader_defs: defs,
                entry_point: Some("vertex".into()),
                buffers: vec![corners, instance_layout()],
                ..default()
            },
            fragment,
            primitive: PrimitiveState {
                topology: key.primitive_topology(),
                cull_mode: None,
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

/// The pipelines and the shared blade mesh.
#[derive(Resource)]
struct GrassPipelines {
    main: GrassMainPipeline,
    prepass: GrassPrepassPipeline,
    layout: MeshVertexBufferLayoutRef,
    corners: Buffer,
    indices: Buffer,
}

fn init_pipelines(
    mut commands: Commands,
    mesh: Res<MeshPipeline>,
    prepass: Option<Res<PrepassPipeline>>,
    asset_server: Res<AssetServer>,
    device: Res<RenderDevice>,
    mut layouts: ResMut<MeshVertexBufferLayouts>,
) {
    let Some(prepass) = prepass else {
        return;
    };
    let blade = grass::blade_mesh();
    let grass = grass_layout();
    commands.insert_resource(GrassPipelines {
        main: GrassMainPipeline {
            mesh: mesh.clone(),
            grass: grass.clone(),
            shader: asset_server.load(SHADER),
        },
        prepass: GrassPrepassPipeline {
            view_motion_vectors: prepass.view_layout_motion_vectors.clone(),
            view_no_motion_vectors: prepass.view_layout_no_motion_vectors.clone(),
            empty: prepass.empty_layout.clone(),
            grass,
            shader: asset_server.load(PREPASS_SHADER),
            depth_clip_control: device.features().contains(WgpuFeatures::DEPTH_CLIP_CONTROL),
        },
        layout: blade.get_mesh_vertex_buffer_layout(&mut layouts),
        corners: device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("grass_blade_corners"),
            contents: &blade.create_packed_vertex_buffer_data(),
            usage: BufferUsages::VERTEX,
        }),
        indices: device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some("grass_blade_indices"),
            contents: blade
                .get_index_buffer_bytes()
                .expect("the blade is indexed"),
            usage: BufferUsages::INDEX,
        }),
    });
}

/// Per group: its instance buffer (with the set it holds), this frame's uniform offset and drawn
/// count, and last frame's transform; the shared uniforms and their bind group; the groups queued
/// last frame.
#[derive(Resource, Default)]
struct GrassGpu {
    buffers: MainEntityHashMap<(Arc<GrassInstances>, Buffer)>,
    draws: MainEntityHashMap<(u32, u32)>,
    previous: MainEntityHashMap<Mat4>,
    params: UniformBuffer<GrassParams>,
    tiles: DynamicUniformBuffer<GrassTile>,
    bind_group: Option<BindGroup>,
    queued: Vec<MainEntity>,
}

/// Uploads a new group's set once, drops a gone group's buffer, and writes every group's
/// transforms (now and one frame ago) and the shared uniforms.
fn prepare_grass(
    extracted: Res<ExtractedGrass>,
    mut gpu: ResMut<GrassGpu>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let gpu = &mut *gpu;
    gpu.draws.clear();
    gpu.tiles.clear();
    let mut live = MainEntityHashMap::default();
    for g in &extracted.groups {
        live.insert(g.main, ());
        if gpu
            .buffers
            .get(&g.main)
            .is_none_or(|(held, _)| !Arc::ptr_eq(held, &g.instances))
        {
            let buffer = device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("grass_blades"),
                contents: bytemuck::cast_slice::<GpuBlade, u8>(&g.instances.blades),
                usage: BufferUsages::VERTEX,
            });
            gpu.buffers.insert(g.main, (g.instances.clone(), buffer));
            UPLOADS.fetch_add(1, Ordering::Relaxed);
        }
        let now = Mat4::from(g.world_from_local);
        let previous = gpu.previous.insert(g.main, now).unwrap_or(now);
        let offset = gpu.tiles.push(&GrassTile {
            world_from_local: now,
            previous_world_from_local: previous,
            tag: g.tag,
        });
        gpu.draws.insert(g.main, (offset, g.drawn));
    }
    gpu.buffers.retain(|k, _| live.contains_key(k));
    gpu.previous.retain(|k, _| live.contains_key(k));
    gpu.params.set(extracted.params);
    gpu.params.write_buffer(&device, &queue);
    gpu.tiles.write_buffer(&device, &queue);
}

fn prepare_bind_group(
    mut gpu: ResMut<GrassGpu>,
    pipelines: Option<Res<GrassPipelines>>,
    pipeline_cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
) {
    let gpu = &mut *gpu;
    gpu.bind_group = None;
    let (Some(pipelines), Some(params), Some(tiles)) =
        (pipelines, gpu.params.binding(), gpu.tiles.binding())
    else {
        return;
    };
    gpu.bind_group = Some(device.create_bind_group(
        "grass",
        &pipeline_cache.get_bind_group_layout(&pipelines.main.grass),
        &BindGroupEntries::sequential((params, tiles)),
    ));
}

/// Queues every drawing group whose Lush-set bounds meet a camera's frustum into its opaque and
/// prepass phases. Last frame's groups are removed first, so a group gone (bulldozed, dropped,
/// Start over) or out of view leaves no item behind.
#[allow(clippy::too_many_arguments)]
fn queue_grass(
    extracted: Res<ExtractedGrass>,
    pipelines: Option<Res<GrassPipelines>>,
    mut gpu: ResMut<GrassGpu>,
    mut main_pipelines: ResMut<SpecializedMeshPipelines<GrassMainPipeline>>,
    mut prepass_pipelines: ResMut<SpecializedMeshPipelines<GrassPrepassPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    view_key_cache: Res<ViewKeyCache>,
    (opaque_draw, prepass_draw): (
        Res<DrawFunctions<Opaque3d>>,
        Res<DrawFunctions<Opaque3dPrepass>>,
    ),
    (mut opaque_phases, mut prepass_phases): (
        ResMut<ViewBinnedRenderPhases<Opaque3d>>,
        ResMut<ViewBinnedRenderPhases<Opaque3dPrepass>>,
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
) {
    let stale = std::mem::take(&mut gpu.queued);
    for phase in opaque_phases.values_mut() {
        for &me in &stale {
            phase.remove(me);
        }
    }
    for phase in prepass_phases.values_mut() {
        for &me in &stale {
            phase.remove(me);
        }
    }
    let Some(pipelines) = pipelines else {
        return;
    };
    let draw_main = opaque_draw.read().id::<DrawGrassMain>();
    let draw_prepass = prepass_draw.read().id::<DrawGrassPrepass>();
    let topology = MeshPipelineKey::from_primitive_topology_and_strip_index(
        PrimitiveTopology::TriangleList,
        None,
    );
    let mut queued: Vec<MainEntity> = vec![];
    for (view, msaa, normal, motion, depth) in &cameras {
        let clip_from_world = view.clip_from_view * view.world_from_view.to_matrix().inverse();
        let frustum = Frustum(ViewFrustum::from_clip_from_world(&clip_from_world));
        let seen: Vec<&ExtractedGroup> = extracted
            .groups
            .iter()
            .filter(|g| g.visible && g.drawn > 0)
            .filter(|g| frustum.intersects_obb(&g.instances.aabb, &g.world_from_local, true, true))
            .collect();
        if seen.is_empty() {
            continue;
        }
        if let (Some(phase), Some(&view_key)) = (
            opaque_phases.get_mut(&view.retained_view_entity),
            view_key_cache.get(&view.retained_view_entity),
        ) && let Ok(pipeline) = main_pipelines.specialize(
            &pipeline_cache,
            &pipelines.main,
            view_key | topology,
            &pipelines.layout,
        ) {
            for g in &seen {
                phase.add(
                    Opaque3dBatchSetKey {
                        pipeline,
                        draw_function: draw_main,
                        material_bind_group_index: None,
                        slabs: MeshSlabs::default(),
                        lightmap_slab: None,
                    },
                    Opaque3dBinKey {
                        asset_id: BIN.untyped(),
                    },
                    (g.entity, g.main),
                    InputUniformIndex::default(),
                    BinnedRenderPhaseType::NonMesh,
                );
                queued.push(g.main);
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
            let Ok(pipeline) = prepass_pipelines.specialize(
                &pipeline_cache,
                &pipelines.prepass,
                key | topology,
                &pipelines.layout,
            ) else {
                continue;
            };
            for g in &seen {
                phase.add(
                    OpaqueNoLightmap3dBatchSetKey {
                        pipeline,
                        draw_function: draw_prepass,
                        material_bind_group_index: None,
                        slabs: MeshSlabs::default(),
                    },
                    OpaqueNoLightmap3dBinKey {
                        asset_id: BIN.untyped(),
                    },
                    (g.entity, g.main),
                    InputUniformIndex::default(),
                    BinnedRenderPhaseType::NonMesh,
                );
                queued.push(g.main);
            }
        }
    }
    queued.sort_unstable();
    queued.dedup();
    gpu.queued = queued;
}

type DrawGrassMain = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshViewBindingArrayBindGroup<1>,
    DrawGrass,
);

type DrawGrassPrepass = (
    SetItemPipeline,
    SetPrepassViewBindGroup<0>,
    SetPrepassViewEmptyBindGroup<1>,
    DrawGrass,
);

/// Binds group 2 at the group's offset, the blade mesh at slot 0 and the group's instances at
/// slot 1, and draws the density's prefix of them.
struct DrawGrass;

impl<P: PhaseItem> RenderCommand<P> for DrawGrass {
    type Param = (SRes<GrassGpu>, SRes<GrassPipelines>);
    type ViewQuery = ();
    type ItemQuery = ();

    fn render<'w>(
        item: &P,
        _view: (),
        _group: Option<()>,
        (gpu, pipelines): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let (gpu, pipelines) = (gpu.into_inner(), pipelines.into_inner());
        let main = item.main_entity();
        let (Some(bind_group), Some(&(offset, drawn)), Some((_, blades))) = (
            gpu.bind_group.as_ref(),
            gpu.draws.get(&main),
            gpu.buffers.get(&main),
        ) else {
            return RenderCommandResult::Skip;
        };
        if drawn == 0 {
            return RenderCommandResult::Skip;
        }
        pass.set_bind_group(2, bind_group, &[offset]);
        pass.set_vertex_buffer(0, pipelines.corners.slice(..));
        pass.set_vertex_buffer(1, blades.slice(..));
        pass.set_index_buffer(pipelines.indices.slice(..), IndexFormat::Uint16);
        pass.draw_indexed(0..grass::BLADE_INDICES.len() as u32, 0, 0..drawn);
        RenderCommandResult::Success
    }
}
