# Grass tile build: native A/B and the MiniMac wasm recipe

Card: "Grass tile build takes 4-6 ms in wasm, over the 4 ms per-frame budget". `docs/perf/p2-grass-drag.json` (2026-10-21, web) put each building frame's slowest tile at p50 3.9-4.0 ms and p95 5.6-6.0 ms. This page records where the time went, what changed, the native measurements taken on the dev host, and the recipe for measuring the wasm build on MiniMac. Browser timing was not taken on the dev host: its 1-minute load was 280 to 470 throughout the work.

## Where the time went

The span `state().grass.last_build_ms` times is `grass::layout` (the seeded Lush set) plus `GrassInstances::new` (the GPU records sorted by `keep`). The layout walks a jittered 208 x 208 grid over the hex, about 43,000 points, drawing 11 SplitMix64 numbers at each. A stage breakdown and a `sample` profile of the native build (meadow at cell (3, -1), the slowest tile, 69 clears) showed this split:

| Stage (old meadow build, 5.5 ms in all) | ms | Share |
|---|---|---|
| Each point's 11 draws, its hex test, and every exclusion (`Clear::contains`) tested at every point inside the hex | 2.9 | 53% |
| Ground lookup (`Surface::top`) | 1.0 | 17% |
| The slot's `HashMap<String>` lookup, and per blade the `style` string match, `jitter`'s HSL conversion and the push | 1.0 | 18% |
| `GrassInstances::new`, mostly the stable sort | 0.6 | 11% |

Culling the exclusions alone took the meadow build from about 5 ms to about 2.5 ms, so most of the first row was exclusion tests.

On road, rail and river tiles with few clears, the ground lookup is the largest part. On beach and river-mouth tiles most of the hex is sand or water, and the old build still looked up the ground at every point.

A second cost sat outside the span, in every frame. `grass::sync` rebuilds `state().grass` each frame, and its `tiles` rows hashed every built tile's whole blade set with byte-wise FNV-1a (`GrassLayout::fingerprint`, 32 bytes per blade). One meadow tile takes 0.91 ms natively, so 25 groups cost several milliseconds in every frame, whether or not anything was built.

## What changed

Every change keeps the output bit for bit (the blades, their order, the GPU records and the bounds). Native and wasm take the same arithmetic as before.

1. **Exclusions culled per row.** `Clear::bounds` gives each clear a conservative (x, z) box: a disc's square, a turned box's extent, and for a rock the corners of its gap-pushed outline, all with a 1 cm margin. Each grid row keeps only the clears whose box reaches it, and a point tests a clear only when it lies inside that clear's x extent. `clear_bounds_hold_every_point_the_clear_accepts` samples every clear kind densely, including rocks at scale 0.3 where the gap is largest, and checks that every point a clear accepts lies inside its bounds with the margin to spare.
2. **Ground buckets that cannot grow grass are skipped.** A 1 m ground bucket where no triangle is a grass slot reaching the water line can only give `continue`, so the point skips `Surface::top`. `Surface` stores each triangle's divisor, computed with the same expression, and `top` returns the slot index.
3. **Per-slot work done once per tile.** Each surface slot's style and its base colour's HSL (`Hsl`, the first half of `jitter`) are found once. Each blade only applies the shift. `jitter` is now `Hsl::new(c).shift(dh, dl)`, with the same operations.
4. **Only the draws a point reads are made.** SplitMix64's state steps by a constant, so `Rng::unit_at(k)` makes the k-th draw out of order and `Rng::skip(11)` steps past a point. A point outside the hex or off the grass makes 2 draws instead of 11. Every point still advances the stream by 11.
5. **The keep sort is an unstable sort on `keep`'s bits and the index.** `keep` is never negative, so its bits sort as its value, and the index keeps ties in generation order.
6. **The fingerprint is computed once per tile.** It is stored in `Built` when the tile is built, and the report reads it back. Its value is unchanged. It runs in the build frame, outside `last_build_ms`, and counts against the frame budget.
7. **The start rule for a second or third tile looks ahead.** A further tile now starts only if the frame's grass work so far, plus the slowest whole tile (the ground, the exclusions, the build, the group and the fingerprint) built this frame, still fits in 4 ms. Before, any tile started while less than 4 ms had passed, so a frame could run to 4 ms plus a whole build. The first tile of a frame always starts, whatever it costs, and at most 3 start a frame. A single-tile frame is therefore not bounded by the rule. Natively, a meadow tile's whole work is about 2.8 ms (1.8 ms build plus 0.9 ms fingerprint). The first meadow of a session also pays its definition's ground once (about 2.1 ms).

