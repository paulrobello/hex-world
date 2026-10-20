# wildflower_single_e friction

1. **`_single.build` takes only the four `_props.SPECIES`.** It asserts the species against
   `_props.SPECIES` and draws heads through `_props.flower_head`, whose `else` branch would have drawn
   a poppy as a cornflower. Both are looked up at call time, so `_single_poppy.build` extends
   `_props.SPECIES` and swaps in a `flower_head` wrapper that delegates every other species, and the
   import-time asserts pin those lookups (`inspect.getsource`). No helper edited, no time lost. No skill
   change: project.md "Variants" already says to drive a family through its module globals.
2. **The family's stem cannot carry a single tall flower.** `_single.side_tile` draws the stem as a
   `_props._blade`, which tapers to a point and bends its tip sideways by up to 0.12 × its length
   (about 0.42 tile widths on a 3.5-width stem), so on the poppy's side tile 0 the stem ended at the
   tile's left margin with the head floating beside it. The poppy draws its own side tile from the
   family's canvas, blade painter, head placement, dilation and fill, with a stem of constant width on
   a cubic Bezier ending in the cup's bowl (`poppy_side_tile`). The approved siblings show the same
   quirk on some tiles (wildflower_single's daisy in `docs/checkpoints/prop-variants/lineup-singles.jpg`:
   one stem line passes left of the head); they are left as they are. Lost: one build and one atlas
   look, about 4 minutes. Proposed skill text (pitfalls.md, "Leaves, foliage and LODs"): "`_props._blade`
   is a grass blade: it tapers to a point and its seeded bend moves the tip up to 0.12 × its length off
   the line to its end point. Never draw a stem that must meet a head with it; draw the stem on a curve
   that ends at the head (`_single_poppy._stalk`)."
3. **The brief's `scatter::VARIANT_WEIGHTS` is not on main.** It exists only on the unmerged landing
   branch `land-b7` (`1f6102d`), which also changes `pick_variant`'s signature. The integration patch is
   built on main (uniform picks, so the poppy draws 1 in 5) and the weight row ships as a separate patch
   checked against `land-b7`'s `scatter.rs`. Lost: about 3 minutes. No skill change (a brief/landing
   order issue).
4. **A detail below about 0.5 m hides in the play grass.** The first bud hung at 60% of the card (about
   0.45 m with the instance scale) and no play camera showed it; the kit scenario, which has no grass,
   did. Raised to 70% (about 0.5 m); the GLB kept its SHA, only the maps changed. Lost: one rebuild,
   recompress, provenance and kit-check, about 8 minutes. No skill change: pitfalls.md already says to
   size plants against the play grass; this is the same rule for a part of a plant.
5. **Re-pick measurement for an equal-reach variant and for a weight.** A dump of `scatter::layout` from
   a temporary ignored test (nine tiles with a singles rule, seeds 0..255, the thumbnail seed 0 and the
   49 cells around the origin at world seed 1) before and after each edit gave: adding `_e` at the
   group's own 0.25 m ground re-picks 80% of single flowers and moves no instance on any tile; adding
   the group to `VARIANT_WEIGHTS` switches its draw from `below(n)` to `next() % total` and re-picks
   80% again. The world-seed-1 cells predicted the play run's per-piece counts exactly (47 and 32
   poppies over seven meadows), so the dump also gives the shot targets. No time lost. Proposed skill
   text (project.md, "Variants", after the re-pick rule the bush_verge_c log proposes): "A variant
   whose `FOOTPRINTS` ground equals the group's largest moves no instance, only re-picks; weighting a
   group in `VARIANT_WEIGHTS` re-picks it again, because the weighted draw uses another function of
   the same hash. Dump the cells around the origin at the shot's world seed (`cell_seed(1, cell)`) in
   the same test to get the play cameras' targets."
