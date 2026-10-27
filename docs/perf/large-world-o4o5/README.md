# Large world: the O5 and O4 regressions (MiniMac, 2026-10-27)

Card: "Large world: O5 cascade skip and O4 slot coalescing each regress the 2,066-tile world alone". The optional per-change arms of `docs/perf/close-camera-draws/README.md` (3 rounds, 2026-10-26) showed two regressions on the 2,066-tile save. O5 (the screen-aware cascade skip, `b37f0bb`) cost +0.92 ms of web mean at the overview. O4 (runtime slot coalescing, `4b90a09`) added 1.3 to 1.4 ms of web p95 on the 2,066-tile world.

## Summary

- **Both regressions reproduce in the historical builds**, with 5 load-guarded rounds and the arm order rotated each round.
- **O4's p95 rise was a frame-pacing artifact, not extra work.** The web frame-latency limiter of that time ran GPU-bound frames in back-to-back pairs. A pair is one short frame (one frame's main-thread time) and one long frame (two GPU frames minus the short one). O4 cuts the main thread by 0.7 to 1.3 ms a frame, so the short frame got shorter and the long frame, which is the p95, got longer by about the same amount. The mean did not move. With only `web/frame-latency.js` replaced by main's (the pairing fix, `e88b1bd`), the same O4 wasm shows no p95 change.
- **O5's cost was GPU time, from lost pass overlap in the shadow cascades.** At the overview, O5 empties the kit's casters out of cascades 0 and 1. That cuts 2.1 ms of encoder time, but the GPU is busy 1.09 ms longer per frame. Cascade 2 used to overlap cascade 1's work, and without it, cascade 2's vertex and fragment work ran exposed: +1.64 ms vertex and +1.07 ms fragment of encoder wall time (Metal System Trace). The main thread got 0.3 to 0.5 ms faster. The regression did not depend on the limiter: +1.06 ms with the fixed limiter too.
- **On main neither change regresses any more, and both draw cuts are kept.** A new runtime flag turns each change off in one build: `kit_ablate=cascade_clip` (O5 off) and `kit_ablate=coalesce` (O4 off). The comparison is therefore free of whole-module differences. Over 5 rounds, turning either one off makes no camera faster:
  - O5 off: overview mean +0.16 ms [+0.13, +1.20]. That is within noise by the rule below, but all 5 rounds favour O5, and in a trace O5 on is 0.35 ms less GPU-busy time than O5 off.
  - O4 off: within ±0.03 ms of mean at both cameras, with 0.4 to 0.6 ms more main-thread time.
  - Changes already on main fixed the regressions. For O4 that is the limiter fix `e88b1bd`, measured on its own. For O5 the measured fact is that main's cascade 1 holds little kit work for O5 to remove: with O5 off it has 48 draws and 0.19 ms of encoder time, against 174 draws and 3.5 ms in the historical before build. The likely cause is the per-cell cascade skip (`f1aff3e`), which halved the kit's cascade indices at this overview. O6 (per-tile culling) also landed in between, and no run here separates the two.

No fix was needed, so none was made. The only rendering code change is the off-by-default ablation flag, and `make thumbs` run twice changed no thumbnail.

## Setup

- **Host.** MiniMac (Mac16,10, Apple M4 with a 10-core GPU, macOS 27), booked 12:15-14:45Z. All runs took place 12:15-13:28Z.
- **Renderer.** `ANGLE (Apple, ANGLE Metal Renderer: Apple M4, Unspecified Version)`, WebGPU adapter `apple | metal-3`.
- **Browser.** Headless Chromium (Playwright), uncapped, 2560x1440 at DPR 1.
- **Harness.** `e2e/perf-close-camera.mjs` with `WORLDS=world2066 ROUNDS=5 WARMUP=1`, on the 2,066-tile save (`docs/perf/post-arc001-2026-10-16/local-world.hexworld`). Settings: play, High, grass off, tilt off, seed 1, photo mode with DOF off, pointer off the world. Two cameras: close (10, 12, 40) looking at the origin, and overview (20, 160, 150) looking at (20, 0, -5). Each window is read 700 frames after the camera moves.
- **Harness additions.**
  - It now takes any number of `name=dist?keys` arms, rotated by one each round.
  - One discarded warm-up run goes first. The cache run's first run of a session read about 2 ms fast.
  - Every camera window records `cpu`: the main-thread time of each engine animation-frame callback that submitted GPU work, over the window's last 600 frames, the same frames as the diag ring.
  - It also records the stall histogram.
