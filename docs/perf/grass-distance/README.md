# Grass distance: MiniMac frame time (2026-10-26)

This measures the frame-time cost of the Settings → Graphics → Grass distance stops (`grass_distance`, see `docs/agent-operability.md`). Near (x1) is the default, Medium is x1.5 and Far is x2. Very far (x3) was not measured. Every sample, with its loads, renderer and grass state, is in [`results-2026-10-26.json`](results-2026-10-26.json).

## Setup

- **Host.** MiniMac (Mac mini Mac16,10, Apple M4 with a 10-core GPU, 32 GiB, macOS 27.0.1). This is a separate baseline from the M4 Max dev host. No timing ran on the dev host.
- **Build.** `main` at `041ad32`, built with `make build-web` in a fresh worktree on the dev host (dist build id `4544e4c76a`), then copied to MiniMac.
- **Harness.** `e2e/perf-grass.mjs`, unchanged:
  - `GRASS_ARMS=normal` (100% density), `GRASS_RADIUS=10`, `GRASS_DISTANCES=near,medium,far` and `PERF_EXCLUSIVE=1`.
  - World: the 317-tile water world of `e2e/water-world.mjs`, the same world `perf-grass-drag` uses.
  - Rendering: `scenario=play`, High tier, tilt-shift off, seed 1, at 2560x1440 and DPR 1.
  - Browser: uncapped GPU Chromium (`--use-angle=metal`, vsync and frame-rate limit off) under `caffeinate -d -u`.
  - Readout: each run is one fresh browser. It builds the world, sets the camera, waits 700 frames, then reads the 600-frame diag window and `state().grass`.
- **Renderer.** `ANGLE (Apple, ANGLE Metal Renderer: Apple M4, Unspecified Version)`, WebGPU adapter `apple | metal-3 | Apple M4`, Chrome for Testing 153.0.8010.12.
- **Load guard.** Every valid run had a 1-minute load of 2.4 to 4.3 before and after (`e2e/perf-load.mjs`, limit 15). No sample was discarded for load.
- **Order.** Each invocation ran its arms in forward order on even rounds and reversed on odd rounds. The extra play-camera run was a fresh invocation, so its two rounds (rounds 3 and 4) restarted at forward: rounds 2 and 3 ran in the same order. Pairing is within a round, and every run is a fresh browser.

### Cameras

