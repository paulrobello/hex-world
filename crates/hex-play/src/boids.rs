//! GPU compute boids inside Bevy's render graph, drawn by a storage-buffer material (PRD §7.2).
//!
//! The flock lives only on the GPU: a compute system in the `RenderGraph` schedule updates a
//! `ShaderBuffer` before the cameras draw, and `BoidMaterial` reads the same buffer in its
//! vertex shader to pose and place one kit piece per boid: a bird species per flock
//! (`bird_starling` display x4 or `bird_goldfinch` x5, [`BIRDS`]) and a fish species by spawn water
//! (`fish_trout` in river water, `fish_mackerel` in the sea, display x2, [`FISH`]), each drawn
//! from one merged copy of its LOD0 per slot it may hold, a bird species' copies over its own
//! flocks' slots only, each fish species' over every fish slot, drawing only its own water's fish
//! ([`FormSlots`], [`fish_form_draws`]; no per-boid LOD: the GPU moves the boids, so Bevy's
//! visibility ranges never see them). The
//! CPU half ([`plan`]) decides the budget split, where birds roost and when they launch, the
//! water each fish school is confined to and each fish's palette; it uploads that as read-only
//! buffers and never reads the flock back.
//!
//! Fixed-clock capture runs (`--clock fixed`) also insert [`FishTrack`], which reads the flock
//! back each frame to count fish crossing between river and sea water for the checkpoint dumps.
//! Each readback is tagged with the frame, boid clock and plan it was copied under, and folded in
//! frame order [`TRACK_LAG`] frames later, so the counts do not depend on when the GPU's map
//! callbacks happen to land.
//!
//! The compute pass reads every boid from a copy of the flock taken before it runs (`boids_prev`)
//! and writes the flock buffer, so each boid steers from its neighbours' state of the previous
//! frame. Reading and writing one buffer in one dispatch raced: whether a neighbour was read
//! before or after its own thread wrote it depended on GPU scheduling, and the flock diverged run
//! to run.
//!
//! In `play` the birds fly in small flocks over the world and roost in waves on the oak tiles' oak
//! canopies and on `barn_red_b`'s ridge and cupola (a tile's Bird rule; scattered spruces on such a tile would share its cap,
//! `play::scatter`), and the fish school in the water volume of connected water tiles. A world
//! without an oak keeps every bird in the air. Every other scenario keeps the P0 stress flock:
//! every slot a bird over an 800 m square.
use std::{
    borrow::Cow,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::NoFrustumCulling,
    core_pipeline::schedule::camera_driver,
    gltf::{Gltf, GltfMesh},
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        gpu_readback::{ReadbackComplete, ReadbackOnce},
        render_asset::RenderAssets,
        render_resource::{
            binding_types::{storage_buffer_read_only_sized, storage_buffer_sized, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice, RenderGraph, RenderQueue},
        settings::WgpuFeatures,
        storage::{GpuShaderBuffer, ShaderBuffer},
    },
    shader::ShaderRef,
};
use bytemuck::Zeroable;

pub mod plan;

pub use plan::{BoidBudget, BoidCounts, BoidPlan};

use crate::{
    ActiveScenario, Scenario, SceneEntity,
    play::{PlayCatalog, PlayKit, PlaySystems, PlayView, rise::PlayRises, scatter::ScatterLayouts},
    water::tiles::CellWater,
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
/// The fish kit pieces' display scale over real size (fish_trout owner-approved 2026-10-14): every
/// fish species draws at FISH_SCALE. Each bird species has its own ([`BirdPiece::scale`]).
pub const FISH_SCALE: f32 = 2.0;

/// A fish piece and the fish it draws: those spawned in river water (`river`, the river palette,
/// [`plan::river_water`]) or those spawned in sea water (mouth, coast and ocean tiles, the ocean
/// palette). Every fish species shares the tail-wag contract ([`FISH_WAG`]) and the trout's height
/// envelope, which the confinement band is built around (`plan::TROUT_TOP`, `plan::TROUT_BELLY`).
#[derive(Clone, Copy, Debug)]
pub struct FishPiece {
    pub id: &'static str,
    pub river: bool,
}

/// The fish species by index ([`plan::fish_species`]).
pub const FISH: [FishPiece; plan::FISH_SPECIES as usize] = [
    FishPiece {
        id: "fish_trout",
        river: true,
    },
    FishPiece {
        id: "fish_mackerel",
        river: false,
    },
];

/// [`BoidForm::flags`]: the piece has a masks map (AO in A).
pub const FLAG_MASKS: u32 = 1;
/// [`BoidForm::flags`]: a fish form that draws the river-palette fish; without it a fish form draws
/// the ocean-palette ones. Mirrors `FLAG_RIVER_FISH` in `boid_material.wesl`.
pub const FLAG_RIVER_FISH: u32 = 2;

/// Whether a fish form with `flags` draws the fish whose palette offset is `palette` (`Boid::anim`
/// z, which the compute pass copies from the slot's `Ctl::look`), as `boid_material.wesl` decides:
/// a fish's species follows its spawn water, as its palette does.
pub fn fish_form_draws(flags: u32, palette: f32) -> bool {
    (palette > RIVER_U * 0.5) == (flags & FLAG_RIVER_FISH != 0)
}

/// [`BoidForm::flags`] for a piece with or without a masks map, and the fish species it is, if any.
pub fn form_flags(masks: bool, fish: Option<FishPiece>) -> u32 {
    let mut flags = 0;
    if masks {
        flags |= FLAG_MASKS;
    }
    if fish.is_some_and(|f| f.river) {
        flags |= FLAG_RIVER_FISH;
    }
    flags
}

/// A bird piece and its runtime contract, the `boid` object of its GLB root extras (checked by a
/// test): the shoulder hinge line (x = +-hinge[0], y = hinge[1]), the perch fold (wing offset
/// scaled by fold_k, rolled fold_roll_deg, moved fold_back * U back) and the pivot's height over
/// the feet. Every bird shares the flap and fold formulas in `boid_material.wesl`.
#[derive(Clone, Copy, Debug)]
pub struct BirdPiece {
    pub id: &'static str,
    pub hinge: [f32; 2],
    pub fold_k: f32,
    pub fold_roll_deg: f32,
    pub fold_back: f32,
    pub pivot_to_feet_m: f32,
    /// Display scale over real size: the starling 4 (owner-approved 2026-10-14), the goldfinch 5
    /// (owner A11-Q2, 2026-10-20: drawn at 5x), which still leaves it shorter and narrower than
    /// the starling (0.64 m long and 1.20 m across against 0.79 m and 1.60 m), so it fits every
    /// perch the starling does.
    pub scale: f32,
}

impl BirdPiece {
    /// How far a perched bird's pivot stands over its perch point at display scale: its feet on
    /// the perch.
    pub fn perch_lift(&self) -> f32 {
        self.pivot_to_feet_m * self.scale
    }
}

/// The bird species by index ([`plan::flock_species`]).
pub const BIRDS: [BirdPiece; plan::BIRD_SPECIES as usize] = [
    BirdPiece {
        id: "bird_starling",
        hinge: [0.020, 0.014],
        fold_k: 0.25,
        fold_roll_deg: -60.0,
        fold_back: 0.015,
        pivot_to_feet_m: 0.026,
        scale: 4.0,
    },
    BirdPiece {
        id: "bird_goldfinch",
        hinge: [0.013, 0.0095],
        fold_k: 0.25,
        fold_roll_deg: -60.0,
        fold_back: 0.010,
        pivot_to_feet_m: 0.018,
        scale: 5.0,
    },
];
/// Tail-wag amplitude (model m) at the caudal fork tips: x += U^2 * FISH_WAG * sin(phase).
pub const FISH_WAG: f32 = 0.035;
/// The river palette's U offset in the fish atlas (ocean in the left half, river in the right).
pub const RIVER_U: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, ShaderType, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Boid {
    /// xyz position; w the generation of [`Ctl`] it was spawned from.
    pub pos: [f32; 4],
    /// xyz velocity; w what it is doing (fly, settle, perch, swim; see the compute shader).
    pub vel: [f32; 4],
    /// Animation the compute pass keeps: x wing-beat or tail-wag phase, y flap drive (0 gliding,
    /// 1 flapping), z the fish palette's U offset, w a river fish's growth over its display scale
    /// (`plan::RIVER_GROW`).
    pub anim: [f32; 4],
}

/// One slot as the CPU sets it: `spawn` xyz and generation (a new generation resets the boid
/// there), `info` (kind: 0 bird 1 fish 2 idle 3 leaving bird, roost index or -1, time sent to the roost, a
/// fish's school key: it coheres only with fish of the same key), `look` (x: the fish palette's U
/// offset, 0 ocean or [`RIVER_U`]).
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Ctl {
    pub spawn: [f32; 4],
    pub info: [f32; 4],
    pub look: [f32; 4],
}

/// A fish's palette offset: the river palette where it spawns in river water (a river or spring
/// tile, [`plan::river_water`], the water that decides its grouping), the ocean palette on mouth,
/// coast and ocean tiles (owner Q5).
pub fn fish_palette(
    cells: &std::collections::BTreeMap<hex_core::Axial, CellWater>,
    spawn: Vec3,
) -> f32 {
    if cells
        .get(&plan::cell_at(spawn.xz()))
        .is_some_and(|c| plan::river_water(&c.kind))
    {
        RIVER_U
    } else {
        0.0
    }
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
            anim: [0.0; 4],
        })
        .collect()
}

