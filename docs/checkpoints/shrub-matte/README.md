# Woodland shrubs' matte leaves (owner A23 Q2, 2026-10-26)

`shrub_forest` (hazel) and `shrub_forest_c` (bramble) showed the grey specular sheen the verge bushes had
before e379d55 (`docs/checkpoints/bush-shine/kit-eye-after-crop.jpg`): their leaf texels drew at the
family's roughness 0.50-0.64 (`_shrub.cluster_tile`).

## Fix

`scripts/blender/kit/_shrub_matte.py` runs each piece's family build (`_shrub.build`,
`_shrub_bramble.build`) with `_shrub.cluster_tile` wrapped through `_verge_bush.matte`, the verge bushes'
mapping: texels under 0.7 go linearly from 0.50-0.64 onto 0.76-0.86. The shade blob (0.85) keeps its value.
`_shrub.py` and `_verge_bush.py` are unchanged, so the holly (`shrub_forest_b`, glossy on purpose, found by
roughness under 0.7 in `_shrub_holly.py`) and the verge bushes are untouched.

ORM G over the opaque texels (albedo alpha > 0.5):

| Piece | Before | After |
| --- | --- | --- |
| shrub_forest | 0.502-0.851, mean 0.575 | 0.761-0.859, mean 0.812 |
| shrub_forest_c | 0.502-0.851, mean 0.573 | 0.761-0.859, mean 0.810 |

Only the ORM's G changed. The albedo, normal and masks PNGs, the masks KTX2 and the compressed GLB (mesh,
LODs, collider, sockets) are byte-identical to the approved build. The ORM's R, B and A are identical. The
albedo and normal KTX2 re-encode under the current UASTC settings, as in e379d55.

No thumbnail changed: `make thumbs` ran twice with identical output, and the raw 384 px capture of
`forest-r0` (the only tile that scatters these shrubs) is pixel-identical on b81f4f2's assets and on the
matte ones, so the thumbnail camera never sees a shrub leaf's specular.

## Shots

- `shrub-matte-kit.json` (kit scenario: the undergrowth forest and the verge showcase's shrub_forest) and
  `shrub-matte-play.json` (play, five forest tiles), both at `--clock fixed`. Run with
  `make shots-piece SCRIPT=docs/checkpoints/shrub-matte/<script>.json OUT=<dir> ARGS="--clock fixed"`
  (the play script adds `--scenario play`). The before frames came from the same cameras on b81f4f2's
  assets. `*-after-shots.json` record the after runs and their host load.
- `sheet.jpg` (built by `sheet.py <before dir> <after dir>`): each shrub cropped and enlarged, before |
  after. The `*-crop.jpg` files are its cells.
