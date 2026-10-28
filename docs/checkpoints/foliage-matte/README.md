# Matte oaks, dune grass and ferns (owner A35 O1, 2026-10-27)

Owner A35 O1 ("go with O1 for foliage"): the foliage slots the matte-foliage gate
(`kit_gate::foliage_stays_matte`, median roughness >= 0.7) allow-listed when it landed are matted the way
the verge bushes (e379d55) and the woodland shrubs (2a26c5b) were. The holly (`shrub_forest_b`) keeps its
gloss and stays on `GLOSSY_FOLIAGE_BEFORE_RULE`, now its only entry. The water-lily petal and bud are
flowers, not leaves, and leave the rule through `FOLIAGE_FLOWER_SLOTS`. The oak, forest and beach
thumbnail shifts are accepted.

## Fix

Each family passes its leaf roughness through `_verge_bush.matte` (texels under 0.7 map linearly from
0.50-0.64 onto 0.76-0.86, clipped; texels at or above 0.7 keep their value) where it builds the ORM:

- `_oak_scatter.build` (tree_oak, _b, _c), `_oak_fork.build` (tree_hero_oak_b, and _c through
  `_oak_ancient`) and `tree_hero_oak.py`: the canopy shell's 0.62 fill and the leaf strip's 0.50-0.66,
  before the canopy gaps ramp toward 1.0, so a gap rim never steps down;
- `_dunegrass.build_piece` (beach_dunegrass, _b, _c): the blades' 0.66;
- `fern_clump.py` and `_ferns_var.build` (fern_clump_b, _c): the pinnae's 0.62-0.69; the 0.70-0.72
  pinna rims, stipes and background keep theirs. `_ferns` stays bpy-free.

`_leaves`, `_tree`, `_oak`, `_shrub` and `_verge_bush` are unchanged, so no other piece went stale.

Only the ORM's G changed. Every rebuilt piece's albedo, normal and masks PNGs are pixel-identical to the
approved build, and the ORM's R, B and A are identical (the oak bakes reproduced on the same devices:
CPU, and GPU for tree_hero_oak_c as it was built).

Median ORM G over the opaque texels at LOD0 (`cargo test -p hex-play --test kit_gate
print_foliage_roughness -- --ignored --nocapture`; LOD1 and LOD2 read the same):

| Slot | Pieces | Before (p10 / median / p90) | After (p10 / median / p90) |
| --- | --- | --- | --- |
| leaf cards | tree_hero_oak, _b, _c | 0.518 / 0.580 / 0.643 | 0.773 / 0.816 / 0.859 |
| leaf cards | tree_oak, _b, _c | 0.522 / 0.580 / 0.639 | 0.776 / 0.816 / 0.859 |
| canopy shell | all six oaks | 0.620 / 0.620 / 1.000 | 0.847 / 0.847 / 1.000 |
| foliage | beach_dunegrass, _b, _c | 0.659 / 0.659 / 0.659 | 0.859 / 0.859 / 0.859 |
| foliage | fern_clump, _b, _c | 0.631 / 0.671 / 0.710 | 0.706 / 0.859 / 0.859 |

The holly `shrub_forest_b_leaves` stays at 0.369, the one `GLOSSY_FOLIAGE_BEFORE_RULE` entry. The lily
petal and bud (factor 0.6) are not measured (`FOLIAGE_FLOWER_SLOTS`).

## Shots

- `foliage-matte-kit.json` (kit scenario): the oak grove at (-2, 2) at play distance and close on
  tree_oak's sun side, tree_hero_oak at play distance and close, the forest floor at (-2, 1) at play
  distance and close on a fern_clump, and the beach dressing at (5, -1) at play distance and close on
  beach_dunegrass. Run with
  `make shots-piece SCRIPT=docs/checkpoints/foliage-matte/foliage-matte-kit.json OUT=<dir> ARGS="--clock fixed --water_clock fixed"`.
  The before frames come from the same cameras with the approved assets checked out
  (`git checkout f2ed470 -- assets/kit/p1/<the 12 piece folders>`).
- `sheet.jpg` (built by `sheet.py <before dir> <after dir>`): each subject cropped and enlarged, before |
  after. The `*-crop.jpg` files are its cells. `foliage-matte-kit-before-shots.json` and
  `-after-shots.json` record both runs and their host load (after 98 -> 114, before 121 -> 131, 2026-10-27).

The change reads as less sky sheen on the sun-facing leaf domes and the fern and marram blades; the
albedo, the leaf pattern and the silhouettes are the same in both columns.
