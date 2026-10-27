//! Camera post stack per quality tier: TAA, SSAO, bloom and depth of field.
use bevy::{
    anti_alias::taa::TemporalAntiAliasing,
    core_pipeline::prepass::NormalPrepass,
    pbr::ScreenSpaceAmbientOcclusion,
    post_process::{bloom::Bloom, dof::DepthOfField},
    prelude::*,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum QualityTier {
    Low,
    Medium,
    #[default]
    High,
    Ultra,
}

impl QualityTier {
    /// `tier=low|medium|high|ultra` (PRD §9.2).
    pub fn parse(s: &str) -> Option<QualityTier> {
        match s {
            "low" => Some(QualityTier::Low),
            "medium" => Some(QualityTier::Medium),
            "high" => Some(QualityTier::High),
            "ultra" => Some(QualityTier::Ultra),
            _ => None,
        }
    }

    /// PRD §9.2's agent caps: the share of the High reference caps (§7.2) this tier allows, in
    /// percent. The boid budget and the car cap both scale by it.
    pub fn agent_cap_percent(self) -> u32 {
        match self {
            QualityTier::Low => 25,
            QualityTier::Medium => 50,
            QualityTier::High | QualityTier::Ultra => 100,
        }
    }
}

/// Components for the camera at a tier. TAA requires MSAA off. Required components pull in
/// the rest: `Bloom` adds `Hdr`, TAA adds the depth and motion-vector prepasses and jitter,
/// SSAO adds the depth and normal prepasses. `Option<Component>` is not a `Bundle` in 0.20,
/// so SSAO comes from `camera_ssao` and is inserted separately.
pub fn camera_post_bundle(_tier: QualityTier) -> impl Bundle {
    (
        Msaa::Off,
        TemporalAntiAliasing::default(),
        Bloom::NATURAL,
        DepthOfField {
            focal_distance: 900.0,
            aperture_f_stops: 8.0,
            ..default()
        },
    )
}

/// SSAO runs at High and Ultra only.
pub fn camera_ssao(tier: QualityTier) -> Option<ScreenSpaceAmbientOcclusion> {
    matches!(tier, QualityTier::High | QualityTier::Ultra)
        .then(ScreenSpaceAmbientOcclusion::default)
}

/// A hidden camera that draws another tier's view for a few frames so its pipelines exist before a
/// quality change (`hex_play::tier::warm`), and the water copy only it sees. Never the game camera:
/// systems that act on the game camera skip it.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct TierWarmCamera;

/// Query filter for the game camera: a 3D camera that is not a [`TierWarmCamera`].
pub type GameCamera = (With<Camera3d>, Without<TierWarmCamera>);

/// Gives a full-post camera its tier's SSAO, or takes it off with the normal prepass SSAO requires
/// (nothing else here uses one), so a camera changed in place keys its pipelines like one spawned
/// at the tier.
pub fn set_camera_ssao(cam: &mut EntityCommands, tier: QualityTier) {
    match camera_ssao(tier) {
        Some(ssao) => {
            cam.insert_if_new(ssao);
        }
        None => {
            cam.remove::<(ScreenSpaceAmbientOcclusion, NormalPrepass)>();
        }
    }
}

pub(crate) fn specialize_target(
    descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
    format: bevy::render::render_resource::TextureFormat,
) -> Result<(), BevyError> {
    use bevy::render::render_resource::{ColorTargetState, ColorWrites};
    descriptor.fragment_mut()?.set_target(
        0,
        ColorTargetState {
            format,
            blend: None,
            write_mask: ColorWrites::ALL,
        },
    );
    Ok(())
}

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
