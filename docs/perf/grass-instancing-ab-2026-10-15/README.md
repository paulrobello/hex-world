# Grass instancing: before and after (2026-10-15)

e2e/perf-grass.mjs (with the 993c6d6 world-build wait) on the M4 Max, web, 2,066-tile play world, High, 2560x1440, uncapped GPU Chromium. Every run had host load at or below 15 before and after. The two dists were built from e5ef3c9 (CPU-mesh grass, the parent of the instancing commit) and fc1561a (GPU-instanced grass), and run alternately.

| Arm | Before (e5ef3c9) p95 | Instanced (fc1561a) p95, rounds 1 and 2 | Grass mesh bytes before | Grass mesh bytes instanced |
|---|---|---|---|---|
| off | 42.5 ms | 43.3, 45.7 ms | 0 | 0 |
| normal (179,597 blades) | 51.7 ms | 52.1, 50.4 ms | 83,359,052 | 8,614,268 |
| lush (269,191 blades) | 52.9 ms | 50.7, 52.8 ms | 126,135,976 | 8,614,268 |

Instancing leaves frame time where it was and cuts the grass mesh memory 9.7x at normal and 14.6x at lush. A density change only changes the drawn count of the same buffer.

The "before" round 1 was lost: it ran with the old wait and measured a partial world, then aborted when load stayed above 15. Only its round 2 counts.

Main at a7cb76a measured grass off at p95 54.8 to 55.0 ms on the same world (../p2-grass-2026-10-15*.json), about 12 ms slower than fc1561a's 43 to 46 ms. Something merged between fc1561a and a7cb76a costs CPU on this CPU-bound world (card 01a14028862a70ed87a30b165241fcec).