/// `copies` of a piece's LOD0 in one mesh (piece-local positions, normals, UV0, UV1 and tangents):
/// the vertex shader maps copy `k` to boid slot `base + k` and poses and places it there. `None`
/// when the piece lacks one of those attributes.
pub fn form_mesh(src: &Mesh, copies: u32) -> Option<Mesh> {
    use VertexAttributeValues::{Float32x2, Float32x3, Float32x4};
    let (
        Some(Float32x3(p)),
        Some(Float32x3(n)),
        Some(Float32x2(uv)),
        Some(Float32x2(uv1)),
        Some(Float32x4(t)),
    ) = (
        src.attribute(Mesh::ATTRIBUTE_POSITION),
        src.attribute(Mesh::ATTRIBUTE_NORMAL),
        src.attribute(Mesh::ATTRIBUTE_UV_0),
        src.attribute(Mesh::ATTRIBUTE_UV_1),
        src.attribute(Mesh::ATTRIBUTE_TANGENT),
    )
    else {
        return None;
    };
    let verts = p.len() as u32;
    let one: Vec<u32> = src.indices()?.iter().map(|i| i as u32).collect();
    let mut idx = Vec::with_capacity(one.len() * copies as usize);
    for k in 0..copies {
        idx.extend(one.iter().map(|i| i + k * verts));
    }
    let n_copies = copies as usize;
    Some(
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, p.repeat(n_copies))
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, n.repeat(n_copies))
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv.repeat(n_copies))
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, uv1.repeat(n_copies))
        .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, t.repeat(n_copies))
        .with_inserted_indices(Indices::U32(idx)),
    )
}

/// The boid slots a form's mesh covers: copy `k` draws slot
/// `base + ((k / flock) * stride + offset) * flock + k % flock` ([`Self::slot`], as
/// `boid_material.wesl` maps it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormSlots {
    pub base: u32,
    pub copies: u32,
    pub flock: u32,
    pub stride: u32,
    pub offset: u32,
}

impl FormSlots {
    /// Bird species `species`: every slot of its flocks ([`plan::flock_species`]) in the budget, so
    /// the two species' meshes together hold one copy per slot (the stress flock is all birds).
    pub fn bird(species: u32, budget: BoidBudget, seed: u64) -> Self {
        Self {
            base: 0,
            copies: plan::species_slots(species, seed, budget.total),
            flock: plan::FLOCK_SIZE,
            stride: plan::BIRD_SPECIES,
            offset: plan::species_offset(species, seed),
        }
    }

    /// A fish species: every slot from [`BoidAssets::fish_base`]. A fish's species follows its spawn
    /// water, which each world decides, so every fish species' mesh covers every fish slot and the
    /// vertex shader draws only its own water's fish there ([`fish_form_draws`]).
    pub fn fish(budget: BoidBudget) -> Self {
        let base = BoidAssets::fish_base(budget);
        Self {
            base,
            copies: budget.total - base,
            flock: 1,
            stride: 1,
            offset: 0,
        }
    }

    pub fn slot(&self, copy: u32) -> u32 {
        self.base
            + ((copy / self.flock) * self.stride + self.offset) * self.flock
            + copy % self.flock
    }
}

/// Which piece a [`BoidExt`] draws and how: the slots it covers, its display scale and the bird
/// and fish contracts. Mirrors `BoidForm` in `boid_material.wesl`.
#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct BoidForm {
    /// 0 bird, 1 fish.
    pub kind: u32,
    /// [`FormSlots::base`].
    pub base: u32,
    /// Vertices per copy.
    pub boid_verts: u32,
    /// [`FLAG_MASKS`], [`FLAG_RIVER_FISH`].
    pub flags: u32,
    pub scale: f32,
    /// Perched lift (m): the pivot's height over the feet at display scale.
    pub lift: f32,
    pub wag: f32,
    pub fold_k: f32,
    pub hinge: Vec2,
    pub fold_roll: f32,
    pub fold_back: f32,
    /// [`FormSlots::flock`], [`FormSlots::stride`] and [`FormSlots::offset`].
    pub flock: u32,
    pub stride: u32,
    pub offset: u32,
}

/// StandardMaterial (the piece's albedo, normal and ORM maps) + the boid buffer and pose: the
/// vertex shader (`boid_material.wesl`) replaces Bevy's. Sampled-texture budget: the extension
/// binds only `masks`, as `KitExt` does, so a boid pipeline samples the kit forward pipeline's
/// count (15 with the IBL, 16 in a view with refracting water); the boid buffer is a vertex-stage
/// storage binding.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct BoidExt {
    #[uniform(100)]
    pub form: BoidForm,
    #[texture(101)]
    #[sampler(102)]
    pub masks: Option<Handle<Image>>,
    #[storage(103, read_only, visibility(vertex))]
    pub boids: Handle<ShaderBuffer>,
}

pub type BoidMaterial = ExtendedMaterial<StandardMaterial, BoidExt>;

impl MaterialExtension for BoidExt {
    fn vertex_shader() -> ShaderRef {
        MATERIAL_SHADER.into()
    }
    fn fragment_shader() -> ShaderRef {
        MATERIAL_SHADER.into()
    }
    /// The default prepass and shadow shaders know nothing of the boid buffer and would draw
    /// every copy at its rest position around the origin. Staying out of the depth prepass also
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

/// One drawn form: a merged mesh of piece copies and its material.
#[derive(Clone, Debug)]
pub struct BoidFormAssets {
    pub mesh: Handle<Mesh>,
    pub material: Handle<BoidMaterial>,
}

/// The flock's handles for the current budget, re-used each time the scene is (re)built, so a
/// scenario switch neither duplicates nor leaks them. `forms` (each bird species in [`BIRDS`]
/// order, then each fish species in [`FISH`] order) are built once the pieces have loaded;
/// `generation` counts the builds, so
/// [`sync_flock`] respawns the flock entities only when the handles change.
#[derive(Resource, Clone)]
pub struct BoidAssets {
    pub boids: Handle<ShaderBuffer>,
    pub budget: BoidBudget,
    pub forms: Option<Vec<BoidFormAssets>>,
    pub generation: u64,
}

impl BoidAssets {
    /// The first slot a fish can hold: play's birds fill slots `0..birds` and the fish follow, and
    /// birds are fewest when the fish are at their cap. The fish mesh covers the slots from here.
    pub fn fish_base(budget: BoidBudget) -> u32 {
        let capped = budget.fish_max.div_ceil(budget.fish_per_water_tile.max(1));
        budget.split(capped as usize).0
    }
}

/// A boid piece's form: a bird species ([`BIRDS`] index) or a fish species ([`FISH`] index).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Form {
    Bird(u32),
    Fish(u32),
}

/// A piece the boids draw, loading: its GLB and its material without the boid binding, and the
/// species it is.
struct BoidPieceLoad {
    id: &'static str,
    form: Form,
    glb: Handle<Gltf>,
    base: StandardMaterial,
    masks: Option<Handle<Image>>,
}

/// The boid pieces (the birds, the fish), requested from `kit://` once the render device picks
/// the texture family, in every scenario. The flock flies from the first frame, so all are in the
/// manifest's `core` pack: in play these are the same files (and handles) core fetches.
#[derive(Resource)]
pub struct BoidPieces {
    loads: Vec<BoidPieceLoad>,
    /// Every piece loaded (or one failed: the flock stays undrawn and the report says so).
    pub state: PieceState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceState {
    Loading,
    Ready,
    Failed,
}

/// Marks the flock entities of one [`BoidAssets::generation`].
#[derive(Component, Clone, Copy, Debug)]
pub struct BoidFlock(pub u64);

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
    /// The plan as last uploaded, shared by the [`FishTrack`] readbacks tagged since; cleared
    /// whenever slots or the world upload again.
    tracked: Option<Arc<BoidPlan>>,
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
    /// Birds per play flock ([`plan::FLOCK_SIZE`]).
    flock: u32,
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
            tracked: None,
        })
        .init_resource::<BoidReport>()
        .add_systems(
            Startup,
            (move |commands: Commands, buffers: ResMut<Assets<ShaderBuffer>>| {
                init_boids(budget, commands, buffers)
            })
            .before(crate::scene::setup),
        )
        .add_systems(
            Update,
            (
                plan_boids.after(PlaySystems),
                (load_pieces, build_forms, sync_flock).chain(),
                (track_fish, drop_skipped_samples).chain().after(plan_boids),
            ),
        );

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

