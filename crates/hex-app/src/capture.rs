//! In-app screenshots to PNG: a file under `target/shots` natively, a data URL on the web.
//!
//! A screenshot captures the primary window, or while `--offscreen` is on (`crate::offscreen`)
//! the image the game camera draws into instead ([`shot_for`]). On macOS an occluded window gets
//! no swap-chain texture, so a window capture comes back all black however long it is retaken;
//! the offscreen image draws whatever the window's state, and capture runs turn it on by default.
use std::io::Cursor;
use std::sync::atomic::{AtomicU32, Ordering};

use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::ecs::observer::IntoEntityObserver;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use image::{DynamicImage, ImageFormat, RgbaImage};

#[cfg(not(target_arch = "wasm32"))]
pub const SHOT_DIR: &str = "target/shots";

static PENDING: AtomicU32 = AtomicU32::new(0);
/// Held captures ([`request_held`]) whose frame is not stored yet.
static HELD: AtomicU32 = AtomicU32::new(0);

/// How long a capture whose frame comes back all black is captured again before the run stops
/// with exit 1. Wall time, not frames: under host load the GPU can return black frames for
/// several seconds, across several shots in a row. A window capture of an occluded window
/// (`--offscreen off`) stays black until the window is uncovered.
pub const BLANK_WAIT: std::time::Duration = std::time::Duration::from_secs(60);
/// A held capture pauses `Time<Virtual>` for a whole number of these rendered frames: the hold
/// ends on the first multiple after the frame is stored. The readback lands a wall-clock-dependent
/// number of frames after the request, and `FrameCount` (TAA jitter, SSAO, shadow and blue-noise
/// phases) keeps counting while simulated time stands still, so a hold of that raw length would
/// shift every later frame's noise against its simulated time. 64 is a multiple of every
/// frame-count period Bevy's passes use (the TAA Halton 8, SSAO's `% 64`).
pub const HOLD_FRAMES: u32 = 64;
/// Test hook: the first N frames of every held capture count as all black, so the retake runs
/// on a GPU that draws.
const FORCE_BLACK_ENV: &str = "HEXW_SHOT_FORCE_BLACK";

/// Finished web captures keyed by name, taken by `hex_world_take_shot`.
#[cfg(target_arch = "wasm32")]
pub static SHOTS: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

/// The image `crate::offscreen` points the game cameras at, exactly as their `RenderTarget` holds
/// it: a screenshot finds the camera output it captures by target, scale factor included.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct OffscreenTarget(pub ImageRenderTarget);

/// What a screenshot captures: the offscreen image when there is one, else the primary window.
pub fn shot_for(offscreen: Option<&OffscreenTarget>) -> Screenshot {
    match offscreen {
        Some(t) => Screenshot(RenderTarget::Image(t.0.clone())),
        None => Screenshot::primary_window(),
    }
}

/// Spawns a screenshot of [`shot_for`]'s target with `observer` on it. The target is read when the
/// command applies, so a retake requested after the offscreen image appears captures it.
fn spawn_shot<M>(commands: &mut Commands, observer: impl IntoEntityObserver<M>) {
    commands.queue(move |world: &mut World| {
        let shot = shot_for(world.get_resource::<OffscreenTarget>());
        world.spawn(shot).observe(observer);
    });
}

/// Screenshots requested but not yet encoded; exit waits for this to reach zero.
pub fn pending() -> u32 {
    PENDING.load(Ordering::SeqCst)
}

/// Records `target/shots/<name>.png` with the exit watchdog as a file this run writes.
pub fn expect_shot(_name: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    crate::exit_guard::expect(format!("{SHOT_DIR}/{_name}.png"));
}

/// The web's screenshots: native ones are all [`request_held`] or retaken by thumbs.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn request(commands: &mut Commands, name: &str) {
    expect_shot(name);
    PENDING.fetch_add(1, Ordering::SeqCst);
    let name = name.to_string();
    spawn_shot(commands, move |shot: On<ScreenshotCaptured>| {
        let _ = store(&name, encode_png(shot.image.clone()));
        PENDING.fetch_sub(1, Ordering::SeqCst);
    });
}