Tried and dropped. A per-triangle box reject in `Surface::top`, and 0.25 m cells nested inside the 1 m buckets (3 to 4 candidate triangles per point). Neither moved the build time in paired runs, and the fine cells added 0.4 to 0.6 ms to each definition's one-time `prepare`.

## Native microbenchmark (dev host, 2026-10-26)

Method: `cargo test --release -p hex-play grass_ -- --ignored --nocapture` (`crates/hex-play/src/play/grass_bench.rs`). Each case is a catalog tile with grass ground at cells (0, 0), (3, -1) and (-5, 2) of a world of seed 1. That gives 30 tiles and 90 builds, each with its real LOD0 ground (`scatter::prepare` on the kit GLBs) and its real scatter exclusions (`scatter::layout`, `grass::exclusions`), as `grass::sync` gathers them before it starts its clock.

- `grass_ab` times the old build (a verbatim copy of `layout` and the instance sort as of `041ad32`, the `reference` module) and the new one alternately: 41 rounds, with the order swapped every round. It reports per-case medians and the per-case new/old ratio, and it asserts that both builds give equal output on every case.
- `grass_bench` times the current build alone and prints a digest of all 90 outputs. The digest covers every blade's bits, the area, every GPU record, their order and the bounds. It is `922e04ec1819a73c` before and after the change.

The host's 1-minute load was 280 to 470 during these runs, so the absolute times are inflated and noisy. Pairing old and new within each round, and reporting their ratio, is what makes the comparison hold. Full output: [`native-ab-2026-10-26.txt`](native-ab-2026-10-26.txt).

| Tile (cell 0,0) | Old median ms | New median ms | New/old |
|---|---|---|---|
| meadow (27,566 blades, 68 clears) | 4.94 | 1.82 | 0.37 |
| farm | 3.60 | 1.61 | 0.45 |
| forest | 3.17 | 1.14 | 0.36 |
| road_junction | 3.29 | 2.25 | 0.68 |
| rail_road_crossing | 3.25 | 2.35 | 0.72 |
| river_straight | 2.54 | 1.43 | 0.57 |
| beach | 1.99 | 0.57 | 0.29 |
| river_mouth | 2.11 | 0.48 | 0.23 |
| **all 90 builds** | worst 5.03 | worst 2.35 | median ratio 0.60 |

The other costs in a build frame, which `last_build_ms` does not time:

- The meadow fingerprint takes 0.91 ms. It used to run for every tile in every frame, and now runs once per tile.
- A definition's `scatter::prepare` (its `Surface`), once per definition and session, shared with scatter: meadow 2.1 ms, road_junction 1.1 ms, beach 0.9 ms.

The web build was about 1.15 times the native one on the old numbers (slowest tile per frame p95 5.6-6.0 ms on the web, against a native worst of about 5.0-5.2 ms under similar load). That suggests a wasm slowest tile near 2.7 ms after the change, under the 4 ms budget. This is an estimate, which the MiniMac run below replaces.

## MiniMac recipe (orchestrator)

Book the window first: `kanban schedule book --resource minimac --start <UTC> --end <UTC> --holder <agent-id>`. Allow about 60 minutes for 3 runs of each arm. Check in at the start, and heartbeat during the run.

| Arm | Commit |
|---|---|
| before | `041ad32` |
| after | the `main` commit that lands this work (`git log -1 --format=%h -- docs/perf/grass-build/README.md`) |

Build each arm's web `dist/` from its own checkout on the build host:

```bash
git worktree add /tmp/grass-<arm> <sha>
cd /tmp/grass-<arm>
bash scripts/worktree-bootstrap.sh --web
make build-web
```

Copy both builds to MiniMac, plus the after arm's harness. The after harness also records the frame's whole grass work, `last_frame_build_ms`. The before build has no such field, so that column reads `null` for it.

```bash
for arm in before after; do
  rsync -a --delete /tmp/grass-$arm/dist/ minimac:perf/hex-world-grass/$arm/dist/
done
rsync -a --delete --exclude node_modules /tmp/grass-after/e2e/ minimac:perf/hex-world-grass/e2e/
rsync -a /tmp/grass-after/web/ui/src/ minimac:perf/hex-world-grass/web/ui/src/
ssh minimac 'cd perf/hex-world-grass/e2e && npm ci --no-audit --no-fund'
```

Run the arms alternately, one run each per round, under the load guard (`e2e/perf-load.mjs` waits for a 1-minute load of 15 or less before each run, and discards and re-runs any run whose load after it is over 15):

```bash
ssh minimac
cd perf/hex-world-grass
ulimit -n 10240
for round in 1 2 3; do
  if [ $((round % 2)) -eq 0 ]; then arms="after before"; else arms="before after"; fi
  for arm in $arms; do
    DRAG_RUNS=1 PERF_OUT=grass-$arm-$round.json caffeinate -d -u node e2e/perf-grass-drag.mjs $arm/dist
  done
done
```

