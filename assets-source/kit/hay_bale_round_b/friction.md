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
7. **LOD2 straw-apron seam, 36 degrees (bale-b-apron, 2026-10-20).** Left by entry 6. Measured with
   `docs/checkpoints/hay-bale-seams/measure.py` on the shipped GLB and maps: the LOD2 apron seam
   stepped 36.4 degrees in shading normal (p95 44.6) against an apron interior of 2.5; LOD0 2.8 and
   LOD1 4.9. Cause (`docs/checkpoints/hay-bale-b-apron/diag.py`): at LOD2 each 60-degree apron
   segment is one quad whose vertices belong to that segment only, so a seam vertex carried that
   quad's facet normal, 41 degrees from the other side's and 27 degrees off LOD0's normal at the
   same point. The tangent normal map was baked against LOD0's normals (the receiver), so LOD2 read
   it in a frame tilted differently on the two sides of each seam. Fix:
   `_hay_var.apron_normals_from_lod0` gives LOD2's apron corners LOD0's vertex normal for the same
   island at the same point (every LOD2 apron vertex lies on one), as custom normals; the bale's
   corners keep their own (zero vectors). They survive `finish`'s second triangulate and the export
   (0.00 degrees off LOD0 in the GLB). LOD2 apron seam 36.4 -> 4.5 degrees (p95 9.8), the LOD2
   barrel seam's level (4.3). Tris, positions, UVs and maps unchanged; LOD0 and LOD1 decode
   identical (`compare.py`). LOD1 has the same mechanism at 7.5 degrees off LOD0 (step 4.9 against
   its interior 3.8), left byte-identical by the brief; the same call with level 1 would cover it.
   0 min lost. Proposed pitfalls.md text, under "Leaves, foliage and LODs", after "Shared repeat
   islands need periodic content and closed-mesh normals":
   > **A coarse LOD that samples LOD0's baked tangent map needs LOD0's normals where it can take
   > them.** The bake writes the high's normal relative to the receiver's (LOD0's) interpolated
   > normal, so a LOD whose vertex normals differ reads the map in a tilted frame. Built per island
   > with few columns (`hay_bale_round_b`'s apron: one quad per 60-degree segment at LOD2), a seam
   > vertex's normal is that one facet's, and the tilt differs on the two sides of the seam (a 36
   > degree step). Where a LOD's vertices coincide with LOD0's, copy LOD0's normal per island as a
   > custom normal (`_hay_var.apron_normals_from_lod0`) and check it in the GLB.
8. **Forced LOD2 in engine hid the apron it was meant to show.** `force_lod` sets every `Lod` part
   (`crates/hex-play/src/debug.rs` `force_all`), the meadow base included, and the LOD2 apron's
   straight chords run inside the bale's foot at mid-segment (0.878 x cos 30 = 0.76 m against a
   0.78 m foot). The in-engine LOD2 shots show only a thin straw outline, before and after alike,
   where the EEVEE still over a lowered ground shows the whole band, so the LOD2 meadow most likely
   covers the rest (mean pixel difference 0.03-0.10, below the 0.56-1.10 that the
   identical LOD0 shows from birds and TAA). Fix: textured EEVEE stills of each LOD alone over a
   ground 0.1 m low (`docs/checkpoints/hay-bale-b-apron/still.py`), cropped to the apron corners.
   At play LOD2 distance (400 m+) the 8 cm apron is under a pixel, so the hidden apron is not a
   defect worth geometry. 6 min. Proposed pitfalls.md text, under "Look-dev and engine shots":
   > **Forced LOD shots move the ground's LOD too.** A thin draped skirt (a straw apron, a verge)
   > can sink under a LOD2 base and vanish from a `force_lod` shot. For seam evidence on such a
   > part, render the LOD alone in a textured EEVEE still with the ground lowered.
9. **`e2e/shots.mjs` failed: no `playwright` package.** I bootstrapped without `--web`. Fix:
   `bash scripts/worktree-bootstrap.sh --web`. 1 min. No skill change: SKILL.md already says
   "`--web` for e2e gates"; the web shot route uses those modules too, which this entry records.
