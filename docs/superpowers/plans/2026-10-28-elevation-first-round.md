# Elevation First Round Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tiles sit at levels 0 to 3, hillside tiles join levels with a slope, roads climb on a Road ramp and run through straight tunnels between two Tunnel portals, all on raised Meadow.

**Architecture:** Height is a per-corner property of a tile definition (relative corner heights 0 or 1) plus one absolute `level` per placed tile; two tiles meet when their shared corners agree in absolute height, and flat edges at different absolute heights meet as a cliff. Hillside, ramp and portal ground is a runtime mesh from one pure height function in `hex-core::slope` (shared by rendering, scatter, grass, walkers and cars), drawn with the existing `meadow_base` materials. Tunnels are derived from the placed portals on every change, never saved.

**Tech Stack:** Rust workspace (hex-core, hex-sim, hex-play-state, hex-play, hex-render, hex-app), Bevy 0.20, WGSL/WESL, TypeScript HUD (`web/ui`), headless Blender generators.

**Spec:** `docs/superpowers/specs/2026-10-28-elevation-design.md` (owner-approved 2026-10-28). PRD §15 P4 records every decision.

## Global Constraints

- Corner convention (pinned for Rust and the Task 10 Python generator): world corner `k` is at angle 30° + 60°·k at radius `HEX_RADIUS` (`prism.rs::corner`), where angle 0 is +x and 90° is +z. Edge `e` faces axial direction `DIRS[e]` (edge 0 east, `axial.rs`) and runs clockwise viewed from +Y from corner `(e + 5) % 6` to corner `e`: `pub fn edge_corners(e: u8) -> (u8, u8)` in `tile.rs`.
- `LEVEL_STEP = 4.0` m (`crates/hex-core/src/axial.rs:7`), `HEX_RADIUS = 16.0` m corner radius (27.71 m flat-to-flat). Levels 0 to 3: `pub const MAX_LEVEL: u8 = 3` in `crates/hex-core/src/world.rs`, and a tile's `level + def.top()` never exceeds it.
- `SAVE_FORMAT_VERSION` stays 3. The v1 wire `Tile.level` byte now carries the level. Every existing save and fixture loads with all tiles at level 0, unchanged.
- Raisable defs (`raisable: true`): `meadow`, `road_straight`, `road_bend_60`, `road_bend_120`, `road_junction`, the 12 `hillside_*`, `road_ramp`, `road_tunnel_portal`. Every other def (ocean, beaches, rivers, rail, forest, oak, farm, wheat, houses, bakery, windmill, bridges) is placeable at level 0 only.
- Only `meadow` may be tunnel cover. A tunnel runs in a straight line between two portals at the same level that face each other.
- Placement never defaults to a cliff. Legal orientations are ordered cliff-free first, then by level, then by rotation.
- A placed hillside keeps its shape. Contradicting placements are refused like any socket mismatch.
- A cliff is allowed only between two `Flat` edges with no lane. Lane edges (roads) must meet at equal absolute height.
- Hillside ground is not a Blender piece. The tunnel portal is a headless Blender generator (owner A42 O1; memory: Blender headless only).
- Scatter and grass are never baked into a piece (owner 2026-10-26).
- Flat worlds are unchanged: `make thumbs` dirties only the new tiles' thumbnails plus the contact sheet.
- New pipelines stay within WebGPU's 16 sampled textures per stage.
- The HUD rules mirror (`web/ui/src/rules.ts`, parity-tested) must agree with Rust after every task, so each task that changes placement updates `rules.ts` and runs `make ui-schema` in the same commit.
- Visual standard (owner, 2026-10-29: "the visuals of this project are amazing and must be held to the established standard"). Runtime hillside ground and cliff columns must be indistinguishable in material, shading, AO and grass/scatter density from the baked Meadow beside them. The portal must match its owner-approved reference and the approved kit's quality (bridges, walls and hedges, the well). Every visible change gets in-engine review shots beside the existing look, and the owner sees them before the merge to main. A green gate is not a visual pass.
- Every task ends green on `make checkall` (redirect to a file and read the real exit code, never pipe). The last task runs `make checkall-ci`. Any bridge or catalog change runs `make ui-schema`.
- Sub-agents run no `kanban` command. GPU captures and perf runs happen only inside windows the orchestrator books.

## Review Focus

1. **A non-raisable tile beside raised ground.** A river placed beside a level-1 meadow is offered level 0 only. Its flat grass edge to the meadow is a cliff, but its river edge against a cliff is refused. Task 2 test `non_raisable_defs_only_offer_level_zero`.
2. **Undo leaves a tunnel without cover.** Undoing the cover meadow placement leaves the tunnel incomplete, and the sim stops routing cars into it the same frame. Task 6 test `undo_of_cover_breaks_the_route`.
3. **Picking raised cells at a shallow camera angle.** The hovered cell is the raised tile the cursor is over, not the level-0 cell behind it on the y = 0 plane. Task 8 test `pick_hits_the_raised_top_first`.
4. **Old saves and flat worlds.** Every fixture in `crates/hex-core/tests/fixtures/save/` decodes with every tile at level 0 and re-encodes byte-identically. Task 2 test `fixtures_load_at_level_zero`. Thumbnails are checked in Task 11.
5. **Cycling through many options.** R on an empty slot beside a plateau cycles the hillside group's (shape, rotation, level) options in a stable order, cliff-free first, and wraps. Task 2 test `cycle_group_orders_cliff_free_first_and_wraps`.

## Task order and parallelism

1 → 2 → {3, 4} → 5 → 6. 7 needs 2, 4 and 5. 8 needs 7. 9 needs 2 and 5. 10 (the portal asset) can start at once, since it uses only the height function pinned in Task 4. 11 runs last.

---

### Task 1: Corner heights on edges and absolute-height matching

