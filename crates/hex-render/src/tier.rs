//! Quality-tier choice (PRD §9.2): an explicit `tier=` key wins; otherwise the tier is detected once
//! the render device exists. Natively the rules read the GPU name, device type and limits. On the
//! web the browser masks the adapter (`BrowserWebGpu | Other`, no name), so the rules read limits
//! and features, then a warm-up frame-time probe may step the tier down once before READY.
//! Detection never picks Ultra, the opt-in tier. A tier change rebuilds the scene through the same
//! path as the `scenario` command and keeps the camera where it was. Most of a first change to a
//! tier is the browser compiling that tier's pipelines after the rebuild frame, so the change counts
//! as drawn only once the GPU has finished a frame drawn with the rebuilt scene ([`mark_drawn`]).
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

use bevy::{
    diagnostic::FrameCount,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        render_resource::PipelineCache,
        renderer::{RenderAdapterInfo, RenderDevice, RenderQueue},
    },
};

use crate::{
    SceneEntity,
    play::PlaySpawned,
    post::QualityTier,
    tilt_shift::{TiltPinned, TiltShiftSettings, kept_tilt},
    workload::GateConfig,
};

/// The rebuild the render world waits on: `(generation << 32) | frame`, the generation being
/// `TierChoice::changes` and the frame the main world's `FrameCount` once the scene had spawned.
static ARMED: AtomicU64 = AtomicU64::new(0);
/// The latest generation whose frame finished on the GPU.
static DRAWN: AtomicU32 = AtomicU32::new(0);
/// Render pipelines in the cache when the latest completion callback was registered.
static PIPELINES: AtomicUsize = AtomicUsize::new(0);

/// Frames whose real delta feeds the web warm-up probe. It decides on the last one, ten frames
/// before READY (`diag::READY_FRAME`), so a step down rebuilds the scene before harnesses act.
pub const PROBE_FRAMES: std::ops::RangeInclusive<u32> = 50..=109;
/// A probe frame slower than this is a load or compile hitch, not steady-state cost; it is dropped.
pub const PROBE_HITCH_MS: f32 = 250.0;
/// Fewer kept frames than this and the probe is inconclusive (the static tier stands).
pub const PROBE_MIN_SAMPLES: usize = 30;
/// A warm-up median above this (under 40 fps on a small start scene) steps the tier down once.
pub const PROBE_STEP_MS: f32 = 25.0;

impl QualityTier {
    pub fn name(self) -> &'static str {
        match self {
            QualityTier::Low => "low",
            QualityTier::Medium => "medium",
            QualityTier::High => "high",
            QualityTier::Ultra => "ultra",
        }
    }

    /// One tier lower; Low stays Low.
    pub fn lower(self) -> QualityTier {
        match self {
            QualityTier::Low | QualityTier::Medium => QualityTier::Low,
            QualityTier::High => QualityTier::Medium,
            QualityTier::Ultra => QualityTier::High,
        }
    }
}

/// `wgpu::DeviceType`, owned so the rules stay testable without a device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DeviceKind {
    Discrete,
    Integrated,
    Virtual,
    Cpu,
    #[default]
    Other,
}

/// What the rules read from the adapter and the device it granted.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct AdapterFacts {
    pub name: String,
    pub kind: DeviceKind,
    /// The browser's WebGPU (adapter details masked).
    pub web: bool,
    pub max_texture_dimension_2d: u32,
    pub max_sampled_textures_per_shader_stage: u32,
    pub max_storage_textures_per_shader_stage: u32,
    pub bc: bool,
    pub astc: bool,
    pub timestamp_query: bool,
}

