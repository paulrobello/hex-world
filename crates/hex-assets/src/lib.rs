//! Hex World asset loading: meshopt GLB rewrite and texture-family selection.
#[cfg(feature = "bevy")]
pub mod bevy;
mod meshopt_glb;
mod texture_family;

pub use meshopt_glb::{GlbError, rewrite_glb};
pub use texture_family::{TextureFamily, family_suffix, select_family};
