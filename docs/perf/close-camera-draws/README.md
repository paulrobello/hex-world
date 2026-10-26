# Close-camera draw calls: MiniMac before/after run

This is the frame-time half of the close-camera draw-call work (owner A2 Q3). The draw counts are in `docs/checkpoints/close-camera-draws/README.md`. No timing has been taken yet: this host's load was 100 to 125 throughout the work, and frame time is measured on MiniMac only.

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

## What to report

- For each world and camera: the median, minimum and maximum over the runs of p50, p95 and mean per arm, and after minus before. Uncapped web frames on MiniMac are bimodal (`docs/perf/lod-band-instanced.json`), so mean and p95 are the stable statistics.
- Each row's draw count and `kit_draws` (slots and total). These must match between the web and native rows of an arm, and must show the drop from the census.
- The renderer string, which must be ANGLE Metal Apple M4 and not SwiftShader, the load before and after each run, and any discarded samples.
- Write the results to `docs/perf/close-camera-draws/minimac-<date>.json` with this README's commands. Then add the summary to PRD §10's draw-call row.
