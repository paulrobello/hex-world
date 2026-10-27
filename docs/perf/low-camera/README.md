# Low-camera play frame: where the time goes (MiniMac, 2026-10-27)

Card (high): "Low-camera play frame floor ~20.5 ms p95 on High 1440p (317-tile world)", filed 2026-10-21 on the M4 Max. This run measures the per-pass GPU and CPU cost of that frame on MiniMac (base M4), names the biggest contributor, measures what each existing toggle saves, and ranks options for criterion 2 (p95 at or under 16.6 ms on the reference hardware). No rendering behaviour was changed.

## Summary

- **The frame is GPU-bound on the M4 at 30.2 ms** (submit-interval mean, p95 31.1 ms). The engine's main-thread callback is 10.5 ms (p95 11.0). A Metal System Trace of the browser's GPU process shows the GPU busy 30.3 ms of a 31.2 ms frame.
- **The biggest single pass is the early prepass: 14.1 ms of encoder time** (7.0 vertex, 7.1 fragment), 30% of all encoder time. The main opaque pass is next at 10.6 ms, then the four shadow cascades at 7.8 ms together and SSAO at 5.7 ms.
- **The biggest removable cost is the post stack**: SSAO, TAA and bloom, plus the normal and motion-vector outputs they make the prepass write. `post=none` saves 11.6 ms of frame time and 11.3 ms of GPU-busy time. The prepass's encoder time halves, from 14.1 to 6.5 ms.
- **Toggle savings (frame mean, paired within a round):** post off -11.6 ms, Low tier -11.4, Medium tier -10.7, 1920x1080 -6.2, shadows off -4.1, grass off -3.9, 2 cascades -1.0.
- **On the reference M4 Max this frame is probably already inside the budget.** It has four times the M4's GPU cores, so the 30 ms GPU frame scales to about 8 ms, under the 10.5 ms main thread. That puts the frame near 11 ms. A load-guarded M4 Max re-measure (O1 below) is the next step for criterion 2. No timing on macdaddy was evidence today (its load was 13 to 185 during this session).

## Setup

- **Host.** MiniMac (Mac16,10, Apple M4 with a 10-core GPU, macOS 27), booked 09:00-10:30Z. Renderer `apple | metal-3 | Apple M4`, Chrome for Testing (Playwright chromium 1243), uncapped (`--disable-gpu-vsync --disable-frame-rate-limit`), WebGPU developer features on.
- **Build.** `main` at `cf5be40`, `make build-web` and `cargo build -p hex-app --release` on macdaddy, copied to MiniMac. Harness changes are on this branch.
- **World and camera.** The 317-tile radius-10 water world of `e2e/water-world.mjs`, play scenario, High tier, grass 100%, tilt-shift off, seed 1, 2560x1440 at DPR 1, camera at (10, 12, 40) looking at the origin, Settings closed and the canvas focused.
- **Web harness.** `e2e/frame-pacing.mjs` with the new `lc_*` arms: one toggle per arm, one fresh browser per arm, two 5 s windows per arm (averaged). Each row records the submit intervals (one per engine frame), the engine callback's duration (`cpu`, the main-thread time of each callback that submitted GPU work), and a diag census: tier, shadows, cascades, post, SSAO, grass blades, draws per pass, kit draws and `gpu` timing. Each arm checks that its toggle took effect before it counts.
- **Native harness.** `e2e/perf-low-camera-native.mjs`: the same world, camera and arms, natively, with `--gpu_timing on` for the per-pass stamps.
- **Rounds.** `scripts/perf/low-camera-minimac.sh`: 3 rounds, each a web round and then a native round, with the arm order rotated by one each round. Every run was inside the load guard (`e2e/perf-load.mjs`, 1-minute load at most 15 before and after). Load was 2.26 to 4.36 before and after every one of the 54 runs, and none was discarded.
- **GPU traces.** `scripts/perf/low-camera-trace.sh` records a Metal System Trace (`xctrace`, 12 s) inside a 25 s window of one arm. For the web it adds Chrome's `--enable-dawn-features=use_user_defined_labels_in_backend`, so the Metal encoders carry Bevy's pass labels. `scripts/perf/metal-encoders.py` then turns the trace's `metal-gpu-intervals` table into GPU time per encoder per frame. Trace loads: web base 2.1 -> 5.0, web post off 8.4 -> 8.5, native 6.9 -> 11.3.

