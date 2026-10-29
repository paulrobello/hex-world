# Foliage sway cost cut: 3.9 ms to 1.3 ms at the close camera (MiniMac, 2026-10-28)

Card: cut the kit foliage sway's GPU cost under 2 ms at the close camera on MiniMac web, with no visible change at play cameras and the overview no slower. Baseline: `../grass-cost-2026-10-28/` (sway +3.69 ms at the close camera, +1.29 ms at the overview).

## Summary

- **The sway now costs +1.34 ms mean (p95 +1.0) at the close camera, down from +3.87 ms (p95 +3.9) on main in the same session, and +0.49 ms at the overview, down from +1.72 ms.** The shipped build (`5558c93`, arm `noskip`) is 2.32 ms faster than main at the close camera and 1.23 ms faster at the overview. Every delta is real by the close-camera-draws rule; loads 1.93-3.23, none discarded.
- Two cuts ship, and neither changes what is drawn:
  1. **One wind-field sample per prepass vertex** (`a17e9c7`). The motion-vector prepass placed the previous frame with a second full `wind::sample`. The previous offset now re-evaluates only the sway phases at the previous clock in the current frame's field (`foliage_wind::field` and `offset_at`). Only the motion vectors TAA reads can differ, by the field's own drift over one frame.
  2. **A settled tree's field is computed once per part per frame** (`c8693c1`). A tree samples the wind at its trunk base, so every vertex of it sampled the same field in every pass (forward, prepass, each shadow cascade). A compute pass (`assets/shaders/kit_wind.wesl`, dispatched before the camera passes) writes each kit instance record's packed field (push, gust, phase) into a storage buffer bound at group 2 beside the records (`kit_fields`), and the instanced kit vertex stages read it for tree slots. Plants, and parts still on the per-entity path, keep the per-vertex field.
