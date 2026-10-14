//! Photo mode (PRD §4.6): a free camera detached from the orbit rig, depth of field with a
//! tilt-shift preset, exposure, FOV and roll, an agent pause, and PNG export at 1×, 2× or 4×
//! through an offscreen render target.
//!
//! Enter and exit follow `GameState::photo_mode`, so the P key, Esc, the `toggle` command and the
//! overlay share one snapshot and one restore. Photo mode changes the projection, the exposure, the
//! tilt-shift override and, for Manual DOF only, Bevy's `DepthOfField` post pass. It adds no
//! prepass, so no kit or water pipeline gains a binding: kit forward sits at exactly WebGPU's 16
//! sampled textures beside refracting water (ADR 0002).
use std::collections::VecDeque;

use bevy::asset::RenderAssetUsages;
use bevy::camera::{Exposure, RenderTarget};
use bevy::post_process::dof::DepthOfField;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::window::PrimaryWindow;
use hex_render::{AgentsPaused, GpuLimits, tilt_shift::TiltOverride};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::camera::{focus, free_rotation, yaw_pitch};
use crate::capture;
use crate::game::{GameEvent, GameState, publish_event};

/// The depth-of-field choice in the photo panel (mockup S8.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dof {
    Off,
    /// The game camera's tilt-shift kernel, focused by the panel (one pass, PRD §9.1).
    #[default]
    Tilt,
    /// Bevy's lens depth of field, focused by the panel.
    Manual,
}

pub const FOCUS_M: (f32, f32) = (1.0, 1000.0);
pub const APERTURE_F: (f32, f32) = (1.4, 22.0);
pub const EXPOSURE_EV: (f32, f32) = (-3.0, 3.0);
pub const FOV_DEG: (f32, f32) = (15.0, 100.0);
pub const ROLL_DEG: (f32, f32) = (-45.0, 45.0);
pub const EXPORT_SCALES: [u32; 3] = [1, 2, 4];
/// The largest export scale offered: 4× natively, 2× on the web until a 4× export there is tested
/// for GPU memory (owner, 2026-10-14).
#[cfg(not(target_arch = "wasm32"))]
pub const MAX_EXPORT_SCALE: u32 = 4;
#[cfg(target_arch = "wasm32")]
pub const MAX_EXPORT_SCALE: u32 = 2;
/// The diorama reads as a miniature, so the manual lens uses a sensor this many times Bevy's
/// 18.66 mm default (blur in pixels grows linearly with it). At true scale f/2.8 focused at 20 m
/// blurs a 60 m background by about 0.3 px at 800 px tall; here by about 17 px.
pub const MINIATURE_SENSOR: f32 = 60.0;
/// Frames the offscreen target renders before its capture, so TAA settles at the new size.
pub const EXPORT_WARM_FRAMES: u32 = 8;

/// Panel values; they persist between photo sessions except where `enter` resets them.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct PhotoSettings {
    pub dof: Dof,
    pub focus_m: f32,
    /// f-number.
    pub aperture: f32,
    /// Stops added to the camera's exposure (positive is brighter).
    pub exposure_ev: f32,
    pub fov_deg: f32,
    pub roll_deg: f32,
    pub pause_agents: bool,
}

impl Default for PhotoSettings {
    fn default() -> Self {
        Self {
            dof: Dof::Tilt,
            focus_m: 64.0,
            aperture: 2.8,
            exposure_ev: 0.0,
            fov_deg: 45.0,
            roll_deg: 0.0,
            pause_agents: true,
        }
    }
}

/// The `photo` command: any subset of the panel, each value clamped to its slider range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
pub struct PhotoPatch {
    pub dof: Option<Dof>,
    pub focus: Option<f32>,
    pub aperture: Option<f32>,
    pub exposure: Option<f32>,
    pub fov: Option<f32>,
    pub roll: Option<f32>,
    pub pause_agents: Option<bool>,
}

fn clamp_finite(v: Option<f32>, (lo, hi): (f32, f32), slot: &mut f32) {
    if let Some(v) = v.filter(|v| v.is_finite()) {
        *slot = v.clamp(lo, hi);
    }
}

