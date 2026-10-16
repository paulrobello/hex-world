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
| 1 | 46.7 | 48.8 | 46.72 | 51.8 | 8.583 | 7.272 | Accepted: 34 one-second checks, peak human Chrome7.5%, no violations |

The two clean p95 values improve7.64% and11.27% from the canonical pre-fix55.0ms. Mean improves7.18% and11.52% from52.8ms. This is a modest improvement, not a16.6ms pass. Both runs used the full scripted recipe and strict human-spike guard. Human desktop Chrome was present but idle, with no automated GPU contention.

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

Canonical native evidence is in `local-native/native.json`, `native-state.json`, `native.log` and `native-script.json`. MacDaddy Apple M4 Max, 16 CPU /40 GPU cores, Metal, exact saved2,066-tile world validated in `state.play.tiles`,0rising, same camera, High, grassoff, tiltoff,2560x1440,scale1, immediate presentation,600samples. Frame p50=16.42, p95=20.24, mean=16.07, max=61.33ms, host load7.016 before and7.577 after. Strict exclusivity21samples, human Chrome peak8.2%, no violations. Native play remains approximately60Hz paced despite immediate presentation, so this p95 is not pure CPU/GPU rendering cost. `diag.tiles` is0forplay, so `play_tiles` records the validated2,066 count separately.

Canonical ceiling series uses the same browser, renderer, viewport, DPR and settings as the canonical2,066-tile rows. Radius18 (997tiles) measured p5039.7, p9541.3, mean39.82 and max46.1ms, host load14.140to9.601. Two earlier samples were discarded for post-load27.94 and21.79. The accepted row has no automated contention, human Chrome peak9.7%, zero console errors. This world still misses16.6ms substantially. Radius20(1,231tiles) measured p5032.0,p9533.6,mean32.55,max81.0ms,load9.601to11.587,strictguardhumanChromepeak5.8%,noerrors. Radius22(1,484tiles) was interrupted at the12:40hard cutoff and has no accepted timing. Its `Target crashed` error was emitted during intentional task termination, not diagnosed as a game defect. Radius12/14did not fit the timebox. The997-tile world being slower than1,231tiles shows composition/visible work matters, not tile count alone.

`tile-ceiling-fit.json` records a descriptive least-squares fit across accepted rows. With997,1,231and2,066tile points, intercept is25.14ms and slope0.01163ms/tile. The16.6ms crossing extrapolates to negative734tiles, so this fit yields no positive proposed ceiling. It is unbracketed and not a proof that every smaller world fails. No owner-accepted ceiling is claimed.

## MiniMac assessment cutoff (12:22 PDT)

The MiniMac attempt sequence did not produce an accepted clean row. The later strict scripted round0 was rejected for one human Chrome sample at21.7% CPU (load2.687 to3.641). Strict round1 was rejected for recurring par-factory automated GPU overlap (load3.641 to5.498). These error-only rows are preserved in `web-radius26-rejected.json`, not under an acceptance-implying filename. The single authorized exact-save retry and native attempt remained deferred behind foreign automated GPU work.

The retry-only guard version2 permits isolated human Chrome CPU spikes, rejects five consecutive one-second samples at or above20%, and still rejects any foreign automated/native GPU overlap. Its rows must identify `exact-save-retry-v2-five-consecutive`. No rejected attempt is relabeled as accepted.

Canonical M4 Max measurements continued separately after host recovery. Expensive Cargo/clippy/WASM verification is intentionally deferred until all canonical web/native/ceiling measurements finish, to avoid creating measurement load. Syntax and diff checks passed before deployment, and full gates were green before the timing series. After canonical timing stopped at12:40, `make checkall` and `make checkall-ci` both passed (exit0). The retry-v2guard and native `play_tiles` reporting fix were verified only after the measurement series, preserving the no-build-during-timing rule.

Cleanup: the local measurement Chromium exited. Foreign processes were never terminated. Earlier SSH tasks can leave bounded remote waiters alive, so remote queue cleanup is not claimed. Their missing accepted-summary dependencies and browser/load guards prevent unguarded timing. The remote run directory remains `/Users/probello/perf/hex-world/post-arc001-2026-10-16`.