Files:

- `results-2026-10-27.json`: every row, with per-arm medians and paired deltas (`e2e/perf-low-camera-summary.mjs`).
- `minimac-2026-10-27/`: the raw `web-<n>.json` and `native-<n>.json` rounds and `run.log`.
- `minimac-2026-10-27/traces/`: the per-encoder tables (`*-encoders.json`) and each traced window's pacing.

## Per-pass GPU cost (web, Metal System Trace)

This is the browser GPU process for the base arm and the `post=none` arm, as GPU time per engine frame. Apple GPUs run the vertex and fragment work of neighbouring passes at the same time, so encoder times add up to more than the frame: the encoder sum is 46.7 ms against 30.3 ms of GPU-busy time. Read each row as that pass's share of the work, not as milliseconds you get back by deleting it. The toggle table below gives the frame-time savings. The trace slows the frame by about 2.5 ms (31.2 ms traced, against 28.7 ms in the same page's untraced window).

| Pass | Draws (base) | Vertex ms | Fragment ms | Compute ms | Base ms | Share of encoder time | `post=none` ms |
|---|---|---|---|---|---|---|---|
| Early prepass (depth, plus normals for SSAO and motion vectors for TAA) | 214 (169 + 45 alpha-mask) | 7.01 | 7.10 | | **14.12** | 30% | 6.45 |
| Main opaque pass | 218 (173 + 45 alpha-mask) | 5.04 | 5.51 | | 10.55 | 23% | 10.28 |
| SSAO (preprocess depth, SSAO, spatial denoise) | | | | 5.68 | 5.68 | 12% | 0 |
| Shadow cascade 0 | 81 | 0.27 | 1.58 | | 1.84 | 4% | 2.22 |
| Shadow cascade 1 | 167 | 1.03 | 1.40 | | 2.43 | 5% | 2.21 |
| Shadow cascade 2 | 225 | 1.70 | 1.56 | | 3.27 | 7% | 3.04 |
| Shadow cascade 3 | 80 | 0.12 | 0.16 | | 0.28 | 1% | 0.24 |
| Transmission (refracting water) | | 0.23 | 1.50 | | 1.73 | 4% | 1.77 |
| TAA | | | 0.92 | | 0.93 | 2% | 0 |
| Bloom (4 passes) | | 0.06 | 0.74 | | 0.80 | 2% | 0 |
| Tonemapping | | | 0.34 | | 0.35 | 1% | 0 |
| Upscaling | | | 0.30 | | 0.30 | 1% | 0.24 |
| Clustering, mesh preprocessing, boids | | | | 0.17 | 0.19 | 0% | 0.26 |
| Unlabelled (Dawn internal passes and the browser's compositing) | | 0.20 | 2.81 | 1.10 | 4.11 | 9% | 3.71 |
| **Encoder sum** | 985 | | | | **46.66** | | 30.47 |
| **GPU busy (union of intervals)** | | | | | **30.32** | | 18.97 |
| Frame interval during the trace | | | | | 31.22 | | 19.49 |

Draws are the diag census of the base arm (identical in all three rounds; `census.passes` in `web-<n>.json`; 849 of them are kit draws over 132 slots). Cascade 2 has both the most shadow draws and the costliest shadow pass. Each column is a single trace of one arm. The traces ran outside `e2e/perf-load.mjs`, but each had a 1-minute load at or below 15 before and after (listed under Setup), which meets the load rule.

The prepass is the biggest single pass, and half of it belongs to the post stack. TAA requires a motion-vector prepass and SSAO a normal prepass. The water needs only depth. Without them (`post=none`) the prepass falls from 14.1 to 6.5 ms. Its vertex time falls the most, from 7.0 to 2.8 ms: motion vectors run each vertex for the previous frame's position too, including the grass wind. The grass draws into the prepass and the opaque pass, which is why grass off saves 3.9 ms of frame time although grass has no pass of its own.

## Toggle savings (web, 3 rounds)

Median over 3 rounds, with the paired delta (arm minus the base arm of the same round) as median [min, max]. `cpu` is the engine callback's main-thread time per frame. Engine frames are per 5 s window. Draws are the diag census over the camera's phases and the four cascades.

| Arm | Submit mean ms | p50 | p95 | CPU mean (p95) ms | Delta mean ms | Delta p95 ms | Engine frames / 5 s | Draws |
|---|---|---|---|---|---|---|---|---|
| base (High, grass 100%) | **30.22** | 30.4 | **31.05** | 10.53 (10.95) | | | 167 | 985 |
| `post=none` (no TAA, SSAO, bloom) | 18.62 | 18.8 | 19.65 | 8.95 (9.4) | **-11.60** [-11.61, -11.19] | -11.35 | 270.5 | 999 |
| `tier=low` | 18.66 | 18.25 | 19.75 | 9.09 (9.5) | -11.39 [-11.57, -11.10] | -11.30 | 269.5 | 981 |
| `tier=medium` | 19.45 | 19.45 | 20.95 | 9.17 (9.55) | -10.67 [-10.78, -10.14] | -9.95 | 259 | 981 |
| 1920x1080 viewport | 23.88 | 24.0 | 25.1 | 9.68 (10.0) | -6.15 [-6.35, -4.86] | -5.55 | 211 | 999 |
| `shadows=off` | 26.05 | 26.0 | 27.1 | 8.77 (9.25) | -4.12 [-4.17, -3.71] | -3.90 | 193.5 | 432 |
| `grass=0` | 26.31 | 26.0 | 27.75 | 10.11 (10.55) | -3.90 [-3.92, -2.58] | -3.30 | 192 | 967 |
| `cascades=2` | 29.10 | 29.35 | 30.05 | 9.96 (10.35) | -1.05 [-1.13, -0.01] | -0.90 | 173 | 801 |
| `gpu_timing=on` (cost of the stamps) | 32.72 | 32.7 | 33.4 | 8.85 (9.3) | +2.62 [+2.50, +2.71] | +2.35 | 154 | 985 |

- Every arm stays GPU-bound: the CPU callback is 8.8 to 10.5 ms against frames of 18.6 ms and up. No single toggle brings the M4 under 16.6 ms. The closest are `post=none` and Low at about 19.7 ms p95.
- What each tier step removes: High to Medium drops SSAO (and so the normal prepass), grass (Medium draws none), the water's high-quality path (screen-space reflection march, second caustic layer, 8 waves) and half the boids. Medium to Low drops the water's refraction (no transmission pass) and halves the boids again (this world runs 20 cars at every tier). So Low minus Medium (-0.7 ms) bounds the refraction. Medium minus grass off (-6.8 ms) estimates SSAO, its normal prepass and the water's high-quality path together. `post=none` minus that leaves about 4.8 ms for TAA, its motion-vector prepass and bloom. These two splits come from arms with overlapping changes, so treat them as estimates.
- The 1920x1080 viewport stands in for a render-scale option, which the game does not have. It has 56% of the pixels and saves 20% of the frame, so about half the frame does not scale with resolution: shadow maps, vertex work and the prepass's vertex half.
- `cascades=2` saves little here. With two cascades, the second one covers what cascades 1 to 3 did (draws 985 to 801).
- `shadow_casters=large` was dropped from the arm list. It does not apply to play's instanced kit: draws and kit draws were identical to the base arm in the dry run on macdaddy.

## Native results and why the native split is not used

The native rounds ran too (`native-<n>.json`, 3 rounds, same arms), but neither their frame times nor their `gpu_timing` split can attribute cost on this host:

- **Native frame time is pinned at 60 Hz.** MiniMac's display runs at 60 Hz, and every native arm reads p50 16.6-16.7 ms with `--present immediate`. A Metal System Trace of the native base arm shows the GPU busy 16.55 ms of each 16.68 ms frame (the table divides the window by the 16.667 ms display interval, so the frame interval there is an input, not a measurement). A GPU paced at 60 Hz can clock down until its work fills the interval, so 99% busy does not show saturation: the native GPU frame is at most about 16.5 ms, against 30.3 ms for the same frame through Chrome and Dawn. That gap of at least 1.8x is a finding of its own (see O7).
- **The native `gpu_timing` stamps lose the end of the frame on the M4.** In every arm, `opaque`, `transmission`, `taa` and `post` read 0 to 0.06 ms, so the stamped total (11.7 ms in the base arm) misses the main pass. Cost also moves between segments from arm to arm: with shadows off, `prepass` reads 9.7 ms against 2.7 ms in the base arm. On the M4 Max the same build reads non-zero opaque and transmission segments. The stable reading is `ssao` at 3.6 ms in every arm that has it; the native trace's compute-labelled SSAO encoders read 3.7 ms.
- **The native trace's encoder labels are misassigned on MiniMac** (`xctrace` 26.0): fragment time is booked to compute-labelled encoders such as `clustering count pass`, and the main opaque pass has no label. So only the native busy total is reported. The web trace's labels are consistent: compute time lands only on compute passes, and each pass has the channels it should.

The web `gpu_timing` frame total (20.0 ms in the `gpu_timing=on` arm) also under-reads. Its span runs from preprocessing to tonemapping, so it leaves out upscaling (0.3 ms) and the unlabelled browser and Dawn passes (about 4.1 ms). The in-span work in the trace is about 26 ms of the 30.3 ms GPU-busy time, so the stamps miss roughly a quarter of it. Neither platform's stamps give the frame cost on the M4. Use a Metal System Trace for per-pass numbers on Apple hardware.

## M4 against the M4 Max (the reference)

PRD §10 sets the budget on the reference desktop, "Apple M-series, High tier, 1440p". MiniMac is the slowest M-series machine available: a base M4 with 10 GPU cores and about 120 GB/s of memory bandwidth. macdaddy is an M4 Max with 40 GPU cores (`system_profiler`) and about 4.5 times the bandwidth. The two have about the same single-core CPU speed.

- **GPU.** At 4 times the cores and about 4.5 times the bandwidth, the M4's 30.3 ms GPU frame is about 7 to 9 ms on the M4 Max.
- **CPU.** The engine callback (10.5 ms, single-threaded wasm) carries over at about the same time.
- **So on the M4 Max this frame should be CPU-bound at about 10.5 to 12 ms,** below the 16.6 ms budget. This is consistent with the readings below, none of which is evidence because each ran above the load limit:
  - `docs/perf/frame-pacing/README.md` measured this world and camera at a submit mean of 12.6-13.8 ms on macdaddy, at load 25-34, unpaired.
  - Today's labelled web trace on macdaddy (load 13-19) read an 11.25 ms frame with the GPU busy 8.05 ms.
- **The card's 20.5 ms p95 predates the close-camera draw cuts** (instanced slot coalescing, screen-aware and per-cell cascade skips), which lowered the draws this frame issues. The frame-latency limiter fix (`e88b1bd`) does not apply here: at 1440p the M4 Max is CPU-bound and its frames do not pair (`docs/perf/frame-pacing/README.md`). The card's reading may also have been taken under host load, which lengthens a CPU-bound frame.
- **CPU per arm.** The callback times differ by 1 to 1.7 ms between arms, but the `gpu_timing=on` arm, which does more work than the base arm, reads 1.7 ms less. Treat those differences as spread, not as CPU savings.

## Options for criterion 2, ranked

Savings are measured on the M4 unless marked as an estimate. On the M4 Max the GPU savings scale down by about 4 times, and they stop showing in frame time once the GPU frame is under the CPU frame.

| # | Option | Expected saving | Look cost |
|---|---|---|---|
| O1 | **Re-measure on the reference M4 Max under the load guard** (this harness, `PACING_ARMS=lc_base`, with macdaddy at load 15 or below), then close criterion 2 or set a per-camera budget from the result. If it comes back over budget, the frame there is CPU-bound, so the lever is the 10.5 ms main thread, most directly the draws: shadows are 553 of the 985 (`shadows=off` cuts them to 432, `cascades=2` to 801) | None needed if the estimate above holds (about 11 ms, CPU-bound) | None |
| O2 | **Cheaper SSAO**: Bevy's lower SSAO quality level or half-resolution AO on High, or SSAO only from Ultra | Up to about 6.8 ms on the M4 if SSAO goes (5.7 ms of compute plus the normal prepass; estimate from Medium minus grass off). A lower quality level saves part of the 5.7 ms. Estimate 1.5-2 ms on the M4 Max | Softer or missing contact darkening under houses, trees and props. Lower levels are noisier |
| O3 | **Drop TAA's motion vectors**: SMAA or FXAA instead of TAA, so the prepass writes no motion vectors | Up to about 4.8 ms on the M4, together with bloom (estimate from `post=none` minus the SSAO estimate); SMAA itself costs back about 0.5-1 ms | More shimmer on grass blades and thin edges; TAA's temporal smoothing of the wind animation is lost |
| O4 | **Render scale** (a new setting: render at 75% and upscale) | 6.2 ms on the M4 at 1920x1080 (measured with the viewport) | A softer image. This would be a new product feature |
| O5 | **Shadows**: lower cascade 2's resolution or shadow distance at low cameras (cascade 2 is the costliest at 3.3 ms of encoder time) | Up to 4.1 ms with shadows off. `cascades=2` saves only 1.0 ms | Off is a large look loss. A cheaper far cascade costs softer or popping far shadows |
| O6 | **Grass**: lower density on slower GPUs, or keep grass out of the motion-vector prepass | 3.9 ms at 0%. Density scales the blades drawn, so 50% should save about half (estimate) | Sparser meadows. Grass without motion vectors ghosts under TAA |
| O7 | **Close the web-to-native gap**: the same frame takes at most about 16.5 ms of GPU natively and 30.3 ms through Chrome and Dawn. The prepass fragment work (7.1 ms web) and the unlabelled 4.1 ms of browser and Dawn passes are where to look first | Unknown until a Dawn-side trace splits it | None |
| O8 | **Auto tier on slow GPUs**: step base-M4-class GPUs to Medium | 10.7 ms on the M4 (to 20.95 ms p95, still over the budget there alone) | Medium's look: no SSAO, no grass, simpler water |

O2 and O3 together cover most of `post=none`'s 11.6 ms, and on the M4 they would still leave the frame at about 19 ms. Getting the base M4 itself under 16.6 ms at this camera needs one of them plus O4 or O5. On the reference M4 Max, O1 comes first: if the re-measure confirms about 11 ms, the remaining options are for slower M-series GPUs, not for criterion 2.

## M4 Max result (2026-10-27, criterion 2)

The reference re-measure (O1) ran on macdaddy (Apple M4 Max) at 10:29-10:37Z. It used `PACING_GUARD=1 PACING_ARMS=lc_base node e2e/frame-pacing.mjs dist` on `main` at `e9e0b8d`, with the same world, camera, High tier, grass 100%, 2560x1440 at DPR 1 and uncapped Chromium. There were 3 rounds, with a 1-minute load of 8.9 to 14.3 before and after each (`m4max-2026-10-27/lc-base-<n>.json`).

| Round | Frames | Submit mean ms | p50 | p95 | CPU mean (p95) ms | Load before -> after |
|---|---|---|---|---|---|---|
| 1 | 467 | 10.72 | 10.7 | 11.2 | 9.83 (10.3) | 14.33 -> 10.99 |
| 2 | 460 | 10.89 | 10.9 | 11.5 | 10.08 (10.7) | 10.99 -> 8.94 |
| 3 | 458 | 10.93 | 10.9 | 11.4 | 10.04 (10.6) | 8.94 -> 9.42 |

The frame is CPU-bound at about 10.9 ms, as estimated above: the engine callback is 9.8-10.1 ms of it. p95 is 11.2-11.5 ms, inside the 16.6 ms budget, so criterion 2 is met on the reference hardware. The card's 20.5 ms predates the close-camera draw cuts. The M4 options above (O2-O8) remain for slower M-series GPUs.

## Reproduce

On the build host (any load): `make build-web`, `cargo build -p hex-app --release`, then copy `dist/`, `e2e/` (without `node_modules`), `web/ui/src/`, `assets/`, `target/release/hex-world` and `scripts/perf/` to `minimac:perf/hex-world-lowcam/`, and run `npm ci` in its `e2e/`. On MiniMac, inside a booked window:

```bash
cd perf/hex-world-lowcam
bash scripts/perf/low-camera-minimac.sh 3 1                        # 3 rounds -> results/web-<n>.json, native-<n>.json
bash scripts/perf/low-camera-trace.sh web lc_base traces/web-base  # per-encoder table -> traces/web-base-encoders.json
bash scripts/perf/low-camera-trace.sh web lc_post_none traces/web-post-none
bash scripts/perf/low-camera-trace.sh native "" traces/native-base # needs target/shots/lc-native-script.json from a native round
node e2e/perf-low-camera-summary.mjs results results.json
```

A trace needs about 1.5 GB of disk while it is open. The script deletes it once the table is written.
