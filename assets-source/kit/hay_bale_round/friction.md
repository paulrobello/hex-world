# hay_bale_round friction (2026-10-19, built with hay_bale_stack from kit/_hay.py)

1. **Removed object left a `None` in the view layer.** `bpy.data.objects.remove(high)` before
   `lod.triangulate` (whose `scene.select_only` iterates `view_layer.objects`) raised
   `'NoneType' object has no attribute 'select_set'`. Fix: `bpy.context.view_layer.update()` after the
   removal. 2 min. No skill change: already in pitfalls.md ("Removed objects linger in
   `view_layer.objects` as `None`").
2. **Overlapping UV islands need a receiver subset.** The lathe's four barrel sectors share one island
   and both caps share one (mirrored), so every LOD samples the same texels with no decimation. A bake
   onto the whole LOD0 would write the same texels four times, and the bottom sector would bring its
   ground-contact AO. Fix: bake onto a copy of LOD0 that keeps only the top sector and one cap
   (int face layer `island`). 0 min lost (planned up front). Proposed pitfalls.md text, under
   "Leaves, foliage and LODs":
   > **A lathe or other rotationally repeated prop can share islands across its repeats.** The round
   > hay bale (`kit/_hay.py`) maps four 90-degree barrel sectors onto one island and both caps onto one,
   > with analytic UVs computed from the profile, so every LOD (built from the same profile at fewer
   > columns, with seams on angles every LOD has) samples LOD0's atlas exactly and the 1024² albedo
   > holds about 430 px/m on a 1.5 m bale. Bake onto a receiver that keeps one copy of each island
   > (the top sector, so the ground contact AO stays off it); the gate's per-material density still
   > reads every face.
3. **Shot placement: a canopy shadow is not the sun side.** The first stack spot sat inside the hero
   oak's shadow, so its "sun side" close-up was in shade. Fix: moved the stack to offset (-5.5, 6.0)
   and re-took the shots. 4 min. Proposed SKILL.md text under "Owner checkpoints": "Place the piece
   for its review shots in open sun: the kit sun is at (200, 400, 150), so a tall neighbour's shadow
   falls toward -x/-z of it."
4. **Sector seams (owner Q55, 2026-10-20, bale-seam-fix).** Measured on the shipped GLB and maps
   (`docs/checkpoints/hay-bale-seams/measure.py`: both sides of every shared edge sampled one texel
   inside each triangle): at LOD0 the sector seams stepped 16.9 in albedo (interior edges 8.2) and
   10.2 degrees in shading normal (interior 2.1), the same at LOD1 and LOD2. Two causes:
   - **Non-periodic straw.** The noises read object xyz and arc length, and the net's 0.11 m spacing
     gave 10.7 lines per 90-degree sector, so a sector island's u = 0 and u = 1 edges carried
     different straw (the net's zig-zag). Fix: every round-straw noise and the high's relief read a
     SECTORS-fold circle embedding of the angle (`_hay.periodic_point`, fading to the plain point
     inside the caps), and the net has 11 lines per sector. Albedo step 16.9 -> 5.0.
   - **The open receiver leaned the normal map.** The receiver is one sector of LOD0 with the rest
     deleted, so its edge vertices averaged only the kept faces and leaned half a column (5.6
     degrees) inward; the tangent bake wrote the opposite lean into the island edges (normal-map x
     -0.086 at u = 0, +0.075 at u = 1), and the full mesh, whose seam normals are radial, showed a 10
     degree step. Fix: `_hay.receiver_of` sets the kept corners to LOD0's own vertex normals. Tilt
     -> 0.000, normal step 10.2 -> 1.5 degrees.
   - Also: the cap island baked its own ground-contact AO, which the barrel's shared top-sector
     texels cannot match at the rim (AO step 16.6). Fix: bake lifted past the AO distance
     (`BAKE_LIFT`, as round_b). Rim AO step 16.6 -> 1.0. The rim's albedo step (17, against 11-15
     for two texels radially inside either island, the rings' own gradient) is left; the -x cap
     still samples the +x cap mirrored.
   - Lost 4 min: the first `receiver_of` mapped normals through `BMVert.index` after a bmesh delete,
     which does not keep the source indices, and 7% of texels missed the high (normalMissFraction
     0.07). An int vertex layer carries the source index instead.
   Proposed pitfalls.md text, under "Leaves, foliage and LODs", after "A lathed prop can share UV
   islands across its repeats":
   > **Shared repeat islands need periodic content and closed-mesh normals.** A sector island is
   > drawn on every repeat, so its u = 0 edge meets its own u = 1 edge: every procedural field the
   > bake reads (and the high's relief) must repeat once per sector, through a circle embedding of
   > SECTORS x angle rather than object xyz or arc length, and line patterns need a whole number of
   > lines per sector. Bake onto a receiver cut from the LOD with that LOD's own vertex normals on
   > the open edges (`_hay.receiver_of`), or the tangent bake writes the edge vertices' inward lean
   > into the map (a 10 degree step at every hay bale seam). Carry source indices through a bmesh
   > delete in an int layer, not `BMVert.index`. Measure seams with
   > `docs/checkpoints/hay-bale-seams/measure.py` against the interior-edge baseline.
5. **Native after-shots came back black on every retry (2026-10-20).** The before set rendered at
   14:59; from 15:14 every native run wrote all-black frames (mean 0.000, about 26 KB), five runs
   in a row, with the console unlocked and `caffeinate -u -d` holding the display (host load
   340-375). The 1600x1000 window was most likely behind another app's window (pitfalls.md,
   "Native shots can come back black"). Fix: shot before and after on one web build in headless
   GPU Chromium, which has no window to cover: `make build-web`, clone `dist/` (`cp -cR`), write
   the old revision's maps over the clone (`docs/checkpoints/hay-bale-seams/old_maps.py`), run
   `e2e/shots.mjs` on each, and force the LOD with an in-script `{"cmd":"debug","force_lod":"1"}`
   so one script covers LOD0 and LOD1 (`run-web.sh`). 12 min (mostly the wasm build). Proposed
   pitfalls.md text, under "Look-dev and engine shots", after "Native shots can come back black":
   > **When native shots stay black, shoot before and after on the web build.** Headless GPU
   > Chromium (`e2e/shots.mjs dist <script>`) captures no window, so another app cannot cover
   > it. For a before/after of an asset change, clone `dist/` and write the old revision's maps
   > (or GLB) over the clone from git, so both sets come from one build; force a LOD inside the
   > script with `{"cmd":"debug","force_lod":"1"}` (`docs/checkpoints/hay-bale-seams/run-web.sh`).
6. **Stack provenance refused: "_hay.py changed since the build".** I rebuilt the two stacks in
   parallel with the round bales, then edited `_hay.receiver_of` once more (entry 4's index
   fix), so their export reports carried the older helper hash. Fix: rebuilt both (GLB and maps
   byte-identical again) and re-ran provenance. 4 min. Proposed SKILL.md text, appended to the
   "Provenance" rule's stale-pieces sentence:
   > Rebuild every piece that imports an edited helper after the last edit to it, not while you
   > are still iterating on it: provenance refuses a build older than its helpers.
