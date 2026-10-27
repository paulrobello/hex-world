# Frame pacing: short and long frames in pairs (2026-10-27)

Card (high): with Settings closed, play alternates short and long frames at a 63 ms pair period on MiniMac (`docs/perf/grass-build/README.md`, `pairs-2026-10-26/`).

## Cause

The web frame-latency limiter (`web/frame-latency.js`, on by default at 3) counted a frame as in flight only when its `requestAnimationFrame` callback returned. Bevy's winit runner submits the frame's GPU work and then asks for its next animation frame inside the same callback (`bevy_winit` 0.20 `redraw_requested`: `run_app_update`, then `window.request_redraw()`). The limiter checks the in-flight count when a frame is requested, so that request saw the count from before the frame's own submit and went through. Every second frame passed the gate. The in-flight count went 2, 3, 4, and the limiter released frames again only once it was back under 3, which took two GPU completions.

Once the GPU is the bottleneck, frames therefore run in back-to-back pairs. The first frame is released by a completion, and the second runs immediately after it. The short interval is one frame's CPU time. The long interval is two GPU frames minus that. On MiniMac that is 11 ms and 52 ms against a GPU frame of about 31.7 ms. Main-thread work moves between the two frames of a pair without changing the pair period, which is why the grass change shifted 7 ms from the short frame to the long one and left the mean unchanged.

The Settings sheet does not cause it. Opening Settings focuses the sheet's tab button, which blurs the canvas. Bevy's default `WinitSettings::game()` then switches the engine to a 60 Hz reactive update. In that mode the harness's rAF loop no longer measures engine frames: on macdaddy the page's rAF runs at under 1 ms while the engine draws exactly 300 frames per 5 s. Every Settings-open window in `perf-grass-drag.mjs` (`baseline`, `drag`, `still_0`, `still_150`) ran in that mode. With focus put back on the canvas, the Settings-open frames pair exactly like the closed ones.

## Evidence (macdaddy, M4 Max, headless GPU Chromium, uncapped)

`e2e/frame-pacing.mjs` uses perf-grass-drag's world, camera, keys and flags, and runs 3-5 s still windows in a sequence of HUD states inside one page. It records two things:

- the probe's rAF intervals, which pass through the limiter like the engine's;
- the first `GPUQueue.submit` of each task, which is an engine frame (`submit`), with its work-done time.

`PACING_TIMELINE=1` adds a 1 s log of every real rAF request, callback start and end, submit and work-done, each with the limiter's in-flight count.

At 2560x1440 this host is CPU-bound, at about 11-13 ms per frame under loads of 25-250, and frames do not pair. A 5120x2880 viewport has four times the pixels and makes the M4 Max GPU-bound at about 21 ms per frame. There the MiniMac pattern reproduces. Host load was 55-390 throughout, so absolute times are not evidence. The pattern within each run is.

### HUD and focus toggles (`toggles-5k.json`, `toggles-1440.json`)

Every window below except `unfocused` had focus put back on the canvas before it ran. "Submit pair" is the mean short and long interval of consecutive pairs, with `alternation`.

| Window (5120x2880, limit 3) | Engine frames / 3 s | Submit mean ms | Submit p95 ms | Submit pair ms | Alternation |
|---|---|---|---|---|---|
| Settings closed (first) | 143 | 21.05 | 31.2 | 12.1 / 30.0 | 1 |
| Settings open | 142 | 21.12 | 31.6 | 11.8 / 30.6 | 1 |
| Settings closed | 138 | 21.74 | 33.2 | 12.1 / 31.6 | 1 |
| Tiles sheet closed | 144 | 20.92 | 28.8 | 14.2 / 27.8 | 1 |
| HUD hidden (H) | 142 | 21.23 | 32.7 | 14.4 / 28.1 | 1 |
| HUD script not loaded (`hud_off`) | 132 | 22.55 | 33.1 | 13.6 / 31.7 | 1 |
| Canvas blurred (`unfocused`) | 137 | 21.97 | 26.7 | 19.9 / 24.1 | 0.65 |

At 2560x1440 (`toggles-1440.json`) every focused window runs at 11.1 ms with no pairing. That run used `PACING_DPR=2`, which leaves the canvas at 2560x1440, and it was taken before submit recording existed, so it has rAF and `engine_frames` fields only. Blurring the canvas alone gives exactly 300 engine frames per 5 s, while the page's rAF intervals fall to a mean of 0.75 ms.

### Frame-latency limit (`latency-5k.json`, Settings closed, canvas focused)

