# MiniMac perf runs

MiniMac is a Mac mini (Mac16,10): Apple M4, 10-core CPU (4 performance, 6 efficiency), 10-core GPU, 32 GB. It is a second, lower-end baseline. The budgets in docs/perf were set on the M4 Max development machine, so compare MiniMac only with itself or as a relative check.

Runs copy `dist/`, `e2e/` and `web/ui/src/` to `~/perf/hex-world` on MiniMac, run `npm ci` in `e2e/`, then run the same scripts under the same host-load guard (MiniMac's own load average).

## 2026-10-15, main a7cb76a

`p2-grass-2026-10-15.json` (e2e/perf-grass.mjs, 2,066-tile play world, High, 2560x1440):
- 2 of 6 runs measured. In the other 4 the world reached only 355 of 2,066 tiles before the measurement window, and the script refuses those.
- grass off: p95 56.6 ms. grass lush: p95 60.4 ms.
- The M4 Max gave off 54.8 to 55.0 and lush 63.3 to 64.4 ms on the same world. The frame time hardly moves with a quarter of the GPU cores, so the large play world is limited by CPU work on the single wasm thread, not the GPU.

`stall-probe-2026-10-15.json` (e2e/stall-probe.mjs, gate2000, uncapped, fl3 against base):
- base: p50 4.2 to 4.3, p95 30.5 to 32.2, max 250 ms, 72 and 106 frames over 25 ms per 600.
- fl3, which is the bare-URL default: p50 9.7 to 11.7, p95 30.1 to 30.6, max 41.5 to 42.6 ms, 277 and 273 frames over 25 ms per 600.
- This GPU is the bottleneck on gate2000 at 2560x1440. The frame-latency limit removes the 250 ms stalls but turns many short frames into 25 to 40 ms ones. On the M4 Max the same comparison gives 1 and 1 frames over 25 ms with fl3 against 5 and 5 with base.

## 2026-10-15, main 993c6d6 (world-build fix)

`p2-grass-normal-3runs-2026-10-15.json`: grass normal, 3 rounds, with the 993c6d6 wait. All 3 runs built the full 2,066-tile world: p95 59.9, 66.7 and 64.0 ms (load 1.7 to 6.5).