| Camera | Position | Look-at |
|---|---|---|
| low (the perf-grass default, over the start meadows) | (10, 12, 40) | (0, 0, 0) |
| play (the grass-distance checkpoint's 80 m view, pitched 30°) | (-44.64, 40, 90) | (-10, 0, 30) |

### Rounds

The low camera ran 3 rounds. The play camera ran 3 rounds, then 2 extra rounds (rows with `round` 3 and 4, `source_file` `play-extra.json`), because two of its rows were rejected.

The exclusivity guard rejected two play-camera rows, round 0 Medium and round 4 Near. In both, the owner's desktop Chrome on MiniMac rose above 20% CPU during the window (21.8% and 20.8%). Those rows are kept in the JSON with their error and are not counted.

Valid samples per arm: low 3/3/3, play 4/4/5 (Near/Medium/Far).

## Results (web)

Each value is the median over the valid rounds.

| Camera | Stop | Mean ms | p95 ms | p50 ms | Frames > 25 ms (of 600) | Groups | Blades drawn | Instances stored | Mesh bytes |
|---|---|---|---|---|---|---|---|---|---|
| low | Near | 33.33 | 49.2 | 21.2 | 300 | 25 | 179,574 | 269,117 | 8,611,900 |
| low | Medium | 35.99 | 45.8 | 28.7 | 600 | 46 | 365,074 | 546,995 | 17,503,996 |
| low | Far | 39.69 | 40.7 | 39.6 | 600 | 75 | 615,126 | 922,071 | 29,506,428 |
| play | Near | 34.81 | 49.95 | 22.55 | 300 | 16 | 229,016 | 342,578 | 10,962,652 |
| play | Medium | 37.80 | 44.3 | 34.45 | 600 | 40 | 482,283 | 722,063 | 23,106,172 |
| play | Far | 50.58 | 51.3 | 50.5 | 600 | 72 | 860,500 | 1,290,572 | 41,298,460 |

The grass counts at the end of each window were identical in every round of each arm. No valid run logged a `console.error` or a line matching `BAD_TEXT` (`e2e/console-errors.mjs`: `ERROR`, `uncaptured` or `invalid`, which covers WebGPU uncaptured validation errors). The two rejected rows recorded no console state, because the exclusivity error replaced the row.

### Paired deltas (stop minus Near, within a round)

Each delta is the median of the paired deltas. The bracket shows their min and max, which is the noise floor. A delta is real when every paired delta has the same sign and the median is larger than their spread.

| Camera | Delta | Mean | p95 |
|---|---|---|---|
| low | Medium − Near (3 pairs) | **+2.65 ms** (+2.62 to +2.68), +8.0%, real | −3.7 (−3.7 to −3.4) |
| low | Far − Near (3 pairs) | **+6.42 ms** (+6.32 to +8.13), +19.3%, real | −8.5 (−8.8 to −5.1) |
| play | Medium − Near (3 pairs) | **+2.98 ms** (+2.94 to +3.47), +8.6%, real | −5.5 (−5.8 to −5.0) |
| play | Far − Near (4 pairs) | **+15.75 ms** (+15.71 to +18.06), +45.2%, real | +1.35 (+1.1 to +6.6), noise |

### Reading the numbers

- **Use the mean, not p95 or p50.** At Near, the frame times split into two modes. Exactly 300 of the 600 frames are over 25 ms in every Near run, with p50 near 21 ms and p95 near 50 ms. At Medium and Far, every frame is over 25 ms, and p50 sits close to p95. So p95 falls from Near to Far even though the mean rises, and p50 rises far more than the mean. The mean is the only one of the three that tracks the cost here.
- **Cost scales with drawn blades.** The farther stops build more tiles' groups and draw more blades. Far draws 3.4x Near's blades at the low camera and 3.8x at the play camera. The play camera looks across the meadows, so it reaches the most new grass at Far.
- **Medium costs about 3 ms of mean frame time at both cameras.** Far costs 6.4 ms at the low camera and 15.8 ms at the play camera.
- **Memory.** Instance memory follows the groups: 8.6 to 11.0 MB at Near, 17.5 to 23.1 MB at Medium and 29.5 to 41.3 MB at Far.

## Native

Not measured. `e2e/perf-play-native.mjs` hard-codes `--grass off` and its own camera, and `perf-grass.mjs` is web only. A native arm would need a new harness and a release build, which was out of scope for this run.

## Reproduce

Ship `dist/`, `e2e/` (without `node_modules`) and `web/ui/src/` to a MiniMac scratch directory (`~/.claude/projects/-Users-probello-Repos-hex-world/memory/minimac-perf-host.md`), run `npm ci` in `e2e/`, then:

```bash
export PATH=/opt/homebrew/bin:$PATH; ulimit -n 10240; mkdir -p docs/perf/grass-distance
PERF_EXCLUSIVE=1 GRASS_ARMS=normal GRASS_RADIUS=10 GRASS_DISTANCES=near,medium,far GRASS_ROUNDS=3 \
  PERF_OUT=docs/perf/grass-distance/low.json caffeinate -d -u node e2e/perf-grass.mjs dist
PERF_EXCLUSIVE=1 GRASS_ARMS=normal GRASS_RADIUS=10 GRASS_DISTANCES=near,medium,far GRASS_ROUNDS=3 \
  GRASS_CAMERA=-44.64,40,90,-10,0,30 \
  PERF_OUT=docs/perf/grass-distance/play.json caffeinate -d -u node e2e/perf-grass.mjs dist
```