/// Every pixel's colour channels are zero: a frame the GPU never drew (a window capture while the
/// window is occluded, a sleeping display, or heavy GPU contention), as opposed to a dark scene,
/// which always carries some colour.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn all_black(data: &[u8]) -> bool {
    !data.is_empty()
        && data
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0)
}

/// Like [`request`], but an all-black frame is not stored unless `keep_black` is set; `blank` is
/// raised instead so the caller can capture again.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn request_unless_blank(
    commands: &mut Commands,
    name: &str,
    blank: std::sync::Arc<std::sync::atomic::AtomicBool>,
    keep_black: bool,
) {
    expect_shot(name);
    PENDING.fetch_add(1, Ordering::SeqCst);
    let name = name.to_string();
    spawn_shot(commands, move |shot: On<ScreenshotCaptured>| {
        let black = shot.image.data.as_deref().is_some_and(all_black);
        if black {
            blank.store(true, Ordering::SeqCst);
        }
        if !black || keep_black {
            let _ = store(&name, encode_png(shot.image.clone()));
        } else {
            warn!("screenshot {name} came back all black; capturing again");
        }
        PENDING.fetch_sub(1, Ordering::SeqCst);
    });
}

/// What a held capture does with a frame that came back `waited` after the shot was requested.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Store,
    Retake,
    /// Still all black at [`BLANK_WAIT`]: the frame is stored so the shot checks see it, then the
    /// run stops.
    Fail,
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn verdict(black: bool, waited: std::time::Duration) -> Verdict {
    match (black, waited >= BLANK_WAIT) {
        (false, _) => Verdict::Store,
        (true, false) => Verdict::Retake,
        (true, true) => Verdict::Fail,
    }
}

/// Held captures are out, or `Time<Virtual>` was still paused for one this frame: a script waits
/// for both, so its next command runs at the simulated time it would have without the hold.
pub fn holding(time: &Time<bevy::time::Virtual>) -> bool {
    HELD.load(Ordering::SeqCst) > 0 || time.was_paused()
}

