# massif_granite friction log

This file moves to `assets-source/kit/massif_granite/friction.md` with the manifest entry. A
tracked source folder of a piece missing from the manifest fails
`no_kit_artifacts_outside_the_manifest`.

1. **The texel floor cannot hold on a landform piece.** The heightfield pass's 1,750 m² of
   unique rock and scree measured 32 px/m on one 2048² atlas. The column rock's 3,470 m² (true
   3-D slab sides) measures 21.5 px/m. Both are well short of 256 and of the boulders' 100, and
   the conflict is arithmetic, not packing. Fix: an options table for the owner (integration.md).
   Lost: about 10 min. Proposed `gates.md` text, Texel density row: "A landform piece (a peak
   over a footprint) cannot reach either floor with a unique bake: 2048² over 1,750-3,500 m²
   gives 20-32 px/m. It takes an owner-scoped floor or a world-space set on the rock."
2. **The lib's border helpers are single-hex.** `geom.set_cap_normals`, `set_border_normals_up`,
   `lod.border_verts` and `assert_border_identical` filter on `hexgeo.dist_to_border`. On a
   7-cell cap they see the inner member borders and miss the outer ring. `_massif.py` builds its
   own CDT, ring lattice and normals, and the Rust `footprint_base_errors` checks the result.
   Lost: 0 min (read first). Proposed `project.md` text, Seams: "A footprint base (mega tile)
   uses the 18-edge flower outline: 16 points per outer edge, no interior edges. Build the ring
   from the geometry pin and set its normals yourself; the lib's border helpers are single-hex.
   `kit_gate::footprint_base_errors` checks it."
3. **Blender reallocates the loop data on a mode switch.** A `uv_layers[...].data` reference taken
   before `mode_set(EDIT)`/`mode_set(OBJECT)` writes nowhere afterwards. The LOD islands
   silently kept their unit-square packing, and only the density print showed it. Fix: re-fetch
   after every mode switch. Lost: 5 min. Proposed `pitfalls.md` text, Blender and its API:
   "Re-fetch `mesh.uv_layers[...].data` after every `mode_set`: the old reference points at
   freed loop data, so reads are stale and writes vanish."
4. **A fixed-width smooth max lifts a plane.** `smax(a, b, k)` returns k/4 where a = b = 0. It
   lifted the whole flat ring 0.375 m, and the ring-anchor assert caught it. Fix: fade the width
   with min(a, b). Lost: 5 min. No skill change: piece-local maths.
5. **A piece held at an owner decision cannot commit its generator.** The orphan gate fails a
   tracked `scripts/blender/kit/<id>.py` without a manifest entry. The pipeline went into
   `_massif.py` (helpers are exempt), and the thin generator into `integration.patch`. Lost:
   5 min. Proposed `SKILL.md` text, Rules: "A piece that stops at an owner decision before its
   manifest entry keeps its pipeline in `kit/_<family>.py` and ships the thin generator in the
   integration patch, so the orphan gate stays green."
6. **`git status` in this worktree lists no untracked files.** Even `-uall` printed only the
   modified file. `git ls-files --others --exclude-standard` lists them. Lost: 2 min. No skill
   change: a worktree quirk.
7. **A heightfield cannot draw ref-2's slabs.** Voronoi columns along the fall line gave
   jointed plates and steep walls, but no overhangs or free-standing slabs. After five landform
   passes the peak still read as a steep jointed cone. Fix (shape pass): closed column prisms
   joined by a voxel remesh, smoothed and noise-displaced, as the high poly; the decimated
   high poly is LOD0, with a heightfield ground cap around it. Lost: about 30 min, plus the pass.
   Proposed `stages.md` text: "A landform hero piece whose reference shows slabs, clefts or
   overhangs builds its rock as closed primitives joined by a voxel remesh (Remesh modifier,
   VOXEL) and weathered by smoothing and triplanar displacement. LOD0 is that high poly
   decimated with its buried faces cut first, LOD1/LOD2 decimate LOD0 and keep its UVs, and
   the ground stays a heightfield cap with the covered faces left out (`_massif.py`)."
8. **Error-driven refinement cost texel density.** Two passes that added a point at each face
   whose centroid sat more than 0.5 m off the surface cut LOD0's worst rock error from 7.27 m to
   4.79 m. They added 300 tris, and smart project then cut more islands, so the rock fell from
   31.8 to 28.9 px/m. Reverted. The worst face was not a buttress wall, as first read: it was
   the bug in entry 9. Lost: 8 min. No skill change: piece-local.
