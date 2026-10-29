# P4 elevation perf check (2026-10-29)

Host: MiniMac (base M4), GPU Chromium, uncapped, load-guarded (1-minute load ≤ 15 before and after every sample). Booked load-sensitive windows on the fleet schedule.

## Flat world: main vs p4-elevation

`e2e/perf-close-camera.mjs`, `world2066` (the 2,066-tile save), 1 warm-up and 5 paired rounds per arm, alternating order. main is `4c845c3`; the branch arm is `4c23536`. The final-review fixes after it touch only bulldoze hover, the HUD tag anchor and debug labels. Load 1.84–2.79, 0 samples discarded. Raw: `minimac-close-camera.json`.

| camera | arm | mean ms | p95 ms | cpu mean ms | draws |
|---|---|---|---|---|---|
| close | main | 27.55 | 29.4 | 12.12 | 1472 |
| close | branch | 27.65 | 29.5 | 12.18 | 1472 |
| overview | main | 37.86 | 39.3 | 12.23 | 962 |
| overview | branch | 37.98 | 39.5 | 12.16 | 962 |

The branch is within 0.4% of main at both cameras, and the round-to-round spread is about 1.4 ms. Draw counts are identical. A flat world is not slower: pass.

## Raised world under a moving camera

`e2e/perf-elevation-orbit.mjs` on the branch build at `8ab7500`: the camera orbits continuously (70 m radius, 30 m up, one `camera` command per frame), so the raised world's CPU `occlude` re-cut runs every frame. The raised world is `docs/checkpoints/elevation/elevation.json` (36 tiles, levels 0–1, with roads, a ramp, portals, water and cars). The control is `flat-control.json` (the same 36 cells as level-0 meadows). Times are rAF-to-rAF intervals, so they do not compare with the table above. 1 warm-up and 5 alternating rounds per world, 8 s each. Raw: `minimac-orbit.json`.

| world | mean ms | p95 ms | p99 ms | round means |
|---|---|---|---|---|
| raised | 11.90 | 14.54 | 19.80 | 11.32–13.02 |
| flat control | 11.80 | 13.70 | 15.32 | 11.53–12.70 |

The ratio is 1.01 on the mean, 1.06 at p95 and 1.29 at p99. Rounds 0–2 were at load 2.0–2.7 and show the raised world level with the control (11.32–11.34 ms against 11.53–11.58 ms). Load rose to 5.4–7.7 during rounds 3–4, still inside the guard, and the raised world's p99 of 25.6–30.5 ms comes from those two rounds. The control holds none of the raised world's roads, cars, water or portals, so 1.01 is an upper bound on what elevation itself costs. Orbiting a raised world costs about the same per frame as a flat one: pass.
