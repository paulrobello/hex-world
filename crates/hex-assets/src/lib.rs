//! Hex World asset loading: meshopt GLB rewrite, texture-family selection, kit manifest.
#[cfg(feature = "bevy")]
pub mod bevy;
mod kit_manifest;
mod ktx2_header;
mod meshopt_glb;
mod texture_family;

pub use kit_manifest::*;
pub use ktx2_header::*;
pub use meshopt_glb::{GlbError, rewrite_glb};
pub use texture_family::{TextureFamily, family_suffix, select_family};
