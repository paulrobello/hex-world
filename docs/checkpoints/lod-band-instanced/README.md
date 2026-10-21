# Settled pieces cross-fade over the LOD band (owner, 2026-10-21)

Settled play pieces draw through the instanced chunk path (`crates/hex-render/src/kit_instances.rs`). Before this change the path picked one LOD per chunk from the camera's distance to the chunk centre, and switched every part of the chunk at once with no band. It now cross-fades each part over the same 10 m bands the per-entity path uses (145–155 m and 395–405 m from the camera to the part's origin, `scene::lod_range`), with bevy's dither pattern, so the two LODs split each band pixel for pixel.

## Shots

Shots come from `make shots-piece SCRIPT=docs/checkpoints/lod-band-instanced/band.json OUT=<dir> ARGS='--scenario play'` with `--tilt off`. The world is seven settled pine forest tiles around the origin. The camera dollies from 130 m to 170 m from the origin. "Before" is `6c9aab8` and "after" is this branch.

- `strip-tint.jpg` uses the LOD tint (yellow is LOD0, teal is LOD1). The top row is before and the bottom row is after, at 140, 145, 148, 150, 152, 155 and 160 m.
  - Before, the whole one-chunk world flips from yellow to teal between 150 and 152 m.
  - After, each part changes at its own distance. Parts inside the band show the 4×4 dither checker of both tints.
- `strip.jpg` shows the same frames untinted.
- `before-150*.jpg` and `after-150*.jpg` are the full frames at 150 m.
- `zoom-150-*.png` are 3× nearest-neighbour crops of the band region at 150 m. In `zoom-150-tint-after.png` the checker shows on the band trees and the tile base. In the untinted `zoom-150-after.png` TAA resolves it, with no holes. The dark-pixel count inside the forest box stays within 3% of the before frame at every distance; most of that difference comes from the far parts being LOD1.
- `world20-band-tint-{before,after}.jpg` are the 20-tile scatter world with its centre 152 m away (from `draws.json`).
- `web-webgpu-150-tint.png` is the same seven forests in GPU Chromium on WebGPU (`dist/`, `?scenario=play&tier=high&tilt=off`, camera 150 m away). The band dithers there too, and the page logged no WebGPU validation error.

## Draws

`census.json` counts draws over every pass and cascade, as `docs/checkpoints/scatter/check.mjs` does. Its census is taken after frame 721.

| Camera | Before | After |
|---|---|---|
| 20-tile overview (scatter-10-world camera) | 407 | 407 |
| 20-tile forest cluster (scatter-02 camera) | 633 | 633 |
| 20-tile world, centre 152 m away | 616 | 748 |
| 7 forests at 150 m | 212 | 404 |

- The overview and forest-cluster cameras have no part within 145–155 m, so nothing changes there.
- Inside the band, a (slot, LOD) holding any band part draws all its parts in one dithered draw. So the band adds only the neighbouring LOD's draws.
  - In the 20-tile case that is 66 in the camera views.
  - Another 66 are in the two shadow cascades, because band parts cast with both LODs undithered, as per-entity parts do (bevy's shadow queue adds no dither key).
- The 7-forest world was a single chunk, so before it drew one LOD. After, its near parts are LOD0 and its far parts LOD1, as per-entity parts would be.

## Thumbnails

`make thumbs` (twice) leaves every `assets/thumbs/*.webp` byte-identical to `6c9aab8`. See the report for the hashes.
