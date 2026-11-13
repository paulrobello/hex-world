# Friction: outcrop_granite_b (second outcrop variant, built ahead 2026-11-13)

1. **The brief named a refs folder that does not exist.** It gave `assets-source/refs/outcrop_granite`;
   the outcrop's approved refs are `assets-source/refs/boulders` (ref-2, approval A25), as
   outcrop_granite's provenance cites. Lost: about 1 minute. No skill change: provenance names the folder.
2. **The fracture code lives in a generator, not a helper.** outcrop_granite's redo puts its `Fracture`
   carving and lichen in `outcrop_granite.py`, which the brief kept byte-identical, so a variant cannot
   import it (importing a generator runs its build and is not hashed as a `generatorDep`). This file
   copies that code; once both outcrops are approved it can move to a `_outcrop.py` helper that both
   import. No skill change: SKILL.md already says shared per-family code belongs in `kit/_<family>.py`;
   the redo predates its variant.
3. **A new variant in a two-piece group of a rare rule broke a variety test by sample size.**
   `scatter::instances_draw_every_variant_of_their_rule` drew 12 seeds; the forest's outcrop rule places
   one on only a few of them, and every one drew `_b` (the pick hash itself is unbiased, checked for
   seeds 0-23). The test now draws 24 seeds. Lost: about 5 minutes. Proposed skill text (`dispatch.md`,
   integration): "A variant added to a rare scatter rule (outcrops, big boulders) can fail
   `instances_draw_every_variant_of_their_rule` on sample size alone; check the pick hash before
   changing anything else."
4. **The kit showcase had no room for a second 8 m stone on the boulder meadow.** (-2, 0) holds the
   first outcrop and four boulders, each 3.5 m inside the border; b went on a new meadow at (-3, 0),
   which raises `tile_counts` from 39 to 40. Lost: about 3 minutes. No skill change.
5. **Keeping the group footprint.** `scatter::group_footprint` takes the largest reach in a group, so a
   variant wider than outcrop_granite (4.25 m) would have moved every outcrop rule's darts and spacing on
   the meadow, forest and twelve hillside tiles. b's plan was sized to a 3.90 m collider reach, so
   placement is unchanged and only which mesh an instance draws changes. The ROCK_BASES 12-gon comes
   from `docs/checkpoints/outcrop_granite_b/rock_base.py` (supporting lines of the GLB's y = 0 slice).
   No skill change: the pitfall is the existing "lead their tiles' rules" note plus this file.
6. **A slab step that peaks at its edge reads as a ridge.** The redo's step term lifts the high side
   from 0.25 to 1.5 groove half-widths and fades it from the path at once, so with a short `fade`
   (1.4 m at step 0.10) the high side is a narrow raised band beside the joint: the close sun-side
   still showed a ridge, not a slab drop (coordinator pass 2). b's copy adds `bevel` (the ramp's
   outer edge, 3.2 half-widths here) and `hold` (the step stays full out to 1.8 m before fading over
   4 m), and the main joint runs the other way so the raised side is the high +x end. outcrop_granite
   keeps its terms (defaults reproduce them); its own joints have longer fades (1.0-2.2 m at steps of
   0.04-0.07 m). Lost: about 20 minutes. Proposed skill text (`pitfalls.md`, rocks): "A stepped joint
   whose high side fades from the groove reads as a ridge; hold the step level across the slab and
   ramp the edge over about three groove half-widths, and judge it in a low eye-level skyline still."
