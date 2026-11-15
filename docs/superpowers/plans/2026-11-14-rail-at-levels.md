# Rail at Levels Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rail runs on raised ground, through hills in rail tunnels, and over low ground on viaducts.

**Architecture:** Three phases in dependency order. Phase 1 reuses the road-at-levels machinery (the `raisable` flag, the per-cell level height `CellGround::y0`, `heights_meet`) for rail with no new assets. Phase 2 generalizes `hex-core/src/tunnel.rs` from road-only to per-network-kind portals and adds a rail portal piece. Phase 3 adds a viaduct tile whose two rail edges stand above the tile's own level.

**Tech Stack:** Rust workspace (hex-core, hex-sim, hex-play, hex-app), Bevy 0.20.1, RON tile data, headless Blender generators, `make checkall-ci`.

**Spec:** `docs/superpowers/specs/2026-11-14-rail-at-levels-design.md`

## Global Constraints

- One level is `LEVEL_STEP` = 4.0 m, levels 0..=3. The rail top is `RAIL_TOP_Y` = 0.46 m over the tile's ground. The steam engine is 4.66 m tall and 2.7 m wide.
- Rail never changes level inside a tile. No rail ramps.
- Save format v3 stays unchanged. Flat worlds stay byte-identical in saves, captures and thumbnails.
- A tunnel joins two portals of the same network kind. Cover is `meadow` one level higher (`COVER_DEF`).
- RL-Q1 O2 portal: head wall about 2 m above a one-level hill under a turf mound, face about 6 m inside edge 0, arch crown about 5.0 m, wall top about 5.9 m.
- Viaduct: one design for all heights (pier height scales), `rail_viaduct_l1`, `_l2`, `_l3`. No walking on the deck. Piers are obstacles.
- Station, level crossing and river rail bridge stay level 0 only (not `raisable`).
- Never edit an approved Blender helper (`_tunnel_portal.py`, `_rail.py`). New family files set module globals.
- Blender is headless only. The owner approves each piece in engine before it merges. Never record an owner approval.
- Every merge to `main` is followed by `make build-web`. Commit after each verified task. Push only when `make checkall-ci` is green and the batch is a significant addition (standing approval), then watch CI.
- Sub-agents never run `kanban`. Book GPU and MiniMac windows through the orchestrator.

## Review Focus

- A raised rail tile beside a level-0 rail tile: the seam must be refused (a rail edge never meets rail on another level), not drawn as a cliff.
- A rail end at a raised tile's open edge: the buffer stop and the cliff rules must hold at level 1..3.
- Bulldozing one cover meadow or one portal of a rail tunnel: the train graph loses the link and trains on it re-route or stop, no panic.
- A tunnel line with a road portal at one end and a rail portal at the other: no tunnel forms.
- Loading a save made before this change: unchanged, byte for byte re-saved.
- A train mid-tunnel when a portal is removed.

---

## Phase 1: Rail on raised ground

### Task 1: Rail tiles are raisable and take a raised ring

**Files:**
- Modify: `data/tiles/rail_straight.ron`, `rail_bend_60.ron`, `rail_bend_120.ron`, `rail_junction.ron` (add `raisable: true`)
- Test: `crates/hex-core/src/world.rs` (tests module, beside `deferred_land_tiles_take_the_raised_level_of_their_ring`)

**Interfaces:**
- Consumes: `raised_ring(c, roads)`, `HOLLOW`, `cliffs`, `World::legal_orientations`, `World::auto_orient` (all in the world.rs tests module).
- Produces: nothing new. Later tasks rely on these four defs being `raisable`.

- [ ] **Step 1: Write the failing tests** in world.rs tests:
  - `rail_tiles_are_raisable`: for `rail_straight`, `rail_bend_60`, `rail_bend_120`, `rail_junction`, assert `def(&c, id).raisable` and `level_allowed(d, 3)`.
  - `a_raised_rail_straight_meets_raised_rail_not_ground_rail`: build a level-1 `rail_straight` at cell A, then place a `rail_straight` aligned to it at the neighbouring cell at level 1 (`place` is `Ok`) and at level 0 (`place` is `Err`, and `fits` is false). The seam must not appear as a cliff in `cliff_edges` because a Rail edge is laned.
  - `station_level_crossing_and_rail_bridge_stay_level_zero`: `rail_station`, `rail_road_crossing`, `river_rail_bridge` are not `raisable`.