| `frame_latency` | Submit mean ms | Submit p95 ms | Submit pair ms | Alternation |
|---|---|---|---|---|
| off | 19.8-21.1 | 17.9-24.5 | 15.0-19.3 / 20.2-27.0 | 0.34-0.45 |
| 1 | 27.9-32.3 | 46.9-54.7 | 11.7-12.0 / 44.3-53.0 | 1 |
| 2 | 22.5-26.2 | 31.4-42.1 | 12.9-16.2 / 28.8-39.8 | 1 |
| 3 (default) | 20.8-21.7 | 26.4-33.4 | 12.0-18.7 / 22.9-31.6 | 0.97-1 |
| 8 | 27.5-34.5 | 65.8-88.4 | 17.5-19.2 / 36.3-51.9 | 0.70-0.76 |

Turning the limiter off removes the pairing. Every limit from 1 to 8 pairs.

### Timeline, limit 3 before the fix (`fix-5k.json`, `shim_a` rows, `timeline`)

One pair, in ms from the start of the log:

```
29.5  done   inflight 3 -> 2: the limiter releases the deferred engine and HUD callbacks
29.8  cb     engine frame A
40.9  submit
41.0  req    A asks for its next frame inside its callback: inflight still reads 2, so it is not deferred
41.1  end    inflight 3
41.9  cb     engine frame B runs at once
53.6  submit
53.6  end    inflight 4; B's request was deferred
53.6-69.9    main thread idle until the second completion brings inflight back to 2
```

At the end of each callback the in-flight count reads 2, 3 or 4 (21, 43 and 22 of 86 callbacks).

### Chrome trace (`trace-before-5k.json`, `trace-after-5k.json`)

These are 3 s CDP traces of a closed window (`PACING_TRACE`), split by `e2e/frame-pacing-trace.mjs` into the intervals between engine frames. Tracing slows the frames, so the long interval reads 45 ms instead of 30-35 ms.

| Limiter | Interval | n | Interval ms | Engine callback ms | After the callback, ms | Renderer main busy, ms | GPU main busy, ms | Of which `ApplyBackpressure::Metal`, ms |
|---|---|---|---|---|---|---|---|---|
| before | short | 53 | 11.9 | 10.9 | 1.0 | 0.8 | 1.0 | 1.0 |
| before | long | 53 | 44.9 | 10.9 | 34.0 | 1.2 | 33.9 | 30.2 |
| fix | short | 78 | 18.7 | 10.5 | 8.2 | 0.8 | 8.2 | 7.8 |
| fix | long | 78 | 20.0 | 10.5 | 9.5 | 1.0 | 9.5 | 8.6 |

In the long interval the page's main thread is idle. No JS or wasm runs there, and raster, layout or HUD work would show as renderer main busy time. The GPU process's main thread spends that time in `CALayerTreeCoordinator::ApplyBackpressure::Metal`, waiting for the GPU to finish earlier frames. The long frame is the wait for GPU completions. With the fix, the same wait is split evenly across frames.

## Fix

The limiter now counts a frame as in flight from its first submit inside the callback, and still waits on `onSubmittedWorkDone` after the callback returns, so the wait covers all of the frame's submits. A request made after the submit sees the frame and is deferred. The default limit (owner decision Q7, 3) is unchanged and is now the real bound: at the end of every callback the in-flight count reads exactly 3 (96 of 96).

A/B on one dist, with the limiter file swapped by `page.route` (`fix-5k.json`, `fix-1440.json`). Arm A is the `8464de8` limiter, arm B is the fix, in alternating runs:

| 5120x2880, closed and open windows | Submit mean ms | Submit p95 ms | Submit pair ms | Alternation |
|---|---|---|---|---|
| A (before) | 20.8-25.7 | 31.5-36.9 | 11.7-12.3 / 30.0-39.4 | 1 |
| B (fix) | 20.4-26.0 | 22.0-27.1 | 19.9-24.5 / 21.0-27.5 | 0.37-0.72 |

Mean throughput is unchanged, frames are even, and submit-interval p95 falls by 9-12 ms. The rebuilt `dist` with the fix (`shipped-5k.json`, the full toggle sequence, load 230-460) stays even in every focused window: submit pair 22.9-23.4 / 23.9-25.0 ms and p95 24.6-26.4 ms around a 23.5-24.0 ms mean. At 2560x1440, where this host is CPU-bound, the arms match within the load drift. In the low-load round (load 25-34) the submit mean was 12.6-13.8 ms before and 12.8-13.1 ms after, with no pairing in either.

With `frame_latency=off`, the work-done latency reached 344-1003 ms in a window, which is the uncapped backlog the limiter exists to bound (`docs/perf/stall-tail/`). The limiter stays on.

### Paced, as players run (`paced-1440.json`, `paced-5k.json`)

`PACING_PACED=1` keeps Chrome's display pacing (60 Hz headless). It ran as A, B, B, A, with 5 s windows.

