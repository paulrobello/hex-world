# shrub_forest_b friction

1. **A dark evergreen reads near black under the spruce canopy in play.** The first palette (MID
   #1d4129) looked right in EEVEE and in the kit scenario's open sun, but on a pine forest tile in
   play, where the scatter puts the shrub in spruce shade, it read as a black lump, darker than the
   hazel beside it. One step lighter (MID #28573a, LIGHT #64965e) kept it darker and bluer than the
   hazel and readable in shade. The GLB was unchanged (maps only), so determinism held. Lost: about
   6 minutes (one rebuild, compress, provenance and re-shoot). Proposed skill text (pitfalls.md, "Look-dev
   and engine shots"): "Judge a forest-floor piece's albedo in play under the canopy, not only in the
   kit scenario's open sun: forest scatter stands it in tree shade, where a dark palette goes black."
2. **Play close-up cameras land inside spruce fronds.** The first sun-side camera 5 m from the
   instance sat in a frond and shot only needles. Cameras aimed at a forest instance need height
   (about 3 m) or a gap; re-shot once. No skill change: the review-sheet rule already demands the
   view, and the fix is per tile.
3. **Wrapping two family functions.** `_shrub.build` looks `cards` up by global name and
   `_shrub.atlas` looks `cluster_tile` up the same way, so `_shrub_holly.py` replaces both (an ogive
   card placer, a glossy berried tile) and asserts both lookups, both signatures and that
   `_shrub.build` has no defaults. It does not import `_verge_bush*` (they mutate `_shrub` at import and
   would join generatorDeps). No time lost. Supports bush_verge_b friction 3's proposed SKILL.md text
   ("a variant family file may wrap one of its functions through the module ... asserting the helper
   still calls it by global name and its signature").
4. **Instance positions for play shots.** `dump_state` reports scatter counts only, so the close-up
   targets came from a temporary ignored test printing `scatter::layout` per forest cell at world
   seed 1 (`cell_seed(1, cell)` plus `Axial::to_world(16)`), removed before the patch. No time lost
   beyond one compile; same method as bush_verge_b. Proposed skill text (stages.md step 7): "To aim a
   play close-up at a scatter instance, print `scatter::layout(def, cell_seed(world_seed, cell), …)`
   instances from a temporary test; `dump_state` has counts only."
