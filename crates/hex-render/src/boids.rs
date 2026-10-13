//! GPU compute boids inside Bevy's render graph, drawn by a storage-buffer material.
//!
//! The flock lives only on the GPU: a compute system in the `RenderGraph` schedule updates a
//! `ShaderBuffer` before the cameras draw, and `BoidMaterial` reads the same buffer in its
//! vertex shader to place one tetrahedron per boid.
use std::{
    borrow::Cow,
    sync::atomic::{AtomicU32, Ordering},
};

use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::NoFrustumCulling,
    core_pipeline::schedule::camera_driver,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_resource::{
            binding_types::{storage_buffer_sized, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice, RenderGraph, RenderQueue},
        storage::{GpuShaderBuffer, ShaderBuffer},
    },
    shader::ShaderRef,
};

const COMPUTE_SHADER: &str = "shaders/boids_compute.wgsl";
const MATERIAL_SHADER: &str = "shaders/boid_material.wesl";
const WORKGROUP_SIZE: u32 = 64;
/// Longest simulated step; the slow first frames would otherwise fling the flock apart.
const MAX_DT: f32 = 1.0 / 30.0;

#[derive(Clone, Copy, Debug, PartialEq, ShaderType, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Boid {
    pub pos: [f32; 4],
    pub vel: [f32; 4],
}

/// Deterministic LCG so tests and screenshots are reproducible.
pub fn initial_boids(count: u32, seed: u32) -> Vec<Boid> {
    let mut s = seed.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    let mut rnd = move || {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (s >> 8) as f32 / (1u32 << 24) as f32
    };
    (0..count)
        .map(|_| Boid {
            pos: [
                rnd() * 800.0 - 400.0,
                40.0 + rnd() * 80.0,
                rnd() * 800.0 - 400.0,
                1.0,
            ],
            vel: [rnd() * 2.0 - 1.0, 0.0, rnd() * 2.0 - 1.0, 0.0],
        })
        .collect()
}

/// One merged mesh of `count` tetrahedra, 4 vertices each; the vertex shader maps
/// vertex `v` to boid `v / 4`.
pub fn boid_mesh(count: u32) -> Mesh {
    let tet = [
        [0.0, 0.0, 1.2],
        [-0.6, 0.0, -0.6],
        [0.6, 0.0, -0.6],
        [0.0, 0.5, -0.4],
    ];
    let mut pos: Vec<[f32; 3]> = Vec::with_capacity(count as usize * 4);
    let mut idx = Vec::with_capacity(count as usize * 12);
    for b in 0..count {
        let base = b * 4;
        pos.extend_from_slice(&tet);
        idx.extend_from_slice(&[
            base,
            base + 1,
            base + 2,
            base,
            base + 3,
            base + 1,
            base + 1,
            base + 3,
            base + 2,
            base + 2,
            base + 3,
            base,
        ]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_indices(Indices::U32(idx))
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct BoidMaterial {
    #[storage(0, read_only)]
    pub boids: Handle<ShaderBuffer>,
}

impl Material for BoidMaterial {
    fn vertex_shader() -> ShaderRef {
        MATERIAL_SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        MATERIAL_SHADER.into()
    }
    /// The default prepass and shadow shaders know nothing of the boid buffer and would draw
    /// every tetrahedron at its rest position around the origin.
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
}

#[derive(Resource, Clone, ExtractResource)]
#[extract_app(RenderApp)]
pub struct BoidBuffer(pub Handle<ShaderBuffer>);

/// Handles the scene re-uses each time it is (re)built, so a scenario switch neither
/// duplicates nor leaks the flock's mesh and material.
#[derive(Resource, Clone)]
pub struct BoidAssets {
    pub mesh: Handle<Mesh>,
    pub material: Handle<BoidMaterial>,
}

/// Per-frame simulation input, extracted to the render world.
#[derive(Resource, Clone, Copy, ExtractResource)]
#[extract_app(RenderApp)]
struct BoidSim {
    count: u32,
    dt: f32,
}

#[derive(Clone, Copy, Default, ShaderType)]
struct Params {
    count: u32,
    dt: f32,
    _pad0: u32,
    _pad1: u32,
}

/// Boids the compute pass dispatched in the latest render frame; 0 until it has run.
static DISPATCHED: AtomicU32 = AtomicU32::new(0);

/// Number of boids the GPU simulated in the latest frame (what ran, not what was configured).
pub fn dispatched_boids() -> u32 {
    DISPATCHED.load(Ordering::Relaxed)
}

pub struct BoidsPlugin {
    pub count: u32,
}

impl Default for BoidsPlugin {
    fn default() -> Self {
        Self { count: 1_000 }
    }
}

impl Plugin for BoidsPlugin {
    fn build(&self, app: &mut App) {
        let count = self.count;
        app.add_plugins((
            MaterialPlugin::<BoidMaterial>::default(),
            ExtractResourcePlugin::<BoidBuffer>::default(),
            ExtractResourcePlugin::<BoidSim>::default(),
        ))
        .insert_resource(BoidSim { count, dt: 0.0 })
        .add_systems(
            Startup,
            (move |commands: Commands,
                   meshes: ResMut<Assets<Mesh>>,
                   buffers: ResMut<Assets<ShaderBuffer>>,
                   materials: ResMut<Assets<BoidMaterial>>| {
                init_boids(count, commands, meshes, buffers, materials)
            })
            .before(crate::scene::setup),
        )
        .add_systems(Update, tick);

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .add_systems(RenderStartup, init_pipeline)
            .add_systems(
                Render,
                prepare_bind_group.in_set(RenderSystems::PrepareBindGroups),
            )
            .add_systems(RenderGraph, dispatch_boids.before(camera_driver));
    }
}

fn init_boids(
    count: u32,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut materials: ResMut<Assets<BoidMaterial>>,
) {
    // RENDER_WORLD only: the GPU owns the state after upload, and a main-world copy that
    // changed would overwrite the simulation with the initial positions.
    let mut buffer = ShaderBuffer::new(initial_boids(count, 1), RenderAssetUsages::RENDER_WORLD);
    buffer.buffer_usage |= BufferUsages::STORAGE | BufferUsages::COPY_DST;
    buffer.label = Cow::Borrowed("boids");
    let boids = buffers.add(buffer);
    commands.insert_resource(BoidBuffer(boids.clone()));
    commands.insert_resource(BoidAssets {
        mesh: meshes.add(boid_mesh(count)),
        material: materials.add(BoidMaterial { boids }),
    });
}

fn tick(time: Res<Time>, mut sim: ResMut<BoidSim>) {
    sim.dt = time.delta_secs().min(MAX_DT);
}

/// Spawns the flock entity; `scene::setup` calls this so it is a `SceneEntity` like the rest.
pub fn spawn_flock(commands: &mut Commands, assets: &BoidAssets) -> Entity {
    commands
        .spawn((
            Mesh3d(assets.mesh.clone()),
            MeshMaterial3d(assets.material.clone()),
            // The mesh AABB is the rest pose at the origin, not where the GPU moved the boids.
            NoFrustumCulling,
            Transform::IDENTITY,
            Name::new("boids"),
        ))
        .id()
}

#[derive(Resource)]
struct BoidsPipeline {
    layout: BindGroupLayoutDescriptor,
    update: CachedComputePipelineId,
}

#[derive(Resource)]
struct BoidsBindGroup {
    group: BindGroup,
    count: u32,
}

#[derive(Resource, Default)]
struct BoidParams(UniformBuffer<Params>);

fn init_pipeline(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    pipeline_cache: Res<PipelineCache>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "boids",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                storage_buffer_sized(false, None),
                uniform_buffer::<Params>(false),
            ),
        ),
    );
    let update = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("boids_update".into()),
        layout: vec![layout.clone()],
        shader: asset_server.load(COMPUTE_SHADER),
        entry_point: Some(Cow::from("update")),
        ..default()
    });
    commands.insert_resource(BoidsPipeline { layout, update });
    commands.init_resource::<BoidParams>();
}