9. **A point on a member border read as outside the footprint.** `outline_dist`'s inside test
   took the most-inside of the seven cells with `>= 0.0`. On the shared edge of two ring cells
   both cells give about -1e-15, so the ground fell to the rim height along those lines. That made
   the heightfield pass's 7.27 m LOD0 error. Found by probing `height()` across the line. Fix: a
   1e-6 tolerance. Lost: 10 min. Proposed `pitfalls.md` text, Coordinate frame: "A union-of-cells
   inside test (a footprint base) needs a small tolerance: on a shared member edge every cell's
   signed distance is a rounding error below 0."
10. **A bake cage wider than a joint starts rays inside the next column.** At cage 0.8 m and ray
    1.6 m, 4.6% of the rock's normal texels missed, and the joint faces baked from the wrong
    surface. LOD0 lies within 0.13 m of the high poly, so a 0.15 m cage and 0.3 m rays fixed it
    (misses 0.04%). Lost: 10 min. Proposed `pitfalls.md` text, Cycles bakes: "Size the cage from
    the measured LOD0-to-high distance, not the relief: on a jointed rock a cage wider than a
    joint starts rays inside the neighbouring part."
11. **Decimation leaves zero tangents.** The decimated rock LOD2 exported one vertex with a zero
    tangent, which `export_glb` refuses, although no face had zero area in space or UV. Fix: cut
    the faces at vertices whose `calc_tangents` tangent is under 0.5 long, after every
    decimation (`drop_degenerate`). Lost: 8 min. Proposed `pitfalls.md` text, Leaves, foliage and
    LODs: "A collapse-decimated LOD can carry a zero MikkTSpace tangent where flipped UV
    triangles meet. Run `calc_tangents` on the LOD and cut the faces at any zero-tangent vertex
    before export."
12. **A python heredoc that edits a file was refused by the worktree hook** ("too complex to
    verify"), where earlier heredocs had passed. Fix: write the edit as a script file and run it,
    as SKILL.md's "Worktree sessions" already says. Lost: 1 min. No skill change: the rule
    stands.
13. **Decimating an open shell lifts its rim.** The rock's buried feet were cut 0.4 m under the
    ground before decimating, and LOD1's collapse pulled one rim vertex 0.65 m back above the
    ground, opening a slit at the foot (LOD2: 0.1 m). Found by measuring min(ground - z) over each
    LOD's boundary vertices. Fix: `rebury_rim` after every decimation. Lost: 5 min. Proposed
    `pitfalls.md` text, Leaves, foliage and LODs: "A part planted in the ground and cut below it is
    an open shell: after decimating it, check every LOD's boundary vertices stay under the ground,
    and push any that rose back down."
14. **A decimated LOD that shares LOD0's UVs can stretch across islands.** LOD1 kept 99.9% of its
    area within 0.5-2x LOD0's density and rendered clean. LOD2 (548 tris from 6,164) had 4% outside
    it and drew dark wedges in a LOD2-only still. Fix: LOD2's rock gets its own islands and bake.
    Lost: 10 min. Proposed `pitfalls.md` text, same section: "Before letting a decimated LOD share
    LOD0's atlas, measure the share of its area outside 0.5-2x LOD0's texel density and render it
    alone; past a percent or so, give it its own islands and bake."
15. **One cage constant for every bake target.** After the rock's cage shrank to 0.15 m, the scree
    LODs' AO pass still used it and baked black where the coarse ground lies up to 0.9 m off the
    high poly. Fix: a reach per target (`SCREE_CAGE`, `ROCK2_CAGE`). Lost: 5 min. No skill change:
    covered by entry 10's text (size each cage from its own measured distance).
16. **A reference camera needs a constrained fit.** A free 3x4 DLT on ref-4's outline corners gave
    skew 226 px and fx 2,514 against fy 3,339; ref-2's homography decomposition had one negative
    f² candidate, so its heights were poorly conditioned. Fix: Levenberg-Marquardt on position,
    yaw, pitch, roll and one focal length (square pixels, principal point at the centre), with the
    skirt bottoms at z = -4 as non-coplanar points (`fit/cam_fit.py`; residuals 17 and 25 px on
    AI-drawn outlines). Lost: about 10 min. Proposed `stages.md` text, Reference: "To measure an
    approved reference (coverage, heights, a silhouette IoU), fit a constrained pinhole camera to
    its outline corners and skirt bottoms (docs/checkpoints/massif_granite/fit/cam_fit.py), then
    render the piece through that camera; an AI reference's outline is only roughly the true one."
17. **Smart project cannot unwrap a slab-faceted rock.** On the decimated rock (7,000 tris) it cut
    thousands of islands that filled 19% of their block, so the rock measured 12.3 px/m against
    the 20 px/m floor. Transferring the UVs of a smoothed copy filled 53% but folded triangles.
    Fix: smart project on a copy smoothed five times, `seams_from_islands`, then an angle-based
    unwrap of the real rock along those seams (`unwrap_into(..., seams_from=)`): 20.4 px/m with
    all the area within 0.5-2x the median. Lost: about 25 min. Proposed `pitfalls.md` text, Cycles
    bakes: "Smart project on a bumpy decimated rock cuts thousands of small islands (a fifth of
    the block filled). Cut the islands on a smoothed copy with the same topology, mark seams from
    them, and unwrap the real mesh angle-based along those seams."
