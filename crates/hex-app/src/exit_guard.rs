//! Native exit guard: ends the process right after the last exit write, and a watchdog that ends
//! it when it still has not exited.
//!
//! Bevy's own teardown after `AppExit` (winit `exiting`, then `World::clear_all`) drops
//! `RenderAppChannels`, which waits for the render thread's frame. At high host load that wait has
//! hung for good at 0% CPU with every output already written (a `shots-scatter` run hung 50 min).
//! The process writes all its outputs in `Last` on the `AppExit` frame, so [`exit_after_writes`]
//! ends it there with `AppExit`'s code instead of tearing down. `HEXW_EXIT_TEARDOWN=bevy` keeps
//! Bevy's teardown.
//!
//! The watchdog arms on the first `ExitRequest` or `AppExit`. When the process is still alive
//! `HEXW_EXIT_WATCHDOG_SECS` (default 20) later, it samples every thread's stack to
//! `target/exit-hang-<pid>.txt` and exits by [`verdict`]: 0 with a warning when every expected
//! output is on disk, [`WATCHDOG_EXIT`] naming the missing ones otherwise.
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use bevy::prelude::*;

use crate::control::ExitRequest;

const TEARDOWN_ENV: &str = "HEXW_EXIT_TEARDOWN";
const WATCHDOG_ENV: &str = "HEXW_EXIT_WATCHDOG_SECS";
const RENDER_DELAY_ENV: &str = "HEXW_EXIT_RENDER_DELAY_MS";
const DEFAULT_WATCHDOG: Duration = Duration::from_secs(20);
pub const WATCHDOG_EXIT: i32 = 3;
const SAMPLE_SECS: &str = "2";
/// `sample` symbolicates after sampling, which is slow under load; it is killed past this.
const SAMPLE_LIMIT: Duration = Duration::from_secs(30);

/// Files this run promises to write, recorded when each write is requested.
static EXPECTED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static ARMED: AtomicBool = AtomicBool::new(false);
/// Set when `AppExit` is read; the render-world delay hook waits for it.
static EXIT_SIGNALLED: AtomicBool = AtomicBool::new(false);
static STARTED: std::sync::OnceLock<SystemTime> = std::sync::OnceLock::new();

/// Records the process start (an expected file must be newer) and the exit-time outputs.
pub fn init(dump_state: Option<&str>, save: Option<&str>) {
    STARTED.get_or_init(SystemTime::now);
    for path in [dump_state, save].into_iter().flatten() {
        expect(path);
    }
}

/// Records a file this run will write; the watchdog's verdict checks it is on disk.
pub fn expect(path: impl Into<PathBuf>) {
    let path = path.into();
    let mut expected = EXPECTED.lock().unwrap_or_else(|p| p.into_inner());
    if !expected.contains(&path) {
        expected.push(path);
    }
}

