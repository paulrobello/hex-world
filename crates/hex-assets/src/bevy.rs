//! `kit://` asset source: reads `assets/kit`, rewriting meshopt GLBs to plain GLBs before
//! `bevy_gltf` parses them. Every other file passes through unchanged.
use bevy::{
    asset::io::{
        AssetReader, AssetReaderError, AssetSource, AssetSourceBuilder, ErasedAssetReader,
        PathStream, Reader, VecReader,
    },
    prelude::*,
};
use std::{borrow::Cow, path::Path, sync::Arc};

use crate::rewrite_glb;

/// Registers the `kit` asset source. Add it before `DefaultPlugins` (asset sources must exist
/// before `AssetPlugin` builds the server).
pub struct MeshoptGlbPlugin;

impl Plugin for MeshoptGlbPlugin {
    fn build(&self, app: &mut App) {
        let root = kit_root();
        app.register_asset_source(
            "kit",
            AssetSourceBuilder::new(move || {
                Box::new(MeshoptGlbReader(AssetSource::get_default_reader(
                    root.clone(),
                )()))
            }),
        );
    }
}

/// Natively, `cargo run` resolves relative asset paths against the crate dir, so the kit root is
/// made absolute from the working directory (matching hex-app's `AssetPlugin::file_path`). On
/// the web, `assets/kit` is fetched relative to the page.
fn kit_root() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(dir) = std::env::current_dir().map(|d| d.join("assets").join("kit"))
        && dir.is_dir()
    {
        return dir.to_string_lossy().into_owned();
    }
    "assets/kit".to_string()
}

struct MeshoptGlbReader(Box<dyn ErasedAssetReader>);

impl AssetReader for MeshoptGlbReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<Box<dyn Reader + 'a>, AssetReaderError> {
        let mut inner = self.0.read(path).await?;
        if path.extension().and_then(|e| e.to_str()) != Some("glb") {
            return Ok(inner);
        }
        let mut bytes = Vec::new();
        inner
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| AssetReaderError::Io(Arc::new(e)))?;
        let out = match rewrite_glb(&bytes) {
            Ok(Cow::Borrowed(_)) => bytes,
            Ok(Cow::Owned(plain)) => plain,
            Err(e) => {
                return Err(AssetReaderError::Io(Arc::new(std::io::Error::other(
                    format!("{}: {e}", path.display()),
                ))));
            }
        };
        Ok(Box::new(VecReader::new(out)))
    }

    async fn read_meta<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<dyn Reader + 'a>, AssetReaderError> {
        self.0.read_meta(path).await
    }

    async fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<PathStream>, AssetReaderError> {
        self.0.read_directory(path).await
    }

    async fn is_directory<'a>(&'a self, path: &'a Path) -> Result<bool, AssetReaderError> {
        self.0.is_directory(path).await
    }
}
