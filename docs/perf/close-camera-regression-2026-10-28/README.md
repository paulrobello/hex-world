# Close camera: is there a frame regression since `c032d97`? (MiniMac, 2026-10-28)

Card: "Possible close-camera frame regression since c032d97". The card's numbers are unpaired: `c032d97` read a main-thread `cpu` of 16.1 ms and a 30.4 ms mean at the close camera of the 2,066-tile save (`docs/perf/large-world-cpu/`, 2026-10-27 14:00Z session), and `4897f2e` read 23.6 ms and 37.2 ms in a later session (`docs/perf/large-world-bounds/`, 23:00Z). Since then main landed the `calculate_bounds` fix (`d3f4673`, about -6 ms of main thread).

This run pairs `c032d97`, `4897f2e` (the card's "after" build) and main `a543d47` in the same session, rounds interleaved.

## Summary

- **No regression remains, and none shows between `c032d97` and `4897f2e` either.** Paired in one session, the three builds read the same frame: close mean 31.56, 31.52 and 31.54 ms, overview 40.11, 40.17 and 40.10 ms. Every mean and p95 delta is under 0.25 ms and none is real.
- **Main thread.** `c032d97` and `4897f2e` read the same `cpu` (21.97 and 22.17 ms close, +0.21 ms, not real). Main reads 16.88 ms close and 13.94 ms overview: -5.08 and -4.83 ms against `c032d97`, both real. That is the `calculate_bounds` fix (`d3f4673`), the size the bounds run measured (-6.00 and -5.59 ms).
- **Draws.** Close 1,422 to 1,472 (+50) and overview 930 to 962 (+32), all in the kit, between `c032d97` and `4897f2e`, with 5 more distinct kit slots drawn (135 to 140 at both cameras). That matches the five B1 granite pieces `0ac9693` added to the scatter rules (`boulder_granite`, `_b`, `_c`, `_d`, `outcrop_granite`); the JSON records only the slot count, not their names. Main draws the same as `4897f2e`. The extra draws cost no measurable frame time.
- **The card's gap was between sessions, not builds.** The card compared `c032d97` at 16.1 ms `cpu` and a 30.4 ms mean (2026-10-27 14:00Z session) with `4897f2e` at 23.6 ms and 37.2 ms (23:00Z session). In this session the same `c032d97` reads 22.0 ms and 31.6 ms, and `4897f2e` reads 22.2 ms and 31.5 ms. Neither earlier session's numbers reproduce, in either direction, with the same browser (HeadlessChrome 153, ANGLE Metal on the Apple M4) and loads of 2-5 in all three. A likely contributor to the 14:00Z-to-23:00Z jump in `cpu`: a renderer of the owner's desktop Google Chrome on MiniMac (pid 16157) has run at about 100% of one CPU core since 2026-10-27 20:59Z, between those two sessions, and was still running through this one. With none of this run's processes alive it held MiniMac's GPU at 5-6% utilization (`ioreg` `Device Utilization %`, 12 samples at 07:07Z). That is an inference from timing, not a measured cause, and it does not explain the earlier session's 30.4 ms mean against today's 31.6 ms for the same build. Only paired runs in one session compare builds.
- **Both cameras are GPU-bound.** Nearly every window frame is over 25 ms (598-600 of 600), and the main thread sits 15 ms (close) and 26 ms (overview) under the frame on main. The frame mean does not follow main-thread time.
- **No bisect was needed.** The `4897f2e` arm splits the range at the bounds fix, and neither half moves the frame.

## Results (`close-camera-web.json`, `close-camera-web.log`; summaries `summary-vs-c032d97.json`, `summary-vs-4897f2e.json`)

Medians over 4 rounds; deltas against `c032d97` paired within a round, median [min, max].

| Camera | Build | Mean ms | p95 ms | Main-thread `cpu` mean ms | `cpu` p95 ms | Draws (kit) |
|---|---|---|---|---|---|---|
| close | `c032d97` | 31.56 | 33.1 | 21.97 | 22.8 | 1,422 (1,287) |
| close | `4897f2e` | 31.52 (-0.12 [-0.20, +0.06]) | 33.05 (-0.1 [-0.3, 0]) | 22.17 (+0.21 [-0.21, +0.44]) | 23.0 | 1,472 (1,337) |
| close | main `a543d47` | 31.54 (-0.03 [-0.25, +0.06]) | 32.95 (-0.15 [-0.8, 0]) | **16.88 (-5.08 [-5.28, -5.02], real)** | 17.7 (-5.1, real) | 1,472 (1,337) |
| overview | `c032d97` | 40.11 | 41.7 | 18.77 | 19.6 | 930 (849) |
| overview | `4897f2e` | 40.17 (+0.05 [+0.01, +0.24]) | 41.8 (+0.1 [0, +0.2]) | 18.93 (+0.18 [-0.25, +0.34]) | 19.65 | 962 (881) |
| overview | main `a543d47` | 40.10 (-0.02 [-0.23, +0.09]) | 41.6 (-0.1 [-0.5, +0.1]) | **13.94 (-4.83 [-5.08, -4.73], real)** | 14.65 (-4.95, real) | 962 (881) |

Against `4897f2e` (`summary-vs-4897f2e.json`), main reads close mean +0.04 [-0.05, +0.10] and `cpu` -5.29 [-5.46, -5.07] (real), overview mean -0.11 [-0.29, -0.03] and `cpu` -5.01 [-5.07, -4.83] (real). The overview's -0.11 ms passes the same-sign test but not the spread test, so it is not real.

- Every run's 1-minute load was between 3.43 and 4.61, before and after. No sample was discarded or failed, and no console error was logged.
- Draw counts were identical in every round of each build.
- The overview's main-thread time fell with the bounds fix as much as the close camera's, so the fix is the whole main-thread difference between `4897f2e` and main.
- MiniMac's desktop Google Chrome (the owner's session, not automated) had one renderer process at about 100% CPU for the whole window. It was present for every arm and round alike, so it does not bias the paired deltas, but it is one reason absolute numbers can differ from other sessions.

