//! GPU compute boids inside Bevy's render graph, drawn by a storage-buffer material (PRD §7.2).
//!
//! The flock lives only on the GPU: a compute system in the `RenderGraph` schedule updates a
//! `ShaderBuffer` before the cameras draw, and `BoidMaterial` reads the same buffer in its
//! vertex shader to draw one procedural bird or fish per boid. The CPU half ([`plan`]) decides
//! the budget split, where birds roost and when they launch, and the water each fish school is
//! confined to; it uploads that as read-only buffers and never reads the flock back.
//!
//! In `play` the birds flock over the world and roost on the forest's oak canopy and the tops of
//! its scattered spruces (`play::scatter`) in waves, and
//! the fish school in the water volume of connected water tiles. Every other scenario keeps the
//! P0 stress flock: every slot a bird over an 800 m square.
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
            binding_types::{storage_buffer_read_only_sized, storage_buffer_sized, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice, RenderGraph, RenderQueue},
        storage::{GpuShaderBuffer, ShaderBuffer},
    },
    shader::ShaderRef,
};
use bytemuck::Zeroable;

pub mod plan;

pub use plan::{BoidBudget, BoidCounts, BoidPlan};

use crate::{
    ActiveScenario, Scenario,
    play::{PlayCatalog, PlayKit, PlaySystems, PlayView, rise::PlayRises, scatter::ScatterLayouts},
};

/// The drawn scattered trees that have canopy perches, by cell, nearest the centre first.
fn scatter_trees(
    s: &ScatterLayouts,
) -> std::collections::BTreeMap<hex_core::Axial, Vec<plan::TreeInstance>> {
    let mut out: std::collections::BTreeMap<_, Vec<_>> = Default::default();
    let mut drawn: Vec<_> = s
        .drawn()
        .filter(|(_, piece, _)| !plan::canopy_perches(piece).is_empty())
        .collect();
    drawn.sort_by_key(|(_, _, i)| i.order);
    for (cell, piece, i) in drawn {
        out.entry(cell).or_default().push(plan::TreeInstance {
            piece: piece.to_owned(),
            at: i.pos,
            yaw: i.rotation(),
            scale: i.scale,
        });
    }
    out
}

const COMPUTE_SHADER: &str = "shaders/boids_compute.wgsl";
const MATERIAL_SHADER: &str = "shaders/boid_material.wesl";
const WORKGROUP_SIZE: u32 = 64;
/// Longest simulated step; the slow first frames would otherwise fling the flock apart.
const MAX_DT: f32 = 1.0 / 30.0;
/// Vertices of one boid's procedural shape (`boid_material.wesl` holds the bird and fish forms).
pub const BOID_VERTS: u32 = 8;
/// Its triangles, each drawn from both sides: the left and right wing (or flank), the tail, and
/// the upper and lower keel.
const BOID_TRIS: [[u32; 3]; 5] = [[0, 1, 3], [0, 3, 2], [3, 4, 5], [0, 6, 3], [0, 3, 7]];

#[derive(Clone, Copy, Debug, PartialEq, ShaderType, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Boid {
    /// xyz position; w the generation of [`Ctl`] it was spawned from.
    pub pos: [f32; 4],
    /// xyz velocity; w what it is doing (fly, settle, perch, swim; see the compute shader).
    pub vel: [f32; 4],
}

