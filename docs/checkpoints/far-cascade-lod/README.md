# Fewer LODs per shadow cascade: measured and rejected (2026-10-26)

Card: "Large worlds: far shadow cascades draw every slot at up to three LODs". On the 2,066-tile save (`docs/perf/post-arc001-2026-10-16/local-world.hexworld`) the close camera draws 1,535 native draws, 717 of them in cascades 2 and 3. A shadow view casts each settled part at its LOD from the main camera, and a far cascade covers the whole world, so every slot casts there at up to three LODs, one draw each.

Two rules cut those draws. Both are reverted, and far cascades cast as on `041ad32`.

| Rule | Commit | 2,066 close draws | 2,066 overview draws | Kit shadow indices (close, overview) | MiniMac against `041ad32` |
|---|---|---|---|---|---|
| main (kept) | `041ad32` | 1,535 | 923 | 36.1 M, 22.0 M | |
| One LOD per cascade: the LOD at its nearest read depth (LOD0 in cascades 0-2, LOD1 in cascade 3) | `e06edcf` | 1,018 | 706 | 65.2 M, 62.0 M | 4 to 7 ms slower than the clamp |
| Clamp per cascade (LOD0 in cascades 0 and 1, LOD0-1 in cascade 2, LOD1-2 in cascade 3) | `d8cd4cd` | 1,235 | 831 | 39.3 M, 27.8 M | web mean +1.03 / +1.43 ms (p95 +3.0 / +3.5), native +0.93 / +1.16 ms |

- The MiniMac run is `docs/perf/close-camera-draws/results-clamp-2026-10-26.json`, on branch perf-batch27 at `386f459`. Its clamp arm was `6c3bd36`, which also holds the bind-group cache. The 20-tile cameras were unchanged.
- On this GPU, the extra shadow triangles cost more than the saved draws.
- A third rule, one LOD at the centre of each cascade's read depths (LOD1 in cascade 2, LOD2 in cascade 3), changed 129 tile thumbnails. The thumbnail camera sits 108 m from its tile, inside cascade 2's read range. That rule was never committed.
- Both committed rules kept `make thumbs` byte-identical. Shots at fixed clocks differed from `041ad32` by at most the baseline's own noise (the clamp) or a mean of 0.14 of 255 (the pinned rule).

## Files

- `census.json`: draws per view for `041ad32` and the clamp, native. Both were captured with `--clock fixed --water_clock fixed`. The 20-tile world is `docs/checkpoints/lod-band-instanced/draws.json` (1600x1000, grass on). The 2,066-tile world is `world2066.json` (2560x1440, High, grass off, `--load` of the save).
- `kit-queue/`: the clamp's 2,066-tile `diag.kit_queue` dumps (`HEXW_KIT_QUEUE=1`). Their `camera_lod_*` fields are what each cascade would have drawn at the main camera's LODs. The census no longer records those fields.

What stays from this work: the kit's group-2 bind-group cache, and the `kit_queue` census (`HEXW_KIT_QUEUE`, native).
