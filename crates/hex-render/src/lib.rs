//! Hex World Bevy render plugins.
use bevy::prelude::*;

pub mod boids;
mod chunks;
pub mod diag;
pub mod fps;
pub mod instanced;
mod kit_test;
pub mod post;
mod scene;
pub mod workload;
pub use hex_assets::bevy::MeshoptGlbPlugin;
pub use scene::{SceneEntity, TILE_HEIGHT, Tile, Vehicle, setup as setup_scene};
pub use workload::{
    GATE, GateConfig, GateView, GateWorkload, Layout, PostMode, ShadowCasters, parse_cascades,
    parse_chunk, parse_shadow_dist, parse_shadows,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Scenario {
    #[default]
    Single,
    Gate2000,
}

impl Scenario {
    pub fn parse(s: &str) -> Option<Scenario> {
        match s {
            "single" => Some(Scenario::Single),
            "gate2000" => Some(Scenario::Gate2000),
            _ => None,
        }
    }

    pub fn tile_count(self) -> usize {
        match self {
            Scenario::Single => 1,
            Scenario::Gate2000 => 2000,
        }
    }
}

#[derive(Resource, Clone, Copy)]
pub struct ActiveScenario(pub Scenario);

/// Camera view used when the gate scenario is (re)built.
#[derive(Resource, Clone, Copy)]
pub struct ActiveView(pub GateView);

pub struct HexRenderPlugin {
    pub scenario: Scenario,
    pub view: GateView,
    pub config: GateConfig,
    /// Initial FPS counter state (`fps=on|off`).
    pub fps: bool,
}

impl Plugin for HexRenderPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ActiveScenario(self.scenario))
            .insert_resource(ActiveView(self.view))
            .insert_resource(self.config)
            .add_plugins((
                diag::DiagPlugin,
                boids::BoidsPlugin::default(),
                fps::FpsPlugin { visible: self.fps },
                instanced::InstancedPlugin,
            ))
            .add_systems(Startup, scene::setup)
            .add_systems(Update, (scene::move_vehicles, kit_test::spawn_when_ready));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_and_rejects_unknown() {
        assert_eq!(Scenario::parse("single"), Some(Scenario::Single));
        assert_eq!(Scenario::parse("gate2000"), Some(Scenario::Gate2000));
        assert_eq!(Scenario::parse("nope"), None);
        assert_eq!(Scenario::parse(""), None);
    }

    #[test]
    fn tile_counts() {
        assert_eq!(Scenario::Single.tile_count(), 1);
        assert_eq!(Scenario::Gate2000.tile_count(), 2000);
    }
}