impl PhotoSettings {
    pub fn apply(&mut self, p: &PhotoPatch) {
        if let Some(d) = p.dof {
            self.dof = d;
        }
        clamp_finite(p.focus, FOCUS_M, &mut self.focus_m);
        clamp_finite(p.aperture, APERTURE_F, &mut self.aperture);
        clamp_finite(p.exposure, EXPOSURE_EV, &mut self.exposure_ev);
        clamp_finite(p.fov, FOV_DEG, &mut self.fov_deg);
        clamp_finite(p.roll, ROLL_DEG, &mut self.roll_deg);
        if let Some(b) = p.pause_agents {
            self.pause_agents = b;
        }
    }
}

/// The lens fields photo mode changes on Bevy's `DepthOfField`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lens {
    pub focal_distance: f32,
    pub aperture_f_stops: f32,
    pub sensor_height: f32,
    pub max_coc_px: f32,
}

impl Lens {
    fn of(d: &DepthOfField) -> Self {
        Self {
            focal_distance: d.focal_distance,
            aperture_f_stops: d.aperture_f_stops,
            sensor_height: d.sensor_height,
            max_coc_px: d.max_circle_of_confusion_diameter,
        }
    }

    fn write(&self, d: &mut DepthOfField) {
        d.focal_distance = self.focal_distance;
        d.aperture_f_stops = self.aperture_f_stops;
        d.sensor_height = self.sensor_height;
        d.max_circle_of_confusion_diameter = self.max_coc_px;
    }
}

/// What photo mode changes on the orbit camera, taken on enter and written back on exit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraSnapshot {
    pub transform: Transform,
    /// Vertical FOV in radians.
    pub fov: f32,
    pub ev100: Option<f32>,
    pub lens: Option<Lens>,
}

/// The camera state photo mode wants this frame. `lens: None` means the camera carries no
/// `DepthOfField` (the play camera drops the gate's far-focus one), so Manual inserts it and every
/// other choice and the exit take it off again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhotoLook {
    pub rotation: Quat,
    pub fov: f32,
    pub ev100: Option<f32>,
    pub lens: Option<Lens>,
    pub tilt: Option<TiltOverride>,
}

/// Tilt-shift strength for an f-number: full at f/1.4, gone at f/22, logarithmic between.
pub fn aperture_strength(f: f32) -> f32 {
    let (lo, hi) = APERTURE_F;
    ((hi / f.clamp(lo, hi)).log2() / (hi / lo).log2()).clamp(0.0, 1.0)
}

/// Panel values on entering: focus on the orbit camera's focus point and keep its framing, so
/// entering never jumps the view. DOF mode, aperture and the pause persist.
pub fn on_enter(s: &mut PhotoSettings, snap: &CameraSnapshot) {
    let t = &snap.transform;
    s.focus_m = t.translation.distance(focus(t)).clamp(FOCUS_M.0, FOCUS_M.1);
    s.fov_deg = snap.fov.to_degrees().clamp(FOV_DEG.0, FOV_DEG.1);
    s.roll_deg = 0.0;
    s.exposure_ev = 0.0;
}

/// The look for the panel, given the snapshot and the view direction the free camera has now.
/// `export_scale` widens the lens's pixel blur cap so a 4× export blurs as much as the screen.
pub fn look(
    s: &PhotoSettings,
    snap: &CameraSnapshot,
    forward: Vec3,
    export_scale: u32,
) -> PhotoLook {
    let (yaw, pitch) = yaw_pitch(forward);
    let lens = match s.dof {
        Dof::Manual => {
            let base = snap
                .lens
                .unwrap_or_else(|| Lens::of(&DepthOfField::default()));
            Some(Lens {
                focal_distance: s.focus_m,
                aperture_f_stops: s.aperture,
                sensor_height: base.sensor_height * MINIATURE_SENSOR,
                max_coc_px: base.max_coc_px * export_scale as f32,
            })
        }
        Dof::Off | Dof::Tilt => snap.lens,
    };
    PhotoLook {
        rotation: free_rotation(yaw, pitch, s.roll_deg.to_radians()),
        fov: s.fov_deg.to_radians(),
        // ev100 counts down as the image brightens.
        ev100: snap.ev100.map(|e| e - s.exposure_ev),
        lens,
        tilt: (s.dof == Dof::Tilt).then(|| TiltOverride {
            focus_m: s.focus_m,
            strength: aperture_strength(s.aperture),
        }),
    }
}

