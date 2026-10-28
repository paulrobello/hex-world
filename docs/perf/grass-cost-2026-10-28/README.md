# Grass and foliage sway: frame cost in play on the 2,066-tile world (MiniMac, 2026-10-28)

PRD §15 P2, "Grass blades and wind sway": "Still open: a load-guarded frame-cost measurement of both in play". Grass card criterion 4: "2,000-tile world on High holds p95 <= 16.6 ms native and web, load-guarded, before/after numbers recorded".

## Summary

- **p95 <= 16.6 ms does not hold on MiniMac, and it fails before grass or sway is added.** With grass off, the 2,066-tile world at the low camera reads a 31.3 ms mean and 32.7 ms p95 on the web and 36.5 ms and 40.7 ms natively. Every one of the 600 window frames is over 25 ms in every run, on both platforms. The frame is GPU-bound: in the paired close-camera run (`../close-camera-regression-2026-10-28/`) main's main thread takes 16.9 ms of a 31.5 ms frame.
- **Grass cost, Normal (the default) against off:** +4.22 ms mean and +4.7 ms p95 on the web (31.34 to 35.48 ms, p95 32.7 to 37.3), +6.08 ms mean and +5.17 ms p95 natively (36.51 to 42.57 ms, p95 40.65 to 45.92). Cost is linear in drawn blades: about 2.1 ms (web) and 3.0 ms (native) per 50% of density, so Lush costs +6.4 ms (web) and +9.1 ms (native).
- **Foliage sway cost (web, ablation build, grass at Normal):** +3.69 ms mean and +3.85 ms p95 at the close camera (31.92 to 35.56 ms), +1.29 ms mean and +1.55 ms p95 at the overview (38.92 to 40.10 ms). Draws are identical with and without sway, and main-thread time does not change, so this is GPU vertex work in the kit passes (forward, prepass and shadow cascades).
- Every delta above is real by the close-camera-draws rule, and every run sat inside the load guard (MiniMac 1-minute load 3.02-5.95 before and after, no sample discarded, no run failed).

## Setup

- **Host.** MiniMac (Mac16,10, Apple M4 with a 10-core GPU), booking `01a180eb98f374759ee80384d84184e8`, 06:15-07:45Z. All runs took place 06:16-07:04Z (this directory and `../close-camera-regression-2026-10-28/` together), and `~/perf/gc` was deleted at 07:04Z with no test browser, `caffeinate` or `hex-world` process left running. MiniMac is the slowest M-series machine available. PRD §10 sets the budget on "Apple M-series, High tier, 1440p", and the M4 Max dev host has 4 times its GPU cores, so MiniMac is the conservative reading.
- **Build.** main `a543d47`, built on macdaddy (`make build-web` from `git archive`, see `../close-camera-regression-2026-10-28/build-arm.sh`; `cargo build -p hex-app --release` for native) and copied to MiniMac.
- **World.** The exact 2,066-tile save `docs/perf/post-arc001-2026-10-16/local-world.hexworld` (the water world of `e2e/water-world.mjs` at radius 26), loaded rather than built by script.
- **Load guard.** `e2e/perf-load.mjs`: MiniMac's 1-minute load at most 15 immediately before and after every run, else the run is discarded and rerun.

### Grass, web (`grass-web.json`, `grass-web.log`)

`e2e/perf-grass.mjs` with `GRASS_LOAD=<save>` and `GRASS_ARMS=off,sparse,normal,lush` (0%, 50%, 100% and 150% density), 3 rounds, arm order reversed on odd rounds. Play, High, tilt-shift off, seed 1, 2560x1440, DPR 1, uncapped GPU Chromium, HUD overlay live, the low camera over the start meadows (10, 12, 40) looking at the origin. Each run is a fresh browser: load the save, set the camera, wait 240 frames, then 700 more, then read the 600-frame diag window and `state().grass`.

### Grass, native (`grass-native.json`, `grass-native.log`)

`grass-native.mjs` here, the same world, camera and arms natively: one release process per arm with `--present immediate --offscreen on --size 2560x1440`, the save loaded with `--load`, read 1,500 frames after the camera is set, arms `off`, `sparse`, `normal` and `lush`. `--offscreen on` is needed because MiniMac's console is locked and the binary runs over SSH; without it the window gets no swap-chain texture and the camera draws nothing (`docs/perf/low-camera/README.md`, Native results). Arm order rotates by one each round, 3 rounds.

