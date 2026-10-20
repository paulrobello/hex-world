# tree_oak_c friction

- **No family file needed** (0 min, by design). `_oak_scatter.build` takes the crown (`tiers`) and
  the skeleton (`shape`) as arguments, so a third scatter oak with the siblings' look is one 25-line
  generator, as `tree_oak_b` is; no approved helper is edited or wrapped, and `make kit-check` prints
  no stale warning. Proposed skill text (project.md, "Variants"): "When the family pipeline already
  takes the form as parameters (`_oak_scatter.build(tiers=…, shape=…)`), a variant is a generator only;
  add a family file only for a form those parameters cannot express (a lean, a fork, several stems)."
- **The scatter trunk leans toward the first top-tier clump, and the collider takes the lean**
  (2 min, reasoning before the first build). `_oak_scatter.skeleton` runs the trunk into `tops[0]`
  with a smoothstep lean from 1 m, and the collider is the hull of the trunk's lowest 3 m, so a top
  clump 0.6 m off the axis moves the trunk about 0.3 m at 3 m and pushes the ground reach past
  `tree_oak`'s 0.59 m, which would change the oak tile's group footprint and move every oak on every
  seed. Fix: one top dome on the axis (ring 0), measured ground 0.580 m. Proposed skill text
  (project.md, "Variants"): "A variant that must keep its group's footprint stays inside the largest
  sibling's ground and crown reach (`scatter::group_footprint` takes the maximum, and spacing, masks
  and overhang read it). For a scatter oak, the first top-tier clump sits on the axis or the trunk's
  lean grows the collider."
- **Adding a third variant re-picks most instances** (5 min, measured). `pick_variant` is
  `hash % count`, so going from 2 to 3 keeps only residues 0 and 1 of 6: about 2/3 of oak instances
  draw another piece, and because the dart test uses each variant's own ground reach, 6% of tiles
  also move or lose an oak. Measured with a temporary `#[ignore]` scan over 2,792 cell seeds (removed
  before the patch). Proposed skill text (dispatch.md, part 11): "A new variant in a group re-picks
  most instances (2 to 3 keeps a third). Measure it: a temporary ignored test that prints the tile's
  layout per cell seed, run before and after the `VARIANTS` change, and report the unchanged,
  re-picked and moved shares and the thumbnail seed `cell_seed(0, ORIGIN)`."
- **Writing the patch without a redirect** (0 min, avoided). `git diff -- crates > patch` puts `>` on
  a git line, which SKILL.md says the worktree hook refuses; `git diff --output=docs/checkpoints/<piece>/integration.patch -- PRD.md crates`
  writes it in one plain git command.
  Proposed skill text (SKILL.md, "Worktree sessions"): "Write a patch with
  `git diff --output=<file> -- <paths>`, never a `>` redirect."