- [ ] **Step 2: Run** `cargo test -p hex-core rail_tiles_are_raisable a_raised_rail station_level_crossing` and confirm the first two fail.
- [ ] **Step 3: Add `raisable: true`** to the four RON files, after the `props` block as `road_straight.ron` does.
- [ ] **Step 4: Run** the same tests plus `cargo test -p hex-core` and confirm pass. If `data_validator` flags the raisable flag on a non-hillside tile with non-flat edges, none of the four has any, so no change is expected.
- [ ] **Step 5: Run `make ui-schema`** and commit any regenerated `web/ui/src/bridge-schema.json` and `rules-parity.json` together with the data change. Run `make ui-check`.
- [ ] **Step 6: Commit** `feat(p4): rail straight, bends and junction are raisable`.

### Task 2: Buffer stops and rail ends at raised levels

**Files:**
- Test: `crates/hex-core/src/rail_end.rs` tests (and world.rs if the placement test needs the world helpers)

**Interfaces:**
- Consumes: Task 1's raisable rail defs; the existing buffer-stop rule in `rail_end.rs` / `TileCatalog::edges_meet`.
- Produces: none.

- [ ] **Step 1: Write the failing test** `a_raised_rail_end_beside_raised_grass_takes_a_buffer_stop_and_a_ground_end_is_a_cliff`: a level-1 `rail_straight` with its open edge facing a level-1 `meadow` places with no cliff and registers a stopped end; the same tile at level 1 facing a level-0 meadow reports a cliff on that edge, and `auto_orient` for that meadow keeps level 0 first (mirrors `stopped_rail_end_beside_a_raised_meadow_is_a_cliff`).
- [ ] **Step 2: Run it.** If it passes at once, keep it as the pin and move on. If it fails, fix `rail_end.rs` so the stopped-end rule compares absolute edge heights through `heights_meet`.
- [ ] **Step 3: Run** `cargo test -p hex-core` and commit `test(p4): raised rail ends and buffer stops`.

### Task 3: A train routes across raised rail

**Files:**
- Test: `crates/hex-sim/tests/rail.rs`
- Possibly modify: `crates/hex-sim/src/rail.rs` only if the test shows the link graph reads level-0 assumptions.

**Interfaces:**
- Consumes: the rail test harness in `crates/hex-sim/tests/common/mod.rs` and the existing loop/station fixtures in `rail.rs` and `station.rs`.
- Produces: none.

- [ ] **Step 1: Write the failing test** `a_train_runs_a_raised_loop`: build a world whose station sits at level 0 and whose track climbs by no ramp (a station is level 0 only), so use the closest existing fixture: a closed loop of `rail_straight` and `rail_bend_60` placed at level 1 with a train spawned on it by the same helper the other rail tests use. Assert the train advances at least 100 m over 400 ticks and never stops with an error.
- [ ] **Step 2: Run** `cargo test -p hex-sim a_train_runs_a_raised_loop`. Expected: pass already (the sim is 2D), which pins the claim. If it fails, fix the stitching in `network.rs` / `rail.rs` that rejects a level above 0.
- [ ] **Step 3: Commit** `test(sim): trains run raised rail`.

### Task 4: Trains stand at their rail's level

**Files:**
- Modify: `crates/hex-play/src/trains.rs` (`stock_transform`, `sync`)
- Modify: `crates/hex-play/src/walk.rs` (`WalkGround`, add an accessor)
- Test: `crates/hex-play/src/trains.rs` tests module

**Interfaces:**
- Produces:
  - `WalkGround::level_y(&self, p: Vec2) -> f32`: the `y0` of the cell under `p`, or `0.0` off the drawn tiles.
  - `pub fn placed(ground: Option<&WalkGround>, p: &StockPose) -> Transform` in trains.rs: `stock_transform` raised by `level_y` of `Vec2::from(p.pos)`. `stock_transform` stays as the level-0 form and is what `placed` calls with the raised y.
- Consumes: `cars::placed` as the pattern, `RAIL_TOP_Y`, `hex_core::LEVEL_STEP`.

- [ ] **Step 1: Write the failing tests** in trains.rs:
  - `a_train_on_a_level_one_tile_stands_one_step_higher`: build a `WalkGround` for a level-1 `rail_straight` (use the same builder the cars tests use for a raised road, `cars.rs` near the "raised village tiles" test), place a `StockPose` at the tile centre, assert `placed(Some(&g), &pose).translation.y` is within 1e-3 of `LEVEL_STEP + RAIL_TOP_Y`.
  - `placed_without_ground_is_the_level_zero_transform`: `placed(None, &pose) == stock_transform(&pose)`.
  - Keep the existing `stock_transform` tests unchanged (flat worlds must not move).