### Sway, web (`sway-web.json`, `sway-web.log`)

There is no shipped switch for the foliage sway. This measures a **measurement-only ablation build**: main with `crates/hex-render/src/kit_material.rs` building every kit slot's `wind` as `None` (`build-arm.sh nosway`), so no material carries `FLAG_WIND_TREE` or `FLAG_WIND_PLANT` or a wind amplitude and the kit vertex stages skip the foliage wind in every pass (forward, prepass and shadow cascades). It was never committed to the game. `e2e/perf-close-camera.mjs` runs main and the ablation at `grass=normal` (`WORLDS=world2066`, 3 rounds, one warm-up discarded): the same save, photo mode with DOF off, the close camera (10, 12, 40) and the overview (20, 160, 150). Grass itself is wind-driven too (its own vertex shader); the ablation leaves it on, so this isolates the kit foliage sway.

These rows are not comparable to the grass rows above: perf-grass runs with the HUD live, perf-close-camera in photo mode with DOF off.

## Results

Medians over 3 rounds. Deltas against `off` are paired within a round, median [min, max]. "Real" follows the close-camera-draws rule: every paired delta has the same sign and the median is larger than their spread. `summary.py` here prints these from the JSON.

### Grass, web (06:31-06:39Z, loads 3.52-4.29 before and after every run)

| Density | Blades drawn | Mean ms | p95 ms | p50 ms | Frames > 25 ms (of 600) | Mean − off | p95 − off |
|---|---|---|---|---|---|---|---|
| off (0%) | 0 | 31.34 | 32.7 | 31.4 | 600 | | |
| sparse (50%) | 89,069 | 33.38 | 35.2 | 33.4 | 600 | **+2.09** [+1.87, +2.29], real | +2.3 [+2.3, +2.6], real |
| normal (100%, default) | 178,190 | 35.48 | 37.3 | 35.4 | 600 | **+4.22** [+3.97, +4.39], real | +4.7 [+4.4, +4.7], real |
| lush (150%) | 267,037 | 37.70 | 39.1 | 37.6 | 600 | **+6.40** [+6.36, +6.59], real | +6.4 [+6.4, +6.5], real |

- 12 of 12 runs valid: every run built 2,066 tiles on High with tilt off and the requested density, no sample was discarded for load, and no console error was logged. Dist id `3598d98135`, renderer `ANGLE Metal Renderer: Apple M4`, WebGPU adapter `apple | metal-3 | Apple M4`, Chrome for Testing 153.0.8010.12.
- 25 grass groups at every density, 267,037 blades held in the instance buffers (8.5 MB) at every density; only the drawn count changes.
- **Grass costs about 2.1 ms of frame time per 50% of density at this camera, linear in drawn blades: +4.2 ms (13%) at the default Normal and +6.4 ms (20%) at Lush.**
- The frame is GPU-bound with grass off: all 600 window frames are over 25 ms, mean 31.3 ms and p95 32.7 ms.
### Grass, native (06:39-06:52Z, loads 3.61-5.18 before and after every run)

| Density | Mean ms | p95 ms | p50 ms | Frames > 25 ms (of 600) | Draws | Mean − off | p95 − off |
|---|---|---|---|---|---|---|---|
| off (0%) | 36.51 | 40.65 | 36.16 | 600 | 1,398 | | |
| sparse (50%) | 39.63 | 44.3 | 39.29 | 600 | 1,418-1,420 | **+3.26** [+3.00, +3.67], real | +3.65 [+3.29, +3.69], real |
| normal (100%, default) | 42.57 | 45.92 | 43.85 | 600 | 1,420 | **+6.08** [+6.03, +6.50], real | +5.17 [+5.02, +6.41], real |
| lush (150%) | 45.48 | 50.35 | 46.72 | 600 | 1,420 | **+9.12** [+8.94, +9.45], real | +9.61 [+9.49, +9.72], real |

- 12 of 12 runs valid: 2,066 tiles, High, the requested density, every pack ready, no tile rising, the diag window full. Adapter `Apple M4 | Metal | IntegratedGpu`; `window.not_acquired` 0 (the offscreen path draws every pass); `frame_cap` off with the continuous unfocused update, so frames are not paced to 60 Hz. No sample was discarded for load.
- Same blade counts as the web (25 groups). The kit draws 1,337 at every density; grass adds 20-22 draws.
- **Natively grass costs about 3 ms per 50% of density: +6.1 ms (17%) at Normal and +9.1 ms (25%) at Lush, half again the web's cost.** The native frame without grass is also slower than the web's (36.5 against 31.3 ms mean), as the low-camera run found with every pass drawn (`docs/perf/low-camera/README.md`, Native results). Why native pays more per blade was not investigated.

