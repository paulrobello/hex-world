# Owner checkpoint: hero oak (`tree_hero_oak`), P2 polish

Prepared for the owner's re-approval after the P2 polish of the approved solid-dome oak (5bc77f1). The rebuild changed every hash, so the earlier piece approval was not carried over: `approvals.json` is empty and provenance holds only the reference approval. Nothing here records an approval. The P1 checkpoint follows below for history.

## Screenshots (before left, after right; native, 1600x1000)

| What | Committed |
|---|---|
| About 8 m from the crown edge | `renders/p2_dome8m-none.jpg` |
| Same, after only, sharp (`--tilt off`: the default tilt-shift blurs every close-up) | `renders/p2_dome8m-sharp.jpg` |
| `kit-tree` camera (8.8 m from the trunk, inside the crown) | `renders/p2_close8m-none.jpg` |
| Low angle, limbs under the crown | `renders/p2_under-none.jpg` |
| Mid (about 27 m) and strategic (about 70 m) | `renders/p2_mid-none.jpg`, `renders/p2_strategic-none.jpg` |
| Snow hook (`--kit_hooks snow`) | `renders/p2_dome8m-snow.jpg`, `renders/p2_under-snow.jpg` |
| Night hook (`--kit_hooks night`) | `renders/p2_dome8m-night.jpg` |
| LOD1 at 159 m and LOD2 at 411 m (just past the 400 m switch), 4x crops | `renders/p2_lod1-crop.jpg`, `renders/p2_lod2-crop.jpg` |
| Blender six-view (untextured evidence) | `renders/{front,back,left,right,top,three_quarter}.jpg` |

Shot script: `assets-source/kit/tree_hero_oak/shots/oak-p2.json`.

## Against the five P2 criteria

1. **Canopy masks.** The canopy slot now samples masks (`"masks": true` in the manifest; no Rust change, binding 101 already exists in every kit material). Its islands are packed around the locked bark islands with a 16-texel gutter, and the masks carry A = 1 on every canopy texel (minimum 1.0; the canopy AO stays in ORM.R) and R snow capacity from the canopy AO (mean 0.65). Snow now settles on the dome tops (before: only the fringe cards took snow). Night: trees bake B = 0, so the hook reaches the domes but shows no glow.
2. **Smooth domes, no visible 3-fold repeat (improved; a few rim corners remain).** The shell went from 15 to 21 columns around with denser upper rings, and each LOD's shell normals are the analytic normals of its clump's own surface. The baked master is now the smooth ellipsoid (no 3-fold lumps), and each clump's shell gets its own seeded lumps (theta frequencies 1, 2, 4, 5) in geometry, so no low-frequency feature repeats per 120-degree sector. Shell faces hidden inside a neighbouring clump are culled (417 of 2,310) to pay for it. In the sharp shot the outlines read round and no leaf pattern repeats around a dome, but the rims of the upper-right and lower-right domes still show slight corners at 21 columns; more columns would not fit the 3,000-triangle budget.
3. **Limbs and roots.** Limbs leave the trunk at 2.6-3.0 m (was 3.3-3.7), run out nearly level before rising, and are thicker (0.5 to 0.17 m); they show between the trunk and the dome undersides. Surface roots are heavier: radius 0.7 m at the flare (was 0.52), 0.13 m at the tip (was 0.08), 15% longer reach, 5-sided in LOD0.
4. **Bark AO.** Not met, and lower than P1 on the same metric: mean 0.645 over all bark texels (P1 0.714). Over the visible texels only (a new measure with no P1 counterpart) it is 0.721, p05 0.25. A quarter of the bark texels sit inside another branch's low tube and never show. The rest is the twisted-strand grooves and root junctions inside the 0.35 m crevice-scale AO distance, kept rather than shortened to reach the number (`friction.md` entry 28).
5. **LOD2.** LOD1 and LOD2 shells decimate each closed clump shell (no hidden-face cull at these LODs, which would open gaps at the dome junctions) and carry the smooth analytic normals, so LOD2 reads as one rounded crown on a trunk instead of faceted lumps (crop above).

## Budgets

| Item | Budget | Actual |
|---|---|
| LOD0 / LOD1 / LOD2 triangles | ≤ 3,000 / ≤ 50% / ≤ 15% | 2,988 / 1,476 (49.4%) / 440 (14.7%) |
| LOD0 split (bark / canopy shells / fringe) | | 941 / 1,893 / 154 |
| Texel density, per material | ≥ 256 px/m median each | bark 531 (tile, K = 18), canopy 267, fringe 392 |
| Runtime GLB | ≤ 2,097,152 B | 247,524 B |
| Piece KTX2 (hero class) | ≤ 11,500,000 B per family | BC7 4,951,142; ASTC 4,968,779 |

`.blend`: `<main checkout>/assets-source/blend/tree_hero_oak/tree_hero_oak_high.blend`, SHA-256 `adfb21a59601ae707b971193a1f9815647a61529dd91690acd15b3173dba384a`, 16,983,400 B, written headless by `live/06_p2_highpoly.py`. The approved P1 revision is kept beside it as `tree_hero_oak_high.5709011d4754.blend`.

---

# Owner checkpoint: hero oak (`tree_hero_oak`), solid-dome canopy pass

Prepared for the owner's review after the solid-dome canopy pass. Nothing here records an approval; the controller records it once the owner has looked at the screenshots. Judge against the approved `assets-source/refs/tree_hero_oak/ref-1.jpg`.

## Screenshots

