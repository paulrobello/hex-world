#!/bin/bash
# houses-masks-fill checkpoint (card 01a15544869e): house_cottage and house_twostorey had LOD triangles on
# masks texels the bake never wrote (A = 0, R = 0 at their UV centroid: black, no snow). This samples
# the masks at every LOD triangle's UV centroid on the compressed GLBs (masks_centroids.mjs, before from
# a dir holding the old assets/kit/p1/<id>/ files, e.g. `git archive` of the parent commit, after from
# assets/kit/p1), then renders textured headless EEVEE stills
# (still.py: masks.A on UV0, nearest) of each house before and after from the same cameras, and builds
# the review sheet (sheet.py). No in-engine shots: yawed pieces render unlit walls until the renderer
# fix (card 01a155602a2f) lands.
# Usage: bash docs/checkpoints/houses-masks-fill/run.sh <before dir holding assets/kit/p1/<id>/> (repo root).
set -euo pipefail
DOCS=docs/checkpoints/houses-masks-fill
BEFORE=${1:?usage: run.sh <before dir>}
OUT=target/houses-masks-fill
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT"
pairs() {
  for p in house_cottage house_twostorey; do printf '%s %s ' "$1/assets/kit/p1/$p/$p.glb" "$1/assets/kit/p1/$p/$p.masks.png"; done
}
# shellcheck disable=SC2046
node "$DOCS/masks_centroids.mjs" $(pairs "$BEFORE") > "$DOCS/centroids-before.json" || true
# shellcheck disable=SC2046
node "$DOCS/masks_centroids.mjs" $(pairs .) assets/kit/p1/bakery_shop/bakery_shop.glb \
  assets/kit/p1/bakery_shop/bakery_shop.masks.png > "$DOCS/centroids-after.json"
for p in house_cottage house_twostorey; do
  for v in front back; do
    for w in before after; do
      root=$([ "$w" = before ] && echo "$BEFORE" || echo .)
      "$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$OUT/$p-$v-$w.png" "$v" "$p" \
        "$root/assets/kit/p1/$p/$p.glb" "$root/assets/kit/p1/$p/$p.masks.png" assets/kit/manifest.json \
        > "$OUT/$p-$v-$w.log" 2>&1
    done
  done
done
python3 "$DOCS/sheet.py" "$OUT"
# Geometry identity and box-filtered mips 0-3 per slot, before vs after.
for p in house_cottage house_twostorey; do
  node "$DOCS/geometry_and_mips.mjs" "$BEFORE/assets/kit/p1/$p/$p.glb" "$BEFORE/assets/kit/p1/$p/$p.masks.png" \
    "assets/kit/p1/$p/$p.glb" "assets/kit/p1/$p/$p.masks.png" > "$DOCS/mips-$p.json"
done
echo "wrote $DOCS/"
