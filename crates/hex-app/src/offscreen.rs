//! `--offscreen on` (native): the game camera draws into an image the window's size instead of the
//! window. On macOS wgpu hands out no swap-chain texture while the window is occluded (another
//! window on top, minimized, or a locked or sleeping screen), and Bevy then skips every pass that
//! draws to the window: the frame runs shadows, prepass and SSAO only (`diag().window`). Offscreen,
//! every pass runs whatever the window's state, and nothing is presented, so frame times are the
//! render loop's own (no display pacing). Used by perf runs on a host whose screen is locked.
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

pub struct OffscreenPlugin;

impl Plugin for OffscreenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            retarget.before(bevy::camera::CameraUpdateSystems),
        );
    }
}

/// Points every 3D camera that targets the window at the offscreen image (made once, at the
/// window's physical size), including cameras spawned later (a load, a scene rebuild).
fn retarget(
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cams: Query<&mut RenderTarget, With<Camera3d>>,
    mut image: Local<Option<Handle<Image>>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    for mut target in &mut cams {
        if !matches!(*target, RenderTarget::Window(_)) {
            continue;
        }
        let handle = image
            .get_or_insert_with(|| {
                let size = UVec2::new(window.physical_width(), window.physical_height());
                info!(
                    "offscreen: the camera draws into a {}x{} image",
                    size.x, size.y
                );
                images.add(crate::photo::target_image(size))
            })
            .clone();
        *target = RenderTarget::Image(handle.into());
    }
}
