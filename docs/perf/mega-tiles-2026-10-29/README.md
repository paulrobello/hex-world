# Mega tiles flat-world perf and parity check (2026-10-29)

Plan Task 10 Step 5 (`docs/superpowers/plans/2026-10-29-mega-tiles-first-round.md`). Host: MiniMac (base M4), GPU Chromium, uncapped, load-guarded (1-minute load ≤ 15 before and after every sample). The orchestrator booked a load-sensitive window and checked in.

## Flat world: main vs mega-tiles

`e2e/perf-close-camera.mjs`, `world2066` (the 2,066-tile save), 1 warm-up and 5 paired rounds per arm, alternating order. main is `7292908` (`index.html` sha1 `20eb9f76…`). The branch arm is `mega-tiles` at `0176c72` (`index.html` sha1 `0e8e1b2b…`). Both dists were verified by `shasum` on MiniMac before timing. The harness is main's `e2e/` for both arms (the branch's `perf-close-camera.mjs` is identical). Load 1.78–2.58, 0 samples discarded, 0 error rows, 0 console errors. Raw: `minimac-close-camera.json`.

| camera | arm | mean ms | p95 ms | cpu mean ms | draws |
|---|---|---|---|---|---|
| close | main | 27.25 | 29.0 | 12.15 | 1472 |
| close | branch | 27.17 | 28.9 | 12.21 | 1472 |
| overview | main | 37.02 | 38.7 | 11.69 | 962 |
| overview | branch | 37.10 | 38.7 | 11.91 | 962 |

Paired per-round deltas (branch minus main) are −0.34 to +0.03 ms at the close camera (mean −0.08) and −0.30 to +0.34 ms at the overview (mean +0.07). The round-to-round spread of either arm is about 1.4 ms (close 26.26–27.68, overview 35.95–38.24). Draw counts (1472 and 962) and kit draws (1337 and 881) are identical in every round. A flat world is not slower: pass.

## Flat-world pixel parity

The existing visual-parity shots (`docs/checkpoints/elevation/visual-parity/run.sh`) are a native `make shots-piece` run, and their world includes hillsides and a cliff plateau. MiniMac has no Rust `target/`, and two native release builds did not fit in the window, so they were not used. Instead `parity.mjs` (a scratch copy of the harness run, not in `e2e/`) captured the web build: `world2066` at 1600x1000, `clock=fixed&water_clock=fixed`, grass at the game default, photo mode with DOF off and agents paused, the perf close and overview cameras, each screenshot queued with a `wait` so it lands on the same absolute frame (4000 and 4700) in every arm. Order main, branch, main, as a determinism control. Load 1.98–3.05, no console errors. Raw: `minimac-parity-run.json`; diffs from `diff.py` over the PNGs: `minimac-parity-diff.json`.

| camera | pair | differing pixels (of 1.6 M) | differing by > 5/255 | max |
|---|---|---|---|---|
| close | main vs main | 1,221,462 | 514,748 | 144 |
| close | main A vs branch | 1,010,505 | 258,125 | 121 |
| close | main B vs branch | 1,197,658 | 471,074 | 134 |
| overview | main vs main | 833,876 | 72,115 | 81 |
| overview | main A vs branch | 555,625 | 16,808 | 88 |
| overview | main B vs branch | 805,272 | 53,632 | 83 |

The web capture is not deterministic: main against itself differs on foliage, grass and water, the wind- and water-animated surfaces, while roads and bare ground match. So pixel-identical parity cannot be shown this way. Branch against either main run differs no more than main differs from itself, and side by side the three frames read the same. Verdict: no flat-world visual change detected, but pixel-identical parity is **unproven**. A pixel-exact check needs the native `--clock fixed` path (deterministic per `docs/perf/sway-cut-2026-10-28/`) or a web key that pins the animation clocks the way `--thumbs` does.

## Verdict

Perf: pass (within 0.3% of main at both cameras, identical draws). Parity: no change beyond the main-vs-main noise floor, pixel-identical not proven.
