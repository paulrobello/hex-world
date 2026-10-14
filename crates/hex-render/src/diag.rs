//! Frame-time percentiles and render-mode facts for the engine gate (PRD P0).
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

use bevy::{
    anti_alias::taa::TemporalAntiAliasing,
    core_pipeline::{core_3d::Opaque3d, prepass::Opaque3dPrepass},
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
        view::ExtractedView,
    },
};

use crate::scene::{Chunk, Lod, SceneFacts, Tile, Tris, Vehicle};

const RING: usize = 600;
/// A frame slower than this missed a 60 Hz presentation (counted as dropped).
const DROPPED_MS: f32 = 25.0;

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

/// What one render phase of one view holds in a census frame (I4: main, prepass and each
/// shadow cascade counted separately).
#[derive(Clone, Debug, Default)]
pub struct PassCensus {
    /// `main` (the camera) or `cascade<i>`.
    pub view: String,
    /// `opaque`, `prepass` or `shadow`.
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

/// Counts what each view's opaque, prepass and shadow phases will draw, when the main world
/// asked for a census (outside the measured window).
fn census_passes(
    cameras: Query<&ExtractedView, With<bevy::render::camera::ExtractedCamera>>,
    lights: Query<(&ExtractedView, &LightEntity)>,
    opaque: Res<ViewBinnedRenderPhases<Opaque3d>>,
    prepass: Res<ViewBinnedRenderPhases<Opaque3dPrepass>>,
    shadow: Res<ViewBinnedRenderPhases<Shadow>>,
) {
    if !PASS_CENSUS_REQUESTED.swap(false, Ordering::Relaxed) {
        return;
    }
    let mut out = vec![];
    for v in &cameras {
        if let Some(p) = opaque.get(&v.retained_view_entity) {
            out.push(count_phase("main".into(), "opaque", p));
        }
        if let Some(p) = prepass.get(&v.retained_view_entity) {
            out.push(count_phase("main".into(), "prepass", p));
        }
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
) {
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
) {
    st.frames += 1;
    if in_window(st.frames) {
        st.stats.push(time.delta_secs() * 1000.0);
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
