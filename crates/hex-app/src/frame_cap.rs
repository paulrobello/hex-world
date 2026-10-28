//! Frame rate cap (owner A29): Off (the default), 30, 60 or 120 frames per second, from the
//! Settings sheet (`set_frame_cap`), a script, or the `frame_cap` key at start. Natively a sleep at
//! the end of the main schedule holds each frame to its deadline. On the web the main thread cannot
//! sleep, so the engine hands the cap to the page's animation-frame gate (`window.__frameCap`,
//! web/frame-latency.js), which holds every animation frame until the interval has elapsed.
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use bevy::prelude::*;

/// Natively, whether an unfocused window keeps the continuous update: set at start when the present
/// mode is uncapped (`--present immediate|novsync`) or the run is offscreen (`crate::offscreen`),
/// else Bevy's `WinitSettings::game()` steps an unfocused window at 60 Hz.
static UNFOCUSED_CONTINUOUS: AtomicBool = AtomicBool::new(cfg!(target_arch = "wasm32"));

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn set_unfocused_continuous(on: bool) {
    UNFOCUSED_CONTINUOUS.store(on, Ordering::Relaxed);
}

/// The caps the Settings sheet offers and the command accepts; `None` is Off.
pub const CHOICES: [u32; 3] = [30, 60, 120];

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameCap(pub Option<u32>);

impl FrameCap {
    /// A `set_frame_cap` value: `null` or 0 is Off, 30, 60 and 120 are caps, anything else is
    /// refused (`None`).
    pub fn from_fps(fps: Option<u32>) -> Option<Self> {
        match fps {
            None | Some(0) => Some(Self(None)),
            Some(n) if CHOICES.contains(&n) => Some(Self(Some(n))),
            Some(_) => None,
        }
    }

    /// The `frame_cap` key: `off` or `0`, or one of the caps.
    pub fn parse(v: &str) -> Option<Self> {
        if v.eq_ignore_ascii_case("off") {
            return Some(Self(None));
        }
        Self::from_fps(Some(v.trim().parse().ok()?))
    }

    /// The minimum time between frames; `None` when Off.
    pub fn interval(self) -> Option<Duration> {
        self.0
            .map(|fps| Duration::from_secs_f64(1.0 / f64::from(fps)))
    }

    /// `state().frame_cap`.
    pub fn json(self) -> serde_json::Value {
        let web = cfg!(target_arch = "wasm32");
        serde_json::json!({
            "fps": self.0,
            "interval_ms": self.interval().map(|d| (d.as_secs_f64() * 1e5).round() / 100.0),
            "limiter": if web { "raf" } else { "sleep" },
            "unfocused": if UNFOCUSED_CONTINUOUS.load(Ordering::Relaxed) { "continuous" } else { "reactive_60hz" },
        })
    }
}

/// One frame of the native limiter: how long to sleep now and the next frame's deadline. The
/// deadline advances by whole intervals, so sleep overshoot never accumulates. A frame that ends
/// a whole interval or more past its deadline (a stall, a scene rebuild) restarts the schedule
/// from now instead of letting the frames after it run back to back to catch up.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn pace<T>(now: T, deadline: Option<T>, interval: Duration) -> (Duration, T)
where
    T: Copy
        + PartialOrd
        + std::ops::Add<Duration, Output = T>
        + std::ops::Sub<T, Output = Duration>,
{
    match deadline {
        Some(d) if now < d => (d - now, d + interval),
        Some(d) if now - d < interval => (Duration::ZERO, d + interval),
        _ => (Duration::ZERO, now + interval),
    }
}

pub struct FrameCapPlugin(pub FrameCap);

impl Plugin for FrameCapPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0);
        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(Last, limit);
        #[cfg(target_arch = "wasm32")]
        app.add_systems(Last, push_to_page);
    }
}

