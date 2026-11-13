# Mega tiles pre-merge evidence: flat-world parity, flat-world perf, massif grass units (2026-10-30)

The question: does a world with no massif render pixel-identically to main and perform the same, and what does a massif's grass build cost per unit? The orchestrator booked and checked in every window: macdaddy load-tolerant for the builds, macdaddy load-sensitive 01:35-02:05Z for the native captures, and MiniMac load-sensitive 01:35-02:35Z.

## Verdicts

- **Parity: not identical.** At the deterministic play camera the branch differs from main by sub-pixel speckle on swaying LOD1 foliage (pines, wildflowers) and the river surface. That is 300,377 of 1.6 M pixels, 22,815 of them by more than 2/255, max 47. Main against main differs by 260 pixels (13 over 2/255, max 6). The difference is deterministic: both main runs read the same against the branch. It bisects to **`151b3a1`** ("lazy massif peak in an existing material; ring shows only footprints while one is selected"). `b1ef668`, its parent, still matches main. Tiles, agents, LOD selection and the whole state JSON are identical. Nothing is moved, added or missing. A second, smaller difference at the overview (about 500 pixels, about 57 over 2/255, max 20) comes after `151b3a1` and was not bisected.
- **Perf: pass, within noise.** At the close camera the branch matches main: 27.34 against 27.34 ms mean, paired per-round deltas −0.24 to +0.22 ms, mean −0.00. At the overview it reads 37.18 against 37.09 ms, with paired deltas −0.31 to +0.51 ms, mean +0.09. Each arm's round-to-round spread is about 1.4 ms. Draws are identical in every round (1,472 and 962; kit 1,337 and 881).
- **Massif grass: within budget.** The slowest massif build unit was 2.3 ms (median 2.1) against the PRD §10 budget of 4 ms per unit. The 7 units build over 6 frames, with the frame's whole grass work at most 3.2 ms. A single Meadow coming into range measured 3.3 ms per unit (median 2.6 cold, 3.0 warm), with up to 4.2 ms of frame work.

## Arms and hardware