### Notes

- MiniMac's desktop Google Chrome (the owner's session, not automated) had one renderer process (pid 16157, running since 2026-10-27 20:59Z) at about 100% of one CPU core through the whole window, and with none of this run's processes alive MiniMac's GPU sat at 5-6% utilization (`ioreg`, 12 samples at 07:07Z). The paired deltas are not biased by it, but the absolute frame times may be a few percent above an idle host's. It ran under every arm alike, so it does not bias the paired deltas. `PERF_EXCLUSIVE` was left off because its 20% threshold would have held the run for up to an hour.

### Sway, web (`sway-web.json`, `sway-web.log`, `sway-web-summary.json`; 06:52-07:04Z, loads 3.02-5.95)

Main (`sway`, dist `3598d98135`) against the ablation (`nosway`, dist `f5b37573a5`), both at `grass=normal`, 4 rounds, arms rotated, one warm-up discarded. Deltas are nosway minus sway, so a negative delta is what the sway costs.

| Camera | Arm | Mean ms | p95 ms | Main-thread `cpu` mean ms | Draws (kit) |
|---|---|---|---|---|---|
| close | sway (main) | 35.56 | 37.5 | 16.17 | 1,490-1,492 (1,337) |
| close | nosway | 31.92 (**-3.69** [-3.96, -3.24], real) | 33.75 (-3.85 [-4.6, -3.6], real) | 16.79 (+0.4 [-0.48, +0.89]) | 1,490-1,492 (1,337) |
| overview | sway (main) | 40.10 | 41.6 | 14.12 | 962 (881) |
| overview | nosway | 38.92 (**-1.29** [-1.61, -0.99], real) | 40.15 (-1.55 [-2.0, -1.2], real) | 13.81 (-0.17 [-0.45, +0.03]) | 962 (881) |

- 8 of 8 runs valid, no console error. Kit draws are identical between the arms in every round; the close total moves by 2 between rounds in both arms alike (grass groups).
- The close camera pays almost three times the overview's sway cost. Which swaying pieces are in each view was not counted.
- Cross-check: main with grass at Normal reads 35.56 ms here and main with grass off 31.54 ms in the paired close-camera run of the same session, a 4.0 ms grass cost under photo mode, in line with perf-grass's +4.2 ms.

## Inferred, not measured

- The sway ablation removes the wind flags and amplitude from every kit material (`kit_material.rs`), which skips the wind branch in the kit vertex stages. It leaves the kit's custom vertex stages in place (they also carry other per-vertex work, such as the farm animals' pose), so any fixed cost of the wind path beyond its branch is not in the delta. Grass blades keep their own wind in both arms.
- Native sway was not measured (no native ablation binary was built).
- No GPU trace was taken, so which passes the grass and sway time lands in is not split here.
- MiniMac is a base M4 (10 GPU cores). The M4 Max dev host has 40, so the absolute frame there is lower; this run does not say whether it meets 16.6 ms on the M4 Max.

## Reproduce

```bash
# macdaddy (load-tolerant)
git archive a543d47 -o /tmp/cc-arms/main.tar
bash docs/perf/close-camera-regression-2026-10-28/build-arm.sh main /tmp/cc-arms/main.tar
bash docs/perf/close-camera-regression-2026-10-28/build-arm.sh nosway /tmp/cc-arms/main.tar nosway
cargo build -p hex-app --release
# MiniMac ~/perf/gc: arms/<name> (each dist), e2e/ (npm ci), web/ui/src, assets/, target/release/hex-world,
# the save, docs/checkpoints/scatter/scatter-mixed.json and this directory's scripts. Copy each dist with
# rsync -c: a size-and-mtime --link-dest hardlinked one arm's index.html and version.json to another's.
bash docs/perf/grass-cost-2026-10-28/minimac-run.sh grass
bash docs/perf/grass-cost-2026-10-28/minimac-run.sh native
SWAY_ROUNDS=4 bash docs/perf/grass-cost-2026-10-28/minimac-run.sh sway
python3 docs/perf/grass-cost-2026-10-28/summary.py results/grass-web.json off
node e2e/perf-close-camera-summary.mjs results/sway-web.json sway
```
