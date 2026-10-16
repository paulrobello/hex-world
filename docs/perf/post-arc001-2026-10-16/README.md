# Post-ARC-001 play-world measurement

Source engine revision: `e67ba43` (2026-10-16). Build ID `a6e0e284a2`. No engine or workspace files changed in this measurement branch. See `build.json` for exact web/native hashes.

## Hosts and acceptance policy

The canonical MacDaddy host (Apple M4 Max, 16 CPU cores) initially exceeded the absolute 1-minute load limit of 15. Observed values ranged from 93.53 to 503.53 during setup. `load-wait.jsonl` preserves this evidence. The host later recovered: two consecutive samples 60 seconds apart were 13.270 and 13.097, recorded in `local-load-stability.json`. Canonical measurements then began with the normal per-run load and browser-contention guards.

The coordinator authorized the standing secondary perf host: MiniMac, Apple M4, 10 CPU cores (4 performance and 6 efficiency), 10 GPU cores, 32 GiB RAM. MiniMac measurements are a separate baseline, not substituted into the M4 Max series. `minimac-host.json` records the actual host.

Every budget run requires host load <=15 immediately before and after, with overloaded samples discarded. Additionally, foreign automated Chromium jobs must be absent. Human desktop Chrome may remain only while its process-tree summed CPU stays below 20% of one core, checked every second. Accepted evidence must say: **human desktop Chrome present but idle; no automated GPU contention**. A CPU-load pass alone does not establish GPU exclusivity.

## Exact workload

`waterWorldScript(26)` from `e2e/water-world.mjs`: 2,066 tiles, including 838 ocean, 347 river_straight, 271 meadow, 200 beach, 142 forest, 123 road_straight, 81 house_cluster, 28 beach_outer, 21 river_bend and 15 beach_inner. Play scenario, High, seed 1, grass off, tilt off, 2560x1440, DPR 1. Camera `[10,12,40]`, looking at `[0,0,0]`. HUD live. Chromium headless GPU, channel `chromium`, ANGLE Metal, GPU enabled, blocklist ignored, vsync and frame limit disabled. The shipped default frame-latency limit remains unchanged.

Each web diagnostic is a rolling 600-frame window read after 700 settled frames. Renderer must be `ANGLE Metal Renderer: Apple M4`, not SwiftShader. The exact browser-built world is saved for native comparison.

## Prior baseline and rejected attempts

All rows below are MiniMac Apple M4, 10 CPU / 10 GPU cores, 2560x1440, DPR 1, hardware GPU Chromium headless and uncapped. Times are milliseconds. The accepted historical baseline was Chromium 153 with WebGPU adapter `Apple M4`. New rejected attempts explicitly confirmed Chromium `153.0.8010.12` and renderer `ANGLE (Apple, ANGLE Metal Renderer: Apple M4, Unspecified Version)`.

| Run | p50 | p95 | Mean | Max | Load before | Load after | Assessment |
|---|---:|---:|---:|---:|---:|---:|---|
| 2026-10-15 pre-fix MiniMac | 54.1 | 56.6 | 54.45 | 68.3 | 3.979 | 4.919 | Accepted historical same-host baseline |
| Post-fix attempt 0 | 52.2 | 55.4 | 52.11 | 62.3 | 7.142 | 6.975 | Rejected: concurrent the-cube automated GPU work |
| Post-fix attempt 1 | 52.6 | 63.9 | 54.17 | 104.5 | 6.975 | 11.743 | Rejected: concurrent automated GPU work |

The separate 2026-10-15 M4 Max baseline was p50 52.5, p95 55.0, mean 52.8 and max 58.8 ms, load 7.8 to 11.3. That is context only, not the comparator for MiniMac's new series.

The rejected MiniMac observations show that the publication fix's frame-time benefit is either small or masked by contention. They do not establish a clean post-fix p95, a validated speedup, or the <=16.6 ms target. The contaminated p95 is not a mathematical upper bound on an unobserved clean run.

## Canonical M4 Max result

`local-radius26-valid.json`, MacDaddy Apple M4 Max, 16 CPU cores, 40 GPU cores, Chromium153.0.8010.12 headless GPU/uncapped, ANGLE Metal Apple M4 Max, 2560x1440, DPR1, exact scripted radius26 world:

