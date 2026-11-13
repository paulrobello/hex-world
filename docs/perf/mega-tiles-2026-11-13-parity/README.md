# Mega tiles flat-world parity, after the held-capture fix (2026-11-13)

The question: with the held-capture fix (`a09e359`, "held captures pause simulated time for whole 64-frame quanta") on both sides, does a flat world (no massif) render pixel-identically on main and on `mega-tiles`? The fix changes harness behaviour shared with main, so the verdict compares MAIN+FIX against BRANCH (which contains the fix), never against unfixed main.

> **Resolved later the same day:** the overview and low-road-cars residuals below came from boids. Commit `6942098`'s peak lift in the boids compute shader moved the fixed-clock flock even in worlds without a massif. Fixed in `40b070d`; parity at every camera is in `docs/perf/mega-tiles-2026-11-13-overview/README.md`.

## Verdict

**Parity is not proven.** The fix removes the play-camera speckle, but two cameras still differ by a small amount, the same way in every run:

- **Play camera: fixed.** MAIN+FIX against BRANCH reads 131-276 differing pixels, 1-9 over 2/255, max 3-9 (36 pairs). The arms' own run-to-run floor is 144-275 / 1-9 / 3-9. Before the fix, main against the branch read 300,377 / 22,815 / 47 at older commits (`../mega-tiles-2026-10-30/`; no unfixed arm of `b06ac09` ran here). The result is consistent with H1 from the 2026-10-30 debug report: the raw hold length varies (the first readback landed 3 or 4 frames after the request, run to run within one binary), and with the hold quantized to 64 frames both arms match. Still open: unfixed main against itself is clean at play in all 15 pairs here, despite that latency jitter, so why the unfixed branch differed from unfixed main in every 10-30 run is not explained.
- **Overview: a small difference that repeats in every run (not fixed, and not caused by the hold).** MAIN+FIX against BRANCH reads 422-519 / 53-60 / 20, against a floor of at most 237 / 8 / 8. The overview is the first capture, and the instrumented runs put its screenshot command at the same frame (1416) and virtual time (23.6000 s) in both arms. Shots of the same overview taken later in the run, after the seven later cameras' holds (`flat-parity-late.json`), are byte-identical within each arm, run to run, and still differ between the arms by 322 / 61 / 18 and 245 / 61 / 17. That makes it a deterministic render difference in the branch, not noise. The pixels are the small grey marks on the beach sand and the small brown marks on the shallow seabed beside the east river mouth (`overview-zoom-fix-branch-diffx12.jpg`). The marks sit in the same places in both arms, but their shading differs. This matches the untraced overview difference of 2026-10-30, which appeared after `151b3a1` (candidates listed in the mega-tiles worktree's `.superpowers/sdd/2026-10-29-mega-tiles-first-round/parity-debug-report.md`). It was not bisected here: no builds were allowed in the capture window.
- **low-road-cars: a residual of 7-8 pixels.** MAIN+FIX against BRANCH reads 328-807 / 7-8 / 3, against a floor of 0-479 / 0-1 / 1-3. The same seven pixels on the near pine branch at the top right (`low-road-cars-zoom-fix-diffx40.jpg`) differ in every pair. That is below visibility, but it repeats every run.
- **The other five cameras are at the floor:** play-road-farm, close, low-level1, low-coast and low-farm.

**A side effect of the fix: grass cameras are now nearly deterministic.** Main against main at the six grass cameras differs on up to 731,754 pixels (max 130), as in 2026-10-30. With the fix, MAIN+FIX against itself differs on at most 479 pixels (low-road-cars), and at most 39 over 2/255 with max 23 (low-farm). The fix removes most of the grass-camera run-to-run spread that the 10-30 README put down to grass build timing. These runs do not separate why: the quantized hold both keeps the frame phase and gives frame-driven grass builds 64 paused frames to finish.

## Arms

| Arm | Tree | Commit | Binary sha1 |
|---|---|---|---|
| main (unfixed, noise floor) | `/tmp/hw-parity/main` | `c9688ea` | `7c16f641…` |
| MAIN+FIX | agent worktree | `c9688ea` + `ba74699` (cherry-pick of `a09e359`) | `be1ce440…` |
| BRANCH | `/tmp/hw-parity/branch` | `mega-tiles` `b06ac09` | `1aef547d…` |
| MAIN+FIX instrumented | agent worktree + `instr.py` | as MAIN+FIX | `4b2915e6…` |
| BRANCH instrumented | branch tree + `instr.py` | as BRANCH | `bf8299b2…` |

`crates/hex-app/src/capture.rs` is byte-identical between MAIN+FIX and BRANCH. Each arm ran `CARGO_BUILD_JOBS=4 cargo build -p hex-app --release` in its own tree, one at a time, in the 06:55-09:55Z load-tolerant window on macdaddy. Each binary was copied out (`binaries.sha1`) and run with its own tree as the working directory, because the data paths are embedded at build time. The instrumented binaries add only `ab:` log lines (`instr.py`): each script command, hold pause, readback and resume, with Bevy's `FrameCount` and the virtual time. The logs are `hold-*.log`.

## Run

`run.sh <tree> <binary> <label> [script]` runs the 2026-10-30 evidence flags and script unchanged (`../mega-tiles-2026-10-30/flat-parity.json`: the 45-tile flat world, 900-frame settle, photo mode, 8 cameras with 240 frames each):