- **Load guard.** `e2e/perf-load.mjs` required a 1-minute load of 15 or less before and after every run. No sample was discarded or failed, and the console logged no error.
- **Builds.**
  - Main: `9df8b8b` (`15ea1d1` plus the `kit_ablate` flag), built with `make build-web` on macdaddy.
  - Historical arms: `29a67fb` (before), `b37f0bb` (O5) and `4b90a09` (O5 + O4), each built from `git archive` with its workspace crates cleaned. A first attempt reused the newer tree's crates, because the archives' sources carried commit-time mtimes older than the cloned `target/`. The builds measured here touched the sources first, so every workspace crate was recompiled.
  - "Fixed limiter" arms: the same three dists with `web/frame-latency.js` replaced by main's file. The wasm is unchanged. Main's file also carries the frame-rate cap wrapper (owner A29), which passes every frame through while the cap is Off, as it is here; that is the only other difference.
- **Traces.** `scripts/perf/large-world-trace.sh` runs one arm with Dawn's user-defined labels. It holds each camera 12 s after its window and records a 4 s Metal System Trace inside the overview's hold. `scripts/perf/metal-encoders.py` reduces the last 3 s to GPU time per encoder per frame, and `scripts/perf/metal-encoders-diff.py` compares two tables. Each `.ktrace` was deleted once its table was written.

Deltas are paired within a round, given as the median [min, max] over the 5 rounds. "Real" follows the close-camera-draws rule: every paired delta has the same sign, and the median is larger than their spread.

## Results

### Historical builds, the limiter of that time (`historical-web.json`, loads 2.17-4.45)

| Camera | Change | Draws | Mean ms | Mean delta | p95 ms | p95 delta | Main-thread `cpu` delta |
|---|---|---|---|---|---|---|---|
| overview | O5 (`b37f0bb` - `29a67fb`) | 1,426 -> 1,252 | 42.31 -> 43.34 | **+1.07** [+0.67, +1.21] | 65.6 -> 68.2 | **+2.6** [+2.3, +2.8] | **-0.52** |
| close | O5 | 2,264 -> 2,264 | 38.28 -> 38.35 | +0.01 | 53.5 -> 53.6 | +0.2 | +0.12 |
| close | O4 (`4b90a09` - `b37f0bb`) | 2,264 -> 1,863 | 38.35 -> 38.38 | 0.00 [-0.13, +0.55] | 53.6 -> 54.8 | **+1.4** [+0.7, +1.8] | -0.93 [-2.24, -0.49] |
| overview | O4 | 1,252 -> 990 | 43.34 -> 43.41 | +0.18 [-0.52, +0.47] | 68.2 -> 69.6 | +1.2 [+0.3, +2.2] | **-1.29** |

The frames are paired: 300 to 326 of each window's 600 frames are over 25 ms, with p50 21-31 ms against a mean of 38-43 ms. This reproduces the 2026-10-26 optional arms: O5 +0.92 mean and +2.5 p95 at the overview, and O4 +1.3 to 1.4 p95.

### The same wasm with main's frame-latency limiter (`historical-fixed-limiter-web.json`, loads 2.00-3.15)

| Camera | Change | Mean ms | Mean delta | p95 delta | Main-thread `cpu` delta |
|---|---|---|---|---|---|
| overview | O5 | 42.22 -> 43.29 | **+1.06** [+1.02, +1.11] | **+0.9** [+0.8, +1.6] | **-0.30** |
| close | O5 | 38.20 -> 38.19 | -0.01 | +0.1 | 0.00 |
| close | O4 | 38.19 -> 38.29 | +0.10 [+0.09, +0.12] (real by the rule, 0.26%) | +0.1 [0.0, +0.3] | **-0.68** |
| overview | O4 | 43.29 -> 43.32 | +0.01 | 0.0 | **-0.56** |

