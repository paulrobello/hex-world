# Large world: main-thread time per frame (MiniMac, 2026-10-27)

Card: "Large world: main-thread time per frame grew ~6 ms close / ~4.5 ms overview since the Oct 26 builds". On the 2,066-tile save, `e2e/perf-close-camera.mjs` reads each camera window's main-thread time per engine frame (`cpu`). It read about 22.6 ms close and 20.4 ms overview for the historical build `4b90a09` (O5+O4), and 29.1 ms close and 25.8 ms overview for main.

## Summary

- **The growth was real engine work, not a change in what the probe measures.**
  - With every WebGPU queue, device, canvas, encoder and buffer call timed (`GPU_CALLS=1`), the calls total 0.4 to 0.9 ms a frame on both builds. None of them blocks. So the callback is not waiting on the GPU.
  - The limiter fix `e88b1bd` moves `cpu` by -0.24 ms close and +1.09 ms overview on the same `4b90a09` wasm. That comes from frame pairing, not engine work, and it is the only measurement part of the growth.
- **Culprits: the runtime-scatter commits `e9ea60d` (river) and `2e331f3` (coast).** Each step is paired against its parent over 3 rounds:

  | Step | Close `cpu` | Overview `cpu` |
  |---|---|---|
  | `4b90a09` to `6bc3a43` (O6 per-tile culling) | -0.01 | +0.09 |
  | `6bc3a43` to `9c882f4` (cars yield, via `0ace163`) | +0.47, not real | +0.30, not real |
  | `9c882f4` to `e9ea60d` (river scatter) | +2.43 (derived) | +2.67 (derived) |
  | `e9ea60d` to `2e331f3` (coast scatter) | +5.98 (derived) | +4.26 (derived) |
  | `2e331f3` to `f1aff3e` (per-cell cascade skip and others) | -1.09 (derived) | -0.69 (derived) |
  | `f1aff3e` to main | -0.06 (derived) | +0.11 (derived) |

  The measured numbers are each arm's paired delta against `6bc3a43` (pass 2: `9c882f4`, `e9ea60d`, `2e331f3`) or against `4b90a09` (pass 1: the rest). The step rows marked "derived" are differences between two such deltas.
- **The cause was a per-frame recompute, which the scatter commits made bigger.**
  - `hex_play::play::finish_kit` took `PlayKit` with `as_deref_mut()` before checking whether any pack had arrived, so `PlayKit` was marked changed every frame (since `a672056`, 2026-10-16).
  - Scatter's `sync` treats a kit change as a reason to re-lay every scatter tile and rebuild its JSON report. `chunks::settle` then re-parsed and rewrote that report, and that write made the report differ again on the next frame.
  - Profiled at the close camera, `scatter::sync` took 4.0 ms on `4b90a09` and 7.6 ms on main. `chunks::settle` took 2.1 and 5.2 ms. The river, beach and ocean tiles that gained scatter rules made both the work and the report larger.
- **Fix (`c032d97`): `PlayKit` is marked changed only on a frame where a pack arrives.**

  | Camera | Main-thread `cpu`, main to fix | Mean, main to fix | Draws |
  |---|---|---|---|
  | Close | 29.11 to 16.09 ms (**-13.02** [-13.66, -12.88]) | 30.44 to 30.36 ms (-0.08) | 1,422, unchanged |
  | Overview | 25.79 to 15.28 ms (**-10.58** [-10.91, -10.33]) | 38.90 to 38.40 ms (-0.51) | 930, unchanged |

  Both rows are 5 load-guarded rounds. The fix is 6.5 ms (close) and 5.1 ms (overview) under `4b90a09` with the same limiter, because `4b90a09` already paid the per-frame recompute, only on fewer tiles.
- **The close camera is GPU-bound, not CPU-bound.** Removing 13 ms of main-thread work moved its mean by only 0.08 ms. The main thread now sits 14 ms under the 30.4 ms frame. The premise that the close camera was CPU-bound (`cpu` 29.1 of a 30.4 ms frame) was coincidence: the main thread had grown to just under the GPU frame time.

## Setup

- **Host.** MiniMac (Mac16,10, Apple M4 with a 10-core GPU), booked 14:02-17:02Z. All runs took place 14:02-15:12Z.
- **Renderer.** `ANGLE (Apple, ANGLE Metal Renderer: Apple M4, Unspecified Version)`, WebGPU adapter `apple | metal-3`.
- **Harness.** `e2e/perf-close-camera.mjs` with `WORLDS=world2066`, the cameras and settings of `docs/perf/large-world-o4o5/README.md`. Settings: headless Chromium, uncapped, 2560x1440, play, High, grass off, photo mode with DOF off. Arms rotate each round, and one warm-up run is discarded.
- **Load guard.** `e2e/perf-load.mjs` required a 1-minute load of 15 or less before and after every run. No sample was discarded or failed, and no console errors were logged.

  | Run | Loads |
  |---|---|
  | bisect 1 | 2.19-3.64 |
  | bisect 2 | 2.29-3.57 |
  | fix | 1.76-3.12 |
  | agents ablation | 2.42-3.75 |
  | call timing | 2.25-2.76 |
  | profiles | 2.18-3.57 |
