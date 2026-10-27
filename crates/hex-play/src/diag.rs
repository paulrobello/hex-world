//! Frame-time percentiles and render-mode facts for the engine gate (PRD P0).
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use bevy::{
    anti_alias::taa::TemporalAntiAliasing,
    core_pipeline::{
        core_3d::{AlphaMask3d, Opaque3d},
        prepass::{AlphaMask3dPrepass, Opaque3dPrepass},
    },
    diagnostic::FrameCount,
    light::CascadeShadowConfig,
    pbr::{LightEntity, Shadow, SsaoPipelineId},
    post_process::{bloom::Bloom, dof::DepthOfField},
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        batching::gpu_preprocessing::{GpuPreprocessingMode, GpuPreprocessingSupport},
        render_phase::{BinnedPhaseItem, BinnedRenderPhase, ViewBinnedRenderPhases},
        render_resource::PipelineCache,
        renderer::RenderAdapterInfo,
        view::{ExtractedView, RetainedViewEntity},
    },
};

use crate::scene::{Chunk, Lod, SceneFacts, Tile, Tris, Vehicle};
use hex_render::kit_instances::{KitInstanceSet, KitInstanceSetData, KitSlotDraw};

const RING: usize = 600;
/// A frame slower than this missed a 60 Hz presentation (counted as dropped).
const DROPPED_MS: f32 = 25.0;
/// Upper bucket edges (ms) of the frame-time histogram; the last bucket is open.
pub const HISTOGRAM_MS: [f32; 7] = [8.33, 16.7, 20.0, 25.0, 33.3, 50.0, 100.0];
/// Stall events (frames over `DROPPED_MS` inside the window) kept for the tail report.
const TAIL_KEEP: usize = 32;
/// Frames either side of a stall searched for its GPU sample and pipeline compiles (readbacks land
/// late and the render world runs a frame behind natively).
const TAIL_JOIN: u32 = 3;

/// Frame on which `hex-app` flips READY. Frame times are sampled only after it, so the
/// percentile window never includes load or scene-build frames.
pub const READY_FRAME: u32 = 120;
/// First frame whose time enters `FrameStats`.
pub const WINDOW_START_FRAME: u32 = READY_FRAME + 1;

fn in_window(frame: u32) -> bool {
    frame >= WINDOW_START_FRAME
}

/// The visible-set census walks every mesh entity, so it runs only outside the first
/// 600-frame window and never adds a spike to the measured frames.
fn census_allowed(frame: u32) -> bool {
    !(WINDOW_START_FRAME..WINDOW_START_FRAME + RING as u32).contains(&frame)
}

/// What the main world marked visible: `ViewVisibility` is set when an entity is visible in
/// any view, the camera or a shadow cascade.
#[derive(Default, Clone, Copy)]
struct VisibleCensus {
    entities: usize,
    /// Distinct (mesh, material) pairs among visible entities: the most draws Bevy can need
    /// after instancing identical pairs.
    mesh_material_pairs: usize,
    tris: u64,
    /// Visible static pieces or chunk meshes per LOD level.
    by_lod: [usize; 3],
}

fn wasm_memory_bytes() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        core::arch::wasm32::memory_size(0) as u64 * 65_536
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0
    }
}

#[derive(Default)]
pub struct FrameStats {
    samples: Vec<f32>,
    next: usize,
}

impl FrameStats {
    pub fn push(&mut self, ms: f32) {
        if self.samples.len() < RING {
            self.samples.push(ms);
        } else {
            self.samples[self.next] = ms;
        }
        self.next = (self.next + 1) % RING;
    }

    fn percentile(&self, p: f32) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let mut v = self.samples.clone();
        v.sort_by(|a, b| a.total_cmp(b));
        let idx = ((p / 100.0) * v.len() as f32).ceil() as usize;
        v[idx.clamp(1, v.len()) - 1]
    }

    pub fn p50(&self) -> f32 {
        self.percentile(50.0)
    }

    pub fn p95(&self) -> f32 {
        self.percentile(95.0)
    }

    pub fn p99(&self) -> f32 {
        self.percentile(99.0)
    }

    pub fn max(&self) -> f32 {
        self.samples.iter().copied().fold(0.0, f32::max)
    }

    /// Frames slower than `ms`: at a 60 Hz cadence, 25 ms marks a missed presentation.
    pub fn over(&self, ms: f32) -> usize {
        self.samples.iter().filter(|&&s| s > ms).count()
    }

    /// Mean frame time: the throughput an uncapped run sustains. When frames queue behind the
    /// GPU, p50 shows the CPU submit time and p95 the backpressure stalls; the mean shows both.
    pub fn mean(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        self.samples.iter().sum::<f32>() / self.samples.len() as f32
    }

    /// Frames per `HISTOGRAM_MS` bucket (`< edge`), plus the open bucket at the end.
    pub fn histogram(&self) -> [usize; HISTOGRAM_MS.len() + 1] {
        let mut h = [0; HISTOGRAM_MS.len() + 1];
        for &s in &self.samples {
            let i = HISTOGRAM_MS
                .iter()
                .position(|&e| s < e)
                .unwrap_or(HISTOGRAM_MS.len());
            h[i] += 1;
        }
        h
    }
}

