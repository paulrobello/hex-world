//! `kit://` asset source: reads `assets/kit`, rewriting meshopt GLBs to plain GLBs before
//! `bevy_gltf` parses them. Every other file passes through unchanged.
//!
//! Every kit file (pack pieces and maps, the boid and car pieces, the IBL) is read here, so this is
//! where a dropped request is retried: a transport failure (`Io`, which is how a reset connection
//! reaches a web fetch, or `HttpError`) is re-read after each of [`RETRY_DELAYS`], before the asset
//! server sees anything. A load only fails once the last retry has. `NotFound` (403 and 404 on the
//! web, a missing file natively) and a GLB that does not rewrite are final at once. Every final
//! failure logs an error naming the file and lands in [`KitLoadFailures`].
use bevy::{
    asset::io::{
        AssetReader, AssetReaderError, AssetSource, AssetSourceBuilder, ErasedAssetReader,
        PathStream, Reader, VecReader,
    },
    prelude::*,
};
use std::{
    borrow::Cow,
    collections::BTreeSet,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::rewrite_glb;

/// Waits before each retry of a failed read: the first request plus three retries, about 3.5 s
/// before a load gives up.
pub const RETRY_DELAYS: [Duration; 3] = [
    Duration::from_millis(500),
    Duration::from_millis(1000),
    Duration::from_millis(2000),
];

/// Kit files that failed for good, as `kit://<path>`, sorted (`hexWorld.state().assets.failed`).
#[derive(Resource, Clone, Default)]
pub struct KitLoadFailures(Arc<Mutex<BTreeSet<String>>>);

impl KitLoadFailures {
    pub fn list(&self) -> Vec<String> {
        self.0
            .lock()
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn record(&self, path: &Path) {
        if let Ok(mut s) = self.0.lock() {
            s.insert(format!("kit://{}", path.display()));
        }
    }
}

/// Registers the `kit` asset source. Add it before `DefaultPlugins` (asset sources must exist
/// before `AssetPlugin` builds the server).
pub struct MeshoptGlbPlugin;

impl Plugin for MeshoptGlbPlugin {
    fn build(&self, app: &mut App) {
        register_kit_source(app, kit_root());
    }
}

/// Registers the `kit` source at `root` (tests pass an absolute fixture or repo path) and inserts
/// its [`KitLoadFailures`].
pub fn register_kit_source(app: &mut App, root: String) {
    let failures = KitLoadFailures::default();
    app.insert_resource(failures.clone());
    app.register_asset_source(
        "kit",
        AssetSourceBuilder::new(move || {
            Box::new(MeshoptGlbReader {
                inner: AssetSource::get_default_reader(root.clone())(),
                delays: &RETRY_DELAYS,
                failures: failures.clone(),
            })
        }),
    );
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

struct MeshoptGlbReader {
    inner: Box<dyn ErasedAssetReader>,
    delays: &'static [Duration],
    failures: KitLoadFailures,
}

fn transient(e: &AssetReaderError) -> bool {
    matches!(e, AssetReaderError::Io(_) | AssetReaderError::HttpError(_))
}

#[cfg(not(target_arch = "wasm32"))]
async fn sleep(d: Duration) {
    async_io::Timer::after(d).await;
}

#[cfg(target_arch = "wasm32")]
async fn sleep(d: Duration) {
    let ms = i32::try_from(d.as_millis()).unwrap_or(i32::MAX);
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let scheduled = web_sys::window().is_some_and(|w| {
            w.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
                .is_ok()
        });
        // Outside a window (a worker) the retry runs at once rather than never.
        if !scheduled {
            let _ = resolve.call0(&wasm_bindgen::JsValue::NULL);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

impl MeshoptGlbReader {
    /// Reads `path` (on the web the whole body arrives here), retrying transport failures.
    async fn read_retrying<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<dyn Reader + 'a>, AssetReaderError> {
        let mut retry = 0;
        loop {
            match self.inner.read(path).await {
                Ok(r) => return Ok(r),
                Err(e) if transient(&e) && retry < self.delays.len() => {
                    let d = self.delays[retry];
                    retry += 1;
                    warn!(
                        "kit: kit://{} did not load ({e}); retry {retry}/{} in {} ms",
                        path.display(),
                        self.delays.len(),
                        d.as_millis()
                    );
                    sleep(d).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn read_kit<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<dyn Reader + 'a>, AssetReaderError> {
        if path.extension().and_then(|e| e.to_str()) != Some("glb") {
            return self.read_retrying(path).await;
        }
        let mut inner = self.read_retrying(path).await?;
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
}

impl AssetReader for MeshoptGlbReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<Box<dyn Reader + 'a>, AssetReaderError> {
        let out = self.read_kit(path).await;
        if let Err(e) = &out {
            error!(
                "kit: kit://{} failed to load, giving up: {e}",
                path.display()
            );
            self.failures.record(path);
        }
        out
    }

    async fn read_meta<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<dyn Reader + 'a>, AssetReaderError> {
        self.inner.read_meta(path).await
    }

    async fn read_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<PathStream>, AssetReaderError> {
        self.inner.read_directory(path).await
    }

    async fn is_directory<'a>(&'a self, path: &'a Path) -> Result<bool, AssetReaderError> {
        self.inner.is_directory(path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Fails its first `fails` reads with `error`, then serves `b"ok"`.
    struct Flaky {
        fails: u32,
        error: fn() -> AssetReaderError,
        reads: Arc<AtomicU32>,
    }

    impl AssetReader for Flaky {
        async fn read<'a>(
            &'a self,
            _path: &'a Path,
        ) -> Result<Box<dyn Reader + 'a>, AssetReaderError> {
            let n = self.reads.fetch_add(1, Ordering::SeqCst);
            if n < self.fails {
                return Err((self.error)());
            }
            Ok(Box::new(VecReader::new(b"ok".to_vec())))
        }
        async fn read_meta<'a>(
            &'a self,
            path: &'a Path,
        ) -> Result<Box<dyn Reader + 'a>, AssetReaderError> {
            Err(AssetReaderError::NotFound(path.to_owned()))
        }
        async fn read_directory<'a>(
            &'a self,
            path: &'a Path,
        ) -> Result<Box<PathStream>, AssetReaderError> {
            Err(AssetReaderError::NotFound(path.to_owned()))
        }
        async fn is_directory<'a>(&'a self, _path: &'a Path) -> Result<bool, AssetReaderError> {
            Ok(false)
        }
    }

    // Non-zero, so each retry waits on a real timer.
    const SHORT_WAIT: [Duration; 3] = [Duration::from_millis(2); 3];

    fn reset() -> AssetReaderError {
        AssetReaderError::Io(Arc::new(std::io::Error::from(
            std::io::ErrorKind::ConnectionReset,
        )))
    }

    /// Reads `p1/x/x.png` through the retrying reader: (succeeded, inner reads, failures).
    fn run(fails: u32, error: fn() -> AssetReaderError) -> (bool, u32, Vec<String>) {
        let reads = Arc::new(AtomicU32::new(0));
        let reader = MeshoptGlbReader {
            inner: Box::new(Flaky {
                fails,
                error,
                reads: reads.clone(),
            }),
            delays: &SHORT_WAIT,
            failures: KitLoadFailures::default(),
        };
        let ok = bevy::tasks::block_on(async {
            match AssetReader::read(&reader, Path::new("p1/x/x.png")).await {
                Ok(mut r) => {
                    let mut b = vec![];
                    r.read_to_end(&mut b).await.unwrap();
                    b == b"ok"
                }
                Err(_) => false,
            }
        });
        (ok, reads.load(Ordering::SeqCst), reader.failures.list())
    }

    #[test]
    fn a_dropped_request_is_retried_until_it_loads() {
        assert_eq!(run(1, reset), (true, 2, vec![]));
        assert_eq!(run(3, reset), (true, 4, vec![]));
        assert_eq!(
            run(2, || AssetReaderError::HttpError(503)),
            (true, 3, vec![])
        );
    }

    #[test]
    fn a_file_that_keeps_failing_gives_up_after_the_last_retry() {
        let failed = vec!["kit://p1/x/x.png".to_string()];
        let t = std::time::Instant::now();
        assert_eq!(run(u32::MAX, reset), (false, 4, failed.clone()));
        assert!(t.elapsed() >= SHORT_WAIT.iter().sum::<Duration>());
        // A missing file is not retried.
        assert_eq!(
            run(u32::MAX, || AssetReaderError::NotFound("p1/x/x.png".into())),
            (false, 1, failed)
        );
        assert_eq!(RETRY_DELAYS.len(), 3);
    }
}