- A third cut was measured and **not shipped**: directional cascades with texels coarser than 0.25 m (play's cascades 2 and 3, read past about 90 m) drawing foliage at rest (`a17e9c7`, removed again in `5558c93`). It saved another 0.6 ms at the close camera (arm `cand2`: +0.70 ms) but freezes the shadows of far foliage, and the sway was already under 2 ms without it.
- **No visible change in the fixed-clock shots.** Main against itself is pixel-identical at all three cameras, so the shots are deterministic. Main against the cut differs only on pine-needle edges, by sub-pixel TAA resolve differences (0.23% of the close frame's pixels by more than 2%), about a tenth of what the sway itself changes (main against nosway: 2.4%). Side by side the two frames read the same (`shots/close-crop-main-cut-diff.jpg`).

## Setup

- **Host.** MiniMac (Mac16,10, Apple M4, 10-core GPU), booking `01a184a4195c742393f00195222e86af` (23:45Z-01:15Z). All MiniMac work ran 23:45-00:31Z; `~/perf/sw` was deleted at 00:31Z with no browser, `caffeinate` or `hex-world` process left. Builds and gates ran on macdaddy (load-tolerant booking).
- **Arms** (web dists built on macdaddy with `../close-camera-regression-2026-10-28/build-arm.sh` from `git archive` of each commit, every workspace crate recompiled for wasm):

| Arm | Commit | Dist id | What it is |
|---|---|---|---|
| main | `24af2ee` | `3598d98135` | main (the same bytes as the baseline's `a543d47` dist) |
| nosway | `24af2ee` + ablation | `f5b37573a5` | every kit slot built with `wind: None` (the baseline's ablation, measurement only) |
| cand1 | `a17e9c7` | `901e7eb099` | cut 1 plus the coarse-cascade skip |
| cand2 | `c8693c1` | `b633db6382` | cuts 1 and 2 plus the coarse-cascade skip |
| cand2_c1 | `c8693c1` variant | `f57f776c0f` | cand2 with `STILL_CASCADE_TEXEL_M = 1e9` (skip never taken), `make-variant.sh` |
| cand2_ns | `c8693c1` variant | `9c7bcff4d7` | cand2 with `STILL_CASCADE_TEXEL_M = -1` (no cascade sways foliage), diagnostic only |
| noskip | `5558c93` | `ebe6cb0236` | **shipped**: cuts 1 and 2, skip removed |

- **Run.** As the baseline's sway arm: `e2e/perf-close-camera.mjs`, `WORLDS=world2066`, the 2,066-tile save, photo mode with DOF off, grass at Normal, the close camera (10, 12, 40) and the overview (20, 160, 150), 2560x1440, uncapped GPU Chromium (ANGLE Metal, Apple M4), one warm-up discarded, arms rotated by one each round, `e2e/perf-load.mjs` guard (1-minute load at most 15 before and after every run). `minimac-run.sh web <arms>`.
- **Shots.** Native release binaries, `--scenario play --load <save> --clock fixed --offscreen on --size 2560x1440 --seed 1 --tilt off --grass normal`, script `shots.json` (photo mode, DOF off, agents paused, 900 frames to settle, then the close camera, a play camera (30, 60, 90) looking at the origin, and the overview, 240 frames each). `--clock fixed` steps the wind clock 1/60 s per frame, so each screenshot is the same wind pose in every arm. `minimac-run.sh shots <arms>`, `diff.sh` for the numbers.

## Results

### Final run: main, shipped cut, skip variant, nosway (`sway-final.json`, `sway-final.log`, `sway-final-summary.json`; 00:15-00:31Z, loads 1.93-3.23)

Medians over 3 rounds. Deltas are against `nosway` and paired within a round, median [min, max]: the sway's cost in that arm.

| Camera | Arm | Mean ms | p95 ms | Sway cost, mean | Sway cost, p95 | Main-thread `cpu` mean |
|---|---|---|---|---|---|---|
| close | main | 34.28 | 35.8 | **+3.87** [+3.64, +3.98] | +3.9 [+3.3, +4.3] | 12.83 |
| close | noskip (shipped) | 31.96 | 33.1 | **+1.34** [+1.15, +1.62] | +1.0 [+0.8, +1.5] | 12.54 |
| close | cand2 (with skip) | 31.09 | 32.8 | +0.70 [+0.50, +0.75] | +0.9 [+0.3, +1.2] | 12.43 |
| close | nosway | 30.38 | 31.9 | | | 12.36 |
| overview | main | 38.80 | 40.6 | **+1.72** [+1.60, +1.94] | +1.8 [+1.7, +1.9] | 12.16 |
| overview | noskip (shipped) | 37.57 | 39.1 | **+0.49** [+0.35, +0.50] | +0.3 [+0.3, +0.5] | 12.08 |
| overview | cand2 (with skip) | 37.29 | 38.8 | +0.21 [+0.11, +0.28] | +0.0 [+0.0, +0.3] | 12.07 |
| overview | nosway | 37.08 | 38.8 | | | 12.03 |

- 12 of 12 runs valid, no console error, no sample discarded. Draws are equal across arms (close 1,490-1,492, kit 1,337; overview 962, kit 881), and main-thread time moves by at most 0.5 ms, so the cut is GPU vertex work, as the baseline inferred.
- This session's main reads faster than the baseline's (34.3 against 35.6 ms at the close camera), and its sway cost slightly higher (+3.87 against +3.69 ms). The paired deltas are what the card measures.

### Diagnostic run: six arms (`sway-diag.json`, `sway-diag.log`, `sway-diag-summary.json`; 23:47-00:08Z, loads 1.75-3.70)

Same protocol, 3 rounds. With six arms a round takes about 8 minutes, and the host drifted (round 0 read about 1 ms faster than rounds 1 and 2 for every arm), so these deltas are looser than the final run's; they split the cost between the cuts.

| Close camera arm | Mean ms | Sway cost, mean |
|---|---|---|
| main | 34.21 | +3.81 [+3.19, +3.99] |
| cand1 (cut 1 + skip) | 32.40 | +2.02 [+1.95, +3.03] |
| cand2_c1 (cuts 1 and 2, no skip) | 31.68 | +1.41 [+1.22, +2.49] |
| cand2_ns (cuts 1 and 2, no cascade sways) | 31.34 | +0.98 [+0.89, +2.08] |
| cand2 (cuts 1 and 2 + skip) | 31.18 | +0.94 [+0.78, +1.94] |
| nosway | 30.40 | |

At the overview every candidate was faster than main (main +1.62, cand2_c1 +0.68, cand2 +0.73).

### Shots (`shots/`, `shots/diffs.txt`; 00:08-00:15Z, loads 2.10-3.28)

Mean absolute difference (0-1) and pixels differing by more than 2% and 8% of full scale, of 3,686,400. `cut` is the `cand2_c1` binary (`c8693c1` with the skip constant at 1e9), the shipped behaviour; no native binary of `5558c93` itself was built.

| Pair | Close: mean, >2%, >8% | Play camera | Overview |
|---|---|---|---|
| main vs main (second run) | 0.000000, 0, 0 | 0, 0, 0 | 0, 0, 0 |
| main vs cut | 0.0063, 8,657, 1,581 | 0.0020, 916, 57 | 0.0014, 302, 25 |
| main vs cand2 (with skip) | 0.0063, 8,681, 1,580 | 0.0030, 1,580, 63 | 0.0021, 815, 29 |
| main vs nosway (no sway) | 0.0345, 87,224, 48,074 | 0.0090, 19,984, 7,125 | 0.0038, 6,696, 1,445 |

- `main-<camera>.jpg`, `cut-<camera>.jpg`, `nosway-<camera>.jpg`, and levelled difference images `main-vs-cut-<camera>-diff.jpg` and `main-vs-nosway-<camera>-diff.jpg` (bright = changed; each is auto-levelled to its own maximum, so compare the numbers, not the brightness). `close-crop-main-cut-diff.jpg` is a 500x350 crop of the centre pine, main, cut, and their difference levelled at 25%.
- The main-vs-cut difference sits on pine-needle edges only: TAA resolves those sub-pixel edges from motion vectors and history, and both cuts change the motion vectors' last bits (cut 1 by design, cut 2 because the compute pass and the vertex stage round the same field differently). Ground, trunks, shadows and plants do not differ. The skip's own change (cand2 against cut) is near zero at the close camera (24 pixels over 2%) and 440-557 pixels at the play camera and the overview, where cascade 2 shades the view.

## Inferred, not measured

- Where the remaining 1.3 ms goes. `cand2_ns` (no cascade sways foliage) reads the same as `cand2`, so after the cuts the shadow cascades that shade the close camera's view (0 and 1) carry almost no sway cost; the rest is presumably the plants' per-vertex field (reeds, cattails, wildflowers sample the field at each vertex, in every pass) and the trees' per-vertex sway tail in the forward pass and prepass. No GPU trace was taken.
- Cut 1's motion-vector change is not visible in motion: the field moves a fraction of a millimetre of sway in one frame, far below a pixel. Stills cannot show TAA artefacts in motion, so this is reasoned, not observed.
- Native frame cost was not measured (no native ablation), and the M4 Max was not measured.
- Until the compute pipeline has compiled (the first frames after start), the field buffer is zero and trees on the instanced path stand still.

## Reproduce

```bash
# macdaddy
git archive <commit> -o /tmp/sw-arms/<arm>.tar
HEXW_REPO=$PWD bash docs/perf/close-camera-regression-2026-10-28/build-arm.sh <arm> /tmp/sw-arms/<arm>.tar [nosway]
bash docs/perf/sway-cut-2026-10-28/make-variant.sh <dist> <out> <texel>   # cand2_c1 (1e9), cand2_ns (-1); needs a17e9c7..c8693c1
cargo build -p hex-app --release                                          # per arm, for the shots
# MiniMac ~/perf/sw: arms/<arm> (dists, rsync -c), native/<arm>/{hex-world,assets}, e2e/ (with node_modules),
# web/ui/src, docs/perf/post-arc001-2026-10-16/local-world.hexworld, docs/checkpoints/scatter/scatter-mixed.json,
# shots.json and minimac-run.sh
SWAY_ROUNDS=3 OUT=sway-final bash minimac-run.sh web main noskip cand2 nosway
SHOT_REPS="1 2" bash minimac-run.sh shots main; bash minimac-run.sh shots cand2 cand2_c1 nosway
node e2e/perf-close-camera-summary.mjs results/sway-final.json nosway
bash docs/perf/sway-cut-2026-10-28/diff.sh results/shots/main-1 results/shots/cand2_c1-1 main-vs-cut <out>
```