| Viewport | Arm | Engine frames / 5 s | Submit p50 ms | Submit pair ms | Alternation | Submit intervals over 25 ms | Deferrals / 5 s |
|---|---|---|---|---|---|---|---|
| 2560x1440, load 15-21 | A (before) | 299-301 | 16.6-16.7 | 16.2-16.4 / 16.9-17.2 | 0.59-0.71 | 0-1 | 0-7 |
| 2560x1440 | B (fix) | 298-301 | 16.6-16.7 | 16.2-16.3 / 17.2-17.4 | 0.68-0.76 | 0-1 | 0-14 |
| 5120x2880, load 20-250 | A (before) | 229-241 | 17.9-18.9 | 13.4-14.0 / 27.7-29.9 | 0.96-0.98 | 83-100 | 340-352 |
| 5120x2880 | B (fix) | 227-237 | 20.5-21.4 | 18.0-18.9 / 23.9-25.4 | 0.53-0.65 | 50-62 | 680-711 |

At 60 Hz, when the GPU keeps up, the fix changes nothing measurable, so Q7's premise holds. When the GPU does not keep up, frames are drawn at the same rate, the pairs are gone, and submit intervals over 25 ms fall by about 40%. Deferrals double because each frame now waits at its own request instead of every other one.

## Separate finding: Settings throttles the engine to 60 Hz

While any HUD control held focus, the engine updated at 60 Hz reactive (Bevy `WinitSettings::game()` `unfocused_mode`). The Settings sheet, the Worlds sheet's fields and anything else that takes focus all did this. On a 60 Hz display it is close to invisible. On a 120 Hz display, opening Settings halved the frame rate.

Fixed for owner A29 (2026-10-27): the web build runs `WinitSettings::continuous()`; native keeps `game()` (PRD §9.1). `focus-keep-a29.json` is the `default` arm with `PACING_FOCUS=keep` (macdaddy, uncapped, 2560x1440, load 157-174): with Settings open and focus on its tab button the engine drew 410 frames in 5 s, against 416 with the canvas focused and exactly 300 before the fix. Every later window kept focus on a HUD control and drew 345-364.

## Frame rate cap (owner A29)

`cap_off`, `cap_30`, `cap_60` and `cap_120` arms set `?frame_cap=` and run closed, open and unfocused windows. Engine frames per 5 s on macdaddy at 1280x720, radius-4 world (load 157-235):

| Arm | Paced 60 Hz (`cap-paced-a29.json`) | Uncapped (`cap-uncapped-a29.json`) |
|---|---|---|
| Off | 299-301 | 732-748 |
| 30 | 151 (even 33.3 ms) | 151 |
| 60 | 300-301 | 299-301 |
| 120 | 300-301 (above the display) | 209-210 |

Uncapped headless Chrome delivers an animation frame that draws nothing 17-24 ms late, so each held frame costs that long there: the 60 cap alternates 6.5 and 24 ms intervals around the right mean, and the 120 cap reads 42 fps. With display pacing every frame arrives on vsync, and 30 and 60 hold exactly. A 120 Hz paced run (a real ProMotion display) is still to do.

Off is unchanged on the web (`cap-off-ab-a29.json`): the same dist with `web/frame-latency.js` swapped by `PACING_SHIM_A` (the file before the gate) and `PACING_SHIM_B` (with it), ABBA, uncapped at the light setting, load 346-561. Engine frames per 5 s were 651-693 before and 674-694 after, and submit p95 was 7.6-8.9 ms before and 7.6-7.9 ms after.

Native (`target/release/hex-world --scenario single --size 640x400 --present immediate --frame-cap N`, a script times 301 frames between two `dump_state` commands from the log; macdaddy, load 347-490, so rough): Off 126-140 fps, 30 cap 29.7, 60 cap 59.8, 120 cap 116.8-119.5. One Off run read 58.6 fps: its window opened without focus, and native keeps Bevy's 60 Hz unfocused update.

## MiniMac before/after recipe

On the build host, build the fix's `dist` and keep the `8464de8` limiter beside it:

```bash
make build-web
git show 8464de8:web/frame-latency.js > /tmp/frame-latency-8464de8.js
rsync -a --delete dist/ minimac:perf/hex-world-pacing/dist/
rsync -a --delete --exclude node_modules --exclude test-results e2e/ minimac:perf/hex-world-pacing/e2e/
rsync -a web/ui/src/ minimac:perf/hex-world-pacing/web/ui/src/
scp /tmp/frame-latency-8464de8.js minimac:perf/hex-world-pacing/frame-latency-8464de8.js
ssh minimac 'cd perf/hex-world-pacing/e2e && npm ci --no-audit --no-fund'
```

