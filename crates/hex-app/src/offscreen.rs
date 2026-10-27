//! `--offscreen on` (native): the game camera draws into an image of the window's physical size
//! and scale factor instead of the window. On macOS wgpu hands out no swap-chain texture while the
//! window is occluded (another window on top, minimized, on another Space, or a locked or sleeping
//! screen), and Bevy then skips every pass that draws to the window: the frame runs shadows,
//! prepass and SSAO only (`diag().window`), and a window screenshot comes back all black.
//! Offscreen, every pass runs whatever the window's state, nothing is presented, and every
//! screenshot captures the image ([`crate::capture::shot_for`]).
//!
//! Capture runs (`args::Flags::capture_default`) turn it on unless `--offscreen off` is given, and
//! pace themselves to [`CAPTURE_FPS`] while no `frame_cap` is set: with nothing presented no vsync
//! paces the loop, and frame-counted waits (thumbs warm-up, script `wait`s, pack holds) would
//! shrink in wall time against pipeline compiles and pack loads. An explicit `--offscreen on`
//! (perf runs on a host whose screen is locked) runs unpaced, so frame times are the render
//! loop's own.
use std::time::{Duration, Instant};

use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::prelude::*;
use bevy::ui::IsDefaultUiCamera;
use bevy::window::PrimaryWindow;

use crate::capture::OffscreenTarget;
use crate::frame_cap::FrameCap;

/// A capture run's frame rate while no `frame_cap` is set: what an unfocused window steps at
/// (Bevy's `WinitSettings::game()`).
pub const CAPTURE_FPS: f64 = 60.0;

pub struct OffscreenPlugin {
    /// Pace the loop to [`CAPTURE_FPS`] while no frame cap is set (a capture run's default).
    pub paced: bool,
}

impl Plugin for OffscreenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            retarget.before(bevy::camera::CameraUpdateSystems),
        );
        if self.paced {
            app.add_systems(Last, pace);
        }
    }
}

/// Whether `target` is drawn to the window, or to an offscreen image `retarget` has replaced.
fn needs_retarget(target: &RenderTarget, stale: Option<&ImageRenderTarget>) -> bool {
    match target {
        RenderTarget::Window(_) => true,
        RenderTarget::Image(i) => stale.is_some_and(|s| s == i),
        _ => false,
    }
}

/// Points every 3D camera that targets the window at the offscreen image, including cameras
/// spawned later (a load, a scene rebuild), and makes it the default UI camera so the FPS counter
/// draws into the image as it does into the window. The image follows the window's physical size
/// and scale factor (it is replaced when they change), so the camera's logical viewport, picking
/// and UI layout match the window's.
fn retarget(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cams: Query<(Entity, &mut RenderTarget), With<Camera3d>>,
    current: Option<Res<OffscreenTarget>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let size = UVec2::new(window.physical_width(), window.physical_height());
    if size.x == 0 || size.y == 0 {
        return;
    }
    let scale_factor = window.scale_factor();
    let current = current.map(|c| c.0.clone());
    let fits = current.as_ref().is_some_and(|t| {
        t.scale_factor == scale_factor && images.get(&t.handle).is_some_and(|i| i.size() == size)
    });
    let (target, stale) = if fits {
        (current.clone().expect("fits implies a target"), None)
    } else {
        info!(
            "offscreen: the camera draws into a {}x{} image (scale factor {scale_factor})",
            size.x, size.y
        );
        let target = ImageRenderTarget {
            handle: images.add(crate::photo::target_image(size)),
            scale_factor,
        };
        commands.insert_resource(OffscreenTarget(target.clone()));
        (target, current)
    };
    for (cam, mut t) in &mut cams {
        if needs_retarget(&t, stale.as_ref()) {
            *t = RenderTarget::Image(target.clone());
            commands.entity(cam).insert(IsDefaultUiCamera);
        }
    }
}

