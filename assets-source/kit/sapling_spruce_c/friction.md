# sapling_spruce_c friction

1. **On a spruce under 3 m the collider takes the stem tip, so a leader bend spends the group's ground
   reach.** `_spruce.build` hulls the trunk rings nearest z = 0, 1, 2 and 3 m; on a 2 m stem the 2 m and
   3 m rings are both the tip, so any sideways bend at the top adds to the collider reach, and the
   sapling group's `FOOTPRINTS` ground is 0.1 m (wobble and radius already take about 0.045). A bend
   past about 0.06 m would have widened the group's footprint and moved every sapling. Fix: the crook
   is an offset BOW x sin^2(pi s) from 1.0 m to the stem tip (`_sapling_c.offset`, swapped in for
   `_sapling_b.offset`), zero with zero slope at both ends, so it shows 0.14 m mid-crown and the
   collider reach stays 0.058 m. No time lost (reasoned before the first build).
   Proposed skill text (project.md, Variants, after the spruce sentence): "A spruce variant's
   collider is the hull of its trunk rings nearest 0, 1, 2 and 3 m (`_spruce.build`), so a lean or bend
   at any of those heights (on a stem under 3 m, the tip) widens it. Assert the collider reach the build
   prints against the group's largest `FOOTPRINTS` ground (`_sapling_c.build`); on this 2.0 m stem a
   sin^2 crook that is zero at 1 m and at the tip shows 0.14 m and keeps the reach at 0.058 m."
2. **Low droop with the original's tip curl gives upswept V tiers, not level sprays.** `_forest._path`
   adds `CURL * t^3` (0.2 L) at every card tip, so `DROOP_SCALE` 0.35 left every tier's tips 0.16 L
   above its root: the first textured EEVEE still showed V-shaped plates. 0.8 reads as level shade
   sprays with upturned tips. Lost about 3 minutes (one skip-bake rebuild and still). No skill change:
   the still caught it, as the existing rule says.
3. **`_sapling.FILLER_WHORLS = 0` keeps every filler, not none** (`whorl_z[FILLER_WHORLS - 1]` is the top
   whorl). Avoided (advisor review); this piece keeps fillers on the lowest whorl only. Proposed skill
   text (pitfalls.md, Leaves, foliage and LODs, after "Conifer fronds"): "`_sapling.FILLER_WHORLS` must
   be at least 1: 0 indexes the top whorl and keeps every filler; thin fillers in a `branches` wrapper
   instead."
4. **Staging neighbours for stills without touching their committed reports.** `make kit-build
   PIECE=<neighbour>` copies its rewritten reports into `assets-source/kit/<id>/` (then a `git restore`
   per file). Running the generator directly, `HEXW_SKIP_BAKE=1 "$BLENDER" -b --factory-startup
   --python-exit-code 1 -P scripts/blender/kit/<id>.py -- target/kit-stage/<id>`, stages the GLB and
   maps for the stills and leaves the committed files clean. Saved the restore step; no time lost.
   Proposed skill text (pitfalls.md, Biome bases, "Staging an approved base" entry): "To stage an
   approved piece for a still or mock, run its generator straight into `target/kit-stage/<id>` (the
   Blender line `make kit-build` uses, without its `cp` of the reports), so nothing committed changes."
5. **Equal ground reach in a group means a new variant re-picks but never moves an instance.**
   `scatter::layout` tests clearance and `stand` with each instance's own variant's `FOOTPRINTS` ground;
   all three saplings carry 0.1, so over 1,325 forest tiles (world seeds 0-3 on a radius-10 disc plus
   the thumbnail seed) 1,798 of 2,643 saplings (68.0%) drew another piece and none moved, appeared or
   went missing, and no other rule's instance changed. Measured with a temporary ignored dump test
   (removed before the patch). The first dump run failed because the output directory the test wrote
   to did not exist (`std::fs::write` NotFound); about 1 minute. Proposed skill text (dispatch.md,
   part 11): "A variant with the same `FOOTPRINTS` ground as the rest of its group, and a crown at or
   below the group's largest (spacing and scale jitter read `group_footprint`), re-picks about
   (n - 1) / n of the instances and moves none; a different ground, or a crown above the group's, also
   moves positions. Say which in the report."
6. **Every sapling inside the five-forest shot cluster stands under a spruce crown, and a play script
   cannot place a tile away from the others.** The first probe (four azimuths round each of the four
   `sapling_spruce_c` instances there) gave 16 frames of spruce fronds: at 1.4-1.6 m every camera sat
   in the lowest whorls of a 16 m spruce (Q46: no world-seed-1 c instance on the radius-10 disc has more than 1.4 m
   of clearance from a crown). Tiles placed far from the cluster to reach open saplings were refused
   (`hex_core::World` returns `PlaceError::NotAdjacent`, "cell is not next to the world",
   `crates/hex-core/src/world.rs`), and the script ran on, so the cameras looked at empty ground and one
   frame came back black. Fix: rank the c instances on cells adjacent to the
   cluster by clearance from their own tile's crowns (`docs/checkpoints/sapling_spruce_c/run.sh`
   header), take the one 3.9 m inside the open east edge of (3, -1), and shoot it from outside the
   tile; the play shade side still sat inside a crown and was dropped (the kit undergrowth forest gives
   the shade close-up). Lost about 8 minutes (two shot runs at host load 200-300). Proposed skill text
   (SKILL.md, Owner checkpoints, after the sapling_spruce_b entry): "A play `place` command only takes
   a cell next to a placed tile (`PlaceError::NotAdjacent`; the script does not stop on it): pick play
   close-up targets on
   cells adjacent to the shot cluster, ranked by clearance from the crowns in the layout dump, and put
   the camera off the tile's open edge."
