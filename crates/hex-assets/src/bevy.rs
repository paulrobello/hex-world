//! `kit://` asset source: reads `assets/kit`, rewriting meshopt GLBs to plain GLBs before
//! `bevy_gltf` parses them. Every other file passes through unchanged. On the web the default
//! source (the `shaders/` the materials and the boid compute pass load) is read through the same
//! retrying reader.
//!
//! Every kit file (pack pieces and maps, the boid and car pieces, the IBL) and, on the web, every
//! default-source file is read here, so this is where a dropped request is retried: a transport
//! failure (`Io`, which is how a reset connection reaches a web fetch, or `HttpError`) is re-read
//! after each of [`RETRY_DELAYS`], before the asset server sees anything. A load only fails once
//! the last retry has. `NotFound` (403 and 404 on the web, a missing file natively) and a GLB that
//! does not rewrite are final at once. Every final failure logs an error naming the file and lands
//! in [`AssetLoadFailures`].
//!
//! Web fetches are cache-busted: `scripts/web/version-dist.mjs` inlines a content hash for every
//! file in `dist/` into `index.html` as `window.__HEX_HASHES`, and each fetch URL gets
//! `?v=<hash>` ([`versioned`]). Asset paths, the kit manifest's paths and `kit://` identities stay
//! unversioned. Native builds read files from disk, so they keep Bevy's plain file reader.
use bevy::{
    asset::io::{
        AssetReader, AssetReaderError, AssetSourceBuilder, ErasedAssetReader, PathStream, Reader,
        VecReader,
    },
    prelude::*,
};
use std::{
    borrow::Cow,
    collections::{BTreeSet, HashMap},
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

/// Files that failed for good, sorted (`hexWorld.state().assets.failed`): kit files as
/// `kit://<path>`, default-source files (web only) by their asset path, such as
/// `shaders/boids_compute.wgsl`.
#[derive(Resource, Clone, Default)]
pub struct AssetLoadFailures(Arc<Mutex<BTreeSet<String>>>);

impl AssetLoadFailures {
    pub fn list(&self) -> Vec<String> {
        self.0
            .lock()
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn record(&self, label: String) {
        if let Ok(mut s) = self.0.lock() {
            s.insert(label);
        }
    }
}

/// Registers the `kit` asset source and, on the web, the default source's retrying, versioned
/// reader. Add it before `DefaultPlugins`: asset sources must exist before `AssetPlugin` builds
/// the server, and a default source registered first replaces the platform one.
pub struct MeshoptGlbPlugin;

impl Plugin for MeshoptGlbPlugin {
    fn build(&self, app: &mut App) {
        register_kit_source(app, kit_root());
        #[cfg(target_arch = "wasm32")]
        {
            let failures = app.world().resource::<AssetLoadFailures>().clone();
            app.register_asset_source(
                bevy::asset::io::AssetSourceId::Default,
                AssetSourceBuilder::new(move || {
                    Box::new(RetryingReader {
                        inner: platform_reader("assets"),
                        delays: &RETRY_DELAYS,
                        failures: failures.clone(),
                        prefix: "",
                        rewrite_glbs: false,
                    })
                }),
            );
        }
    }
}

/// Registers the `kit` source at `root` (tests pass an absolute fixture or repo path), sharing the
/// app's [`AssetLoadFailures`] (inserted when missing).
pub fn register_kit_source(app: &mut App, root: String) {
    app.init_resource::<AssetLoadFailures>();
    let failures = app.world().resource::<AssetLoadFailures>().clone();
    app.register_asset_source(
        "kit",
        AssetSourceBuilder::new(move || {
            Box::new(RetryingReader {
                inner: platform_reader(&root),
                delays: &RETRY_DELAYS,
                failures: failures.clone(),
                prefix: "kit://",
                rewrite_glbs: true,
            })
        }),
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn platform_reader(root: &str) -> Box<dyn ErasedAssetReader> {
    bevy::asset::io::AssetSource::get_default_reader(root.to_string())()
}

/// Bevy's fetch reader with each request URL versioned from the page's hash map.
#[cfg(target_arch = "wasm32")]
fn platform_reader(root: &str) -> Box<dyn ErasedAssetReader> {
    Box::new(
        bevy::asset::io::wasm::HttpWasmAssetReader::new(root)
            .with_request_mapper(|url| versioned(url, page_hashes())),
    )
}

/// `window.__HEX_HASHES` (dist-relative path to content hash), read once; `None` when the page
/// has no map (a build that skipped `scripts/web/version-dist.mjs`).
#[cfg(target_arch = "wasm32")]
fn page_hashes() -> Option<&'static HashMap<String, String>> {
    use wasm_bindgen::JsCast;
    static HASHES: std::sync::OnceLock<Option<HashMap<String, String>>> =
        std::sync::OnceLock::new();
    HASHES
        .get_or_init(|| {
            let v = js_sys::Reflect::get(
                &js_sys::global(),
                &wasm_bindgen::JsValue::from_str("__HEX_HASHES"),
            )
            .ok()?;
            if !v.is_object() {
                return None;
            }
            let entries = js_sys::Object::entries(v.unchecked_ref());
            Some(
                entries
                    .iter()
                    .filter_map(|e| {
                        let pair: js_sys::Array = e.unchecked_into();
                        Some((pair.get(0).as_string()?, pair.get(1).as_string()?))
                    })
                    .collect(),
            )
        })
        .as_ref()
}

/// The URL to fetch for the dist-relative `path`: `path?v=<hash>` when `hashes` has it, else
/// `path` itself, logged once per path as a warning (once in all when there is no map), so a file
/// fetched without a version shows in the console; web-smoke fails on any such request.
pub fn versioned<'a>(path: &'a str, hashes: Option<&HashMap<String, String>>) -> Cow<'a, str> {
    static WARNED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
    let key = path.trim_start_matches("./").trim_start_matches('/');
    let Some(hashes) = hashes else {
        if WARNED.lock().is_ok_and(|mut w| w.insert(String::new())) {
            warn!("assets: no window.__HEX_HASHES in the page; fetching every asset unversioned");
        }
        return Cow::Borrowed(path);
    };
    match hashes.get(key) {
        Some(hash) => Cow::Owned(format!("{path}?v={hash}")),
        None => {
            if WARNED.lock().is_ok_and(|mut w| w.insert(key.to_string())) {
                warn!(
                    "assets: {key} has no content hash in window.__HEX_HASHES; fetched unversioned"
                );
            }
            Cow::Borrowed(path)
        }
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

struct RetryingReader {
    inner: Box<dyn ErasedAssetReader>,
    delays: &'static [Duration],
    failures: AssetLoadFailures,
    /// Names the source in logs and [`AssetLoadFailures`]: `kit://`, or empty for the default
    /// source, whose paths are already the asset paths (`shaders/water.wesl`).
    prefix: &'static str,
    /// Rewrite meshopt GLBs to plain GLBs (the kit source).
    rewrite_glbs: bool,
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

impl RetryingReader {
    fn label(&self, path: &Path) -> String {
        format!("{}{}", self.prefix, path.display())
    }

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
                        "assets: {} did not load ({e}); retry {retry}/{} in {} ms",
                        self.label(path),
                        self.delays.len(),
                        d.as_millis()
                    );
                    sleep(d).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn read_file<'a>(
        &'a self,
        path: &'a Path,
    ) -> Result<Box<dyn Reader + 'a>, AssetReaderError> {
        if !self.rewrite_glbs || path.extension().and_then(|e| e.to_str()) != Some("glb") {
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

impl AssetReader for RetryingReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<Box<dyn Reader + 'a>, AssetReaderError> {
        let out = self.read_file(path).await;
        if let Err(e) = &out {
            let label = self.label(path);
            error!("assets: {label} failed to load, giving up: {e}");
            self.failures.record(label);
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

    /// Reads `path` through a retrying reader with `prefix`: (succeeded, inner reads, failures).
    fn read(
        prefix: &'static str,
        path: &str,
        fails: u32,
        error: fn() -> AssetReaderError,
    ) -> (bool, u32, Vec<String>) {
        let reads = Arc::new(AtomicU32::new(0));
        let reader = RetryingReader {
            inner: Box::new(Flaky {
                fails,
                error,
                reads: reads.clone(),
            }),
            delays: &SHORT_WAIT,
            failures: AssetLoadFailures::default(),
            prefix,
            rewrite_glbs: prefix == "kit://",
        };
        let ok = bevy::tasks::block_on(async {
            match AssetReader::read(&reader, Path::new(path)).await {
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

    fn run(fails: u32, error: fn() -> AssetReaderError) -> (bool, u32, Vec<String>) {
        read("kit://", "p1/x/x.png", fails, error)
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

    #[test]
    fn a_default_source_file_retries_and_is_named_by_its_asset_path() {
        let shader = "shaders/boids_compute.wgsl";
        assert_eq!(read("", shader, 1, reset), (true, 2, vec![]));
        assert_eq!(
            read("", shader, u32::MAX, reset),
            (false, 4, vec![shader.to_string()])
        );
    }

    #[test]
    fn a_fetch_url_carries_its_files_content_hash() {
        let hashes: HashMap<String, String> = [
            ("assets/kit/p1/a/a.glb", "0123456789"),
            ("assets/shaders/water.wesl", "abcdef0123"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        assert_eq!(
            versioned("assets/kit/p1/a/a.glb", Some(&hashes)),
            "assets/kit/p1/a/a.glb?v=0123456789"
        );
        assert_eq!(
            versioned("/assets/shaders/water.wesl", Some(&hashes)),
            "/assets/shaders/water.wesl?v=abcdef0123"
        );
        // A file missing from the map, or a page without one, is fetched unversioned.
        assert_eq!(
            versioned("assets/kit/p1/b/b.glb", Some(&hashes)),
            "assets/kit/p1/b/b.glb"
        );
        assert_eq!(
            versioned("assets/kit/p1/a/a.glb", None),
            "assets/kit/p1/a/a.glb"
        );
    }
}
