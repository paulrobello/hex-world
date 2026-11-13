# Friction: outcrop_granite stamped-lines redo (owner 2026-10-30)

The B1 set's shared log is `assets-source/kit/boulder_granite/friction.md`; this one covers the redo only.

1. **The brief named the wrong generators.** It listed `boulder_granite`, `_b`, `_c` and `_d` as the
   candidates for "the large boulder", but none of them has a crease: their tops are cleave facets.
   The straight lines were `outcrop_granite`'s three `Joint` V-grooves (`_boulder.shape_high` carves a
   groove by distance to a straight segment), a T that the owner saw on every instance. Found from the
   committed six-view `top.jpg` renders side by side and `docs/checkpoints/boulders/b1-play-meadow.jpg`.
   Lost: about 5 minutes. No skill change: a brief can only guess; the evidence step caught it.
2. **Changing one piece of a shared family without staling the others.** Any edit to `_boulder.py`
   (or `_rock.py`) marks every piece that imports it stale (four boulders, and for `_rock.py` also
   `rock_small_b`, `rock_small_c` and the tunnel portal). The generator instead replaces
   `_boulder.shape_high` and wraps `_boulder.sparser_moss` (module globals `_boulder.build` looks up),
   as `_river_flat.py` does for `_river`. No skill change: SKILL.md already names the module-global
   route.
3. **A top-only edit still moves the rim, so the grass outline needs a refit.** Every vertex below
   z = 0.3 m of the high poly is unchanged, but collapse decimation is global: the redo's top detail
   shifted which edges LOD0 and LOD2 keep, and the y = 0 slice of LOD0 plus the collider moved by up
   to 12.4 mm, against `grass::rock_bases_match_the_kit`'s 1 mm containment (scatter `FOOTPRINTS`
   kept its fit: collider reach 4.236 -> 4.242 against 4.25). Caught before compressing with a
   Python check of the staged GLB that mirrors both crate tests; the refit 12-gon (supporting lines
   of the slice hull) is `docs/checkpoints/outcrop_granite/integration.patch`. Lost: about 10 minutes.
   Proposed skill text (`pitfalls.md`, scatter): "Reshaping any part of a scatter rock whose base
   has a `grass::ROCK_BASES` outline moves its LOD0 rim through the global collapse decimate, even
   when the base of the high poly is untouched. Check the y = 0 slice of LOD0 plus the collider
   against the outline (1 mm containment, 2 cm touch) and the collider reach against `FOOTPRINTS`
   on the staged GLB before compressing, and hand back the refit outline as an integration patch."
4. **The old joints' sawtooth.** The committed `top.jpg` showed zig-zag teeth along the old N-S
   joint: a 0.22 m groove against the high poly's 5-8 cm vertex spacing. The redo subdivides twice
   more only near a fracture (`REFINE`). No skill change.
