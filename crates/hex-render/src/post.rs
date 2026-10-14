//! Camera post stack per quality tier: TAA, SSAO, bloom and depth of field.
use bevy::{
    anti_alias::taa::TemporalAntiAliasing,
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