- **Builds.**
  - Each historical arm was built on macdaddy with `build-arm.sh`:
    1. Extract `git archive <sha>` into a directory.
    2. Clone `target/` and `web/ui/node_modules` into it.
    3. Touch `crates/`, `assets/shaders` and `data/`.
    4. `cargo clean -p` the seven workspace crates for the wasm target.
    5. Run `make build-web`.
  - Every arm's wasm hash differs.
  - `4b90a09` is the dist built the same way for `docs/perf/large-world-o4o5/`.
  - Main is `38b15d1`'s `make build-web`. The fix is `c032d97`'s `make build-web`.
- **Fixed-limiter arms.** Every arm older than `e88b1bd` had main's `web/frame-latency.js` dropped into its dist, as the `fl-` arms of the O4/O5 run did. Each step between arms is therefore a wasm change. The arm `o5o4_oldlim` is `4b90a09` with its own limiter, and it measures the limiter's share.
- **Profiles.**
  - `build-named.sh` is `build-arm.sh` with the Trunk rust link set to `data-keep-debug data-wasm-opt-params="-g"`, so the wasm keeps its name section. The wasm is 58 MB instead of 28 MB, with the same code.
  - `PROFILE_DIR` records a 250 µs V8 sampling profile of each camera window.
  - `scripts/perf/cpuprofile-functions.py` reduces the profiles to inclusive and self ms per engine frame per function. It names Bevy systems as `system <path>::run_unsafe`.
  - The profiler slows what it samples, so profiles are used for attribution and timing comes from unprofiled runs.

Deltas are paired within a round, given as the median [min, max]. "Real" follows the close-camera-draws rule: every paired delta has the same sign, and the median is larger than their spread.

## Results

### Is it measurement? (`gpucalls-diag.json`, `bisect1-web.json`)

Time inside wrapped WebGPU methods, per engine frame, for one round each:

| Build | Camera | `cpu` | WebGPU calls total | Largest call |
|---|---|---|---|---|
| main | close | 29.65 | 0.52 | `writeBuffer` 0.16 (20 calls) |
| main | overview | 25.80 | 0.43 | `createBindGroup` 0.11 (58) |
| `4b90a09` | close | 23.06 | 0.88 | `createBindGroup` 0.42 (437) |
| `4b90a09` | overview | 19.59 | 0.79 | `createBindGroup` 0.42 (439) |

- `getCurrentTexture`, `submit` and `onSubmittedWorkDone` each take under 0.02 ms a frame. The callback does not absorb GPU back-pressure.
- `4b90a09` still created its kit bind groups every frame. `bbeb675` later cached them.
- The limiter's share, `4b90a09` with its own limiter against the same wasm with main's, over 3 rounds:
  - Close: `cpu` -0.24 [-0.26, +0.07] ms.
  - Overview: `cpu` -1.09 [-1.26, -0.93] ms (real), and p95 +15 to +24 ms (the paired frames).
  - Of the card's overview growth measured against the old-limiter build, about 1.1 ms is frame pairing.

### Bisect pass 1 (`bisect1-web.json`, reference `fl-4b90a09`, 3 rounds)

| Arm (commit) | Close mean | Close `cpu` (delta) | Overview mean | Overview `cpu` (delta) | Draws close / overview |
|---|---|---|---|---|---|
| `4b90a09` (fl) | 38.29 | 22.58 | 43.07 | 20.41 | 1,863 / 990 |
| `4b90a09`, old limiter | 38.35 | 22.34 (-0.24) | 42.73 | 19.27 (**-1.09**) | 1,863 / 990 |
| `6bc3a43` (O6) | 34.46 | 22.54 (-0.01) | 41.96 | 20.36 (+0.09) | 1,609 / 952 |
| `2e331f3` (river + coast scatter) | 34.16 | 30.22 (**+7.64** [7.50, 7.82]) | 41.77 | 26.43 (**+6.23**) | 1,660 / 984 |
| `f1aff3e` (cell cascade skip) | 30.43 | 29.03 (+6.55) | 38.92 | 25.74 (+5.54) | 1,402 / 913 |
| `6b8dca3` (before the limiter fix) | 30.55 | 29.09 (+6.59) | 38.90 | 25.76 (+5.30) | 1,422 / 930 |
| main (`38b15d1`) | 30.42 | 29.11 (**+6.54**) | 38.95 | 25.89 (**+5.65**) | 1,422 / 930 |

