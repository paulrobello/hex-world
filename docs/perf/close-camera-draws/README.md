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

## Far-cascade LOD rules (rejected) and the kit bind-group cache

Two far-cascade LOD rules cut the 2,066-tile close camera's draws: a clamp per cascade (`d8cd4cd`) to 1,235, and one LOD per cascade (`e06edcf`) to 1,018, from 1,535. They raised the kit's shadow indices by 9-26% and 1.8-2.8 times. MiniMac timed them (`results-clamp-2026-10-26.json`, branch perf-batch27, `386f459`). The clamp, together with the bind-group cache (`6c3bd36`), was slower than `041ad32` on the 2,066-tile world:

- web mean: +1.03 ms at the close camera, +1.43 ms at the overview;
- web p95: +3.0 ms and +3.5 ms;
- native: +0.93 ms and +1.16 ms.

The 20-tile cameras did not change, and the one-LOD rule was 4 to 7 ms slower still. Both rules are reverted. `docs/checkpoints/far-cascade-lod/README.md` has their counts.

What remains is the group-2 bind group cache. On the 2,066-tile world it cuts 381 `createBindGroup` calls a frame (127 slots times 3 LODs) to 0 (`diag.kit_queue.bind_groups_created`). On the web each call reaches the browser's GPU process, so the saving is wasm main-thread time in every frame. Natively, `create_bind_group` is a cheap wgpu call. So the recipe below times the web only.

### Cache only against main

| Arm | Commit | What it holds |
|---|---|---|
| before | `041ad32` | main |
| cache | the branch head that reverts the cascade rules (`git log -1 --format=%h -- docs/checkpoints/far-cascade-lod/README.md`) | the bind-group cache and the `kit_queue` census (off unless `HEXW_KIT_QUEUE` is set natively, so the web never runs it) |

Draws and shading are identical in both arms: far cascades cast as on main, and `make thumbs` is byte-identical.

Build each arm's `dist/` as in [Builds](#builds) (`/tmp/ccd-before` at `041ad32`, `/tmp/ccd-cache` at the cache sha; `make build-web` only, no native binary). Then copy:

```bash
for arm in before cache; do
  rsync -a --delete /tmp/ccd-$arm/dist/ minimac:perf/hex-world-ccd/$arm/dist/
done
# The web harness (with its WORLDS filter) and the save come from the cache arm.
rsync -a --delete --exclude node_modules /tmp/ccd-cache/e2e/ minimac:perf/hex-world-ccd/e2e/
rsync -a /tmp/ccd-cache/web/ui/src/ minimac:perf/hex-world-ccd/web/ui/src/
rsync -a /tmp/ccd-cache/docs/perf/post-arc001-2026-10-16/local-world.hexworld minimac:perf/hex-world-ccd/docs/perf/post-arc001-2026-10-16/
ssh minimac 'cd perf/hex-world-ccd/e2e && npm ci --no-audit --no-fund'
```

Book the window first. `kit_draws` runs in both arms on census frames, about 20 of each 600-frame window, so it does not bias the pair. On MiniMac:

```bash
ssh minimac
cd perf/hex-world-ccd
ulimit -n 10240
# The 2,066-tile close camera and overview only, 5 rounds, arms alternating (about 40 minutes).
WORLDS=world2066 caffeinate -d -u node e2e/perf-close-camera.mjs before/dist cache/dist cache-only-web.json
```

Check before reading the timings:

- Each row's `draws` is equal in both arms: about 1,609 web at the close camera and 952 to 976 at the overview.
- The renderer is ANGLE Metal Apple M4.
- Every run passed the load guard.

Report each camera's paired mean and p95 deltas (cache minus before; median, minimum and maximum over the 5 rounds) as [What to report](#what-to-report) says. Write the result to `docs/perf/close-camera-draws/results-cache-<date>.json`.

A real saving shows as a negative mean delta at both cameras. The cache removes the same 381 calls at both, so the saving should be about equal. A delta inside the round-to-round spread means the calls cost less than this harness can resolve, and the cache stays for the CPU work it saves.

#### Result, MiniMac 2026-10-26 (`results-cache-2026-10-26.json`)

Run by the orchestrator, 21:43-21:56Z. Host: MiniMac (Apple M4), headless Chromium on ANGLE Metal Apple M4, 2,066-tile world, 5 rounds, arms alternating, host load 2.1-3.8 before and after every run. Before arm: main's play build (code identical to `041ad32`). Cache arm: `ec47f79`. Draws equal in every run: 1,609 close, 952 overview.

Cache minus before, per round (ms):

| Camera | Mean delta, rounds 0-4 | Median | p95 delta, rounds 0-4 | Median |
|---|---|---|---|---|
| close | +1.86, -0.07, -0.04, -0.15, -0.01 | -0.04 | +3.3, +0.3, +1.2, -0.1, +0.5 | +0.5 |
| overview | +2.62, -0.06, -0.12, -0.01, -0.03 | -0.03 | +5.4, +1.2, +2.0, 0.0, +0.2 | +1.2 |