/// One slot as the CPU sets it: `spawn` xyz and generation (a new generation resets the boid
/// there), `info` (kind: 0 bird 1 fish, roost index or -1, time sent to the roost, 0).
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Ctl {
    pub spawn: [f32; 4],
    pub info: [f32; 4],
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

/// One merged mesh of `count` boid shapes, [`BOID_VERTS`] vertices each; the vertex shader maps
/// vertex `v` to boid `v / BOID_VERTS` and replaces the positions with the boid's form.
pub fn boid_mesh(count: u32) -> Mesh {
    let mut pos: Vec<[f32; 3]> = Vec::with_capacity((count * BOID_VERTS) as usize);
    let mut idx = Vec::with_capacity(count as usize * BOID_TRIS.len() * 6);
    for b in 0..count {
        let base = b * BOID_VERTS;
        pos.extend((0..BOID_VERTS).map(|v| [v as f32 * 0.1, 0.0, 0.0]));
        for [a, c, d] in BOID_TRIS {
            idx.extend_from_slice(&[base + a, base + c, base + d, base + a, base + d, base + c]);
        }
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
    /// every shape at its rest position around the origin. Staying out of the depth prepass also
    /// keeps fish out of the depth refracting water reads as its bed; they draw in the opaque
    /// pass, which the water's transmission texture copies, so they show through the surface.
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

/// The CPU-written inputs of the compute pass (`plan`): per-slot control, roost perches, and the
/// fish field's chunk table and samples. Replaced (new handles) whenever their data changes.
#[derive(Resource, Clone, ExtractResource)]
#[extract_app(RenderApp)]
pub struct BoidInputs {
    pub ctl: Handle<ShaderBuffer>,
    pub roosts: Handle<ShaderBuffer>,
    pub chunks: Handle<ShaderBuffer>,
    pub field: Handle<ShaderBuffer>,
}

/// Handles the scene re-uses each time it is (re)built, so a scenario switch neither
/// duplicates nor leaks the flock's mesh and material.
#[derive(Resource, Clone)]
pub struct BoidAssets {
    pub mesh: Handle<Mesh>,
    pub material: Handle<BoidMaterial>,
}

/// The world seed boids derive from (`--seed`); `hex-app` inserts it. Seed 1 without it.
#[derive(Resource, Clone, Copy, Debug)]
pub struct BoidSeed(pub u64);

/// The boid state the published state reports (`hexWorld.state().boids`), as JSON.
#[derive(Resource, Clone, Debug, Default)]
pub struct BoidReport(pub String);

/// The CPU plan, its clock and the scenario it was made for.
#[derive(Resource)]
pub struct BoidState {
    pub plan: BoidPlan,
    /// Boid clock (s): sums the same clamped steps the GPU integrates.
    pub t: f32,
    scenario: Option<Scenario>,
}

#[derive(Clone, Copy, Default, ShaderType)]
struct Params {
    count: u32,
    birds: u32,
    fish: u32,
    roosts: u32,
    dt: f32,
    time: f32,
    /// 0: the P0 stress flock; 1: play (roosts and fish).
    mode: u32,
    field_w: u32,
    field_h: u32,
    field_x0: i32,
    field_z0: i32,
    _pad: u32,
    /// Play flock centre (xyz) and horizontal radius (w).
    sky: Vec4,
}

/// Per-frame simulation input, extracted to the render world.
#[derive(Resource, Clone, Copy, Default, ExtractResource)]
#[extract_app(RenderApp)]
struct BoidSim(Params);

/// Boids the compute pass dispatched in the latest render frame; 0 until it has run.
static DISPATCHED: AtomicU32 = AtomicU32::new(0);

/// Number of boids the GPU simulated in the latest frame (what ran, not what was configured).
pub fn dispatched_boids() -> u32 {
    DISPATCHED.load(Ordering::Relaxed)
}

pub struct BoidsPlugin {
    pub budget: BoidBudget,
}

impl Default for BoidsPlugin {
    fn default() -> Self {
        Self {
            budget: BoidBudget::HIGH,
        }
    }
}

impl Plugin for BoidsPlugin {
    fn build(&self, app: &mut App) {
        let budget = self.budget;
        app.add_plugins((
            MaterialPlugin::<BoidMaterial>::default(),
            ExtractResourcePlugin::<BoidBuffer>::default(),
            ExtractResourcePlugin::<BoidInputs>::default(),
            ExtractResourcePlugin::<BoidSim>::default(),
        ))
        .insert_resource(BoidSim::default())
        .insert_resource(BoidState {
            plan: BoidPlan::stress(budget, 1),
            t: 0.0,
            scenario: None,
        })
        .init_resource::<BoidReport>()
        .add_systems(
            Startup,
            (move |commands: Commands,
                   meshes: ResMut<Assets<Mesh>>,
                   buffers: ResMut<Assets<ShaderBuffer>>,
                   materials: ResMut<Assets<BoidMaterial>>| {
                init_boids(budget.total, commands, meshes, buffers, materials)
            })
            .before(crate::scene::setup),
        )
        .add_systems(Update, plan_boids.after(PlaySystems));

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

fn storage<T: bytemuck::NoUninit>(
    buffers: &mut Assets<ShaderBuffer>,
    data: Vec<T>,
    label: &'static str,
) -> Handle<ShaderBuffer> {
    let mut b = ShaderBuffer::new(data, RenderAssetUsages::RENDER_WORLD);
    b.buffer_usage |= BufferUsages::STORAGE | BufferUsages::COPY_DST;
    b.label = Cow::Borrowed(label);
    buffers.add(b)
}

fn init_boids(
    count: u32,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut materials: ResMut<Assets<BoidMaterial>>,
) {
    // RENDER_WORLD only: the GPU owns the state after upload, and a main-world copy that
    // changed would overwrite the simulation. Generation 0 never matches a slot, so the first
    // dispatch spawns every boid from its `Ctl`.
    let boids = storage(&mut buffers, vec![Boid::zeroed(); count as usize], "boids");
    let inputs = BoidInputs {
        ctl: storage(
            &mut buffers,
            vec![Ctl::zeroed(); count as usize],
            "boid_ctl",
        ),
        roosts: storage(&mut buffers, vec![[0f32; 4]], "boid_roosts"),
        chunks: storage(&mut buffers, vec![-1i32], "boid_chunks"),
        field: storage(&mut buffers, vec![plan::LAND], "boid_field"),
    };
    commands.insert_resource(inputs);
    commands.insert_resource(BoidBuffer(boids.clone()));
    commands.insert_resource(BoidAssets {
        mesh: meshes.add(boid_mesh(count)),
        material: materials.add(BoidMaterial { boids }),
    });
}

/// The CPU side each frame: (re)starts the plan on a scenario switch, feeds it the placed tiles
/// that reached their rise's agents stage and the tree pieces the kit draws, advances the roost
/// waves, uploads what changed and fills the compute parameters and the report.
#[allow(clippy::too_many_arguments)]
fn plan_boids(
    time: Res<Time>,
    active: Res<ActiveScenario>,
    seed: Option<Res<BoidSeed>>,
    (view, catalog, rises, kit, scatter): (
        Res<PlayView>,
        Res<PlayCatalog>,
        Res<PlayRises>,
        Option<Res<PlayKit>>,
        Option<Res<ScatterLayouts>>,
    ),
    mut state: ResMut<BoidState>,
    mut sim: ResMut<BoidSim>,
    inputs: Option<ResMut<BoidInputs>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut report: ResMut<BoidReport>,
    paused: Option<Res<crate::AgentsPaused>>,
) {
    let Some(mut inputs) = inputs else {
        return;
    };
    let dt = step_dt(time.delta_secs(), paused.is_some_and(|p| p.0));
    let play = active.0 == Scenario::Play;
    let state = &mut *state;
    if state.scenario != Some(active.0) {
        let seed = seed.map_or(1, |s| s.0);
        let budget = state.plan.budget;
        let base = state
            .plan
            .slots
            .iter()
            .map(|s| s.generation)
            .max()
            .unwrap_or(0);
        state.plan = if play {
            BoidPlan::play(budget, seed)
        } else {
            BoidPlan::stress(budget, seed)
        };
        // Every slot gets a generation the GPU has not seen, so a switch respawns the flock.
        for s in &mut state.plan.slots {
            s.generation += base;
        }
        state.scenario = Some(active.0);
        state.t = 0.0;
    }
    state.t += dt;
    let t = state.t;
    let plan = &mut state.plan;
    if play {
        let ready: Vec<_> = view
            .tiles
            .iter()
            .filter(|tile| rises.agents_ready(tile.cell))
            .cloned()
            .collect();
        let drawn =
            |id: &str| matches!(kit.as_deref(), Some(PlayKit::Ready(m)) if m.contains_key(id));
        if let Some(s) = scatter.as_deref() {
            plan.set_scatter(s.revision, || scatter_trees(s));
        }
        plan.set_world(&ready, &catalog.0, &drawn, t);
        plan.tick(t);
    }
    if plan.slots_dirty {
        let ctl: Vec<Ctl> = plan
            .slots
            .iter()
            .map(|s| Ctl {
                spawn: [s.spawn.x, s.spawn.y, s.spawn.z, s.generation as f32],
                info: [
                    f32::from(u8::from(s.kind == plan::SlotKind::Fish)),
                    s.roost
                        .and_then(|k| plan.roost_index(k))
                        .map_or(-1.0, |i| i as f32),
                    s.assigned_at,
                    0.0,
                ],
            })
            .collect();
        inputs.ctl = storage(&mut buffers, ctl, "boid_ctl");
        plan.slots_dirty = false;
    }
    if plan.world_dirty {
        let mut roosts: Vec<[f32; 4]> = plan
            .roosts
            .iter()
            .map(|r| [r.pos.x, r.pos.y, r.pos.z, 0.0])
            .collect();
        roosts.push([0.0; 4]);
        let f = &plan.water.field;
        let chunks = if f.chunks.is_empty() {
            vec![-1]
        } else {
            f.chunks.clone()
        };
        let mut samples = f.samples.clone();
        samples.push(plan::LAND);
        inputs.roosts = storage(&mut buffers, roosts, "boid_roosts");
        inputs.chunks = storage(&mut buffers, chunks, "boid_chunks");
        inputs.field = storage(&mut buffers, samples, "boid_field");
        plan.world_dirty = false;
    }
    let f = &plan.water.field;
    sim.0 = Params {
        count: plan.slots.len() as u32,
        birds: plan.birds,
        fish: plan.fish,
        roosts: plan.roosts.len() as u32,
        dt,
        time: t,
        mode: u32::from(plan.play),
        field_w: f.w,
        field_h: f.h,
        field_x0: f.x0,
        field_z0: f.z0,
        _pad: 0,
        sky: plan.sky,
    };
    report.0 = report_json(&plan.counts(t), plan.play, dispatched_boids());
}

/// The `boids` object of the published state.
pub fn report_json(c: &BoidCounts, play: bool, dispatched: u32) -> String {
    let regions: Vec<String> = c
        .regions
        .iter()
        .map(|(cell, tiles, fish)| {
            format!(
                "{{\"q\":{},\"r\":{},\"tiles\":{tiles},\"fish\":{fish}}}",
                cell.q, cell.r
            )
        })
        .collect();
    format!(
        "{{\"mode\":\"{}\",\"budget\":{},\"birds\":{},\"fish\":{},\"roosts\":{},\"roosting\":{},\"settling\":{},\"water_tiles\":{},\"fish_regions\":[{}],\"dispatched\":{dispatched}}}",
        if play { "play" } else { "stress" },
        c.budget,
        c.birds,
        c.fish,
        c.roosts,
        c.roosting,
        c.settling,
        c.water_tiles,
        regions.join(",")
    )
}

/// The compute step for a frame: clamped, and zero while photo mode pauses the agents (the flock
/// and the roost waves hold still).
pub fn step_dt(delta: f32, paused: bool) -> f32 {
    if paused { 0.0 } else { delta.min(MAX_DT) }
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
                storage_buffer_read_only_sized(false, None),
                storage_buffer_read_only_sized(false, None),
                storage_buffer_read_only_sized(false, None),
                storage_buffer_read_only_sized(false, None),
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

/// Binds this frame's buffers. While a replaced input is still uploading, the previous bind group
/// (which keeps its buffers alive) runs one more frame.
#[allow(clippy::too_many_arguments)]
fn prepare_bind_group(
    mut commands: Commands,
    pipeline: Res<BoidsPipeline>,
    pipeline_cache: Res<PipelineCache>,
    buffers: Res<RenderAssets<GpuShaderBuffer>>,
    (boids, inputs, sim): (
        Option<Res<BoidBuffer>>,
        Option<Res<BoidInputs>>,
        Option<Res<BoidSim>>,
    ),
    mut params: ResMut<BoidParams>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let (Some(boids), Some(inputs), Some(sim)) = (boids, inputs, sim) else {
        return;
    };
    let (Some(gpu), Some(ctl), Some(roosts), Some(chunks), Some(field)) = (
        buffers.get(&boids.0),
        buffers.get(&inputs.ctl),
        buffers.get(&inputs.roosts),
        buffers.get(&inputs.chunks),
        buffers.get(&inputs.field),
    ) else {
        return;
    };
    params.0.set(sim.0);
    params.0.write_buffer(&device, &queue);
    let group = device.create_bind_group(
        "boids",
        &pipeline_cache.get_bind_group_layout(&pipeline.layout),
        &BindGroupEntries::sequential((
            gpu.buffer.as_entire_binding(),
            &params.0,
            ctl.buffer.as_entire_binding(),
            roosts.buffer.as_entire_binding(),
            chunks.buffer.as_entire_binding(),
            field.buffer.as_entire_binding(),
        )),
    );
    commands.insert_resource(BoidsBindGroup {
        group,
        count: sim.0.count,
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
    fn paused_boids_take_no_step() {
        assert_eq!(step_dt(0.016, true), 0.0);
        assert_eq!(step_dt(0.016, false), 0.016);
        assert_eq!(step_dt(5.0, false), MAX_DT);
    }

    #[test]
    fn mesh_has_one_double_sided_shape_per_boid() {
        let m = boid_mesh(10);
        let Some(VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions missing");
        };
        assert_eq!(p.len(), 10 * BOID_VERTS as usize);
        assert_eq!(m.indices().unwrap().len(), 10 * BOID_TRIS.len() * 6);
        assert!(
            m.indices()
                .unwrap()
                .iter()
                .all(|i| i < 10 * BOID_VERTS as usize)
        );
    }

    #[test]
    fn initial_boids_are_deterministic_and_inside_bounds() {
        let a = initial_boids(64, 7);
        let b = initial_boids(64, 7);
        assert_eq!(a, b);
        assert!(a.iter().all(|x| x.pos[1] >= 40.0 && x.pos[1] <= 120.0));
    }

    #[test]
    fn gpu_structs_match_the_shader_layout() {
        assert_eq!(std::mem::size_of::<Boid>(), 32);
        assert_eq!(std::mem::size_of::<Ctl>(), 32);
        assert_eq!(Params::min_size().get(), 64);
    }

    #[test]
    fn report_is_json_with_the_counts() {
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 1);
        let c = hex_core::TileCatalog::builtin().unwrap();
        let tiles = [crate::play::PlayTile {
            cell: hex_core::Axial::new(0, 0),
            tile: "forest".into(),
            rotation: 0,
        }];
        plan.set_world(&tiles, &c, &|_| true, 0.0);
        let s = report_json(&plan.counts(0.0), true, 1000);
        assert!(s.starts_with("{\"mode\":\"play\",\"budget\":1000,\"birds\":1000,\"fish\":0,\"roosts\":12,\"roosting\":12"), "{s}");
        assert!(
            s.ends_with("\"fish_regions\":[],\"dispatched\":1000}"),
            "{s}"
        );
    }
}
