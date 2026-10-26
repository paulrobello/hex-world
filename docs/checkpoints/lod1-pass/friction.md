# LOD1 pass friction log (2026-10-25)

One log for the 16 pieces this pass rebuilt (owner A2 Q4: "every piece should have a middle LOD"); each
family's own `friction.md` points here.

1. **No piece shipped without a LOD1 mesh.** Every manifest GLB has `<id>_lod1`. The gap was per slot:
   23 (piece, slot) pairs on 16 pieces had LOD0 geometry and none at LOD1 (the river cattails, house_pad's
   doorsteps, the windmills' iron, the bakeries' wicker and bread, the station's coping, iron, timber,
   board and flowers, the wells' buff stones and mortar). Because `hex-play`'s `KitFamilies` keys a draw
   family on (piece, material, ordinal within the LOD), a missing slot also shifted every later LOD1
   primitive into a different family than its LOD0 counterpart (97 families without LOD1 across 32
   pieces before; 0 with a LOD0 mesh and no LOD1 mesh after, 62 LOD2-only ones remain). Fix: enumerate
   from the GLB JSON by slot and by family, not by mesh name. About 5 minutes.
   Proposed skill text (`references/gates.md`, LOD1 / LOD2 row): "Every LOD1 carries every LOD0 slot, in
   LOD0's material order: the runtime pairs LOD meshes by (material, ordinal), so a slot dropped at LOD1
   both hides that part from 150 to 400 m and shifts every later primitive. Not pinned yet: proposed
   `kit_gate::lod1_carries_every_lod0_slot`."
2. **Any new LOD1 face repacks the whole masks atlas.** The atlas pieces unwrap every LOD together
   (`uv.atlas_unwrap`, `bakery_shop.unwrap`, `_station.unwrap`: smart project plus one pack), so a LOD1
   change moves LOD0's UV0 and re-bakes LOD0's masks, which an approved piece must not do. Fix: change
   LOD1 only after the bake: `scripts/blender/kit/_lod_copy.py` copies chosen LOD0 faces with their
   baked UVs into LOD1; new faces (the platform's 3-sided tubs and posts) put every UV on one baked texel
   of the part they stand for; cuts of an existing LOD1 face (the wells' courses) interpolate its UVs.
   Every masks PNG and every LOD0 primitive stayed byte-identical. Planned up front, no time lost.
   Proposed skill text (`references/pitfalls.md`, Leaves, foliage and LODs): "Adding a part to a coarser
   LOD of an approved atlas piece: never before the unwrap (the pack moves LOD0's islands). Copy LOD0's
   faces in after the bake with `_lod_copy.copy_faces`, or place new faces' UVs on a baked texel of the
   part they stand for; check with the staged GLB that LOD0's primitives are byte-identical."
3. **The first support copy duplicated a face LOD1 already had.** `bakery_shop`'s bench rule (timber
   below 0.6 m within 1 m of a basket) also caught a window post both LODs share: 6 coplanar duplicate
   triangles. Fix: `copy_faces` skips any face whose vertex positions match a face already in the target
   LOD. About 5 minutes.
4. **A radius rule caught a wall rail behind the shelter bench.** The shelter's bench timber is selected
   inside the legs' plan box grown by 0.15 m instead (the 8.2 m rail sits 0.55 m behind the bench). About
   3 minutes.
5. **river_spring_center asserted a fixed LOD1 cap of 320 tris** (no recorded reason; it matched the
   straight centre's old LOD1), which the cattails exceed. Dropped it; the 50% ratio is still asserted.
6. **EEVEE re-render churn under load.** `windmill_b`'s three-quarter render changed on 39,696 pixels
   (3.8%, up to 21 levels) at host load 700 while its LOD0 primitives were byte-identical; the committed
   render was kept. `references/gates.md` (Determinism) says "a few levels on a few hundred pixels"; at
   load 700 it is tens of thousands of pixels. Proposed text: "...can differ by up to about 20 levels on a
   few percent of the pixels at load 700". No time lost.
7. **A masks coverage sampler must test G, not A.** Baked AO (A) can be 0 in a fully occluded texel; every
   slot's porosity (G) is at least 0.05, so G = 0 marks an unbaked texel, as the pipelines' own coverage
   test does. A-based sampling false-flagged approved LOD0 triangles. About 3 minutes. Every LOD1 triangle
   of the 16 pieces samples a covered texel (staged GLB and masks PNG) except windmill's 4 rotor timber
   triangles, which were already uncovered in the approved build (as are 26 of its LOD0 triangles).
8. **Worktree hook refusals: 2** (a `cd` plus subshell bootstrap line, and a `for` loop handing `$p` to
   `python3 -c`). Known rule in SKILL.md; rewritten as plain commands and a script. About 2 minutes.
9. **`make kit-check` hid the first stale-generator warning.** Its warning pass runs libtest with `-q`,
   which prints the progress dot on the same line as the first warning, and `grep '^warning: '` skipped
   that line: with both river mouth centres stale it printed only `river_mouth_l_center`. Fixed in the
   Makefile (`grep -o 'warning: .*'`, both print). About 8 minutes.
10. **The masks KTX2 re-encode changes the look slightly, not only the bytes.** Seven thumbnails changed
    (house_cluster r0/r1/r4/r5, rail_station r3, river_rail_bridge r2, river_road_bridge r1; 75-781 px,
    up to 13 levels) although every LOD0 primitive and masks PNG was byte-identical. With main's masks
    KTX2 swapped back in and this branch's GLBs kept, `make thumbs` reproduced the committed set byte for
    byte, so the cause is the first single-threaded UASTC re-encode (house_pad: 118 of 16,384 BC7 blocks
    differ). `references/project.md` (Tools and pins) says that re-encode changes a file's "KTX2 bytes
    (not its look)"; proposed text: "changes its KTX2 bytes and, on a few BC7 blocks, its decoded texels
    (seven thumbnails moved by up to 13 levels in the LOD1 pass), so re-shoot the thumbnails after it".
    About 15 minutes (two extra thumbs runs).
11. **Joining a part into a road piece after its shading pass changes the other primitives.**
    `house_pad`'s first LOD2 doorsteps joined after the cap collider and re-ran `shade_smooth`,
    `set_sharp_from_angle` and `verge_normals_up`, which split the kerb, asphalt, gravel and hedge vertices
    differently (kerb 41 -> 37 vertices). Fix: join the doorsteps in the LOD loop like LOD0's and LOD1's,
    and drop their faces from the `cap` collider copy instead. About 5 minutes.
12. **A slot may leave LOD2 only with a measured reason.** After the runtime agent's
    `docs/checkpoints/lod1-missing.md` the pass kept each listed slot in LOD2 where its part spans about a
    pixel or more at 400 m (one pixel is about 0.31 m there: 45 degree vertical field of view, 1080 rows)
    and fits the 15% ceiling, and recorded the reason in the pending entry where it does not. Proposed
    skill text (`references/gates.md`, LOD1 / LOD2 row): "A slot may drop out at LOD2 only when its part is
    under about a pixel at 400 m (0.31 m) or LOD2 has no room under its ceiling; say which in the
    provenance notes."
13. **`kit-compress` gave different KTX2 bytes for an identical PNG.** Re-compressing `river_fork_center`
    (masks PNG unchanged) produced a masks BC7/ASTC KTX2 3 bytes smaller (5,034 -> 5,031 B) than the
    pinned encode committed an hour earlier in this pass; level 0's BC7 blocks are identical (`ktx extract
    --raw --level 0`), so the difference sits in a smaller mip or the supercompression. `references/
    project.md` and `gates.md` (Determinism) say KTX2 bytes are identical for identical PNGs at any host
    load; host load was 270-700 during the pass. The committed encode was kept. Filed as a finding, not
    fixed (tool bugs are high priority): compare every level of the two files to find the varying step.
