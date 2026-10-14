//! GPU compute boids inside Bevy's render graph, drawn by a storage-buffer material (PRD §7.2).
//!
//! The flock lives only on the GPU: a compute system in the `RenderGraph` schedule updates a
//! `ShaderBuffer` before the cameras draw, and `BoidMaterial` reads the same buffer in its
//! vertex shader to pose and place one kit piece per boid: `bird_starling` (display x4) and
//! `fish_trout` (display x2), each drawn from one merged copy of its LOD0 per slot it may hold
//! (no per-boid LOD: the GPU moves the boids, so Bevy's visibility ranges never see them). The
//! CPU half ([`plan`]) decides the budget split, where birds roost and when they launch, the
//! water each fish school is confined to and each fish's palette; it uploads that as read-only
//! buffers and never reads the flock back.
//!
//! In `play` the birds fly in small flocks over the world and roost in waves on the oak tiles' oak
//! canopies (a tile's Bird rule; scattered spruces on such a tile would share its cap,
//! `play::scatter`), and the fish school in the water volume of connected water tiles. A world
//! without an oak keeps every bird in the air. Every other scenario keeps the P0 stress flock:
//! every slot a bird over an 800 m square.
use std::{
    borrow::Cow,
    sync::atomic::{AtomicU32, Ordering},
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
    water::tiles::{CellWater, WaterKind},
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
/// The boid kit pieces (owner-approved 2026-10-14) and their display scale over real size.
pub const BIRD_PIECE: &str = "bird_starling";
pub const FISH_PIECE: &str = "fish_trout";
pub const BIRD_SCALE: f32 = 4.0;
pub const FISH_SCALE: f32 = 2.0;
/// The bird's runtime contract, the `boid` object of its GLB root extras (checked by a test): the
/// shoulder hinge line (x = +-HINGE[0], y = HINGE[1]), the perch fold (wing offset scaled by
/// FOLD_K, rolled FOLD_ROLL_DEG, moved FOLD_BACK * U back) and the pivot's height over the feet.
pub const HINGE: [f32; 2] = [0.020, 0.014];
pub const FOLD_K: f32 = 0.25;
pub const FOLD_ROLL_DEG: f32 = -60.0;
pub const FOLD_BACK: f32 = 0.015;
pub const PIVOT_TO_FEET_M: f32 = 0.026;
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
    /// 1 flapping), z the fish palette's U offset, w unused.
    pub anim: [f32; 4],
}

/// One slot as the CPU sets it: `spawn` xyz and generation (a new generation resets the boid
/// there), `info` (kind: 0 bird 1 fish 2 idle, roost index or -1, time sent to the roost, a
/// fish's school key: it coheres only with fish of the same key), `look` (x: the fish palette's U
/// offset, 0 ocean or [`RIVER_U`]).
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Ctl {
    pub spawn: [f32; 4],
    pub info: [f32; 4],
    pub look: [f32; 4],
}

/// A fish's palette offset: the river palette where it spawns in river water (a river tile, and
/// so a spring), the ocean palette on coast and ocean tiles (owner Q5).
pub fn fish_palette(
    cells: &std::collections::BTreeMap<hex_core::Axial, CellWater>,
    spawn: Vec3,
) -> f32 {
    match cells.get(&plan::cell_at(spawn.xz())).map(|c| &c.kind) {
        Some(WaterKind::River) => RIVER_U,
        _ => 0.0,
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

/// Which piece a [`BoidExt`] draws and how: the slots it covers, its display scale and the bird
/// and fish contracts. Mirrors `BoidForm` in `boid_material.wesl`.
#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct BoidForm {
    /// 0 bird, 1 fish.
    pub kind: u32,
    /// The first boid slot the mesh's copy 0 draws.
    pub base: u32,
    /// Vertices per copy.
    pub boid_verts: u32,
    /// 1: the piece has a masks map (AO in A).
    pub flags: u32,
    pub scale: f32,
    /// Perched lift (m): the pivot's height over the feet at display scale.
    pub lift: f32,
    pub wag: f32,
    pub fold_k: f32,
    pub hinge: Vec2,
    pub fold_roll: f32,
    pub fold_back: f32,
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
/// scenario switch neither duplicates nor leaks them. `forms` (bird, fish) are built once the
/// pieces have loaded; `generation` counts the builds, so [`sync_flock`] respawns the flock
/// entities only when the handles change.
#[derive(Resource, Clone)]
pub struct BoidAssets {
    pub boids: Handle<ShaderBuffer>,
    pub budget: BoidBudget,
    pub forms: Option<[BoidFormAssets; 2]>,
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

/// A piece the boids draw, loading: its GLB and its material without the boid binding.
struct BoidPieceLoad {
    id: &'static str,
    glb: Handle<Gltf>,
    base: StandardMaterial,
    masks: Option<Handle<Image>>,
}

/// The boid pieces (bird, fish), requested from `kit://` once the render device picks the
/// texture family, in every scenario. The flock flies from the first frame, so both are in the
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
        boids,
        budget,
        forms: None,
        generation: 0,
    });
}