- [ ] **Step 2: Run** them and confirm they fail to compile or fail.
- [ ] **Step 3: Implement** `WalkGround::level_y` and `trains::placed`, then use `placed` in `sync` (add `ground: Option<Res<WalkGround>>` as `cars.rs` does around line 502, and use it for both the existing-entity update and the spawn).
- [ ] **Step 4: Run** `cargo test -p hex-play trains` and `cargo test -p hex-play` and confirm pass.
- [ ] **Step 5: Commit** `feat(p4): trains ride the level of their rail`.

### Task 5: Evidence, gate and merge for phase 1

**Files:**
- Create: `e2e/elevation-rail.mjs` (copy the structure of `e2e/elevation.mjs`) and a shots target `make shots-elevation-rail` if `make shots-elevation` cannot take a scenario argument.
- Modify: `docs/agent-operability.md` only if a new flag or script command is added.

- [ ] **Step 1:** Write a play scenario: a raised level-1 meadow platform carrying a short rail loop with one train, captured at the play and overview cameras. Book a GPU window first (`kanban schedule book --resource macdaddy`, load guard 15).
- [ ] **Step 2:** Run `make checkall-ci`. Expected: green. Confirm a flat-world capture is unchanged (`make thumbs` dirties no thumbnail).
- [ ] **Step 3:** Review the shots myself for the train riding level on the raised track. Commit evidence to `docs/checkpoints/rail-levels/raised/`.
- [ ] **Step 4:** `make build-web`, record the PRD P4 note for raised rail (a line under the P4 decisions), update `handoff.md`, commit `feat(p4): rail on raised ground`. Push under the standing approval, then watch CI.

---

## Phase 2: Rail tunnels

### Task 6: Portals pair by network kind

**Files:**
- Modify: `crates/hex-core/src/tunnel.rs`, `crates/hex-core/src/tile.rs` (`TunnelDef`), `data/tiles/road_tunnel_portal.ron` only if the kind cannot be derived
- Test: `crates/hex-core/src/tunnel.rs` tests module (helpers `catalog()`, `tile()`)

**Interfaces:**
- Produces: `pub fn portal_kind(def: &TileDef) -> Option<NetworkKind>`: the kind of the network whose end is the portal's tunnel node. `faces(...)` requires `portal_kind(far) == portal_kind(this)`. `Tunnel` gains `pub kind: NetworkKind`.
- Consumes: `TileDef::tunnel`, `TileDef::networks`, `NetworkKind::{Road, Rail}`.

- [ ] **Step 1: Write the failing tests:** `a_road_portal_does_not_pair_with_a_rail_portal` (two facing portals, one road one rail, cover between, `tunnels()` is empty), `two_rail_portals_form_a_rail_tunnel_of_kind_rail`, and the existing road tests unchanged. The rail portal def does not exist until Task 7, so build the rail portal for this test by cloning the road portal def in the test and swapping its network kind.
- [ ] **Step 2: Run, see them fail. Step 3: Implement. Step 4: Run `cargo test -p hex-core tunnel`.**
- [ ] **Step 5: Commit** `feat(core): tunnels pair portals of the same network kind`.

### Task 7: The `rail_tunnel_portal` tile (data only, placeholder art)

**Files:**
- Create: `data/tiles/rail_tunnel_portal.ron` (copy `road_tunnel_portal.ron`: same ramp-bank edges `110001` shape, `ground: Cutting`, `raisable: true`; the Rail edge on edge 3 with `lane` 3.5 m, `networks: [(kind: Rail, from: Edge(3), to: Node("arch"))]`, the arch node at about `(9.0, 0.0)`, 6 m inside edge 0 per RL-Q1 O2, `tunnel: Some((node: "arch", edge: 0))`, hero `rail_tunnel_portal`)
- Modify: `crates/hex-core/src/tile.rs` hillside_family check to include `rail_tunnel_portal`; `crates/hex-core/tests/data_validator.rs` expectations
- Test: `crates/hex-core/src/tunnel.rs` tests

**Interfaces:**
- Consumes: Task 6's `portal_kind`.
- Produces: tile id `rail_tunnel_portal`, hero piece id `rail_tunnel_portal` (built in Task 9).