**Files:**
- Modify: `crates/hex-core/src/socket.rs` (`EdgeProfile` :78, `EdgeSocket` :87, `edges_compatible` :300, `heights_compatible` :370)
- Modify: `crates/hex-core/src/catalog.rs` (`placement_meets` :260, `edges_meet` :279, `stop_closes` :286)
- Modify: `crates/hex-core/src/tile.rs` (`TileDef` :328, `validate` :438)
- Test: in-file `#[cfg(test)]` modules of those files

**Interfaces:**
- Produces:
  - `pub enum EdgeProfile { #[default] Flat, Rise, Fall }`. `Rise` means the edge's clockwise-start corner is at `height` and its end corner at `height + 1`. `Fall` is the reverse. The old `Cliff`/`Ramp` variants go away, because no data uses them.
  - `impl EdgeSocket { pub fn corners(&self) -> (u8, u8) }` returns (start, end) relative heights in clockwise order.
  - `pub enum HeightMeet { Seamless, Cliff, Refused }` and `pub fn heights_meet(a: &EdgeSocket, a_level: u8, b: &EdgeSocket, b_level: u8) -> HeightMeet`. Seamless means a's absolute (start, end) equals b's absolute (end, start). Cliff means both are Flat, both have no lane, and their absolute heights differ. Everything else is Refused.
  - `SocketTable::edges_compatible(&self, a: &EdgeSocket, a_level: u8, b: &EdgeSocket, b_level: u8) -> bool`, and `heights_meet != Refused`.
  - `pub fn edge_corners(e: u8) -> (u8, u8)` returns `((e + 5) % 6, e)`, the (start, end) corner indices of edge `e`.
  - `TileDef.raisable: bool` (`#[serde(default)]`). `TileDef::corners(&self, rotation: u8) -> [u8; 6]` gives relative corner heights in world corner order, with edge `e`'s `corners()` = (corner `(e+5)%6`, corner `e`). `TileDef::top(&self) -> u8` is the maximum corner. `TileDef::is_sloped(&self) -> bool` is true when any corner differs.
  - `TileCatalog::placement_meets(&self, def, rotation, level, w, neighbour, nrot, nlevel) -> bool` and `edges_meet(&self, a, ra, la, w, b, rb, lb) -> bool`, which return the old answer when both levels are 0 and all edges are Flat at height 0.
  - `TileCatalog::meet_kind(&self, a, ra, la, w, b, rb, lb) -> HeightMeet`, used by Task 2 to rank cliffs.

- [ ] **Step 1: Write the failing tests** in `socket.rs`:
  - `flat_equal_absolute_is_seamless`: Flat h=1 at level 0 against Flat h=0 at level 1 gives `Seamless`.
  - `flat_unequal_is_cliff_without_lanes`: Grass Flat h=0 at level 1 against Grass Flat h=0 at level 0 gives `Cliff`.
  - `lane_edges_never_cliff`: two Road edges with the builtin lane, Flat, levels 1 and 0, give `Refused`.
  - `rise_meets_fall_at_same_base`: Rise h=0 at level 0 against Fall h=0 at level 0 gives `Seamless`. Against Fall at level 1 it gives `Refused`.
  - `slope_against_flat_refused`: Rise against Flat gives `Refused` at every level pair 0..=2.
  - In `tile.rs`, `validate_rejects_broken_corner_chain`: a def whose edge 0 ends at 1 while edge 1 starts at 0 gets a validate error naming the edge.
  - Also `corners_rotate_with_the_tile`: `corners(1)` is `corners(0)` rotated by one.
  - `edge_corners_match_geometry`: for every `e`, the midpoint of `prism::corner(16, start)` and `prism::corner(16, end)` points along `Axial::ORIGIN.neighbor(e).to_world(16)`, and start→end is clockwise viewed from +Y.

- [ ] **Step 2: Run** `cargo test -p hex-core socket tile`. Expected: compile errors or FAIL on the new names.