/// The open photo session: `Some` while the game is in photo mode.
#[derive(Resource, Default)]
pub struct PhotoSession {
    pub saved: Option<CameraSnapshot>,
}

type PhotoCamera<'a> = (
    &'a mut Transform,
    &'a mut Projection,
    Option<&'a mut Exposure>,
    Option<&'a mut DepthOfField>,
);

/// Enters and leaves photo mode on `GameState::photo_mode` edges and applies the panel while it is
/// on (after `game::apply_actions`, which moves the free camera).
pub fn sync(
    mut commands: Commands,
    game: Res<GameState>,
    mut session: ResMut<PhotoSession>,
    mut settings: ResMut<PhotoSettings>,
    exports: Res<PhotoExports>,
    tilt: Option<Res<TiltOverride>>,
    mut paused: ResMut<AgentsPaused>,
    mut cams: Query<(Entity, PhotoCamera), With<Camera3d>>,
) {
    let Some((cam, (mut t, mut proj, mut exposure, mut dof))) = cams.iter_mut().next() else {
        return;
    };
    let fov = match &*proj {
        Projection::Perspective(p) => p.fov,
        _ => return,
    };
    match (game.photo_mode, session.saved) {
        (true, None) => {
            let snap = CameraSnapshot {
                transform: *t,
                fov,
                ev100: exposure.as_ref().map(|e| e.ev100),
                lens: dof.as_deref().map(Lens::of),
            };
            on_enter(&mut settings, &snap);
            session.saved = Some(snap);
            info!("photo mode: on");
        }
        (false, Some(snap)) => {
            *t = snap.transform;
            if let Projection::Perspective(p) = &mut *proj {
                p.fov = snap.fov;
            }
            if let (Some(e), Some(v)) = (exposure.as_deref_mut(), snap.ev100) {
                e.ev100 = v;
            }
            set_lens(&mut commands, cam, dof.as_deref_mut(), snap.lens);
            commands.remove_resource::<TiltOverride>();
            paused.0 = false;
            session.saved = None;
            info!("photo mode: off, orbit camera restored");
            return;
        }
        (false, None) => return,
        (true, Some(_)) => {}
    }
    let snap = session.saved.expect("set above");
    let l = look(&settings, &snap, *t.forward(), exports.scale());
    if t.rotation != l.rotation {
        t.rotation = l.rotation;
    }
    if let Projection::Perspective(p) = &mut *proj
        && p.fov != l.fov
    {
        p.fov = l.fov;
    }
    if let (Some(e), Some(v)) = (exposure.as_deref_mut(), l.ev100)
        && e.ev100 != v
    {
        e.ev100 = v;
    }
    set_lens(&mut commands, cam, dof.as_deref_mut(), l.lens);
    match (l.tilt, tilt.as_deref()) {
        (Some(want), Some(have)) if want == *have => {}
        (Some(want), _) => commands.insert_resource(want),
        (None, Some(_)) => commands.remove_resource::<TiltOverride>(),
        (None, None) => {}
    }
    paused.0 = settings.pause_agents;
}

/// Brings the camera's `DepthOfField` to `want`: written in place, inserted, or removed. It is a
/// post pass with its own pipelines, so adding it changes no mesh pipeline's bindings.
fn set_lens(
    commands: &mut Commands,
    cam: Entity,
    have: Option<&mut DepthOfField>,
    want: Option<Lens>,
) {
    match (have, want) {
        (Some(d), Some(l)) if Lens::of(d) != l => l.write(d),
        (Some(_), Some(_)) | (None, None) => {}
        (None, Some(l)) => {
            let mut d = DepthOfField::default();
            l.write(&mut d);
            commands.entity(cam).insert(d);
        }
        (Some(_), None) => {
            commands.entity(cam).remove::<DepthOfField>();
        }
    }
}