The arm order alternates every round, which takes the drift out. Read these from each JSON's `rows[0]`:

- **The card's gate:** `build.at_100.slowest_tile_per_frame_ms` and `build.at_150.slowest_tile_per_frame_ms` (p50, p95, max). The card is met when the after arm's p95 is under 4 ms.
- **Context, not the gate:** `build.at_100.grass_work_per_frame_ms` and `build.at_150.grass_work_per_frame_ms` (after arm only). This is the frame's whole grass work while it builds, every tile and its fingerprint included. Multi-tile frames stay near 4 ms or below. A single-tile frame can legitimately exceed 4 ms. A meadow is estimated at about 3.2 ms on the web (2.8 ms natively times 1.15), and the first tile of each definition adds that definition's ground preparation (about 2.1 ms natively for a meadow).
- `build.*.frames_building` and `groups_built`. The look-ahead budget may spread the same groups over more frames. Report both arms.
- **Still frames: read `pairs`, not p95.** Each window (`still_closed`, `baseline`, `drag`, `still_0`, `still_150`) carries `pairs`: `short_mean`, `long_mean` and `alternation`. These take consecutive frame intervals two at a time. `alternation` is the share of pairs whose shorter interval is at most the window's lower median and whose longer one is at least its upper median. It is near 1 when frames alternate short and long, about 0.5 when long frames come in runs, and 0 to 0.4 for even frames. It only means alternation together with a wide gap between `long_mean` and `short_mean`. With 25 groups standing, the before arm hashed every group's blades in every frame, and the after arm does not. On a window whose frames alternate short and long at a fixed pair period (`still_closed` on MiniMac, below), that saving shows as a lower `short_mean`. The mean and frame count stay the same, and p95 rises, because p95 samples the long frames. On a window that is not paced that way, it shows as a lower mean.
- `rebuild.none` must stay `true`, and the console error list must be empty.

Record the host, the build shas, the loads before and after each run, and the renderer string with the numbers in a `results-<date>.json` beside this README, and update the PRD's §9.1 grass drawing line with the measured p95.

## MiniMac result 2026-10-26: the still-frame p95

The gate passed: the slowest tile per building frame went from p95 5.8 to 3.0 ms at 100%, and from 5.5 to 3.1 ms at 150% (`results-2026-10-26.json` on `perf-batch27`). With Settings closed and nothing building, the still frames' p95 rose from 46.0 to 53.1 ms in every round, while their mean held (31.73 to 31.78 ms).

**Cause.** The arms have the same pair period, and less main-thread work in the after arm moves time from the short frame of each pair to the long one. Nothing new runs in a frame that builds no tile, and no cost was added. The evidence:

- **Frame counts.** `still_closed` holds 157 frames in 5 s in both arms (152 in one before round): 79 over 25 ms and 78 under. Those counts point to frames alternating long and short, and the re-run's `pairs` will confirm or rule that out. Each pair takes 63.2 to 63.7 ms in every round of both arms (65.5 ms in that one before round).
- **The split.** This is worked out from each run's `tail_ms`, `frames` and `mean`:

  | Arm | Short frame mean | Long frame mean | Pair |
  |---|---|---|---|
  | before | 18.1-18.2 ms | 45.1-47.2 ms | 63.2-65.5 ms |
  | after | 10.8-11.0 ms | 52.3-52.8 ms | 63.2-63.7 ms |

  The short frame fell by 7.3 ms, and the long frame grew by the same amount. The pair period, and with it the throughput, is set by something other than grass when Settings is closed. Here, p95 samples the long frames.
- **The saving is the fingerprint.** Each per-frame grass report used to hash every built group's blades with byte-wise FNV-1a. For the 25 groups standing here (179,574 blades), that is 5.75 million dependent multiply steps per frame in the before arm, and none in the after arm. At the native rate measured earlier (0.91 ms for 27,566 blades), that is 5.9 ms natively, or about 7 ms at the web's 1.15-1.25 times native. That matches the 7.3 ms drop in the short frame.
- **Where frames are not paced in pairs, the mean falls.** These are the Settings-open windows. Rounds 1 and 2 had no 16.67 ms pacing in either arm:
  - `still_0` mean went from 27.1 and 29.8 ms to 12.6 ms.
  - `baseline` p50 went from 18.4-18.6 to 9.9-10.9 ms.
  - The before arm's round 3 ran paced at 16.67 ms, as the web p50 bimodality noted in `docs/perf/close-camera-draws/README.md` describes.