/// Sleeps the main thread to [`CAPTURE_FPS`] while no frame cap is set; `frame_cap::limit` paces
/// it otherwise.
fn pace(cap: Res<FrameCap>, mut deadline: Local<Option<Instant>>) {
    if cap.0.is_some() {
        *deadline = None;
        return;
    }
    let interval = Duration::from_secs_f64(1.0 / CAPTURE_FPS);
    let (sleep, next) = crate::frame_cap::pace(Instant::now(), *deadline, interval);
    if !sleep.is_zero() {
        std::thread::sleep(sleep);
    }
    *deadline = Some(next);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::window::WindowResolution;

    fn app(size: (u32, u32)) -> App {
        let mut app = App::new();
        app.init_resource::<Assets<Image>>()
            .add_systems(Update, retarget);
        app.world_mut().spawn((
            Window {
                resolution: WindowResolution::new(size.0, size.1).with_scale_factor_override(1.0),
                ..default()
            },
            PrimaryWindow,
        ));
        app
    }

    fn offscreen(app: &App) -> ImageRenderTarget {
        app.world().resource::<OffscreenTarget>().0.clone()
    }

    /// The image a camera or screenshot targets, `None` for the window.
    fn image_of(t: Option<&RenderTarget>) -> Option<ImageRenderTarget> {
        match t {
            Some(RenderTarget::Image(i)) => Some(i.clone()),
            _ => None,
        }
    }

    fn cam_image(app: &App, cam: Entity) -> Option<ImageRenderTarget> {
        image_of(app.world().get::<RenderTarget>(cam))
    }

    #[test]
    fn a_window_camera_draws_into_the_image_and_screenshots_capture_it() {
        let mut app = app((384, 384));
        assert!(app.world().get_resource::<OffscreenTarget>().is_none());
        assert!(
            matches!(
                crate::capture::shot_for(None).0,
                RenderTarget::Window(bevy::window::WindowRef::Primary)
            ),
            "without an offscreen image a screenshot captures the window"
        );
        let cam = app.world_mut().spawn(Camera3d::default()).id();
        let other = Handle::<Image>::default();
        let export = app
            .world_mut()
            .spawn((
                Camera3d::default(),
                RenderTarget::Image(other.clone().into()),
            ))
            .id();
        app.update();
        let target = offscreen(&app);
        assert_eq!(target.scale_factor, 1.0);
        let image = app.world().resource::<Assets<Image>>().get(&target.handle);
        assert_eq!(image.map(Image::size), Some(UVec2::new(384, 384)));
        assert_eq!(cam_image(&app, cam), Some(target.clone()));
        assert!(app.world().get::<IsDefaultUiCamera>(cam).is_some());
        assert_eq!(
            cam_image(&app, export),
            Some(other.into()),
            "a camera already on another image (a photo export) is left alone"
        );
        let shot = crate::capture::shot_for(app.world().get_resource::<OffscreenTarget>());
        assert_eq!(image_of(Some(&shot.0)), Some(target.clone()));
        // A camera spawned later (a scenario rebuild) joins the same image.
        let later = app.world_mut().spawn(Camera3d::default()).id();
        app.update();
        assert_eq!(offscreen(&app), target, "the image is made once");
        assert_eq!(cam_image(&app, later), Some(target));
    }

    #[test]
    fn the_image_follows_the_window_size_and_scale_factor() {
        let mut app = app((640, 400));
        let cam = app.world_mut().spawn(Camera3d::default()).id();
        app.update();
        let first = offscreen(&app);
        let mut q = app
            .world_mut()
            .query_filtered::<&mut Window, With<PrimaryWindow>>();
        let mut window = q.single_mut(app.world_mut()).unwrap();
        window.resolution.set_scale_factor_override(Some(2.0));
        window.resolution.set_physical_resolution(1280, 800);
        app.update();
        let second = offscreen(&app);
        assert_ne!(second, first);
        assert_eq!(second.scale_factor, 2.0);
        let image = app.world().resource::<Assets<Image>>().get(&second.handle);
        assert_eq!(image.map(Image::size), Some(UVec2::new(1280, 800)));
        assert_eq!(cam_image(&app, cam), Some(second));
    }
}
