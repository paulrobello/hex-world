//! In-app screenshots to PNG: a file under `target/shots` natively, a data URL on the web.
use std::io::Cursor;
use std::sync::atomic::{AtomicU32, Ordering};

use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use image::{DynamicImage, ImageFormat, RgbaImage};

#[cfg(not(target_arch = "wasm32"))]
pub const SHOT_DIR: &str = "target/shots";

static PENDING: AtomicU32 = AtomicU32::new(0);

/// Finished web captures keyed by name, taken by `hex_world_take_shot`.
#[cfg(target_arch = "wasm32")]
pub static SHOTS: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

/// Screenshots requested but not yet encoded; exit waits for this to reach zero.
pub fn pending() -> u32 {
    PENDING.load(Ordering::SeqCst)
}

pub fn request(commands: &mut Commands, name: &str) {
    PENDING.fetch_add(1, Ordering::SeqCst);
    let name = name.to_string();
    commands
        .spawn(Screenshot::primary_window())
        .observe(move |shot: On<ScreenshotCaptured>| {
            let _ = store(&name, encode_png(shot.image.clone()));
            PENDING.fetch_sub(1, Ordering::SeqCst);
        });
}

/// Captures an offscreen image target (photo mode's export) to a PNG stored like a screenshot,
/// then reports the PNG's pixel size or the error to `done`.
pub fn request_image(
    commands: &mut Commands,
    name: &str,
    image: Handle<Image>,
    done: impl Fn(Result<UVec2, String>) + Send + Sync + 'static,
) {
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
