use bevy::prelude::*;

use super::{
    PlayCatalog, PlaySystems, PlayView, affordances, ghost, grass, grass_gpu, rise, scatter,
};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PlaySet {
    Kit,
    Tiles,
    Scatter,
    Grass,
    Rise,
    Ghost,
    Affordances,
}

pub struct PlayPlugin;

impl Plugin for PlayPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                PlaySet::Kit,
                PlaySet::Tiles,
                PlaySet::Scatter,
                PlaySet::Grass,
                PlaySet::Rise,
                PlaySet::Ghost,
                PlaySet::Affordances,
            )
                .chain()
                .in_set(PlaySystems),
        )
        .add_plugins((
            TilesPlugin,
            ScatterPlugin,
            GrassPlugin,
            RisePlugin,
            GhostPlugin,
            AffordancesPlugin,
        ));
    }
}

struct TilesPlugin;
impl Plugin for TilesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<super::chunks::PlayChunks>()
            .add_systems(
                Update,
                super::chunks::restore
                    .after(PlaySet::Kit)
                    .before(PlaySet::Tiles),
            )
            .add_systems(
                Update,
                super::chunks::settle
                    .after(PlaySet::Rise)
                    .in_set(PlaySystems),
            )
            .init_resource::<PlayView>()
            .init_resource::<PlayCatalog>()
            .add_systems(
                Update,
                (super::start_kit, super::finish_kit)
                    .chain()
                    .in_set(PlaySet::Kit),
            )
            .add_systems(Update, super::sync.in_set(PlaySet::Tiles));
    }
}

struct ScatterPlugin;
impl Plugin for ScatterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<scatter::PlayScatter>()
            .init_resource::<scatter::ScatterLayouts>()
            .init_resource::<scatter::ScatterReport>()
            .add_systems(Update, scatter::sync.in_set(PlaySet::Scatter));
    }
}

struct GrassPlugin;
impl Plugin for GrassPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<grass::PlayGrass>()
            .init_resource::<grass::GrassReport>()
            .add_plugins(grass_gpu::GrassGpuPlugin)
            .add_systems(Update, grass::sync.in_set(PlaySet::Grass));
    }
}

struct RisePlugin;
impl Plugin for RisePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<rise::PlayRises>()
            .init_resource::<rise::ReducedMotion>()
            .init_resource::<rise::CameraShake>()
            .init_resource::<rise::PlayDust>()
            .add_message::<rise::RiseMessage>()
            .add_systems(First, rise::unshake)
            .add_systems(
                PostUpdate,
                rise::shake.before(bevy::transform::TransformSystems::Propagate),
            )
            .add_systems(
                Update,
                (rise::advance, rise::animate, rise::sink, rise::dust)
                    .chain()
                    .in_set(PlaySet::Rise),
            );
    }
}

struct GhostPlugin;
impl Plugin for GhostPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ghost::GhostMeshes>()
            .init_resource::<ghost::GhostDrawn>()
            .init_resource::<ghost::GhostReport>()
            .add_plugins(MaterialPlugin::<ghost::GhostMaterial>::default())
            .add_systems(Update, ghost::sync.in_set(PlaySet::Ghost));
    }
}

struct AffordancesPlugin;
impl Plugin for AffordancesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<affordances::Affordances>()
            .add_plugins(MaterialPlugin::<affordances::AffordanceMaterial>::default())
            .add_systems(
                Update,
                (
                    affordances::sync,
                    affordances::occlude,
                    affordances::animate,
                )
                    .chain()
                    .in_set(PlaySet::Affordances),
            );
    }
}