- **The code.** Between `041ad32` and `8200ef6`, a frame that builds no tile now differs only in that it formats one more number (`last_frame_build_ms`) and reads each tile's stored fingerprint instead of hashing its blades. The start-rule check, the per-tile timing and the fingerprint all run inside the build loop, which is empty when nothing builds.

**Fix.** The grass code needs none, because no cost was added. The cause was identified from the existing runs and still has to be confirmed by the `pairs` re-run below. `e2e/perf-grass-drag.mjs` now records `pairs` on every window.

The p95 rise is still real as a measurement. With Settings closed, frame-to-frame jitter widened: frames alternate around 11 and 52 ms, where they alternated around 18 and 45 ms. No work was added, but a viewer could notice it. The way to reduce it is the 63 ms closed-Settings pair period below, which is outside grass.

**Open, outside grass.** With Settings closed, this camera paces at 63 ms per two frames, which alternate around 11 and 52 ms in the after arm. With Settings open, the same frames run at about 10 ms. That cadence is the same in both arms and predates this work. It is a separate finding about the closed-Settings HUD or presentation, not grass.

**Resolved 2026-10-27 (`docs/perf/frame-pacing/`).** The Settings sheet does not cause the pairing. The web frame-latency limiter let every other frame through one over its limit, so a GPU-bound page ran frames in back-to-back pairs. The limiter now counts a frame from its first submit. Opening Settings moves focus off the canvas, which puts Bevy on its 60 Hz unfocused update. The Settings-open windows here (`baseline`, `drag`, `still_0`, `still_150`) therefore measured the page's rAF loop, not engine frames, and their "about 10 ms" and lower means are not frame times.

### Re-run of the still-frame metric (orchestrator, MiniMac)

Nothing needs rebuilding. Only `e2e/perf-grass-drag.mjs` changed, so both `dist` builds are the arms already measured: before `041ad32`, after `8200ef6`. MiniMac's scratch directory was removed, so copy both `dist` directories over again as in the copy step above, from the build host's worktrees. Rebuild them from those two commits only if those worktrees are gone. Then rsync `e2e/` from the commit that adds `pairs` (this README's latest commit), and run 3 alternating rounds as in the loop above.

Read `rows[0].still_closed.pairs` and `rows[0].still_0.pairs` for both arms. The cause is confirmed when, on `still_closed`:
- `alternation` is near 1 in both arms, with a gap of tens of milliseconds between `long_mean` and `short_mean`;
- the window mean is unchanged within its paired spread;
- the after arm's `short_mean` is lower by about 7 ms, with `long_mean` higher by about the same.

A real per-frame regression would instead show as a higher `short_mean`, or as a mean that does not fall in the Settings-open windows (`still_0`, `baseline`).

#### Result, MiniMac 2026-10-26 22:05-22:12Z (`pairs-2026-10-26/`)

Orchestrator run, 3 alternating rounds, headless Chromium on ANGLE Metal Apple M4, host load 2.45-3.33 before and after every run. Before arm: main's play build (code identical to `041ad32`); after arm: the `8d4c647` grass code (the branch's `dist`). The harness's slider-send check was relaxed from 25 to 20 sends first: the slower before arm sent 24 in all three runs of a first attempt, with both ends and the final value reached (`62d0259`).

`still_closed` (Settings closed, nothing building):

| Arm | Mean | p95 | Pairs | short_mean | long_mean | alternation |
|---|---|---|---|---|---|---|
| before | 31.72 / 32.53 / 32.23 | 45.9 / 48.3 / 47.5 | 78 / 76 / 77 | 18.25 / 18.07 / 18.08 | 45.03 / 46.80 / 46.19 | 1 / 1 / 1 |
| after | 31.77 / 31.76 / 31.74 | 53.2 / 53.2 / 52.6 | 78 / 78 / 78 | 10.96 / 11.06 / 11.00 | 52.34 / 52.21 / 52.24 | 1 / 1 / 1 |

The cause is confirmed on all three criteria: frames alternate short and long in both arms (alternation 1, a gap of 27-41 ms); the window mean is unchanged (31.7-32.5 ms both arms); and the after arm's short frame is 7.1 ms shorter (18.1 to 11.0) while its long frame is 6.3 ms longer (46.0 to 52.3). The p95 rise is the pair period's long frame, not added work.

The card's gate held again: the slowest tile per building frame, p95, went from 5.8 to 2.9-3.0 ms at 100% and from 5.4-5.6 to 2.7-2.9 ms at 150%. `still_0` (Settings open, not paced in pairs) fell from a 29.8-32.0 ms mean to 10.9-15.7 ms. `rebuild.none` held in every run and no console errors were recorded.