- [ ] **Step 3: Implement.**
  - Replace `heights_compatible` with `heights_meet`.
  - Thread levels through `edges_compatible`, `placement_meets`, `edges_meet` and `stop_closes`. `stop_closes` copies `height`/`profile` as today.
  - Add the corner chain check to `TileDef::validate`: edge `e`'s end corner equals edge `e+1`'s start corner, corners lie in 0..=1, and `raisable` defs whose edges are not all Flat must be hillside-family ids.
  - Update every caller of the changed signatures (`world.rs`, `save.rs`, `bridge.rs` schema, tests) to pass level 0. This keeps the build green with behavior unchanged.
  - Edit PRD §5.2: `EdgeProfile` becomes `Flat | Rise | Fall` (Rise/Fall: the edge's height climbs or drops one level along it, clockwise), and the height rule becomes the `heights_meet` rule above (owner rule: PRD tracks decisions).

- [ ] **Step 4: Run** `cargo test -p hex-core`, then `make checkall > /tmp/elev-t1.log 2>&1; echo EXIT=$?`. Expected: PASS and `EXIT=0`. `data_validator::every_mvp_edge_is_flat_at_height_zero` still passes, because no data changed.

- [ ] **Step 5: Commit** `feat(core): corner-height edges and absolute-height socket matching (P4)`.

### Task 2: Levels on placed tiles, placement, cycling and saves

**Files:**
- Modify: `crates/hex-core/src/world.rs` (`PlacedTile` :19, `PlaceError` :43, placement methods :240-460)
- Modify: `crates/hex-core/src/save.rs` (`MAX_LEVEL` :37 moves to world.rs, `into_runtime` :612, `Tile::new` :601, `validate_tiles` :410)
- Modify: `crates/hex-app/src/game.rs` (`rotate` :525, `snap_at` :307, `resnap` :331, `ghost` :349, `place_with` :781, `view` :932), `crates/hex-play-state/src/lib.rs` (`PlayTile` :11, `PlayGhost` :18)
- Modify: `data/tiles/meadow.ron`, `road_straight.ron`, `road_bend_60.ron`, `road_bend_120.ron`, `road_junction.ron` (`raisable: true`)
- Modify: `crates/hex-app/src/bridge.rs` (`schema::catalog` :771, `parity_of` :861), `web/ui/src/rules.ts`, `web/ui/test/rules.test.ts`; regenerate `web/ui/src/bridge-schema.json` and `web/ui/test/rules-parity.json` with `make ui-schema`
- Test: `world.rs` and `save.rs` test modules, `crates/hex-core/tests/save_fixtures.rs`, `web/ui/test/rules.test.ts`

**Interfaces:**
- Consumes: Task 1 `placement_meets`/`edges_meet`/`meet_kind` with levels, `TileDef::top`, `raisable`.
- Produces:
  - `PlacedTile { def, rotation, level: u8, seed }`, with `#[serde(default)]` on `level`.
  - `pub const MAX_LEVEL: u8 = 3`.
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)] pub struct Orientation { pub rotation: u8, pub level: u8 }`.
  - `World` methods:
    - `legal_orientations(&self, c, cell, def) -> Vec<Orientation>`: ordered by (has any cliff edge, level, rotation). A def that is not raisable gets level 0 only. Levels satisfy `level + def.top() <= MAX_LEVEL`.
    - `edge_status(&self, c, cell, def, o: Orientation) -> [EdgeStatus; 6]`
    - `fits(&self, c, cell, def, o) -> bool`
    - `auto_orient(&self, c, cell, def) -> Option<Orientation>`: the first legal orientation.
    - `cycle_group(&self, c, cell, members, current: (&str, Orientation), dir) -> Option<(&TileDef, Orientation)>`
    - `place(&mut self, c, cell, def_id, o: Orientation) -> Result<(), PlaceError>`
    - `replace(&mut self, c, cell, def_id, o) -> Result<PlacedTile, ReplaceError>`
    - `cliff_edges(&self, c, cell, def, o) -> [bool; 6]`
  - The old rotation-only methods (`legal_rotations`, `auto_rotate`, `cycle_rotation`, `distinct_legal_rotations`) are replaced by the orientation forms. Callers are updated, and no wrappers are kept.
  - `PlaceError::LevelOutOfRange` covers a non-raisable def above 0, or `level + top > MAX_LEVEL`.
  - `PlayTile { cell, tile, rotation, level }` and `PlayGhost { ..., level }`. `GameState` holds `chosen: Orientation` in place of `rotation`/`chosen_rotation`.
  - HUD mirror: the schema catalog exports per tile `heights: [[start, end] ×6]` (relative corners per edge, world order at rotation 0), `lanes: [bool ×6]`, `raisable` and `top`. `parity_of` records `legal_orientations` as `[[rotation, level], ...]` per (tile, slot). `rules.ts` gains `heightsMeet` (mirrors Task 1) and `legalOrientations(def, needs): {rotation, level}[]`, ordered exactly like Rust, and `needsAt` carries neighbour levels and absolute corner heights.

- [ ] **Step 1: Write the failing tests** in `world.rs`:
  - `meadow_beside_meadow_defaults_to_level_zero`: `legal_orientations(...)[0].level == 0`, and level 1 is also legal, as a cliff.
  - `level_three_is_the_cap`: a level-3 meadow next to a level-3 meadow is legal, and nothing above 3 is offered.
  - `non_raisable_defs_only_offer_level_zero`: `river_straight` beside a level-1 meadow is offered only level-0 orientations. A rotation whose river edge faces the meadow is absent.
  - `road_cannot_cliff`: `road_straight` whose road edge faces a level-1 road end is offered only level 1.
  - `cycle_group_orders_cliff_free_first_and_wraps`: cycling Forward through a two-member group (use `beach_curve` until Task 3 adds the hillside group) visits every legal (def, orientation) once in order and returns to the start.
  - In `save.rs`:
    - `levels_round_trip`: a world with tiles at levels 0..=3 encodes, then decodes equal, including undo/redo history.
    - `level_above_max_is_rejected`: level 4 gives `WorldIssue::LevelOutOfRange`. This replaces `reserved_levels_other_than_zero_are_rejected`.
  - In `save_fixtures.rs`, `fixtures_load_at_level_zero`: every fixture decodes with all levels 0, and `encode(decode(bytes)) == bytes`.
  - Run `make ui-schema`, then update `rules.test.ts` to assert `legalOrientations` against the regenerated parity file.

- [ ] **Step 2: Run** `cargo test -p hex-core world save`. Expected: FAIL or compile errors.

- [ ] **Step 3: Implement.**
  - Levels flow through the save wire (`Tile::new` writes `tile.level`, and `into_runtime` keeps it). `validate_tiles` passes each tile's level to `edges_meet` and checks raisability.
  - In hex-app, `GameState` uses `Orientation`, and `rotate` cycles orientations, so R/Shift+R and Ctrl+wheel now also step levels. `view()` copies `level`.
  - Mark the five defs `raisable: true`.
  - Mirror the rules in `rules.ts` until `make ui-check` passes.

- [ ] **Step 4: Run** `cargo test --workspace`, then `make checkall > /tmp/elev-t2.log 2>&1; echo EXIT=$?`. Expected: PASS and `EXIT=0`. Render is still at y = 0, which Task 7 lifts.

- [ ] **Step 5: Commit** `feat(core): per-tile levels 0-3, orientation placement and save levels (P4)`.

### Task 3: Hillside, Road ramp and Tunnel portal tile definitions

**Files:**
- Create: `scripts/tiles/hillsides.py`, a uv script that writes the 12 hillside RON files deterministically.
- Create: `data/tiles/hillside_<pattern>.ron` ×12, `data/tiles/road_ramp.ron`, `data/tiles/road_tunnel_portal.ron`
- Modify: `crates/hex-core/src/catalog.rs` (`BUILTIN_TILES` :16, `PICKER_GROUPS` :123), `crates/hex-core/src/tile.rs` (`TileDef.tunnel`), `data/pending-pieces.ron` (`road_tunnel_portal`), `crates/hex-core/tests/data_validator.rs` (`F1` :49, `every_mvp_edge_is_flat_at_height_zero` :182)
- Test: `data_validator.rs`, `catalog.rs` tests

**Interfaces:**
- Consumes: Task 1 `EdgeProfile::{Rise, Fall}` and `corners`, Task 2 `raisable`.
- Produces:
  - **Hillside patterns.** The 12 binary corner necklaces of length 6, excluding all-0 and all-1, each named by its lexicographically smallest rotation read as corners c0..c5, for example `hillside_000001` and `hillside_000111`.
    - Edge `e`'s (start, end) corners are `edge_corners(e)`: (0,0) is Flat h0, (1,1) is Flat h1, (0,1) is Rise h0, (1,0) is Fall h0.
    - All edges are `kind: Grass`, `base: "meadow_base"`, and `raisable: true`.
    - The scatter copies meadow's scatter list. No edge pieces.
  - **`road_ramp`:** corners `[1,1,0,0,0,1]` (the straight bank `000111` rotated). Edges 0 and 1 are Flat h1, edge 2 is Fall h0, edges 3 and 4 are Flat h0, and edge 5 is Rise h0. Edges 0 and 3 are `Road` with lane `(0.0, 6.0)`, and the rest are Grass. `center: Some("road_straight_center")`, `networks: [(kind: Road, from: Edge(3), to: Edge(0))]`, `ground: RoadRamp`, `raisable: true`.
  - `TileDef.ground: Ground` (`#[serde(default)] pub enum Ground { #[default] Plain, RoadRamp, Cutting }`) selects the height field in Task 4.
  - **`road_tunnel_portal`:** the same corners and `ground: Cutting` (ruling 2026-10-29: a car-height arch cannot fit under 4 m of cover on the ramp field, so the portal's road stays at the tile's level through a cutting).
    - Edge 3 is a Road lane at h0, and edge 0 is Grass Flat h1.
    - Wiring W1 (ruling 2026-10-29): `center: Some("road_straight_center")` with `road_straight_edge` placed explicitly at edges 0 and 3, and the portal piece `road_tunnel_portal` as a ground hero at the origin (the rail station platform pattern). Take the exact hero/edge-piece RON shape and the `GROUND_BOXES`/`GROUND_PROPS` rows from Task 10's `docs/checkpoints/road_tunnel_portal/integration.patch` (branch `worktree-agent-a97b4ddc8f0f8ddd0`). The portal piece is never draped.
    - `nodes: [(id: "arch", at: [13.3, 0.0])]` and `networks: [(kind: Road, from: Edge(3), to: Node("arch"))]`.
    - `tunnel: Some((node: "arch", edge: 0))`.
  - `TileDef.tunnel: Option<TunnelEnd>` with `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] pub struct TunnelEnd { pub node: String, pub edge: u8 }`. `edge` is the local edge the tunnel runs out of.
  - `PICKER_GROUPS` gains `("hillside", &[...12 ids in pattern order...])`. `road_ramp` and `road_tunnel_portal` are lone entries.
  - The `arch` node position is confirmed against the portal piece in Task 10.

