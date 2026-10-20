# fence_paddock friction log

## 1. One piece or modular segments (decision, 0 min lost)
- **Snag.** The brief offered either one closed paddock or modular `fence_rail_straight` / `fence_rail_corner` segments placed by the tile. Play places every hero at y = 0 (`play.rs:629`, `walk.rs:560`), and meadow_base's relief reaches -0.25 m, so a straight segment placed by the tile floats or sinks along its run.
- **Fix.** One piece authored in the tile frame and placed as a hero at `(0, 0)`, yaw 0. The mesh follows meadow_base's `relief()` (falloff included, because the fence line sits 1.6 m inside the border, within the 2 m falloff band). One slot gives one draw per LOD, against one per segment instance.
- **Skill change.** Proposed for `project.md`, "Budgets": "A fence, hedge or wall that follows the ground is one prop at the tile centre carrying the meadow relief, not segments: heroes stand at y = 0. Copy `relief()` with its falloff (`meadow_base.py` runs on import and cannot be imported; `_wheat.meadow_relief` drops the falloff and is only valid more than 2 m inside the border)."

## 2. A thin multi-part obstacle against a convex-hull walker (integration, 0 min lost)
- **Snag.** `walk.rs` hulls all of a piece's `<id>_collider` vertices into one convex polygon (`PieceShape::from_meshes`), and `scatter::footprints_match_the_kit` forces a `FOOTPRINTS` disc to cover the collider's whole reach. A paddock as one convex footprint would block the entire south of the tile, animals included.
- **Fix.** The collider is six disjoint boxes, one per run. The runtime changes go in the report, for the orchestrator to land: per-connected-component hulls in walk, and polyline exclusions in scatter and grass.
- **Skill change.** Proposed for `pitfalls.md`, "Look-dev and engine shots" or a new "Runtime couplings" item: "A prop that is not a convex blob (a fence, a wall) needs per-component collider hulls in `walk.rs` and line exclusions in scatter and grass; build its collider as disjoint boxes and flag the runtime change before the build."

## 3. Shade-side shot reads near-black (expected, 5 min)
- **Snag.** The yard face of the north run (facing runtime -z, away from the kit sun at (200, 400, 150)) measured 13-15/255 in engine.
- **Fix.** None to the asset. The round bale's shade side in the same frame measured 14, and the barn's sun-averted long wall 11-21. The sheet quotes the comparison (pitfalls.md, "A white face turned away from the kit sun reads dark slate").
- **Skill change.** None needed: the pitfall already covers it.

## 4. Worktree hook refused a `for v in hero close` loop with a variable in a Blender argument (2 min)
- **Fix.** Ran each still as its own plain command.
- **Skill change.** None: SKILL.md "Worktree sessions" already says loops pass only when every command in them is literal.

## 5. LOD2 had no posts, so Force LOD 2 lost the fence (defect fix, 2026-10-20, about 25 min to diagnose)
- **Snag.** Another agent saw the paddock fence vanish in every Force LOD 2 shot of the farm. The LOD2 branch built only the top and middle rails (one three-facet prism per run) and the leaf as a 0.9 m slab: 6 runs x 2 rails x 6 + 6 = 78 tris, no posts. Two 0.1 m rails read as faint pale lines at play distance and a grey board at the gate. The GLB, the meshopt decode and Force LOD were fine: decoded LOD2 had all 78 tris, and LOD2 drew on both paths (kit scenario per-entity parts, play's instanced settled parts).
- **Fix.** LOD2 keeps every other post of each run from its start (so all five corners and both gate posts) as an open three-sided prism, a little thicker than LOD0's, plus the same rails and the leaf as its top bar: 168 tris (ceiling floor(0.15 x 1172) = 175). LOD0, LOD1 and the collider are identical on decoded values (`docs/checkpoints/fence-lod2/compare_lods.mjs`).
- **Skill change.** Proposed for `pitfalls.md` (LODs): "A thin multi-member prop (fence, railing, gate) keeps the members that carry its silhouette at every LOD: the uprights, not only the long runs. Rails alone at LOD2 read as faint lines and the prop disappears under Force LOD 2. Thin members may be thickened a little at LOD2 to hold coverage. Check every LOD of such a prop with `{"cmd":"debug","force_lod":"2"}` at play distance before review."

## 6. Window screenshots all black; `photo_export` renders (15 min)
- **Snag.** Every native window `screenshot` came back all black (16,717 B at 1280x800, 25,867 B at 1600x1000) on three runs at host load about 300, under `caffeinate -u -d`, with the display awake and the session unlocked; other agents' `hex-world` windows were open at the time. `{"cmd":"photo_export","scale":1,"name":...}` in the same runs rendered every frame, because it draws the game camera offscreen.
- **Fix.** The checkpoint script shoots with `photo_export` and checks each PNG's mean.
- **Skill change.** Proposed for `pitfalls.md` ("Native shots can come back black"): "When window screenshots stay black after a retry, shoot with `photo_export` (scale 1) instead: it renders offscreen, so a covered window or an unpresented swap chain does not blank it (2026-10-20: three black runs, every export rendered)."

## 7. The bootstrap-cloned release binary crashed (3 min)
- **Snag.** `target/release/hex-world` cloned from main by `worktree-bootstrap.sh` quit on its first frame with a wgpu validation error (`pbr_opaque_mesh_pipeline`, group 3 binding 100: buffer size 64 over `min_binding_size` 48): main's binary was older than its tree.
- **Fix.** `cargo build -p hex-app --release` (2 min 10 s) before shooting.
- **Skill change.** Proposed for `stages.md` or the worktree bootstrap note: "Rebuild the release binary after bootstrap before any native shot; the cloned one can predate main's shaders."

## 8. The Force LOD census reads zero in play (note, 0 min)
- **Snag.** In play, a `dump_state` 20 frames after a Force LOD 2 `photo_export` read `state().debug.lod.by_lod` and `tris_by_lod` as `[0,0,0]` while the export showed the farm drawn. `debug.rs` `count_lods` queries `(&Lod, &ViewVisibility, &Mesh3d)`, and the instanced settled kit draws (`chunks.rs`, `KitSlotDraw` entities) carry neither `Lod` nor `Mesh3d`; `KitSlot.tris` ("for the census/report") has no reader. So settled kit pieces in play are likely left out of the census; not confirmed with a dump outside a photo export.
- **Fix.** None here (runtime, out of this piece's scope); reported to the orchestrator.
- **Skill change.** None until the census counts instanced chunks.