## Criterion assessment

1. **Chrome trace: met for diagnostic attribution.** Live trace, verified-symbol CPU profile, parsed ranking and hardware GPU total are recorded. Contention during construction is labeled, and absolute trace timings are excluded from budget evidence.
2. **Target not met; ceiling evidence pending.** Two canonical clean rounds are accepted measurements, p9550.8 and48.8ms, both above16.6ms. Ceiling probes997tiles(p9541.3ms,load14.140to9.601) and1,231tiles(p9533.6ms,load9.601to11.587) both fail. The descriptive fit is unbracketed and yields no positive16.6ms crossing. Radius22was interrupted at cutoff. No defensible tile ceiling or owner acceptance exists, so this criterion remains open. No card criterion was checked.
3. **Exact-world native measurement: met.** Canonical Metal run validates2,066playtiles and the exact camera/settings. p95=20.24ms, load7.016to7.577. Native pacing caveat is recorded.

Board handling was limited to the requested move to `in_progress`. No criteria were checked, no card was completed, and nothing was pushed.
## Ceiling refinement after host recovery (2026-10-16)

The follow-up used the original measured engine, not the newer follow-up worktree build. Main-checkout `dist/` was verified byte-identical: build `a6e0e284a2`, WASM SHA-256 `9cf1cf71ebdfd71755480dc1e7ffde44e08aa277da4c2d3ca83e05dfab10db79`. No rebuild or tracked repository edit occurred during this follow-up.

Host: MacDaddy Apple M4 Max, 16 CPU / 40 GPU cores. Chromium153.0.8010.12, headless GPU, channel `chromium`, ANGLE Metal Apple M4 Max, uncapped,2560x1440,DPR1,play,High,seed1,grassoff,tiltoff,camera[10,12,40]lookingat[0,0,0]. Every world was rebuilt with the original waterWorldScript recipe and read after700settledframes with600timing samples. Strict single-sample human-Chrome guard was unchanged.

A fresh stability pair preceded the series:1-minute host load7.461914 at13:27:39.733PDT and8.291992 at13:28:39.736PDT,60.003seconds apart. Immediate and post-run host loads had to remain<=15. Foreign automated GPU work was prohibited. Human desktop Chrome was present but idle during accepted windows.

| Radius | Tiles | p50ms | p95ms | Meanms | Maxms | Load before | Load after | Assessment |
|---|---:|---:|---:|---:|---:|---:|---:|---|
|22|1,484|—|—|—|—|8.320313|13.056641|Rejected: human Chrome reached24.6%CPU at one strict sample. No retry.|
|14|610|19.8|21.1|20.08|68.3|14.624023|9.072754|Accepted. Human Chrome peak9.1%,zero violations. One earlier post-load20.77sample discarded.|
|12|453|18.2|20.0|19.39|74.7|9.072754|7.682617|Accepted. Human Chrome peak7.7%,zero violations.|

All accepted rows report Chromium153.0.8010.12,ANGLEMetalAppleM4Max,matching buildID,grassdensity0,High,zero rising tiles,andzero console errors. The rejected radius22point is excluded from the fit.

Refitting all accepted rows (two radius26rounds plus radii18,20,14,12) gives:

`p95_ms = 13.871761881 + 0.01783233581 * tiles`

The global ordinary-least-squares extrapolation reaches16.6ms at approximately153tiles. This is **not a measured or validated ceiling**: the smallest measured world is453tiles and still has p9520.0ms, so no accepted point lies below16.6ms and no crossing is bracketed. The997-tile world being slower than1,231tiles also demonstrates nonmonotonic composition/visibility cost. The extrapolated153tile count is descriptive guidance for a future probe, not owner acceptance or proof that153tiles will meet budget.

Criterion2remainsopen: the original2,066tile target fails and no owner-approved ceiling exists. The additional low-end measurements strengthen the conclusion that reducing the world to several hundred tiles is still insufficient for the16.6ms target at these settings. No additional radius attempts were made. The browser exited and the tracked worktree remained clean. The orchestrator owns committing these scratchpad artifacts and updating the repository report.
