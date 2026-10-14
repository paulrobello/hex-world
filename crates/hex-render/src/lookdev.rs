//! Look-dev camera components: tone mapping (D4), exposure, the noon IBL (D5) and the sun aim.
use bevy::{
    camera::Exposure, core_pipeline::tonemapping::Tonemapping, light::EnvironmentMapLight,
    prelude::*,
};

use crate::workload::{Ev100, Tonemap};

/// cd/m² scale for the prefiltered HDRI; set at the owner's look-dev checkpoint.
pub const IBL_INTENSITY: f32 = 1500.0;

/// Exposure (EV100) of the TonyMcMapface grades (`tony`, `rich-1`, `rich-2`). Bevy's default (9.7)
/// is an indoor value and clips the 30,000 lx sun. 12.9 under TonyMcMapface keeps the grass and
/// gravel within 3% of the ACES look at 12.5 (owner, 2026-10-14, Q4;
/// `docs/checkpoints/shade-diagnosis/`).
pub const KIT_EV100: f32 = 12.9;

/// The clear colour behind the world under the `tony` and `rich-1` grades and in the ungraded P0
/// scenarios, in linear RGB. HDR cameras tone-map it: Bevy's default clear colour read about 15/255
/// under ACES and 47/255 under TonyMcMapface, so this value keeps the ACES-era backdrop under
/// TonyMcMapface (owner, 2026-10-14, Q4).
pub const BACKDROP: Color = Color::linear_rgb(0.0034, 0.0035, 0.0044);

/// Unit direction toward the HDRI's sun in the runtime frame (`make lookdev-ibl`). The kit
/// scenario's DirectionalLight shines along its negation so shadows match the visible sky.
pub const KIT_SUN_DIR: Vec3 = Vec3::new(0.553, 0.742, 0.379);

/// End of the kit scenario's first shadow cascade (m). At the shared 120 m (`FIRST_CASCADE_FAR_BOUND`)
/// a shadow texel is ~0.1 m, so the 0.12 m road kerbs self-shadow as sawtooth acne on sun-facing
/// walls and cast no contact shadow. 25 m resolves both at Bevy's default depth/normal bias; raising
/// the depth bias instead (0.2 at 120 m) hid the acne but detached the kerb shadow.
pub const KIT_FIRST_CASCADE_FAR_BOUND: f32 = 25.0;

/// First cascade far bound per scenario. P0 scenarios keep the shared bound (perf baseline).
pub fn first_cascade_far_bound(kit: bool) -> f32 {
    if kit {
        KIT_FIRST_CASCADE_FAR_BOUND
    } else {
        crate::workload::FIRST_CASCADE_FAR_BOUND
    }
}

pub fn exposure(ev100: Ev100) -> Exposure {
    Exposure { ev100: ev100.get() }
}

/// The kit sun: placed along `KIT_SUN_DIR`, aimed at the origin.
pub fn sun_transform() -> Transform {
    Transform::from_translation(KIT_SUN_DIR.normalize() * 500.0).looking_at(Vec3::ZERO, Vec3::Y)
}

pub fn tonemapping(t: Tonemap) -> Tonemapping {
    match t {
        Tonemap::AgX => Tonemapping::AgX,
        Tonemap::TonyMcMapface => Tonemapping::TonyMcMapface,
        Tonemap::AcesFitted => Tonemapping::AcesFitted,
    }
}

pub fn environment(server: &AssetServer) -> EnvironmentMapLight {
    EnvironmentMapLight {
        diffuse_map: server.load("kit://p1/ibl/noon.diffuse.ktx2"),
        specular_map: server.load("kit://p1/ibl/noon.specular.ktx2"),
        intensity: IBL_INTENSITY,
        ..default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workload::GateConfig;

    #[test]
    fn maps_every_tonemap() {
        assert_eq!(tonemapping(Tonemap::AgX), Tonemapping::AgX);
        assert_eq!(
            tonemapping(Tonemap::TonyMcMapface),
            Tonemapping::TonyMcMapface
        );
        assert_eq!(tonemapping(Tonemap::AcesFitted), Tonemapping::AcesFitted);
        assert_eq!(
            GateConfig::default().tonemap,
            crate::grade::Grade::default().tonemap()
        );
    }

    #[test]
    fn exposure_follows_config_and_sun_points_down_the_hdri_direction() {
        assert_eq!(exposure(Ev100::from_f32(13.3)).ev100, 13.3);
        assert_eq!(
            exposure(GateConfig::default().ev100).ev100,
            crate::grade::Grade::default().ev100()
        );
        let t = sun_transform();
        let shine = t.forward().as_vec3();
        assert!(shine.dot(-KIT_SUN_DIR.normalize()) > 0.9999, "{shine:?}");
    }

    #[test]
    fn only_the_kit_scenario_tightens_the_first_cascade() {
        assert_eq!(
            first_cascade_far_bound(false),
            crate::workload::FIRST_CASCADE_FAR_BOUND
        );
        assert_eq!(first_cascade_far_bound(true), KIT_FIRST_CASCADE_FAR_BOUND);
    }
}