- [ ] **Step 1: Failing test** `two_rail_tunnel_portals_under_one_cover_meadow_form_a_tunnel` using the real def. **Step 2:** run, fail on missing def. **Step 3:** add the RON and the family check. **Step 4:** run `cargo test -p hex-core`. The validator will demand a thumbnail and a kit piece, so keep the tile out of the inventory until Task 9 lands the piece (the way `data/pending-pieces.ron` holds a tile whose piece is not built; follow the entry format already there). **Step 5: Commit.**

### Task 8: The simulation links facing rail portals and trains fade

**Files:**
- Modify: `crates/hex-sim/src/network.rs` (`set_tunnels` currently road only), `crates/hex-sim/src/sim.rs` (`sync_tunnels`), `crates/hex-sim/src/rail.rs` (the train fade opacity, mirroring `Car::opacity_at`), `crates/hex-play/src/trains.rs` (apply opacity and hide a fully transparent vehicle as `cars.rs` `placeholder_visibility` does)
- Test: `crates/hex-sim/tests/tunnels.rs`, `crates/hex-sim/tests/rail.rs`

**Interfaces:**
- Produces: `Simulation::sync_tunnels` sets the rail graph's hidden links too; `Train::opacity_at(frac: f32) -> f32` (0 inside a tunnel, linear ramp over `TUNNEL_FADE` metres like cars); a `StockPose.opacity: f32` field the renderer reads.
- Consumes: Task 6 `Tunnel::kind`, `network.rs` `TUNNEL_SEGMENT`.

- [ ] **Step 1: Failing tests:** `a_train_crosses_a_rail_tunnel` (two level-1-cover rail tunnels world, train enters one portal, exits the other, never errors), `a_train_is_invisible_inside_the_tunnel_and_fades_at_both_arches`, and `bulldozing_the_cover_removes_the_rail_link_and_the_train_stops_without_panic`.
- [ ] **Step 2: Run, see failures. Step 3: Implement** by generalizing `set_tunnels` over `NetworkKind` (the road graph and the rail graph each take the tunnels of their kind) and adding the train opacity from the tunnel stretch exactly as `Car::tunnel_stretches` does. **Step 4:** `cargo test -p hex-sim -p hex-play`. **Step 5: Commit** `feat(sim): trains run through rail tunnels and fade at the arches`.

### Task 9: Rail portal piece (Blender agent) — dispatched in parallel with Tasks 6 to 8

**Files:**
- Create (agent): `scripts/blender/kit/_tunnel_portal_rail.py`, `scripts/blender/kit/rail_tunnel_portal.py`, `assets-source/kit/rail_tunnel_portal/`, `assets/kit/p1/rail_tunnel_portal/`, `docs/checkpoints/rail_tunnel_portal/` (review sheet, `integration.patch`, `integration.md`), the manifest entry, thumbnail
- Brief: `.claude/skills/blender-asset/references/dispatch.md` (all 11 parts), including: references `assets-source/refs/rail_tunnel_portal/ref-2.jpg` and `ref-3.jpg` with `approval.json`; the RL-Q1 O2 numbers from the Global Constraints; the clearance check against the engine's eave corners and the carriage roofs (`_train` sweep); pack `village`; LOD0 at most 1,500 tris, LOD1 at most 50%, LOD2 at most 15%; KTX2 class `piece` (at most 1,500,000 B) with a 512 px png fallback albedo beside a 1024 px KTX2; shared-island stone method; a ground hero at the origin; and the integration section (walker step at the face, `GROUND_PROPS`, the fern slot in `NOT_GROUND`, the wall-top turf slot, `FOOTPRINT_OBSTACLES`).

- [ ] **Step 1:** Orchestrator books the Blender/GPU windows, then dispatches one Opus 5.5 xhigh agent with `isolation: "worktree"` from the main checkout, telling it to `git merge --ff-only main` first.
- [ ] **Step 2:** When it returns, verify its claims myself (`make kit-build kit-compress kit-check kit-provenance`), inspect the review sheet, and land the `integration.patch`. Record the owner approval entry as pending only.
- [ ] **Step 3:** In-engine shots to the owner queue (`docs/review/2026-11-14-owner-queue.md`); owner approves before merge. Then `make thumbs`, `make checkall-ci`, `make build-web`, commit `feat(p4): rail tunnels`.

---

## Phase 3: Viaducts

### Task 10: Gate branch and edge-height support for a raised-edge tile (failing test first)