impl AdapterFacts {
    pub fn from_device(info: &RenderAdapterInfo, device: &RenderDevice) -> AdapterFacts {
        use bevy::render::render_resource::WgpuFeatures as F;
        let l = device.limits();
        let f = device.features();
        let kind = match info.device_type {
            wgpu::DeviceType::DiscreteGpu => DeviceKind::Discrete,
            wgpu::DeviceType::IntegratedGpu => DeviceKind::Integrated,
            wgpu::DeviceType::VirtualGpu => DeviceKind::Virtual,
            wgpu::DeviceType::Cpu => DeviceKind::Cpu,
            wgpu::DeviceType::Other => DeviceKind::Other,
        };
        AdapterFacts {
            name: info.name.clone(),
            kind,
            web: info.backend == wgpu::Backend::BrowserWebGpu,
            max_texture_dimension_2d: l.max_texture_dimension_2d,
            max_sampled_textures_per_shader_stage: l.max_sampled_textures_per_shader_stage,
            max_storage_textures_per_shader_stage: l.max_storage_textures_per_shader_stage,
            bc: f.contains(F::TEXTURE_COMPRESSION_BC),
            astc: f.contains(F::TEXTURE_COMPRESSION_ASTC),
            timestamp_query: f.contains(F::TIMESTAMP_QUERY),
        }
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "kind": format!("{:?}", self.kind),
            "web": self.web,
            "max_texture_dimension_2d": self.max_texture_dimension_2d,
            "max_sampled_textures_per_shader_stage": self.max_sampled_textures_per_shader_stage,
            "max_storage_textures_per_shader_stage": self.max_storage_textures_per_shader_stage,
            "texture_compression_bc": self.bc,
            "texture_compression_astc": self.astc,
            "timestamp_query": self.timestamp_query,
        })
    }
}

fn is_software(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [
        "swiftshader",
        "llvmpipe",
        "lavapipe",
        "softpipe",
        "microsoft basic render",
    ]
    .iter()
    .any(|s| n.contains(s))
}

/// Apple silicon desktop GPUs ("Apple M1" .. "Apple M4 Max"): the PRD §10 reference class.
fn is_apple_m(name: &str) -> bool {
    name.strip_prefix("Apple M")
        .and_then(|r| r.chars().next())
        .is_some_and(|c| c.is_ascii_digit())
}

/// The static rules, first match wins. Never returns Ultra.
pub fn detect(f: &AdapterFacts) -> (QualityTier, String) {
    use QualityTier::*;
    if f.kind == DeviceKind::Cpu || is_software(&f.name) {
        return (Low, format!("software rasterizer ({})", f.name));
    }
    if f.max_texture_dimension_2d < 8192 {
        return (
            Low,
            format!(
                "max_texture_dimension_2d {} < 8192 (pre-2016 class GPU)",
                f.max_texture_dimension_2d
            ),
        );
    }
    // Bevy's SSAO skips itself below 5 storage textures per stage, and High's AO is SSAO.
    if f.max_storage_textures_per_shader_stage < 5 {
        return (
            Medium,
            format!(
                "max_storage_textures_per_shader_stage {} < 5: no SSAO",
                f.max_storage_textures_per_shader_stage
            ),
        );
    }
    if f.web {
        return match (f.bc, f.astc) {
            (true, _) => (
                High,
                "web: adapter masked; BC textures and limits meet High (warm-up probe may step down)"
                    .into(),
            ),
            (false, true) => (
                Medium,
                "web: ASTC without BC, a mobile-class GPU".into(),
            ),
            (false, false) => (
                Medium,
                "web: no BC or ASTC texture compression".into(),
            ),
        };
    }
    if is_apple_m(&f.name) {
        return (
            High,
            format!("{}: Apple silicon, the reference desktop class", f.name),
        );
    }
    match f.kind {
        DeviceKind::Discrete => (High, format!("{}: discrete GPU", f.name)),
        DeviceKind::Integrated => (Medium, format!("{}: integrated GPU", f.name)),
        DeviceKind::Virtual => (Medium, format!("{}: virtual GPU", f.name)),
        DeviceKind::Cpu | DeviceKind::Other => (Medium, format!("{}: unknown device type", f.name)),
    }
}

/// What the warm-up probe measured and decided.
#[derive(Clone, Debug, PartialEq)]
pub struct ProbeResult {
    pub median_ms: f32,
    /// Frames kept (at most `PROBE_FRAMES`, minus hitches).
    pub kept: usize,
    pub hitches: usize,
    pub tier: QualityTier,
    pub reason: String,
}

