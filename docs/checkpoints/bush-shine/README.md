# Verge bush shine (owner, 2026-10-26: "these bushes are too shiny")

The deployed build (041ad32) drew the round verge bushes in meadow grass dark with a grey specular
sheen across their tops.

## Cause

The verge bushes' leaf texels had ORM G (roughness) 0.50-0.64 (`_shrub.cluster_tile`, mean 0.57 over
the opaque texels): the lowest of the matte foliage (fern 0.67, wildflower 0.70, tree_forest 0.75; the
grass shader draws at 0.78). The card normals read as one rounded canopy, so at roughness about 0.55 the
sky's specular lay across the dark dome as one broad grey sheen. It is the asset, not a regression:

- Runtime slot coalescing (4b90a09) never merges these pieces. `coalesce::material_key` keeps
  `alpha_mode`, so the masked leaves slot and the opaque stems slot never share a key and draw slot by
  slot as before.
- The leaves slot's roughness factor is 1.0 (`kit_material::build_material`, unique slot with an ORM),
  so the ORM G is the roughness.
- The weather hooks (wetness, night) are 0 in default play (`hooks::surface_hooks(KitHooks::None)`).
- The sheen is already in the landing checkpoints, shot on 2026-10-20 before the coalescing work:
  `docs/checkpoints/bush_verge/verge-sun.jpg` and `verge-kit-eye.jpg` show the same grey leaf highlights.
  The after run's play log reports `slot coalescing: ... left out: {}`, so no bush slot was excluded for
  vertex colours or flags; with no partner key they draw alone.

## Fix

`scripts/blender/kit/_verge_bush.py` wraps the installed cluster tile (`matte_tile`) and maps texels
under 0.7 linearly from 0.50-0.64 onto 0.76-0.86 (mean 0.81). The shade blob (0.85) and bush_verge_c's
florets (0.72) keep theirs. bush_verge, bush_verge_b and bush_verge_c were rebuilt: only the ORM's G
changed (albedo, normal and masks PNGs are byte-identical; the albedo and normal KTX2 re-encode under the
current single-threaded UASTC settings). Each piece has a pending re-approval entry.

## Shots

- `bush-shine.json`: play, `--clock fixed`, the bush_verge checkpoint's road world plus forest, oak and
  meadow tiles; an overview, the old `verge-sun` camera and three 2.2 m cameras on the road beside a
  bush. Run with `make shots-piece SCRIPT=docs/checkpoints/bush-shine/bush-shine.json OUT=<dir>
  ARGS="--scenario play --clock fixed"` (`bush-shine-after-shots.json` records the after run and its
  host load).
- The before frames came from the same cameras on 5d81a68's assets.
- `sheet.jpg` (built by `sheet.py <before dir> <after dir>`): each bush cropped and doubled, before |
  after. The `*-crop.jpg` files are its cells.
- `kit-eye-after-crop.jpg`: `docs/checkpoints/bush_verge/verge-kit.json`'s eye-level frame after the
  fix. bush_verge (left) is matte. shrub_forest (centre, unchanged) still shows the grey sheen on its
  top: its hazel leaves share the 0.50-0.64 range, as do shrub_forest_c's bramble leaves. They were
  left for an owner decision (no rebuilds beyond the reported bush). Extending the fix means wrapping
  their tiles as `_verge_bush.py` does, not changing `_shrub.cluster_tile`: `_shrub_holly.py` finds the
  leaves it glosses by roughness under 0.7. shrub_forest_b (holly) is glossy on purpose.
- Other foliage: `oak-canopy-after-crop.jpg` (tree_hero_oak and tree_oak in `bush-shine-sun`, ORM G
  median 0.62) and `fern-forest-crop.jpg` (fern_clump, 0.67) show no grey sheen: their albedo is lighter,
  so the specular share is small. wildflower_clump (0.70), tree_forest and sapling_spruce (0.75) sit
  above the bush range.