```
caffeinate -d -u <binary> --scenario play --script flat-parity.json --size 1600x1000 --seed 1 --tier high \
  --fps off --tilt off --clock fixed --water_clock fixed --offscreen on --grass normal
```

On macdaddy in the booked load-sensitive window, `capture.sh` ran from 09:55:30 to 10:07:12Z, in this order:

1. Three rounds of fix, branch and main, interleaved.
2. Two runs of each instrumented arm, interleaved.
3. Two runs of each arm with `debug force_lod "0"` (`flat-parity-lod0.json`).
4. Three more rounds of fix, branch and main.

Then, from 10:07:18 to 10:09:24Z, `run.sh` ran by hand two runs of each arm with two extra overview shots after the last camera (`flat-parity-late.json`, 240 and 840 frames later).

All 30 runs exited 0. Host 1-minute load was 13.9-41.0 before and after each run (`runs-load.txt`), mostly 15-20, with one peak at 41 before fix-6. These are fixed-clock pixel captures, not timings, and every arm saw the same load range. `compare.py` and `analyze.py` diff every pair pixel by pixel. All pairs are in `pairs.json`, the full table is in `analyze.out` and the per-arm ranges are in `summary.txt`. The PNGs stay in `/tmp/hw-parity/runs`.

## Results (ranges over all pairs: differing pixels / over 2/255 / largest channel delta, of 1,600,000)

| Camera | main vs main (15) | fix vs fix (15) | branch vs branch (15) | **fix vs branch (36)** | fix vs main (36) |
|---|---|---|---|---|---|
| overview | 158-440 / 2-9 / 3-6 | 141-212 / 0-5 / 2-7 | 167-237 / 2-8 / 4-8 | **422-519 / 53-60 / 20** | 142-432 / 2-8 / 4-7 |
| play | 177-270 / 4-9 / 4-6 | 144-269 / 1-8 / 3-9 | 175-275 / 3-9 / 4-7 | **131-276 / 1-9 / 3-9** | 400,977-401,009 / 46,899-46,911 / 87 |
| play-road-farm | 33-297,327 / 1-26,911 / 3-130 | 5-71 / 0-4 / 1-5 | 24-62 / 0-4 / 1-5 | **5-71 / 0-5 / 1-5** | 117,871-320,466 / 673-27,022 / 19-130 |
| close | 2-731,754 / 0-130,032 / 1-73 | 0-408 / 0-10 / 0-7 | 1-95 / 0-9 / 1-7 | **0-408 / 0-10 / 0-7** | 796,970-810,689 / 159,281-166,367 / 74-86 |
| low-road-cars | 0-535,810 / 0-84,785 / 0-69 | 1-479 / 0-1 / 1-3 | 0-477 / 0-1 / 0-3 | **328-807 / 7-8 / 3** | 592,612-601,953 / 110,468-112,451 / 69-75 |
| low-level1 | 0-300,327 / 0-42,989 / 0-65 | 0-30 / 0 / 0-2 | 0-22 / 0 / 0-2 | **0-30 / 0 / 0-2** | 150,816-326,760 / 286-43,511 / 14-65 |
| low-coast | 0-246,321 / 0-52,457 / 0-91 | 0-121 / 0 / 0-2 | 0-115 / 0 / 0-2 | **0-120 / 0 / 0-2** | 270,239-279,387 / 56,256-64,808 / 84-87 |
| low-farm | 0-429,031 / 0-56,385 / 0-82 | 0-444 / 0-39 / 0-23 | 0-444 / 0-39 / 0-23 | **0-444 / 0-39 / 0-23** | 467,898-483,895 / 54,948-74,379 / 85-92 |

The fix-vs-main column is expected to differ: the fix changes when simulated time resumes after each hold. The overview is captured before any hold, so it matches.

**Localization runs:**

| Pair | overview | overview-late (+240 frames, after 7 more holds) | overview-later (+840) |
|---|---|---|---|
| fix-late vs fix-late | 235 / 7 / 7 | 0 / 0 / 0 (byte-identical) | 0 / 0 / 0 |
| branch-late vs branch-late | 195 / 3 / 4 | 0 / 0 / 0 | 0 / 0 / 0 |
| fix-late vs branch-late (×2) | 493-516 / 58-60 / 20 | 322 / 61 / 18 | 245 / 61 / 17 |

With forced LOD0, fix against branch reads 515-1,481 / 68-74 / 20 at the overview, so the overview difference does not depend on the LOD choice. Forced LOD0 also raises each arm's own run-to-run noise (overview 812 and 1,194 pixels, branch play 604), which is not explained.

**Hold lengths (instrumented).** In all four runs both arms take the overview at frame 1416 and the play shot at frame 1723, at the same virtual times. Each readback lands 3-4 frames after its request, and each hold lasts exactly 64 frames. No run needed a 128-frame hold. The state dumps (`flat-state.json`) are identical apart from the branch's new `play.mega_slots`.

## Next step for the overview residual

Bisect the overview between `151b3a1` and `b06ac09` with MAIN+FIX's `capture.rs` (or none: the overview comes before any hold). The candidates are the scatter and boids commits named in the debug report. The overview-late shot is the better probe: it is byte-identical run to run, so any nonzero diff at a bisect step is signal.