All the growth falls between `6bc3a43` and `2e331f3`. Nothing after `2e331f3` adds to it, including the limiter fix: `6b8dca3` and main match.

### Bisect pass 2 (`bisect2-web.json`, reference `fl-6bc3a43`, 3 rounds)

| Arm (commit) | Close `cpu` (delta) | Overview `cpu` (delta) | Draws close / overview |
|---|---|---|---|
| `6bc3a43` | 22.62 | 20.33 | 1,609 / 952 |
| `9c882f4` (cars yield) | 23.08 (+0.47 [0.34, 1.21], not real) | 20.70 (+0.30, not real) | 1,609 / 952 |
| `e9ea60d` (river scatter) | 25.38 (**+2.90** [2.37, 3.51]) | 23.30 (**+2.97**) | 1,656 / 976 |
| `2e331f3` (coast scatter) | 31.50 (**+8.88** [7.36, 9.34]) | 27.56 (**+7.23**) | 1,660 / 984 |

- Session to session the same arm reads up to 1.3 ms apart (`2e331f3`: 30.22 in pass 1 and 31.50 in pass 2), which is why every delta here is paired within its pass.
- The sim's share is also bounded on main with an agents ablation (`ablate-agents-web.json`, 3 rounds). Pausing the agents (photo mode `pause_agents`, which stops the sim tick) saves 1.11 ms at the close camera, not real, and 0.98 ms at the overview, real. That is the whole sim cost, including the `9c882f4` yield rules and the `876609e`/`cf5be40` junction rules.

### Which functions (`profiles/`, one profiled round per build)

These are inclusive ms per engine frame for the systems that moved, from the named builds of `4b90a09`, main and the fix:

| System | `4b90a09` close | main close | fix close | `4b90a09` overview | main overview | fix overview |
|---|---|---|---|---|---|---|
| `hex_play::play::scatter::sync` | 4.03 | 7.55 | 0.00 | 4.12 | 7.46 | 0.01 |
| `hex_play::play::chunks::settle` | 2.10 | 5.16 | 0.00 | 2.22 | 5.20 | 0.00 |
| `bevy_camera::visibility::calculate_bounds` | 4.48 | 5.07 | 4.61 | 4.35 | 4.21 | 4.80 |
| render schedule (all render-world systems) | 9.38 | 8.46 | 8.71 | 6.49 | 5.98 | 6.50 |
| `hex_render::kit_instances::prepare_kit_bind_group` | 0.41 | 0.00 | 0.00 | 0.40 | 0.00 | 0.00 |
| `hex_world::sim::tick` | 0.06 | 0.24 | 0.21 | 0.07 | 0.22 | under 0.3 |
| profiled `cpu` mean | 23.15 | 30.48 | 16.65 | 20.30 | 26.59 | 14.57 |

Inside `scatter::sync` and `settle`, most of the time is `serde_json` (`Value::serialize`, `from_str`), `core::fmt::write`, `BTreeMap` inserts and allocation: building, parsing and rewriting the scatter report string.

- `finish_kit` (`crates/hex-play/src/play.rs`) ran `let (Some(kit), Some(packs)) = (kit.as_deref_mut(), packs.as_deref_mut())` before `packs.poll`. `as_deref_mut` on an `Option<ResMut<PlayKit>>` marks the resource changed whether or not anything is written.
- `scatter::sync` (`crates/hex-play/src/play/scatter.rs`) returns early unless `view`, `spawned` or the kit changed, or a scatter group was removed. Because the kit changed every frame, it did the following on every frame:
  - It re-ran its layout lookup for every scatter tile.
  - It rebuilt `TileScatter` for each tile.
  - It formatted the whole report JSON (one row per scatter tile) and wrote it if it differed.
- `chunks::settle` (`crates/hex-play/src/play/chunks.rs`) runs `update_report` when the report changed. That parses the report JSON, adjusts `entities` and `unbatched_entities`, and serialises it back. The settle-adjusted string never equals what `sync` formats, so (read from the code, not traced) the report changed every frame too.

### The fix (`fix-web.json`, reference main, 5 rounds)

| Camera | Arm | Mean ms | p95 ms | `cpu` mean ms | `cpu` p95 ms | Draws (kit) |
|---|---|---|---|---|---|---|
| close | main | 30.44 | 31.9 | 29.11 | 29.8 | 1,422 (1,287) |
| close | fix | 30.36 (-0.08 [-0.11, -0.05]) | 31.6 (-0.1) | **16.09 (-13.02 [-13.66, -12.88])** | 16.7 (-13.0) | 1,422 (1,287) |
| overview | main | 38.90 | 40.1 | 25.79 | 26.3 | 930 (849) |
| overview | fix | 38.40 (**-0.51** [-0.62, -0.21]) | 40.0 (-0.2) | **15.28 (-10.58 [-10.91, -10.33])** | 15.8 (-10.5) | 930 (849) |