fn expected() -> Vec<PathBuf> {
    EXPECTED.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// The watchdog's exit code and message after waiting `waited`: 0 when every expected file is a
/// non-empty file written since `since`, [`WATCHDOG_EXIT`] naming the missing ones otherwise.
pub fn verdict(expected: &[PathBuf], since: SystemTime, waited: Duration) -> (i32, String) {
    let missing: Vec<&PathBuf> = expected
        .iter()
        .filter(|p| {
            !std::fs::metadata(p)
                .is_ok_and(|m| m.is_file() && m.len() > 0 && m.modified().is_ok_and(|t| t >= since))
        })
        .collect();
    let secs = waited.as_secs();
    if missing.is_empty() {
        return (
            0,
            format!(
                "hex-world: exit watchdog: WARNING: the app did not exit within {secs} s of the \
                 exit request; all {} expected files are verified on disk, so exiting 0",
                expected.len()
            ),
        );
    }
    let names: Vec<String> = missing
        .iter()
        .take(5)
        .map(|p| p.display().to_string())
        .collect();
    (
        WATCHDOG_EXIT,
        format!(
            "hex-world: exit watchdog: the app did not exit within {secs} s of the exit request \
             and {} of {} expected files are missing ({}{}); exiting with {WATCHDOG_EXIT}",
            missing.len(),
            expected.len(),
            names.join(", "),
            if missing.len() > names.len() {
                ", ..."
            } else {
                ""
            }
        ),
    )
}

fn watchdog_wait() -> Duration {
    std::env::var(WATCHDOG_ENV)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map_or(DEFAULT_WATCHDOG, Duration::from_secs)
}

/// Starts the exit watchdog once; later calls do nothing. A thread, not a system: a hung
/// teardown never runs another frame.
pub fn arm() {
    if ARMED.swap(true, Ordering::SeqCst) {
        return;
    }
    let wait = watchdog_wait();
    let since = *STARTED.get_or_init(SystemTime::now);
    std::thread::spawn(move || {
        std::thread::sleep(wait);
        // `sample` symbolicates for up to SAMPLE_LIMIT under load, so say the watchdog fired first.
        eprintln!(
            "hex-world: exit watchdog: still running {} s after the exit request; sampling stacks",
            wait.as_secs()
        );
        flush();
        let sample = sample_stacks();
        let (code, msg) = verdict(&expected(), since, wait);
        eprintln!("{msg} ({sample})");
        hard_exit(code);
    });
}

/// Runs `/usr/bin/sample` on this process into `target/exit-hang-<pid>.txt`, bounded by
/// [`SAMPLE_LIMIT`], and says what happened.
fn sample_stacks() -> String {
    let pid = std::process::id();
    let out = format!("target/exit-hang-{pid}.txt");
    let sample = std::path::Path::new("/usr/bin/sample");
    if !sample.exists() {
        return "no /usr/bin/sample on this host".into();
    }
    let _ = std::fs::create_dir_all("target");
    let child = std::process::Command::new(sample)
        .args([&pid.to_string(), SAMPLE_SECS, "-file", &out])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return format!("sample failed to start: {e}"),
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let written = std::fs::metadata(&out).is_ok_and(|m| m.len() > 0);
                return if written {
                    format!("stacks sampled to {out}")
                } else {
                    format!("sample exited {status} without writing {out}")
                };
            }
            Ok(None) if start.elapsed() < SAMPLE_LIMIT => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Ok(None) => {
                let _ = child.kill();
                return format!("sample killed after {} s", SAMPLE_LIMIT.as_secs());
            }
            Err(e) => return format!("sample wait failed: {e}"),
        }
    }
}

fn flush() {
    use std::io::Write;
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
}

/// Ends the process from the watchdog thread without running `atexit` handlers: the main thread
/// may already be inside `exit()`, and a second `exit()` is undefined and can block on its lock.
fn hard_exit(code: i32) -> ! {
    flush();
    // SAFETY: `_exit` only ends the process; nothing in this process runs after it.
    unsafe { libc::_exit(code) }
}

fn bevy_teardown() -> bool {
    std::env::var(TEARDOWN_ENV).is_ok_and(|v| v == "bevy")
}

/// The exit code for this frame's `AppExit` messages, the first error winning as in
/// `App::should_exit`.
fn exit_code<'a>(exits: impl Iterator<Item = &'a AppExit>) -> Option<i32> {
    let mut code = None;
    for e in exits {
        match e {
            AppExit::Error(n) => return Some(i32::from(n.get())),
            AppExit::Success => code = Some(0),
        }
    }
    code
}

/// Arms the watchdog on the first `ExitRequest` (pending screenshots may still hold `AppExit`).
pub fn arm_on_request(exit: Res<ExitRequest>) {
    if exit.0 {
        arm();
    }
}

/// In `Last` after `OnAppExitSystems` (`--dump-state`, `--save`): on `AppExit`, signals the
/// render-delay hook, arms the watchdog, then ends the process with `AppExit`'s code unless
/// `HEXW_EXIT_TEARDOWN=bevy`.
pub fn exit_after_writes(mut exits: MessageReader<AppExit>) {
    let Some(code) = exit_code(exits.read()) else {
        return;
    };
    EXIT_SIGNALLED.store(true, Ordering::SeqCst);
    arm();
    if bevy_teardown() {
        info!("exit: {TEARDOWN_ENV}=bevy: Bevy teardown, code {code}");
        return;
    }
    info!("exit: outputs written, ending the process with code {code}");
    flush();
    std::process::exit(code);
}

/// Test hook: the render world sleeps this long before `prepare_windows` once exit is signalled,
/// so the main thread's teardown runs while the render thread has yet to acquire its frame.
#[derive(Resource)]
struct RenderDelay(Duration);

