# Tilt-shift frame-time cost (PRD §9.1)

Measured 2026-10-13 with `node e2e/perf-tilt.mjs web dist 8`: the kit scenario at 2560x1440, uncapped
Chromium (`channel: 'chromium'`, `--use-angle=metal --enable-gpu --ignore-gpu-blocklist`,
vsync and frame-rate limit off) on Apple M4 Max, `tilt=off` vs `tilt=on` interleaved over 8 rounds, one
fresh page per sample, 600-frame diag window after the kit spawned. Every sample ran with host load
at most 15 before and after (observed 12.1 to 14.9; samples over the limit were discarded and re-run).
Raw runs with both loads: `p2-tilt-shift-web.json`.

| Arm | mean ms (min / median / max) | p50 ms (median) | p95 ms (median, max) |
|---|---|---|---|
| off | 2.01 / 2.26 / 2.44 | 2.2 | 2.8, 3.0 |
| on | 2.07 / 2.29 / 3.56 | 2.3 | 2.8, 5.5 |

Cost on minus off, median of samples: 0.03 ms (mean), 0.10 ms (p50), 0 ms (p95). The two slowest `on`
samples (rounds 6 and 7, mean 3.56 and 2.96 ms) did not repeat in rounds 0 to 5 and were not discarded
because host load stayed under the limit. The per-round spread (about 0.4 ms) is larger than the median
cost, so the honest reading is "under about 0.3 ms at 1440p on the kit scene". The frame is about 2 ms,
so it is not necessarily GPU-bound: CPU frame time may not include all of the pass's GPU time.

Native is not isolatable. `--present novsync` and `--present immediate` both pinned the kit scenario at
16.67 ms mean and p50 for both arms (a 60 Hz paced run), the same pacing `p0-gate-matrix.json` records
as `vsync_like_runs` for native. Wgpu timestamp queries are unsupported on Metal, so there is no
per-pass GPU time either. The native p95 near 17.7 ms in both arms is that pacing, not tilt-shift.
`perf-tilt.mjs` reports `vsync_like_runs` so such a pair is not read as a cost.

The pass samples 2 textures in the fragment stage (colour and prepass depth); the kit forward pipeline
stays at 16 sampled textures (14 with `ibl=off`) and its prepass at 7.