**Files:**
- Modify: `crates/hex-play/tests/kit_gate.rs` (additive branch for a piece whose declared rail sockets stand above the border ring), `crates/hex-core/src/tile.rs` (edge `height` validation, currently capped at 0..=1 per corner), `crates/hex-core/src/world.rs` (`level_allowed`, `top()`), `crates/hex-core/src/slope.rs` if edge heights feed ground meshes
- Test: `crates/hex-core/src/tile.rs` and `world.rs` tests

**Interfaces:**
- Produces: a tile may declare `Rail` edges with `height: N` (N in 1..=3) on a flat profile while its other edges are at height 0. `TileDef::top()` returns the maximum edge height. Placement at tile level L requires neighbours' absolute edge heights to match (`heights_meet`).

- [ ] **Step 1: Failing tests:** `a_tile_with_two_rail_edges_at_height_two_validates`, `a_viaduct_tile_between_two_level_two_rail_tiles_places`, `the_same_tile_beside_level_one_rail_is_refused`, and the kit-gate test `viaduct_deck_ends_may_stand_above_the_border_ring` which asserts the existing ring contract still fails for an ordinary piece with a raised edge.
- [ ] **Step 2: Run, see failures. Step 3: Implement.** Keep the change additive: the ring check is relaxed only for sockets of kind Rail declared at height N in the tile's own RON. **Step 4:** run `cargo test --workspace` and `make kit-check`. **Step 5: Commit** `feat(core): a tile's rail edges may stand above its own level`.

### Task 11: Viaduct tiles, trains over the deck, walker and scatter rows

**Files:**
- Create: `data/tiles/rail_viaduct_l1.ron`, `_l2.ron`, `_l3.ron` (rail on edges 0 and 3 at height 1, 2, 3; other edges Grass at 0; `center: Some("rail_viaduct_l<N>")`; `raisable: false`; the network `Edge(0)` to `Edge(3)`)
- Modify: `crates/hex-play/src/walk.rs` (`OBSTACLES` pier rows; deck not ground), `crates/hex-play/src/play/scatter.rs` (`FOOTPRINTS`, `NOT_GROUND`), `crates/hex-play/src/trains.rs` and `walk.rs` (a train over a viaduct tile stands at the deck level: `WalkGround` gains `rail_y(p: Vec2) -> Option<f32>` for cells whose tile has a raised rail section, and `placed` prefers it over `level_y`)
- Test: `crates/hex-play/src/trains.rs`, `walk.rs`, `scatter.rs` tests; `crates/hex-sim/tests/rail.rs`

**Interfaces:**
- Consumes: Task 10's edge-height support and Task 4's `placed`.
- Produces: tile ids `rail_viaduct_l1..l3`; piece ids `rail_viaduct_l1..l3` (Task 12).

- [ ] **Step 1: Failing tests:** `a_train_crosses_a_viaduct_at_the_deck_level` (y = N x LEVEL_STEP + RAIL_TOP_Y at the tile centre for each N), `a_walker_passes_under_the_arches_but_not_through_a_pier`, `a_walker_on_the_raised_neighbour_cannot_step_onto_the_deck`, `scatter_keeps_out_of_piers_and_off_the_deck`.
- [ ] **Step 2-4:** implement, run `cargo test --workspace`. Keep the tiles in `data/pending-pieces.ron` until Task 12 lands. **Step 5: Commit.**

### Task 12: Viaduct pieces (Blender agent), dispatched in parallel with Tasks 10 and 11

**Files:** `scripts/blender/kit/_rail_viaduct.py`, `rail_viaduct.py` (one generator, three pieces), review sheets, `integration.patch`.
- Brief as Task 9, with: references `assets-source/refs/rail_viaduct/ref-2.jpg` (three tall arches on tapering piers) and `ref-4.jpg` (granite detail, parapets, string course); pier height scaled by deck level 1, 2, 3; deck ends flush at the rim at the deck height; kind `center` in pack `village`; LOD0 at most 4,000 tris; faces sample a world-space tileable granite block (`STONE_UV` route) or shared islands, with masks baked or analytic; the track uses `_rail.track` and `_rail.section()` at the deck height; colliders for the piers.
- [ ] Verify, land the patch, owner in-engine review, `make thumbs`, `make checkall-ci`, `make build-web`, commit `feat(p4): rail viaducts`.

### Task 13: Close-out

- [ ] Update PRD §15 P4 (rail at levels landed), `handoff.md`, `docs/agent-operability.md` if flags changed, the owner queue with pending approvals, and the vault if a lesson came up. Confirm the kanban items and reconcile their states. Remove every agent worktree and branch.
