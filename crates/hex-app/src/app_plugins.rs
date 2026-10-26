use bevy::prelude::*;

use crate::{actions, control, persist, photo, photo_fx};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AppSet {
    Frame,
    Flags,
    Input,
    Hover,
    Commands,
    Apply,
    Photo,
    Focus,
    Walk,
    Export,
    Effects,
    Persist,
    Latest,
    Packs,
    View,
    Mirror,
    Cars,
    State,
    Diagnostics,
    Exit,
}

pub(crate) struct ActionsPlugin;

impl Plugin for ActionsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<actions::ActionQueue>()
            .add_systems(Update, actions::read_input.in_set(AppSet::Input))
            .add_systems(
                PreUpdate,
                actions::drop_live_input.after(bevy::input::InputSystems),
            );
    }
}

pub(crate) struct PhotoPlugin;

impl Plugin for PhotoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<photo::PhotoSettings>()
            .init_resource::<photo::PhotoSession>()
            .init_resource::<photo::PhotoExports>()
            .init_resource::<photo::PhotoFocus>()
            .add_systems(Update, photo::sync.in_set(AppSet::Photo))
            .add_systems(Update, photo::focus_pick.in_set(AppSet::Focus))
            .add_systems(Update, photo::run_exports.in_set(AppSet::Export));
    }
}

pub(crate) struct PhotoFxPlugin;

impl Plugin for PhotoFxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<photo_fx::PhotoFxSettings>()
            .add_systems(Update, photo_fx::sync.in_set(AppSet::Effects));
    }
}

pub(crate) struct PersistPlugin;

impl Plugin for PersistPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<persist::PersistQueue>()
            .init_resource::<persist::SavedSettings>()
            .add_systems(Update, persist::run.in_set(AppSet::Persist))
            .add_systems(Update, persist::keep_latest.in_set(AppSet::Latest))
            .add_systems(Last, persist::save_on_exit);
    }
}

pub(crate) struct ControlPlugin;

impl Plugin for ControlPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<control::ExitRequest>()
            .add_systems(Update, control::run_commands.in_set(AppSet::Commands))
            .add_systems(Update, control::publish_state.in_set(AppSet::State))
            .add_systems(Update, control::exit_when_idle.in_set(AppSet::Exit))
            .add_systems(Last, control::dump_state_on_exit);
    }
}
