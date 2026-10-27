# shrub_forest_c friction

1. **An elevation ramp cannot draw an arching cane.** The first cane placer stepped along each cane
   with the elevation falling linearly from about 78 degrees to -48 (as `_shrub.stems` does from 80
   to 55): the canes reached 1.3-1.4 m out and crested at 0.65-0.74 m, below the 0.95 m mound top, so
   they hid in the leaves and pushed the reach to 1.51 m (build assert). A cubic Bezier per cane, in
   its own vertical plane, with both inner controls at height apex / 0.75, sets the crest, the tip
   height and the tip reach directly (`_shrub_bramble.cane_stems`). Lost: about 6 minutes (two builds
   and a scratch simulation of the arcs in plain Python). No skill change: specific to arching canes;
   the docstring records the shape.
2. **Cards at a cane tip read as parasols.** The first cane cards sat flat, facing up, on the last
   points of each cane: in EEVEE the canes looked like spider legs with an umbrella on each foot.
   Three smaller cards along the outer bow, alternating sides of the cane and facing out from it,
   read as leafy canes. One EEVEE still caught it before any compress. Lost: about 4 minutes. No skill
   change: covered by the existing rule to judge foliage from a textured EEVEE still first.
3. **An ellipsoid with only azimuthal lobes reads as a clipped box bush.** Low-order radius and
   height lobes kept a smooth closed outline (topiary). Seeded sub-mounds and dents on the upper shell
   (gaussian bumps in the direction from the crown centre, `bumps`) broke the outline. Lost: one
   rebuild. No skill change.
4. **The re-pick scan needs positions, not just picks.** With a third variant `pick_variant` draws
   from 3 instead of 2, and an instance's own ground reach feeds both the clearance test and `stand`
   over a shared RNG, so a re-pick between pieces of different reach moves later darts (shrubs and
   the saplings after them). A temporary ignored test dumping `layout` for 1,325 forest seeds (four
   world seeds over a radius-10 disc, plus `cell_seed(0, origin)`) to `target/`, run before and after
   the `VARIANTS` edit and compared in Python, gave all three shares and the seed-1 instance positions
   for the play cameras in one pass. No time lost. Proposed skill text (stages.md step 7, after the
   bush_verge_b / shrub_forest_b proposal): "For a variant's re-pick impact and its play-shot targets,
   dump `scatter::layout` per cell seed from a temporary ignored test before and after the `VARIANTS`
   edit; compare picks, positions and the later rules separately, and remove the test before writing
   the patch."
5. **Keeping the group footprint.** A rule's ground mask and spacing use the group's largest ground
   reach (`group_footprint`, the hazel's 0.71 m), so a wider collider would have moved every shrub on
   every forest tile. The bramble's leaves reach 1.25 m but its collider stays at 0.69 m (footprint
   0.70), asserted in `_shrub_bramble.build`. No time lost. Proposed skill text (project.md,
   Variants): "Keep a scatter variant's collider reach at or below its group's largest `FOOTPRINTS`
   ground: `group_footprint` sets every rule's mask and spacing, so a larger one moves the whole
   group on every tile."

## Matte leaves (owner A23 Q2, 2026-10-26)

6. **The staged GLB never matches the committed one.** To prove a map-only rebuild left the mesh alone, the first check compared `target/kit-stage/<id>/<id>.glb` with `assets/kit/p1/<id>/<id>.glb` and reported a difference: the staged file is the raw Blender export (239 KB), the committed one the glTF-Transform meshopt output of `kit-compress` (168 KB). Compare the committed GLB before and after `kit-compress` instead (identical here). About 3 minutes lost. Proposed skill text (pitfalls.md): "Prove an unchanged mesh on the compressed GLB: hash `assets/kit/p1/<id>/<id>.glb` before the rebuild and after `kit-compress`. The staged GLB in `target/kit-stage/` is the uncompressed Blender export and never equals the committed file."
7. **`kit-check` fails until the approval note's citations are committed** (`provenance_notes_cite_only_committed_files`: the new helper and the sheet). Expected per gates.md (Provenance); commit, then rerun. No time lost, no skill change.
8. **An untextured six-view render changed for an unchanged GLB** (shrub_forest `renders/right.jpg`, load about 600). This is the EEVEE-under-load variance gates.md already records (Determinism); the GLB hashes are identical. No skill change.