/// The probe rule: drop hitches, then step `ceiling` down once if the median is slow.
pub fn probe(ceiling: QualityTier, frames_ms: &[f32]) -> ProbeResult {
    let mut kept: Vec<f32> = frames_ms
        .iter()
        .copied()
        .filter(|&m| m > 0.0 && m < PROBE_HITCH_MS)
        .collect();
    kept.sort_by(f32::total_cmp);
    let hitches = frames_ms.len() - kept.len();
    let median_ms = if kept.is_empty() {
        0.0
    } else {
        kept[kept.len() / 2]
    };
    let (tier, reason) = if kept.len() < PROBE_MIN_SAMPLES {
        (
            ceiling,
            format!(
                "inconclusive: {} steady frames < {PROBE_MIN_SAMPLES}",
                kept.len()
            ),
        )
    } else if median_ms > PROBE_STEP_MS {
        (
            ceiling.lower(),
            format!("warm-up median {median_ms:.1} ms > {PROBE_STEP_MS} ms: one tier down"),
        )
    } else {
        (
            ceiling,
            format!("warm-up median {median_ms:.1} ms <= {PROBE_STEP_MS} ms"),
        )
    };
    ProbeResult {
        median_ms,
        kept: kept.len(),
        hitches,
        tier,
        reason,
    }
}

/// Where the current tier came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TierSource {
    /// High, until the device exists and detection runs.
    #[default]
    Default,
    /// `--tier` / `?tier=`: detection still runs and is reported, but never applied.
    Explicit,
    Detected,
    /// The web warm-up probe stepped the detected tier down.
    Probe,
    /// The `set_tier` command.
    Runtime,
}

impl TierSource {
    pub fn name(self) -> &'static str {
        match self {
            TierSource::Default => "default",
            TierSource::Explicit => "explicit",
            TierSource::Detected => "detected",
            TierSource::Probe => "probe",
            TierSource::Runtime => "runtime",
        }
    }
}

/// The tier in force, why, and what detection found (published as `state.tier`).
#[derive(Resource, Clone, Debug, Default)]
pub struct TierChoice {
    pub tier: QualityTier,
    pub source: TierSource,
    pub reason: String,
    pub adapter: Option<AdapterFacts>,
    /// The static rules' answer, even when an explicit tier overrides it.
    pub detected: Option<(QualityTier, String)>,
    pub probe: Option<ProbeResult>,
    /// Tier changes applied since startup (scene rebuilds).
    pub changes: u32,
    /// The latest change whose rebuilt scene the GPU has finished drawing; behind `changes` while a
    /// rebuild is in flight.
    pub drawn: u32,
    /// Quality requests handled (`set_tier` and a loaded save's quality), no-ops included.
    pub requests: u32,
}

impl TierChoice {
    pub fn new(tier: QualityTier, explicit: bool) -> TierChoice {
        TierChoice {
            tier,
            source: if explicit {
                TierSource::Explicit
            } else {
                TierSource::Default
            },
            reason: if explicit {
                "tier key".into()
            } else {
                "default until the render device exists".into()
            },
            ..default()
        }
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "tier": self.tier.name(),
            "source": self.source.name(),
            "reason": self.reason,
            "detected": self.detected.as_ref().map(|(t, r)| serde_json::json!({ "tier": t.name(), "reason": r })),
            "probe": self.probe.as_ref().map(|p| serde_json::json!({
                "median_ms": (f64::from(p.median_ms) * 100.0).round() / 100.0,
                "kept": p.kept,
                "hitches": p.hitches,
                "tier": p.tier.name(),
                "reason": p.reason,
            })),
            "adapter": self.adapter.as_ref().map(AdapterFacts::json),
            "changes": self.changes,
            "drawn": self.drawn,
            "requests": self.requests,
            "rebuilding": self.rebuilding(),
        })
    }

    /// A tier change has rebuilt the scene and the GPU has not yet finished a frame of it.
    pub fn rebuilding(&self) -> bool {
        self.drawn != self.changes
    }
}