| What | Path |
|---|---|
| Side by side with ref-1: EEVEE high-poly showcase | `renders/vs_ref1_showcase.jpg` |
| Side by side with ref-1: in-engine overview crop | `renders/vs_ref1_engine.jpg` |
| Side by side with ref-1: in-engine low-angle underside | `renders/vs_ref1_under.jpg` |
| Side by side with ref-1: in-engine close-up at gameplay zoom (new `kit-tree-game`, about 27 m) | `renders/vs_ref1_game.jpg` |
| In-engine native shots | not committed: `make shots-kit` |
| Web (WebGPU) versions | not committed: `make web-shots-kit` |
| LOD crops (zoomed) | not committed (crops of the `kit-lod1` and `kit-lod2` shots) |
| Blender six-view (untextured evidence) | `assets-source/kit/tree_hero_oak/renders/{front,back,left,right,top,three_quarter}.jpg` |
| EEVEE showcase of the high poly (live session, key/fill/rim + noon HDRI) | `renders/highpoly_showcase.jpg` |
| Wind U / V debug | `renders/wind_{u,v}.jpg` |

In-engine shots use the kit defaults (ACES fitted, `KIT_EV100 = 12.5`, `IBL_INTENSITY = 1500`); kit LODs switch at 150 m and 400 m. The black background in the low-angle shots is the kit scene's clear colour.

## Against the five targets

1. **Sculpted dome clumps.** The crown is eleven closed dome shells (puffy top 0.9 R, flattened underside 0.4 R, 1.15 R wide, lumpy outline) in three staggered tiers. Each shell carries a bake of one high-poly master dome covered in about 1,440 overlapping, cupped, lobed oak leaves over a dark-green under-dome (albedo through EMIT, normal, AO into ORM.R; canopy AO mean 0.72, p01 0.19, no ray misses). Leaf relief comes from the bake, not from cards.
2. **Fringe only.** 77 lobed alpha cards (7 per clump) sit half-sunk in the shell rims and undersides, each facing straight out from its clump centre with a small tilt, to break the dome outlines.
3. **Crown proportions.** Measured on ref-1's outline: crown width / tree height 1.15, crown height / tree height 0.65, clear trunk 0.35. The oak: height 11.4 m, crown about 12.5-13.4 m wide (width / height about 1.14), crown from about 4-4.8 m (clump undersides) to the top (about 0.6 of the height). The trunk sits 3.6 m from its cell centre and the crown radius is about 6.7 m, inside the 10.2 m to the nearest cell edge, so the canopy stays within the tree's own hex and clear of the road verge. It exceeds the brief's 9-12 m diameter (flagged).
4. **Bark tone.** The bark set is lighter and warmer (plates #9c7149 to #c19569, fissures #3a2717); see the close-ups.
5. **Kept.**
   - Underside lit: the shell normals are bent toward up and out of the canopy after the bake (the bake uses the smooth dome normals, so the map holds leaf relief, not facets), and the fringe keeps the foliage back-face rule.
   - Wind on fringe and shells: shells U 0.22-0.31 (0.35 x their clump's sway, one phase per clump), fringe 0.71-1.0, bark 0-0.88.
   - Tileable bark on UV0, the per-material texel gate (bark 533, canopy 340, fringe 392 px/m median), 16/7 sampled textures, a byte-identical GLB across two runs, and every gate.

## Budgets

| Item | Budget | Actual |
|---|---|
| LOD0 / LOD1 / LOD2 triangles | ≤ 3,000 / ≤ 50% / ≤ 15% | 2,981 / 1,472 (49.4%) / 436 (14.6%) |
| LOD0 split (bark / canopy shells / fringe) | | 847 / 1,980 (11 x 180) / 154 |
| LOD2 | | shells decimated to about 22 tris each (a faceted lump per clump), every 4th fringe card at 1.5x, bark decimated |
| Texel density, per material | ≥ 256 px/m median each | bark 533 (tile), canopy 340 (atlas), fringe 392 (atlas) |
| Runtime GLB | ≤ 2,097,152 B | 261,212 B |
| Piece KTX2 (hero class) | ≤ 11,500,000 B per family | BC7 5,444,564; ASTC 5,500,554 |
| Bark set KTX2 (tileable) | ≤ 3,600,000 B per family | BC7 2,040,344; ASTC 2,043,045 |
| Sampled textures, kit forward / prepass | 16 / 7 | 16 / 7 |

## What I saw

- At gameplay zoom the crown reads as solid, sculpted, leaf-covered domes with dark gaps between clumps, close to ref-1's figurine clumps. Edge-on card streaks are gone; the few fringe cards read as leaf rosettes on the dome rims.
- Up close (the 8 m `kit-tree` camera and the low-angle shot) the low-poly shell silhouettes are visibly polygonal, and the 3-fold bake repeat can be spotted on a single dome.
- The underside is green and lit; the crevices between domes are dark, as in ref-1.
- LOD1 keeps the dome cluster; LOD2 becomes a compact lumpy mass with no holes (crops above).
- Against ref-1: proportions and dome clumps now match in kind. The reference's domes are rounder and more numerous, its trunk splits into visible limbs under the crown, and its roots are more pronounced.

## Sources

- `.blend`: `<main checkout>/assets-source/blend/tree_hero_oak/tree_hero_oak_high.blend`, SHA-256 `5709011d47546ee830787c4cd5122deadbc2b8dd899ad8f96dfa591761fb293a`, 16,063,141 B (contains the leaf-covered master dome `tree_hero_oak_canopy_high`).
- Provenance: `assets-source/kit/tree_hero_oak/provenance.json` (`route: "interactive"`, `validation.pass: true`), `assets-source/kit/materials/bark/provenance.json`.
- Contracts: `.claude/skills/blender-asset/references/project.md` (solid-dome canopy, bark on UV0, foliage normals, tree wind). Friction: `assets-source/kit/tree_hero_oak/friction.md`.