/// Like [`request`], for a scripted or CLI screenshot: an all-black frame is captured again until
/// one is not, for up to [`BLANK_WAIT`] after the request, and the run is held meanwhile:
/// [`holding`] is true and [`hold_time`] pauses simulated time, so a retake draws the scene the
/// shot was requested in.
/// On the web it is a plain [`request`]: the page's promise has its own timeout and no black-frame
/// report.
pub fn request_held(commands: &mut Commands, name: &str) {
    #[cfg(target_arch = "wasm32")]
    request(commands, name);
    #[cfg(not(target_arch = "wasm32"))]
    {
        expect_shot(name);
        PENDING.fetch_add(1, Ordering::SeqCst);
        HELD.fetch_add(1, Ordering::SeqCst);
        capture_held(commands, name.to_string(), std::time::Instant::now(), 0);
    }
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn capture_held(commands: &mut Commands, name: String, since: std::time::Instant, blanks: u32) {
    spawn_shot(
        commands,
        move |shot: On<ScreenshotCaptured>, mut commands: Commands| {
            let black =
                blanks < forced_black() || shot.image.data.as_deref().is_some_and(all_black);
            let waited = since.elapsed().as_secs_f32();
            match verdict(black, since.elapsed()) {
                Verdict::Retake => {
                    warn!("screenshot {name} came back all black; capturing again");
                    capture_held(&mut commands, name.clone(), since, blanks + 1);
                    return;
                }
                Verdict::Store => {
                    let _ = store(&name, encode_png(shot.image.clone()));
                    if blanks > 0 {
                        info!(
                            "screenshot {name} rendered after {waited:.1} s and {blanks} black frame(s)"
                        );
                    }
                }
                Verdict::Fail => {
                    let _ = store(&name, encode_png(shot.image.clone()));
                    error!(
                        "screenshot {name} still all black {waited:.1} s after the request ({} black frames); exiting 1",
                        blanks + 1
                    );
                    std::process::exit(1);
                }
            }
            HELD.fetch_sub(1, Ordering::SeqCst);
            PENDING.fetch_sub(1, Ordering::SeqCst);
        },
    );
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn forced_black() -> u32 {
    static N: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *N.get_or_init(|| {
        std::env::var(FORCE_BLACK_ENV)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    })
}

/// Whether a hold paused `frames` frames ago ends this frame: no held capture is out and the
/// pause has lasted a whole number of [`HOLD_FRAMES`].
fn hold_ends(frames: u32, held: bool) -> bool {
    !held && frames > 0 && frames.is_multiple_of(HOLD_FRAMES)
}

/// Pauses `Time<Virtual>` while a held capture is out and resumes it once none is, on the first
/// multiple of [`HOLD_FRAMES`] frames since the pause. Run after the requests: the pause takes
/// effect at the next frame's time update, so the requested frame itself advances as usual, and
/// from then until the hold ends no `Time` delta, fixed tick or fixed water step passes. A
/// readback still out a further [`BLANK_WAIT`] past the retake bound stops the run with exit 1, so
/// a capture that never returns cannot hold a script forever.
pub fn hold_time(
    mut time: ResMut<Time<bevy::time::Virtual>>,
    mut paused_at: Local<Option<(std::time::Instant, u32)>>,
) {
    let held = HELD.load(Ordering::SeqCst) > 0;
    if let Some((_, frames)) = paused_at.as_mut() {
        *frames += 1;
    }
    match *paused_at {
        None if held => {
            time.pause();
            *paused_at = Some((std::time::Instant::now(), 0));
        }
        Some((_, frames)) if hold_ends(frames, held) => {
            time.unpause();
            *paused_at = None;
        }
        Some((t, _)) if t.elapsed() >= 2 * BLANK_WAIT => {
            error!(
                "a held screenshot has had no frame back for {:.1} s; exiting 1",
                t.elapsed().as_secs_f32()
            );
            std::process::exit(1);
        }
        _ => {}
    }
}

/// Captures an offscreen image target (photo mode's export) to a PNG stored like a screenshot,
/// then reports the PNG's pixel size or the error to `done`.
pub fn request_image(
    commands: &mut Commands,
    name: &str,
    image: Handle<Image>,
    done: impl Fn(Result<UVec2, String>) + Send + Sync + 'static,
) {
    expect_shot(name);
    PENDING.fetch_add(1, Ordering::SeqCst);
    let name = name.to_string();
    commands
        .spawn(Screenshot::image(image))
        .observe(move |shot: On<ScreenshotCaptured>| {
            let size = UVec2::new(shot.image.width(), shot.image.height());
            done(store(&name, encode_png(shot.image.clone())).map(|()| size));
            PENDING.fetch_sub(1, Ordering::SeqCst);
        });
}

#[cfg(not(target_arch = "wasm32"))]
fn store(name: &str, png: Result<Vec<u8>, String>) -> Result<(), String> {
    let path = format!("{SHOT_DIR}/{name}.png");
    let res = png.and_then(|bytes| {
        std::fs::create_dir_all(SHOT_DIR)
            .and_then(|_| std::fs::write(&path, bytes))
            .map_err(|e| e.to_string())
    });
    match &res {
        Ok(()) => info!("wrote {path}"),
        Err(e) => error!("screenshot {name} failed: {e}"),
    }
    res
}

#[cfg(target_arch = "wasm32")]
fn store(name: &str, png: Result<Vec<u8>, String>) -> Result<(), String> {
    match png {
        Ok(bytes) => {
            let url = format!("data:image/png;base64,{}", base64(&bytes));
            let mut shots = SHOTS.lock().unwrap();
            shots.retain(|(n, _)| n != name);
            shots.push((name.to_string(), url));
            info!("captured screenshot {name}");
            Ok(())
        }
        Err(e) => {
            error!("screenshot {name} failed: {e}");
            Err(e)
        }
    }
}

/// Encodes a captured frame as an opaque RGB PNG.
pub fn encode_png(img: Image) -> Result<Vec<u8>, String> {
    let (w, h) = (img.width(), img.height());
    let format = img.texture_descriptor.format;
    let dynamic = if format == TextureFormat::Rgba8Unorm {
        let data = img.data.ok_or("uninitialized screenshot image")?;
        DynamicImage::ImageRgba8(
            RgbaImage::from_raw(w, h, data).ok_or("screenshot data size mismatch")?,
        )
    } else {
        img.try_into_dynamic()
            .map_err(|e| format!("cannot convert {format:?}: {e}"))?
    };
    let mut out = Vec::new();
    dynamic
        .to_rgb8()
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(any(target_arch = "wasm32", test))]
fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension};

    fn image(format: TextureFormat, px: &[u8]) -> Image {
        Image::new_fill(
            Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            px,
            format,
            RenderAssetUsages::default(),
        )
    }

    #[test]
    fn a_held_black_frame_is_retaken_until_the_wait_runs_out() {
        use std::time::Duration;
        assert_eq!(verdict(false, Duration::ZERO), Verdict::Store);
        assert_eq!(
            verdict(false, BLANK_WAIT * 2),
            Verdict::Store,
            "a frame that draws is stored however late"
        );
        assert_eq!(verdict(true, Duration::ZERO), Verdict::Retake);
        assert_eq!(verdict(true, BLANK_WAIT / 2), Verdict::Retake);
        assert_eq!(verdict(true, BLANK_WAIT), Verdict::Fail);
    }

    /// A hold whose capture lands `lands_after` frames after the request: the frames that passed no
    /// simulated time and were `holding`, and the frame count from the request to the first frame
    /// that advanced time again.
    fn hold_for(lands_after: u32) -> (u32, u32) {
        use bevy::time::{TimePlugin, TimeUpdateStrategy, Virtual};
        let step = std::time::Duration::from_nanos(16_666_667);
        let mut app = App::new();
        app.add_plugins(TimePlugin)
            .insert_resource(TimeUpdateStrategy::ManualDuration(step))
            .add_systems(Update, hold_time);
        app.update();
        HELD.fetch_add(1, Ordering::SeqCst);
        app.update();
        let elapsed = app.world().resource::<Time<Virtual>>().elapsed();
        let (mut still, mut frames) = (0, 0);
        loop {
            frames += 1;
            if frames == lands_after {
                HELD.fetch_sub(1, Ordering::SeqCst);
            }
            app.update();
            let time = app.world().resource::<Time<Virtual>>();
            if time.elapsed() != elapsed {
                assert_eq!(time.elapsed(), elapsed + step);
                assert!(!holding(time));
                return (still, frames);
            }
            assert!(
                holding(time),
                "a frame that passes no time holds the script"
            );
            still += 1;
        }
    }

    /// The hold pauses virtual time from the frame after the request for a whole number of
    /// `HOLD_FRAMES`, however many frames the readback takes: `FrameCount` then keeps the same
    /// phase against simulated time after every capture, so a capture run's later frames do not
    /// depend on readback latency.
    #[test]
    fn a_held_capture_pauses_virtual_time_for_whole_hold_frames() {
        assert_eq!(hold_for(1), (HOLD_FRAMES, HOLD_FRAMES + 1));
        assert_eq!(hold_for(2), hold_for(1), "one frame later readback");
        assert_eq!(hold_for(5), hold_for(1));
        assert_eq!(hold_for(HOLD_FRAMES), hold_for(1));
        assert_eq!(
            hold_for(HOLD_FRAMES + 3),
            (2 * HOLD_FRAMES, 2 * HOLD_FRAMES + 1),
            "a readback past one quantum holds the next whole one"
        );
        assert!(
            !hold_ends(HOLD_FRAMES, true),
            "never while a capture is out"
        );
    }

    #[test]
    fn base64_padding() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b""), "");
    }

    #[test]
    fn encodes_bgra_swapchain_as_rgb_png() {
        let png = encode_png(image(TextureFormat::Bgra8UnormSrgb, &[10, 20, 200, 255])).unwrap();
        let back = image::load_from_memory(&png).unwrap().to_rgb8();
        assert_eq!(back.dimensions(), (2, 2));
        assert_eq!(back.get_pixel(1, 1).0, [200, 20, 10]);
    }

    #[test]
    fn encodes_rgba8_unorm() {
        let png = encode_png(image(TextureFormat::Rgba8Unorm, &[1, 2, 3, 0])).unwrap();
        let back = image::load_from_memory(&png).unwrap().to_rgb8();
        assert_eq!(back.get_pixel(0, 0).0, [1, 2, 3]);
    }

    #[test]
    fn unsupported_format_is_error() {
        let e = encode_png(image(TextureFormat::Rg8Snorm, &[0; 2])).unwrap_err();
        assert!(e.contains("Rg8Snorm"), "{e}");
    }
}
