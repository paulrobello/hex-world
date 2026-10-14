//! Quality-tier choice (PRD §9.2): an explicit `tier=` key wins; otherwise the tier is detected once
//! the render device exists. Natively the rules read the GPU name, device type and limits. On the
//! web the browser masks the adapter (`BrowserWebGpu | Other`, no name), so the rules read limits
//! and features, then a warm-up frame-time probe may step the tier down once before READY.
//! Detection never picks Ultra, the opt-in tier. A tier change rebuilds the scene through the same
//! path as the `scenario` command and keeps the camera where it was.
use bevy::{
    prelude::*,
    render::renderer::{RenderAdapterInfo, RenderDevice},
};

use crate::{SceneEntity, post::QualityTier, workload::GateConfig};

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
        })
    }
}

/// Queues a tier change: updates `TierChoice` and, when the tier differs, rebuilds the scene at the
/// new tier (the `scenario` command's path) with the camera kept and the flock resized.
pub fn set_tier(commands: &mut Commands, tier: QualityTier, source: TierSource, reason: String) {
    commands.queue(move |world: &mut World| apply_tier(world, tier, source, reason));
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
        return;
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

pub struct TierPlugin {
    pub tier: QualityTier,
    pub explicit: bool,
}

impl Plugin for TierPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(TierChoice::new(self.tier, self.explicit))
            .add_systems(Update, detect_tier);
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
    }
}
