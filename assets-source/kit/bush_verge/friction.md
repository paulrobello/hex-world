# bush_verge friction

1. **`_shrub.cards` `spread` sets how far down the dome the outer cards run, not their density.**
   The first build copied shrub_forest's `spread` 1.95 with 124 cards: the Fibonacci rows stop at
   zf = 1 - 2 / spread (about -0.03), so the cards covered only the top half and the bush read as a
   mushroom on three bare stems (EEVEE low side). `spread` 1.05 runs them to `zmin` -0.92, down to
   the grass. Lost: about 4 minutes (one build and one still). Fix: a comment in `bush_verge.py`.
   Proposed skill text (pitfalls.md, "Leaves, foliage and LODs"): "`_shrub.cards` places outer cards
   on Fibonacci rows from the top to zf = 1 - 2 / `spread` (or `zmin`, whichever is higher): a
   closed bush that sits on the ground needs `spread` near 1, and the hazel's 1.95 leaves the lower
   half open on purpose."
2. **A short stem straddles the ground and trips the wind assert.** With `length` 0.25 and `sink`
   0.06 the stem climbs only about 0.06 m to its third skeleton point (`stride` 3), so that ring sits
   on z = 0 and its lower vertices carry wind U > 0 below ground (`_shrub.build`: "AssertionError:
   bush_verge_lod0", no number printed). `sink` 0.1 fixes it. Lost: about 3 minutes. Proposed:
   `_shrub.build` could name the vertex and U in that assert; until then, note in the family file
   that `sink` must exceed the first kept ring's climb (length / 12 x stride x sin(el0)).
3. **Raster jitter versus a band test.** The verge band is tested on scatter's 0.25 m raster cell
   centres and a dart lands within half a cell, so an instance can sit up to 0.18 m past the band.
   The band test allows RASTER / sqrt(2), as the existing lane test allows 0.2. No skill change.
4. **The bake report's `reachM` is the widest LOD, not LOD0.** `_shrub.build` takes the reach over
   every LOD, and LOD2's cards are scaled 1.3x, so bush_verge reports 0.629 m while LOD0 reaches
   0.595 m. A first kerb test took LOD0 only; a clearance test must take the widest LOD (as
   `footprints_match_the_kit` does for crowns). Lost: about 5 minutes. Proposed skill text
   (project.md, "Fast loops and runtime couplings"): "A prop's leaves can reach past its collider
   and its LOD2 past its LOD0 (scaled-up cards): a clearance test against roads or paths measures
   the widest LOD from the GLB."
5. **Approvals written after provenance do not reach it.** `make kit-provenance` copies
   `approvals.json` into `provenance.json` when it runs, so writing the pending entry afterwards left
   provenance without it; re-running provenance fixed it. Lost: about 2 minutes. Proposed skill text
   (stages.md step 5): "Write the piece's `approvals.json` pending entry before `make
   kit-provenance`."
