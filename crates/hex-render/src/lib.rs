//! Hex World Bevy render plugins.
use bevy::prelude::*;

pub mod car_material;
pub mod chunks;
pub mod grade;
pub mod hooks;
#[cfg(feature = "gate-tools")]
pub mod instanced;
pub mod kit_instances;
pub mod kit_material;
pub mod kit_scene;
#[cfg(feature = "gate-tools")]
mod kit_test;
pub mod lighting;
pub use lighting as lookdev;
pub mod photo_fx;
pub mod post;
pub mod scene;
pub mod tilt_shift;
pub mod water;
pub mod workload;
pub use grade::Grade;
pub use hex_assets::bevy::{AssetLoadFailures, MeshoptGlbPlugin};
pub use scene::{SceneEntity, TILE_HEIGHT, Tile, Vehicle, setup as setup_scene};
pub use workload::{
    Ev100, GATE, GateConfig, GateView, GateWorkload, KitHooks, KitSource, Layout, PostMode,
    ShadowCasters, TiltMode, TiltStrength, Tonemap, parse_cascades, parse_chunk, parse_grass,
    parse_grass_distance, parse_ibl, parse_shadow_dist, parse_shadows,
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
    /// The water patch and the `kit_src` kit pieces in one view, under the water camera: the
    /// sampled-texture gate for kit pipelines in a view with screen-space transmission (P2).
    WaterKit,
    /// The playable world: placement, undo/redo and bulldoze, driven by `hex-app` (`play`).
    Play,
}

impl Scenario {
    pub fn parse(s: &str) -> Option<Scenario> {
        match s {
            "single" => Some(Scenario::Single),
            "gate2000" => Some(Scenario::Gate2000),
            "kit" => Some(Scenario::Kit),
            "water" => Some(Scenario::Water),
            "water_kit" => Some(Scenario::WaterKit),
            "play" => Some(Scenario::Play),
            _ => None,
        }
    }

    /// Hex cells the scenario lays out. The kit scenarios depend on the source (`kit_src`).
    pub fn tile_count(self, kit_src: KitSource) -> usize {
        match self {
            Scenario::Single => 1,
            Scenario::Gate2000 => 2000,
            Scenario::Kit => kit_scene::cell_count(kit_src),
            Scenario::Water => water::layout().len(),
            Scenario::WaterKit => {
                let mut cells: Vec<_> = water::layout().into_iter().map(|(c, _)| c).collect();
                cells.extend(kit_scene::source(kit_src).1.iter().map(|p| p.cell));
                cells.sort_by_key(|c| (c.q, c.r));
                cells.dedup();
                cells.len()
            }
            // The play world's size lives in hex-app's game state, which reports it.
            Scenario::Play => 0,
        }
    }

    /// Scenarios that spawn the water patch and use its camera.
    pub fn has_water(self) -> bool {
        matches!(self, Scenario::Water | Scenario::WaterKit)
    }
}

/// The render device's limits that decide whether a pipeline fits (read once at startup; on the
/// web this is what the adapter granted, e.g. 48 on Chrome/Metal, 44 on Safari macOS).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuLimits {
    pub max_sampled_textures_per_shader_stage: u32,
    /// Largest 2D texture side; bounds photo mode's offscreen export (PRD §4.6).
    pub max_texture_dimension_2d: u32,
}

/// Photo mode's agent pause (PRD §4.6): cars and boids hold still while it is set.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AgentsPaused(pub bool);

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
        max_texture_dimension_2d: device.limits().max_texture_dimension_2d,
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
}

impl Plugin for HexRenderPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "gate-tools")]
        app.add_plugins(instanced::InstancedPlugin)
            .add_systems(Update, (kit_test::spawn_when_ready, scene::move_vehicles));
        bevy::shader::load_shader_library!(app, "kit_common.wesl");
        bevy::shader::load_shader_library!(app, "kit_instances.wesl");
        bevy::shader::load_shader_library!(app, "water_common.wesl");
        bevy::shader::load_shader_library!(app, "hooks.wesl");
        bevy::shader::load_shader_library!(app, "wind.wesl");
        bevy::shader::load_shader_library!(app, "grass_common.wesl");
        bevy::shader::load_shader_library!(app, "foliage_wind.wesl");
        app.insert_resource(ActiveScenario(self.scenario))
            .insert_resource(ClearColor(if grade::graded(self.scenario) {
                self.config.grade.backdrop()
            } else {
                lighting::BACKDROP
            }))
            .insert_resource(ActiveView(self.view))
            .insert_resource(self.config)
            .add_plugins((
                MaterialPlugin::<kit_material::KitMaterial>::default(),
                kit_instances::KitInstancesPlugin,
                tilt_shift::TiltShiftPlugin,
                MaterialPlugin::<water::WaterMaterial>::default(),
                MaterialPlugin::<water::TerrainMaterial>::default(),
                hooks::HooksPlugin {
                    hooks: self.config.kit_hooks,
                },
            ))
            .add_plugins(photo_fx::PhotoFxPlugin)
            .init_resource::<AgentsPaused>()
            .add_systems(Startup, scene::setup)
            .add_systems(
                Update,
                (
                    kit_scene::start_kit,
                    kit_scene::finish_kit,
                    kit_scene::spin_rotors,
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
        assert_eq!(Scenario::parse("water_kit"), Some(Scenario::WaterKit));
        assert_eq!(Scenario::parse("play"), Some(Scenario::Play));
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
        // The water patch's 11 cells plus the showcase blocks' cells; each block in
        // `kit_scene::kit_layout` names its own cells. A new block adds its cells here.
        assert_eq!(cells.len(), 33);
        assert_eq!(Scenario::Kit.tile_count(KitSource::Runtime), cells.len());
        // Review M5: the fixture source lays out 3 cells, not the runtime family's 12.
        assert_eq!(Scenario::Kit.tile_count(KitSource::Fixture), 3);
        assert_eq!(Scenario::Water.tile_count(KitSource::Runtime), 12);
        // The fixture lays out water-patch cells only; the runtime kit adds the showcase blocks.
        assert_eq!(Scenario::WaterKit.tile_count(KitSource::Fixture), 12);
        assert_eq!(Scenario::WaterKit.tile_count(KitSource::Runtime), 33);
        assert!(Scenario::WaterKit.has_water() && Scenario::Water.has_water());
        assert!(!Scenario::Kit.has_water());
    }
}

/// Scene lifecycle hook installed by hex-play before Startup and retained across scenario switches.
#[derive(Resource, Clone, Copy)]
pub struct PlaySceneSetup(
    pub fn(&mut Commands, &mut Assets<Mesh>, &mut Assets<StandardMaterial>) -> Transform,
);
