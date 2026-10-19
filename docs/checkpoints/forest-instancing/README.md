# Forest placement on the instanced kit path (2026-10-19)

Card `01a15485cb01756581d96017fea753d3`. Owner report after deploying `510e115`: placing a forest, the
trees rise, then several vanish, branches draw as flat panels with strange blending, and the LOD level
is wrong at Ultra; the Force LOD and LOD tint debug settings do nothing.

The pre-instancing build `218097d` (chunk merge `a434c5b` too) renders correctly; the regression came
with the GPU-instancing pivot `c92a961`. Five defects on that path, each confirmed by a same-build A/B
against the per-entity path (a temporary, uncommitted switch that keeps settled parts as entities):

1. One record per placement was deduplicated on the parent entity, but scatter spawns every tree of a
   tile under one group entity: each tile kept one tree per variant (100 trees drew as about 20).
2. Records stored translation relative to the chunk origin while the shader read it as world space:
   every tile outside the origin chunk drew displaced, so whole tiles vanished once settled.
3. The slot material key was hardcoded to 0 (no normal map, cull mode or depth bias) and the alpha
   mode's `MAY_DISCARD` was never added: the needles drew as solid cards.
4. `pbr_fragment` reads `mesh[instance_index].flags`; on this path group 2 binding 0 is the kit
   record buffer, so the winding sign and shadow-receiver bits were record bytes.
5. Masked slots were queued into `AlphaMask3dPrepass` with the `Opaque3dPrepass` draw-function id,
   so needles never drew in the prepass: with the normal prepass on (High and Ultra SSAO) the forward
   pass loaded a zero normal and the trees went dark and flat.

The debug Force LOD and LOD tint acted only on per-entity parts; `KitLodDebug` now carries both to
the instanced path.

## Evidence

All native shots: Apple M4 Max, Metal, 1280x800, `--seed 1 --clock fixed --water_clock fixed`.

| File | Shows |
|---|---|
| `01-before-after-baseline.jpg` | Five forests at Ultra, settled and close-up: `510e115` (top), the fix (middle), `218097d` (bottom) |
| `02-multichunk-and-lod-transitions.jpg` | Forests at q=5..6 (outside the origin chunk): rising vs settled stay put; LOD tint pulled back past 150 m and 400 m shows per-chunk LOD 0/1/2 |
| `03-tier-ab-instanced-vs-entity.jpg` | Low, Medium, High, Ultra with full post: instanced (left) vs per-entity (right). RMSE 0.0024, 0.0027, 0.00025, 0.00025 (Ultra was 0.035 before defect 5) |
| `04-lod-debug-tint-and-force.jpg` | LOD tint while rising and settled (LOD0), forced LOD2 with tint, forced LOD1 close-up |
| `05-web-webgpu.jpg` | Served `dist/` on Chromium WebGPU (Metal): settled and close-up, multi-chunk rising vs settled, tint and forced LOD2; no console errors |

`make thumbs` reports zero shader parse errors on the fix.
