# fern_clump_c friction

Variant of fern_clump on the new family helper `kit/_ferns_var.py` (shared with fern_clump_b).

- **The collider sets the whole family's scatter footprint** (design, 0 min lost). `scatter.rs`
  `group_footprint` takes the largest ground reach over a variant group, and a rule's spacing and
  ground mask use it for every instance. A collider sized to this clump's 0.95 m frond reach would
  have spread the ferns on every existing pine forest, so the collider stays fern_clump's core
  (0.295 m, under the original's 0.3 m footprint). Proposed skill text (project.md, "Variants"):
  "A variant's collider reach must not exceed the original's FOOTPRINTS ground unless the owner wants
  the whole group's spacing to change: `group_footprint` takes the group's maximum, so one wide
  variant re-spaces every instance of the family on existing tiles."
- **The three-quarter EEVEE still was framed for the original's size** (2 min). The first camera
  sat 4 m out and showed the clump small and in shade; moved to 2.4 m. No skill change: piece-local.
- **A close-up camera placed by offset landed inside the back spruce's crown** (4 min, one sheet
  re-run). The first shade-side shot of this clump was a green blur over a third of the frame: the
  camera sat inside tree_forest's crown at (-7.5, -5.0). Fix: moved the camera clear of the crown.
  Proposed skill text (pitfalls, "Look-dev and engine shots"): "A review camera placed near a tree in
  the kit layout can sit inside its crown and shoot a foreground blur; read every shot before
  building the sheet and keep close-up cameras farther than the crown radius from any trunk."
- **Clump normals need the clump's own radius** (design). fern_clump rounds its normals over 0.6 m;
  a 1.9 m clump uses 1.0 m (`round_r`) so its outer fronds do not all face straight out. Piece-local.