- [ ] **Step 1: Write the failing tests.**
  - In `data_validator.rs`:
    - Extend `F1` with the 14 new ids.
    - Replace `every_mvp_edge_is_flat_at_height_zero` with `only_hillside_family_edges_leave_flat_zero`: non-Flat or h>0 edges appear only on `hillside_*`, `road_ramp` and `road_tunnel_portal`.
    - `hillside_patterns_are_the_twelve_necklaces`: the set of corner patterns at rotation 0 equals the 12 necklaces, and no two are rotations of each other.
  - In `catalog.rs`, `portal_and_ramp_share_the_straight_bank`: both have `corners(0) == [1,1,0,0,0,1]`, the ramp has `ground == Ground::RoadRamp`, the portal `ground == Ground::Cutting`, and the portal's `tunnel.edge == 0`.

- [ ] **Step 2: Run** `cargo test -p hex-core --test data_validator`. Expected: FAIL.

- [ ] **Step 3: Implement.** Write `scripts/tiles/hillsides.py` and run it to produce the RON files. Hand-write the two road tiles. Register them in `BUILTIN_TILES` and `PICKER_GROUPS`, and add `road_tunnel_portal` to `pending-pieces.ron`. `every_tile_rotation_has_a_thumbnail` will fail on the new ids: mark it `#[ignore = "P4 thumbs land in Task 11"]` in this commit only, and Task 11 removes the ignore.

- [ ] **Step 4: Run** `make ui-schema`, `cargo test -p hex-core`, then `make checkall > /tmp/elev-t3.log 2>&1; echo EXIT=$?`. Expected: PASS and `EXIT=0`, and the regenerated schema and parity files (now including the new tiles at every frontier slot) are committed. `rules.ts` already handles Rise/Fall from Task 2.

- [ ] **Step 5: Commit** `feat(data): hillside group, road ramp and tunnel portal tiles (P4)`.

### Task 4: The slope height function and ground mesh

**Files:**
- Create: `crates/hex-core/src/slope.rs`, exported from `lib.rs`
- Test: `slope.rs` test module