Frames are unimodal here: p50 is within 0.3 ms of the mean, and all 600 frames are in the 33-50 ms bucket. O4's p95 rise is gone. At the close camera a 0.1 ms mean rise remains, which is too small for a trace to resolve. O5's mean cost is unchanged, so it is real work. Its p95 delta now equals its mean delta, as it should for one unimodal mode.

Why the pairing turns a CPU saving into a p95 rise: in a pair, the short interval is the second frame's main-thread time, and the long interval is two GPU frames minus it (`docs/perf/frame-pacing/README.md`). With the GPU frame G unchanged, a main-thread saving of x shortens the short frame by x and lengthens the long one by x. That moves the p95 up and leaves the mean (G) where it was.

O5 also fits that model. Its p95 rise of +2.6 ms is about 2 x 1.07 (its rise in G) plus 0.52 (its main-thread saving).

### O5 at the overview, per pass (Metal System Trace, fixed-limiter builds, `traces/o5-historical-overview-diff.json`)

| | before (`29a67fb`) | O5 (`b37f0bb`) | Delta |
|---|---|---|---|
| Frame interval ms | 42.19 | 43.29 | +1.10 |
| **GPU busy ms per frame** | **41.85** | **42.94** | **+1.09** |
| Encoder sum ms per frame | 60.46 | 58.36 | -2.10 |
| Cascade 1 (vertex + fragment) | 0.62 + 2.89 | 0.02 + 0.00 | -3.49 |
| Cascade 2 vertex | 2.61 | 4.25 | +1.64 |
| Cascade 2 fragment | 2.54 | 3.61 | +1.07 |
| Early prepass fragment | 3.83 | 1.90 | -1.93 |
| Main opaque fragment | 7.85 | 8.27 | +0.42 |
| SSAO compute | 8.35 | 8.62 | +0.27 |

Each column is a single 3 s window (71 and 69 frames). Run loads were 2.05 -> 2.42 and 3.48 -> 3.08, with 6.04 just after the O5 trace. Apple GPUs run the vertex and fragment work of neighbouring passes at the same time, so an encoder's time is wall time shared with its neighbours, and only GPU busy (the union of intervals) is cost.

The diag census gives the overview's cascade draws as 0 / 174 / 342 / 376 (cascades 0-3) before and 0 / 0 / 342 / 376 with O5: cascade 2 draws exactly what it did. O5 removed all of cascade 1's work and 2.1 ms of encoder time. The GPU was still busy 1.09 ms longer, matching the +1.06 ms frame delta. Cascade 2, the far cascade that holds most of the world, used to run its vertex work while cascade 1's fragment work occupied the GPU. With cascade 1 empty, the cascades ran back to back and cascade 2's vertex and fragment time grew by 2.7 ms, with the same draws, so the growth is scheduling, not work. The prepass's fragment time moved the other way, because its overlap changed as well.

### Main with each change turned off (`ablate-web.json`, loads 2.56-3.17)

| Camera | Arm | Draws (kit) | Mean ms | Mean delta vs main | p95 ms | p95 delta | Main-thread `cpu` ms (delta) |
|---|---|---|---|---|---|---|---|
| close | main | 1,422 (1,287) | 30.42 | | 31.9 | | 29.16 |
| close | O5 off | 1,422 (1,287) | 30.43 | -0.02 [-0.15, +0.45] | 31.9 | 0.0 [-0.3, +1.6] | 29.29 (+0.01) |
| close | O4 off | 1,477 (1,342) | 30.39 | -0.03 [-0.18, +0.47] | 31.1 | -0.8 [-1.2, +1.9] | 29.67 (+0.56 [+0.11, +0.81]) |
| overview | main | 930 (849) | 38.89 | | 40.3 | | 25.64 |
| overview | O5 off | 978 (897) | 39.05 | +0.16 [+0.13, +1.20] | 40.6 | +0.3 [+0.2, +1.3] | 26.07 (+0.35) |
| overview | O4 off | 1,061 (980) | 38.86 | -0.01 [-0.06, +1.02] | 40.1 | -0.2 [-0.5, +0.9] | 26.07 (+0.44) |

