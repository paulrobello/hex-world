# log_fallen friction

- **An odd ring count pushed a half-length UV island past V = 1** (found by a new check, 5 min). The
  body is two half-length islands split at `rings // 2`; LOD2 had 3 rings, so its second island held
  two thirds of the log and its UVs ran to V 1.14, sampling outside the atlas. The 6-view render
  shows only LOD0, so nothing showed it. Fix: assert an even ring count, and assert after the build
  that every face of every LOD has its UVs inside one island rectangle. Skill text (pitfalls, "Leaves,
  foliage and LODs", next to the lathed-prop rule): "When LODs share one hand-laid UV layout, assert
  per LOD that every face's UVs fall inside one island rectangle: a ring count that does not divide
  the layout's split pushes a coarse LOD off the atlas, and only LOD0 is rendered."
- **A clamped map-range darkened every end face** (3 min). The rot gradient for the hollow tip was a
  clamped ramp on object x that read 1 everywhere below the hollow, so the butt cap and the stub tips
  baked as rot too. Fix: multiply by a second ramp that is 0 away from the tip. Seen in the first
  atlas view (albedo beside AO), not in the stats. No skill change: covered by "read the atlas".
- **A ground occluder in `highs` turns it into a bake source** (2 min). rock_small puts its ground
  plane in `highs`, which makes it a selected-to-active source; the merged `bake_to(occluders=...)`
  (rail station, 2026-10-19) keeps it an occluder only. Skill text (lib-contracts, `bake_to`): add
  "`occluders` stay visible without being bake sources: use it for a ground plane under a prop's AO
  bake (log_fallen), not `highs`".
- **The bark set alone could not carry the moss** (design, 0 min lost). A tileable `bark` slot cannot
  blend moss over it (slots cannot blend; a slot edge follows triangle edges), so the log bakes the set
  times the trunks' manifest tint into a unique atlas with moss and end-grain. Owner question in the
  report; no skill change.
