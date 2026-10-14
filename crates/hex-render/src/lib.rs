//! Hex World Bevy render plugins.
use bevy::prelude::*;

pub mod boids;
mod chunks;
pub mod diag;
pub mod fps;
pub mod instanced;
mod kit_material;
mod kit_scene;
mod kit_test;
pub mod lookdev;
pub mod post;
mod scene;
pub mod tilt_shift;
pub mod water;
pub mod workload;
pub use hex_assets::bevy::MeshoptGlbPlugin;
pub use scene::{SceneEntity, TILE_HEIGHT, Tile, Vehicle, setup as setup_scene};
pub use workload::{
    Ev100, GATE, GateConfig, GateView, GateWorkload, KitHooks, KitSource, Layout, PostMode,
    ShadowCasters, TiltMode, TiltStrength, Tonemap, parse_cascades, parse_chunk, parse_ibl,
    parse_shadow_dist, parse_shadows,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Scenario {
    #[default]
    Single,
    Gate2000,
    /// The P1 kit family from `assets/kit/manifest.json` (`kit_scene`).
    Kit,
    /// The hero-water look-dev patch (`water`).
    Water,
}

impl Scenario {
    pub fn parse(s: &str) -> Option<Scenario> {
        match s {
            "single" => Some(Scenario::Single),
            "gate2000" => Some(Scenario::Gate2000),
            "kit" => Some(Scenario::Kit),
            "water" => Some(Scenario::Water),
            _ => None,
        }
    }

    /// Hex cells the scenario lays out. Only the kit scenario depends on the source (`kit_src`).
    pub fn tile_count(self, kit_src: KitSource) -> usize {
        match self {
            Scenario::Single => 1,
            Scenario::Gate2000 => 2000,
            Scenario::Kit => kit_scene::cell_count(kit_src),
            Scenario::Water => water::layout().len(),
        }
    }
}

/// The render device's limits that decide whether a pipeline fits (read once at startup; on the
/// web this is what the adapter granted, e.g. 48 on Chrome/Metal, 44 on Safari macOS).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuLimits {
    pub max_sampled_textures_per_shader_stage: u32,
}

fn record_gpu_limits(
    mut commands: Commands,
    limits: Option<Res<GpuLimits>>,
    device: Option<Res<bevy::render::renderer::RenderDevice>>,
) {
    let (None, Some(device)) = (limits, device) else {
        return;
    };
    let l = GpuLimits {
        max_sampled_textures_per_shader_stage: device
            .limits()
            .max_sampled_textures_per_shader_stage,
    };
    info!(
        "gpu: max_sampled_textures_per_shader_stage={}",
        l.max_sampled_textures_per_shader_stage
    );
    commands.insert_resource(l);
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
        bevy::shader::load_shader_library!(app, "kit_common.wesl");
        bevy::shader::load_shader_library!(app, "water_common.wesl");
        app.insert_resource(ActiveScenario(self.scenario))
            .insert_resource(ActiveView(self.view))
            .insert_resource(self.config)
            .add_plugins((
                diag::DiagPlugin,
                boids::BoidsPlugin::default(),
                fps::FpsPlugin { visible: self.fps },
                instanced::InstancedPlugin,
                MaterialPlugin::<kit_material::KitMaterial>::default(),
                tilt_shift::TiltShiftPlugin,
                MaterialPlugin::<water::WaterMaterial>::default(),
                MaterialPlugin::<water::TerrainMaterial>::default(),
            ))
            .add_systems(Startup, scene::setup)
            .add_systems(
                Update,
                (
                    scene::move_vehicles,
                    kit_test::spawn_when_ready,
                    kit_scene::start_kit,
                    kit_scene::finish_kit,
                    record_gpu_limits,
                    water::tick,
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_and_rejects_unknown() {
        assert_eq!(Scenario::parse("single"), Some(Scenario::Single));
        assert_eq!(Scenario::parse("gate2000"), Some(Scenario::Gate2000));
        assert_eq!(Scenario::parse("kit"), Some(Scenario::Kit));
        assert_eq!(Scenario::parse("water"), Some(Scenario::Water));
        assert_eq!(Scenario::parse("nope"), None);
        assert_eq!(Scenario::parse(""), None);
    }

    #[test]
    fn tile_counts() {
        assert_eq!(Scenario::Single.tile_count(KitSource::Runtime), 1);
        assert_eq!(Scenario::Gate2000.tile_count(KitSource::Fixture), 2000);
        let mut cells: Vec<_> = kit_scene::kit_layout()
            .iter()
            .map(|p| (p.cell.q, p.cell.r))
            .collect();
        cells.sort();
        cells.dedup();
        assert_eq!(cells.len(), 8);
        assert_eq!(Scenario::Kit.tile_count(KitSource::Runtime), cells.len());
        // Review M5: the fixture source lays out 3 cells, not the runtime family's 8.
        assert_eq!(Scenario::Kit.tile_count(KitSource::Fixture), 3);
        assert_eq!(Scenario::Water.tile_count(KitSource::Runtime), 12);
    }
}