18. **The tangent check ran before the rock had UVs.** `drop_degenerate` runs inside `rock_lod`,
    before the unwrap, and skips the MikkTSpace check without a UV map, so the zero tangent first
    showed at export. A NaN tangent also passes `length < 0.5`. Fix: run it again after each
    unwrap and test `not length >= 0.5`. Lost: 8 min. Proposed `pitfalls.md` text, Leaves, foliage
    and LODs (beside entry 11's): "Check tangents after the unwrap, not after the decimation, and
    count a NaN length as zero."
19. **Cutting the faces at a zero-tangent vertex leaves holes.** Entry 11's fix cut about 65 faces
    of LOD0, seen as dark notches up to 17 m above the grass. Fix: collapse each degenerate face
    and each zero-tangent vertex into a neighbour along its shortest edge
    (`bmesh.ops.collapse(..., uvs=True)`), and cut only what is still bad; LOD0 and LOD2 now have
    no open rock edge above the ground. Found by counting open edges above the cap. Lost: 10 min.
    Proposed `pitfalls.md` text, replacing entry 11's fix: "Collapse a degenerate face or a
    zero-tangent vertex into a neighbour (bmesh.ops.collapse with uvs=True) rather than deleting
    faces, then count the shell's open edges above the ground."
20. **Compositing only the island texels drew a line along every seam.** The maps took the bake
    only inside the rasterised islands, so the gutter kept the flat fill (default albedo, flat
    normal), and filtering at each island edge read it as a dark line. Fix: dilate the island mask
    by the bake margin before compositing. Lost: 5 min. Proposed `pitfalls.md` text, Cycles bakes:
    "When a generator composites baked images into its own maps, take the gutter too (the island
    mask dilated by the bake margin), or every UV seam draws a line."
21. **A decimated LOD0 cannot become LOD2.** Its UV seams and open rim held the collapse at about
    830 tris with a 7-9 m error. Fix: LOD2 meshes the same volume at 1 m and decimates it
    (`rock_coarse`): 361 tris, 0.65 m. Lost: 5 min. Proposed `stages.md` text beside entry 7's:
    "A rock built from a volume takes its LOD2 from the volume meshed coarse, not from LOD0."
22. **The AO distance has to follow the relief kept, not the piece.** At 2.5 m the rock's AO mean
    was 0.75 (whole clefts darkened); 1.2 m gives 0.82 against the boulders' 0.9 at 0.5 m. Lost: 3
    min. No skill change: `pitfalls.md` already says to set the AO distance to the scale of the
    local relief.
23. **The ff-only start assumed the previous branch held main's newest commits.** Main had two A54
    review commits the column-rock branch lacked, so the second fast-forward failed; the
    coordinator chose a reset to `d5cae34`. Lost: 2 min. No skill change: a dispatch detail.
24. **A lone triangle read as a hole.** LOD1's "hole" 11 m up was one loose triangle a decimation
    left floating at the surface: its three edges each had one face. Fix: `drop_loose` deletes
    loose bits of up to 8 faces on LOD1 and LOD2. Dropping LOD0's 141 loose faces too moved its
    islands, and two of two `make kit-determinism` runs then differed in one or two exported
    TANGENT components at the fourth decimal (different vertices each run), so LOD0 keeps them.
    (Corrected 2026-11-13, entry 31: the drop did not cause those flips. LOD0's tangents were
    threaded MikkTSpace noise all along, and the drop only moved some components onto rounding
    ties.) Lost: 20 min. Proposed `pitfalls.md` text, Leaves, foliage and LODs:
    "Count a LOD's open edges above the ground to find holes, and check a hit's edges: a triangle
    whose edges all have one face is a loose bit, not a hole; delete it, do not fill it."

## Form pass 2 (raised flanks, A55 Q1 O1, 2026-10-29)

25. **Raising the ground pushed the cap over the LOD0 budget.** The lift turned the old flat ring
    into gentle and mid slope, so the graded lattice spent 1.8 m and 0.9 m spacing there (the
    spacing steps are even multiples of the steep spacing, so 1.3 m is really 0.9 m): LOD0 went
    to 13,991 tris. Retuning the thresholds jumped between 10,062 and 13,727. Fix: the LOD0/LOD1
    classes come from the ground less the lift outside the rim's fade band, since the lift is
    smooth enough for the flat-ring spacing. Lost: 6 min. No skill change: specific to a field
    added on top of a graded lattice.