| Arm | Commit | Native binary sha1 | Web `index.html` sha1 | wasm sha1 |
|---|---|---|---|---|
| main | `b3dbc9a` (no code change since the merge base `dc85b9a`) | `ca66b364…` | `20eb9f76…` (the same as 2026-10-29's main) | `99856604…` |
| branch | `mega-tiles` `1ed817f` | `1cf09af8…` | `b4190729…` | `2164b749…` |

- **Builds.** `cargo build -p hex-app --release` and `make build-web` in each checkout (window 1). The dists were snapshotted to `/tmp` and copied from there, and both were verified by `shasum` on MiniMac before timing. Every native capture checked the checkout HEAD and binary sha first (`run-parity.sh`).
- **macdaddy.** Mac16,5, Apple M4 Max (16 CPU, 40 GPU cores), macOS 27.0.1. It was not idle: the 1-minute load ran 13.7-28.4 during the captures. A par-rt-db gate ran on it from about 01:40 to 01:44Z, inside the window. That overlapped only the diagnostic runs (the manifest swap, force-LOD0 and LOD tint), not the main/branch/main chain (01:38:26-01:39:45Z). No frame came back black, short or timed out in any of the 12 runs (all logs re-checked after the bisect). Fixed-clock native frames do not depend on load.
- **MiniMac.** Apple M4 (base), GPU Chromium (ANGLE Metal Renderer: Apple M4), node 24.22.1, playwright 1.63.0. Load was 1.84-3.49 throughout, no sample was discarded and there were no console errors. Staging, 3a and 3b ran one at a time from 01:35 to 01:59Z. The run directory `~/perf/mega-tiles-1030` was deleted afterwards.

## 1. Native flat-world pixel parity (macdaddy)

**World and script.** `flat-parity.json` builds a 45-tile world from the start Meadow with no massif:

- the river, coast and ocean world of `docs/checkpoints/scatter/scatter-mixed.json`, less its beach at (-2, -2);
- two `house_cluster`s joined by the three-tile road, with one car driving;
- three farms and a wheat field fenced by walls and hedges against forest and Meadow;
- three level-1 Meadows (0, 1), (-1, 2) and (-2, 2) with cliff columns.

Every placement was checked offline first against `web/ui/src/rules.ts` (hex-core's parity-tested rules), and the dumped state confirms all 45 tiles at levels 0 and 1. After a 900-frame settle the script enters photo mode (DOF off, agents paused). It then shoots 8 cameras with 240 frames each: `overview`, `play`, `play-road-farm`, `close`, `low-road-cars`, `low-level1`, `low-coast` and `low-farm`.

**Run.** `run-parity.sh <checkout> <label> <out> <head> <bin-sha>` runs: `--scenario play --size 1600x1000 --seed 1 --tier high --fps off --tilt off --clock fixed --water_clock fixed --offscreen on --grass normal`, under `caffeinate -d -u`, in the order main, branch, main. `diff.py` compares the PNGs byte for byte and pixel for pixel. All the pair numbers, and every run's load, HEAD and binary sha, are in `parity-diff.json`. The PNGs themselves are not committed; they were in `/tmp`.

| Camera | main vs main | main A vs branch | main B vs branch |
|---|---|---|---|
| overview | 218 / 3 / 6 | 502 / 57 / 20 | 489 / 56 / 20 |
| play | 260 / 13 / 6 | 300,377 / 22,815 / 47 | 300,386 / 22,810 / 47 |
| play-road-farm | 297,310 / 26,907 / 130 | 324,221 / 34,609 / 90 | 284,312 / 25,236 / 89 |
| close | 731,754 / 130,030 / 73 | 793,676 / 160,263 / 77 | 703,238 / 122,074 / 45 |
| low-road-cars | 535,798 / 84,784 / 69 | 559,724 / 92,382 / 54 | 515,511 / 75,832 / 55 |
| low-level1 | 300,323 / 42,989 / 65 | 338,368 / 70,082 / 72 | 309,156 / 46,514 / 71 |
| low-coast | 246,321 / 52,456 / 91 | 262,209 / 56,332 / 87 | 244,517 / 50,935 / 82 |
| low-farm | 429,028 / 56,385 / 82 | 464,999 / 74,676 / 71 | 420,753 / 65,099 / 47 |

Each cell reads differing pixels / pixels differing by more than 2/255 / the largest channel difference, of 1,600,000 pixels. No frame is byte-identical in any pair.

**The control is only deterministic where no grass builds in view.** The overview and the play camera are 60 m or more from every tile, past the grass build radius, and main against main is nearly identical there. At the six cameras where grass is in range, main against main differs on 0.25-0.73 M pixels. Grass groups build under a wall-clock 4 ms per-frame budget (`hex_play::play::grass`), so the frame a group lands on varies between runs even under `--clock fixed`. Whatever that changes persists past the 240 frames each camera waits, so it is more than TAA history (possibly draw or entity order); it was not traced. The runs fall into modes. Main A and a later branch-binary run both have 0 differing pixels at `low-coast` and `low-farm` (byte-identical), while main B does not match either. At those six cameras the branch sits inside the main-vs-main spread, so they cannot show a difference either way. A pixel-exact check there needs grass builds pinned to frames, as `--thumbs` does with its clocks.

**The play-camera difference (bisected).**

- `play-crop-main-branch-diff.jpg` shows main, branch and their difference ×6 for a 300x300 crop of the centre forest at 2x. `play-diff-x6.jpg` is the whole frame's difference ×6, and `play-main.jpg` is the frame.
- The speckle covers the pine needles, the wildflowers and the river surface. Rocks, ground, roads, buildings and shadows do not differ. Nothing moved.
- **Not the state.** The full state JSON at the dump is identical except for the branch's new `play.mega_slots` (`flat-state-main.json`, `flat-state-branch.json`). That covers tiles, the car (id 2 at x -18.23), the boids and fish digest, and 1,173 frames.
- **Not the manifest.** The branch binary with main's `assets/kit/manifest.json` (the massif piece removed from the hero pack, so the hero pack's mesh and texture load order is main's) still reads like the branch at the play camera: 211 pixels from the branch, 300,383 from main.
- **It needs LOD1.** With `debug force_lod "0"`, main against branch at the play camera is 203 / 10 / 8 (`lod0-*`).
- **LOD selection is the same.** With the LOD tint on, the play frames match (14 / 0 / 1), and the LOD counts are equal (46 LOD0 and 30 LOD1 parts in both).
- **Bisect.** Native builds from `git archive` of each commit, play and overview frames against main A:

  | Commit | play vs main | overview vs main |
  |---|---|---|
  | `7f035b6` | 152 / 7 / 4 | 161 / 1 / 6 |
  | `3c5aaf5` | 147 / 10 / 6 | 154 / 1 / 3 |
  | `b1ef668` | 173 / 5 / 6 | 159 / 4 / 5 |
  | `151b3a1` | **300,387 / 22,816 / 47** | 154 / 3 / 5 |

  `151b3a1` against the branch head at the play camera reads 202 / 4 / 4.

**Cause, as far as it was traced.** For a world with no massif, `151b3a1` makes two changes:

1. `PlainAssets` no longer allocates the placeholder peak mesh and granite material at startup. The massif's first spawn builds the peak lazily (`SlopeMeshes::massif_peak`).
2. The slot ring's signature gains `footprint_selected`. That is inert here: a Meadow is selected and photo mode hides the ring.

**The mechanism is unknown.** The startup asset counts do not explain it. The commit's own test holds the startup allocation at main's 6 meshes and 12 materials, and main allocates the same startup set as `151b3a1` (5 `meshes.add` and 3 material groups in `PlainAssets::new`). Yet `b1ef668`, which allocated one more of each, matched main.

What is known:

- The difference lives only in LOD1 kit draws.
- The state, the LOD choice and the geometry are identical.
- The kit manifest is ruled out.

Play-mode kit draws bin on load-order mesh `AssetId`s (CLAUDE.md; thumbs mode bins on mesh family for that reason), so a change in runtime mesh allocation order is one candidate. Nothing here shows that it is the cause. Two A/B runs would settle it, and both are product changes outside this measurement:

1. `151b3a1` against `b1ef668` with the draw-bin key on the mesh family.
2. `151b3a1` with the ring signature change reverted.

The overview's small branch difference (about 57 pixels over 2/255) appears after `151b3a1` and was not traced. Update 2026-11-13: it was traced to `6942098`, the boid compute pass's peak lift, and fixed (`../mega-tiles-2026-11-13-overview/`).

## 2. Flat-world perf (MiniMac)

This repeats `../mega-tiles-2026-10-29/`'s run exactly: `WORLDS=world2066 ROUNDS=5 WARMUP=1 caffeinate -d -u node e2e/perf-close-camera.mjs main=dist-main branch=dist-branch`. That is main's harness, the 2,066-tile save and the default keys (`scenario=play tier=high tilt=off fps=off grass=off seed=1`), at 2560x1440, uncapped. The run had 1 warm-up (main, discarded) and 5 paired rounds with alternating order, all inside the `e2e/perf-load.mjs` guard. Run 01:36-01:48Z, load 1.84-3.21, 0 samples discarded, 0 error rows, 0 console errors. Raw: `minimac-close-camera.json` and `.log`.

| camera | arm | mean ms | p95 ms | cpu mean ms | draws | kit draws | mean range |
|---|---|---|---|---|---|---|---|
| close | main | 27.34 | 29.24 | 12.18 | 1472 | 1337 | 26.27-27.65 |
| close | branch | 27.34 | 29.28 | 12.12 | 1472 | 1337 | 26.49-27.69 |
| overview | main | 37.09 | 38.64 | 12.21 | 962 | 881 | 35.95-37.54 |
| overview | branch | 37.18 | 38.80 | 12.14 | 962 | 881 | 36.46-37.66 |

Paired deltas (branch minus main, rounds 0-4): close +0.22, −0.24, −0.05, +0.04, +0.02 (mean −0.00); overview +0.51, −0.31, +0.32, −0.16, +0.08 (mean +0.09). Every delta is well inside an arm's own 1.4 ms round-to-round spread, and 2026-10-29's numbers (27.25/27.17 close, 37.02/37.10 overview) reproduce within 0.1-0.2 ms.

| round | arm | load before → after | close mean | overview mean |
|---|---|---|---|---|
| 0 | main | 3.02 → 3.21 | 26.27 | 35.95 |
| 0 | branch | 3.21 → 2.67 | 26.49 | 36.46 |
| 1 | branch | 2.67 → 1.84 | 27.28 | 36.91 |
| 1 | main | 1.84 → 2.01 | 27.52 | 37.22 |
| 2 | main | 2.01 → 1.85 | 27.65 | 37.34 |
| 2 | branch | 1.85 → 2.42 | 27.60 | 37.66 |
| 3 | branch | 2.42 → 2.63 | 27.69 | 37.24 |
| 3 | main | 2.63 → 2.87 | 27.65 | 37.40 |
| 4 | main | 2.87 → 2.61 | 27.60 | 37.54 |
| 4 | branch | 2.61 → 2.96 | 27.62 | 37.62 |

## 3. Massif grass build units (MiniMac, branch dist)

`grass-units.mjs` is a scratch harness; copy it into `e2e/` to run it as `REPEATS=5 node e2e/grass-units.mjs dist-branch out.json`. Keys: `scenario=play tier=high tilt=off fps=off grass=normal seed=1`, 2560x1440, GPU Chromium, real clock. Each repeat is a fresh browser per world, inside the load guard.

The world is a row of Meadows east from the start tile, ending in the target at (8, 0): a massif centred there, or a single Meadow. The camera starts over the origin, where the start Meadows build and pay the Meadow ground's one-time preparation. Then one `camera` command jumps it east of the target, so only the target is inside the 60 m build radius. The harness checks that the target was not built before the jump.

`state().grass` is read every animation frame until 90 frames pass with no new upload. That reading is **cold**. The camera then goes home, the target's groups drop past 72 m, and it jumps back for a **warm** reading (that definition's ground already prepared). The harness also checks that the units add up: 7 for the massif and 1 for the Meadow. Every repeat passed. A dry run of one repeat (`grass-dry`, the same numbers) preceded the 3a run.

`last_build_ms` is the slowest unit in a frame that built units. In a frame that builds 2 units it bounds each unit rather than timing it. `last_frame_build_ms` is that frame's whole grass work, the span the 4 ms per-frame start rule holds.

| target | reading | units / frames | slowest unit ms (max, median) | frame grass work ms (max, median) | blades |
|---|---|---|---|---|---|
| massif | cold | 7 / 6 every repeat | 2.3, 2.1 | 3.0, 2.8 | 92,877 |
| massif | warm | 7 / 6 every repeat | 2.3, 2.1 | 3.2, 3.0 | 92,877 |
| Meadow | cold | 1 / 1 | 3.3, 2.6 | 4.2, 3.5 | 18,330 |
| Meadow | warm | 1 / 1 | 3.1, 3.0 | 4.2, 4.1 | 18,330 |

Every repeat builds the massif the same way: 2 units in the first frame, then one a frame (repeat 0 cold: 2.0, 2.1, 1.9, 2.2, 2.0, 2.0 ms per frame's slowest unit; frame work 2.6-2.9 ms). A massif unit (a ring or centre cell, about 13,300 blades) costs less than a whole Meadow (18,330 blades), and splitting into seven units keeps every massif frame under 3.2 ms. Load was 2.44-3.49, no samples were discarded and there were no console errors. Raw: `minimac-grass-units.json` and `.log`.

**Against the PRD §10 budget of 4 ms per grass build unit, the massif's worst unit is 2.3 ms: pass.** A single Meadow's frame can reach 4.2 ms of whole grass work. That is the single-tile case the start rule does not bound (`docs/perf/grass-build/README.md`), not a massif cost.

## Files

- `flat-parity.json`, `run-parity.sh` and `diff.py`: the native parity script, its runner and the comparison.
- `parity-diff.json`: every pair compared, including the diagnostic and bisect runs, with each run's load, HEAD and binary sha.
- `flat-state-main.json` and `flat-state-branch.json`: the state dumps.
- `play-crop-main-branch-diff.jpg`, `play-diff-x6.jpg` and `play-main.jpg`: the play-camera difference.
- `minimac-close-camera.json` and `.log`: 3a. `minimac-grass-units.json` and `.log`, with `grass-units.mjs`: 3b.