/// Resizes the flock for a new budget (a tier change): new buffers, meshes and materials, and a
/// fresh plan on the next frame. [`sync_flock`] spawns the flock from the new `BoidAssets`.
pub fn set_budget(world: &mut World, budget: BoidBudget) {
    let Some(mut state) = world.get_resource_mut::<BoidState>() else {
        return;
    };
    if state.plan.budget == budget {
        return;
    }
    state.plan.budget = budget;
    state.scenario = None;
    if let Err(e) = world.run_system_cached_with(init_boids_with, budget) {
        warn!("boids: resize to {} failed: {e}", budget.total);
    }
}

fn init_boids_with(
    In(budget): In<BoidBudget>,
    commands: Commands,
    buffers: ResMut<Assets<ShaderBuffer>>,
) {
    init_boids(budget, commands, buffers);
}

fn init_boids(
    budget: BoidBudget,
    mut commands: Commands,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
) {
    let count = budget.total;
    // RENDER_WORLD only: the GPU owns the state after upload, and a main-world copy that
    // changed would overwrite the simulation. Generation 0 never matches a slot, so the first
    // dispatch spawns every boid from its `Ctl`. COPY_SRC: [`FishTrack`] reads it back.
    let boids = storage(&mut buffers, vec![Boid::zeroed(); count as usize], "boids");
    if let Some(mut b) = buffers.get_mut(&boids) {
        b.buffer_usage |= BufferUsages::COPY_SRC;
    }
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
        boids,
        budget,
        forms: None,
        generation: 0,
    });
}

/// The boid pieces in form order: each bird species ([`BIRDS`]), then each fish species ([`FISH`]).
fn boid_pieces() -> impl Iterator<Item = (&'static str, Form)> {
    (0..)
        .zip(BIRDS)
        .map(|(s, b)| (b.id, Form::Bird(s)))
        .chain((0..).zip(FISH).map(|(s, f)| (f.id, Form::Fish(s))))
}

/// Requests the boid pieces ([`boid_pieces`]) once the render device has picked the texture family.
fn load_pieces(
    mut commands: Commands,
    pieces: Option<Res<BoidPieces>>,
    device: Option<Res<RenderDevice>>,
    server: Res<AssetServer>,
) {
    let (None, Some(device)) = (pieces, device) else {
        return;
    };
    let f = device.features();
    let family = hex_assets::select_family(
        f.contains(WgpuFeatures::TEXTURE_COMPRESSION_BC),
        f.contains(WgpuFeatures::TEXTURE_COMPRESSION_ASTC),
    );
    let mut loads = vec![];
    let mut state = PieceState::Loading;
    match hex_assets::parse_manifest(crate::kit_scene::MANIFEST) {
        Ok(m) => {
            for (id, form) in boid_pieces() {
                let entry = m.pieces.iter().find(|e| e.id == id);
                let slot = entry.and_then(|e| e.slots.get(&format!("{id}_body")).map(|s| (e, s)));
                let Some((entry, slot)) = slot else {
                    warn!("boids: piece {id} or its {id}_body slot is not in the manifest");
                    state = PieceState::Failed;
                    continue;
                };
                match crate::kit_material::slot_requests(&m, entry, slot, family) {
                    Ok(r) => {
                        let km = crate::kit_material::build_material(
                            &server,
                            slot,
                            &r,
                            crate::workload::KitHooks::None,
                        );
                        let mut base = km.base;
                        // Wings and fins are single sheets (the GLB material is double-sided).
                        base.double_sided = true;
                        base.cull_mode = None;
                        loads.push(BoidPieceLoad {
                            id,
                            form,
                            glb: server.load(format!("kit://{}", entry.glb)),
                            base,
                            masks: km.extension.masks,
                        });
                    }
                    Err(e) => {
                        warn!("boids: {e}");
                        state = PieceState::Failed;
                    }
                }
            }
        }
        Err(e) => {
            warn!("boids: {e}");
            state = PieceState::Failed;
        }
    }
    commands.insert_resource(BoidPieces { loads, state });
}

/// Once every piece has loaded, builds the forms for the current budget and the world seed: each
/// bird species' mesh covers its own flocks' slots ([`FormSlots::bird`]; the stress flock is all
/// birds), each fish species' mesh the slots from [`BoidAssets::fish_base`], flagged with the water
/// whose fish it draws ([`FLAG_RIVER_FISH`]).
#[allow(clippy::too_many_arguments)]
fn build_forms(
    pieces: Option<ResMut<BoidPieces>>,
    assets: Option<ResMut<BoidAssets>>,
    seed: Option<Res<BoidSeed>>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<BoidMaterial>>,
    mut next_generation: Local<u64>,
) {
    let (Some(mut pieces), Some(mut assets)) = (pieces, assets) else {
        return;
    };
    if assets.forms.is_some() || pieces.state == PieceState::Failed {
        return;
    }
    let failed = |h: &Handle<Gltf>| {
        server.load_state(h).is_failed() || server.recursive_dependency_load_state(h).is_failed()
    };
    if pieces.loads.iter().any(|l| failed(&l.glb)) {
        warn!("boids: a boid piece failed to load; the flock is not drawn");
        pieces.state = PieceState::Failed;
        return;
    }
    if !pieces
        .loads
        .iter()
        .all(|l| server.is_loaded_with_dependencies(&l.glb))
    {
        return;
    }
    let budget = assets.budget;
    let seed = seed.map_or(1, |s| s.0);
    let mut built = vec![];
    let mut drawn = vec![];
    for l in &pieces.loads {
        let lod0 = gltfs
            .get(&l.glb)
            .and_then(|g| g.named_meshes.get(format!("{}_lod0", l.id).as_str()))
            .and_then(|h| gltf_meshes.get(h))
            .and_then(|gm| gm.primitives.first())
            .and_then(|p| meshes.get(&p.mesh));
        let (bird, fish) = match l.form {
            Form::Bird(s) => (Some((s, BIRDS[s as usize])), None),
            Form::Fish(s) => (None, Some(FISH[s as usize])),
        };
        let slots = match bird {
            Some((s, _)) => FormSlots::bird(s, budget, seed),
            None => FormSlots::fish(budget),
        };
        let Some((mesh, verts)) =
            lod0.and_then(|m| Some((form_mesh(m, slots.copies)?, m.count_vertices() as u32)))
        else {
            warn!(
                "boids: {}_lod0 lacks positions, normals, UV0, UV1 or tangents; the flock is not drawn",
                l.id
            );
            pieces.state = PieceState::Failed;
            return;
        };
        // The fish ignore the bird contract; they take the starling's numbers.
        let b = bird.map_or(BIRDS[0], |(_, b)| b);
        let form = BoidForm {
            kind: u32::from(bird.is_none()),
            base: slots.base,
            boid_verts: verts,
            flags: form_flags(l.masks.is_some(), fish),
            scale: bird.map_or(FISH_SCALE, |(_, b)| b.scale),
            lift: bird.map_or(0.0, |(_, b)| b.perch_lift()),
            wag: FISH_WAG,
            fold_k: b.fold_k,
            hinge: Vec2::from(b.hinge),
            fold_roll: b.fold_roll_deg.to_radians(),
            fold_back: b.fold_back,
            flock: slots.flock,
            stride: slots.stride,
            offset: slots.offset,
        };
        drawn.push(format!("{} x{}", l.id, slots.copies));
        built.push(BoidFormAssets {
            mesh: meshes.add(mesh),
            material: materials.add(BoidMaterial {
                base: l.base.clone(),
                extension: BoidExt {
                    form,
                    masks: l.masks.clone(),
                    boids: assets.boids.clone(),
                },
            }),
        });
    }
    if built.len() != BIRDS.len() + FISH.len() {
        return;
    }
    pieces.state = PieceState::Ready;
    *next_generation += 1;
    assets.forms = Some(built);
    assets.generation = *next_generation;
    info!(
        "boids: {} built (fish from slot {})",
        drawn.join(", "),
        BoidAssets::fish_base(budget)
    );
}