26. **The glTF exporter wrote a zero tangent that Blender's own tangents passed.** One fold vertex
    on LOD0's rock (23 m up, its normal facing down) got TANGENT (0, 0, 0) in the GLB, though
    `calc_tangents` on the rock gave every loop a unit tangent. Changing the decimation target by
    10 tris did not move it. Fix: `export_clean` reads the written GLB's non-unit tangents, collapses
    the nearest vertex's shortest edge in that LOD and exports again (deterministic, one pass).
    Lost: 8 min. Proposed `pitfalls.md` text, Export: "A tangent check in Blender does not prove
    the exporter's: when `check_unit_vectors` fails on a TANGENT that `calc_tangents` passed, map
    the GLB vertex back to Blender (x, -z, y), collapse it and re-export."
27. **A 2% larger rock surface dropped the scoped texel floor.** The steeper lift (5.5 m, 16-32 m)
    grew the rock to 3,950 m² and its LOD0 median to 19.15 px/m. Fix: a gentler lift (5 m, 15-35 m)
    and a left column of 0.20 instead of 0.25 (the LOD1/LOD2 blocks and the masks-only ground
    block shrink, drawn far off or masks only): 20.8 px/m. Lost: 4 min. No skill change.

## Form pass 3 (the grass alone rises, A55 Q1 O1 and the orchestrator's ruling, 2026-10-30)

28. **The profile check read its own sampling as drops.** form_measure took each bearing's
    profile from the 0.25 m top-down map's nearest cell, so on the steeper flank a sample stepping
    sideways read as an inward drop: 10-13 cm on four symmetric bearings (70/110/250/290 deg) of a
    numpy model whose exact rays dropped at most 2 cm. Fix: the profile casts a ray at each exact
    point along the bearing. Lost: 5 min. Proposed `pitfalls.md` text, Gates: "Sample a profile
    along a ray at exact points, not from a gridded map's nearest cell: on a slope the cell steps
    sideways and reads as a dip."
29. **A coarser lattice cut across the scree lens's sides.** With the flank on the gentle class's
    1.8 m spacing (it was the steep class's 0.9 m under the form pass's slopes) the cap's chords
    crossed a fan's thinning side and dipped 12 cm on one bearing. Fix: the fans add points along
    their sides (0.75 and 1.0 of the half angle). Lost: 4 min. No skill change.
30. **LOD2 over 15% of LOD0 by 26 tris.** The flank's true gradient put LOD2's ring on the steep
    class's 4.2 m spacing (1,716 tris against 0.15 x 11,270). Fix: LOD2 classes the ground less the
    flank too, the mid spacing on the rim's 6 m ease-in and the gentle 8.4 m inside it (1,594).
    Lost: 4 min. No skill change.

## Determinism fix (2026-11-13)

31. **Threaded MikkTSpace broke `make kit-determinism`.** Two fresh runs differed in one LOD0
    rock TANGENT x, -0.7455 against -0.7456. Hashing every mesh after every stage across two runs
    showed that positions, topology, edges, every attribute (`custom_normal` and `UVMap` among
    them) and the corner normals were byte-identical at every stage. Only the full-precision
    tangents differed, and only on the joined LOD0 body (11,468 tris). The 7,148-tri rock alone
    and LOD1 never differed. In one process, 40 `calc_tangents` calls on the same LOD0 mesh gave
    14 different arrays, up to 1.8e-7 apart on about a third of the corners. With `-t 1`, 40 of
    40 matched, and so did fresh processes. The exporter rounds tangents to 4 decimals in float32,
    and this tangent's x (-0.74554998) rounds onto the tie -7455.5, so the thread schedule chose
    the last digit. OpenVDB, decimate and the collapses were not involved: their output matched at
    every stage. Fix: `export_single_thread` saves the scene and exports it in a child Blender run
    with `-t 1`. The bake keeps every thread. The GLB is byte-identical to b06ac09's outcome-b
    build (sha256 04e459f6...). Lost: about 40 min, plus the hour before the host booking.
    Proposed `pitfalls.md` text, Export: "Blender 5.2.2's `calc_tangents` (MikkTSpace, which the
    glTF exporter calls) is threaded on large meshes, and its sums follow the thread schedule:
    about 2e-7 of noise. An 11,468-tri body varied, and meshes of 7,148 tris or fewer never did;
    the exact threshold was not measured. The exporter rounds
    tangents to 4 decimals, so a value on a rounding tie flips from run to run. When
    `make kit-determinism` differs only in TANGENT, export in a child Blender with `-t 1`
    (`_massif.export_single_thread`). Re-rounding hides the flip only until the next tie. To
    find the step, hash full-precision tangents, not the rounded GLB."