/// A frame over `DROPPED_MS` inside the measured window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stall {
    /// Bevy `FrameCount` when it was sampled (the key GPU samples carry).
    pub frame: u32,
    pub ms: f32,
}

/// Render-world pipeline compiles still waiting, by `FrameCount` (the last 64 frames).
static WAITING_PIPELINES: Mutex<VecDeque<(u32, u32)>> = Mutex::new(VecDeque::new());

fn waiting_near(frame: u32) -> u32 {
    WAITING_PIPELINES
        .lock()
        .unwrap()
        .iter()
        .filter(|(f, _)| f.abs_diff(frame) <= TAIL_JOIN)
        .map(|(_, w)| *w)
        .max()
        .unwrap_or(0)
}

/// The `stall` object: the window's histogram and each recent stall joined to the GPU sample of
/// the frames around it (when `gpu_timing` is on) and to pipeline compiles in flight.
pub fn stall_json(stats: &FrameStats, tail: &VecDeque<Stall>) -> serde_json::Value {
    let h = stats.histogram();
    let mut edges: Vec<String> = HISTOGRAM_MS.iter().map(|e| format!("<{e}")).collect();
    edges.push(format!(">={}", HISTOGRAM_MS[HISTOGRAM_MS.len() - 1]));
    let events: Vec<serde_json::Value> = tail
        .iter()
        .map(|s| {
            let gpu = crate::gpu_timing::frames_in(
                s.frame.saturating_sub(TAIL_JOIN)..=s.frame + TAIL_JOIN,
            );
            let worst = gpu.iter().max_by(|a, b| a.total_ms.total_cmp(&b.total_ms));
            serde_json::json!({
                "frame": s.frame,
                "ms": (f64::from(s.ms) * 100.0).round() / 100.0,
                "pipelines_waiting": waiting_near(s.frame),
                "gpu_max_ms": worst.map(|g| (f64::from(g.total_ms) * 100.0).round() / 100.0),
                "gpu_top": worst.and_then(|g| g.segments.iter().max_by(|a, b| a.1.total_cmp(&b.1)))
                    .map(|(n, ms)| serde_json::json!([n, (f64::from(*ms) * 100.0).round() / 100.0])),
            })
        })
        .collect();
    serde_json::json!({
        "threshold_ms": DROPPED_MS,
        "histogram": edges.iter().zip(h).map(|(e, n)| serde_json::json!([e, n])).collect::<Vec<_>>(),
        "over_25ms": stats.over(DROPPED_MS),
        "tail": events,
    })
}

/// `base` (a JSON object) with `extra`'s fields appended.
pub fn merge_json(base: &str, extra: &serde_json::Value) -> String {
    let extra = extra.to_string();
    match (
        base.strip_suffix('}'),
        extra.strip_prefix('{').filter(|e| e.len() > 1),
    ) {
        (Some(b), Some(e)) => format!("{b},{e}"),
        _ => base.to_owned(),
    }
}

#[derive(Resource, Default)]
pub struct DiagSnapshot {
    pub json: String,
}

#[derive(Resource, Default)]
struct DiagState {
    stats: FrameStats,
    frames: u32,
    census: VisibleCensus,
    stalls: VecDeque<Stall>,
}

/// Facts about the render setup that the engine gate must log (PRD P0).
#[derive(Resource, Default, Clone, PartialEq, Debug)]
pub struct RenderModes {
    pub ssao: bool,
    /// `GpuPreprocessingSupport::max_supported_mode`: what the device can do, not a per-view mode.
    pub gpu_preprocessing_supported: String,
    pub adapter: String,
}

/// Written by the render world each frame, read by the main world.
static RENDER_MODES: Mutex<Option<RenderModes>> = Mutex::new(None);

