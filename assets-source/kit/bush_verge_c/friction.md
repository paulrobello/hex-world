# bush_verge_c friction

1. **An upright ellipsoid alone reads as an egg.** The first build set the family's crown to b > a
   (0.4 x 0.5 m) with no other change; the EEVEE three-quarter still showed a smooth egg dotted with
   blossom, not a shrub. `_verge_bush_c.spray_cards` wraps `_shrub.cards` (looked up by global name,
   asserted) and pushes the cards near five seeded spray directions on the upper two thirds out by
   up to 26% (Gaussian over the angle, 0.45 rad), which breaks the outline like arching shoots. Lost:
   about 5 minutes (one build, one still). Proposed skill text (pitfalls.md, "Leaves, foliage and
   LODs"): "A `_shrub` crown is a smooth ellipsoid with jittered cards, so changing only its axes
   gives a ball or an egg. Give a variant its own outline function over `_shrub.cards` (lumps in
   `_verge_bush_b`, sprays in `_verge_bush_c`) rather than relying on the axes."
2. **Blossom must survive the deep mips, so measure the tile mean, not the look at mip 0.** The
   brief worried that 1 cm florets would average away at play distance. The family file records,
   per outer tile, the blossom share of the opaque texels and the linear mean colour of the opaque
   texels with and without blossom (`blossomStats` in the bake report): 14-16% blossom moves the
   mean from (0.034, 0.095, 0.015) to about (0.14, 0.19, 0.10). The play-distance shot confirms it:
   both bush_verge_c instances on tile (0, 1) and (-1, 1) read as pale-flecked bushes beside the
   dark round ones. No time lost. Proposed skill text (project.md, "Variants"): "A variant meant to
   read differently at play distance needs a different deep-mip mean colour or silhouette; record
   the per-tile opaque mean (linear) in the bake report and compare it against the original's."
3. **The brief said verge bushes do not block the walker; the code says they do.** `walk.rs`
   `OBSTACLES` lists `bush_verge` and `bush_verge_b` (prop kind "tree"), and `_verge_bush.py` calls
   the octagonal collider solid to the walker. Kept consistent with the code: bush_verge_c is in
   `OBSTACLES` and the prop-kind test. No time lost; no skill change (a brief error, not a pipeline
   one).
4. **Measuring a variant-pick change needs a before and after layout dump.** `pick_variant` draws
   `below(count)`, so a third variant re-picks most instances, and clearance uses each variant's own
   ground reach, so a few positions move too. A temporary `#[ignore]` test in `scatter.rs` printed
   (tile, seed, piece, position, scale) for the four road tiles over seeds 0..32 and the thumbnail
   seed (`cell_seed(0, Axial::ORIGIN)` = 0, so the thumbnails use seed 0), run before and after the
   `VARIANTS` edit and diffed with a scratch script. Removing the test with a script left a blank
   line that `cargo fmt --check` caught. Lost: about 3 minutes. Proposed skill text (project.md,
   "Variants"): "Adding a variant to a group re-picks about (n - 1) / n of its instances and moves a
   few (each variant's own ground reach decides acceptance): measure it with a temporary `#[ignore]`
   layout dump before and after the `VARIANTS` edit, report the changed tiles and seeds, and say
   that the group's thumbnails (seed 0 at the origin) change, so `make thumbs` is due."