/// Requests the bird and fish pieces once the render device has picked the texture family.
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
            for id in [BIRD_PIECE, FISH_PIECE] {
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

/// Once both pieces have loaded, builds the bird and fish forms for the current budget: the bird
/// mesh covers every slot (the stress flock is all birds), the fish mesh the slots from
/// [`BoidAssets::fish_base`].
#[allow(clippy::too_many_arguments)]
fn build_forms(
    pieces: Option<ResMut<BoidPieces>>,
    assets: Option<ResMut<BoidAssets>>,
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
    let total = assets.budget.total;
    let fish_base = BoidAssets::fish_base(assets.budget);
    let mut built = vec![];
    for l in &pieces.loads {
        let lod0 = gltfs
            .get(&l.glb)
            .and_then(|g| g.named_meshes.get(format!("{}_lod0", l.id).as_str()))
            .and_then(|h| gltf_meshes.get(h))
            .and_then(|gm| gm.primitives.first())
            .and_then(|p| meshes.get(&p.mesh));
        let fish = l.id == FISH_PIECE;
        let (base, copies) = if fish {
            (fish_base, total - fish_base)
        } else {
            (0, total)
        };
        let Some((mesh, verts)) =
            lod0.and_then(|m| Some((form_mesh(m, copies)?, m.count_vertices() as u32)))
        else {
            warn!(
                "boids: {}_lod0 lacks positions, normals, UV0, UV1 or tangents; the flock is not drawn",
                l.id
            );
            pieces.state = PieceState::Failed;
            return;
        };
        let form = BoidForm {
            kind: u32::from(fish),
            base,
            boid_verts: verts,
            flags: u32::from(l.masks.is_some()),
            scale: if fish { FISH_SCALE } else { BIRD_SCALE },
            lift: if fish {
                0.0
            } else {
                PIVOT_TO_FEET_M * BIRD_SCALE
            },
            wag: FISH_WAG,
            fold_k: FOLD_K,
            hinge: Vec2::from(HINGE),
            fold_roll: FOLD_ROLL_DEG.to_radians(),
            fold_back: FOLD_BACK,
        };
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
    let Ok(forms) = <[BoidFormAssets; 2]>::try_from(built) else {
        return;
    };
    pieces.state = PieceState::Ready;
    *next_generation += 1;
    assets.forms = Some(forms);
    assets.generation = *next_generation;
    info!(
        "boids: {BIRD_PIECE} x{total} and {FISH_PIECE} x{} (slots {fish_base}..) built",
        total - fish_base
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
    (paused, pieces): (Option<Res<crate::AgentsPaused>>, Option<Res<BoidPieces>>),
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

/// The `boids` object of the published state. `river_palette` counts each region's fish drawn
/// with the river palette; `pieces` is the bird and fish pieces' load state.
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
                "{{\"q\":{},\"r\":{},\"tiles\":{},\"fish\":{},\"river\":{},\"largest_group\":{},\"singles\":{},\"river_palette\":{}}}",
                g.cell.q,
                g.cell.r,
                g.tiles,
                g.fish,
                g.river,
                g.largest_group,
                g.singles,
                river_palette.get(i).copied().unwrap_or(0)
            )
        })
        .collect();
    format!(
        "{{\"mode\":\"{}\",\"budget\":{},\"birds\":{},\"fish\":{},\"roosts\":{},\"roosting\":{},\"settling\":{},\"water_tiles\":{},\"fish_regions\":[{}],\"dispatched\":{dispatched},\"pieces\":\"{pieces}\"}}",
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
        let bird = extras(BIRD_PIECE);
        assert_eq!(bird["form"], "bird");
        assert!(close(&bird["hinge"]["x"], HINGE[0]) && close(&bird["hinge"]["y"], HINGE[1]));
        assert!(close(&bird["fold"]["k"], FOLD_K));
        assert!(close(&bird["fold"]["rollDeg"], FOLD_ROLL_DEG));
        assert!(close(&bird["fold"]["back"], FOLD_BACK));
        assert!(close(&bird["pivotToFeetM"], PIVOT_TO_FEET_M));
        let fish = extras(FISH_PIECE);
        assert_eq!(fish["form"], "fish");
        assert!(close(&fish["variants"]["river"][0], RIVER_U));
        assert!(close(&fish["variants"]["ocean"][0], 0.0));
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
        assert_eq!(BoidForm::min_size().get(), 48);
    }

    /// Owner feedback (2026-10-14): a perched bird sits level and faces out from its tree, so the
    /// roost the compute pass turns it to is a level unit direction away from the trunk.
    #[test]
    fn perched_birds_face_level_and_out_from_their_tree() {
        let c = hex_core::TileCatalog::builtin().unwrap();
        let tree = plan::tree_instances(c.get("oak").unwrap())[0].clone();
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

    #[test]
    fn report_is_json_with_the_counts() {
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 1);
        let c = hex_core::TileCatalog::builtin().unwrap();
        let tiles = [crate::play::PlayTile {
            cell: hex_core::Axial::new(0, 0),
            tile: "oak".into(),
            rotation: 0,
        }];
        plan.set_world(&tiles, &c, &|_| true, 0.0);
        let s = report_json(&plan.counts(0.0), true, 1000, &[], "loading");
        assert!(s.starts_with("{\"mode\":\"play\",\"budget\":1000,\"birds\":750,\"fish\":0,\"roosts\":12,\"roosting\":12"), "{s}");
        assert!(
            s.ends_with("\"fish_regions\":[],\"dispatched\":1000,\"pieces\":\"loading\"}"),
            "{s}"
        );
    }
}