/// Queues a tier change: updates `TierChoice` and, when the tier differs, rebuilds the scene at the
/// new tier (the `scenario` command's path) with the camera kept and the flock resized.
pub fn set_tier(commands: &mut Commands, tier: QualityTier, source: TierSource, reason: String) {
    commands.queue(move |world: &mut World| apply_tier(world, tier, source, reason));
}

/// What Auto means once detection has run, as the startup applied it: the probe's tier and both
/// reasons when the probe stepped down, else the static rules' tier and reason. `None` before the
/// render device exists.
pub fn auto_choice(choice: &TierChoice) -> Option<(QualityTier, TierSource, String)> {
    let (detected, why) = choice.detected.clone()?;
    Some(match &choice.probe {
        Some(p) if p.tier != detected => (
            p.tier,
            TierSource::Probe,
            format!("{why}; probe: {}", p.reason),
        ),
        _ => (detected, TierSource::Detected, why),
    })
}

/// Resolves a player's quality choice, `want` (`None` is Auto), against the current choice. A save's
/// quality never overrides a `tier` key (`from_save`); Auto before the device exists hands the tier
/// back to detection (`Default`). `None`: nothing to apply now.
pub fn resolve(
    choice: &TierChoice,
    want: Option<QualityTier>,
    reason: String,
    from_save: bool,
) -> Option<(QualityTier, TierSource, String)> {
    if from_save && choice.source == TierSource::Explicit {
        return None;
    }
    match want {
        Some(t) => Some((t, TierSource::Runtime, reason)),
        None => auto_choice(choice),
    }
}

/// Queues a player's quality choice (the `set_tier` command, or a loaded save's quality).
pub fn request(
    commands: &mut Commands,
    want: Option<QualityTier>,
    reason: String,
    from_save: bool,
) {
    commands.queue(move |world: &mut World| {
        world.resource_mut::<TierChoice>().requests += 1;
        let choice = world.resource::<TierChoice>();
        let undetected = choice.detected.is_none();
        match resolve(choice, want, reason, from_save) {
            Some((tier, source, reason)) => apply_tier(world, tier, source, reason),
            None if want.is_none() && undetected && !from_save => {
                let mut choice = world.resource_mut::<TierChoice>();
                choice.source = TierSource::Default;
                choice.reason = "default until the render device exists".into();
            }
            None => {}
        }
    });
}

fn apply_tier(world: &mut World, tier: QualityTier, source: TierSource, reason: String) {
    let current = world.resource::<GateConfig>().tier;
    {
        let mut choice = world.resource_mut::<TierChoice>();
        choice.tier = tier;
        choice.source = source;
        choice.reason = reason;
        if current != tier {
            choice.changes += 1;
        }
    }
    if current == tier {
        return;
    }
    info!("tier: {} -> {}", current.name(), tier.name());
    world.resource_mut::<GateConfig>().tier = tier;
    let camera = world
        .query_filtered::<&Transform, With<Camera3d>>()
        .iter(world)
        .next()
        .copied();
    let tilt_before = world.get_resource::<TiltShiftSettings>().copied();
    let pinned = world.get_resource::<TiltPinned>().is_some_and(|p| p.0);
    let scene: Vec<Entity> = world
        .query_filtered::<Entity, With<SceneEntity>>()
        .iter(world)
        .collect();
    for e in scene {
        if let Ok(e) = world.get_entity_mut(e) {
            e.despawn();
        }
    }
    crate::boids::set_budget(world, crate::boids::BoidBudget::for_tier(tier));
    if let Err(e) = world.run_system_cached(crate::scene::setup) {
        warn!("tier: scene rebuild failed: {e}");
        let mut choice = world.resource_mut::<TierChoice>();
        choice.drawn = choice.changes;
        return;
    }
    // The rebuild resets tilt-shift to the new tier's default; a player's own choice outlives it.
    if let (Some(before), Some(mut now)) =
        (tilt_before, world.get_resource_mut::<TiltShiftSettings>())
    {
        *now = kept_tilt(before, *now, pinned);
    }
    if let Some(t) = camera {
        for mut c in world
            .query_filtered::<&mut Transform, With<Camera3d>>()
            .iter_mut(world)
        {
            *c = t;
        }
    }
}

