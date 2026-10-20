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