Round 0's before run was the first run of the session and ran about 2 ms faster than every other run of either arm (close mean 32.72 against 34.41-34.63), so round 0 is an outlier. Over rounds 1-4 the mean delta is -0.01 to -0.15 ms at both cameras: no measurable saving. The p95 deltas lean slightly positive (median +0.5 ms close, +1.2 ms overview), inside the before arm's own p95 spread over those rounds (45.9-46.9 close, 64.5-66.6 overview). Read: removing the 381 `createBindGroup` calls a frame costs nothing measurable and saves nothing measurable on the web at this world size. The cache stays for the CPU work it removes, per the rule above.

### O5 and O4 regressions, re-read with CPU counts

The optional arms above showed O5 alone at +0.92 ms web mean on the 2,066-tile overview, and O4 alone at +1.3 to 1.4 ms web p95 on the 2,066-tile world. `diag.kit_queue` counts the kit's work per view without timing (`docs/checkpoints/far-cascade-lod/kit-queue/`). It does not explain either regression:

- O5 adds no per-view work. Per frame it adds one `shadowed_depth_range` per cascade: 30 polygons clipped by up to 6 planes, plus up to 20 plane triples. It also adds one pass over the cells' bounds. Its only output is the list of cascades the kit skips, and at the 2,066-tile overview those are cascades 0 and 1. By construction it cannot add a draw, instance or vertex to any view.
- O4's merged meshes carry COLOR_0, 16 bytes a vertex. That is under 3% of the kit's vertex bytes in every view. At the 2,066-tile close camera it is 0.7% in the main view, 1.7% in cascade 2 and 2.4% in cascade 3. O4 merges slots within a piece, and every slot of a piece sits in the same tile, so O4 cannot defeat per-tile culling. Each view draws the same instances and vertices in fewer draws.
- The census found one per-frame cost that grows with the world: the bind-group rebuild, one `createBindGroup` per (slot, LOD) every frame. That is 381 a frame on the 2,066-tile world after O4, and more before it. Both arms had it, and neither change caused it. It is now gone, and the cache-only recipe above times it.

So the census finds no CPU cause in the kit's work for either regression. Both are 3-round deltas on a bimodal web frame-time distribution, between builds that differ in their whole wasm module. To settle them, re-run the optional O5 and O4 arms from [Optional arms](#optional-arms-each-change-alone-web-3-rounds-per-pair) with `ROUNDS=5`.

## Per-cell cascade skip: MiniMac before/after

The per-cell cascade skip (PRD §9.1, 2026-10-27) takes casters out of shadow cascades they cannot shadow anything in, tile by tile. It changes no shading, and it removes draws and instances only from cascades. Earlier far-cascade rules measured slower on MiniMac because they traded fewer draws for more shadow triangles (above). This one removes triangles as well, so it needs its own frame-time check before it is kept.

| Arm | Commit | What it holds |
|---|---|---|
| before | `8464de8` | main before the change |
| after | the commit that lands it (`git log -1 --format=%h -S cell_shadowed_depth_range -- crates/hex-render/src/kit_instances.rs`) | the per-cell cascade skip |

