# well_stone / well_stone_b friction log

Both pieces share one builder (`scripts/blender/kit/_house_well.py`), so their snags are logged here.

1. **kit-determinism failed on both pieces: "GLB differs".** Only LOD0's first primitive's `indices`
   bufferView differed between the two runs; positions, UVs and the JSON were identical, so the faces
   were the same and their order was not. Cause: `_house_longhouse.box` removes a box's dropped faces
   while iterating a set of BMFaces, whose order follows memory addresses, and later faces refill the
   freed slots in that order. With the well's 72 stone boxes the order changed per run. Fix: the well
   builds its boxes face by face in a fixed order (`_house_well.box`), without editing the approved
   helper. About 8 minutes (diagnosis with a GLB bufferView diff, one rebuild). Skill: new pitfall in
   `references/pitfalls.md` ("Removing faces while iterating a set of BMFaces breaks determinism").
   Measured afterwards: `house_twostorey_c` (`lh.box`) and `house_cottage` (`_house.Mesh.box`) are
   deterministic in `make kit-determinism` today, with far fewer such boxes; the pitfall notes the risk.

2. **The crank arm came out as a 0.9 m iron bar to the roof.** The arm's end was written as an
   absolute height where the box wanted an offset from the axle. The first textured EEVEE still showed
   it, so the fix cost about 3 minutes. No skill change: the "self-review from stills" rule caught it.

3. **Grass, scatter and the walker all read the collider, and a round prop on paving wants two
   different shapes.** `footprints_match_the_kit` pins `FOOTPRINTS.ground` (the grass and scatter
   clear) to the GLB collider's reach. So the collider has to reach the apron's rim, which would stop
   the walker at the rim, 0.6 m from the wall. `_house_longhouse.build_piece` also hard-codes the LOD2
   box, which for a round well reaches 1.96 m at its corners. Fix: a `collider` module-global shim
   that returns the LOD2 hull (reach 1.385 m, the apron polygon), and `walk::DECLARED_PARTS` for the
   walker: the covered well's roof footprint, and for the open well a stepped disc over the wall plus
   the posts and the crank. Planned up front, so no time lost. Skill: new note in `references/project.md`
   ("A prop on its own paving ties grass to its collider").

4. **The brief's footprint test command skipped hex-play**, where walk and scatter live
   (`cargo test -p hex-render --lib -- boids scatter walk kit_scene`). `references/project.md` had it
   right; `references/dispatch.md` had the short form. Fixed the dispatch template. No time lost.

5. **The masks atlas is 36 px/m, not the ~100 px/m an area estimate gave.** Each stone box and flag
   is its own island, and the family's 12 px island margin dominates at that island count. This is
   still about twice the houses' 13-18 px/m, `fill_unbaked` covers the sub-texel faces, and the build
   asserts 0 unbaked triangles. No skill change: pitfalls.md already says that the island margin sets
   a busy building's masks density.

- **LOD1 pass (2026-10-25, owner A2 Q4).** This piece's LOD1 gained the slots it dropped; the snags and the proposed skill text are logged once for all 16 pieces in `docs/checkpoints/lod1-pass/friction.md`.