/// Runs detection once the render device exists, then (web, detected only) the warm-up probe.
fn detect_tier(
    mut commands: Commands,
    mut choice: ResMut<TierChoice>,
    info: Option<Res<RenderAdapterInfo>>,
    device: Option<Res<RenderDevice>>,
    time: Res<Time<Real>>,
    mut frame: Local<u32>,
    mut probe_ms: Local<Vec<f32>>,
) {
    *frame += 1;
    if choice.adapter.is_none() {
        let (Some(info), Some(device)) = (info, device) else {
            return;
        };
        let facts = AdapterFacts::from_device(&info, &device);
        let (tier, reason) = detect(&facts);
        info!(
            "tier: detected {} ({reason}){}",
            tier.name(),
            if choice.source == TierSource::Explicit {
                ", explicit tier wins"
            } else {
                ""
            }
        );
        choice.adapter = Some(facts);
        choice.detected = Some((tier, reason.clone()));
        if choice.source == TierSource::Default {
            set_tier(&mut commands, tier, TierSource::Detected, reason);
        }
        return;
    }
    let web = choice.adapter.as_ref().is_some_and(|a| a.web);
    if !web || choice.source != TierSource::Detected || choice.probe.is_some() {
        return;
    }
    if PROBE_FRAMES.contains(&*frame) {
        probe_ms.push(time.delta_secs() * 1000.0);
    }
    if *frame == *PROBE_FRAMES.end() {
        let p = probe(choice.tier, &probe_ms);
        info!("tier: probe {} -> {}", p.reason, p.tier.name());
        let stepped = p.tier != choice.tier;
        let reason = format!(
            "{}; probe: {}",
            choice.detected.as_ref().map_or("", |d| d.1.as_str()),
            p.reason
        );
        let tier = p.tier;
        choice.probe = Some(p);
        if stepped {
            set_tier(&mut commands, tier, TierSource::Probe, reason);
        }
    }
}

/// Settles a rebuild once the render world reports its frame done on the GPU, and arms the render
/// world once the rebuilt scene has spawned (play redraws its tiles in the sync after the rebuild).
fn track_rebuild(
    mut choice: ResMut<TierChoice>,
    frame: Res<FrameCount>,
    active: Res<crate::ActiveScenario>,
    spawned: Option<Res<PlaySpawned>>,
) {
    if !choice.rebuilding() {
        return;
    }
    let generation = choice.changes;
    if DRAWN.load(Ordering::Acquire) >= generation {
        choice.drawn = generation;
        return;
    }
    if active.0 == crate::Scenario::Play && spawned.is_some_and(|s| s.redraw_pending()) {
        return;
    }
    if (ARMED.load(Ordering::Acquire) >> 32) as u32 != generation {
        ARMED.store(
            (u64::from(generation) << 32) | u64::from(frame.0),
            Ordering::Release,
        );
    }
}

/// Render world, after the frame's submit: once the armed rebuild's scene has been drawn with no
/// pipeline left to create, asks the GPU to report when that frame's work is done. On WebGPU that
/// cannot happen before the browser has compiled the pipelines the frame uses. New pipelines in a
/// later frame void the pending report and a new one is asked for.
fn mark_drawn(
    queue: Option<Res<RenderQueue>>,
    cache: Res<PipelineCache>,
    frame: Res<FrameCount>,
    mut asked: Local<(u32, usize)>,
) {
    let armed = ARMED.load(Ordering::Acquire);
    let (generation, at) = ((armed >> 32) as u32, armed as u32);
    if generation == 0 || DRAWN.load(Ordering::Acquire) >= generation {
        return;
    }
    // The render world's `FrameCount` runs one ahead of the main world's `Update`, so `at + 1`
    // extracted the spawned scene; the frame after it also draws materials prepared a frame late.
    if frame.0 <= at.wrapping_add(1) || cache.waiting_pipelines().next().is_some() {
        return;
    }
    let pipelines = cache.pipelines().count();
    PIPELINES.store(pipelines, Ordering::Release);
    if *asked == (generation, pipelines) {
        return;
    }
    *asked = (generation, pipelines);
    let done = move || {
        if PIPELINES.load(Ordering::Acquire) == pipelines {
            DRAWN.fetch_max(generation, Ordering::AcqRel);
        }
    };
    match queue {
        Some(q) => q.on_submitted_work_done(done),
        None => done(),
    }
}