Build each arm as in [Builds](#builds): `/tmp/ccd-before` at `8464de8`, `/tmp/ccd-after` at the after sha, with both `make build-web` and `cargo build -p hex-app --release` in each.

Copy to MiniMac as in [Copy to MiniMac](#copy-to-minimac), with the harnesses from the after arm (they add the `mixed` world and a `WORLDS` filter to the native harness), plus the mixed world's script, which both harnesses read from `docs/checkpoints/scatter/scatter-mixed.json` relative to the working directory:

```bash
for arm in before after; do
  rsync -a --delete /tmp/ccd-$arm/dist/ minimac:perf/hex-world-ccd/$arm/dist/
  rsync -a --delete /tmp/ccd-$arm/assets/ minimac:perf/hex-world-ccd/$arm/assets/
  rsync -a /tmp/ccd-$arm/target/release/hex-world minimac:perf/hex-world-ccd/$arm/target/release/
done
rsync -a --delete --exclude node_modules /tmp/ccd-after/e2e/ minimac:perf/hex-world-ccd/e2e/
rsync -a /tmp/ccd-after/web/ui/src/ minimac:perf/hex-world-ccd/web/ui/src/
rsync -a /tmp/ccd-after/docs/perf/post-arc001-2026-10-16/local-world.hexworld minimac:perf/hex-world-ccd/docs/perf/post-arc001-2026-10-16/
rsync -a /tmp/ccd-after/docs/checkpoints/scatter/scatter-mixed.json minimac:perf/hex-world-ccd/docs/checkpoints/scatter/
ssh minimac 'cd perf/hex-world-ccd/e2e && npm ci --no-audit --no-fund'
```

Book the window first (`kanban schedule book --resource minimac ...`): about 75 minutes for 5 rounds of both harnesses on two worlds. On MiniMac:

```bash
ssh minimac
cd perf/hex-world-ccd
ulimit -n 10240
# The mixed world (every-tile overview, 20-tile pose) and the 2,066-tile world (close, overview), 5 rounds, arms alternating.
WORLDS=mixed,world2066 caffeinate -d -u node e2e/perf-close-camera.mjs before/dist after/dist cell-cascades-web.json
WORLDS=mixed,world2066 caffeinate -d -u node e2e/perf-close-camera-native.mjs before after cell-cascades-native.json
```

Check before reading the timings:

- Each row's `draws` matches the dev-host dry run of both harnesses (2026-10-27, `ROUNDS=1 PERF_LOAD_LIMIT=1000`, grass off, timings discarded at host loads of 40 to 290):

  | World | Camera | Native draws, before -> after (kit draws) | Web draws, before -> after |
  |---|---|---|---|
  | mixed (31 tiles) | overview (every tile) | 548 -> 452 (500 -> 404) | 564 -> 468 |
  | | 20-tile pose | 428 -> 428 (375) | 446 -> 446 |
  | 2,066 tiles | close | 1,586 -> 1,328 (1,525 -> 1,267) | 1,660 -> 1,402 |
  | | overview | 951 -> 880 (903 -> 832) | 984 -> 913 |

  The kit's work in the cascades fell more than its draws (native, `HEXW_KIT_QUEUE`): at the 2,066-tile close camera, 35.3 M -> 16.0 M indices over the four cascades (cascade 3: 1,892 -> 538 tiles, 47,917 -> 12,166 instances, 17.4 M -> 3.1 M indices), and at its overview 21.7 M -> 11.1 M. The main view is unchanged at every camera.
- The renderer is ANGLE Metal Apple M4, and every run passed the load guard.

Report each camera's paired mean and p95 deltas (after minus before; median, minimum and maximum over the 5 rounds) as [What to report](#what-to-report) says, and write them to `docs/perf/close-camera-draws/results-cell-cascades-<date>.json`. The skip is kept only if no camera is slower by more than the round-to-round spread. The 2,066-tile close camera, whose far cascades hold most of its draws, is where a saving should show.

### Result (2026-10-27)

Run on MiniMac 04:15–04:49Z, 5 rounds, every run within the load guard (highest load 9.8 web, 4.5 native), renderer ANGLE Metal Apple M4, no console errors. The draws matched the dry-run table. Results: `results-cell-cascades-2026-10-27.json`. Paired deltas, after minus before, as median [min, max] over the rounds:

| World / camera | Web mean ms | Web p95 ms | Native mean ms | Native p95 ms |
|---|---|---|---|---|
| mixed / overview | 12.54 -> 12.54, -0.02 [-0.09, 0.10] | 0.0 [-0.1, 0.3] | 16.67 -> 16.67 (60 Hz cadence) | -0.04 [-1.75, 5.7] |
| mixed / 20-tile pose | 13.09 -> 13.11, 0.00 [-0.08, 0.14] | 0.1 [0.0, 0.2] | 16.66 -> 16.67 (60 Hz cadence) | -0.09 [-1.88, 3.1] |
| 2,066 / close | 34.11 -> 30.39, -3.65 [-7.53, -3.15] | -6.9 [-14.1, -5.5] | 23.32 -> 18.89, -4.43 [-4.90, -3.54] | -4.41 [-5.15, -3.96] |
| 2,066 / overview | 41.73 -> 38.92, -2.84 [-5.60, -2.32] | -4.9 [-6.1, -1.7] | 16.63 -> 16.66 (60 Hz cadence) | 0.53 [-2.33, 4.84] |

No camera is slower than the round-to-round spread, so the skip is kept. The native rows marked 60 Hz cadence sit at the play loop's pace in both builds, so their means cannot show a GPU change, and their p95 swings are frame-pacing jitter, which spans 3.8 to 4.8 ms across the before rounds alone.

## What to report

- For each world and camera: the median, minimum and maximum over the runs of p50, p95 and mean per arm, and after minus before. Uncapped web frames on MiniMac are bimodal (`docs/perf/lod-band-instanced.json`), so mean and p95 are the stable statistics.
- Each row's draw count and `kit_draws` (slots and total). `kit_draws` must match between the web and native rows of an arm. The total draw count does not: web counts a near-constant few more per camera in both arms (8 at the 20-tile overview, 74 at the 2,066-tile close camera, and within 3 of constant at the 152 m band). Both must show the drop from the census.
- The renderer string, which must be ANGLE Metal Apple M4 and not SwiftShader, the load before and after each run, and any discarded samples.
- Write the results to `docs/perf/close-camera-draws/minimac-<date>.json` with this README's commands. Then add the summary to PRD §10's draw-call row.