/// Keeps one entity per form of the current [`BoidAssets::generation`] (a scene entity, so a
/// scenario switch despawns it with the rest; it is respawned here on the next frame).
fn sync_flock(
    mut commands: Commands,
    assets: Option<Res<BoidAssets>>,
    flock: Query<(Entity, &BoidFlock)>,
) {
    let Some(assets) = assets else {
        return;
    };
    let mut current = false;
    for (e, f) in &flock {
        if f.0 == assets.generation && assets.forms.is_some() {
            current = true;
        } else {
            commands.entity(e).despawn();
        }
    }
    if current {
        return;
    }
    if let Some(forms) = &assets.forms {
        for form in forms {
            commands.spawn((
                Mesh3d(form.mesh.clone()),
                MeshMaterial3d(form.material.clone()),
                // The mesh AABB is one piece at the origin, not where the GPU moved the boids.
                NoFrustumCulling,
                Transform::IDENTITY,
                BoidFlock(assets.generation),
                SceneEntity,
                Name::new("boids"),
            ));
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct BoidWorld<'w> {
    time: Res<'w, Time>,
    active: Res<'w, ActiveScenario>,
    seed: Option<Res<'w, BoidSeed>>,
    view: Res<'w, PlayView>,
    catalog: Res<'w, PlayCatalog>,
    rises: Res<'w, PlayRises>,
    kit: Option<Res<'w, PlayKit>>,
    scatter: Option<Res<'w, ScatterLayouts>>,
    paused: Option<Res<'w, crate::AgentsPaused>>,
    pieces: Option<Res<'w, BoidPieces>>,
    track: Option<Res<'w, FishTrack>>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct BoidOutputs<'w> {
    state: ResMut<'w, BoidState>,
    sim: ResMut<'w, BoidSim>,
    inputs: Option<ResMut<'w, BoidInputs>>,
    buffers: ResMut<'w, Assets<ShaderBuffer>>,
    report: ResMut<'w, BoidReport>,
}

/// The CPU side each frame: (re)starts the plan on a scenario switch, feeds it the placed tiles
/// that reached their rise's agents stage and the tree pieces the kit draws, advances the roost
/// waves, uploads what changed and fills the compute parameters and the report.
fn plan_boids(world: BoidWorld, outputs: BoidOutputs) {
    let BoidWorld {
        time,
        active,
        seed,
        view,
        catalog,
        rises,
        kit,
        scatter,
        paused,
        pieces,
        track,
    } = world;
    let BoidOutputs {
        mut state,
        mut sim,
        inputs,
        mut buffers,
        mut report,
    } = outputs;
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
    let tracked = &mut state.tracked;
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
    if plan.slots_dirty || plan.world_dirty {
        *tracked = None;
    }
    if plan.slots_dirty {
        let birds = plan.birds as usize;
        let ctl: Vec<Ctl> = plan
            .slots
            .iter()
            .enumerate()
            .map(|(i, s)| Ctl {
                spawn: [s.spawn.x, s.spawn.y, s.spawn.z, s.generation as f32],
                info: [
                    match s.kind {
                        plan::SlotKind::Bird => 0.0,
                        plan::SlotKind::Fish => 1.0,
                        plan::SlotKind::Idle => 2.0,
                        plan::SlotKind::Leaving => 3.0,
                    },
                    s.roost
                        .and_then(|k| plan.roost_index(k))
                        .map_or(-1.0, |i| i as f32),
                    s.assigned_at,
                    i.checked_sub(birds)
                        .and_then(|f| plan.schools.get(f))
                        .map_or(0.0, |&k| k as f32),
                ],
                look: [
                    if s.kind == plan::SlotKind::Fish {
                        fish_palette(&plan.water.cells, s.spawn)
                    } else {
                        0.0
                    },
                    0.0,
                    0.0,
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
            .map(|r| [r.pos.x, r.pos.y, r.pos.z, r.face.z.atan2(r.face.x)])
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
        flock: plan::FLOCK_SIZE,
        sky: plan.sky,
    };
    let pieces = match pieces.map(|p| p.state) {
        Some(PieceState::Ready) => "ready",
        Some(PieceState::Failed) => "failed",
        _ => "loading",
    };
    report.0 = report_json(
        &plan.counts(t),
        plan.play,
        dispatched_boids(),
        &river_palettes(plan),
        pieces,
    );
    if let Some(track) = track
        && let Some(open) = report.0.strip_suffix('}')
    {
        report.0 = format!("{open},\"fish_track\":{}}}", track.json());
    }
}

/// Fish crossing between river and sea water, counted from the flock read back each frame: the
/// checkpoint's evidence that fish swim through a river mouth (owner, 2026-10-14). Inserted only
/// for fixed-clock capture runs; play itself never reads the flock back.
#[derive(Resource, Clone, Debug, Default)]
pub struct FishTrack {
    /// Readbacks folded in.
    pub samples: u32,
    /// Fish (a slot and its generation) seen in river water and later in sea water, and the
    /// other way round: across a mouth's fan ([`FAN_S`]), which is neither.
    pub river_to_sea: u32,
    pub sea_to_river: u32,
    /// The same crossings per mouth cell, credited to the last mouth tile the fish swam in
    /// (river_to_sea, sea_to_river): the checkpoint tells the two mouth handednesses apart by cell.
    pub mouths: std::collections::BTreeMap<hex_core::Axial, (u32, u32)>,
    /// Per fish slot: (generation, water first seen in, crossed, last position, boid clock then,
    /// last mouth cell seen in).
    fish: Vec<Option<TrackedFish>>,
    /// Frames [`track_fish`] has run: the frame each readback is tagged with.
    frame: u64,
    /// The next frame to fold in.
    next: u64,
    /// Readbacks that arrived before their turn, by frame.
    pending: std::collections::BTreeMap<u64, TrackSample>,
    /// The last frame folded in (`through`), and an FNV-1a hash of its fish (slot, position and
    /// velocity bits; `digest`): two runs with equal digests drew the same fish at that frame.
    pub through: Option<u64>,
    pub digest: u64,
    /// Readbacks that never arrived within [`TRACK_GIVE_UP`] frames.
    pub skipped: u32,
}

/// One readback and what the GPU ran under when it was copied: the boid clock and the plan.
#[derive(Clone, Debug)]
struct TrackSample {
    t: f32,
    plan: Arc<BoidPlan>,
    boids: Vec<Boid>,
}

/// Frames a readback waits before it is folded in, in frame order: the GPU's map callbacks land a
/// few frames after the copy, on wall time, so folding each one as it landed made the counts
/// depend on the host's load.
pub const TRACK_LAG: u64 = 8;
/// Frames after which a readback that never arrived is skipped (with a warning).
pub const TRACK_GIVE_UP: u64 = 120;

type TrackedFish = (u32, Side, bool, Vec2, f32, Option<hex_core::Axial>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    River,
    Sea,
}

/// The stretch of a river mouth's fan (s, metres seaward of its coastline) a crossing passes:
/// the band was closed from s = -1.17 to 4.37 before fish skimmed it. Upstream of it is river
/// water, past it sea water.
const FAN_S: (f32, f32) = (-3.0, 6.0);

/// Fastest a fish moves (m/s): the compute pass caps its horizontal speed at 2 m/s. A longer jump
/// between two readbacks is a respawn or a stranded fish put back, not a swim.
const TRACK_SPEED: f32 = 2.5;

impl FishTrack {
    fn json(&self) -> String {
        let mouths: Vec<String> = self
            .mouths
            .iter()
            .map(|(c, (out, back))| {
                format!(
                    "{{\"q\":{},\"r\":{},\"river_to_sea\":{out},\"sea_to_river\":{back}}}",
                    c.q, c.r
                )
            })
            .collect();
        format!(
            "{{\"samples\":{},\"crossed\":{},\"river_to_sea\":{},\"sea_to_river\":{},\"mouths\":[{}],\"through\":{},\"digest\":\"{:016x}\",\"skipped\":{}}}",
            self.samples,
            self.river_to_sea + self.sea_to_river,
            self.river_to_sea,
            self.sea_to_river,
            mouths.join(","),
            self.through.map_or("null".to_owned(), |f| f.to_string()),
            self.digest,
            self.skipped,
        )
    }

    /// Takes the readback copied on `frame` (at boid clock `t`, under `plan`), in any order.
    fn arrive(&mut self, frame: u64, t: f32, plan: Arc<BoidPlan>, boids: Vec<Boid>) {
        if frame >= self.next {
            self.pending.insert(frame, TrackSample { t, plan, boids });
        }
    }

    /// Folds in, in frame order, every readback at least [`TRACK_LAG`] frames older than `now`.
    fn advance(&mut self, now: u64) {
        while self.next + TRACK_LAG <= now {
            if let Some(s) = self.pending.remove(&self.next) {
                if s.plan.play {
                    self.observe(&s.plan, &s.boids, s.t);
                    self.digest = fish_digest(&s.plan, &s.boids);
                }
                self.through = Some(self.next);
            } else if self.next + TRACK_GIVE_UP <= now {
                warn!(
                    "boids: fish track readback of frame {} never arrived, skipped",
                    self.next
                );
                self.skipped += 1;
            } else {
                break;
            }
            self.next += 1;
        }
    }

    /// Folds one readback of the flock in at boid clock `t`.
    pub fn observe(&mut self, plan: &BoidPlan, boids: &[Boid], t: f32) {
        self.samples += 1;
        self.fish.resize(plan.slots.len(), None);
        for (i, slot) in plan.slots.iter().enumerate() {
            let b = boids.get(i);
            let fish = slot.kind == plan::SlotKind::Fish;
            let Some(b) = b.filter(|b| fish && b.pos[3] as u32 == slot.generation) else {
                self.fish[i] = None;
                continue;
            };
            let p = Vec2::new(b.pos[0], b.pos[2]);
            let cell = plan::cell_at(p);
            let side = match plan.water.cells.get(&cell).map(|c| &c.kind) {
                Some(k) if plan::river_water(k) => Some(Side::River),
                Some(crate::water::tiles::WaterKind::RiverMouth(coast)) => {
                    let s = coast.seaward(p - Vec2::from(cell.to_world(hex_core::HEX_RADIUS)));
                    if s < FAN_S.0 {
                        Some(Side::River)
                    } else if s > FAN_S.1 {
                        Some(Side::Sea)
                    } else {
                        None
                    }
                }
                Some(_) => Some(Side::Sea),
                None => None,
            };
            let entry = &mut self.fish[i];
            let mouth = matches!(
                plan.water.cells.get(&cell).map(|c| &c.kind),
                Some(crate::water::tiles::WaterKind::RiverMouth(_))
            )
            .then_some(cell);
            let fresh = entry.is_none_or(|(g, _, _, last, at, _)| {
                g != slot.generation || last.distance(p) > TRACK_SPEED * (t - at).max(0.0) + 0.5
            });
            if fresh {
                *entry = side.map(|s| (slot.generation, s, false, p, t, mouth));
                continue;
            }
            let Some((_, first, crossed, last, at, via)) = entry else {
                continue;
            };
            (*last, *at) = (p, t);
            *via = mouth.or(*via);
            if !*crossed && side.is_some_and(|s| s != *first) {
                *crossed = true;
                let river = *first == Side::River;
                if river {
                    self.river_to_sea += 1;
                } else {
                    self.sea_to_river += 1;
                }
                if let Some(c) = *via {
                    let n = self.mouths.entry(c).or_default();
                    if river {
                        n.0 += 1;
                    } else {
                        n.1 += 1;
                    }
                }
            }
        }
    }
}

/// FNV-1a over every live fish's slot, position and velocity bits.
fn fish_digest(plan: &BoidPlan, boids: &[Boid]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for &b in bytes {
            h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
    };
    for (i, (slot, b)) in plan.slots.iter().zip(boids).enumerate() {
        if slot.kind != plan::SlotKind::Fish || b.pos[3] as u32 != slot.generation {
            continue;
        }
        eat(&(i as u32).to_le_bytes());
        for v in b.pos[..3].iter().chain(&b.vel[..3]) {
            eat(&v.to_bits().to_le_bytes());
        }
    }
    h
}

/// A one-frame readback of the flock for [`FishTrack`]: the frame, boid clock and plan it was
/// requested under, which are the ones the compute pass ran with before the copy.
#[derive(Component)]
struct FishSample {
    frame: u64,
    t: f32,
    plan: Arc<BoidPlan>,
}

/// With a [`FishTrack`], reads this frame's flock back once (`ReadbackOnce`: Bevy copies it after
/// this frame's compute pass) and folds in the readbacks old enough to be complete.
fn track_fish(
    mut commands: Commands,
    track: Option<ResMut<FishTrack>>,
    state: ResMut<BoidState>,
    assets: Option<Res<BoidAssets>>,
) {
    let (Some(mut track), Some(assets)) = (track, assets) else {
        return;
    };
    let state = state.into_inner();
    let frame = track.frame;
    track.frame += 1;
    let plan = state
        .tracked
        .get_or_insert_with(|| Arc::new(state.plan.clone()))
        .clone();
    commands
        .spawn((
            ReadbackOnce::buffer(assets.boids.clone()),
            FishSample {
                frame,
                t: state.t,
                plan,
            },
        ))
        .observe(fold_readback);
    track.advance(frame);
}

/// Despawns the readback entities of frames the [`FishTrack`] has moved past: a readback that
/// never landed was skipped (`TRACK_GIVE_UP`), and its entity would otherwise stay for the run.
/// One that landed was despawned by [`fold_readback`] already.
fn drop_skipped_samples(
    mut commands: Commands,
    track: Option<Res<FishTrack>>,
    samples: Query<(Entity, &FishSample)>,
) {
    let Some(track) = track else {
        return;
    };
    for (e, s) in &samples {
        if s.frame < track.next {
            commands.entity(e).despawn();
        }
    }
}

fn fold_readback(
    ev: On<ReadbackComplete>,
    mut commands: Commands,
    samples: Query<&FishSample>,
    track: Option<ResMut<FishTrack>>,
) {
    if let (Ok(s), Some(mut track)) = (samples.get(ev.entity), track) {
        let boids: Vec<Boid> = ev
            .data
            .as_chunks::<{ std::mem::size_of::<Boid>() }>()
            .0
            .iter()
            .map(|c| bytemuck::pod_read_unaligned(c.as_slice()))
            .collect();
        track.arrive(s.frame, s.t, s.plan.clone(), boids);
    }
    commands.entity(ev.entity).despawn();
}

/// River-palette fish per fish region (the plan's region order).
pub fn river_palettes(plan: &BoidPlan) -> Vec<u32> {
    let mut out = vec![0; plan.water.regions.len()];
    for s in &plan.slots {
        if s.kind == plan::SlotKind::Fish
            && fish_palette(&plan.water.cells, s.spawn) == RIVER_U
            && let Some(n) = s.region.and_then(|r| out.get_mut(r))
        {
            *n += 1;
        }
    }
    out
}

/// The `boids` object of the published state. Per fish region, `river_fish`,
/// `river_largest_group` and `river_singles` describe the fish spawned in its river water and
/// `river_palette` counts the fish drawn with the river palette; `pieces` is the bird and fish
/// pieces' load state; `species` is `birds` per bird species ([`BIRDS`] order) and `fish_species`
/// `fish` per fish species ([`FISH`] order).
pub fn report_json(
    c: &BoidCounts,
    play: bool,
    dispatched: u32,
    river_palette: &[u32],
    pieces: &str,
) -> String {
    let regions: Vec<String> = c
        .regions
        .iter()
        .enumerate()
        .map(|(i, g)| {
            format!(
                "{{\"q\":{},\"r\":{},\"tiles\":{},\"fish\":{},\"river\":{},\"largest_group\":{},\"singles\":{},\"river_fish\":{},\"river_largest_group\":{},\"river_singles\":{},\"river_palette\":{}}}",
                g.cell.q,
                g.cell.r,
                g.tiles,
                g.fish,
                g.river,
                g.largest_group,
                g.singles,
                g.river_fish,
                g.river_largest_group,
                g.river_singles,
                river_palette.get(i).copied().unwrap_or(0)
            )
        })
        .collect();
    format!(
        "{{\"mode\":\"{}\",\"budget\":{},\"birds\":{},\"bird_slots\":{},\"leaving\":{},\"fish\":{},\"roosts\":{},\"roosting\":{},\"settling\":{},\"water_tiles\":{},\"fish_regions\":[{}],\"dispatched\":{dispatched},\"pieces\":\"{pieces}\",\"species\":[{}],\"fish_species\":[{}]}}",
        if play { "play" } else { "stress" },
        c.budget,
        c.birds,
        c.bird_slots,
        c.leaving,
        c.fish,
        c.roosts,
        c.roosting,
        c.settling,
        c.water_tiles,
        regions.join(","),
        c.species.map(|n| n.to_string()).join(","),
        c.fish_species.map(|n| n.to_string()).join(",")
    )
}

/// The compute step for a frame: clamped, and zero while photo mode pauses the agents (the flock
/// and the roost waves hold still).
pub fn step_dt(delta: f32, paused: bool) -> f32 {
    if paused { 0.0 } else { delta.min(MAX_DT) }
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
    /// The flock buffer and the copy of it the pass reads (`boids_prev`), refreshed before it runs.
    boids: Buffer,
    prev: Buffer,
}

/// The render world's `boids_prev`: last frame's flock, which the compute pass reads every boid
/// from. Sized to the flock buffer and replaced with it on a budget change.
#[derive(Resource)]
struct BoidsPrev(Buffer);

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
    prev: Option<Res<BoidsPrev>>,
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
    let size = gpu.buffer.size();
    let prev = match prev {
        Some(p) if p.0.size() == size => p.0.clone(),
        _ => {
            let b = device.create_buffer(&BufferDescriptor {
                label: Some("boids_prev"),
                size,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            commands.insert_resource(BoidsPrev(b.clone()));
            b
        }
    };
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
            prev.as_entire_binding(),
        )),
    );
    commands.insert_resource(BoidsBindGroup {
        group,
        count: sim.0.count,
        boids: gpu.buffer.clone(),
        prev,
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
    let encoder = render_context.command_encoder();
    encoder.copy_buffer_to_buffer(
        &bind_group.boids,
        0,
        &bind_group.prev,
        0,
        bind_group.boids.size(),
    );
    let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
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

    #[test]
    fn paused_boids_take_no_step() {
        assert_eq!(step_dt(0.016, true), 0.0);
        assert_eq!(step_dt(0.016, false), 0.016);
        assert_eq!(step_dt(5.0, false), MAX_DT);
    }

    fn piece_mesh() -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32, 0.0, 0.0]; 3])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; 3])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32, 0.0]; 3])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, vec![[0.5f32, 0.0]; 3])
        .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0f32, 0.0, 0.0, 1.0]; 3])
        .with_inserted_indices(Indices::U16(vec![0, 1, 2]))
    }

    #[test]
    fn form_mesh_holds_one_piece_copy_per_slot() {
        let m = form_mesh(&piece_mesh(), 10).unwrap();
        assert_eq!(m.count_vertices(), 30);
        let idx: Vec<usize> = m.indices().unwrap().iter().collect();
        assert_eq!(idx.len(), 30);
        assert_eq!(&idx[27..], &[27, 28, 29]);
        let Some(VertexAttributeValues::Float32x2(uv1)) = m.attribute(Mesh::ATTRIBUTE_UV_1) else {
            panic!("UV1 missing");
        };
        assert!(uv1.iter().all(|w| w[0] == 0.5));
        let mut bare = piece_mesh();
        bare.remove_attribute(Mesh::ATTRIBUTE_UV_1);
        assert!(form_mesh(&bare, 2).is_none());
    }

    /// The fish mesh covers the slots from `fish_base`: play's fish never start lower, at any
    /// water count and tier.
    #[test]
    fn fish_slots_never_start_below_the_fish_mesh() {
        for budget in [
            BoidBudget::HIGH,
            BoidBudget::for_tier(crate::post::QualityTier::Medium),
            BoidBudget::for_tier(crate::post::QualityTier::Low),
        ] {
            let base = BoidAssets::fish_base(budget);
            for water in 0..200 {
                let (birds, fish) = budget.split(water);
                assert!(birds >= base, "{budget:?} water {water}");
                assert!(birds + fish <= budget.total);
            }
        }
    }

    /// The bird species' meshes hold one copy per slot between them, each slot in the species of
    /// its flock, in slot order, at every tier and either seed phase; the fish mesh still covers
    /// the slots from `fish_base`. Two bird draws (one per species) and one fish draw.
    #[test]
    fn bird_forms_draw_each_slot_once_in_its_flocks_species() {
        for budget in [
            BoidBudget::HIGH,
            BoidBudget::for_tier(crate::post::QualityTier::Medium),
            BoidBudget::for_tier(crate::post::QualityTier::Low),
        ] {
            for seed in 0..8u64 {
                let mut drawn = vec![0u32; budget.total as usize];
                for s in 0..plan::BIRD_SPECIES {
                    let f = FormSlots::bird(s, budget, seed);
                    let slots: Vec<u32> = (0..f.copies).map(|k| f.slot(k)).collect();
                    assert!(slots.windows(2).all(|w| w[0] < w[1]), "{budget:?} {seed}");
                    for slot in slots {
                        assert_eq!(plan::flock_species(slot / plan::FLOCK_SIZE, seed), s);
                        drawn[slot as usize] += 1;
                    }
                }
                assert!(drawn.iter().all(|&n| n == 1), "{budget:?} seed {seed}");
                let fish = FormSlots::fish(budget);
                assert_eq!(fish.slot(0), BoidAssets::fish_base(budget));
                assert_eq!(fish.slot(fish.copies - 1), budget.total - 1);
            }
        }
    }

    /// The starling draws at 4x, the goldfinch at 5x (owner A11-Q2, 2026-10-20), each perched with
    /// its own feet on the perch; at 5x the goldfinch is still shorter and narrower than the
    /// starling at 4x (real sizes from the GLB extras), so every perch spaced for starlings fits it.
    #[test]
    fn each_bird_species_draws_at_its_own_scale() {
        let by_id = |id: &str| BIRDS.iter().find(|b| b.id == id).copied().unwrap();
        let (starling, goldfinch) = (by_id("bird_starling"), by_id("bird_goldfinch"));
        assert_eq!(starling.scale, 4.0);
        assert_eq!(goldfinch.scale, 5.0);
        assert!((goldfinch.perch_lift() - 0.018 * 5.0).abs() < 1e-6);
        assert!((starling.perch_lift() - 0.026 * 4.0).abs() < 1e-6);
        // (real length, real span) in metres: `realLengthM` and `realSpanM`.
        let (s, g) = ((0.197, 0.40), (0.128, 0.24));
        assert!(g.0 * goldfinch.scale < s.0 * starling.scale);
        assert!(g.1 * goldfinch.scale < s.1 * starling.scale);
    }

    /// Both fish species are lifted under water (owner A12-Q3, 2026-10-20: the mackerel read as a
    /// dark navy silhouette): the river palette by RIVER_LIFT, the sea fish by SEA_LIFT, chosen by
    /// the form's river flag. Captured on docs/checkpoints/fish_mackerel (lift-before-*.jpg): the
    /// school's fish pixels from luma 71 to 127 over water at 126.
    #[test]
    fn sea_fish_are_lifted_like_river_fish() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../assets/shaders/boid_material.wesl"),
        )
        .unwrap();
        let constant = |name: &str| -> f32 {
            let line = src
                .lines()
                .find(|l| l.starts_with(&format!("const {name}: f32 = ")))
                .unwrap_or_else(|| panic!("{name} is declared"));
            line.split('=')
                .nth(1)
                .unwrap()
                .trim_end_matches(';')
                .trim()
                .parse()
                .unwrap()
        };
        assert_eq!(constant("RIVER_LIFT"), 1.5);
        assert_eq!(constant("SEA_LIFT"), 2.0);
        assert!(
            src.contains("select(SEA_LIFT, RIVER_LIFT, (form.flags & FLAG_RIVER_FISH) != 0u)"),
            "every fish form is lifted, by its own water's factor"
        );
    }

    /// The constants the shader poses the pieces with are the pieces' own contract (GLB root
    /// extras `boid`, written by scripts/blender/kit/_fauna.py).
    #[test]
    fn boid_contract_matches_the_glb_extras() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/kit/p1");
        let extras = |id: &str| -> serde_json::Value {
            let b = std::fs::read(root.join(format!("{id}/{id}.glb"))).unwrap();
            let len = u32::from_le_bytes(b[12..16].try_into().unwrap()) as usize;
            let j: serde_json::Value = serde_json::from_slice(&b[20..20 + len]).unwrap();
            let node = j["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["name"] == id)
                .unwrap()
                .clone();
            serde_json::from_str(node["extras"]["boid"].as_str().unwrap()).unwrap()
        };
        let close =
            |v: &serde_json::Value, want: f32| (v.as_f64().unwrap() as f32 - want).abs() < 1e-6;
        for b in BIRDS {
            let bird = extras(b.id);
            assert_eq!(bird["form"], "bird", "{}", b.id);
            assert!(
                close(&bird["hinge"]["x"], b.hinge[0]) && close(&bird["hinge"]["y"], b.hinge[1]),
                "{}",
                b.id
            );
            assert!(close(&bird["fold"]["k"], b.fold_k), "{}", b.id);
            assert!(close(&bird["fold"]["rollDeg"], b.fold_roll_deg), "{}", b.id);
            assert!(close(&bird["fold"]["back"], b.fold_back), "{}", b.id);
            assert!(close(&bird["pivotToFeetM"], b.pivot_to_feet_m), "{}", b.id);
        }
        for f in FISH {
            let fish = extras(f.id);
            assert_eq!(fish["form"], "fish", "{}", f.id);
            assert!(
                close(&fish["realLengthM"], 0.40),
                "{}: the trout's length",
                f.id
            );
            assert!(close(&fish["variants"]["ocean"][0], 0.0), "{}", f.id);
            // The river species carries the river palette; a sea species has one palette.
            assert_eq!(fish["variants"].get("river").is_some(), f.river, "{}", f.id);
            if f.river {
                assert!(close(&fish["variants"]["river"][0], RIVER_U), "{}", f.id);
            }
        }
    }

    /// Every fish slot is drawn by exactly one fish form, the species of its spawn water
    /// (`plan::spawn_species`), as the shader picks it from the palette offset; in a river joined
    /// to the sea through a mouth and in separate river and sea regions, at several seeds. The
    /// shader carries the flag values.
    #[test]
    fn fish_forms_draw_each_fish_once_in_its_spawn_waters_species() {
        let c = hex_core::TileCatalog::builtin().unwrap();
        let t = |a: hex_core::Axial, id: &str| crate::play::PlayTile {
            cell: a,
            tile: id.into(),
            rotation: 0,
        };
        let o = hex_core::Axial::new(0, 0);
        let (s, m) = (o.neighbor(3), o.neighbor(0));
        let joined = vec![
            t(s, "river_spring"),
            t(o, "river_straight"),
            t(m, "river_mouth"),
            t(m.neighbor(0), "ocean"),
            t(m.neighbor(0).neighbor(1), "ocean"),
        ];
        let apart = vec![
            t(hex_core::Axial::new(-3, 0), "river_straight"),
            t(hex_core::Axial::new(-2, 0), "river_straight"),
            t(hex_core::Axial::new(3, 0), "ocean"),
            t(hex_core::Axial::new(4, 0), "ocean"),
        ];
        let flags: Vec<u32> = FISH.iter().map(|&f| form_flags(true, Some(f))).collect();
        for world in [joined, apart] {
            for seed in 1..5u64 {
                let mut plan = BoidPlan::play(BoidBudget::HIGH, seed);
                plan.set_world(&world, &c, &|_| true, 0.0);
                let fish = FormSlots::fish(plan.budget);
                let mut drawn = [0u32; plan::FISH_SPECIES as usize];
                for (i, slot) in plan.slots.iter().enumerate() {
                    if slot.kind != plan::SlotKind::Fish {
                        continue;
                    }
                    assert!(
                        (fish.slot(0)..=fish.slot(fish.copies - 1)).contains(&(i as u32)),
                        "both fish meshes cover slot {i}"
                    );
                    let palette = fish_palette(&plan.water.cells, slot.spawn);
                    let forms: Vec<usize> = (0..FISH.len())
                        .filter(|&k| fish_form_draws(flags[k], palette))
                        .collect();
                    let species = plan::spawn_species(&plan.water.cells, slot.spawn);
                    assert_eq!(forms, vec![species as usize], "seed {seed} slot {i}");
                    drawn[species as usize] += 1;
                }
                let counts = plan.counts(0.0);
                assert_eq!(drawn, counts.fish_species, "seed {seed}");
                assert!(
                    drawn.iter().all(|&n| n > 0),
                    "seed {seed}: both species {drawn:?}"
                );
            }
        }
        let src = include_str!("../../../assets/shaders/boid_material.wesl");
        for (name, want) in [
            ("FLAG_MASKS", FLAG_MASKS),
            ("FLAG_RIVER_FISH", FLAG_RIVER_FISH),
        ] {
            let line = format!("const {name}: u32 = {want}u;");
            assert!(src.contains(&line), "boid_material.wesl lacks `{line}`");
        }
        assert!(src.contains(&format!("const RIVER_U: f32 = {RIVER_U:?};")));
    }

    /// A fish takes the river palette where it spawns in a river tile and the ocean palette on
    /// coast and ocean tiles; the report counts them per region.
    #[test]
    fn fish_take_the_palette_of_their_spawn_water() {
        let c = hex_core::TileCatalog::builtin().unwrap();
        let t = |q, r, id: &str| crate::play::PlayTile {
            cell: hex_core::Axial::new(q, r),
            tile: id.into(),
            rotation: 0,
        };
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 1);
        let tiles = [
            t(-3, 0, "river_straight"),
            t(-2, 0, "river_straight"),
            t(3, 0, "ocean"),
            t(4, 0, "ocean"),
        ];
        plan.set_world(&tiles, &c, &|_| true, 0.0);
        let counts = plan.counts(0.0);
        let palettes = river_palettes(&plan);
        assert_eq!(palettes.len(), counts.regions.len());
        let mut seen = (false, false);
        for (g, &n) in counts.regions.iter().zip(&palettes) {
            if g.river {
                assert_eq!(n, g.fish, "{g:?}");
                seen.0 |= g.fish > 0;
            } else {
                assert_eq!(n, 0, "{g:?}");
                seen.1 |= g.fish > 0;
            }
        }
        assert!(seen.0 && seen.1, "{counts:?}");
        let s = report_json(&counts, true, 1000, &palettes, "ready");
        assert!(s.contains("\"river_palette\":"), "{s}");
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
        assert_eq!(std::mem::size_of::<Boid>(), 48);
        assert_eq!(std::mem::size_of::<Ctl>(), 48);
        assert_eq!(Params::min_size().get(), 64);
        // 60 bytes of fields (the slot mapping's three u32s after fold_back), padded to 16.
        assert_eq!(BoidForm::min_size().get(), 64);
    }

    /// Owner feedback (2026-10-14): a perched bird sits level and faces out from its tree, so the
    /// roost the compute pass turns it to is a level unit direction away from the trunk.
    #[test]
    fn perched_birds_face_level_and_out_from_their_tree() {
        let c = hex_core::TileCatalog::builtin().unwrap();
        let tree =
            plan::tree_instances(c.get("oak").unwrap(), hex_core::Axial::new(0, 0))[0].clone();
        let tiles = [crate::play::PlayTile {
            cell: hex_core::Axial::new(0, 0),
            tile: "oak".into(),
            rotation: 2,
        }];
        let roosts = plan::roosts(&tiles, &c, &|_| true);
        assert_eq!(roosts.len(), 12);
        let trunk = crate::kit_scene::piece_rotation(2) * tree.at;
        for r in &roosts {
            assert!(
                (r.face.length() - 1.0).abs() < 1e-4 && r.face.y.abs() < 1e-6,
                "{r:?}"
            );
            let out = (r.pos - trunk).xz().normalize();
            assert!(out.dot(r.face.xz()) > 0.99, "{r:?} faces {out}");
        }
    }

    /// The tracker counts a fish that swims from river water across a mouth into the sea, not one
    /// that turns back in the mouth or one put back at its spawn.
    #[test]
    fn fish_track_counts_mouth_crossings() {
        let c = hex_core::TileCatalog::builtin().unwrap();
        let o = hex_core::Axial::new(0, 0);
        let (s, m) = (o.neighbor(3), o.neighbor(0));
        let sea = m.neighbor(0);
        let t = |a: hex_core::Axial, id: &str| crate::play::PlayTile {
            cell: a,
            tile: id.into(),
            rotation: 0,
        };
        let world = [
            t(s, "river_spring"),
            t(o, "river_straight"),
            t(m, "river_mouth"),
            t(sea, "ocean"),
        ];
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 1);
        plan.set_world(&world, &c, &|_| true, 0.0);
        let b = plan.birds as usize;
        let at = |cell: hex_core::Axial, i: usize| {
            let mut v = vec![Boid::zeroed(); plan.slots.len()];
            let [x, z] = cell.to_world(hex_core::HEX_RADIUS);
            v[i].pos = [x, -1.0, z, plan.slots[i].generation as f32];
            v
        };
        let mut track = FishTrack::default();
        // Fish b swims river, mouth, sea at 1.5 m/s; fish b + 1 goes river, mouth and back.
        let step = |from: hex_core::Axial, to: hex_core::Axial| {
            let [x0, z0] = from.to_world(hex_core::HEX_RADIUS);
            let [x1, z1] = to.to_world(hex_core::HEX_RADIUS);
            Vec2::new(x1 - x0, z1 - z0).length() / 1.5
        };
        let mut clock = 0.0;
        track.observe(&plan, &at(o, b), clock);
        clock += step(o, m);
        track.observe(&plan, &at(m, b), clock);
        clock += step(m, sea);
        track.observe(&plan, &at(sea, b), clock);
        assert_eq!((track.river_to_sea, track.sea_to_river), (1, 0));
        track.observe(&plan, &at(o, b + 1), clock);
        clock += step(o, m);
        track.observe(&plan, &at(m, b + 1), clock);
        clock += step(m, o);
        track.observe(&plan, &at(o, b + 1), clock);
        // A jump straight from the river to the sea is a respawn, not a crossing.
        track.observe(&plan, &at(sea, b + 1), clock + 0.1);
        assert_eq!((track.river_to_sea, track.sea_to_river), (1, 0));
        assert!(track.json().contains("\"crossed\":1"), "{}", track.json());
        assert_eq!(track.mouths.get(&m), Some(&(1, 0)));
    }

    /// With a river_mouth and a mirrored river_mouth_l in one world, each crossing is credited to
    /// the mouth the fish swam through, in both directions.
    #[test]
    fn fish_track_credits_each_mouth_handedness() {
        let c = hex_core::TileCatalog::builtin().unwrap();
        let t = |a: hex_core::Axial, id: &str, rotation: u8| crate::play::PlayTile {
            cell: a,
            tile: id.into(),
            rotation,
        };
        // river_mouth: River on edge 3 at rotation 0; river_mouth_l: River on edge 4.
        let o = hex_core::Axial::new(0, 0);
        let m = o.neighbor(0);
        let ml = hex_core::Axial::new(0, 6);
        let ol = ml.neighbor(4);
        let world = [
            t(o.neighbor(3), "river_spring", 0),
            t(o, "river_straight", 0),
            t(m, "river_mouth", 0),
            t(m.neighbor(0), "ocean", 0),
            t(ol.neighbor(4), "river_spring", 1),
            t(ol, "river_straight", 1),
            t(ml, "river_mouth_l", 0),
            t(ml.neighbor(0), "ocean", 0),
        ];
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 1);
        plan.set_world(&world, &c, &|_| true, 0.0);
        let b = plan.birds as usize;
        let at = |cell: hex_core::Axial, i: usize| {
            let mut v = vec![Boid::zeroed(); plan.slots.len()];
            let [x, z] = cell.to_world(hex_core::HEX_RADIUS);
            v[i].pos = [x, -1.0, z, plan.slots[i].generation as f32];
            v
        };
        let mut track = FishTrack::default();
        let mut clock = 0.0;
        let mut swim = |track: &mut FishTrack, path: &[hex_core::Axial], i: usize| {
            for (k, &cell) in path.iter().enumerate() {
                if k > 0 {
                    let [x0, z0] = path[k - 1].to_world(hex_core::HEX_RADIUS);
                    let [x1, z1] = cell.to_world(hex_core::HEX_RADIUS);
                    clock += Vec2::new(x1 - x0, z1 - z0).length() / 1.5;
                }
                track.observe(&plan, &at(cell, i), clock);
            }
        };
        let sea_l = ml.neighbor(0);
        swim(&mut track, &[ol, ml, sea_l], b);
        swim(&mut track, &[sea_l, ml, ol], b + 1);
        swim(&mut track, &[o, m, m.neighbor(0)], b + 2);
        assert_eq!((track.river_to_sea, track.sea_to_river), (2, 1));
        assert_eq!(track.mouths.get(&ml), Some(&(1, 1)));
        assert_eq!(track.mouths.get(&m), Some(&(1, 0)));
        let json = track.json();
        assert!(
            json.contains("{\"q\":0,\"r\":6,\"river_to_sea\":1,\"sea_to_river\":1}"),
            "{json}"
        );
    }

    /// The compute pass's copies of the plan's fish constants agree with it.
    #[test]
    fn compute_shader_constants_match_the_plan() {
        let src = include_str!("../../../assets/shaders/boids_compute.wgsl");
        for (name, want) in [
            ("LEVEL_BAND", plan::LEVEL_BAND),
            ("RIVER_GROW", plan::RIVER_GROW),
            ("TROUT_TOP", plan::TROUT_TOP),
            ("SKIM_GAP", plan::SKIM_GAP),
            ("WATER_Y", crate::water::tiles::WATER_Y),
        ] {
            let line = src
                .lines()
                .find(|l| l.starts_with(&format!("const {name}: f32 = ")))
                .unwrap_or_else(|| panic!("{name} missing"));
            let got: f32 = line
                .trim_end_matches(';')
                .rsplit(' ')
                .next()
                .unwrap()
                .parse()
                .unwrap();
            assert_eq!(got, want, "{name}");
        }
    }

    /// The tracker's counts and digest depend on which frame each readback was copied on, not on
    /// when it lands: readbacks arriving late, two at once or out of order fold in to the same
    /// result, and one that never arrives is skipped after `TRACK_GIVE_UP` frames.
    #[test]
    fn fish_track_folds_readbacks_in_frame_order_however_they_land() {
        let c = hex_core::TileCatalog::builtin().unwrap();
        let o = hex_core::Axial::new(0, 0);
        let (s, m) = (o.neighbor(3), o.neighbor(0));
        let sea = m.neighbor(0);
        let t = |a: hex_core::Axial, id: &str| crate::play::PlayTile {
            cell: a,
            tile: id.into(),
            rotation: 0,
        };
        let world = [
            t(s, "river_spring"),
            t(o, "river_straight"),
            t(m, "river_mouth"),
            t(sea, "ocean"),
        ];
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 1);
        plan.set_world(&world, &c, &|_| true, 0.0);
        let plan = Arc::new(plan);
        let b = plan.birds as usize;
        // Fish b swims from the river cell through the mouth into the sea at 1.5 m/s, a frame
        // every 1/60 s.
        let [x0, z0] = o.to_world(hex_core::HEX_RADIUS);
        let [x1, z1] = sea.to_world(hex_core::HEX_RADIUS);
        let (from, to) = (Vec2::new(x0, z0), Vec2::new(x1, z1));
        let frames = ((to - from).length() / 1.5 * 60.0) as u64 + 2;
        let sample = |f: u64| {
            let p = from.lerp(to, (f as f32 / frames as f32).min(1.0));
            let mut v = vec![Boid::zeroed(); plan.slots.len()];
            v[b].pos = [p.x, -1.0, p.y, plan.slots[b].generation as f32];
            (f as f32 / 60.0, v)
        };
        let run = |arrivals: &dyn Fn(u64) -> Vec<u64>| {
            let mut track = FishTrack::default();
            let end = frames + TRACK_LAG + 4;
            for now in 0..=end {
                for f in arrivals(now) {
                    let (clock, v) = sample(f);
                    track.arrive(f, clock, plan.clone(), v);
                }
                track.advance(now);
            }
            track
        };
        // Each readback the frame after its copy; three frames late; in pairs, newest first.
        let prompt = run(&|now| now.checked_sub(1).into_iter().collect());
        let late = run(&|now| now.checked_sub(3).into_iter().collect());
        let pairs = run(&|now| {
            if now % 2 == 1 {
                vec![now, now - 1]
            } else {
                vec![]
            }
        });
        assert_eq!((prompt.river_to_sea, prompt.sea_to_river), (1, 0));
        assert_eq!(prompt.json(), late.json());
        assert_eq!(prompt.json(), pairs.json());
        assert_ne!(prompt.digest, 0);
        // A readback that never lands holds the fold until it is given up on, then is skipped.
        let mut track = FishTrack::default();
        for now in 0..=TRACK_GIVE_UP + 2 {
            if now != 0 {
                let (clock, v) = sample(now.min(frames));
                track.arrive(now, clock, plan.clone(), v);
            }
            track.advance(now);
            if now < TRACK_GIVE_UP {
                assert_eq!(track.through, None, "frame {now}");
            }
        }
        assert_eq!(track.skipped, 1);
        assert_eq!(track.through, Some(TRACK_GIVE_UP + 2 - TRACK_LAG));
    }

    /// A readback skipped after `TRACK_GIVE_UP` frames leaves no entity behind; a readback still
    /// due is kept for its `ReadbackComplete`.
    #[test]
    fn a_skipped_readback_leaves_no_entity_behind() {
        use bevy::ecs::system::RunSystemOnce;
        let plan = Arc::new(BoidPlan::play(BoidBudget::HIGH, 1));
        let mut world = World::new();
        let sample = |frame: u64| FishSample {
            frame,
            t: frame as f32 / 60.0,
            plan: plan.clone(),
        };
        let lost = world.spawn(sample(0)).id();
        let due = world.spawn(sample(TRACK_GIVE_UP)).id();
        let mut track = FishTrack::default();
        for now in 0..=TRACK_GIVE_UP {
            if now != 0 && now + TRACK_LAG <= TRACK_GIVE_UP {
                track.arrive(now, now as f32 / 60.0, plan.clone(), vec![]);
            }
            track.advance(now);
        }
        assert_eq!(track.skipped, 1);
        assert!(track.next > 0 && track.next <= TRACK_GIVE_UP);
        world.insert_resource(track);
        world.run_system_once(drop_skipped_samples).unwrap();
        assert!(
            world.get_entity(lost).is_err(),
            "the skipped readback's entity stayed"
        );
        assert!(
            world.get_entity(due).is_ok(),
            "a readback still due was dropped"
        );
    }

    /// The compute pass reads boids only from `prev` (last frame's copy) and writes only `boids`:
    /// reading neighbours from the buffer it is writing raced, and the flock differed run to run.
    #[test]
    fn compute_pass_reads_last_frame_and_writes_the_flock() {
        let src = include_str!("../../../assets/shaders/boids_compute.wgsl");
        assert!(
            src.contains("var<storage, read> prev: array<Boid>;"),
            "no prev binding"
        );
        for (n, line) in src.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            for (at, _) in code.match_indices("boids[") {
                let rest = &code[at..];
                let close = rest.find(']').expect("boids[ closes");
                assert!(
                    rest[close + 1..].trim_start().starts_with('='),
                    "line {}: boids[] read, not written: {line}",
                    n + 1
                );
            }
        }
    }

    #[test]
    fn report_is_json_with_the_counts() {
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 1);
        let c = hex_core::TileCatalog::builtin().unwrap();
        let tiles: Vec<crate::play::PlayTile> = (0..8)
            .map(|k| crate::play::PlayTile {
                cell: hex_core::Axial::new(k, 0),
                tile: if k == 0 { "oak" } else { "meadow" }.into(),
                rotation: 0,
            })
            .collect();
        plan.set_world(&tiles, &c, &|_| true, 0.0);
        let s = report_json(&plan.counts(0.0), true, 1000, &[], "loading");
        assert!(s.starts_with("{\"mode\":\"play\",\"budget\":1000,\"birds\":24,\"bird_slots\":750,\"leaving\":0,\"fish\":0,\"roosts\":12,\"roosting\":12"), "{s}");
        assert!(
            s.ends_with(
                "\"fish_regions\":[],\"dispatched\":1000,\"pieces\":\"loading\",\"species\":[12,12],\"fish_species\":[0,0]}"
            ),
            "{s}"
        );
    }
}