pub struct TierPlugin {
    pub tier: QualityTier,
    pub explicit: bool,
}

impl Plugin for TierPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(TierChoice::new(self.tier, self.explicit))
            .add_systems(Update, (detect_tier, track_rebuild));
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(Render, mark_drawn.in_set(RenderSystems::Cleanup));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use QualityTier::*;

    fn native(name: &str, kind: DeviceKind) -> AdapterFacts {
        AdapterFacts {
            name: name.into(),
            kind,
            web: false,
            max_texture_dimension_2d: 16384,
            max_sampled_textures_per_shader_stage: 128,
            max_storage_textures_per_shader_stage: 8,
            bc: true,
            astc: false,
            timestamp_query: true,
        }
    }

    fn web(bc: bool, astc: bool) -> AdapterFacts {
        AdapterFacts {
            name: String::new(),
            kind: DeviceKind::Other,
            web: true,
            max_texture_dimension_2d: 16384,
            max_sampled_textures_per_shader_stage: 16,
            max_storage_textures_per_shader_stage: 8,
            bc,
            astc,
            timestamp_query: true,
        }
    }

    #[test]
    fn reference_mac_is_high_although_integrated() {
        // wgpu reports Apple silicon as an integrated GPU; the name rule must win.
        for n in ["Apple M4 Max", "Apple M1", "Apple M2 Pro", "Apple M3 Ultra"] {
            assert_eq!(detect(&native(n, DeviceKind::Integrated)).0, High, "{n}");
        }
    }

    #[test]
    fn native_device_types() {
        assert_eq!(
            detect(&native("NVIDIA GeForce RTX 4070", DeviceKind::Discrete)).0,
            High
        );
        assert_eq!(
            detect(&native("Intel(R) UHD Graphics 630", DeviceKind::Integrated)).0,
            Medium
        );
        assert_eq!(
            detect(&native("Apple A17 Pro GPU", DeviceKind::Integrated)).0,
            Medium
        );
        assert_eq!(detect(&native("Virtio", DeviceKind::Virtual)).0, Medium);
        assert_eq!(detect(&native("Mystery", DeviceKind::Other)).0, Medium);
    }

    #[test]
    fn software_and_old_gpus_are_low() {
        assert_eq!(
            detect(&native("llvmpipe (LLVM 17)", DeviceKind::Cpu)).0,
            Low
        );
        assert_eq!(
            detect(&native("SwiftShader Device (Subzero)", DeviceKind::Other)).0,
            Low
        );
        let mut old = native("Old GPU", DeviceKind::Discrete);
        old.max_texture_dimension_2d = 4096;
        assert_eq!(detect(&old).0, Low);
    }

    #[test]
    fn few_storage_textures_cap_at_medium() {
        let mut f = native("NVIDIA GeForce RTX 4070", DeviceKind::Discrete);
        f.max_storage_textures_per_shader_stage = 4;
        let (t, r) = detect(&f);
        assert_eq!(t, Medium);
        assert!(r.contains("SSAO"), "{r}");
    }

    #[test]
    fn web_reads_features_not_the_masked_name() {
        assert_eq!(detect(&web(true, false)).0, High);
        assert_eq!(detect(&web(true, true)).0, High);
        assert_eq!(detect(&web(false, true)).0, Medium);
        assert_eq!(detect(&web(false, false)).0, Medium);
        // The WebGPU baseline of 16 sampled textures per stage is the design target, not a downgrade
        // (the web-smoke 16-texture cases clamp the device to it and expect High).
        let mut f = web(true, false);
        f.max_sampled_textures_per_shader_stage = 16;
        assert_eq!(detect(&f).0, High);
    }

    #[test]
    fn detection_never_picks_ultra() {
        for f in [
            native("Apple M4 Max", DeviceKind::Integrated),
            native("NVIDIA GeForce RTX 5090", DeviceKind::Discrete),
            web(true, true),
        ] {
            assert_ne!(detect(&f).0, Ultra);
        }
    }

    #[test]
    fn probe_keeps_fast_and_steps_slow_once() {
        let fast = vec![16.7; 60];
        assert_eq!(probe(High, &fast).tier, High);
        let slow = vec![40.0; 60];
        let p = probe(High, &slow);
        assert_eq!((p.tier, p.median_ms, p.kept), (Medium, 40.0, 60));
        assert_eq!(probe(Low, &slow).tier, Low);
        // A step is at most one tier, however slow.
        assert_eq!(probe(High, &[200.0; 60]).tier, Medium);
    }

    #[test]
    fn probe_drops_hitches_and_needs_enough_frames() {
        let mut f = vec![10.0; 40];
        f.extend([900.0; 20]);
        let p = probe(High, &f);
        assert_eq!((p.tier, p.kept, p.hitches), (High, 40, 20));
        let p = probe(High, &[600.0; 60]);
        assert_eq!(p.tier, High);
        assert!(p.reason.starts_with("inconclusive"), "{}", p.reason);
    }

    fn detected(tier: QualityTier, probe: Option<QualityTier>) -> TierChoice {
        TierChoice {
            tier,
            source: TierSource::Runtime,
            reason: "set_tier command".into(),
            detected: Some((High, "rules".into())),
            probe: probe.map(|t| ProbeResult {
                median_ms: 30.0,
                kept: 60,
                hitches: 0,
                tier: t,
                reason: "slow".into(),
            }),
            ..default()
        }
    }

    #[test]
    fn auto_returns_to_what_detection_applied() {
        assert_eq!(
            auto_choice(&detected(Low, None)),
            Some((High, TierSource::Detected, "rules".into()))
        );
        assert_eq!(
            auto_choice(&detected(Low, Some(High))),
            Some((High, TierSource::Detected, "rules".into())),
            "a probe that held keeps the rules' reason"
        );
        assert_eq!(
            auto_choice(&detected(Low, Some(Medium))),
            Some((Medium, TierSource::Probe, "rules; probe: slow".into()))
        );
        assert_eq!(auto_choice(&TierChoice::new(High, false)), None);
    }

    #[test]
    fn a_saved_quality_never_beats_a_tier_key() {
        let mut keyed = detected(Medium, None);
        keyed.source = TierSource::Explicit;
        assert_eq!(resolve(&keyed, Some(Low), "loaded save".into(), true), None);
        assert_eq!(resolve(&keyed, None, "loaded save".into(), true), None);
        assert_eq!(
            resolve(&keyed, Some(Low), "set_tier command".into(), false),
            Some((Low, TierSource::Runtime, "set_tier command".into())),
            "the player's own pick still applies"
        );
        assert_eq!(
            resolve(&detected(High, None), Some(Low), "loaded save".into(), true),
            Some((Low, TierSource::Runtime, "loaded save".into()))
        );
    }

    #[test]
    fn tier_names_and_lower() {
        assert_eq!(QualityTier::parse(High.name()), Some(High));
        assert_eq!(Ultra.lower(), High);
        assert_eq!(Low.lower(), Low);
        let c = TierChoice::new(Medium, true);
        let j = c.json();
        assert_eq!(j["tier"], "medium");
        assert_eq!(j["source"], "explicit");
        assert!(j["adapter"].is_null() && j["probe"].is_null());
        assert_eq!(
            (j["requests"].as_u64(), j["rebuilding"].as_bool()),
            (Some(0), Some(false))
        );
    }

    #[test]
    fn a_change_is_rebuilding_until_drawn() {
        let mut c = TierChoice::new(High, false);
        c.changes = 2;
        c.drawn = 1;
        assert!(c.rebuilding());
        assert_eq!(c.json()["rebuilding"], true);
        c.drawn = 2;
        assert!(!c.rebuilding());
    }
}
