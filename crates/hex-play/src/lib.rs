//! Play presentation and its runtime consumers. Game rules remain in hex-core and hex-app.
use bevy::prelude::*;
pub mod boats;
pub mod boids;
pub mod cars;
#[cfg(feature = "gate-tools")]
pub mod debug;
#[cfg(feature = "gate-tools")]
pub mod diag;
pub mod farm;
#[cfg(feature = "gate-tools")]
pub mod fps;
#[cfg(feature = "gate-tools")]
pub mod gpu_timing;
pub mod headlights;
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
        let config = *app.world().resource::<hex_render::RenderConfig>();
        app.insert_resource(hex_render::PlaySceneSetup(play::setup))
            .add_plugins((
                play::PlayPlugin,
                cars::CarsPlugin,
                trains::TrainsPlugin,
                headlights::HeadlightsPlugin,
                farm::FarmPlugin,
                boats::BoatsPlugin,
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
