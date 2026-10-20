//! Play presentation and its runtime consumers. Game rules remain in hex-core and hex-app.
use bevy::prelude::*;
#[cfg(feature = "gate-tools")]
pub use hex_render::instanced;
pub use hex_render::{ActiveScenario, ActiveView, AgentsPaused, GpuLimits, Scenario, SceneEntity};
pub use hex_render::{car_material, kit_material, kit_scene, post, scene, tilt_shift, workload};
pub mod boids;
pub mod cars;
#[cfg(feature = "gate-tools")]
pub mod debug;
#[cfg(feature = "gate-tools")]
pub mod diag;
#[cfg(feature = "gate-tools")]
pub mod fps;
#[cfg(feature = "gate-tools")]
pub mod gpu_timing;
pub mod play;
pub mod tier;
pub mod trains;
pub mod walk;
pub mod water;
/// Installs play presentation after `HexRenderPlugin`, before the first schedule runs.
pub struct HexPlayPlugin {
    pub fps: bool,
    pub tier_explicit: bool,
    pub gpu_timing: bool,
}
impl Plugin for HexPlayPlugin {
    fn build(&self, app: &mut App) {
        let config = *app.world().resource::<hex_render::GateConfig>();
        app.insert_resource(hex_render::PlaySceneSetup(play::setup))
            .add_plugins((
                play::PlayPlugin,
                cars::CarsPlugin,
                trains::TrainsPlugin,
                boids::BoidsPlugin {
                    budget: boids::BoidBudget::for_tier(config.tier),
                },
                tier::TierPlugin {
                    tier: config.tier,
                    explicit: self.tier_explicit,
                },
            ));
        #[cfg(feature = "gate-tools")]
        app.add_plugins((
            diag::DiagPlugin,
            fps::FpsPlugin { visible: self.fps },
            gpu_timing::GpuTimingPlugin {
                enabled: self.gpu_timing,
            },
            debug::DebugPlugin,
        ));
    }
}