/// What one render phase of one view holds in a census frame (I4: main, prepass, their
/// alpha-mask phases and each shadow cascade counted separately).
#[derive(Clone, Debug, Default)]
pub struct PassCensus {
    /// `main` (the camera) or `cascade<i>`.
    pub view: String,
    /// `opaque`, `alpha_mask`, `prepass`, `alpha_mask_prepass` or `shadow`.
    pub phase: &'static str,
    /// Multi-draw-indirect batch sets: one call each, drawing every bin inside (native only;
    /// web reports `PreprocessingOnly`, which turns these into batchable bins).
    pub multidraw_sets: usize,
    /// Batchable bins: one instanced draw per distinct (mesh, material).
    pub batchable_bins: usize,
    /// Unbatchable entities: one draw each.
    pub unbatchable: usize,
    /// Custom (non-mesh) items such as the instanced layout's slots; empty slots skip their draw.
    pub non_mesh: usize,
}

static PASS_CENSUS: Mutex<Vec<PassCensus>> = Mutex::new(Vec::new());
static PASS_CENSUS_REQUESTED: AtomicBool = AtomicBool::new(false);

/// The instanced kit draws of one census frame by piece: `[camera, shadow]` draws, a camera draw
/// counted once in each of its colour and prepass phases and a shadow draw once per cascade, as
/// [`PassCensus`] counts them. `slots` is the instance set's (family, material) draw slots.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KitDrawCensus {
    pub slots: usize,
    pub by_piece: BTreeMap<String, [usize; 2]>,
}

static KIT_CENSUS: Mutex<Option<KitDrawCensus>> = Mutex::new(None);

/// The render entities of `p`'s non-mesh items (the instanced kit's (slot, LOD) draws among them).
fn non_mesh_entities<BPI: BinnedPhaseItem>(
    p: &BinnedRenderPhase<BPI>,
) -> impl Iterator<Item = Entity> + '_ {
    p.non_mesh_items
        .values()
        .flat_map(|b| b.entities.values().copied())
}

/// Adds every kit slot draw among `entities` to `census` under its piece, in column `col`
/// (0 camera, 1 shadow).
fn count_kit_draws(
    census: &mut KitDrawCensus,
    set: &KitInstanceSetData,
    draws: &Query<&KitSlotDraw>,
    entities: impl Iterator<Item = Entity>,
    col: usize,
) {
    for e in entities {
        let Ok(d) = draws.get(e) else { continue };
        let Some(slot) = set.slots.get(d.slot as usize) else {
            continue;
        };
        census.by_piece.entry(slot.piece.to_string()).or_default()[col] += 1;
    }
}

fn count_phase<BPI: BinnedPhaseItem>(
    view: String,
    phase: &'static str,
    p: &BinnedRenderPhase<BPI>,
) -> PassCensus {
    PassCensus {
        view,
        phase,
        multidraw_sets: p.multidrawable_meshes.len(),
        batchable_bins: p.batchable_meshes.len(),
        unbatchable: p
            .unbatchable_meshes
            .values()
            .map(|b| b.entities.len())
            .sum(),
        non_mesh: p.non_mesh_items.values().map(|b| b.entities.len()).sum(),
    }
}

/// A camera's binned main and prepass phases, opaque and alpha-mask (the kit's foliage cards
/// and other `AlphaMode::Mask` materials draw in the mask phases).
struct CameraPhases<'a> {
    opaque: &'a ViewBinnedRenderPhases<Opaque3d>,
    alpha_mask: &'a ViewBinnedRenderPhases<AlphaMask3d>,
    prepass: &'a ViewBinnedRenderPhases<Opaque3dPrepass>,
    alpha_mask_prepass: &'a ViewBinnedRenderPhases<AlphaMask3dPrepass>,
}

/// One camera view's census rows: each of its binned phases that exists.
fn camera_census(view: &RetainedViewEntity, phases: &CameraPhases) -> Vec<PassCensus> {
    let mut out = vec![];
    if let Some(p) = phases.opaque.get(view) {
        out.push(count_phase("main".into(), "opaque", p));
    }
    if let Some(p) = phases.alpha_mask.get(view) {
        out.push(count_phase("main".into(), "alpha_mask", p));
    }
    if let Some(p) = phases.prepass.get(view) {
        out.push(count_phase("main".into(), "prepass", p));
    }
    if let Some(p) = phases.alpha_mask_prepass.get(view) {
        out.push(count_phase("main".into(), "alpha_mask_prepass", p));
    }
    out
}

