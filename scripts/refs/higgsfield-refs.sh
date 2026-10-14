#!/usr/bin/env bash
# Generates owner-review reference images for one P1 piece with Higgsfield nano_banana_pro (R2).
# Usage: scripts/refs/higgsfield-refs.sh <meadow_base|road_straight|tree_hero_oak>
set -euo pipefail
id="$1"; out="assets-source/refs/$id"; mkdir -p "$out"
style="stylized-real diorama miniature, real PBR materials, slightly exaggerated proportions, soft noon sun, neutral grey studio background, 35 degree three-quarter view"
case "$id" in
  meadow_base) prompts=(
    "A single hexagonal terrain tile, 32 m across, lush meadow grass cap with gentle undulation, sparse wildflowers, visible layered soil strata on the hexagon side walls (topsoil, clay, gravel), $style"
    "Close-up of the meadow grass surface and the soil strata cut face of a hexagonal diorama tile, $style") ;;
  road_straight) prompts=(
    "A single hexagonal diorama tile with a straight two-lane asphalt country road crossing from one flat edge to the opposite flat edge, 6 m wide road, grass shoulders, faded centre line, low curbs, $style"
    "Two identical hexagonal road tiles placed edge to edge with the road continuing seamlessly across the joint, close-up at the seam, $style") ;;
  tree_hero_oak) prompts=(
    "A single hero oak tree for a stylized-real diorama, 12 m tall, broad rounded canopy of leaf clusters, gnarled trunk with root flare, $style"
    "Bark and leaf-cluster material close-up for a stylized-real diorama oak tree, $style") ;;
  *) echo "unknown piece $id" >&2; exit 2 ;;
esac
entries=()
for i in "${!prompts[@]}"; do
  p="${prompts[$i]}"; f="$out/ref-$((i+1)).jpg"; j="$out/job-$((i+1)).json"
  # A kept job file lets a failed download be retried without paying for a new generation,
  # but only a completed job with a result_url is worth keeping.
  if [ -s "$j" ] && ! node -e 'const j=JSON.parse(require("fs").readFileSync(process.argv[1]));const f=Array.isArray(j)?j[0]:j;process.exit(f&&f.status==="completed"&&f.result_url?0:1)' "$j" 2>/dev/null; then
    rm -f "$j"
  fi
  if [ ! -s "$j" ]; then
    higgsfield --json generate create nano_banana_pro --prompt "$p" --aspect_ratio 4:3 --resolution 2k --wait --wait-timeout 10m > "$j"
  fi
  src=$(node scripts/refs/fetch-result.mjs "$j" "$f")
  sha=$(shasum -a 256 "$f" | cut -d' ' -f1)
  job=$(node scripts/refs/fetch-result.mjs --job-id "$j")
  [ -n "$job" ] || { echo "no job id in $j" >&2; exit 1; }
  entries+=("$(node -e 'console.log(JSON.stringify({path:process.argv[1],sha256:process.argv[2],sourceSha256:process.argv[3],provider:"higgsfield",model:"nano_banana_pro",jobId:process.argv[4],prompt:process.argv[5]}))' "$f" "$sha" "$src" "$job" "$p")")
done
(IFS=,; printf '[%s]\n' "${entries[*]}") > "$out/refs.json"
echo "wrote $out"
