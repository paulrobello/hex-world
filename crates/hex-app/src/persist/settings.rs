//! Saved settings and the camera pose as the save format stores them. A save is untrusted input, so
//! the audio volumes (non-finite becomes 0, then clamped to 0..=1) and the orbit camera's pitch and
//! distance (clamped to the rig's limits) are bounded here. The tile data is validated by
//! `SaveData::decode` in `hex-core`.
use crate::camera::{self, MAX_DISTANCE, MAX_PITCH, MIN_DISTANCE, MIN_PITCH};
use bevy::prelude::*;
use hex_core::{AudioVolumes, CameraState, QualitySetting};
use hex_render::post::QualityTier;
use serde::{Deserialize, Serialize};

/// Audio bus gains as the bridge spells them (the save's `sfx` and `ui` are the web audio layer's
/// `effects` and `interface` buses).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Volumes {
    pub master: f32,
    pub music: f32,
    pub sfx: f32,
    pub ambience: f32,
    pub ui: f32,
}

impl From<AudioVolumes> for Volumes {
    fn from(v: AudioVolumes) -> Self {
        Self {
            master: v.master,
            music: v.music,
            sfx: v.sfx,
            ambience: v.ambience,
            ui: v.ui,
        }
    }
}

impl From<Volumes> for AudioVolumes {
    fn from(v: Volumes) -> Self {
        let g = |x: f32| {
            if x.is_finite() {
                x.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        Self {
            master: g(v.master),
            music: g(v.music),
            sfx: g(v.sfx),
            ambience: g(v.ambience),
            ui: g(v.ui),
        }
    }
}

/// The settings a `loaded` event hands the overlay to apply.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SettingsJson {
    pub quality: &'static str,
    pub tilt_shift: bool,
    pub tilt_shift_strength: f32,
    pub volumes: Volumes,
}

/// The tier a quality setting asks for; `None` is Auto (detection's answer).
pub fn quality_tier(q: QualitySetting) -> Option<QualityTier> {
    match q {
        QualitySetting::Auto => None,
        QualitySetting::Low => Some(QualityTier::Low),
        QualitySetting::Medium => Some(QualityTier::Medium),
        QualitySetting::High => Some(QualityTier::High),
        QualitySetting::Ultra => Some(QualityTier::Ultra),
    }
}

pub(super) fn quality_name(q: QualitySetting) -> &'static str {
    match q {
        QualitySetting::Auto => "auto",
        QualitySetting::Low => "low",
        QualitySetting::Medium => "medium",
        QualitySetting::High => "high",
        QualitySetting::Ultra => "ultra",
    }
}

/// Saved settings with no live owner in the engine: the quality choice (stored, not applied until
/// a quality setter exists) and the audio volumes the overlay sends with each `save`.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct SavedSettings {
    pub quality: QualitySetting,
    pub volumes: AudioVolumes,
}

impl Default for SavedSettings {
    /// The web audio layer's defaults (`web/ui/src/audio.ts`).
    fn default() -> Self {
        Self {
            quality: QualitySetting::Auto,
            volumes: AudioVolumes {
                master: 0.8,
                music: 0.55,
                sfx: 0.75,
                ambience: 0.1,
                ui: 0.5,
            },
        }
    }
}

/// The orbit pose of a camera transform.
pub fn camera_state(t: &Transform) -> CameraState {
    let focus = camera::focus(t);
    let (yaw, pitch, distance) = camera::spherical(t.translation - focus);
    CameraState {
        focus: focus.to_array(),
        yaw,
        pitch,
        distance,
    }
}

/// The camera transform for a saved orbit pose, inside the rig's pitch and zoom limits.
pub fn camera_transform(c: &CameraState) -> Transform {
    camera::place(
        Vec3::from_array(c.focus),
        c.yaw,
        c.pitch.clamp(MIN_PITCH, MAX_PITCH),
        c.distance.clamp(MIN_DISTANCE, MAX_DISTANCE),
    )
}

/// The default orbit when there is no camera to read (a headless run).
pub(super) fn default_camera() -> CameraState {
    CameraState {
        focus: [0.0, 0.0, 0.0],
        yaw: 0.0,
        pitch: 0.8,
        distance: 120.0,
    }
}
