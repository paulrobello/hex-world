//! Photo-mode effects (owner, 2026-10-14): the panel's vignette, colour filter, grain and chromatic
//! aberration. The values live here for the session, so leaving and re-entering photo mode keeps
//! them (like the DOF mode and aperture); they reach the camera only while photo mode is on, through
//! `hex_render::photo_fx::PhotoFxLook`. The pass itself is `hex_render::photo_fx`.
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use hex_render::photo_fx::{FxFilter, PhotoFxLook};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::FrameCount;
use crate::photo::PhotoSession;

/// Grain pattern for every export, so the same shot and settings give the same PNG.
pub const EXPORT_SEED: u32 = 0x5eed;

/// The colour filter in the panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhotoFilter {
    #[default]
    Normal,
    /// Black and white.
    Mono,
    Sepia,
}

impl From<PhotoFilter> for FxFilter {
    fn from(f: PhotoFilter) -> Self {
        match f {
            PhotoFilter::Normal => FxFilter::Normal,
            PhotoFilter::Mono => FxFilter::Mono,
            PhotoFilter::Sepia => FxFilter::Sepia,
        }
    }
}

/// The Effects section of the photo panel. Every amount is 0..=1.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct PhotoFxSettings {
    pub filter: PhotoFilter,
    pub filter_strength: f32,
    pub vignette: f32,
    pub vignette_softness: f32,
    pub grain: f32,
    pub aberration: f32,
}

impl Default for PhotoFxSettings {
    fn default() -> Self {
        Self {
            filter: PhotoFilter::Normal,
            filter_strength: 1.0,
            vignette: 0.0,
            vignette_softness: 0.5,
            grain: 0.0,
            aberration: 0.0,
        }
    }
}

/// The effect fields of the `photo` command; omitted fields keep theirs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
pub struct PhotoFxPatch {
    pub filter: Option<PhotoFilter>,
    pub filter_strength: Option<f32>,
    pub vignette: Option<f32>,
    pub vignette_softness: Option<f32>,
    pub grain: Option<f32>,
    pub aberration: Option<f32>,
}

fn unit(v: Option<f32>, slot: &mut f32) {
    if let Some(v) = v.filter(|v| v.is_finite()) {
        *slot = v.clamp(0.0, 1.0);
    }
}

impl PhotoFxSettings {
    pub fn apply(&mut self, p: &PhotoFxPatch) {
        if let Some(f) = p.filter {
            self.filter = f;
        }
        unit(p.filter_strength, &mut self.filter_strength);
        unit(p.vignette, &mut self.vignette);
        unit(p.vignette_softness, &mut self.vignette_softness);
        unit(p.grain, &mut self.grain);
        unit(p.aberration, &mut self.aberration);
    }

    pub fn look(&self, seed: u32) -> PhotoFxLook {
        PhotoFxLook {
            filter: self.filter.into(),
            filter_strength: self.filter_strength,
            vignette: self.vignette,
            vignette_softness: self.vignette_softness,
            grain: self.grain,
            aberration: self.aberration,
            seed,
        }
    }
}

/// Hands the effects to the renderer while photo mode is on, and takes them away when it ends.
/// Runs after `photo::run_exports`: while an export renders, the camera targets its offscreen
/// image, and the grain holds the fixed export seed.
pub fn sync(
    mut commands: Commands,
    settings: Res<PhotoFxSettings>,
    session: Res<PhotoSession>,
    frames: Res<FrameCount>,
    have: Option<Res<PhotoFxLook>>,
    cams: Query<&RenderTarget, With<Camera3d>>,
) {
    if session.saved.is_none() {
        if have.is_some() {
            commands.remove_resource::<PhotoFxLook>();
        }
        return;
    }
    let exporting = cams
        .iter()
        .next()
        .is_some_and(|t| matches!(t, RenderTarget::Image(_)));
    let seed = if exporting { EXPORT_SEED } else { frames.0 };
    let want = settings.look(seed);
    if have.as_deref() != Some(&want) {
        commands.insert_resource(want);
    }
}