/// Sleeps the main thread until this frame's deadline. Last in the frame, so the sleep spaces
/// whole updates (the render world's extraction follows the main world's update).
#[cfg(not(target_arch = "wasm32"))]
fn limit(cap: Res<FrameCap>, mut deadline: Local<Option<Instant>>) {
    let Some(interval) = cap.interval() else {
        *deadline = None;
        return;
    };
    let (sleep, next) = pace(Instant::now(), *deadline, interval);
    if !sleep.is_zero() {
        std::thread::sleep(sleep);
    }
    *deadline = Some(next);
}

#[cfg(target_arch = "wasm32")]
mod page {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        /// `window.__frameCap.set(fps)` (web/frame-latency.js); 0 is Off.
        #[wasm_bindgen(catch, js_namespace = __frameCap, js_name = set)]
        pub fn set_page_cap(fps: u32) -> Result<(), JsValue>;
    }
}

/// Hands each new cap to the page's animation-frame gate. A page without the gate (an embed that
/// left out web/frame-latency.js) logs once and runs uncapped.
#[cfg(target_arch = "wasm32")]
fn push_to_page(cap: Res<FrameCap>, mut sent: Local<Option<FrameCap>>, mut failed: Local<bool>) {
    if *sent == Some(*cap) || *failed {
        return;
    }
    match page::set_page_cap(cap.0.unwrap_or(0)) {
        Ok(()) => *sent = Some(*cap),
        Err(e) => {
            warn!("frame cap: the page has no window.__frameCap gate ({e:?}), running uncapped");
            *failed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_values() {
        assert_eq!(FrameCap::from_fps(None), Some(FrameCap(None)));
        assert_eq!(FrameCap::from_fps(Some(0)), Some(FrameCap(None)));
        for n in CHOICES {
            assert_eq!(FrameCap::from_fps(Some(n)), Some(FrameCap(Some(n))));
        }
        for n in [1, 29, 59, 61, 144, 1000] {
            assert_eq!(FrameCap::from_fps(Some(n)), None, "{n} is not offered");
        }
        assert_eq!(FrameCap::parse("off"), Some(FrameCap(None)));
        assert_eq!(FrameCap::parse("0"), Some(FrameCap(None)));
        assert_eq!(FrameCap::parse("60"), Some(FrameCap(Some(60))));
        assert_eq!(FrameCap::parse("fast"), None);
        assert_eq!(FrameCap::parse("75"), None);
    }

    #[test]
    fn intervals() {
        assert_eq!(FrameCap(None).interval(), None);
        let ms = |n| FrameCap(Some(n)).interval().unwrap().as_secs_f64() * 1000.0;
        assert!((ms(30) - 33.333).abs() < 0.001);
        assert!((ms(60) - 16.667).abs() < 0.001);
        assert!((ms(120) - 8.333).abs() < 0.001);
        let j = FrameCap(Some(60)).json();
        assert_eq!(j["fps"], 60);
        assert_eq!(j["interval_ms"], 16.67);
        assert!(FrameCap(None).json()["interval_ms"].is_null());
    }

    /// Milliseconds as `Duration`s stand in for `Instant`s.
    const fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn first_frame_sets_the_deadline_without_sleeping() {
        assert_eq!(pace(ms(100), None, ms(16)), (ms(0), ms(116)));
    }

    #[test]
    fn steady_frames_sleep_to_whole_intervals_without_drift() {
        // Frames that take 5 ms each, with a 1 ms sleep overshoot: the deadline stays on the grid.
        let mut deadline = Some(ms(16));
        let mut now = ms(5);
        for k in 1..=100u64 {
            let (sleep, next) = pace(now, deadline, ms(16));
            assert_eq!(now + sleep, ms(16 * k));
            assert_eq!(next, ms(16 * (k + 1)));
            deadline = Some(next);
            now = now + sleep + ms(1) + ms(5);
        }
    }

    #[test]
    fn a_late_frame_keeps_the_grid_and_a_stall_restarts_it() {
        // 4 ms late: no sleep, and the next deadline is still on the grid.
        assert_eq!(pace(ms(36), Some(ms(32)), ms(16)), (ms(0), ms(48)));
        // A 200 ms stall: no burst of back-to-back frames, the schedule restarts from now.
        assert_eq!(pace(ms(232), Some(ms(32)), ms(16)), (ms(0), ms(248)));
    }
}