/// Counts what each view's opaque, alpha-mask, prepass and shadow phases will draw, when the
/// main world asked for a census (outside the measured window).
fn census_passes(
    cameras: Query<&ExtractedView, With<bevy::render::camera::ExtractedCamera>>,
    lights: Query<(&ExtractedView, &LightEntity)>,
    opaque: Res<ViewBinnedRenderPhases<Opaque3d>>,
    alpha_mask: Res<ViewBinnedRenderPhases<AlphaMask3d>>,
    prepass: Res<ViewBinnedRenderPhases<Opaque3dPrepass>>,
    alpha_mask_prepass: Res<ViewBinnedRenderPhases<AlphaMask3dPrepass>>,
    shadow: Res<ViewBinnedRenderPhases<Shadow>>,
    (kit, kit_draws): (Option<Res<KitInstanceSet>>, Query<&KitSlotDraw>),
) {
    if !PASS_CENSUS_REQUESTED.swap(false, Ordering::Relaxed) {
        return;
    }
    if let Some(set) = kit.as_deref() {
        let set = &set.0;
        let mut k = KitDrawCensus {
            slots: set.slots.len(),
            ..default()
        };
        for v in &cameras {
            let view = &v.retained_view_entity;
            for e in [
                opaque
                    .get(view)
                    .map(|p| non_mesh_entities(p).collect::<Vec<_>>()),
                alpha_mask.get(view).map(|p| non_mesh_entities(p).collect()),
                prepass.get(view).map(|p| non_mesh_entities(p).collect()),
                alpha_mask_prepass
                    .get(view)
                    .map(|p| non_mesh_entities(p).collect()),
            ]
            .into_iter()
            .flatten()
            {
                count_kit_draws(&mut k, set, &kit_draws, e.into_iter(), 0);
            }
        }
        for (v, l) in &lights {
            if let (LightEntity::Directional { .. }, Some(p)) =
                (l, shadow.get(&v.retained_view_entity))
            {
                count_kit_draws(&mut k, set, &kit_draws, non_mesh_entities(p), 1);
            }
        }
        *KIT_CENSUS.lock().unwrap() = Some(k);
    }
    let phases = CameraPhases {
        opaque: &opaque,
        alpha_mask: &alpha_mask,
        prepass: &prepass,
        alpha_mask_prepass: &alpha_mask_prepass,
    };
    let mut out = vec![];
    for v in &cameras {
        out.extend(camera_census(&v.retained_view_entity, &phases));
    }
    for (v, l) in &lights {
        if let (LightEntity::Directional { cascade_index, .. }, Some(p)) =
            (l, shadow.get(&v.retained_view_entity))
        {
            out.push(count_phase(format!("cascade{cascade_index}"), "shadow", p));
        }
    }
    out.sort_by(|a, b| (a.phase, &a.view).cmp(&(b.phase, &b.view)));
    *PASS_CENSUS.lock().unwrap() = out;
}

pub struct DiagPlugin;

impl Plugin for DiagPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DiagState>()
            .init_resource::<DiagSnapshot>()
            .init_resource::<RenderModes>()
            .add_systems(Update, (sync_render_modes, sample).chain());
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(
                Render,
                (probe_render_modes, census_passes)
                    .after(RenderSystems::PrepareBindGroups)
                    .before(RenderSystems::Render),
            );
        }
    }
}

/// SSAO counts as on only when a view has a compiled SSAO pipeline: the plugin skips itself
/// silently on GPUs with fewer than 5 storage textures per stage, so the camera component
/// alone proves nothing. (`SsaoPipelines`, the resource the brief names, is private in 0.20.)
/// Only the main SSAO pass is checked: its preprocess-depth and spatial-denoise pipelines live
/// in that private resource, so this probe cannot see whether those two have compiled.
fn probe_render_modes(
    ssao_views: Query<&SsaoPipelineId>,
    cache: Res<PipelineCache>,
    preprocessing: Option<Res<GpuPreprocessingSupport>>,
    adapter: Res<RenderAdapterInfo>,
    frame: Res<FrameCount>,
) {
    {
        let mut w = WAITING_PIPELINES.lock().unwrap();
        if w.len() == 64 {
            w.pop_front();
        }
        w.push_back((frame.0, cache.waiting_pipelines().count() as u32));
    }
    let ssao = ssao_views
        .iter()
        .any(|id| cache.get_compute_pipeline(id.0).is_some());
    let gpu_preprocessing_supported = preprocessing
        .map(|p| {
            // `GpuPreprocessingMode` has no `Debug` in 0.20; spell the variants out.
            match p.max_supported_mode {
                GpuPreprocessingMode::None => "None",
                GpuPreprocessingMode::PreprocessingOnly => "PreprocessingOnly",
                GpuPreprocessingMode::Culling => "Culling",
            }
            .to_string()
        })
        .unwrap_or_else(|| "absent".into());
    let adapter = format!(
        "{} | {:?} | {:?} | {}",
        adapter.name, adapter.backend, adapter.device_type, adapter.driver
    );
    *RENDER_MODES.lock().unwrap() = Some(RenderModes {
        ssao,
        gpu_preprocessing_supported,
        adapter,
    });
}