| Round | p50 | p95 | Mean | Max | Load before | Load after | GPU exclusivity |
|---|---:|---:|---:|---:|---:|---:|---|
| 0 | 48.9 | 50.8 | 49.01 | 55.8 | 10.064 | 8.583 | Accepted: 36 one-second checks, peak human Chrome13.9%, no violations |

Round0 p95 is 7.64% below the canonical pre-fix55.0ms, and mean is7.18% below52.8ms. This is a modest improvement, not a16.6ms pass. Human desktop Chrome was present but idle, with no automated GPU contention. Round1 remains in progress.

## Trace attribution

`contended-trace.json` captures the live 2,066-tile world, grass off, with `contended-trace.json.cpuprofile`, `contended-trace-summary.json`, `trace-costs.json` and a one-second `contended-processes.jsonl`. It is deliberately labeled **captured under par-factory GPU contention**, including the trace window: the final one-second process samples show a foreign GPU helper at 15–17% CPU and its renderer at 38–49%. Absolute trace-arm timings are not budget evidence. MiniMac load was 4.276 before and 5.625 after. Diagnostic frame times were p50 50.3, p95 52.6, mean 50.45 and max 59.0 ms on the same Chromium153/M4 renderer, viewport and device scale stated above.

CPU ancestry over the 2,392.906 ms sampled profile names these dominant costs (each category counted at most once per sample):

| CPU category | Sampled ms | Profile share |
|---|---:|---:|
| Directional-light mesh visibility | 355.368 | 14.85% |
| Bounds calculation | 244.011 | 10.20% |
| CPU visibility culling | 230.757 | 9.64% |
| Material mesh queueing | 181.642 | 7.59% |
| Play scatter synchronization | 149.412 | 6.24% |
| Mesh extraction for GPU building | 69.070 | 2.89% |
| State publication | 8.558 | 0.36% |
| JSON work | 6.736 | 0.28% |

The revision gate removed publication from the dominant costs. Remaining CPU work is per-entity visibility/bounds and render preparation. These inclusive categories may overlap and must not be summed with one another. Generic recursive scheduler/sort wrapper totals in the raw summary also overlap.

The instrumented hardware GPU total was mean 22.91, p50 22.89, p95 24.92 and max 26.85 ms (same diagnostic arm, load 4.276 to 5.625). CPU and GPU overlap, so their totals are not additive. Chrome's GPU service thread also spent 1,402.477 ms in `IOSurfaceImageBacking::WaitForCommandsToBeScheduled::Dawn`, a host-side wait rather than GPU execution time. Both CPU work and the base M4 GPU's instrumented cost exceed the 16.6 ms target. A clean measurement is still required.

The release WASM has no names section. `wasm-symbols.json` safely restores function names for CPU profiles: all non-custom sections of the symbol-preserving optimizer output match the shipped release byte-for-byte (SHA-256 checked). The symbol build uses the same wasm-bindgen 0.2.129 output and system wasm-opt 133 with `-Oz -g`. No code-level profiling changes were made. CPU sample ancestry and leaf samples are separate from inclusive, overlapping Chrome event totals. GPU-process host time is not hardware GPU execution time. The separate diagnostic arm enables timestamp queries for the GPU total, not the unreliable web pass split.

## Native and ceiling measurements

The exact ARM64 release binary and runtime assets are transferred. Native uses the saved 2,066-tile world, same camera, High, grass off, tilt off, 2560x1440, scale 1, immediate presentation and a full 600-frame ring. Native play can still pace to 60 Hz, so a paced p95 must not be described as CPU render cost.

Native and smaller-radius runs remain queued behind uncontaminated web rounds and trace, and the same browser-contention/load conditions. Candidate ceiling worlds are radius 18 (997 tiles), 20 (1,231), 22 (1,484), with 8 and 12 available if needed to bracket 16.6 ms. A proposed ceiling needs measured bracketing points and owner acceptance. None is claimed yet.

## Criterion assessment

1. **Chrome trace: met for diagnostic attribution.** Live trace, verified-symbol CPU profile, parsed ranking and hardware GPU total are recorded. Contention during construction is labeled, and absolute trace timings are excluded from budget evidence.
2. **Two guarded <=16.6 ms rounds or owner-accepted ceiling: blocked on host quiescence.** The two observed post-fix attempts are rejected. No ceiling accepted or criteria checked.
3. **Exact-world native measurement: blocked on host quiescence.** Binary and exact-save methodology are ready, but no accepted native timing exists yet.

Board handling was limited to the requested move to `in_progress`. No criteria were checked, no card was completed, and nothing was pushed.