On MiniMac, run 3 alternating rounds (ABBA order) at the card's 2560x1440, under the load guard, then one toggle run:

```bash
ssh minimac
cd perf/hex-world-pacing
ulimit -n 10240
export PACING_GUARD=1 PACING_SHIM_A=frame-latency-8464de8.js PACING_SHIM_B=dist/web/frame-latency.js
for round in 1 2 3; do
  if [ $((round % 2)) -eq 0 ]; then arms="shim_b,shim_a"; else arms="shim_a,shim_b"; fi
  PACING_ARMS=$arms PACING_OUT=pacing-ab-$round.json caffeinate -d -u node e2e/frame-pacing.mjs dist
done
for round in 1 2; do
  if [ $round -eq 2 ]; then arms="shim_b,shim_a"; else arms="shim_a,shim_b"; fi
  PACING_PACED=1 PACING_ARMS=$arms PACING_OUT=pacing-paced-$round.json caffeinate -d -u node e2e/frame-pacing.mjs dist
done
PACING_ARMS=default PACING_OUT=pacing-toggles.json caffeinate -d -u node e2e/frame-pacing.mjs dist
```

For each row, read `phases[]`: `submit.pairs.alternation`, `submit.pairs.short_mean` and `long_mean`, `submit.p95`, `submit.mean`, the rAF `p95`, `engine_frames`, `deferrals`, `latency.p50`, and `load_before` and `load_after`. The fix is confirmed when the following hold on every round:

- In `shim_a`, closed and open windows show alternation near 1 with a gap of tens of milliseconds, reproducing the card's 11/52 ms.
- In `shim_b`, the same windows show alternation at or below about 0.7 with short and long means within a few milliseconds of each other.
- `submit.mean` matches between arms within the round-to-round spread.
- `submit.p95` falls from the long frame (about 52 ms) toward the mean (about 32 ms).

In `pacing-paced-*.json`, where the M4 GPU is the bottleneck at about 31.7 ms, both arms should draw the same `engine_frames`. `shim_b` should lose the pairing and have fewer submit intervals over 25 ms (`submit.over_25ms`). In `pacing-toggles.json`, the `open` window should match `closed` once the canvas is refocused, and `unfocused` should show 300 engine frames per 5 s, the 60 Hz cap.

## MiniMac result, 2026-10-27 07:00-07:12Z (`minimac-2026-10-27/`)

Run as in the recipe above, at 2560x1440 with load 1.9 to 2.9 before and after every run (none discarded). Renderer: Apple M4, Metal. `shim_a` is the `8464de8` limiter and `shim_b` the fix, both on the fix's `dist`. Submit-interval statistics per window (closed, open, closed):

| Run | Arm | Alternation | Short / long mean (ms) | p95 (ms) | Mean (ms) |
|---|---|---|---|---|---|
| uncapped, rounds 1-3 | before (`shim_a`) | 1.00 in every window | 10.6-10.8 / 51.6-55.4 | 52.5-56.4 | 31.1-33.0 |
| uncapped, rounds 1-3 | fix (`shim_b`) | 0.61-0.74 | 30.3-32.6 / 31.2-33.8 | 31.6-34.3 | 30.8-33.2 |
| 60 Hz paced, rounds 1-2 | before | 0.91-0.99 | 14.1-14.7 / 51.2-52.0 | 64.6-68.5 | 32.6-33.1 |
| 60 Hz paced, rounds 1-2 | fix | 0.63-0.75 | 27.6-30.2 / 35.8-39.0 | 48.2-52.2 | 32.8-33.5 |

- The before arm reproduces the card's pairs (11/52 ms). With the fix the pairs are gone: short and long means sit within about 1 ms uncapped (alternation without a gap is not pairing), and the p95 falls from the long frame to the mean, 52.5-56.4 -> 31.6-34.3 ms uncapped and 64.6-68.5 -> 48.2-52.2 ms paced.
- The mean and the engine frame count match between arms in every round (150-164 engine frames per 5 s), so the fix changes pacing, not throughput. At this resolution the M4 is GPU-bound at about 32 ms a frame.
- The recipe expected fewer paced submit intervals over 25 ms with the fix. On MiniMac there are more (127-142 against 72-76), because the GPU-bound frame (about 32 ms) is itself over 25 ms: evenly spaced frames all cross it, while pairs put half under it. The p95 is the measure that applies here.
- Toggles (`pacing-toggles.json`, fix only): the open window matches closed (p95 34.2-34.3 ms). The unfocused window drew 146 engine frames, not the 300 the recipe expected from the 60 Hz unfocused update, because the GPU-bound frame is already slower than 60 Hz. The frame rate cap work (owner A29) removes that 60 Hz unfocused update on the web.

