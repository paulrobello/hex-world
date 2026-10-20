# log_fallen_b friction

Variant of log_fallen on the new family helper `kit/_log.py`.

- **The approved generator is monolithic, so the variant pipeline is a copy** (design, 0 min lost).
  `log_fallen.py` builds at module level and cannot be imported, so `_log.py` carries its parametric
  surface, atlas layout, high-poly materials, bake and collider steps with the shape (`Spec`) and the
  moss pattern (`Moss`) as parameters, and keeps the original's friction invariants: an even ring
  count per LOD, every LOD face inside one UV island rectangle, the island-gap assert. Constants that
  were tied to the 1.8 m half length (body island height, the hollow's rot ramps) are derived from
  `half_l`. No skill change.
- **A solid break fanned to one point read as a sawn face** (8 min, one bake). The first snapped end
  was a fan from the jagged rim to a peak; with the rim's low-frequency jaggedness it read as a clean
  cut in the EEVEE still. Fix: a ring of splinters at half the radius that juts past or falls back
  from the rim (`Spec.splinters`), then the fan. 28 more LOD0 tris. No skill change: piece-local.
- **A dot-product moss mask with a wide range coats a whole flank** (one bake, shared with the
  above). `toward` (0, 0.75, 0.66) with range 0.1-0.55 and a low cut gave 39% moss, the +y flank
  solid green. Fix: range 0.25-0.7, a higher noise cut and a stronger along-grain stretch
  (x 0.25): 13.7% moss in streaks. Read the moss fraction in the bake stats and the moss-flank still
  before compressing. No skill change: covered by "read the numbers, not just the image".
- **Sink re-derived for the length** (3 min). The original's 0.13 m bury came from 20,000 seeded
  placements of forest_base.relief at half length 1.8 m (0.127 m). The same probe gives 0.085 m at
  1.05 m, so 0.12 m keeps both ends in the litter. No skill change.
