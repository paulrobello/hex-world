# tree_oak / tree_oak_b friction

- **Reusing an approved helper at another size through its module globals** (0 min lost, by
  design). `_oak` copies `R0`, `DOME_*` and `SECTORS` from `_tree` at import and computes
  `SHELL_PHIS`/`SHELL_COLUMNS` there, and `_tree.hash01` and `_oak.lump_terms` read their module's
  `SEED` at call time while every `seed=SEED` default is bound at definition. `_oak_scatter.configure`
  sets `_tree.SEED`, `_oak.SEED`, both `SECTORS`, `_oak.SHELL_RINGS`/`SHELL_COLUMNS`/`SHELL_PHIS`
  (recomputed with `_oak`'s formula) and `_tree.TIERS`, asserts the invariants, and passes every
  seed explicitly. `make kit-check` shows no stale warning for any approved piece. Proposed skill
  text (project.md, "Variants"): "To reuse `_tree`/`_oak` at another size, set the globals both
  modules read (SEED on each, SECTORS on each, `_oak`'s three shell-ring tuples together, TIERS),
  assert they agree, and pass seeds explicitly: defaults like `seed=SEED` were bound at import."
- **Removed objects as None in the view layer** (2 min). The first skip-bake build failed in
  `geom.join` (`'NoneType' object has no attribute 'select_set'`) after removing the shell sector
  object. Fix: `bpy.context.view_layer.update()` after each removal. Already in pitfalls; no
  change.
- **A slim trunk's island is wider than a tight bark column** (2 min). `tree_oak_b`'s total bark
  UV area is 0.014, so the hero's tightest column fills (0.6-0.4) gave a column narrower than the
  trunk island itself and every pack failed. Fix: try looser fills down to 0.15 (picked 0.25).
  No skill change: helper-local.
- **Piece-class KTX2 budget on a 1024 tree atlas** (6 min). Four 1024² maps measured 1,513,229 B
  of BC7 (normal 598 KB, ORM 473 KB, albedo 340 KB, masks 101 KB at 512), 0.9% over 1.5 MB. Fix:
  ORM box-filtered to 512 as `rock_small_b`/`c` (now 1,184,089 B); albedo (alpha edges, measured
  density) and normal (leaf relief) stay at 1024. Probe the per-map bytes with
  `node scripts/assets/kit-compress.mjs piece <id> --class hero --out <scratch> --report <scratch>`
  (the hero class lets the probe finish). Proposed skill text (project.md, "Props"): "A tree at
  class `piece` with a 1024² atlas fits only with ORM at 512² (the scatter oaks: about 1.18 MB per
  family); the leaf-dome normal map alone is about 0.6 MB."
- **Pale bands on the limbs in the EEVEE under view** (5 min). They looked like branch start caps
  (the collar pitfall), so the limb, secondary and leader starts moved onto their parent's
  centreline with a start radius under the parent's inscribed tube (`buried_r`); buried culls rose
  from 96 to 106 (tree_oak) and 34 to 71 (tree_oak_b). The bands stayed where they were: they are
  sun patches through gaps between domes, not geometry. The buried starts are kept (they remove
  real start caps). No skill change.
- **Footprint values must clear the measured value, not round to it** (1 min, in a trial of the
  integration patch). The generator printed ground 0.58 (rounded); `footprints_match_the_kit`
  measured 0.5803579 and failed. Proposed skill text (dispatch.md, part 11): "Hand over FOOTPRINTS
  ground and crown rounded UP to 0.01 m from the unrounded collider and LOD reach; the gate rejects
  a value below the measured one."
- **A scattered oak on the oak tile changes what the walk pick test's ray meets** (3 min, trial).
  `walk::a_click_picks_the_nearest_drawn_prop_and_its_kind` casts at the hero oak from 40 m south;
  with the trial oak-tile rule, seed-dependent, a `tree_oak` stood in front and the pick returned
  it (correctly). It passed again after a 0.01 m footprint change moved the layout, so it is
  fragile, not fixed. Proposed skill text (dispatch.md, part 11): "A new scatter rule on a tile
  that tests aim rays or cameras at (walk pick, photo focus, perches) can put an instance in the
  line of sight: run `cargo test -p hex-play --lib -- scatter walk boids` with the rule applied and
  read each failure before calling it a regression."