fn prepare_bind_group(
    mut commands: Commands,
    pipeline: Res<BoidsPipeline>,
    pipeline_cache: Res<PipelineCache>,
    buffers: Res<RenderAssets<GpuShaderBuffer>>,
    boids: Option<Res<BoidBuffer>>,
    sim: Option<Res<BoidSim>>,
    mut params: ResMut<BoidParams>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let (Some(boids), Some(sim)) = (boids, sim) else {
        return;
    };
    let Some(gpu) = buffers.get(&boids.0) else {
        return;
    };
    params.0.set(Params {
        count: sim.count,
        dt: sim.dt,
        ..default()
    });
    params.0.write_buffer(&device, &queue);
    let group = device.create_bind_group(
        "boids",
        &pipeline_cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((gpu.buffer.as_entire_binding(), &params.0)),
    );
    commands.insert_resource(BoidsBindGroup {
        group,
        count: sim.count,
    });
}

fn dispatch_boids(
    mut render_context: RenderContext,
    bind_group: Option<Res<BoidsBindGroup>>,
    pipeline_cache: Res<PipelineCache>,
    pipeline: Res<BoidsPipeline>,
) {
    let (Some(bind_group), Some(update)) = (
        bind_group,
        pipeline_cache.get_compute_pipeline(pipeline.update),
    ) else {
        DISPATCHED.store(0, Ordering::Relaxed);
        return;
    };
    let mut pass = render_context
        .command_encoder()
        .begin_compute_pass(&ComputePassDescriptor {
            label: Some("boids"),
            ..default()
        });
    pass.set_pipeline(update);
    pass.set_bind_group(0, &bind_group.group, &[]);
    pass.dispatch_workgroups(bind_group.count.div_ceil(WORKGROUP_SIZE), 1, 1);
    DISPATCHED.store(bind_group.count, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn mesh_has_four_vertices_and_four_tris_per_boid() {
        let m = boid_mesh(10);
        let Some(VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions missing");
        };
        assert_eq!(p.len(), 40);
        assert_eq!(m.indices().unwrap().len(), 10 * 4 * 3);
    }

    #[test]
    fn initial_boids_are_deterministic_and_inside_bounds() {
        let a = initial_boids(64, 7);
        let b = initial_boids(64, 7);
        assert_eq!(a, b);
        assert!(a.iter().all(|x| x.pos[1] >= 40.0 && x.pos[1] <= 120.0));
    }
}