The loads were 1.76-3.12, with nothing discarded or failed and no console errors. Draw counts are identical in every round. The overview's mean moves 0.5 ms although its frame is GPU-bound at 38.4 ms. Why a main-thread cut moves a GPU-bound mean was not traced.

## Criteria

1. **Responsible commits identified.**
   - The culprits are `e9ea60d` and `2e331f3`, which gave river, beach and ocean tiles runtime scatter rules.
   - Measured against `6bc3a43` (paired, 3 rounds): `e9ea60d` +2.90 ms close and +2.97 ms overview, and `2e331f3` +8.88 ms close and +7.23 ms overview. Derived per step, that is +2.43 / +2.67 ms for the river commit and +5.98 / +4.26 ms for the coast commit. The cars-yield commit before them, +0.47 / +0.30 ms, is not real.
   - The mechanism dates from `a672056`: `finish_kit` marked `PlayKit` changed every frame, so `scatter::sync` and `chunks::settle` recomputed and re-serialised the scatter report every frame, at a cost that grows with the number of scatter tiles.
   - O6, the sim rules and the limiter fix add no main-thread work. The limiter fix changes only the overview's reading, by about 1.1 ms of frame pacing.
2. **Growth removed, with no draw regression.**
   - `c032d97` marks `PlayKit` changed only on a frame where a pack arrives.
   - Main-thread time falls 13.0 ms at the close camera and 10.6 ms at the overview. That is 6.5 and 5.1 ms below `4b90a09` (fixed limiter), comparing across sessions.
   - Draws are unchanged: 1,422 close and 930 overview.
   - The mean falls 0.08 ms close and 0.51 ms overview. Both frames are GPU-bound.
   - Runtime scatter and every sim rule stay. Scatter output is unchanged:
     - `make thumbs` was run twice and changed 0 thumbnails.
     - `make shots-scatter` passed every check, including the stream checkpoint where a pack arrives later (`shots-scatter.log`). Its scatter fields match the committed evidence, and its images differ only by JPEG noise (max channel difference 14).
   - The regression test `finish_kit_leaves_the_kit_unchanged_when_no_pack_arrives` fails without the fix (`[true, true, true]`) and passes with it.

## Inferred, not measured

- Per-function times come from one profiled round per build, under a sampling profiler. The timing numbers above come from the unprofiled runs.
- Per-step deltas between `9c882f4` and `e9ea60d`, and between `e9ea60d` and `2e331f3`, are differences of each arm's paired delta against `6bc3a43`, not runs paired against each other directly.
- The fix's distance below `4b90a09` (6.5 ms close, 5.1 ms overview) compares the fix run with bisect pass 1, two sessions about 30 minutes apart, not paired rounds.
- That `chunks::settle`'s report rewrite alternated with `sync`'s every frame is read from the code. The profiles show both systems at their full cost every frame, but no run traced the change flags.
- Nothing else reads `PlayKit`'s change flag: a search of every crate for `is_changed`, `resource_changed` and `Ref<PlayKit>` finds only scatter's `sync` and the new test. Nothing else relied on the kit changing every frame.
- The build scripts here are a record of how the arms were built. They hardcode the build worktree and `/tmp/a021`, so they do not run as-is.
- The pre-existing `calculate_bounds` cost (4.2 to 5.1 ms a frame, the same in `4b90a09`) is now the largest single main-thread system. It is not part of this growth and was not investigated.

## Reproduce

```bash
# macdaddy: one arm per sha (from git archive tars), main and the fix with make build-web
bash docs/perf/large-world-cpu/build-arm.sh <sha>       # expects /tmp/a021/<sha>.tar
bash docs/perf/large-world-cpu/build-named.sh <sha>     # the same with the wasm name section kept
# MiniMac, inside a booked window (copy dist arms, e2e/ without node_modules, web/ui/src and the save; npm ci in e2e/)
ulimit -n 10240
WORLDS=world2066 ROUNDS=5 WARMUP=1 caffeinate -d -u node e2e/perf-close-camera.mjs main=arms/main fix=arms/fix results/fix-web.json
GPU_CALLS=1 WORLDS=world2066 ROUNDS=1 node e2e/perf-close-camera.mjs main=arms/main o5o4=arms/o5o4 results/gpucalls-diag.json
PROFILE_DIR=profiles WORLDS=world2066 ROUNDS=1 node e2e/perf-close-camera.mjs named_main=arms/named-main results/profile-web.json
node e2e/perf-close-camera-summary.mjs results/fix-web.json main results/fix-web-summary.json
python3 scripts/perf/cpuprofile-functions.py A.cpuprofile B.cpuprofile --match '^system '
```
