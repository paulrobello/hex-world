# Web stall tail: cause and fix (ADR 0001 carry-over)

Measured 2026-10-14 on build `bf64195` (main), Apple M4 Max, Chromium for Testing 153 (Playwright
1.63.0) headless with `--use-angle=metal --enable-gpu --ignore-gpu-blocklist --enable-unsafe-webgpu
--enable-webgpu-developer-features`. "Uncapped" adds `--disable-gpu-vsync --disable-frame-rate-limit`,
"paced" leaves both on (60 Hz). Workload: the perf-passes budget run (`gate2000`, strategic view,
instanced, mid600 shadows, `tier=high`, `post=full`, `fps=off`) at 2560x1440, DPR 1. Every run sat
inside the host-load guard (1-minute load at most 15 before and after; each row records both).

## Cause

The stalls are Chrome's WebGPU backpressure, released in one long block. They happen only when the
page's animation frames come faster than the browser's GPU side consumes them.

- **Uncapped, the page outruns the GPU side.** The page's frame callback takes about 2.4 ms. The GPU
  process's main thread (`CrGpuMain`), which decodes the WebGPU command stream and encodes it for Metal,
  is busy about 2.5 ms per frame. GPU execution adds its own queue on top. The work piles up. A page
  hook on `GPUQueue.submit` and `onSubmittedWorkDone` counted 109 submits in flight at the median (2
  per frame, so about 55 frames) and up to 146, and a submit took 180 ms at the median (up to 286 ms)
  to complete (`hooks-smoke.json`).
- **The compositor then waits, and the page waits on it.** About every 50 frames (every 176 ms), the
  display compositor presents the canvas. In `SkiaOutputSurfaceImplOnGpu::SwapBuffers` →
  `ScheduleOverlays()` it calls `IOSurfaceImageBacking::WaitForCommandsToBeScheduled::Dawn`, which
  blocks `CrGpuMain` for 48–67 ms until the queued work for that surface is scheduled. The page's next
  frame callback flushes WebGPU commands to that same blocked thread, so it sits in
  `CommandBufferProxyImpl::WaitForToken` for 49–55 ms. That frame is the stall (`trace-base-summary.json`,
  `trace-base-excerpt.json`).
- **The arithmetic matches.** Uncapped, the mean frame time (3.3 ms) is the real throughput: about 49
  fast frames at 2.4–2.8 ms followed by one 45–60 ms stall. Tilt-shift adds GPU work per frame, so the
  backlog per cycle and the stall get longer: the worst frame per run was 79–87 ms with it on, against 40–69 ms off.

Native has no such tail. There `--present immediate` frames wait on the window's drawable, not the CPU:
the main thread is busy about 0.9 ms and the render thread about 2.2 ms a frame against a 4.5 ms GPU
total, and the render thread spends the rest of the 7–8 ms frame in `get_current_texture` until one of
the surface's three drawables comes back (`native.md`). The wgpu surface bounds frames in flight, so no
GPU queue forms. The perf-passes re-run on `bf64195` (`../p2-passes-2026-10-14.json`) did log 2–5
native frames over 25 ms per 600 (worst 47.8 ms)
in 5 of 12 runs, against 1 frame in 1 of 12 in the earlier file. They are not this queue. Of the 14 in
timing-on runs, 6 sit next to a GPU frame of 15.6–19.2 ms (3–4 times the 4.5 ms median) and 8 next to
GPU frames of about 5 ms. The GPU ring also logged GPU frames over 25 ms (prepass and shadow segments of
9–22 ms) in 4 of 6 timing-on runs, mostly before the frame-time window. That native tail is attributed
in `native.md`: long waits for the next drawable, a few of them after another process's GPU work, plus
two rare CPU-side events.

The play scenario agrees with the mechanism. The web water world (`../p2-water-world-2026-10-14.json`)
runs uncapped at 7.8–8.4 ms a frame, CPU-bound in the wasm callback (7.3 ms of a 7.8 ms interval), and
had 0 frames over 25 ms in all 4 runs: when the page is slower than the GPU side, no backlog forms.

## What the trace and the toggles rule out

Trace: `node e2e/stall-probe.mjs` with `PROBE_TRACE`, 1 s of steady state (frames 720 to 1,020, 6 stalls,
load 6.55 → 8.47). Categories: devtools.timeline, v8, v8.execute, disabled-by-default-v8.gc, v8.wasm,
viz, gpu, gpu.dawn, gpu.service, webgpu, disabled-by-default-webgpu, toplevel. Summary by
`e2e/stall-trace-summary.mjs`. The excerpt (`trace-base-excerpt.json`, 225 spans of at least 100 µs on
the page main thread, `CrGpuMain` and `VizCompositorThread`, 20 ms either side of the worst stall)
opens in Perfetto. The raw 207 MB trace is not committed.