/// PNG size for an export: the window's logical size times `scale` (1, 2 or 4), refused above
/// `max_scale` or past the GPU's largest texture side (mockup S8.7 suggests the next scale down).
pub fn export_size(base: UVec2, scale: u32, max_dim: u32, max_scale: u32) -> Result<UVec2, String> {
    if !EXPORT_SCALES.contains(&scale) {
        return Err(format!("scale {scale}× is not one of 1×, 2× or 4×"));
    }
    if scale > max_scale {
        return Err(format!(
            "{scale}× is off on the web until it is tested for GPU memory. {max_scale}× is the largest."
        ));
    }
    if base.x == 0 || base.y == 0 {
        return Err("the window has no size".into());
    }
    let size = base * scale;
    if size.max_element() > max_dim {
        let hint = EXPORT_SCALES
            .iter()
            .rev()
            .find(|&&s| (base * s).max_element() <= max_dim)
            .map(|s| format!(" {s}× fits."))
            .unwrap_or_default();
        return Err(format!(
            "{} × {} is past this GPU's {max_dim} px texture limit.{hint}",
            size.x, size.y
        ));
    }
    Ok(size)
}

struct ExportJob {
    name: String,
    scale: u32,
    size: UVec2,
    image: Handle<Image>,
    window_target: RenderTarget,
    frames: u32,
}

/// Queued and running exports. One renders at a time.
#[derive(Resource, Default)]
pub struct PhotoExports {
    queue: VecDeque<(u32, String)>,
    job: Option<ExportJob>,
}

impl PhotoExports {
    pub fn push(&mut self, scale: u32, name: String) {
        self.queue.push_back((scale, name));
    }

    /// The running export's scale, else 1.
    pub fn scale(&self) -> u32 {
        self.job.as_ref().map_or(1, |j| j.scale)
    }

    fn state(&self) -> Value {
        match &self.job {
            Some(j) => {
                json!({ "name": j.name, "scale": j.scale, "width": j.size.x, "height": j.size.y })
            }
            None => Value::Null,
        }
    }
}

