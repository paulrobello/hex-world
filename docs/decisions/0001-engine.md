# 0001 — Engine: Bevy 0.20 vs custom wgpu

Date: 2026-10-12  Status: Provisional, gate not fully met (Safari renders since Task 15; paced p95 misses 17.0 ms in Chrome and in Safari close-up)

## Measurements

The Chrome rows measure the shipping architecture: `layout=instanced` (Task 14), with 2,000 tiles, 40,000 instanced static pieces (15,348 tris per tile at LOD0), 301 entities, 300 vehicles and 1,000 compute boids, at 2560×1440, with TAA, SSAO, bloom and DOF on. Each figure is the median of 3 uncapped runs in a fresh headless Chromium 153 (`apple | metal-3`, Apple M4 Max, as named in the matrix file's `web` header; rows record `BrowserWebGpu | Other`) over the 600 frames after READY, under the host-load guard (1-minute load ≤ 15 before and after every run). Source: `docs/perf/p0-gate-matrix-task10.json`, re-run on this build (`MATRIX_ONLY=web MATRIX_CONFIGS=instanced-tuned,instanced-mid600,instanced-mid1200,instanced-full`). `make perf-gate` was not used because it measures only the entities layout. The Safari column comes from `"safari"` in `docs/perf/p0-safari-gate.json` (Task 15, after the fix in `docs/perf/p0-safari-diagnosis.md`): paced at 60 Hz, median of 3 runs over the 600 frames after READY under the same load guard, instanced layout with tuned shadows and TAA, SSAO, bloom and DOF on, at 2992×1774 device px.

| Check | Chrome | Safari | Pass? |
|---|---|---|---|
| p95 frame time, strategic view, tuned shadows (`shadow_casters=large`, 3 cascades, 2,000 m; no static shadows at this zoom) | 3.7 ms [3.6–3.9] | n/a: Safari has no uncapped mode (see the paced row) | Chrome yes (≤16.6) |
| p95, strategic, middle 600 (all casters, 3 cascades, 600 m) | 3.8 ms [3.7–3.8] | n/a: Safari has no uncapped mode (see the paced row) | Chrome yes |
| p95, strategic, middle 1,200 (all casters, 3 cascades, 1,200 m; all 40,000 pieces cast) | 4.3 ms [4.1–4.5] | n/a: Safari has no uncapped mode (see the paced row); renders with 0 errors (untimed functional run) | Chrome yes |
| p95, strategic, full shadows (all casters, 4 cascades, 2,000 m), contrast row | 22.0 ms [20.1–30.3] | not timed | **fail** |
| p95 frame time, close-up view, tuned | 3.7 ms [3.5–3.7] | n/a: Safari has no uncapped mode (see the paced row) | Chrome yes |
| p95, close-up, middle 600 | 4.0 ms [3.7–4.0] | n/a: Safari has no uncapped mode (see the paced row) | Chrome yes |
| p95, close-up, middle 1,200 | 4.3 ms [4.2–4.5] | n/a: Safari has no uncapped mode (see the paced row) | Chrome yes |
| p95, close-up, full shadows, contrast row | 38.6 ms [34.1–50.6] | not timed | **fail** |
| Uncapped frames over 25 ms per 600, per run (tuned / mid600 / mid1200 / full), strategic | 5, 7, 15 / 8, 9, 8 / 22, 21, 14 / 21, 43, 22 | paced, tuned only: 0, 15, 0 (max frame 18 / 34 / 22 ms) | not a pre-set criterion; cause unmeasured |
| Same, close-up | 9, 12, 10 / 20, 20, 21 / 26, 28, 28 / 41, 43, 43 | paced, tuned only: 0, 1, 3 (max frame 20 / 34 / 38 ms) | not a pre-set criterion; cause unmeasured |
| Paced at 60 Hz, p95 (criterion set before the runs: ≤ 17.0 ms), FP1–FP4 from `p0-gate-matrix-paced-fix1.json`, not re-run | full 17.3 / 17.3, tuned 17.5 / 17.3 (strategic / close-up) | tuned 17.0 / 18.0 [17–18] (strategic / close-up); means 16.67 / 16.69 ms; Safari timestamps have 1 ms resolution | **fail** (Chrome by 0.3–0.5 ms; Safari strategic meets 17.0, Safari close-up misses by one 1 ms tick) |
| SSAO active | yes (`ssao: true` from the render-world probe in the perf-gate run on the same adapter, entities-layout probe, not the instanced rows; the matrix rows do not record it). Instanced pieces sample SSAO since Task 14 fix round 1 | yes: `ssao: true` in every Safari run (instanced rows) | yes |
| GPU preprocessing mode | `PreprocessingOnly` | `PreprocessingOnly` | — |
| TAA / bloom / DOF visible, no validation errors | yes: visible in Task 6 shots; 0 console.error across all 24 matrix runs (runs predate the Bevy ERROR-text check) | yes: post stack `taa, ssao, bloom, dof` active, scene checked by screenshot; 0 console.error across 6 timed and 4 functional runs (runs predate the Bevy ERROR-text check) with `web/wgsl-compat.js` | yes |
| Compute boids + custom material render | yes: boids render and diag reports 1,000 dispatched (Task 7; the count is dispatched, not drawn) | yes: diag reports 1,000 boids; the instanced custom material renders the close-up (screenshot) | yes |
| meshopt GLB + BC7 KTX2 load on wasm32 | yes: `hex_test.glb` (meshopt, 28,060 B) and `hex_test.bc7.ktx2` load in the web smoke test, texture family `Bc7` (Task 8). ASTC and PNG were exercised only natively | yes: texture family `Bc7` selected, the textured test tile renders, no asset 404s | yes |
| wasm gzip wire size / time to interactive | 8,462,064 B gzip (23,287,298 B raw), from this build's `make web-size` / READY 1.16–1.39 s (median `ready_s` per row, served from localhost). Live on GitHub Pages: 8,571,055 B wasm over the wire (gzip), 9,209,958 B total, READY 9.6 s on one cold load (`docs/perf/p0-deploy.json`) | — | yes (≤12 MB) |
| iOS smoke | loads: owner opened https://hex-world.pardev.net on iPhone 18 Pro Max (2026-10-13), default single scenario renders, FPS overlay read 60 fps / 16.7 ms / p95 20 ms (owner reading, not load-guarded) | | yes |

Safari detail: Safari 27.0.1 (the brief named 26; 27.0.1 is installed) on macOS, WebGPU adapter `apple | apple | apple` with `timestamp-query`, `texture-compression-bc` and `texture-compression-astc`. Task 10's "stall at frame 3" was Bevy quitting on a WebGPU validation error: Bevy 0.20's PBR fragment shader binds texture and sampler handles with `let`, which needs the WGSL language extension `texture_and_sampler_let`. Chrome implements it and Safari 27.0.1 does not. `web/wgsl-compat.js` rewrites those aliases when the extension is missing, and is inert in Chrome. Root cause, tracer evidence, minimal repro and the draft upstream issue are in `docs/perf/p0-safari-diagnosis.md`. Safari paces at 60 Hz and coarsens timers to 1 ms, so its rows are paced and quantized. Only the built-in Retina display was attached for these runs (canvas 1496×887 CSS at DPR 2). The Darker and Bitwarden extensions stayed installed.

## Decision

Bevy stays, conditionally. In Chrome the instanced shipping layout passes the 16.6 ms uncapped p95 budget by 12–13 ms with SSAO, TAA, bloom and DOF on, including the middle shadow settings that restore strategic shadows (mid600 3.8 / 4.0 ms; mid1200, where all 40,000 pieces cast, 4.3 / 4.3 ms).

The P0 gate is not fully met:

- **Full shadows fail.** p95 is 22.0 / 38.6 ms. Shipping uses a reduced shadow setting, and that is a visual trade-off.
- **The stall tail is unmeasured.** Every uncapped instanced setting has 5–43 frames over 25 ms per 600 (single frames up to 125 ms). p95 hides this for every setting except full shadows. The cause is unknown because there are no GPU timestamps yet.
- **Paced p95 sits a hair over the pre-set 17.0 ms.** At 60 Hz, FP1–FP4 land at 17.3–17.5 ms with means of 16.66–16.69 ms. That is consistent with presentation jitter, but it is still a fail against the criterion.
- **Safari 27 renders only through a workaround.** The frame-3 "stall" was a shader-compile failure: Bevy 0.20 depends on the optional WGSL extension `texture_and_sampler_let`, which Safari 27.0.1 lacks. A feature-gated page shim (`web/wgsl-compat.js`) fixes it with no Bevy fork. With it, on the tuned instanced rows Safari's median mean frame time is 16.67 / 16.69 ms (strategic / close-up), and strategic run 2 had 15 frames over 25 ms. Paced p95 is 17.0 ms strategic (meets 17.0) and 18.0 ms close-up (misses by one 1 ms timer tick). The cause is in Bevy's shaders (and naga's permissive validation, gfx-rs/wgpu#7393), not wgpu or WebKit, and it is fixable, so it does not reopen this decision.

The Chrome evidence answers the per-entity-cost question: Bevy's per-entity cost, not the GPU or Bevy's passes, caused the earlier miss. The instanced layout removes that cost while still rendering inside Bevy's phases.

## Asset route (Q3)

Meshes use the pure-Rust `meshoptimizer-rs` 0.2.1 decoder, which rewrites `EXT_meshopt_compression` GLBs before `bevy_gltf` sees them (`crates/hex-assets/src/meshopt_glb.rs`, `MeshoptGlbPlugin`). Textures ship as BC7/ASTC KTX2 + zstd with a PNG fallback:

- The shipped files are vkFormat 146 (`BC7_SRGB`) and 158 (`ASTC_4x4_SRGB`), with zstd supercompression (scheme 2).
- They load through Bevy's `ktx2` and `zstd_rust` (ruzstd 0.8.3) image features.
- `select_family` (`crates/hex-assets/src/texture_family.rs`) picks BC7, then ASTC, then PNG from the device's compression features.

No Basis transcoder and no `meshopt_decoder.js` are needed.

## Consequences

- **Safari.** Keep `web/wgsl-compat.js` until Bevy drops the `let` handle aliases or Safari ships `texture_and_sampler_let`. Delete it then, and re-check with `?wgsl_compat=0`. File the drafted Bevy issue (owner decision). Any new Bevy upgrade must be smoke-tested in Safari, because Chrome hides this class of bug. Re-time Safari on the 120 Hz monitor and for the mid600, mid1200 and full rows.
- **GPU timestamps.** P1 must add GPU timestamp queries. They are needed to attribute the uncapped stall tail and the paced p95 jitter. Both Chrome and Safari 27 expose `timestamp-query`.
- **Stall tail.** P1/P2 must measure the frames over 25 ms and either fix them or accept them with evidence. p95 alone is not the bar. Instrumented and measured 2026-10-14 (`docs/perf/p2-passes.json`, build `0187620`, gate2000 instanced with mid600 shadows on High at 2560×1440, 3 rounds, load 6.5–11.2 before and after every run, none discarded). Web (uncapped headless Chromium, `apple | metal-3 | Apple M4 Max`): p95 3.4 / 3.8 / 3.5 ms, and 19 / 6 / 9 frames over 25 ms per 600, each 30–67 ms. The histogram is bimodal: 580–591 frames under 8.33 ms, then a cluster at 25–100 ms with nothing between. The stalls are steady state, not warm-up: evenly spaced through the window, one every 24–26 or 46–50 frames. With tilt-shift on they get longer (10–11 per 600, 39–93 ms, mostly in the 50–100 ms bucket). `pipelines_waiting` was 0 around every stall, but that does not rule out compiles if pipeline creation completes within the frame. Native (`--present immediate`): 0 frames over 25 ms in every run (max 22.5–24 ms), and with `gpu_timing=on` no GPU frame over 25 ms (GPU total p50 4.4 ms, p95 under 4.8 ms, against a 7.0 ms mean frame). Native GPU timing says nothing about Dawn's timeline, so the web tail is unattributed. Still open: attribute it in the browser (Chrome tracing, starting from the fixed period), then fix or accept it.
- **GPU timestamps, as built.** Bevy 0.20's render diagnostics give CPU time only on Metal and WebGPU, so `hex_render::gpu_timing` stamps tiny marker compute passes between Bevy's passes instead. On the M4 Max three fixes were needed before the stamps meant "the passes before this marker finished": each marker reads a texel of the texture those passes wrote, it stamps its end (Metal samples a pass's start before its dependency wait), and each frame's queries are resolved a frame later (a query set is not a tracked resource on Metal, so a same-frame resolve raced the last marker). With all three, functional native runs drop no frames and the segments sum to the frame total. The dependencies serialize the passes, so timing-on frames run slower than real ones; budgets are measured with timing off. The UI and upscale passes and the boids dispatch are not timed. In Chrome the dependent markers hung the page, so the web keeps plain start-of-pass markers: their frame total is usable, but their per-pass split is not.
- **Shadows.** Ship a middle setting (start at mid600, all casters, 3 cascades, 600 m). Full 4-cascade, 2,000 m shadows are out of budget.
- **LOD hard cut.** The instanced layout picks LOD per chunk centroid with a hard cut at 150/400 m and has no crossfade. That is a shipping visual gap. It needs per-piece or dithered LOD selection.
- **iOS smoke.** Passed on 2026-10-13: the live site renders on the owner's iPhone 18 Pro Max at 60 fps, with an overlay p95 of 20 ms in the single scenario. The same day the owner opened `?scenario=gate2000&layout=instanced&view=strategic` (default full shadows: all casters, 4 cascades, 2,000 m): the overlay read 60 fps, 16.7 ms, p95 17.0 ms. This is an owner reading at the phone's native resolution, not a load-guarded matrix run.
- **Measurement noise.** The re-run medians differ from the fix-round-1 medians by up to 0.4 ms for passing rows and 7 ms for the full-shadow close-up, on a shared, busy host. Runs 1–2 and run 3 came from two invocations, because the first aborted when the load guard discarded 4 samples in a row.
