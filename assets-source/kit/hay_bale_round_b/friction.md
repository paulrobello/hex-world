# hay_bale_round_b friction (2026-10-20, built with hay_bale_stack_b from kit/_hay_var.py)

1. **`_hay.ZC` is computed at import.** `ZC = -SINK + R * SQUASH` runs when `_hay` loads, so setting
   `SQUASH` from the family file would not move the lathe axis the shader and `_rest` read. Fix:
   set `_hay.ZC` directly and assert `_rest`'s output. 0 min (caught reading the helper before
   writing). Proposed SKILL.md text, appended to the "Helpers" rule's module-globals sentence:
   > A global derived from other globals at import (`_hay.ZC`) does not follow them: set it too, and
   > assert a function that reads it.
2. **Contact-float filter counted the straw mound as an edge.** The first float check flagged 6 cm
   in the second round slot: it treated the slope ring (r = 0.86, deliberately above ground) as a
   contact. Fix: only the true outer ring (r > R_SLOPE) and the foot count. 3 min. No skill change.
3. **Dark contact ring on a shared barrel island.** Upright, every barrel sector shares one island,
   and `_hay.bake_maps` adds a ground plane at z = 0 for AO. With the receiver lifted 0.10 m the plane
   was still inside the 0.3 m AO distance, and the straw high started under the bale, so the foot's
   lowest 5-10 cm baked AO 0.02 all round (a black line in the EEVEE still). Fix: start the straw
   high at the bale's foot radius and lift the receiver past the AO distance (LIFT 0.5), so only the
   straw darkens the foot. Barrel AO at v 0.02 went from 0.025 to 0.993. 2 rebakes, 8 min. Proposed
   pitfalls.md text, under "Cycles bakes":
   > **A bake-time ground occluder within the AO distance darkens a shared island all round.** A
   > rotationally repeated prop stood on end (`hay_bale_round_b`) shares one island across every side,
   > so ground-contact AO is wrong on all of them. Lift the receiver and its highs past the AO
   > distance for the bake (`_hay_var.LIFT`), and do not start an apron high under the body it rings.
4. **Barrel sector seam reads on the upright bale.** The four barrel sectors share one island, so the
   net's diagonals do not meet at the sector boundaries (a vertical zig-zag line). The original
   `hay_bale_round` has the same seams, mostly on its top and underside where they read less. Not
   fixed: it lives in the approved `_hay` UVs and shader. Raised as an owner question. 0 min.
5. **A worktree hook refused a `for p in …` loop over Blender runs.** Ran each piece as its own call.
   1 min. Already in SKILL.md ("Worktree sessions").
6. **Entry 4 fixed (owner Q55, 2026-10-20, bale-seam-fix).** The seam had two causes, both in
   `_hay` (see hay_bale_round friction entry 4): non-periodic straw and net, and a receiver whose
   open edge normals leaned the normal map. The upright bale also leans (LEAN 3 cm along lathe y),
   which moves the angle and radius the periodic fields read at a sector's two edges by different
   amounts, so its high stores the lathe frame before the slump as the point attribute `lathe` and
   the straw reads that (`straw_material(coords=...)`). Sector seam at LOD0: albedo step 14.4 -> 5.6
   (interior 8.5), normal step 10.1 -> 2.0 degrees. 0 min lost beyond the hay_bale_round entry.
   No skill change beyond that entry's proposal.
