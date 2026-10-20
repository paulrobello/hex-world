# sapling_spruce friction

1. **The spruce pipeline's absolute sizes do not scale to a 2 m tree.** `_forest.crown_r` adds a fixed
   0.32 m tip radius, `core_mesh` a 0.6 m tip spike and 0.25 m foot, and `card_mesh`'s leader a fixed
   0.17 m half width, all sized for a 16 m spruce. The first sapling still showed the core spike above
   the crown and leader cards as wide as a tier. Fix: `_sapling.py` wraps those three functions (and
   thins `branches` to fillers on the lowest two whorls) by module-attribute replacement at call time,
   asserting the wrapped signatures, so no approved helper changed. About 10 minutes.
   Proposed skill text (project.md, Variants): "Driving `_spruce.build` at a very different size: besides
   the module globals, `_forest`'s crown floor (0.32 m), core tip (0.6 m) and leader half width
   (0.17 m) are absolute; wrap `crown_r`, `core_mesh` and `card_mesh` from the family file (`_sapling.py`)."
2. **LOD2 at 15% on a small tree.** A sapling's trunk, core and leader are a large fixed share; the
   first build had LOD2 = 96 / 488. Raising LOD0 card segments to 3 (better droop up close) and keeping
   every third card at LOD2 gave 608 / 258 / 76. One rebuild; already covered by the "size LODs first"
   guidance, no change needed.
3. **Staging approved neighbours for stills rewrote their committed bake reports** (`skipped: true`).
   Already in pitfalls ("Staging an approved base for a mock dirties its committed reports"); the rule
   applies to any approved piece, not only bases. Proposed edit: "Staging an approved piece for a mock
   or still dirties its committed reports". The worktree guard also refused `git checkout -- <two
   files>`; `git restore <file>` one file per call passed. About 3 minutes.