## Answer to the card

The suspected regression is not there. Paired, `c032d97`, `4897f2e` and main read the same close-camera frame (31.5-31.6 ms mean, 33 ms p95), and main's main thread is 5.1 ms lighter than `c032d97`'s thanks to `d3f4673`. The card's 16.1 to 23.6 ms and 30.4 to 37.2 ms came from two sessions whose absolute levels differ, and this session reproduces neither.

## Setup

- **Host.** MiniMac (Mac16,10, Apple M4 with a 10-core GPU), booking `01a180eb98f374759ee80384d84184e8`, 06:15-07:45Z.
- **Harness.** `e2e/perf-close-camera.mjs` (main's copy for every arm) with `WORLDS=world2066 ROUNDS=4 WARMUP=1`: the exact save `docs/perf/post-arc001-2026-10-16/local-world.hexworld`, headless Chromium, uncapped, 2560x1440, DPR 1, play, High, tilt off, grass off, seed 1, photo mode with DOF off, close camera (10, 12, 40) looking at the origin and the overview (20, 160, 150) looking at (20, 0, -5). Each camera window is read 700 frames after the move. Arms rotate by one each round, and the first run (a `c032d97` warm-up) is discarded.
- **Load guard.** `e2e/perf-load.mjs`: MiniMac's 1-minute load at most 15 immediately before and after every run, else the run is discarded and rerun.
- **Arms.** `c032d97` (dist id `263484e4d9`), `4897f2e` (`847c9e5349`, named `b4897f2e` in the JSON) and main `a543d47` (`3598d98135`). One warm-up run of `c032d97` was discarded. All runs took place 06:16-06:31Z.
- **Builds.** Each arm was built on macdaddy with `build-arm.sh` here: `git archive <sha>` extracted to a fresh directory, `target/` and `web/ui/node_modules` cloned in, `crates/`, `assets/shaders` and `data/` touched, the seven workspace crates `cargo clean`ed for the wasm target, then `make build-web`. The dists were copied to MiniMac.
- **Tier warm-up.** `c032d97` predates the tier shader warm-up, and `4897f2e` and main have it off by default (`946cf86`), so no arm runs it.

Deltas are paired within a round and given as the median [min, max]. "Real" follows the close-camera-draws rule: every paired delta has the same sign and the median is larger than their spread.

## Reproduce

```bash
# macdaddy (load-tolerant)
for sha in c032d97 4897f2e a543d47; do git archive $sha -o /tmp/cc-arms/$sha.tar; done
bash docs/perf/close-camera-regression-2026-10-28/build-arm.sh c032d97 /tmp/cc-arms/c032d97.tar   # likewise 4897f2e, main
# MiniMac ~/perf/gc, inside a booked window: arms/<name> (each dist, copied with rsync -c), e2e/ (npm ci),
# web/ui/src, the save and docs/checkpoints/scatter/scatter-mixed.json, then
bash docs/perf/grass-cost-2026-10-28/minimac-run.sh m2
node e2e/perf-close-camera-summary.mjs results/close-camera-web.json c032d97
node e2e/perf-close-camera-summary.mjs results/close-camera-web.json b4897f2e
```

The MiniMac runs and cleanup are recorded in `../grass-cost-2026-10-28/README.md` (Setup): all runs 06:16-07:04Z, `~/perf/gc` deleted at 07:04Z.