/// Copies the render-world facts into `RenderModes`; logs them from frame 10 on, once per change.
fn sync_render_modes(
    st: Res<DiagState>,
    mut modes: ResMut<RenderModes>,
    mut logged: Local<Option<RenderModes>>,
) {
    if st.frames < 10 {
        return;
    }
    let Some(latest) = RENDER_MODES.lock().unwrap().clone() else {
        return;
    };
    if *modes != latest {
        *modes = latest.clone();
    }
    if logged.as_ref() != Some(&latest) {
        info!(
            "render modes: ssao={} gpu_preprocessing_supported={} adapter={}",
            latest.ssao, latest.gpu_preprocessing_supported, latest.adapter
        );
        *logged = Some(latest);
    }
}

/// Everything in the snapshot is measured from the running world, not echoed from `GATE`.
#[allow(clippy::too_many_arguments)]
fn sample(
    time: Res<Time>,
    mut st: ResMut<DiagState>,
    mut snap: ResMut<DiagSnapshot>,
    modes: Res<RenderModes>,
    facts: Option<Res<SceneFacts>>,
    tiles: Query<(), With<Tile>>,
    chunks: Query<&Chunk>,
    meshes: Query<(), With<Mesh3d>>,
    drawn: Query<(
        &ViewVisibility,
        &Mesh3d,
        Option<&MeshMaterial3d<StandardMaterial>>,
        Option<&Tris>,
        Option<&Lod>,
    )>,
    vehicles: Query<(), With<Vehicle>>,
    cars: Query<(), With<crate::cars::PlayCar>>,
    cameras: Query<(Has<TemporalAntiAliasing>, Has<Bloom>, Has<DepthOfField>), With<Camera3d>>,
    lights: Query<(&DirectionalLight, &CascadeShadowConfig)>,
    instanced: Option<Res<crate::instanced::InstancedScene>>,
    config: Option<Res<crate::workload::GateConfig>>,
    (frame, gpu_timing, virt): (
        Res<FrameCount>,
        Option<Res<crate::gpu_timing::GpuTiming>>,
        Option<Res<Time<bevy::time::Virtual>>>,
    ),
) {
    st.frames += 1;
    // A frame with virtual time paused (hex-app holding a screenshot's retake) has no game time to
    // measure: its zero delta would pull the percentiles down.
    if in_window(st.frames) && !virt.is_some_and(|v| v.was_paused()) {
        let ms = time.delta_secs() * 1000.0;
        st.stats.push(ms);
        // Stalls follow the 600-frame window, like the percentiles.
        while st
            .stalls
            .front()
            .is_some_and(|s| s.frame + RING as u32 <= frame.0)
        {
            st.stalls.pop_front();
        }
        if ms > DROPPED_MS {
            if st.stalls.len() == TAIL_KEEP {
                st.stalls.pop_front();
            }
            st.stalls.push_back(Stall { frame: frame.0, ms });
        }
    }
    if st.frames.is_multiple_of(30) && census_allowed(st.frames) {
        let mut pairs = std::collections::HashSet::new();
        let mut c = VisibleCensus::default();
        for (vis, mesh, mat, tris, lod) in &drawn {
            if vis.get() {
                c.entities += 1;
                c.tris += tris.map_or(0, |t| t.0 as u64);
                if let Some(l) = lod {
                    c.by_lod[l.0 as usize] += 1;
                }
                pairs.insert((mesh.id(), mat.map(|m| m.id())));
            }
        }
        c.mesh_material_pairs = pairs.len();
        st.census = c;
        PASS_CENSUS_REQUESTED.store(true, Ordering::Relaxed);
        // The kit queue census walks every cell per view and re-ranges each clamped cascade, so it
        // runs only when asked (`HEXW_KIT_QUEUE`, native): census frames fall inside timed windows.
        if kit_queue_enabled() {
            hex_render::kit_instances::request_queue_stats();
        }
        crate::instanced::request_census();
    }
    if st.frames.is_multiple_of(30) {
        // `ssao` comes from the render-world probe; the rest are the camera's components.
        let mut post = vec![];
        if let Some((taa, bloom, dof)) = cameras.iter().next() {
            for (on, name) in [
                (taa, "taa"),
                (modes.ssao, "ssao"),
                (bloom, "bloom"),
                (dof, "dof"),
            ] {
                if on {
                    post.push(format!("\"{name}\""));
                }
            }
        }
        let facts = facts.map(|f| f.clone()).unwrap_or_default();
        let shadows = lights.iter().any(|(l, _)| l.shadow_maps_enabled);
        // Measured from the light: cascade count and the last cascade's far bound.
        let (cascades, shadow_dist) = lights.iter().next().map_or((0, 0.0), |(_, c)| {
            (c.bounds.len(), c.bounds.last().copied().unwrap_or(0.0))
        });
        let casters = config.map_or("all", |c| c.shadow_casters.name());
        let passes = PASS_CENSUS
            .lock()
            .unwrap()
            .iter()
            .map(|p| {
                format!(
                    "{{\"view\":\"{}\",\"phase\":\"{}\",\"multidraw_sets\":{},\"batchable_bins\":{},\"unbatchable\":{},\"non_mesh\":{}}}",
                    p.view, p.phase, p.multidraw_sets, p.batchable_bins, p.unbatchable, p.non_mesh
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let instanced_tiles: usize = instanced
            .as_ref()
            .map_or(0, |s| s.0.set.chunks.iter().map(|c| c.tiles as usize).sum());
        let inst = match (instanced.is_some(), crate::instanced::latest_census()) {
            (true, Some(ic)) => format!(
                "{{\"views\":[{}],\"union_instances_by_lod\":{:?},\"union_tris\":{}}}",
                ic.views
                    .iter()
                    .map(|v| format!(
                        "{{\"view\":\"{}\",\"draws\":{},\"instances\":{},\"tris\":{}}}",
                        v.view, v.draws, v.instances, v.tris
                    ))
                    .collect::<Vec<_>>()
                    .join(","),
                ic.union_instances_by_lod,
                ic.union_tris
            ),
            _ => "null".into(),
        };
        let c = st.census;
        snap.json = format!(
            "{{\"p50\":{:.2},\"p95\":{:.2},\"p99\":{:.2},\"mean\":{:.2},\"max\":{:.2},\"dropped_over_25ms\":{},\"layout\":\"{}\",\"chunk_size\":{},\"chunks\":{},\"shadows\":{},\"shadow_casters\":\"{}\",\"cascades\":{},\"shadow_dist\":{:.0},\"passes\":[{}],\"instanced\":{},\"visible_entities\":{},\"visible_mesh_material_pairs\":{},\"visible_tris\":{},\"visible_by_lod\":{:?},\"wasm_memory_bytes\":{},\"tiles\":{},\"entities\":{},\"pieces\":{},\"vehicles\":{},\"cars\":{},\"boids\":{},\"tris_lod0_per_tile\":{},\"materials\":{},\"post\":[{}],\"ssao\":{},\"gpu_preprocessing_supported\":\"{}\",\"adapter\":\"{}\",\"samples\":{},\"frames\":{},\"window_start_frame\":{}}}",
            st.stats.p50(),
            st.stats.p95(),
            st.stats.p99(),
            st.stats.mean(),
            st.stats.max(),
            st.stats.over(DROPPED_MS),
            facts.layout,
            facts.chunk_size,
            facts.chunks,
            shadows,
            casters,
            cascades,
            shadow_dist,
            passes,
            inst,
            c.entities,
            c.mesh_material_pairs,
            c.tris,
            c.by_lod,
            wasm_memory_bytes(),
            tiles.iter().count()
                + chunks.iter().map(|c| c.tiles as usize).sum::<usize>()
                + instanced_tiles,
            meshes.iter().count(),
            facts.pieces,
            vehicles.iter().count(),
            cars.iter().count(),
            crate::boids::dispatched_boids(),
            facts.tris_lod0_per_tile,
            facts.materials,
            post.join(","),
            modes.ssao,
            modes.gpu_preprocessing_supported,
            modes.adapter.replace(['"', '\\'], "'"),
            st.stats.samples.len(),
            st.frames,
            WINDOW_START_FRAME
        );
        let extra = serde_json::json!({
            "gpu": crate::gpu_timing::diag_json(gpu_timing.is_some_and(|g| g.enabled)),
            "stall": stall_json(&st.stats, &st.stalls),
            "kit_draws": KIT_CENSUS.lock().unwrap().as_ref().map(kit_census_json),
            "kit_queue": hex_render::kit_instances::queue_stats().map(|q| kit_queue_json(&q)),
        });
        snap.json = merge_json(&snap.json, &extra);
    }
}

/// The `kit_draws` snapshot field: the slot count, the total, and each piece's camera, shadow and
/// total draws.
fn kit_census_json(k: &KitDrawCensus) -> serde_json::Value {
    let by_piece: serde_json::Map<String, serde_json::Value> = k
        .by_piece
        .iter()
        .map(|(piece, [cam, sh])| {
            (
                piece.clone(),
                serde_json::json!({"camera": cam, "shadow": sh, "total": cam + sh}),
            )
        })
        .collect();
    let total: usize = k.by_piece.values().map(|[c, s]| c + s).sum();
    serde_json::json!({"slots": k.slots, "total": total, "by_piece": by_piece})
}

/// Whether `HEXW_KIT_QUEUE` asks for the kit queue census (never on the web).
fn kit_queue_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("HEXW_KIT_QUEUE").is_some())
}

/// The `kit_queue` snapshot field: the instanced kit's per-view queue census.
fn kit_queue_json(q: &hex_render::kit_instances::KitQueueStats) -> serde_json::Value {
    let views: Vec<serde_json::Value> = q
        .views
        .iter()
        .map(|v| {
            serde_json::json!({
                "view": v.view,
                "skipped": v.skipped,
                "cells_visible": v.cells_visible,
                "draws": v.draws,
                "instances": v.instances,
                "vertices": v.vertices,
                "vertex_bytes": v.vertex_bytes,
                "color_bytes": v.color_bytes,
                "indices": v.indices,
            })
        })
        .collect();
    serde_json::json!({
        "slots": q.slots,
        "cells": q.cells,
        "bind_groups_created": q.bind_groups_created,
        "views": views,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The alpha-mask phases draw too: a census that skips them undercounts every view.
    #[test]
    fn camera_census_counts_the_alpha_mask_phases() {
        use bevy::{
            asset::AssetIndex,
            core_pipeline::{
                core_3d::{Opaque3dBatchSetKey, Opaque3dBinKey},
                prepass::{OpaqueNoLightmap3dBatchSetKey, OpaqueNoLightmap3dBinKey},
            },
            material::labels::DrawFunctionId,
            render::{
                mesh::allocator::MeshSlabs,
                render_phase::{BinnedRenderPhaseType, InputUniformIndex},
                render_resource::CachedRenderPipelineId,
                sync_world::MainEntity,
            },
        };
        let view = RetainedViewEntity::new(
            MainEntity::from(Entity::from_raw_u32(100).unwrap()),
            None,
            0,
        );
        let item = |i: u32| {
            let e = Entity::from_raw_u32(i).unwrap();
            (e, MainEntity::from(e))
        };
        let mesh = |i: u64| AssetId::<Mesh>::from(AssetIndex::from_bits(i)).untyped();
        let no_lightmap = || OpaqueNoLightmap3dBatchSetKey {
            pipeline: CachedRenderPipelineId::INVALID,
            draw_function: DrawFunctionId(0),
            material_bind_group_index: None,
            slabs: MeshSlabs::default(),
        };
        let mut opaque = ViewBinnedRenderPhases::<Opaque3d>::default();
        let mut alpha_mask = ViewBinnedRenderPhases::<AlphaMask3d>::default();
        let mut prepass = ViewBinnedRenderPhases::<Opaque3dPrepass>::default();
        let mut alpha_mask_prepass = ViewBinnedRenderPhases::<AlphaMask3dPrepass>::default();
        opaque.prepare_for_new_frame(view, GpuPreprocessingMode::Culling);
        alpha_mask.prepare_for_new_frame(view, GpuPreprocessingMode::Culling);
        prepass.prepare_for_new_frame(view, GpuPreprocessingMode::Culling);
        alpha_mask_prepass.prepare_for_new_frame(view, GpuPreprocessingMode::Culling);
        opaque.get_mut(&view).unwrap().add(
            Opaque3dBatchSetKey {
                pipeline: CachedRenderPipelineId::INVALID,
                draw_function: DrawFunctionId(0),
                material_bind_group_index: None,
                slabs: MeshSlabs::default(),
                lightmap_slab: None,
            },
            Opaque3dBinKey { asset_id: mesh(0) },
            item(1),
            InputUniformIndex::default(),
            BinnedRenderPhaseType::NonMesh,
        );
        for i in 0..3 {
            alpha_mask.get_mut(&view).unwrap().add(
                no_lightmap(),
                OpaqueNoLightmap3dBinKey { asset_id: mesh(i) },
                item(10 + i as u32),
                InputUniformIndex::default(),
                BinnedRenderPhaseType::NonMesh,
            );
        }
        for i in 0..2 {
            alpha_mask_prepass.get_mut(&view).unwrap().add(
                no_lightmap(),
                OpaqueNoLightmap3dBinKey { asset_id: mesh(i) },
                item(20 + i as u32),
                InputUniformIndex::default(),
                BinnedRenderPhaseType::NonMesh,
            );
        }
        let rows = camera_census(
            &view,
            &CameraPhases {
                opaque: &opaque,
                alpha_mask: &alpha_mask,
                prepass: &prepass,
                alpha_mask_prepass: &alpha_mask_prepass,
            },
        );
        let got: Vec<(&str, usize)> = rows.iter().map(|r| (r.phase, r.non_mesh)).collect();
        assert_eq!(
            got,
            [
                ("opaque", 1),
                ("alpha_mask", 3),
                ("prepass", 0),
                ("alpha_mask_prepass", 2)
            ]
        );
    }

    #[test]
    fn window_starts_after_ready() {
        assert!(!in_window(READY_FRAME));
        assert!(in_window(READY_FRAME + 1));
        assert_eq!(WINDOW_START_FRAME, READY_FRAME + 1);
    }

    #[test]
    fn census_skips_the_first_window() {
        assert!(census_allowed(READY_FRAME));
        assert!(!census_allowed(WINDOW_START_FRAME));
        assert!(!census_allowed(WINDOW_START_FRAME + 599));
        assert!(census_allowed(WINDOW_START_FRAME + 600));
    }

    #[test]
    fn empty_is_zero() {
        let s = FrameStats::default();
        assert_eq!(s.p50(), 0.0);
        assert_eq!(s.p95(), 0.0);
    }

    #[test]
    fn percentiles_over_known_samples() {
        let mut s = FrameStats::default();
        for i in 1..=100 {
            s.push(i as f32);
        }
        assert_eq!(s.p50(), 50.0);
        assert_eq!(s.p95(), 95.0);
        assert_eq!(s.p99(), 99.0);
        assert_eq!(s.mean(), 50.5);
        assert_eq!(s.max(), 100.0);
        assert_eq!(s.over(25.0), 75);
        assert_eq!(FrameStats::default().max(), 0.0);
    }

    #[test]
    fn histogram_buckets_and_stall_json_parse() {
        let mut s = FrameStats::default();
        for ms in [4.0, 8.0, 10.0, 16.0, 18.0, 24.0, 30.0, 40.0, 60.0, 125.0] {
            s.push(ms);
        }
        assert_eq!(s.histogram(), [2, 2, 1, 1, 1, 1, 1, 1]);
        let mut tail = VecDeque::new();
        tail.push_back(Stall {
            frame: 400,
            ms: 125.0,
        });
        let j = stall_json(&s, &tail);
        assert_eq!(j["over_25ms"], 4);
        assert_eq!(j["histogram"][7][0], ">=100");
        assert_eq!(j["tail"][0]["frame"], 400);
        assert!(j["tail"][0]["gpu_max_ms"].is_null());
    }

    #[test]
    fn merged_snapshot_is_valid_json() {
        let extra = serde_json::json!({ "gpu": crate::gpu_timing::diag_json(false), "stall": { "over_25ms": 0 } });
        let m = merge_json("{\"p95\":3.70,\"adapter\":\"Apple M4 Max\"}", &extra);
        let v: serde_json::Value = serde_json::from_str(&m).unwrap();
        assert_eq!(v["p95"], 3.7);
        assert_eq!(v["gpu"]["enabled"], false);
        assert_eq!(merge_json("{}", &serde_json::json!({})), "{}");
    }

    #[test]
    fn ring_keeps_latest_600() {
        let mut s = FrameStats::default();
        for _ in 0..600 {
            s.push(100.0);
        }
        for _ in 0..600 {
            s.push(10.0);
        }
        assert_eq!(s.p95(), 10.0);
    }
}