/// Adds the effect values to the `photo` state object; `active` is whether the camera carries the
/// pass this frame.
pub fn extend_state(photo: &mut Value, s: &PhotoFxSettings, active: bool) {
    let r = |v: f32| (f64::from(v) * 100.0).round() / 100.0;
    if let Value::Object(o) = photo {
        let fx = json!({
            "filter": s.filter,
            "filter_strength": r(s.filter_strength),
            "vignette": r(s.vignette),
            "vignette_softness": r(s.vignette_softness),
            "grain": r(s.grain),
            "aberration": r(s.aberration),
            "fx_active": active,
        });
        if let Value::Object(f) = fx {
            o.extend(f);
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use hex_render::photo_fx::{PhotoFx, update_cameras};

    use super::*;
    use crate::game::GameState;
    use crate::photo::{PhotoSettings, sync as photo_sync};

    #[test]
    fn patch_clamps_and_keeps_omitted_fields() {
        let mut s = PhotoFxSettings::default();
        s.apply(&PhotoFxPatch {
            filter: Some(PhotoFilter::Sepia),
            filter_strength: Some(1.5),
            vignette: Some(-1.0),
            vignette_softness: Some(f32::NAN),
            grain: Some(0.4),
            aberration: None,
        });
        assert_eq!(s.filter, PhotoFilter::Sepia);
        assert_eq!(
            (
                s.filter_strength,
                s.vignette,
                s.vignette_softness,
                s.grain,
                s.aberration
            ),
            (1.0, 0.0, 0.5, 0.4, 0.0)
        );
    }

    #[test]
    fn the_command_spelling_parses() {
        let p: PhotoFxPatch = serde_json::from_str(
            r#"{"filter":"mono","filter_strength":0.8,"vignette":0.5,"vignette_softness":0.3,"grain":0.2,"aberration":0.1}"#,
        )
        .unwrap();
        assert_eq!(p.filter, Some(PhotoFilter::Mono));
        assert_eq!(p.aberration, Some(0.1));
        assert!(serde_json::from_str::<PhotoFxPatch>(r#"{"filter":"bw"}"#).is_err());
    }

    #[test]
    fn state_reports_every_field() {
        let mut v = json!({ "on": true });
        extend_state(&mut v, &PhotoFxSettings::default(), false);
        assert_eq!(
            v,
            json!({ "on": true, "filter": "normal", "filter_strength": 1.0, "vignette": 0.0,
                    "vignette_softness": 0.5, "grain": 0.0, "aberration": 0.0, "fx_active": false })
        );
    }

    fn app() -> App {
        let mut app = App::new();
        app.insert_resource(GameState::new(1))
            .init_resource::<PhotoSession>()
            .init_resource::<PhotoSettings>()
            .init_resource::<crate::photo::PhotoExports>()
            .init_resource::<PhotoFxSettings>()
            .init_resource::<FrameCount>()
            .init_resource::<hex_render::AgentsPaused>();
        let t = Transform::from_xyz(0.0, 60.0, 80.0).looking_at(Vec3::ZERO, Vec3::Y);
        app.world_mut().spawn((
            Camera3d::default(),
            t,
            Projection::Perspective(PerspectiveProjection::default()),
        ));
        app
    }

    fn frame(app: &mut App) -> Option<PhotoFx> {
        app.world_mut().run_system_once(photo_sync).unwrap();
        app.world_mut().run_system_once(sync).unwrap();
        app.world_mut().run_system_once(update_cameras).unwrap();
        let mut q = app.world_mut().query_filtered::<&PhotoFx, With<Camera3d>>();
        q.iter(app.world()).next().copied()
    }

    fn toggle(app: &mut App) {
        app.world_mut()
            .resource_mut::<GameState>()
            .apply(crate::actions::Action::TogglePhoto);
    }

    #[test]
    fn effects_apply_only_in_photo_mode_and_persist_across_sessions() {
        let mut app = app();
        app.world_mut()
            .resource_mut::<PhotoFxSettings>()
            .apply(&PhotoFxPatch {
                filter: Some(PhotoFilter::Mono),
                vignette: Some(0.5),
                ..default()
            });
        assert!(frame(&mut app).is_none(), "never on the game camera");
        toggle(&mut app);
        let on = frame(&mut app).expect("photo mode shows the effects");
        assert_eq!(on.filter_mode, 1);
        toggle(&mut app);
        assert!(
            frame(&mut app).is_none(),
            "leaving photo mode takes them off"
        );
        assert!(app.world().get_resource::<PhotoFxLook>().is_none());
        toggle(&mut app);
        assert_eq!(
            frame(&mut app).map(|u| u.filter_mode),
            Some(1),
            "re-entering keeps the settings"
        );
        app.world_mut()
            .resource_mut::<PhotoFxSettings>()
            .apply(&PhotoFxPatch {
                filter: Some(PhotoFilter::Normal),
                vignette: Some(0.0),
                ..default()
            });
        assert!(frame(&mut app).is_none(), "all off in photo mode: no pass");
    }

    #[test]
    fn grain_animates_in_the_viewfinder_and_holds_still_for_an_export() {
        let mut app = app();
        app.world_mut()
            .resource_mut::<PhotoFxSettings>()
            .apply(&PhotoFxPatch {
                grain: Some(0.5),
                ..default()
            });
        toggle(&mut app);
        app.world_mut().resource_mut::<FrameCount>().0 = 41;
        assert_eq!(frame(&mut app).unwrap().seed, 41);
        app.world_mut().resource_mut::<FrameCount>().0 = 42;
        assert_eq!(frame(&mut app).unwrap().seed, 42);
        // An export points the camera at its offscreen image (`photo::run_exports`).
        let mut q = app.world_mut().query_filtered::<Entity, With<Camera3d>>();
        let cam = q.single(app.world()).unwrap();
        app.world_mut()
            .entity_mut(cam)
            .insert(RenderTarget::Image(Handle::<Image>::default().into()));
        for f in [50, 51] {
            app.world_mut().resource_mut::<FrameCount>().0 = f;
            assert_eq!(frame(&mut app).unwrap().seed, EXPORT_SEED);
        }
    }
}
