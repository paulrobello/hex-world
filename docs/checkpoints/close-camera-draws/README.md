# Close-camera draw calls (owner A2 Q3, 2026-10-26)

The owner delegated the structural fix for the close cameras' draw calls ("pick whichever has the most benefit for the entire game"). Kit draws are one draw per (slot, LOD, band) per pass across the whole world (`crates/hex-render/src/kit_instances.rs`), and every (piece, slot) had its own material, so a close camera paid every slot in the main pass, the prepass and each shadow cascade it kept. WebGPU has no bindless or real multi-draw, so texture arrays and per-chunk mesh merges were ruled out. Three changes landed:

1. **O5, screen-aware cascade skip.** The kit skips a shadow cascade that no fragment it can shadow reads (`8dc5129`). That region was the kit bounds swept along the sun, taken whole, so a camera looking down still counted the world under and behind it and kept cascade 0. `shadowed_depth_range` now clips the swept box by the camera frustum (the box faces, their swept copies and the edge parallelograms, clipped by the frustum planes, plus the frustum corners inside it). When nothing the kit can shadow is on screen, every cascade is skipped.
2. **O4, runtime slot coalescing** (`crates/hex-play/src/play/coalesce.rs`). When a pack loads, the slots of one piece whose built materials differ only in base colour and roughness merge, per LOD, into one mesh with one material.
   - Each vertex carries its slot's linear colour in COLOR_0. Bevy's `pbr_input_from_standard_material` multiplies it into the base colour, and the tileable path takes it as the tint (`FLAG_SLOT_COLORS`), without the river mouth fan's detail weight.
   - On opaque slots of differing roughness, COLOR_0 alpha is the slot roughness over the material's 1 (`FLAG_VERTEX_ROUGHNESS`).
   - The merged material is the first member's with a white base colour. Every shaded value is the same product the separate slots computed.
   - The merged mesh holds its primitives in the order the separate draws were binned, and the group draws at its first member's family number.
   - Only the instanced settled draws use the merged meshes. Kit parts, the per-entity rise, and every CPU reader of a slot by name keep the original meshes. Those readers are scatter floors, walk colliders, car ground slots, ghost roles and boats.
3. **O6, per-tile culling cells with sparse slot ranges.** The instanced path culled and picked LODs per 19-hex chunk, each holding a dense range for every slot in the world. Each tile is now its own cell (`KitChunk`), holding `(slot, range)` entries only for the slots it has. `KitInstanceSetData::by_slot` is their transpose, so `kit_view_ranges` walks each slot's cells and never visits a slot a view cannot see. A view now draws a slot only when a tile holding it is in its frustum. The near cascade, which covers a few tiles around a low camera, gains the most.

## Census

`census.mjs` summarises the `docs/checkpoints/lod-band-instanced/draws.json` dumps (`make shots-piece SCRIPT=docs/checkpoints/lod-band-instanced/draws.json OUT=<dir> ARGS='--scenario play'`). Each dump counts draws over every pass and cascade, as `docs/checkpoints/scatter/check.mjs` does, plus the instanced kit's draws by piece (`diag.kit_draws`, new in phase 0).

| 20-tile world camera | Baseline (`census-phase0.json`) | O5 (`census-phase1.json`) | O5 + O4 (`census-phase2.json`) | O5 + O4 + O6 (`census-phase3.json`) |
|---|---|---|---|---|
| Overview (gated at 500) | 419 | 419 | 335 | 335 |
| Forest cluster | 633 | 548 | 436 | 436 |
| Meadow | 645 | 645 | 529 | 497 |
| Centre 152 m away | 756 | 756 | 640 | 640 |
| Kit draw slots in the world | 125 | 125 | 97 | 97 |

- O5 removes the forest-cluster camera's cascade 0 (85 draws): looking down from 58 m, nothing the kit can shadow on screen is nearer than its far bound. The meadow camera sits 13 m over the world and still reads cascade 0, and the overview and band cameras had already skipped it.
- O4 brings the whole kit from 423 instanced draw slots to 289: 59 groups merge 193 slots. 44 of those groups merge only through per-vertex roughness, which saves 85 groups that roughness in the key would split off. Slots with a LOD1 mesh merge whatever LOD0 or LOD2 they lack. A slot without LOD1 (the A2 Q4 LOD0-through-LOD1 fallback) merges only with slots of the same LOD pattern.
- O6 takes the meadow camera's cascade 0 from 81 draws to 49, since that cascade covers a few tiles around the low camera. Every camera's main pass already saw every tile of this small world, so its main-pass counts do not move. In a larger world, a close camera's main pass gains too.
- Every tracked camera except the 152 m band is now at or under 500: meadow 497, forest cluster 436. The band camera (640) draws two LODs for the parts inside the band.

## Left out of coalescing

The kit-load log line `play: slot coalescing: ...` lists every slot left out for a reason other than having no partner:

- Pieces with a rotor (`windmill`, `windmill_b`): each LOD's first body part stays per-entity under its rotor.
- Meshes that already carry COLOR_0: the river mouth fan (`river_mouth_base`, `river_mouth_l_base`), whose COLOR_0 is its detail weight. The cars and trains have it too, but they never draw through the instanced path.
- Slots whose shader reads their own material colour, or which animate per slot:
  - farm animals (`FLAG_ANIMAL`: the vertex stage poses them, and herd animals stay per-entity anyway);
  - beach wet band and seabed slots (`FLAG_WET_BAND`, `FLAG_WET_SEABED`: the seabed reads `material.base_color` directly, and `water::tick` writes the band's parameters every frame);
  - the blossom leaves of `bush_verge_c` (`FLAG_BLOSSOM`).
- Alpha-blended slots (none in the kit today).
- Everything else whose built material differs in more than colour and roughness: a different texture set, alpha mode, wind or foliage flag, masks map or `KitParams`. A tree's bark and leaves stay two draws, for example.

## Evidence

- `make thumbs`, run twice after each phase: every `assets/thumbs/*.webp` and the contact sheet are byte-identical to `main`. As a negative control, the merged material's base colour was set to (0.5, 1, 1) instead of white for one run, and 114 of the 142 thumbnails changed. So the thumbnails do draw through the merged groups. They prove the shading of O4, and of O5 at the thumbnail camera. A thumbnail shows one tile, so it cannot exercise O6's per-tile culling. O6 is a conservative frustum cull on the same part bounds the 19-hex chunks used: no coarser than the per-entity path, which culls each part on its mesh bounds. The census shows it at work.
- `make web-scatter` (GPU Chromium, WebGPU, the device clamped to 16 sampled textures, refracting water in view) passed after O6. Coalesced kit pipelines compiled with no WebGPU validation or console error. Kit forward pipelines bind exactly 16, and the per-tile scatter equals the native dump. The overview counts 343 draws there, against 335 natively, because WebGPU's `PreprocessingOnly` mode turns multi-draw sets into batchable bins. The forest-cluster camera counts 442.
- Unit tests:
  - `kit_instances`: the strategic camera reads only cascade 2; the forest-cluster pose drops cascade 0; the meadow pose and a walking camera keep it; a region behind the camera or off to the side is off screen; frustum clipping; the sweep. Per-tile cells cull their own sparse slot ranges, and each slot draws once across the cells a view sees.
  - `play::coalesce`: conservation of vertices and indices; each slot's colour per vertex; roughness in the alpha; the grouping keys; a missing LOD2; family order; rotor and COLOR_0 pieces left alone.
- Frame-time impact is measured on MiniMac only: `docs/perf/close-camera-draws/README.md`.