| Suspect | Evidence | Verdict |
|---|---|---|
| JS GC | 515 GC events in the trace window (54 ms in total, mostly incremental), 0 overlapping a stall | Ruled out |
| wasm `memory.grow` | No grow event in the trace, and `wasm_memory_bytes` is identical at READY and at the end of every run (65–67 MB) | Ruled out |
| Bevy asset or render-world sync | The stalled callback's 49–55 ms is spent in `WaitForToken`, a wait on the GPU process, not in wasm work. `pipelines_waiting` is 0 at every stall | Ruled out |
| HUD overlay rAF and state polling | With `web/ui/hud.js` replaced by an empty script, there are still 12 and 8 stalls per 600 (`hud_off`). In gate2000 the overlay polls once a second anyway | Ruled out |
| `hexWorld.state()` / `diag()` publishing | Harness polling at 5 s instead of 250 ms: 12 and 12 (`poll_slow`), and with the HUD off as well: 12 and 21 (`quiet`). These two arms read the diag up to 5 s after the window filled, so their window also holds the every-30-frame census frames that base never sees. Read them as "polling is not the cause", not as a stall count | Ruled out |
| Audio | gate2000 never starts the audio context (no gesture), and `hud_off` removes the audio layer entirely: stalls remain | Ruled out |
| GPU queue backpressure (`onSubmittedWorkDone`) | Uncapped: 109 submits in flight at the median, 180 ms submit-to-done. Paced: 2 in flight (one frame), 8.5–10.6 ms submit-to-done, 0 stalls (`hooks_paced`) | Cause, with the compositor wait below |
| Browser compositor | `SwapBuffers` → `WaitForCommandsToBeScheduled::Dawn` holds `CrGpuMain` for the whole stall | The point where the backpressure lands |
| Less work per frame | `post=none` 0 and 0 stalls, 1280x720 2 and 0, shadows off 10 and 6 | Consistent: the backlog forms only while consuming a frame takes longer than producing one |
| `getCurrentTexture` / `submit` blocking | Both under 0.2 ms at every stall | Ruled out |

The trace's stall spacing of 49–50 frames is not the visible-set census period (30 frames), so the
census frames in that window do not explain it.

## Fix: an opt-in frame-latency limit

The default build is unchanged, because the backlog never forms at 60 Hz. Paced runs keep at most two
frames in flight and have 0 frames over 25 ms (p95 17.0–17.6 ms, 4 runs). `?frame_latency=N`
(`web/frame-latency.js`, N from 1 to 8) bounds the page the way wgpu's native surface does: a frame
that submitted stays in flight until `onSubmittedWorkDone`, and while N frames are in flight the next
`requestAnimationFrame` callback waits. Without the key nothing is patched.

Before/after, 2 runs each under the load guard. The shipped key was measured on the rebuilt `dist` with
`web/frame-latency.js` (`fix.json`, load 10.9–14.9); the page-shim prototypes came first (`limiter.json`,
load 6.3–8.6):

| Arm | frames over 25 ms per 600 | max ms | mean ms | p95 ms |
|---|---|---|---|---|
| base, uncapped (`fix.json`) | 8, 6 | 69.4, 52.7 | 3.29, 3.36 | 3.5, 3.8 |
| `?frame_latency=3`, uncapped (shipped key) | **0, 0** | 8.1, 23.9 | 3.63, 3.69 | 4.8, 5.5 |
| `?frame_latency=3`, paced (shipped key) | 0, 0 | 19.1, 20.0 | 16.67, 16.67 | 17.5, 17.5 |
| base, uncapped (`limiter.json`) | 8, 9 | 66.9, 46.7 | 3.33, 3.32 | 3.6, 3.6 |
| N = 3, uncapped (prototype) | 0, 0 | 14.3, 21.5 | 3.57, 3.59 | 5.5, 5.4 |
| N = 2, uncapped (prototype) | 1, 1 | 29.6, 30.2 | 4.20, 4.17 | 6.8, 6.6 |
| N = 2, paced (prototype) | 0, 0 | 19.4, 19.0 | 16.67, 16.67 | 17.6, 17.6 |
| base, paced (`toggles.json`, `limiter.json` hooks) | 0 in all 4 | 17.5–18.4 | 16.67 | 17.0–17.6 |

With the limit, the uncapped frames even out at the throughput the GPU side sustains. The mean rises
0.3 ms with N = 3 (the page idles for the round trip of each `onSubmittedWorkDone`; 237 and 307
deferred frames per 600). p95 rises from 3.5–3.8 to 4.8–5.5 ms because the fast frames that ran ahead
are gone. N = 2 costs more (+0.85 ms mean) and still lets one frame per run through. At 60 Hz the limit
is inert: 5 deferrals per 600 frames and the same p95 as the paced base.

## Reproduce

```bash
make build-web
caffeinate -d -u node e2e/stall-probe.mjs dist target/stall-probe.json          # every arm, 2 rounds
PROBE_ARMS=base PROBE_ROUNDS=0 PROBE_TRACE=target/trace.json \
  caffeinate -d -u node e2e/stall-probe.mjs dist target/trace-run.json             # one 1 s trace
node --max-old-space-size=8192 e2e/stall-trace-summary.mjs target/trace.json target/trace-summary.json target/excerpt.json
```

Files: `toggles.json` (the suspect arms, 2 rounds), `hooks-smoke.json` (the first base, hooks and paced
runs, with the in-flight probe), `limiter.json` (limiter prototypes and `hooks_paced`), `fix.json` (the
shipped key), `trace-base-summary.json`, `trace-base-excerpt.json`.