/// An uninitialised colour target the size of the export (no CPU copy of a 4× buffer).
fn target_image(size: UVec2) -> Image {
    let mut img = Image::new_uninit(
        Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_descriptor.usage =
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC;
    img
}

/// Renders the game camera into an offscreen image at the export size for a few frames, captures
/// it to PNG, then hands the camera back to the window. The same camera entity renders, so every
/// pipeline and binding matches the on-screen view.
pub fn run_exports(
    mut commands: Commands,
    mut exports: ResMut<PhotoExports>,
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    limits: Option<Res<GpuLimits>>,
    mut cams: Query<&mut RenderTarget, With<Camera3d>>,
) {
    let exports = &mut *exports;
    if let Some(job) = &mut exports.job {
        job.frames += 1;
        if job.frames == EXPORT_WARM_FRAMES {
            let name = job.name.clone();
            capture::request_image(&mut commands, &job.name, job.image.clone(), move |res| {
                publish_event(&match res {
                    Ok(px) => GameEvent::PhotoExported {
                        name: name.clone(),
                        width: px.x,
                        height: px.y,
                    },
                    Err(reason) => GameEvent::PhotoExportFailed {
                        name: name.clone(),
                        reason,
                    },
                });
            });
        } else if job.frames > EXPORT_WARM_FRAMES {
            if let Some(mut target) = cams.iter_mut().next() {
                *target = job.window_target.clone();
            }
            images.remove(&job.image);
            exports.job = None;
        }
        return;
    }
    let Some((scale, name)) = exports.queue.pop_front() else {
        return;
    };
    let fail = |reason: String| {
        warn!("photo export {name}: {reason}");
        publish_event(&GameEvent::PhotoExportFailed {
            name: name.clone(),
            reason,
        });
    };
    let Some(mut target) = cams.iter_mut().next() else {
        return fail("no camera".into());
    };
    let Ok(window) = windows.single() else {
        return fail("no window".into());
    };
    let base = UVec2::new(
        window.width().round() as u32,
        window.height().round() as u32,
    );
    let max_dim = limits.map_or(8192, |l| l.max_texture_dimension_2d);
    let size = match export_size(base, scale, max_dim, MAX_EXPORT_SCALE) {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    info!(
        "photo export {name}: {}x{} ({scale}x of {}x{})",
        size.x, size.y, base.x, base.y
    );
    let image = images.add(target_image(size));
    // With no camera on the window, Bevy 0.20 acquires and presents no swap-chain texture for it,
    // so the window keeps its last frame until the camera comes back (no backdrop flash, A14).
    // Another camera targeting the window during the job would break that.
    let window_target = std::mem::replace(&mut *target, RenderTarget::Image(image.clone().into()));
    exports.job = Some(ExportJob {
        name,
        scale,
        size,
        image,
        window_target,
        frames: 0,
    });
}

/// The `photo` object of the state JSON.
pub fn state_json(
    s: &PhotoSettings,
    session: &PhotoSession,
    exports: &PhotoExports,
    paused: bool,
    sim_ticks: u64,
    limits: Option<&GpuLimits>,
) -> Value {
    let r = |v: f32| (f64::from(v) * 100.0).round() / 100.0;
    json!({
        "on": session.saved.is_some(),
        "dof": s.dof,
        "focus": r(s.focus_m),
        "aperture": r(s.aperture),
        "exposure": r(s.exposure_ev),
        "fov": r(s.fov_deg),
        "roll": r(s.roll_deg),
        "pause_agents": s.pause_agents,
        "agents_paused": paused,
        "sim_ticks": sim_ticks,
        "exporting": exports.state(),
        "max_texture": limits.map(|l| l.max_texture_dimension_2d),
    })
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use hex_render::{ActiveScenario, Scenario};

    use super::*;
    use crate::actions::Action;
    use crate::sim::CarSim;

    #[test]
    fn export_size_is_the_window_times_the_scale() {
        let base = UVec2::new(1440, 900);
        assert_eq!(export_size(base, 1, 8192, 4), Ok(UVec2::new(1440, 900)));
        assert_eq!(export_size(base, 2, 8192, 4), Ok(UVec2::new(2880, 1800)));
        assert_eq!(export_size(base, 4, 8192, 4), Ok(UVec2::new(5760, 3600)));
        assert!(export_size(base, 3, 8192, 4).is_err(), "only 1, 2 and 4");
        assert!(export_size(UVec2::ZERO, 1, 8192, 4).is_err());
    }

    #[test]
    fn export_past_the_texture_limit_is_refused_with_the_scale_that_fits() {
        let e = export_size(UVec2::new(2560, 1440), 4, 8192, 4).unwrap_err();
        assert!(e.contains("10240 × 5760") && e.contains("2× fits"), "{e}");
        assert_eq!(
            export_size(UVec2::new(2560, 1440), 2, 8192, 4),
            Ok(UVec2::new(5120, 2880))
        );
    }

    #[test]
    fn the_web_cap_refuses_4x_and_keeps_2x() {
        let base = UVec2::new(1280, 800);
        let e = export_size(base, 4, 16384, 2).unwrap_err();
        assert!(
            e.contains("4× is off on the web") && e.contains("2×"),
            "{e}"
        );
        assert_eq!(export_size(base, 2, 16384, 2), Ok(UVec2::new(2560, 1600)));
        assert_eq!(MAX_EXPORT_SCALE, 4, "native keeps 4×");
    }

    #[test]
    fn patch_clamps_to_the_slider_ranges() {
        let mut s = PhotoSettings::default();
        s.apply(&PhotoPatch {
            dof: Some(Dof::Manual),
            focus: Some(0.0),
            aperture: Some(100.0),
            exposure: Some(f32::NAN),
            fov: Some(120.0),
            roll: Some(-90.0),
            pause_agents: Some(false),
        });
        assert_eq!(s.dof, Dof::Manual);
        assert_eq!((s.focus_m, s.aperture, s.exposure_ev), (1.0, 22.0, 0.0));
        assert_eq!(
            (s.fov_deg, s.roll_deg, s.pause_agents),
            (100.0, -45.0, false)
        );
    }

    #[test]
    fn aperture_maps_to_tilt_strength() {
        assert_eq!(aperture_strength(1.4), 1.0);
        assert_eq!(aperture_strength(22.0), 0.0);
        assert!(aperture_strength(2.8) > aperture_strength(8.0));
    }

    #[test]
    fn manual_dof_takes_the_panel_and_tilt_uses_the_shared_kernel() {
        let snap = CameraSnapshot {
            transform: Transform::from_xyz(0.0, 40.0, 60.0).looking_at(Vec3::ZERO, Vec3::Y),
            fov: 45f32.to_radians(),
            ev100: Some(10.0),
            lens: Some(Lens {
                focal_distance: 900.0,
                aperture_f_stops: 8.0,
                sensor_height: 0.01866,
                max_coc_px: 64.0,
            }),
        };
        let mut s = PhotoSettings {
            dof: Dof::Manual,
            focus_m: 20.0,
            aperture: 2.8,
            exposure_ev: 1.0,
            ..default()
        };
        let l = look(&s, &snap, *snap.transform.forward(), 4);
        let lens = l.lens.unwrap();
        assert_eq!((lens.focal_distance, lens.aperture_f_stops), (20.0, 2.8));
        assert_eq!(
            lens.max_coc_px, 256.0,
            "a 4× export keeps the on-screen blur"
        );
        assert_eq!(l.ev100, Some(9.0), "+1 EV is one stop brighter");
        assert!(l.tilt.is_none());
        s.dof = Dof::Tilt;
        let l = look(&s, &snap, *snap.transform.forward(), 1);
        assert_eq!(
            l.lens, snap.lens,
            "the lens stays at the game camera's far focus"
        );
        assert_eq!(l.tilt.unwrap().focus_m, 20.0);
        s.dof = Dof::Off;
        assert!(look(&s, &snap, *snap.transform.forward(), 1).tilt.is_none());
    }

    fn app() -> App {
        let mut app = App::new();
        app.insert_resource(GameState::new(1))
            .init_resource::<PhotoSession>()
            .init_resource::<PhotoSettings>()
            .init_resource::<PhotoExports>()
            .init_resource::<AgentsPaused>()
            .insert_resource(ActiveScenario(Scenario::Play));
        let game = GameState::new(1);
        let cars = CarSim::new(&game.world, &game.catalog);
        app.insert_resource(cars);
        let t = Transform::from_xyz(0.0, 60.0, 80.0).looking_at(Vec3::new(4.0, 0.0, 0.0), Vec3::Y);
        app.world_mut().spawn((
            Camera3d::default(),
            t,
            Projection::Perspective(PerspectiveProjection::default()),
            Exposure { ev100: 12.0 },
            DepthOfField {
                focal_distance: 900.0,
                aperture_f_stops: 8.0,
                ..default()
            },
        ));
        app
    }

    fn camera(app: &mut App) -> (Transform, f32, f32, Lens) {
        let mut q = app
            .world_mut()
            .query::<(&Transform, &Projection, &Exposure, &DepthOfField)>();
        let (t, p, e, d) = q.single(app.world()).unwrap();
        let Projection::Perspective(p) = p else {
            unreachable!()
        };
        (*t, p.fov, e.ev100, Lens::of(d))
    }

    fn act(app: &mut App, a: Action) {
        if a.is_camera() {
            let mut q = app.world_mut().query::<&mut Transform>();
            let mut t = q.single_mut(app.world_mut()).unwrap();
            *t = crate::camera::apply(*t, &a);
        } else {
            app.world_mut().resource_mut::<GameState>().apply(a);
        }
        app.world_mut().run_system_once(sync).unwrap();
    }

    #[test]
    fn enter_frees_the_camera_and_exit_restores_the_orbit_camera() {
        let mut app = app();
        let before = camera(&mut app);
        act(&mut app, Action::TogglePhoto);
        assert!(app.world().resource::<PhotoSession>().saved.is_some());
        assert!(
            app.world().resource::<AgentsPaused>().0,
            "entering pauses agents"
        );
        let entered = camera(&mut app).0;
        assert_eq!(entered.translation, before.0.translation);
        assert!(
            entered.rotation.angle_between(before.0.rotation) < 1e-4,
            "entering never jumps the view"
        );
        let focus = app.world().resource::<PhotoSettings>().focus_m;
        assert!(
            (focus
                - before
                    .0
                    .translation
                    .distance(crate::camera::focus(&before.0)))
            .abs()
                < 1e-3
        );
        assert!(
            app.world().get_resource::<TiltOverride>().is_some(),
            "tilt preset by default"
        );

        // Fly, look, a scripted camera move, and every panel value.
        act(&mut app, Action::Fly(Vec3::new(3.0, -10.0, 25.0)));
        act(&mut app, Action::Look(Vec2::new(200.0, 80.0)));
        app.world_mut()
            .resource_mut::<PhotoSettings>()
            .apply(&PhotoPatch {
                dof: Some(Dof::Manual),
                focus: Some(12.0),
                aperture: Some(1.4),
                exposure: Some(1.5),
                fov: Some(70.0),
                roll: Some(10.0),
                pause_agents: None,
            });
        app.world_mut().run_system_once(sync).unwrap();
        let during = camera(&mut app);
        assert_ne!(during.0.translation, before.0.translation);
        assert!((during.1 - 70f32.to_radians()).abs() < 1e-5);
        assert_eq!(during.2, 12.0 - 1.5);
        assert_eq!(during.3.focal_distance, 12.0);
        assert!(
            app.world().get_resource::<TiltOverride>().is_none(),
            "manual drops the preset"
        );
        let roll = during.0.up().dot(Vec3::Y);
        assert!(roll < 0.999, "roll tilts the horizon");

        act(&mut app, Action::ExitPhoto);
        assert_eq!(
            camera(&mut app),
            before,
            "orbit camera, FOV, exposure and lens restored"
        );
        assert!(app.world().resource::<PhotoSession>().saved.is_none());
        assert!(
            !app.world().resource::<AgentsPaused>().0,
            "exiting resumes agents"
        );
        assert!(app.world().get_resource::<TiltOverride>().is_none());
        // Esc outside photo mode does nothing.
        act(&mut app, Action::ExitPhoto);
        assert!(!app.world().resource::<GameState>().photo_mode);
    }

    #[test]
    fn manual_dof_adds_the_lens_to_a_camera_without_one_and_exit_takes_it_off() {
        let mut app = app();
        let cam = app
            .world_mut()
            .query_filtered::<Entity, With<Camera3d>>()
            .single(app.world())
            .unwrap();
        app.world_mut().entity_mut(cam).remove::<DepthOfField>();
        let has_dof = |app: &App| app.world().get::<DepthOfField>(cam).is_some();
        act(&mut app, Action::TogglePhoto);
        assert!(!has_dof(&app), "the tilt preset adds no lens");
        app.world_mut()
            .resource_mut::<PhotoSettings>()
            .apply(&PhotoPatch {
                dof: Some(Dof::Manual),
                focus: Some(20.0),
                ..default()
            });
        app.world_mut().run_system_once(sync).unwrap();
        let d = app
            .world()
            .get::<DepthOfField>(cam)
            .expect("manual inserts the lens");
        assert_eq!(d.focal_distance, 20.0);
        assert_eq!(
            d.sensor_height,
            DepthOfField::default().sensor_height * MINIATURE_SENSOR
        );
        app.world_mut()
            .resource_mut::<PhotoSettings>()
            .apply(&PhotoPatch {
                dof: Some(Dof::Off),
                ..default()
            });
        app.world_mut().run_system_once(sync).unwrap();
        assert!(!has_dof(&app), "off takes it away");
        app.world_mut()
            .resource_mut::<PhotoSettings>()
            .apply(&PhotoPatch {
                dof: Some(Dof::Manual),
                ..default()
            });
        app.world_mut().run_system_once(sync).unwrap();
        assert!(has_dof(&app));
        act(&mut app, Action::TogglePhoto);
        assert!(!has_dof(&app), "exit leaves the camera as it was");
    }

    #[test]
    fn paused_agents_do_not_tick_and_resume_on_exit() {
        let mut app = app();
        let ticks = |app: &App| app.world().resource::<CarSim>().ticks();
        app.world_mut().run_system_once(crate::sim::tick).unwrap();
        assert_eq!(ticks(&app), 1);
        act(&mut app, Action::TogglePhoto);
        for _ in 0..3 {
            app.world_mut().run_system_once(crate::sim::tick).unwrap();
        }
        assert_eq!(ticks(&app), 1, "the car sim holds while paused");
        app.world_mut()
            .resource_mut::<PhotoSettings>()
            .apply(&PhotoPatch {
                pause_agents: Some(false),
                ..default()
            });
        app.world_mut().run_system_once(sync).unwrap();
        assert!(
            !app.world().resource::<AgentsPaused>().0,
            "the panel's switch lets them run"
        );
        act(&mut app, Action::TogglePhoto);
        app.world_mut().run_system_once(crate::sim::tick).unwrap();
        assert_eq!(ticks(&app), 2, "resumed after exit");
    }
}