fn delay_render_on_exit(delay: Res<RenderDelay>, mut done: Local<bool>) {
    if *done || !EXIT_SIGNALLED.load(Ordering::SeqCst) {
        return;
    }
    *done = true;
    warn!(
        "exit: {RENDER_DELAY_ENV}: render thread sleeping {} ms before prepare_windows",
        delay.0.as_millis()
    );
    std::thread::sleep(delay.0);
    warn!("exit: render thread resumes");
}

/// Adds the render-world delay hook when `HEXW_EXIT_RENDER_DELAY_MS` is set.
pub fn add_render_delay(app: &mut App) {
    let Some(ms) = std::env::var(RENDER_DELAY_ENV)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    else {
        return;
    };
    use bevy::render::{Render, RenderApp, view::prepare_windows};
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app
            .insert_resource(RenderDelay(Duration::from_millis(ms)))
            .add_systems(Render, delay_render_on_exit.before(prepare_windows));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_passes_only_when_every_file_is_on_disk_and_new() {
        let dir = std::env::temp_dir().join(format!("hexw-exit-guard-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let since = SystemTime::now() - Duration::from_secs(1);
        let wait = Duration::from_secs(20);
        let expected: Vec<PathBuf> = ["a-r0.png", "state.json", "w.hexworld"]
            .iter()
            .map(|n| dir.join(n))
            .collect();
        for p in &expected {
            std::fs::write(p, b"data").unwrap();
        }
        let (code, msg) = verdict(&expected, since, wait);
        assert_eq!(code, 0, "{msg}");
        assert!(
            msg.contains("WARNING") && msg.contains("all 3 expected files"),
            "{msg}"
        );

        std::fs::remove_file(&expected[1]).unwrap();
        let (code, msg) = verdict(&expected, since, wait);
        assert_eq!(code, WATCHDOG_EXIT, "{msg}");
        assert!(
            msg.contains("1 of 3") && msg.contains("state.json"),
            "{msg}"
        );

        std::fs::write(&expected[1], b"").unwrap();
        let (code, msg) = verdict(&expected, since, wait);
        assert_eq!(code, WATCHDOG_EXIT, "an empty file is not written: {msg}");

        std::fs::write(&expected[1], b"data").unwrap();
        let later = SystemTime::now() + Duration::from_secs(60);
        let (code, msg) = verdict(&expected, later, wait);
        assert_eq!(
            code, WATCHDOG_EXIT,
            "files older than the run are stale: {msg}"
        );
        assert!(msg.contains("3 of 3"), "{msg}");

        let (code, msg) = verdict(&[], since, wait);
        assert_eq!(code, 0, "no expected files: {msg}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn verdict_names_at_most_five_missing_files() {
        let missing: Vec<PathBuf> = (0..7)
            .map(|i| PathBuf::from(format!("/nonexistent/hexw-{i}.png")))
            .collect();
        let (code, msg) = verdict(&missing, SystemTime::now(), Duration::from_secs(20));
        assert_eq!(code, WATCHDOG_EXIT);
        assert!(
            msg.contains("7 of 7") && msg.contains("hexw-4.png"),
            "{msg}"
        );
        assert!(
            !msg.contains("hexw-5.png") && msg.contains(", ..."),
            "{msg}"
        );
    }

    // The registry is process-global and tests run in parallel, so these use names no other test
    // records and check membership rather than the whole list.
    #[test]
    fn registry_records_each_path_once() {
        let a = PathBuf::from("target/shots/exit-guard-test-a.png");
        let b = PathBuf::from("target/exit-guard-test-b.json");
        expect(a.clone());
        expect(a.clone());
        init(Some("target/exit-guard-test-b.json"), None);
        let all = expected();
        assert_eq!(all.iter().filter(|p| **p == a).count(), 1);
        assert_eq!(all.iter().filter(|p| **p == b).count(), 1);
    }

    #[test]
    fn capture_requests_record_their_png() {
        crate::capture::expect_shot("exit-guard-test-shot");
        assert!(expected().contains(&PathBuf::from("target/shots/exit-guard-test-shot.png")));
    }

    #[test]
    fn the_first_error_sets_the_exit_code() {
        let err = |n: u8| AppExit::Error(std::num::NonZeroU8::new(n).unwrap());
        assert_eq!(exit_code([].iter()), None);
        assert_eq!(exit_code([AppExit::Success].iter()), Some(0));
        assert_eq!(exit_code([AppExit::Success, err(4)].iter()), Some(4));
        assert_eq!(exit_code([err(2), err(4)].iter()), Some(2));
    }
}
