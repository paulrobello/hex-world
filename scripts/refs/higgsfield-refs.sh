#!/usr/bin/env bash
# Generates owner-review reference images for one kit piece or family with Higgsfield nano_banana_pro (R2).
# Usage: scripts/refs/higgsfield-refs.sh <id>
#   P1 pieces: meadow_base road_straight tree_hero_oak
#   P2 MVP families: road_bends river river_road_bridge beach ocean house_cluster forest meadow_props
set -euo pipefail
id="$1"; out="assets-source/refs/$id"; mkdir -p "$out"
style="stylized-real diorama miniature, real PBR materials, slightly exaggerated proportions, soft noon sun, neutral grey studio background, 35 degree three-quarter view"
# Owner-approved looks carried into the P2 families: meadow_base ref-1 strata, road_straight ref-1, docs/checkpoints/water.
strata="layered soil strata on the hexagon side walls (dark topsoil, red clay, gravel)"
road="6 m wide two-lane asphalt country road, faded dashed white centre line, low concrete kerbs, grass verges with tufts and small bushes"
water="clear water tinted pale turquoise in the shallows and deep teal-blue where deep, sun glints, soft caustics on the shallow bed"
cut="the hexagon side wall shows the water column in cross-section like clear tinted resin, turquoise at the top fading to deep blue at the bottom, never an empty gap"
topstyle="stylized-real diorama miniature, real PBR materials, slightly exaggerated proportions, soft noon sun, neutral grey studio background, high camera looking down at about 65 degrees elevation so all six hexagon edges read clearly, soil strata visible on the front side walls"
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
  road_bends) prompts=(
    "Sharp bend road tile: a single regular hexagonal diorama tile with exactly six straight flat edges, centred in frame, 32 m across, lush meadow grass cap and $strata, flat-topped so one flat side faces the camera along the bottom of the frame, carrying a $road. Seen from above, the road starts at the middle of the bottom side and turns sharply right to end at the middle of the lower-right side, the neighbouring side that shares the bottom-right corner. The road stays in the bottom-right part of the tile and the rest of the tile is open grass, $topstyle"
    "Wide bend road tile: a single regular hexagonal diorama tile with exactly six straight flat edges, centred in frame, 32 m across, lush meadow grass cap and $strata, flat-topped so one flat side faces the camera along the bottom of the frame, carrying a $road. Seen from above, the road starts at the middle of the bottom side and curves gently right to end at the middle of the upper-right side. The lower-right side between the two road ends is grass, and the top side opposite the start is grass too, $topstyle"
    "Y junction road tile: a single regular hexagonal diorama tile with exactly six straight flat edges, centred in frame, 32 m across, lush meadow grass cap and $strata, with a three-way junction of $road. Three road arms meet at the tile centre in a symmetric Y, each arm 120 degrees from the next, leaving through three alternating flat edges (every other edge) square-on at the edge midpoints, with grass on the three edges between the arms. The kerbs curve smoothly between neighbouring arms and the dashed centre lines stop short of the junction. Exactly three arms, $topstyle") ;;
  river) prompts=(
    "Two separate flat-sided hexagonal diorama tiles side by side, each 32 m across with a meadow grass cap and $strata. Left tile: a straight river running from one flat edge across to the opposite flat edge. Right tile: a curved river running from one flat edge to the flat edge two positions away, skipping one edge, a gentle bend. The 8 m wide river channel is sunk below the meadow surface between natural earthy banks with pebbles and reeds, clear water tinted pale turquoise at the margins and teal-blue in the middle, sun glints, soft caustics on the pebbly channel floor. Where the river reaches the tile side wall, $cut, $style"
    "Close-up of a natural river bank on a hexagonal diorama tile: the meadow grass edge, an earthy bank sloping down to the water, pebbles and reeds, clear water tinted pale turquoise at the margins and teal-blue in the middle, sun glints, soft caustics on the pebbly channel floor, and where the channel reaches the tile side wall $cut, $style") ;;
  river_road_bridge) prompts=(
    "A single flat-sided hexagonal diorama tile, 32 m across, lush meadow grass cap and $strata. A straight 8 m wide river runs between one pair of opposite flat edges in a channel cut below the meadow surface with natural earthy banks, $water. A $road runs straight between another pair of opposite flat edges and crosses the river at 60 degrees on a small single-span stone-faced road bridge with low parapet walls and abutments set into the banks. Where the river meets the tile side wall, $cut, $style"
    "Close-up of a small single-span stone-faced country road bridge on a diorama tile: the asphalt deck with low kerbs and a faded dashed centre line, low stone parapet walls, abutments set into earthy river banks, the river flowing underneath, $water, $style") ;;
  beach) prompts=(
    "A single flat-sided hexagonal coast tile for a diorama, 32 m across. Ocean fills the side with two adjacent flat edges, lush meadow grass fills the two flat edges opposite them, and a curving sand shoreline runs across the tile from one flat edge to the opposite flat edge. Bands from land to sea: meadow grass, dry pale sand, a darker wet-sand band, a lacy white surf foam line, $water. Under the land the side walls show $strata; on the ocean side $cut, with the sand floor sloping down, $style"
    "Close-up of the waterline on a diorama beach tile: dry pale sand, a darker damp-sand band, a thin lacy white surf foam line along the shore, clear turquoise shallows over a rippled sandy floor with caustics, $style") ;;
  ocean) prompts=(
    "A single regular hexagonal deep ocean tile for a diorama with exactly six straight flat edges, 32 m across, on a plain neutral grey studio backdrop. Open temperate sea with layered gentle waves, a few white foam flecks on crests, $water. On every side $cut. Through the clear side walls a pale sandy shelf rises toward one edge and slopes down to a darker sandy seabed with scattered pebbles. Photoreal PBR rendering with plain sand and pebbles only, $style"
    "Close-up at one corner of a hexagonal diorama ocean tile on a plain neutral grey studio backdrop, looking at the 120 degree corner where two side faces of the hexagon meet. The tile is one cast block of clear tinted resin with no container, no glass walls and no rim: the top of each side face follows the wavy water surface exactly, the water column turquoise near the top fading to deep blue lower down, a pale sandy shelf stepping down to a darker sandy seabed with a few pebbles, soft caustic light on the sand, photoreal PBR rendering, $style") ;;
  house_cluster) prompts=(
    "A single flat-sided hexagonal diorama tile, 32 m across, lush meadow grass cap and $strata, with a cluster of 3 small stylized-real rural houses (pitched clay-tile and slate roofs, rendered plaster and timber walls, chimneys, timber window frames) on a level gravel-and-paving pad, small gardens, low hedges and a picket fence, and a short asphalt driveway that leaves the pad and meets one flat edge of the tile square-on, where it joins a short stub of 6 m wide two-lane asphalt road with low kerbs and a faded dashed centre line, $style"
    "Close-up of one small stylized-real rural house on a diorama tile: weathered clay roof tiles, rendered plaster walls, timber window frames and front door, a chimney, a gravel path, a low hedge and a small garden, $style") ;;
  forest) prompts=(
    "A single flat-sided hexagonal diorama tile, 32 m across, $strata, covered by a dense mixed forest: a few broad rounded oaks among many tall spruce conifers with dark green needle tiers, undergrowth of ferns, shrubs and saplings, a fallen mossy log and leaf litter on the forest floor, $style"
    "Close-up of a stylized-real spruce conifer and the forest undergrowth on a diorama tile: rough flaky bark, layered tiers of needle clusters, ferns, low shrubs, moss and leaf litter at the trunk base, $style") ;;
  meadow_props) prompts=(
    "One corner fragment of a hexagonal meadow diorama tile with $strata, on a plain neutral grey studio backdrop: lush meadow grass with scattered wildflower clumps (white daisies, yellow buttercups, purple clover, blue cornflowers), each clump about 1 m across with small flower heads in natural proportion to the grass, and weathered grey granite rocks from 0.5 m to 2 m with lichen patches, partly sunk into the turf with grass growing around their bases, photoreal PBR materials, $style"
    "Close-up of meadow scatter on a diorama tile on a plain neutral grey studio backdrop: small realistic wildflower heads of white daisies, yellow buttercups, purple clover and blue cornflowers among dense meadow grass, each flower head about the width of a few grass blades, and a weathered lichen-spotted granite rock half sunk into the turf with grass around its base, photoreal PBR materials, $style") ;;
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