- At the close camera `cpu` is 29.2 of the 30.4 ms frame, but the frame is GPU-bound: cutting 13 ms of main-thread time later moved its mean by 0.08 ms (`docs/perf/large-world-cpu/`). At the overview it is GPU-bound too: `cpu` is 25.6 of 38.9 ms.
- Turning O5 off is slower at the overview in all 5 rounds, though its spread keeps it short of "real". Turning O4 off costs 0.4 to 0.6 ms of main-thread time, which shows at neither camera's mean because neither frame is limited by that time.
- A trace of main at the overview (`traces/o5-main-overview-diff.json`, loads 3.33 -> 3.03 and 3.19 -> 3.41) puts O5 on at 38.36 ms GPU busy, against 38.71 ms with O5 off: O5 saves 0.35 ms.
  - With O5 off, cascade 1 holds 48 draws and 0.19 ms of encoder time, against 174 draws and 3.5 ms in the historical before build. Cascade 2 holds 261 draws, against 342 (cascade 3: 231 against 376).
  - So emptying cascade 1 no longer exposes a large cascade 2. The likely cause is the per-cell cascade skip, which cut the kit's cascade indices at this overview from 21.7 M to 11.1 M (PRD §9.1). O6 also landed in between, and these runs do not separate the two.

## Criteria

1. **Root cause measured.**
   - O4: CPU side, as frame pacing. O4 saves main-thread time, and the old limiter's frame pairs turned that saving into a longer long frame (p95), with no extra GPU work. It is removed by the limiter fix alone, on the same wasm.
   - O5: GPU. Emptying cascades 0 and 1 cost cascade 2 its overlap, so GPU-busy time rose 1.09 ms while the encoder sum fell 2.1 ms. The main thread was faster.
2. **No regression on main with the draw cuts kept.** No new fix landed: the regressions were removed by commits already on main, `e88b1bd` (limiter, measured) and most likely `f1aff3e` (per-cell cascade skip, inferred from cascade 1's content, see above). MiniMac before and after (each change off, then on, in one build, 5 rounds) shows no camera slower with either change on. Draws stay cut: overview 1,061 -> 930 with O4 and 978 -> 930 with O5, close 1,477 -> 1,422 with O4.

Native runs were not timed. Both regressions were measured on the web. The close-camera native harness would also need `--offscreen on` on MiniMac's locked screen, or its window draws no main pass (`docs/perf/low-camera/README.md`).

## Reproduce

Build `dist/` on the build host. To build a historical arm, extract `git archive <sha>` into a directory, clone `target/` and `web/ui/node_modules` into it, touch `crates/`, `assets/shaders` and `data/`, then `cargo clean -p` the seven workspace crates for `wasm32-unknown-unknown` / `wasm-release` and run `make build-web`.

Copy `dist/` (and each arm's dist), `e2e/` (without `node_modules`), `web/ui/src/`, `scripts/perf/` and the save to `minimac:perf/<dir>/`, then run `npm ci` in `e2e/`. Inside a booked window, run:

```bash
ulimit -n 10240
WORLDS=world2066 ROUNDS=5 WARMUP=1 caffeinate -d -u node e2e/perf-close-camera.mjs \
  base=dist "no_o5=dist?kit_ablate=cascade_clip" "no_o4=dist?kit_ablate=coalesce" results/ablate-web.json
WORLDS=world2066 ROUNDS=5 WARMUP=1 caffeinate -d -u node e2e/perf-close-camera.mjs \
  before=old-29a67fb/dist o5=old-b37f0bb/dist o5o4=old-4b90a09/dist results/historical-web.json
bash scripts/perf/large-world-trace.sh before=fl-29a67fb/dist overview traces/fl-before-overview
node e2e/perf-close-camera-summary.mjs results/ablate-web.json base results/ablate-web-summary.json
python3 scripts/perf/metal-encoders-diff.py traces/a-encoders.json traces/b-encoders.json --out diff.json
```

Afterwards, delete the `instruments*.ktrace` files that `xctrace` leaves in `$TMPDIR`. Each is about 270 MB.
