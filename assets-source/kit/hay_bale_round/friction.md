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
