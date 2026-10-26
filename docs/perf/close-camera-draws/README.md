# Close-camera draw calls: MiniMac before/after run

This is the frame-time half of the close-camera draw-call work (owner A2 Q3). The draw counts are in `docs/checkpoints/close-camera-draws/README.md`. Frame time is measured on MiniMac only, because this host's load was 100 to 125 throughout the work. The first measurement is in [MiniMac results](#minimac-results-2026-10-26).

## Builds

| Arm | Commit | What it holds |
|---|---|---|
| before | `29a67fb` | main before the work |
| O5 (optional) | `b37f0bb` on the work branch | the screen-aware cascade skip |
| O5 + O4 (optional) | `4b90a09` on the work branch | plus runtime slot coalescing |
| after | the `main` commit that lands the work (`git log -1 --format=%h -- crates/hex-play/src/play/chunks.rs`) | O5, O4 and O6: plus per-tile culling cells |

The work branch's commits keep their hashes only if it lands on `main` without a squash. After a squash merge, rebuild the optional arms from the work branch or skip them.

Each arm needs both its web `dist/` and its native release binary built **from its own checkout**. A native build loads `./assets/shaders` at run time, and the kit shaders differ between the arms. So the native harness runs each binary with its checkout as the working directory.

On the build host, from the main checkout (`<sha>` is each arm's commit):

```bash
git worktree add /tmp/ccd-<arm> <sha>
cd /tmp/ccd-<arm>
bash scripts/worktree-bootstrap.sh --web
make build-web                          # dist/
cargo build -p hex-app --release        # target/release/hex-world
```

## Copy to MiniMac

Book the window first: `kanban schedule book --resource minimac --start <UTC> --end <UTC> --holder <agent-id>`. Allow about 90 minutes for 5 rounds of both harnesses. Then check in, and heartbeat during the run.

The layout keeps each native arm as a checkout-shaped directory, with its binary under `target/release/` and its `assets/` beside it:

```bash
for arm in before after; do
  rsync -a --delete /tmp/ccd-$arm/dist/ minimac:perf/hex-world-ccd/$arm/dist/
  rsync -a --delete /tmp/ccd-$arm/assets/ minimac:perf/hex-world-ccd/$arm/assets/
  rsync -a /tmp/ccd-$arm/target/release/hex-world minimac:perf/hex-world-ccd/$arm/target/release/
done
# The harnesses and the 2,066-tile save from the after arm.
rsync -a --delete --exclude node_modules /tmp/ccd-after/e2e/ minimac:perf/hex-world-ccd/e2e/
rsync -a /tmp/ccd-after/web/ui/src/ minimac:perf/hex-world-ccd/web/ui/src/
rsync -a /tmp/ccd-after/docs/perf/post-arc001-2026-10-16/local-world.hexworld minimac:perf/hex-world-ccd/docs/perf/post-arc001-2026-10-16/
ssh minimac 'cd perf/hex-world-ccd/e2e && npm ci --no-audit --no-fund'
```

MiniMac's `ulimit -n` gotcha is in `~/.claude/guides/infrastructure.md` (MiniMac as a test-gate runner). Raise it in the shell that runs the harnesses.

## Runs

Both harnesses wait for a 1-minute load of 15 or less before each run, and discard and re-run any sample whose post-run load is over 15 (`e2e/perf-load.mjs`). Each run is one fresh process on one arm and one world. The arms alternate their order every round. Every camera window is read 700 frames after the camera moves, so the 600-frame diag ring holds only that camera.

```bash
ssh minimac
cd perf/hex-world-ccd
ulimit -n 10240
# Web: uncapped GPU Chromium (ANGLE Metal), 2560x1440, DPR 1, play/High/tilt off/grass off/seed 1.
caffeinate -d -u node e2e/perf-close-camera.mjs before/dist after/dist close-camera-web.json
# Native: release binaries, 2560x1440, present immediate, --diag, the same keys.
caffeinate -d -u node e2e/perf-close-camera-native.mjs before after close-camera-native.json
```

- `ROUNDS` defaults to 5. Set `SAVE=<path>` to use another `.hexworld`.
- To isolate one change, build and copy an optional arm the same way, and pass it in place of `before` or `after`.
- O6 changes CPU culling for each view, so its cost or benefit shows as CPU time on the wasm main thread. Read it on the 2,066-tile world. There the old path visited every slot in every 19-hex chunk, and the new one visits only the slots each tile holds.

### Worlds and cameras

| World | Camera | Pose (position, look-at) |
|---|---|---|
| 20 tiles (the `draws.json` placements) | overview (gated at 500 draws) | (20, 160, 150), (20, 0, -5) |
| | forest cluster | (46, 58, 48), (46, 0, -12) |
| | meadow | (6, 13, 44), (12, 0, 22) |
| | 152 m band | (20, 56, 136), (20, 0, -5) |
| 2,066 tiles (`local-world.hexworld`) | close (the post-ARC-001 camera) | (10, 12, 40), (0, 0, 0) |
| | overview | (20, 160, 150), (20, 0, -5) |

Both worlds are set up as the draws census sets up the 20-tile world: pointer at (9, -9), photo mode on with DOF off, every pack landed and every scatter instance placed before the first window. Grass is off in both harnesses, unlike the draws census. So the draw counts each run records are lower than `census-phase*.json` by the grass draws, the same in both arms.

## Dry run (2026-10-26, dev host, timings discarded)

Both harnesses ran once on the dev host with `ROUNDS=1` and `PERF_LOAD_LIMIT=1000`, the work's build in both arms. That run checked the scripts only. Its frame times are not a measurement.

Both worlds built, and the save loaded natively (`--load`) and on the web (`hexWorld.load`). Every camera window filled, and the web console logged no error. The draw counts, which do not depend on load, were as follows with grass off:

| World | Camera | Native draws (kit draws, kit slots) | Web draws |
|---|---|---|---|
| 20 tiles | overview | 335 | 343 |
| | forest cluster | 434 | 440 |
| | meadow | 475 | 481 |
| | 152 m band | 640 | 654 |
| 2,066 tiles | close | 1,535 (1,474, 127) | 1,609 |
| | overview | 919 (871, 127) | 960 to 976 |

On the 2,066-tile world the far cascades dominate. At the close camera, cascade 2 holds 324 draws and cascade 3 holds 393: the world reaches past 371 m, so every slot casts there at up to three LODs plus their band draws.

## MiniMac results (2026-10-26)

Measured on MiniMac (Mac mini Mac16,10, Apple M4 with a 10-core GPU, 32 GiB, macOS 27.0.1), with before = `29a67fb` and after = `fee30bd` (the work branch head, which holds O5, O4 and O6). Each arm was built on the dev host in its own fresh worktree, and no timing ran there. Both harnesses ran unchanged with `ROUNDS=4`, so each cell has 4 paired samples, and the arms alternated order every round. Every run passed the load guard: the 1-minute load was 2.1 to 4.7 before and after every run, and no sample was discarded or failed. The web renderer was `ANGLE (Apple, ANGLE Metal Renderer: Apple M4, Unspecified Version)` (WebGPU adapter `apple | metal-3`), in HeadlessChrome 153 at 2560x1440 and DPR 1. The console logged no error. Every sample, with its loads, renderer, DPR and build sha, is in [`results-2026-10-26.json`](results-2026-10-26.json), with the optional per-change arms under `optional_arms_web`.

Each value is the median over the 4 rounds. A delta is after minus before, paired within a round, shown as the median with the min and max of the 4 paired deltas. A delta is marked real when all 4 paired deltas have the same sign and the median is larger than their spread. The arm-to-arm spread across rounds is larger at the 20-tile world: up to 0.9 ms on the web mean, because round 0 read low in both arms. Pairing within a round removes that drift.

### Web (uncapped GPU Chromium)

| World | Camera | Mean ms | Mean delta | p95 ms | p95 delta | Draws | Kit draws after (slots, total) |
|---|---|---|---|---|---|---|---|
| 20 tiles | overview | 11.25 -> 11.21 | -0.02 (-0.11 to +0.04), noise | 19.00 -> 19.05 | +0.10 (0.00 to +0.40), noise | 427 -> 343 | 97, 291 |
| 20 tiles | forest cluster | 17.82 -> 17.06 | **-0.77** (-0.82 to -0.75), -4.3% | 31.35 -> 30.70 | **-0.80** (-1.00 to -0.60) | 637 -> 440 | 97, 388 |
| 20 tiles | meadow | 15.93 -> 15.46 | **-0.47** (-0.53 to -0.46), -2.9% | 28.10 -> 27.80 | **-0.30** (-0.30 to -0.20) | 629 -> 481 | 97, 437 |
| 20 tiles | 152 m band | 14.54 -> 14.49 | -0.05 (-0.08 to +0.01), noise | 25.20 -> 25.25 | +0.10 (-0.10 to +0.10), noise | 764 -> 648 | 97, 596 |
| 2,066 tiles | close | 38.29 -> 34.55 | **-3.73** (-3.80 to -3.68), -9.7% | 53.80 -> 46.65 | **-7.20** (-7.30 to -7.00), -13.4% | 2,264 -> 1,609 | 127, 1,474 |
| 2,066 tiles | overview | 42.27 -> 42.05 | **-0.23** (-0.25 to -0.19), -0.5% | 65.65 -> 66.25 | +0.55 (+0.20 to +0.80), noise | 1,426 -> 952 | 127, 871 |

The web p50 is bimodal on this host, as the README warns. For example, at the 2,066-tile close camera p50 rose from 26.55 to 28.65 ms while the mean fell. So p50 is recorded in the JSON but is not used here.

### Native (release, `--present immediate`)

| World | Camera | Mean ms | Mean delta | p95 ms | Draws | Paced? |
|---|---|---|---|---|---|---|
| 20 tiles | overview | 16.67 -> 16.67 | 0.00 | 19.23 -> 18.89 | 419 -> 335 | both arms at 60 Hz |
| 20 tiles | forest cluster | 16.66 -> 16.67 | +0.01 | 18.83 -> 17.87 | 631 -> 434 | both arms at 60 Hz |
| 20 tiles | meadow | 16.67 -> 16.67 | 0.00 | 19.13 -> 18.36 | 623 -> 475 | both arms at 60 Hz |
| 20 tiles | 152 m band | 16.67 -> 16.67 | 0.00 | 17.77 -> 20.14 | 756 or 759 -> 640 or 643 | both arms at 60 Hz |
| 2,066 tiles | close | 28.05 -> 23.68 | **-4.37** (-4.46 to -2.55), -15.6% | 28.53 -> 24.34 | 2,190 -> 1,535 | neither |
| 2,066 tiles | overview | 17.62 -> 16.62 | -0.99 (-1.02 to -0.66) | 18.47 -> 21.80 | 1,393 -> 919 | after arm at 60 Hz |

Native play is pinned to 60 Hz even with `--present immediate`. A probe of the after binary showing one tile read a mean of 16.67 ms on a 2560x1440 surface (screenshot measured 2560x1440 px). So a native row at 16.67 ms measures pacing, not render cost. Only the 2,066-tile close camera is unpaced in both arms, and there the work saves 4.4 ms (median paired delta). At the 2,066-tile overview, the after arm reached the 60 Hz floor. That makes -0.99 ms a lower bound on the saving, and the p95 rise there comes from vsync misses, not from extra work. GPU timing (`gpu_timing=on`) is not among the harness's arguments, so the native rows have no unpaced GPU time for the paced cameras.

`kit_draws` is null in the before arm because the census arrived in `a506747`. The after arm's `kit_draws` are the same on web and native at every camera.

### Optional arms: each change alone (web, 3 rounds per pair)

Next, the web harness ran three more times, with `ROUNDS=3` each. Each run used one adjacent pair of builds: before `29a67fb` against O5 `b37f0bb`, then O5 against O5 + O4 `4b90a09`, then O5 + O4 against after `fee30bd` (O6). Every run passed the load guard, with loads of 1.6 to 6.8. No run failed, and the console logged no error. These sessions ran later than the main run, so absolute frame times differ slightly from the tables above. The 2,066-tile overview drifted by up to 1.2 ms. Compare paired deltas only.

| World | Camera | O5: mean delta, p95 delta, draws | O4: mean delta, p95 delta, draws | O6: mean delta, p95 delta, draws |
|---|---|---|---|---|
| 20 tiles | overview | -0.01, -0.20 (noise), 427 -> 427 | -0.02, +0.10 (noise), 427 -> 343 | 0.00, 0.00 (noise), 343 -> 343 |
| 20 tiles | forest cluster | **-0.83**, **-1.30**, 637 -> 552 | +0.04, +0.10 (noise), 552 -> 440 | -0.03, -0.10 (noise), 440 -> 440 |
| 20 tiles | meadow | -0.01, 0.00 (noise), 629 -> 629 | -0.02 (noise), **+0.20**, 629 -> 513 | **-0.47**, -0.70 (p95 noise), 513 -> 481 |
| 20 tiles | 152 m band | 0.00, +0.10 (noise), 764 -> 764 | +0.03, +0.10 (noise), 764 -> 648 | +0.09, +0.10 (noise), 648 -> 648 |
| 2,066 tiles | close | +0.07, +0.60 (noise), 2,264 -> 2,264 | +0.11 (noise), **+1.30**, 2,264 -> 1,863 | **-3.84**, **-8.20**, 1,863 -> 1,609 |
| 2,066 tiles | overview | **+0.92**, **+2.50**, 1,426 -> 1,252 | +0.08 (noise), **+1.40**, 1,252 -> 990 | **-1.40**, **-3.00**, 990 -> 952 |

Deltas are medians of 3 paired rounds in ms. Bold marks a real delta by the rule above.

The changes contribute as follows:
- O6 (per-tile culling cells, CPU) produces the whole 2,066-tile close-camera win and the 20-tile meadow win.
- O5 produces the 20-tile forest-cluster win, and costs about 0.9 ms of mean at the 2,066-tile overview.
- O4 cuts the most draws: 84 to 116 per 20-tile camera and 401 at the 2,066-tile close camera. On this host it changes no mean frame time, and it adds 1.3 to 1.4 ms of p95 on the 2,066-tile world.

## What to report

- For each world and camera: the median, minimum and maximum over the runs of p50, p95 and mean per arm, and after minus before. Uncapped web frames on MiniMac are bimodal (`docs/perf/lod-band-instanced.json`), so mean and p95 are the stable statistics.
- Each row's draw count and `kit_draws` (slots and total). `kit_draws` must match between the web and native rows of an arm. The total draw count does not: web counts a near-constant few more per camera in both arms (8 at the 20-tile overview, 74 at the 2,066-tile close camera, and within 3 of constant at the 152 m band). Both must show the drop from the census.
- The renderer string, which must be ANGLE Metal Apple M4 and not SwiftShader, the load before and after each run, and any discarded samples.
- Write the results to `docs/perf/close-camera-draws/minimac-<date>.json` with this README's commands. Then add the summary to PRD §10's draw-call row.