**Interfaces:**
- Consumes: Task 1 corner order (world corner `k` is at angle 30° + 60°·k at radius `HEX_RADIUS`, the same as `prism.rs::corner`).
- Produces:
  - `pub fn height_at(corners: [u8; 6], x: f32, z: f32) -> f32`: metres above the tile's level, tile-local and unrotated (corners in local order). It is `4.0 * smoothstep01(w / 4.0)` (ruling 2026-10-29: hillsides meet flat Meadow tangentially, no crease), where `w` is the **Wachspress interpolation** of the corner heights × `LEVEL_STEP` over the regular hexagon: `w_i = A(c_{i-1}, c_i, c_{i+1}) / (A(p, c_{i-1}, c_i) · A(p, c_i, c_{i+1}))`, normalised. Outside the hexagon it clamps to the nearest boundary point. This is pinned so the Task 10 portal generator can match it in Python.
  - `pub fn road_ramp_height_at(corners: [u8; 6], x: f32, z: f32) -> f32`: for `Ground::RoadRamp` tiles (local frame, road along +x from edge 3 at x = −13.8564 to edge 0 at x = +13.8564; exact apothem `HEX_RADIUS·√3/2`). Inside the road corridor `|z| <= 4` it is the linear `4.0 * (x + 13.8564) / 27.7128`. It blends to `height_at` with smoothstep over `4 < |z| < 8`, and outside that it is `height_at`. Edges 0 and 3 are constant in both fields, so the blend joins neighbours exactly.
  - `pub fn cutting_height_at(corners: [u8; 6], x: f32, z: f32) -> f32`: for `Ground::Cutting` (the tunnel portal). Inside `|z| <= 4` it is 0 (the road at the tile's level); it blends to `height_at` with smoothstep over `4 < |z| < 8`; outside that it is `height_at`. Must equal the Task 10 generator's `cut_height_at` (branch `worktree-agent-a97b4ddc8f0f8ddd0`, `scripts/blender/kit/_tunnel_portal.py`): add a test sampling both at the same points if a Python fixture is practical, otherwise copy 8 sample values from the generator into a Rust test.
  - `pub fn field_at(ground: Ground, corners: [u8; 6], x: f32, z: f32) -> f32` dispatches the three. Everything downstream (meshes, drape, scatter, walk, the portal generator) calls `field_at`, never `height_at` directly.
  - `pub fn ground_mesh(ground: Ground, corners: [u8; 6], rings: u32) -> SlopeMesh`:
    - A cap subdivided in `rings` concentric hex rings (use 12), with y from `height_at` and smooth normals from the analytic gradient.
    - Plus a skirt: one wall per edge from the cap boundary down to y = −`LEVEL_STEP`.
    - Cap UVs use meadow's planar cap mapping: `u = (x + 16) / 32`, `v = (16 − z) / 32`. Skirt UVs are the constant (0.005, 0.005).
    - The cap and skirt are separate meshes: `pub struct SlopeMesh { pub cap: PrismMesh, pub skirt: PrismMesh }`.
  - `pub fn cap_tris(ground: Ground, corners: [u8; 6], rings: u32) -> Vec<[[f32; 3]; 3]>`: the cap triangles, for scatter and walkers.

- [ ] **Step 1: Write the failing tests:**
  - `exact_at_corners`: `height_at` at each corner equals `corners[k] * 4.0` within 1e-4.
  - `linear_along_edges`: at t ∈ {0.25, 0.5, 0.75} along edge 1 (corner 0 to corner 1) of `[1,0,0,0,0,0]`, `height_at == 4.0 * (1 - t)` within 1e-3.
  - `flat_is_flat`: all corners equal gives every sampled point exact.
  - `neighbours_join`: for every pair of hillside patterns that meet seamlessly across world edge 0 (Task 1 `heights_meet`), samples along the shared edge agree within 1e-3 after rotating into each tile's frame.
  - `ramp_grade`: `road_ramp_height_at` for `[1,1,0,0,0,1]` along z = 0 has grade 4 / 27.71 within 0.005 everywhere, the cross slope `|dh/dz|` is below 0.01 for `|z| <= 3`, and along edges 0 and 3 it equals `height_at`.
  - `bank_grade_is_bounded`: the maximum grade of `height_at` over a 64×64 interior sample is below 0.3 for every hillside pattern.
  - `mesh_has_no_cracks`: cap boundary vertices lie on the hexagon and match `height_at`, and skirt tops equal cap boundary heights.

- [ ] **Step 2: Run** `cargo test -p hex-core slope`. Expected: FAIL.

- [ ] **Step 3: Implement** `height_at`, `ground_mesh` and `cap_tris`. Write the Wachspress weights with signed triangle areas, and guard points on the boundary, where an area is 0, by taking the edge-linear value.

- [ ] **Step 4: Run** `cargo test -p hex-core slope`, then `make checkall > /tmp/elev-t4.log 2>&1; echo EXIT=$?`. Expected: PASS and `EXIT=0`.

- [ ] **Step 5: Commit** `feat(core): slope height field and hillside ground mesh (P4)`.

### Task 5: Tunnels derived from portals

**Files:**
- Create: `crates/hex-core/src/tunnel.rs`
- Modify: `crates/hex-core/src/world.rs` (`bulldoze` :518, `BulldozeError` :72, `replace` :460, `ReplaceError` :95)
- Test: `tunnel.rs`, `world.rs`

**Interfaces:**
- Consumes: Task 2 `PlacedTile.level` and `Orientation`, Task 3 `TileDef.tunnel`.
- Produces:
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub struct Tunnel { pub a: Axial, pub b: Axial, pub dir: u8, pub cover: Vec<Axial>, pub level: u8 }`. `dir` is the world edge from `a` toward `b`, with `a < b` by `Axial` order.
  - `pub fn tunnels(world: &World, c: &TileCatalog) -> Vec<Tunnel>`, sorted by `a`. For each portal, step across its world tunnel edge while the cell holds `meadow` at `level + 1`. The tunnel is complete when the next cell is a portal at the same level whose world tunnel edge is the opposite edge. Zero cover cells is allowed.
  - `pub fn open_portals(world: &World, c: &TileCatalog) -> Vec<Axial>`: portals that are in no complete tunnel.
  - `pub fn preview(world: &World, c: &TileCatalog, cell: Axial, def: &TileDef, o: Orientation) -> Vec<Axial>`: for a portal ghost, the cells its tunnel would run under (the same walk as `tunnels`, stopping at the first cell that is not level-`o.level + 1` meadow), plus whether it would complete: the return type is `TunnelPreview { pub cells: Vec<Axial>, pub completes: bool }`.
  - `World::bulldoze(&mut self, c: &TileCatalog, cell) -> Result<PlacedTile, BulldozeError>` with `BulldozeError::TunnelCover`.
  - `World::replace` refuses a cover cell with `ReplaceError::TunnelCover`.
  - `undo`/`redo` still run no rule checks.

- [ ] **Step 1: Write the failing tests:**
  - `two_facing_portals_over_meadow_make_a_tunnel`: three level-1 meadows between two level-0 portals facing each other give one `Tunnel` with three cover cells.
  - `lone_portal_is_open`
  - `forest_in_the_line_breaks_it`
  - `different_levels_do_not_join`
  - `cover_cannot_be_bulldozed`: `bulldoze` returns `TunnelCover`. After bulldozing a portal the cover can be bulldozed.
  - `cover_cannot_be_replaced`
  - `preview_reports_cells_and_completion`: a portal ghost facing three cover meadows and a facing portal gives three cells and `completes: true`.

- [ ] **Step 2: Run** `cargo test -p hex-core tunnel world`. Expected: FAIL.

- [ ] **Step 3: Implement.** Update the `bulldoze` callers in hex-app (`game.rs`) and hex-sim to pass the catalog.

- [ ] **Step 4: Run** `make checkall > /tmp/elev-t5.log 2>&1; echo EXIT=$?`. Expected: `EXIT=0`.

- [ ] **Step 5: Commit** `feat(core): tunnels derived from facing portals, cover protected (P4)`.

### Task 6: Cars through tunnels

**Files:**
- Modify: `crates/hex-sim/src/network.rs` (`insert_tile` :222, `remove_tile` :266), `crates/hex-sim/src/sim.rs` (`from_world` :762, `apply_place` :847, `apply_bulldoze` :862, car pose `opacity`), `crates/hex-app/src/game.rs` or `sim.rs` (undo/redo must re-sync the sim the same way placement does)
- Test: `crates/hex-sim` tests

**Interfaces:**
- Consumes: Task 5 `tunnels`, `Tunnel`.
- Produces:
  - `Network::set_tunnels(&mut self, tunnels: &[Tunnel], world: &World, c: &TileCatalog)`: a straight 2D road segment from portal `a`'s `arch` node to portal `b`'s `arch` node, marked hidden. It replaces the previous tunnel set, and a removed tunnel's segment goes.
  - `Simulation` recomputes `tunnels(world, c)` after every place, bulldoze, replace, undo and redo, and calls `set_tunnels`.
  - Car opacity on a hidden segment is 0, ramping linearly from 1 to 0 over the last 3 m before the arch and back after it.
  - An open portal's arch is a dead end, and no route chooses it as a destination.

- [ ] **Step 1: Write the failing tests:**
  - `route_runs_through_a_tunnel`: a road at each portal's low side, and `find_route` from one to the other passes through the hidden segment.
  - `open_portal_has_no_through_route`
  - `cars_are_hidden_inside`: a car pose sampled at the tunnel midpoint has opacity 0, and 5 m outside the arch it has opacity 1.
  - `undo_of_cover_breaks_the_route`: place cover, then undo, and the next tick has no hidden segment and no car on one.

- [ ] **Step 2: Run** `cargo test -p hex-sim`. Expected: FAIL.

- [ ] **Step 3: Implement.**

- [ ] **Step 4: Run** `make checkall > /tmp/elev-t6.log 2>&1; echo EXIT=$?`. Expected: `EXIT=0`.

- [ ] **Step 5: Commit** `feat(sim): cars route through tunnels and vanish inside (P4)`.

### Task 7: Raised tiles, cliff columns and hillside ground in play

**Files:**
- Modify: `crates/hex-play/src/play.rs` (`spawn_tile` :592, `tile_children` :639), `crates/hex-play/src/play/ghost.rs` (:740), `crates/hex-play/src/play/rise.rs` (dust :878/:890), `crates/hex-play/src/play/scatter.rs` (`ground_pieces` :1473, `ground_tris` :2478), `crates/hex-play/src/play/grass.rs` (build distance :1144)
- Create: `crates/hex-play/src/play/slope_ground.rs`, which owns the runtime ground and column meshes and the drape.
- Test: `crates/hex-play` unit tests. Visual check via `make shots-piece`-style capture is in Task 11.

**Interfaces:**
- Consumes: Task 2 `PlayTile.level`, Task 4 `ground_mesh`/`cap_tris`/`height_at`, `TileDef::is_sloped`/`corners`, Task 5 `tunnel::preview`.
- Produces:
  - The tile root is at y = `level as f32 * LEVEL_STEP`, and so is the ghost root. Rise dust is offset by the same amount.
  - `pub fn column_mesh(depth: f32) -> Mesh`: a hex wall below the baked skirt, from y = −`LEVEL_STEP` to −`LEVEL_STEP` − `depth`, tile-local, outward normals, constant UV. It is spawned under every root with `level > 0` (depth = `level * LEVEL_STEP`) with the `meadow_base_skirt` slot's `KitMaterial`, so every cliff and outer face reaches the level-0 floor.
  - For `def.is_sloped()`, the base piece is replaced by two runtime parts built from `ground_mesh(def.ground, def.corners(0), 12)`. The cap uses the `meadow_base_cap` slot material, with masks UVs per Task 4, and the skirt uses the `meadow_base_skirt` slot material. Pre-check first: confirm `chunks.rs::settle` and `gpu_meta` (:553) accept an uncompressed runtime mesh (`final_aabb`, `final_uv_ranges`). If they do, give the parts the same `Lod`/instancing components as other kit parts. If not, spawn them without `Lod` on the per-entity path, and say so in the report.
  - `pub fn drape(mesh: &Mesh, ground: Ground, corners: [u8; 6]) -> Mesh`: adds `field_at(ground, corners, x, z)` to every vertex y and recomputes normals. It is applied to the centre and lane-edge pieces of sloped defs (the road ramp's `road_straight_center`), and is cached per (piece, corners).
  - The portal ghost draws its `tunnel::preview` cells as a faint dotted line at the road's height, hidden under the cover (the spec's placement preview).
  - `ground_tris` for a sloped def returns `cap_tris(def.ground, def.corners(0), 12)` plus the draped road triangles, so scatter, grass and walkers see the slope.

- [ ] **Step 1: Write the failing tests:**
  - `root_sits_at_level`: spawn a `PlayTile` at level 2 in a minimal app, and its root `Transform.translation.y == 8.0`.
  - `column_reaches_the_floor`: `column_mesh(8.0)` has min y = −12.0 and max y = −4.0.
  - `drape_follows_the_field`: drape a flat 3×3 grid with `Ground::RoadRamp` and `[1,1,0,0,0,1]`, and each vertex y equals `field_at` within 1e-4.
  - `sloped_ground_tris_match_the_field`: `PlayScatter::ground` for `hillside_000111` gives `Surface::top(x, z) ≈ field_at` at 16 interior points.

- [ ] **Step 2: Run** `cargo test -p hex-play`. Expected: FAIL.

- [ ] **Step 3: Implement.** Keep the sampled-texture count unchanged: reuse the existing slot materials and add no new texture bindings.

- [ ] **Step 4: Run** `make checkall > /tmp/elev-t7.log 2>&1; echo EXIT=$?`. Expected: `EXIT=0`. Run `make run ARGS="--scenario play"` and place a raised meadow and a hillside to confirm they render. Capture screenshots only inside a booked window (orchestrator).

- [ ] **Step 4b: Visual parity check.** In a booked load-sensitive window, the controller captures, through `make shots-piece`-style scripts into `docs/checkpoints/elevation/visual-parity/`, a raised Meadow and a hillside ring beside flat Meadow at the play camera and a low camera, plus a cliff edge, at the same light. The hillside cap must show the same grass colour, normal detail, AO and blade and scatter density as the flat Meadow, with no seam, shading crease or facet lines at shared edges. Any visible difference is a finding for this task, not a later one. The owner reviews these shots.
- [ ] **Step 5: Commit** `feat(play): raised tiles, cliff columns and runtime hillside ground (P4)`.

### Task 8: Walkers, cars, camera picking and boids over raised ground

**Files:**
- Modify: `crates/hex-play/src/walk.rs` (`CellGround` :280, `WalkGround::build` :942, height queries :1081-1313), `crates/hex-play/src/cars.rs` (`ROAD_SURFACE_Y` use at :444 and :507), `crates/hex-app/src/camera.rs` (`focus` :40, `ground_cell` :139), `crates/hex-play/src/boids/plan.rs` (`SKY_LOW`/`SKY_HIGH` :1009, roosts :476) and `assets/shaders/boids_compute.wgsl:45`
- Test: unit tests in those files

**Interfaces:**
- Consumes: Task 7 root heights and sloped `ground_tris`.
- Produces:
  - `CellGround.y0: f32`, the level height. Every `WalkGround` query adds it. A missing surface is flat at `y0`. The step check makes a cliff a wall.
  - Car y: the existing `g.ride(q, ROAD_SURFACE_Y)` fallback becomes `ROAD_SURFACE_Y + y0` of the cell under the car, so `grounded_transform` pitches on ramps.
  - `pub fn pick_cell(ray: Ray3d, top_of: impl Fn(Axial) -> Option<f32>) -> Option<Axial>` in `camera.rs`. It intersects the planes y = 12, 8, 4 and 0 in that order and returns the first hit whose cell's top equals that plane (placed tiles: `level * 4 + top * 4`; empty cells: plane 0). It replaces the y = 0 `intersect_plane` in `ground_cell`. `focus()` uses the same function.
  - The boids sky band is lifted by the highest placed level × `LEVEL_STEP`, passed as a uniform, and roosts use the root y.

- [ ] **Step 1: Write the failing tests:**
  - `walker_stands_on_a_raised_meadow`: `stand` at a level-1 meadow centre gives y ≈ 4.0.
  - `cliff_blocks_a_step`: a step from a level-0 cell onto an adjacent level-1 cell is refused.
  - `hillside_is_walkable`: a walk across `road_ramp` from edge 3 to edge 0 never exceeds `MAX_STEP` per 0.25 m.
  - `car_pitches_on_a_ramp`: `grounded_transform` on the ramp centreline gives pitch ≈ atan(0.144) within 0.02 rad.
  - `pick_hits_the_raised_top_first`: a ray at 25° elevation aimed at a level-2 cell's top returns that cell, not the level-0 cell its y = 0 hit lies in.

- [ ] **Step 2: Run** `cargo test -p hex-play walk cars boids && cargo test -p hex-app camera`. Expected: FAIL.

- [ ] **Step 3: Implement.**

- [ ] **Step 4: Run** `make checkall > /tmp/elev-t8.log 2>&1; echo EXIT=$?`. Expected: `EXIT=0`.

- [ ] **Step 5: Commit** `feat(play): walkers, cars, picking and boids follow levels and slopes (P4)`.

### Task 9: Bridge, HUD rules mirror and scripts

**Files:**
- Modify: `crates/hex-app/src/bridge.rs` (`CELL_TOP` :164 / `cell_rect` :169, `schema::catalog` :771, `parity_of` :861, `parity_worlds` :911), `crates/hex-app/src/game.rs` (`world_state_json` :871, `state_fields_json` :887, events), script/control commands (`crates/hex-app/src/control.rs`)
- Modify: `web/ui/src/rules.ts`, `web/ui/src/bridge.ts`, HUD readouts (`hud.ts` / `select*.ts`), `web/ui/test/rules.test.ts`
- Create: `docs/checkpoints/elevation/elevation.json` (a parity and shots world: a 3-tile plateau ringed by hillsides, a road ramp, a two-portal tunnel under raised meadow, a river and beach at level 0 beside it)
- Modify: `docs/agent-operability.md` (level fields, script `place` level)
- Generated: `web/ui/src/bridge-schema.json`, `web/ui/test/rules-parity.json` via `make ui-schema`

**Interfaces:**
- Consumes: Task 2 orientations and levels, Task 5 tunnels.
- Produces:
  - Bridge JSON: `PlayTile` and `Ghost` gain `level`. `tilePlaced`, `placementRefused` and `tileReplaced` gain `level`. `ghostRotated` becomes `{rotation, level, valid}`. `cell_rect` uses `CELL_TOP + level * 4`.
  - `parity_worlds` adds the elevation world (the schema and the `rules.ts` mirror already came in Task 2).
  - `rules.ts` `cutOff` mirrors the Task 5 tunnel-cover bulldoze refusal.
  - The HUD readout shows "Level N" beside the rotation readout when N > 0.
  - Script command `place` takes an optional `level` (defaulting to the first legal orientation), documented in `docs/agent-operability.md`.

- [ ] **Step 1: Write the failing tests.** Add the elevation world to `parity_worlds`, run `make ui-schema`, and add a `bridge-sync.test.ts` assertion for the new `level` fields.

- [ ] **Step 2: Run** `node --test web/ui/test/rules.test.ts`. Expected: FAIL until `rules.ts` mirrors Rust.

- [ ] **Step 3: Implement** `rules.ts`, `bridge.ts` and the HUD readout.

- [ ] **Step 4: Run** `make ui-check > /tmp/elev-t9u.log 2>&1; echo EXIT=$?`, then `make checkall > /tmp/elev-t9.log 2>&1; echo EXIT=$?`. Expected: both `EXIT=0`.

- [ ] **Step 5: Commit** `feat(bridge,ui): levels and tunnels in the bridge, HUD rules mirror (P4)`.

### Task 10: Road tunnel portal kit piece (Blender asset agent)

**Files:**
- Create: `scripts/blender/kit/road_tunnel_portal.py` plus a family helper if needed, and the generated `assets/kit/p4/road_tunnel_portal/*`, manifest entry and `assets-source/kit/road_tunnel_portal/` provenance with a pending owner approval
- Return: `docs/checkpoints/road_tunnel_portal/integration.patch` (walk `OBSTACLES`, `CAR_GROUND` for the road into the arch, scatter `FOOTPRINTS`/`NOT_GROUND`, removal from `pending-pieces.ron`, the `arch` node position in `road_tunnel_portal.ron` if it moves)

**Interfaces:**
- Consumes: the Task 4 field `road_ramp_height_at` for corners `[1,1,0,0,0,1]`, re-implemented in the generator from the pinned definition (Wachspress, the corner convention in Global Constraints, the corridor blend). Task 3 tile layout: road lane `(0.0, 6.0)` along local x, entering from edge 3 (x = −13.86) and running up to the arch, with the raised face toward edge 0 (+x). Confirm the `arch` node position against the piece. Blender's y axis is −runtime z (`meadow_base.py:85-87`).
- Produces: piece `road_tunnel_portal` (centre piece, pack `core`).
  - A stone arched portal and retaining wall set into the straight-bank slope, whose base follows `height_at`.
  - The arch opening fits within one level (crown ≤ 3.6 m above the road).
  - A recessed near-black interior.
  - A lamp pair over the arch, carrying an inert emissive night hook per `docs/kit/hooks-contract.md` (the `bush_verge_c` pattern).
  - LOD chain, collider and sockets per the blender-asset skill gates.

- [ ] **Step 1:** The orchestrator dispatches one Blender asset agent (Opus 5.5, xhigh, `isolation: "worktree"`) with the `blender-asset` skill brief, including the integration section (`.claude/skills/blender-asset/references/dispatch.md` part 11) and the booked host windows.
- [ ] **Step 2:** The agent runs `make kit-build kit-compress kit-check kit-provenance` for the piece, all green, and renders review images to `docs/checkpoints/road_tunnel_portal/`.
- [ ] **Step 3:** The orchestrator lands the piece and the verified integration patch, runs `make checkall`, and commits `feat(kit): road tunnel portal piece (P4, pending owner approval)`.

### Task 11: Thumbnails, review shots, web smoke, docs and the CI gate

**Files:**
- Modify: `crates/hex-core/tests/data_validator.rs` (remove the Task 3 `#[ignore]`), `assets/thumbs/*` (via `make thumbs`), `Makefile` (`shots-elevation`), `e2e/elevation.mjs` (create), `PRD.md` §15 P4 boxes, `docs/save-format.md` (levels now live), `handoff.md`

- [ ] **Step 1: Thumbnails.** Inside a booked load-sensitive window, run `caffeinate -d -u make thumbs > /tmp/elev-thumbs.log 2>&1; echo EXIT=$?`. Expected: `EXIT=0`. `git status assets/thumbs` shows only the 14 new tiles' files, the manifest and the contact sheet. Any other dirtied thumbnail is a regression to fix before going on. Remove the `#[ignore]`, and `cargo test -p hex-core --test data_validator` passes.
- [ ] **Step 2: Review shots.** Add `make shots-elevation`, which captures `docs/checkpoints/elevation/elevation.json` at an overview, a low camera along the ramp, a car entering a portal, and a cliff edge, all into `docs/checkpoints/elevation/`. Run it in the booked window, and check that no frame is black.
- [ ] **Step 3: Web smoke.** In `e2e/elevation.mjs` (Playwright GPU Chromium, after `make build-web`), drive the bridge to build the elevation world by script commands. Assert from `window.hexWorld` state that the levels, a complete tunnel, and a car with opacity 0 inside it all appear within 60 s. Run `node e2e/elevation.mjs dist`, and expect exit 0.
- [ ] **Step 4: Perf check.** In a booked MiniMac window, run `e2e/perf-close-camera.mjs` on `world2066` with main and this branch, load-guarded. The flat world's mean must not get slower beyond the paired spread. Record the numbers in `docs/perf/elevation-2026-10-28/README.md`.
- [ ] **Step 5: Docs.** Update the PRD §15 P4 checkboxes for what landed and what is deferred, `docs/save-format.md` (the level byte is live, levels 0..=3) and `handoff.md`.
- [ ] **Step 6: Gate.** Run `make checkall-ci > /tmp/elev-ci.log 2>&1; echo EXIT=$?`. Expected: `EXIT=0`.
- [ ] **Step 7: Commit** `feat(p4): elevation first round — thumbs, review shots, web smoke, docs`, then `make build-web` after the merge to main (owner rule).
