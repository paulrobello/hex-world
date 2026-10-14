# river_spring_base friction

- **The PLAN budgets (3,124 / 1,332 / 390) can't hold the 0.03 m tolerance, so LOD0 sits near the 4,000 gate cap (orchestrator accepted the deviation).**
  - The tolerance applies at every LOD, and the fixed parts alone cost about 260 tris: the one-lane notched ring plus the one-row skirt, which covers about 84 ring segments.
  - The pool bowl needs rings on its kinks (r = 4.165 and 5.95) and 20 to 33 spokes per ring on the steep wet bank, because chord sag ≈ slope × r × dθ² / 8.
  - That puts LOD2 near 560 cap tris (726 with the skirt). Only a LOD0 of about 3,630 or more keeps that inside the lane-cut 20% LOD2 ceiling.
  - Shipped: 3,970 / 1,602 / 726 (LOD2 18.3% of LOD0).
  - About 40 minutes lost.
- **Refine against the bed, not the relief.** The first loop also tested each face against the meadow relief surface (0.03 m). Inserted points carry relief too, so the meadow refined without end.
  - Fix: refine on |z - bed| only.
  - RELIEF is 0.06 m (river bases use 0.10 m), which keeps headroom under the 0.10 m dry tolerance.
- **Split edges, don't insert centroids.** Centroid insertion added 8 to 16 points per round with no end in sight. Splitting the longest non-ring edge converges in 1 to 4 rounds.
- **Thinning the rings below the sag estimate costs more than it saves.** Dropping the r = 3.15 ring, or cutting the spokes on the dry outer rings, made refinement add more triangles than it removed. The waterline-band rings (4.45 to 5.95) need about 25 to 31 spokes.
- **Columns, not rows, inside the crease (LOD2).** The stream term depends only on y.
  - So the wedge between the crease and the first full row is carried by one crease point per OFFS offset, joined by a constrained edge of constant y to that row (`columns` in `_spring.spring_cap`).
  - That took LOD2 from 806 to 726 tris and made it pass the 20% ceiling.
- **Sampling matches `_river.profile_errors`.** The generator samples vertices and face centroids. Edge midpoints were tried first and cost about 100 more LOD2 tris.
