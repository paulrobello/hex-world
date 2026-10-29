# Mega Tiles First Round (P4 massifs) Implementation Plan

Status: **draft, pending owner approval of the spec and A52.** Nothing is executed until both are approved.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One tile type, the massif (`massif_granite`), covers a centre cell and its six-cell ring. It places, turns, undoes, saves and bulldozes as one tile, meets its neighbours with today's per-edge rules, and reads as a granite landmark on Meadow ground.

**Architecture:** A `TileDef` with a `footprint: Some(Ring1)` is a mega tile. `World` keeps one `PlacedTile` per tile at its anchor (the centre) and a derived `members` map (ring cell to anchor), so every footprint cell is occupied and answers with the anchor's tile. Every per-edge rule runs through one socket lookup, `World::facing`, which returns the socket the occupant of a cell shows across a given world edge. A mega tile shows its 18 outer sockets and nothing across its interior edges. The HUD learns where a massif fits from an engine-published state field instead of mirroring the 7-cell rules. Presentation is one spawn root at the centre, first with a placeholder mesh and then with the Blender-built piece.

**Tech Stack:** Rust workspace (hex-core, hex-sim, hex-play-state, hex-play, hex-render, hex-app), Bevy 0.20, TypeScript HUD (`web/ui`), headless Blender generators (`blender-asset` skill).

**Spec:** `docs/superpowers/specs/2026-10-29-mega-tiles-design.md` (draft). Decisions marked provisional there are A52's recommended options (`docs/review/2026-10-26-open-questions.html`). This plan assumes A52 Q1 to Q6 all resolve to O1. If the owner picks any O2, the plan is rewritten before execution.

## Corrections to the spec this plan makes

These are flagged for the owner with A52. The spec is not edited.

1. **The footprint has 18 outer edges, not 12.** Seven hexes have 42 edges, and the 12 interior edges (6 centre-to-ring, 6 ring-to-ring) are each shared by two hexes, so 42 − 2·12 = 18 edges lie on the perimeter. They face the **12** cells at distance 2 from the centre: each of the 6 corner cells (`2·DIRS[i]`) touches the footprint at one edge, and each of the 6 side cells (`DIRS[i] + DIRS[i+1]`) touches it at two. The spec and A52 Q1 O1 say "12 outer edges", which conflates the 18 edges with the 12 neighbour cells. Everything below uses 18 edges and 12 neighbour cells.
2. **A massif centre is never a frontier cell.** A frontier cell touches a placed tile, and that tile would lie in the centre's own ring. A legal centre is an empty cell whose ring is empty and whose ring touches the world, so it is two steps from the world. Goal 6's line "the inventory shows the massif when any frontier cell can take it" is read as: a frontier cell can take the massif when some legal footprint contains it. The engine publishes the legal centres. The HUD lists the massif for a slot covered by one, and the ghost centres on that footprint (Task 6).
3. **`mega_fits` is a published state field, not a command.** Bridge commands are fire-and-forget through `control::INBOX` and return nothing, and the synchronous exports read statics, not the `World`. The engine therefore publishes `mega_slots: [{ q, r, tile, orientations: [{rotation, level}] }]` with the other per-selection fields (`frontier`, `slot_levels`, `inventory`). The spec leaves the query's name and shape to the builder.

## Global Constraints

- **Geometry pin** (Rust, TypeScript and the Blender generator must agree). Edge `e` faces axial direction `DIRS[e]` (edge 0 east, clockwise viewed from +Y, `axial.rs`). The footprint of anchor `a` is `[a, a.neighbor(0), …, a.neighbor(5)]`. Ring cell `i` (at `a.neighbor(i)`) exposes local edges `(i+5)%6`, `i` and `(i+1)%6` to the outside, and its edges `(i+2)%6`, `(i+3)%6` and `(i+4)%6` face other members. Outer edge index `j` in `0..18` is ring cell `j / 3`, edge `(j/3 + 5 + j%3) % 6`, which lists them clockwise from ring cell 0. Rotation `k` maps ring cell `i` to `(i+k)%6` and outer index `j` to `(j + 3k) % 18`.
- The massif's 18 outer sockets are `Grass`, `Flat`, height 0, with no lane (A52 Q1 O1, Q2 O1). Its six `edges` (the centre's, facing members only) are the same. The validator enforces no lane, network, node, stop, parking or tunnel on a footprint tile.
- `massif_granite` is `raisable: true` and has `look_period: Some(6)`. Raisable matches "flat at the tile's own level, the way a raised Meadow's edges do" (Goal 2). Since `corners()` are all 0, `top()` is 0, so levels 0..=3 are allowed, and a raised massif gets cliff columns on its 18 outer edges like a raised Meadow. Without `look_period: Some(6)`, all-Grass sockets would make `same_orientation` collapse the six turns into one, so R would do nothing and thumbs would render one image.
- `SAVE_FORMAT_VERSION` stays 3. The wire `Tile` is unchanged. Decode expands footprints and validates them. Every existing fixture loads unchanged, and `encode(decode(bytes)) == bytes` holds for every current-version fixture.
- A footprint is atomic: no undo, redo, bulldoze, replace or load can leave a world holding part of one. `World::members` is derived and never saved (`#[serde(skip)]`), and it is rebuilt after every decode.
- Replacing a massif with a single tile, or the reverse, is refused (A52 Q5 O1). Massif-to-massif re-orientation goes through the same 18-edge check.
- No tunnel runs under or through a massif (A52 Q3 O1).
- The HUD never re-derives the 7-cell rules (A52 Q6 O1). It does mirror one fact, the socket a massif member shows a neighbour (the geometry pin plus the exported outer sockets), so that single-tile rules next to a massif stay exact.
- Visual standard (owner, 2026-10-29, memory `visual-standard.md`). The massif's ring ground is indistinguishable from the baked Meadow beside it in grass colour, shading, AO, blade density and scatter density. The piece matches its owner-approved reference and the kit's quality. The owner sees in-engine shots before the merge to main. A green gate is not a visual pass.
- Scatter and grass are never baked into the piece (owner 2026-10-26). Blender work is headless only (memory `blender-headless-only.md`).
- Every task ends green on `make checkall`. Redirect to a file and read the real exit code (`make checkall > /tmp/mega-tN.log 2>&1; echo EXIT=$?`), never pipe. The last task runs `make checkall-ci`. Any bridge, catalog or `data/tiles` change runs `make ui-schema` in the same commit.
- Sub-agents run no `kanban` command. GPU captures (thumbs, shots, e2e) and perf runs happen only inside load-sensitive windows the orchestrator books (memory `gpu-captures-load-sensitive.md`). Perf runs on MiniMac with its own baseline.
- PRD tracks decisions (owner 2026-10-21). Task 10 writes the approved A52 answers and the 18-edge correction into PRD §15 P4.

## Review Focus

1. **18 outer edges, 12 neighbour cells.** A side cell that touches the footprint at two edges must meet both. Task 1 test `side_cells_face_two_outer_edges`. Task 2 test `a_neighbour_meeting_two_outer_edges_checks_both`.
2. **Single tiles beside a massif member.** A slot next to a ring cell sees the massif's outer socket, in Rust (`World::facing`) and in the HUD (`tileMap` member expansion). Neither treats the member as empty. Task 2 test `facing_a_member_returns_the_outer_socket`. Task 6 parity world `massif` and test `single_tiles_beside_a_massif_match_hex_core`.
3. **Atomicity.** Undo, redo, bulldoze and load insert or remove all 7 cells together. Task 3 test `undo_redo_never_leave_part_of_a_footprint`. Task 4 test `overlapping_footprints_are_rejected`.
4. **The centre is never a frontier cell.** `legal_orientations` at a frontier cell is empty for the massif, `mega_slots` lists centres two steps out, and the HUD maps a pinned frontier slot to a covering centre. Task 2 test `a_massif_centre_is_two_steps_from_the_world`. Task 6 test `the_massif_is_listed_only_where_a_footprint_covers_the_slot`.
5. **Connectivity through a footprint.** A tile joined to the world only through a massif member stays connected, and bulldozing that massif is refused as a split. Task 3 test `bulldoze_refuses_a_massif_that_holds_the_world_together`.

## Task order and parallelism

1 → 2 → 3 → 4 → 5 → 6. 7 needs 5. 8 needs 7. 9's reference checkpoint (Steps 1 and 2) starts at once, in parallel with Task 1, and its generator needs only the geometry pin. 9's integration needs 7. 10 runs last.

---

### Task 1: Footprint geometry and the `Ring1` tile footprint

**Files:**
- Create: `crates/hex-core/src/footprint.rs` (exported from `lib.rs`)
- Modify: `crates/hex-core/src/tile.rs` (`TileDef` :353, `same_orientation`, `validate`)
- Test: `footprint.rs` and `tile.rs` test modules

**Interfaces:**
- Produces in `footprint.rs`:
  - `pub const OUTER_EDGES: usize = 18;`
  - `pub fn members(anchor: Axial) -> [Axial; 7]`: the anchor, then `anchor.neighbor(0..6)`.
  - `pub fn outer_edge(j: u8) -> (u8, u8)`: local (ring cell, local edge) of outer index `j`, per the geometry pin.
  - `pub fn outer_index(ring: u8, edge: u8) -> Option<u8>`: the inverse, in the local frame. `None` for an edge that faces another member.
  - `pub fn outer_faces(anchor: Axial, rotation: u8) -> [(Axial, u8, u8); 18]`: (member cell, world edge, local outer index) for every outer edge of a tile turned by `rotation`.
  - `pub fn outer_neighbours(anchor: Axial) -> [Axial; 12]`: the cells at distance 2, sorted.
- Produces in `tile.rs`:
  - `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] pub struct Ring1 { pub outer: Vec<EdgeSocket> }`, with the 18 outer sockets in outer-index order. Vec rather than `[EdgeSocket; 18]`, so RON reads a list and the validator reports a wrong count instead of a parse error.
  - `TileDef.footprint: Option<Ring1>` (`#[serde(default)]`). `pub fn is_mega(&self) -> bool`.
  - `pub fn socket_toward(&self, rotation: u8, ring: Option<u8>, w: u8) -> Option<&EdgeSocket>`. `ring` is the world direction of the member from the anchor, or `None` for the anchor cell. A single tile returns `Some(self.edge(w, rotation))` for `None` and `None` for any ring. A mega tile returns `None` for the anchor and for interior edges, and otherwise the outer socket at `outer_index(local_edge(ring, rotation), local_edge(w, rotation))`.
  - `same_orientation(a, b)` also requires equal outer sockets at every world outer index, so a mega tile's six turns stay distinct whenever its look period is 6.

- [ ] **Step 1: Write the failing tests** in `footprint.rs`:
  - `members_are_the_centre_and_its_ring`: `members(a)[0] == a`, and the other six are exactly `a.neighbor(0..6)`.
  - `outer_edges_cover_the_perimeter_once`: the 18 (member cell, world edge) pairs of `outer_faces(a, 0)` are distinct. Each one's neighbour lies outside `members(a)`, and every perimeter edge of the seven cells appears.
  - `side_cells_face_two_outer_edges`: across `outer_faces(a, 0)` the neighbour cells are exactly `outer_neighbours(a)`. The six corner cells appear once and the six side cells twice.
  - `outer_index_inverts_outer_edge`: `outer_index(outer_edge(j)) == Some(j)` for every `j`, and `outer_index(i, (i+3)%6)` is `None`.
  - `rotation_shifts_outer_indices_by_three`: the world (member, edge) of index `j` at rotation `k` equals that of index `(j + 3k) % 18` at rotation 0.
  - In `tile.rs`, `validate_footprint_rules`. A test def with `footprint: Some(Ring1 { outer })`:
    - with 17 outer sockets, gets an issue naming the count.
    - with a `Road` outer socket, gets "footprint edges carry no lane".
    - with a network or tunnel, gets an issue.
    - with all 18 `Grass` sockets and six `Grass` centre edges, passes.
  - `socket_toward_answers_outer_edges_only`: for that def at rotation 2, the anchor and interior edges give `None`, and every outer face from `outer_faces(a, 2)` gives `Some`.
  - `mega_turns_are_distinct_with_look_period_six`: with `look_period: Some(6)`, `same_orientation(0, k)` is false for k in 1..6.

- [ ] **Step 2: Run** `cargo test -p hex-core footprint tile`. Expected: compile errors or FAIL on the new names.

- [ ] **Step 3: Implement.** Validator rules for `footprint: Some`:
  - `outer.len() == 18`.
  - No outer or centre edge carries a lane, and no outer socket is directional.
  - `nodes`, `networks`, `switches`, `stops`, `parking` and `tunnel` are empty or `None`.
  - Every centre `edges` entry is `Grass`, `Flat`, h0.
  - Outer corners are flat (height and profile give one level all round), so `corners()`, `top()` and `is_sloped()` keep their meaning.
  - No data uses `footprint` yet.

- [ ] **Step 4: Run** `cargo test -p hex-core`, then `make checkall > /tmp/mega-t1.log 2>&1; echo EXIT=$?`. Expected: PASS and `EXIT=0`.

- [ ] **Step 5: Commit** `feat(core): mega-tile footprint geometry and Ring1 tile footprint`.

### Task 2: World occupancy and placement across the footprint

**Files:**
- Modify: `crates/hex-core/src/world.rs` (`World` :200, `tile` :257, `frontier` :262, `is_slot` :273, `edge_status` :280, `cliff_edges` :315, `fits` :341, `ordered_with_cliff` :367, `legal_orientations` :386, `inventory` :399, `group_orientations` :425, `clashes` :520, `place` :537, `pair_edge_pieces` :760)
- Modify: `crates/hex-core/src/catalog.rs` (`placement_meets` :334, `edges_meet` :359, `meet_kind` :376, `stop_closes`)
- Test: `world.rs` and `catalog.rs` test modules. Tests build a catalog of the builtin tiles plus a test massif def (`fn massif_catalog() -> TileCatalog`, in `world.rs` tests, `pub(crate)` for later tasks), until Task 5 adds the data file.

**Interfaces:**
- Consumes: Task 1 `footprint::*`, `TileDef::socket_toward`, `is_mega`.
- Produces in `catalog.rs`:
  - `#[derive(Clone, Copy, Debug)] pub struct Facing<'a> { pub def: &'a TileDef, pub socket: &'a EdgeSocket, pub local: Option<u8>, pub level: u8 }`. `local` is the tile's local edge for a single tile (the rail stop mask needs it), and `None` for a mega outer face.
  - `TileCatalog::facing_meets(&self, mine: Facing, theirs: Facing) -> bool`: the placement-order rule (`edges_compatible`, or `stop_closes` either side).
  - `TileCatalog::facings_meet(&self, a: Facing, b: Facing) -> bool`: the symmetric form.
  - `placement_meets`, `edges_meet` and `meet_kind` keep their signatures and delegate. For two `Facing`s, `meet_kind` is `heights_meet(a.socket, a.level, b.socket, b.level)`.
- Produces in `world.rs`:
  - `World.members: BTreeMap<Axial, Axial>` (ring cell to anchor, `#[serde(skip)]`). `pub fn members(&self) -> &BTreeMap<Axial, Axial>`.
  - `pub fn anchor_of(&self, cell: Axial) -> Option<Axial>`: the cell itself for an anchor, its anchor for a member, otherwise `None`.
  - `pub fn occupied(&self, cell: Axial) -> bool`.
  - `pub fn tile(&self, cell)` answers member cells with the anchor's `PlacedTile`. `tiles()` stays the anchor map, so its 144 callers keep iterating each tile once. The spec's `anchors()` is not added, because `tiles()` already is that.
  - `pub fn facing<'c>(&self, catalog: &'c TileCatalog, cell: Axial, w: u8) -> Option<Facing<'c>>`: the occupant of `cell`'s socket across world edge `w`. `None` when the cell is empty, its def is unknown, or the edge is interior to a footprint.
  - `pub struct Face { pub cell: Axial, pub w: u8, pub status: EdgeStatus, pub cliff: bool }` and `pub fn faces(&self, catalog, cell, def, o) -> Vec<Face>`: every outward face of `def` ghosted with its anchor at `cell` (6 for a single tile, 18 for a mega tile) against the occupant across it.
  - `pub fn is_slot_for(&self, cell: Axial, def: &TileDef) -> bool`. For a single tile it is `is_slot`. For a mega tile it requires all 7 members empty and at least one of `outer_neighbours(cell)` occupied.
  - `fits`, `legal_orientations`, `inventory`, `auto_orient`, `group_orientations`, `cycle_group`, `clashes` and `place` go through `is_slot_for` and `faces`, with `cell` meaning the anchor. For a single tile, `edge_status` and `cliff_edges` keep their `[_; 6]` results, built from `faces`. A mega def gets its statuses only from `faces`.
  - `pub fn mega_slots(&self, catalog: &TileCatalog, def: &TileDef) -> Vec<(Axial, Vec<Orientation>)>`: every legal centre for `def`, sorted, with its legal orientations.
  - `pub fn mega_centre_for(&self, catalog, cell: Axial, def: &TileDef) -> Option<Axial>`: `cell` itself when it is a legal centre, else the first legal centre among `cell.neighbor(0..6)` in edge order.
  - `PlaceError::Occupied` covers any occupied member. `place` inserts the anchor and its 6 members, and records one `Action::Place` at the anchor.
  - `frontier` and `is_slot` test `occupied`, not `tiles.contains_key`.
  - `pair_edge_pieces` reads neighbour sockets through `facing`.

- [ ] **Step 1: Write the failing tests** in `world.rs`:
  - `a_massif_occupies_all_seven_cells`: place the massif at a legal centre. `occupied` and `tile` answer for all 7 members, `anchor_of(member) == Some(centre)`, `tiles().len()` grows by 1, and `place` on any member is `Err(Occupied)`.
  - `a_massif_centre_is_two_steps_from_the_world`: on `World::new`, `legal_orientations(origin.neighbor(0), massif)` is empty, and `mega_slots` lists exactly the empty cells at distance 2 from the origin whose ring is empty. All six rotations are legal at level 0.
  - `facing_a_member_returns_the_outer_socket`: a `Facing` across each of the 18 outer faces is `Some` with `kind == Grass`, and across an interior edge it is `None`.
  - `a_neighbour_meeting_two_outer_edges_checks_both`: a hand-built catalog def that is Grass on one edge and Road on the next is refused at a side cell in every rotation that puts the Road edge on either outer edge.
  - `frontier_rings_the_footprint`: after placing the massif, `frontier()` holds no member, and it holds every free cell of `outer_neighbours(centre)`.
  - `a_road_cannot_face_a_massif`: `road_straight` at a side cell has no orientation with a road edge toward the footprint.
  - `a_raised_massif_meets_meadow_as_a_cliff`: at level 1 beside a level-0 meadow, `faces` reports `cliff` on the facing outer edges, and the cliff-free level-0 orientation is ordered first.
  - `massif_seed_comes_from_the_anchor`: `tile(member).seed == cell_seed(world_seed, centre)`.
  - `mega_centre_for_maps_a_frontier_slot_to_a_covering_centre`: for a frontier cell next to a legal centre, the result is that centre, and `None` where no footprint covers the slot.
  - `existing_orderings_are_unchanged`: the current `world.rs` suite passes untouched. This is the regression bar for the single-tile path.
  - In `catalog.rs`, `facing_rules_match_placement_meets`: for every builtin (def, neighbour, w, rotation) pair at levels 0 and 1, `facing_meets` equals `placement_meets`.

- [ ] **Step 2: Run** `cargo test -p hex-core world catalog`. Expected: FAIL.

- [ ] **Step 3: Implement.** `World::new` and `from_parts` leave `members` empty. `from_parts` callers in `tunnel.rs` tests hold single tiles only. Add `pub(crate) fn rebuild_members(&mut self, catalog: &TileCatalog)` for Task 4. In this task it is called by `place`, and by tests that build worlds by hand.

- [ ] **Step 4: Run** `cargo test -p hex-core`, then `make checkall > /tmp/mega-t2.log 2>&1; echo EXIT=$?`. Expected: PASS and `EXIT=0`. No data has a footprint yet, so the bridge parity is unchanged.

- [ ] **Step 5: Commit** `feat(core): footprint occupancy, per-face socket lookup and mega-tile placement`.

### Task 3: Atomic removal, replace refusal, undo/redo and tunnels

**Files:**
- Modify: `crates/hex-core/src/world.rs` (`ReplaceError` :127, `replace_orientations` :577, `replace` :594, `can_bulldoze` :642, `bulldoze` :659, `undo` :688, `redo` :708, `connected_without` :737)
- Modify: callers of `undo`/`redo`: `crates/hex-app/src/game.rs` (:712, :725), the `save.rs` tests (:963-1438) and `crates/hex-core/tests/save_fixtures.rs` (:56-248)
- Modify: `crates/hex-core/src/tunnel.rs` (tests only, unless the test shows a gap)
- Test: `world.rs` and `tunnel.rs` test modules

**Interfaces:**
- Consumes: Task 2 `members`, `occupied`, `anchor_of`, `rebuild_members`.
- Produces:
  - `pub fn undo(&mut self, catalog: &TileCatalog) -> bool` and `pub fn redo(&mut self, catalog: &TileCatalog) -> bool`. The catalog supplies the footprint when a Place is redone or a Bulldoze undone. A def missing from the catalog inserts the anchor alone. That cannot happen in a validated world, and Task 4 rejects it on load.
  - `bulldoze(catalog, cell)` and `can_bulldoze(catalog, cell)` accept any member cell, resolve it to the anchor, and record `Action::Bulldoze` at the anchor.
  - `connected_without(&self, removed: Axial)` floods over occupied cells, skipping every member of the removed tile's footprint. It succeeds when every remaining anchor is reached.
  - `ReplaceError::FootprintMismatch`: replacing a footprint tile with a single tile, or the reverse. `replace` on a member cell resolves to the anchor. `replace_orientations` is empty across footprint kinds.
  - `undo` of a `Replace` swaps only the anchor's `PlacedTile`, because only massif-to-massif replace is allowed and its members are unchanged.

- [ ] **Step 1: Write the failing tests:**
  - `bulldozing_a_member_removes_the_whole_massif`: bulldoze a ring cell. All 7 cells are free, `tiles()` drops by 1, and the action's cell is the anchor.
  - `undo_redo_never_leave_part_of_a_footprint`: place, bulldoze and re-place a massif among single tiles, then undo everything and redo everything. After every step, for every anchor of a mega def, all 6 members map to it, and no member key points at a missing anchor.
  - `bulldoze_refuses_a_massif_that_holds_the_world_together`: a meadow reachable only through a massif member keeps the massif from being bulldozed (`WouldDisconnect`). The meadow itself can be bulldozed.
  - `a_tile_beside_a_member_counts_as_connected`: removing a single tile leaves a world whose other tiles connect only through a massif, and that removal is allowed.
  - `massif_replace_rules`: massif to `meadow` and `meadow` to massif give `Err(FootprintMismatch)`. Massif to massif at another rotation succeeds as one undo step, and the same rotation gives `Unchanged`.
  - In `tunnel.rs`, `no_tunnel_runs_under_a_massif`: two facing portals with a massif footprint on the line between them give no `tunnels()` entry. `preview` from the near portal stops at the first member, with `completes == false`.

- [ ] **Step 2: Run** `cargo test -p hex-core world tunnel`. Expected: FAIL. The tunnel test may already pass, because `tunnel::walk` (:38) stops on any def other than `meadow` once `tile()` answers members. Keep it as the guard either way.

- [ ] **Step 3: Implement.** Update the `undo`/`redo` callers to pass the catalog (`game.rs` passes `&self.catalog`).

- [ ] **Step 4: Run** `cargo test --workspace`, then `make checkall > /tmp/mega-t3.log 2>&1; echo EXIT=$?`. Expected: PASS and `EXIT=0`.

- [ ] **Step 5: Commit** `feat(core): mega tiles bulldoze, undo and redo as one, replace across footprints refused`.

### Task 4: Save decode expands and validates footprints

**Files:**
- Modify: `crates/hex-core/src/save.rs` (`WorldIssue` :121 and its `Display`, `decode` :282, `validate_tiles` :406, `validate_history` :474, `check_step`, `every_world_reached_by_play_round_trips`)
- Modify: `crates/hex-core/src/world.rs` (shared footprint index helpers)
- Create: `crates/hex-core/tests/fixtures/save/v3-massif.hexworld` (written once by an `#[ignore]` writer)
- Modify: `crates/hex-core/tests/save_fixtures.rs`, `docs/save-format.md` (footprints are implied by the def and expanded on decode)
- Test: `save.rs` tests and `save_fixtures.rs`

**Interfaces:**
- Consumes: Task 2 `Facing`, `facing`, `rebuild_members`. Task 3 `undo`/`redo` with the catalog.
- Produces:
  - `pub(crate) fn footprint_index(tiles: &BTreeMap<Axial, PlacedTile>, catalog: &TileCatalog) -> Result<BTreeMap<Axial, Axial>, Axial>`: the member map, or the first cell two footprints (or a footprint and a tile) both claim.
  - `pub(crate) fn facing_in<'c>(tiles, members, catalog: &'c TileCatalog, cell, w) -> Option<Facing<'c>>`: the lookup `World::facing` delegates to, so save validation and play use one rule.
  - `WorldIssue::FootprintOverlap { cell: Axial }`. Member cells go through the same `OutOfRange` check as anchors.
  - `validate_tiles(tiles, catalog)` keeps its signature. It indexes footprints, checks each touching pair once through `facing_in` + `facings_meet` (world edges 0..3 of every occupied cell, interior edges skipped), and floods over occupied cells.
  - `SaveData::decode` runs `save.world.rebuild_members(catalog)` after `validate` succeeds.
  - `validate_history` replays with footprint-aware tiles: `check_step` validates the expanded map at each step.

- [ ] **Step 1: Write the failing tests** in `save.rs`:
  - `massif_world_round_trips`: a world with a massif and single tiles around it, plus undo/redo history, encodes and decodes equal. The decoded `members()` equals the original's, and the wire holds one tile entry per anchor.
  - `overlapping_footprints_are_rejected`: two massif anchors one cell apart give `InvalidWorld(FootprintOverlap { .. })`. So does a single tile on a member cell.
  - `member_out_of_range_is_rejected`: a massif anchor at `q = CELL_LIMIT` gives `OutOfRange` for the member past the limit.
  - `a_bad_neighbour_of_a_member_is_rejected`: a road end facing an outer edge gives `Incompatible`.
  - `massif_history_replays`: a tampered history that undoes a massif Place while a tile stands on a member gives `InvalidHistory`.
  - Extend `every_world_reached_by_play_round_trips` with a massif branch. On one step in 10, place the massif at a random entry of `mega_slots`, and bulldoze or undo it later. Its round-trip tally includes worlds that hold a massif (assert the count > 0).
  - The `fuzz_*` tests stay green with the massif in the catalog.
  - In `save_fixtures.rs`:
    - `write_v3_massif_fixture` (`#[ignore]`, run once) writes `v3-massif.hexworld`: a level-0 massif, two meadows and a forest touching it, and history with a massif Place and a Bulldoze.
    - `v3_massif_expands_on_load`: every member maps to the anchor, the history undoes and redoes the massif whole, and `encode(decode(bytes)) == bytes`.
    - `fixtures_load_at_level_zero` stays green, which is why the fixture is at level 0.

- [ ] **Step 2: Run** `cargo test -p hex-core save` and `cargo test -p hex-core --test save_fixtures`. Expected: FAIL.

- [ ] **Step 3: Implement.** The fixture writer uses the Task 2 test catalog until Task 5 lands the real def. Write the fixture after Task 5's data file exists, so the fixture names `massif_granite`. If Task 4 runs before Task 5, it lands the decode code and tests on the test catalog, and Task 5 Step 3 writes the fixture.

- [ ] **Step 4: Run** `cargo test -p hex-core`, then `make checkall > /tmp/mega-t4.log 2>&1; echo EXIT=$?`. Expected: PASS and `EXIT=0`.

- [ ] **Step 5: Commit** `feat(save): mega-tile footprints expand and validate on decode (format unchanged)`.

### Task 5: The massif tile, the sim, game state and the bridge schema

**Files:**
- Create: `data/tiles/massif_granite.ron`
- Modify: `crates/hex-core/src/catalog.rs` (`BUILTIN_TILES` :16, a lone picker entry), `data/pending-pieces.ron` (`massif_granite`), `crates/hex-core/tests/data_validator.rs` (`F1` :49 and its count, the footprint checks, `every_tile_rotation_has_a_thumbnail` :411)
- Modify: `crates/hex-app/src/game.rs` (`snap_at` :377, `resnap` :398, `ghost` :424, `rotate` :611, `place_with` :898, `state_fields_json` :1007, `view` :1057)
- Modify: `crates/hex-app/src/bridge.rs` (`schema::catalog` :780, `parity_of` :902), `crates/hex-play-state/src/lib.rs` (`PlayGhost` :20)
- Modify: `web/ui/src/rules.ts` (`TileDef` :24 gains `footprint`, `legalOrientations` :343 returns `[]` for footprint defs), `web/ui/src/bridge.ts` (`PlayState`, `Ghost` and the key lists)
- Generated: `web/ui/src/bridge-schema.json`, `web/ui/test/rules-parity.json` (`make ui-schema`)
- Test: `data_validator.rs`, `crates/hex-sim/tests/` (new `massif.rs`), `game.rs` tests, `web/ui/test/rules.test.ts`, `web/ui/test/bridge-sync.test.ts`

**Interfaces:**
- Consumes: Tasks 1 to 4.
- Produces:
  - `massif_granite.ron`:
    - `biome: Meadow`, `base: "massif_granite"` (one piece: the ground and the peak), `center: None`.
    - Six `Grass` centre edges, and `footprint: Some((outer: [18 × (kind: Grass)]))`.
    - `raisable: true`, `look_period: Some(6)`, and the standard rise timeline.
    - Unlock as the owner's A52 answer sets it, default unlocked.
    - Picker: its own entry (A52 Q4 O1), placed after the hillside group.
  - `GameState`:
    - With a footprint tile selected, the ghost anchors at `world.mega_centre_for(target slot)`. `snap_at` returns that centre's first legal orientation, and R cycles the six rotations and their levels at the centre (`cycle_group`).
    - `hovered_kind` is `"slot"` on a legal centre when a footprint tile is selected, where today it reads `"empty"`.
  - `PlayGhost.outer: Option<[EdgeStatus; 18]>`: per outer index, `Some` for a footprint ghost. `edges` stays for single tiles.
  - The ghost state JSON gains `"outer_edges": [18] | null` and `"footprint": [{q, r} ×7] | null`.
  - State field `mega_slots: [{ q, r, tile, orientations: [{rotation, level}] }]`: for every footprint def in the catalog, `World::mega_slots`, recomputed when the world changes. The bridge.ts `PlayState` type and the bridge-sync key list gain it.
  - Schema catalog per tile: `footprint: boolean`, and for footprint tiles `outer: [{kind, height, profile} ×18]`. The schema also exports the geometry pin as `footprint_outer: [[ring, edge] ×18]` (`outer_edge(j)`), so TypeScript reads it rather than re-deriving it.
  - `parity_of` records, per world, `mega_slots` as the engine computes them.
  - `rules.ts` `legalOrientations(def, needs)` returns `[]` when `def.footprint`, which matches Rust at every frontier slot.
  - hex-sim: `Network::insert_tile` (`network.rs:243`) and the rail `insert_tile` (`rail.rs:721`) add nothing for a footprint def. That needs no code change if the test passes.

- [ ] **Step 1: Write the failing tests:**
  - `data_validator.rs`:
    - Add `massif_granite` to `F1` with six Grass kinds.
    - `footprint_tiles_have_eighteen_plain_outer_edges`: every footprint tile has 18 outer `Grass` Flat h0 sockets, no networks, and `look_period == Some(6)`.
    - Mark `every_tile_rotation_has_a_thumbnail` `#[ignore = "massif thumbs land in Task 10"]` in this commit only.
  - `crates/hex-sim/tests/massif.rs`, `massif_adds_no_nodes`: building the sim from a world with a massif beside a road loop gives the same node and link counts as without it.
  - `game.rs`:
    - `selecting_the_massif_anchors_the_ghost_at_a_covering_centre`: hover a frontier slot. The ghost's cell is `mega_centre_for(slot)`, `valid` is true, and `place` puts the anchor there.
    - `rotate_turns_the_massif_through_six_rotations`: six R presses visit rotations 0..6 once at level 0 and wrap.
    - `state_publishes_mega_slots`: `state_fields_json()["mega_slots"]` equals `World::mega_slots`, one entry per legal centre.
  - `rules.test.ts`: the existing parity test (`legal orientations match hex-core …`) passes with the massif in the catalog.
  - `bridge-sync.test.ts`: the new `mega_slots`, `outer_edges` and `footprint` keys appear.

- [ ] **Step 2: Run** `cargo test -p hex-core --test data_validator`, `cargo test -p hex-sim --test massif` and `cargo test -p hex-app game`. Expected: FAIL.

- [ ] **Step 3: Implement.** Write the data file and register it. Wire `GameState`. Run `make ui-schema` and update `rules.ts`/`bridge.ts` until `make ui-check` passes. Write the Task 4 fixture now if it was deferred. Play draws the massif as a meadow prism at the anchor until Task 7. That is acceptable between commits, and nothing ships before Task 10.

- [ ] **Step 4: Run** `make ui-check > /tmp/mega-t5u.log 2>&1; echo EXIT=$?`, then `make checkall > /tmp/mega-t5.log 2>&1; echo EXIT=$?`. Expected: both `EXIT=0`.

- [ ] **Step 5: Commit** `feat(data,app): massif_granite tile, mega-tile ghost and mega_slots state`.

### Task 6: HUD: single-tile rules beside a massif, the picker and the ghost

**Files:**
- Modify: `web/ui/src/rules.ts` (`tileMap` :229, `needsAt` :250, `isSlot` :279, `fitEntry`, `tunnelCover` :605, `cutOff` :634)
- Modify: `web/ui/src/sheet-controller.ts` (:111, :132), `web/ui/src/hud.ts` (the ghost readout at :573-578, the entry fit at :448), `web/ui/src/words.ts` (massif refusal and fit text)
- Modify: `crates/hex-app/src/bridge.rs` (`parity_worlds` :958: add a `massif` world, a massif with a meadow, a forest and a river mouth around it, plus one tile joined only through a member)
- Generated: `web/ui/test/rules-parity.json` (`make ui-schema`)
- Test: `web/ui/test/rules.test.ts`, `web/ui/test/picker.test.ts` (the Tiles sheet listing)

**Interfaces:**
- Consumes: Task 5 schema (`footprint`, `outer`, `footprint_outer`) and state (`mega_slots`, ghost `footprint`).
- Produces:
  - `tileMap(tiles)` expands each footprint `PlayTile` into its 7 cells. A member entry carries the anchor and its ring direction. `facingAt(map, cell, w)` returns the outer socket from `outer` and `footprint_outer`, or `null` across an interior edge. `needsAt` and `isSlot` read through it, so single-tile rules next to a massif equal Rust's `World::facing`.
  - `export function megaFit(slot: Cell, megaSlots: MegaSlot[], tile: string): { centre: Cell; orientations: Orientation[] } | null`. Its order matches Rust `mega_centre_for`: the slot itself, then `neighbor(slot, 0..6)`. This is geometry over engine output, not a rule mirror.
  - `fitEntry` for a footprint entry uses `megaFit`. The Tiles sheet lists the massif for a slot `megaFit` covers. Choosing it while a slot is pinned moves the ghost to the covering centre (the engine does this via `select_tile`). The frontier fit count (:132) counts it the same way.
  - `cutOff(tiles, target)` treats a footprint as one node: the target's whole footprint is removed, and the flood runs over expanded cells. `tunnelCover` ignores members.

- [ ] **Step 1: Write the failing tests** in `rules.test.ts`:
  - `single tiles beside a massif match hex-core`: in the `massif` parity world, `legalOrientations` equals the parity file at every frontier slot, including slots next to members. Before `tileMap` expansion this fails, which is the point of the test.
  - `the massif is listed only where a footprint covers the slot`: for every frontier slot of every parity world, `megaFit(slot, parity.mega_slots, "massif_granite") !== null` exactly when some engine `mega_slots` centre is the slot or its neighbour. It is never listed where the engine refuses, and never missing where the engine accepts.
  - `cutOff treats a massif as one tile`: bulldozing the massif in the `massif` world cuts off the tile joined only through it, and bulldozing that tile cuts off nothing.
  - `facingAt follows the outer edge pin`: for each of the 18 entries of `footprint_outer` at rotation 2, the expanded map returns the outer socket, and for interior edges it returns `null`.

- [ ] **Step 2: Run** `node --test web/ui/test/rules.test.ts`. Expected: FAIL.

- [ ] **Step 3: Implement.** The readout for a massif ghost says "Rotation N of 6" like any hero tile. A refused massif says why in words: "needs seven free cells beside the world".

- [ ] **Step 4: Run** `make ui-schema`, `make ui-check > /tmp/mega-t6u.log 2>&1; echo EXIT=$?`, then `make checkall > /tmp/mega-t6.log 2>&1; echo EXIT=$?`. Expected: both `EXIT=0`.

- [ ] **Step 5: Commit** `feat(ui): HUD sees massif members, lists the massif from engine mega_slots`.

### Task 7: Play and render presentation with a placeholder mesh

**Files:**
- Modify: `crates/hex-play-state/src/lib.rs` (`PlayView` :37, `index_tiles`, `tile_at`)
- Modify: `crates/hex-play/src/play.rs` (`spawn_tile` :614, `spawn_column` :649, `tile_children` :686)
- Modify: `crates/hex-play/src/play/rise.rs` (`particles` :865, `cloud` :897, `dust`)
- Modify: `crates/hex-play/src/play/affordances.rs` (`lift` :271, `cut_off` :614, `ring_parts` :875, `ghost_parts` :932, `bulldoze_parts` :1100, `tile_sig` :1189)
- Modify: `crates/hex-play/src/play/ghost.rs` (`sync` :539, `ghost_pieces` :395, `runtime_ground` :448)
- Modify: `crates/hex-play/src/play/grass.rs` (`layout` :709), `crates/hex-play/src/play/scatter.rs` (`layout` :2314)
- Modify: `crates/hex-app/src/game.rs` (`view` :1057 writes `members`)
- Test: in-file test modules of those files, `crates/hex-render/src/chunks.rs` tests

**Interfaces:**
- Consumes: Task 5 `PlayGhost.outer`, `World::members`.
- Produces:
  - `PlayView.members: HashMap<Axial, Axial>` (ring cell to anchor, copied from `World::members` by `view()`). `tile_at(cell)` falls back through it, so `lift`, the affordances and slope lookups see members.
  - `PlayView.mega_slots: Vec<Axial>`: the legal centres for the selected footprint tile, or empty. The slot ring (`ring_parts`) outlines these centres' footprints instead of single frontier hexes while a footprint tile is selected.
  - `pub fn footprint_outline(anchor: Axial, y: f32) -> Vec<(Vec3, Vec3)>` in `affordances.rs`: the 18 outer edge segments. `ghost_parts` draws it, glowing each segment by `PlayGhost.outer`, and `bulldoze_parts` draws it for a massif target.
  - `affordances::cut_off(tiles: &[Axial], cell: Axial)` gains `members: &HashMap<Axial, Axial>` and treats a footprint as one node, matching Task 3.
  - Placeholder: while `massif_granite` is pending, `tile_children` for a footprint def spawns one meadow-placeholder prism per member under the anchor root (the existing pending-base path, offset by `member.to_world - anchor.to_world`), plus `fn massif_placeholder_mesh() -> Mesh`, a granite-grey hexagonal cone over the centre. `spawn_column` draws a column under each member when the level is above 0.
  - Rise and dust: `pub fn dust_cells(anchor: Axial, footprint: bool) -> Vec<Axial>`. The rise dust and the bulldoze sink cloud emit for every member cell of a footprint tile, with per-cell particle counts scaled so the total is `7 × the single-tile count × 0.5` (a first value, tuned in Task 9's shots).
  - Grass and scatter: member cells get no runtime grass, scatter or slope ground until Task 9 integrates the piece's ground. `grass::layout` and `scatter::layout` skip footprint defs.
  - Chunks: a massif sits in its anchor's chunk (`chunk_center(tile.cell, 19)`, play/chunks.rs :191). No change is expected, and a test guards it.

- [ ] **Step 1: Write the failing tests:**
  - `hex-play-state`, `tile_at_answers_members`: a view with a massif returns the anchor's `PlayTile` for every member.
  - `play.rs`, `massif_placeholder_spawns_seven_prisms_and_a_peak`: in a minimal app, the massif root has 7 prism children whose XZ centres are the member offsets, plus one cone child.
  - `play.rs`, `raised_massif_columns_reach_the_floor`: at level 2, every member has a column.
  - `affordances.rs`:
    - `footprint_outline_has_eighteen_segments`: every segment midpoint lies on a perimeter edge of the seven hexes.
    - `ghost_outline_glows_by_outer_status`: one `Incompatible` entry in `outer` gives exactly one incompatible-coloured segment.
  - `affordances.rs`, `cut_off_treats_a_footprint_as_one_tile`: matches Task 3's world.
  - `rise.rs`, `massif_dust_covers_the_footprint`: `dust_cells(anchor, true)` is the 7 members, and the particle centroid lies within 2 m of the anchor centre.
  - `chunks.rs`, `a_massif_belongs_to_its_anchor_chunk`: every member's draw goes to `chunk_center(anchor, 19)`, even when a member lies in another chunk.
  - `scatter.rs`, `no_runtime_scatter_on_a_massif_yet`: `layout` for the massif def gives no instances.

- [ ] **Step 2: Run** `cargo test -p hex-play-state`, `cargo test -p hex-play` and `cargo test -p hex-render chunks`. Expected: FAIL.

- [ ] **Step 3: Implement.** Keep the sampled-texture count unchanged: the placeholder reuses the existing placeholder materials.

- [ ] **Step 4: Run** `make checkall > /tmp/mega-t7.log 2>&1; echo EXIT=$?`. Expected: `EXIT=0`. Run `make run ARGS="--scenario play"`, select the massif, check that the slot outlines show footprints two steps out, place one, turn it, bulldoze it and undo. Capture screenshots only inside a booked window (orchestrator).

- [ ] **Step 5: Commit** `feat(play): mega-tile spawn, footprint ghost and affordances with a placeholder massif`.

### Task 8: Walkers, cars, boids and farms around a massif

**Files:**
- Modify: `crates/hex-play/src/walk.rs` (`CellGround` :280, `WalkGround::build` :949, `OBSTACLES` :80), `crates/hex-play/src/cars.rs`, `crates/hex-play/src/boids/plan.rs` (sky band, roosts), `crates/hex-play/src/farm.rs` (neighbour occupancy)
- Modify: `crates/hex-app/src/camera.rs` (`pick_cell`: a member cell picks its anchor's top, and the placeholder peak is treated as the cell top)
- Test: unit tests in those files

**Interfaces:**
- Consumes: Task 7 `PlayView.members` and `tile_at`.
- Produces:
  - `WalkGround::build` gives every member cell a `CellGround` at the anchor's level. With the placeholder, each member's ground is flat at `y0`, and the cone over the centre is an obstacle: `CENTRE_OBSTACLES` gains a `massif_granite` row sized to the placeholder, and Task 9 replaces it with the piece's collider. Walkers can step onto the ring's flat ground and walk around the peak.
  - Cars: no road reaches a massif (sockets refuse it), so no car change. A test guards it.
  - Boids: roosts skip member cells. The sky band over a footprint lifts by the piece's height from its manifest bounds (placeholder: the cone height), so flocks clear the peak.
  - Farm: farm field and fence code treat member cells as occupied, not as empty neighbours.

- [ ] **Step 1: Write the failing tests:**
  - `walk.rs`, `walker_walks_around_a_massif`: a straight walk aimed through the centre is deflected and never enters the peak's obstacle, and `stand` on a ring cell gives y ≈ `level * 4`.
  - `walk.rs`, `member_cells_are_ground_not_void`: `WalkGround` has a surface on every member cell.
  - `cars.rs`, `no_car_route_touches_a_massif`: in the Task 5 sim world, no route cell is a member.
  - `boids/plan.rs`, `flocks_clear_the_massif_peak`: the planned altitude over the anchor exceeds the peak height plus the existing clearance.
  - `farm.rs`, `a_massif_member_is_not_an_empty_neighbour`: whatever reads neighbour occupancy for fences sees the member as occupied.
  - `camera.rs`, `pick_on_a_member_returns_the_member_cell`: a ray hitting a ring cell's top returns that cell, so bulldoze and hover resolve through `anchor_of`.

- [ ] **Step 2: Run** `cargo test -p hex-play walk cars boids farm && cargo test -p hex-app camera`. Expected: FAIL.

- [ ] **Step 3: Implement.**

- [ ] **Step 4: Run** `make checkall > /tmp/mega-t8.log 2>&1; echo EXIT=$?`. Expected: `EXIT=0`.

- [ ] **Step 5: Commit** `feat(play): walkers, boids and farms treat massif members as occupied`.

### Task 9: The `massif_granite` kit piece (Blender asset agent, owner checkpoints)

**Files:**
- Create: `docs/checkpoints/massif_granite/reference/` (reference images and their provenance), `scripts/blender/kit/massif_granite.py` (plus a family helper if needed), the generated `assets/kit/<pack>/massif_granite/*`, its manifest entry (pack `hero`, streamed), and `assets-source/kit/massif_granite/` provenance with pending owner approvals
- Modify: `crates/hex-play/tests/kit_gate.rs` (a footprint base check), `data/pending-pieces.ron` (remove `massif_granite`)
- Return: `docs/checkpoints/massif_granite/integration.patch` with:
  - walk: the `CENTRE_OBSTACLES` or `OBSTACLES` row from the piece's collider.
  - scatter: `FOOTPRINTS`, `GROUND_BOXES` and `NOT_GROUND` rows for the peak.
  - grass: the ring-grass rule.
  - the boids peak height.
  - `kit_scene.rs` showcase layout and the `tile_counts` cell count.
  - removal of the Task 7 placeholder path for this piece.

**Interfaces:**
- Consumes:
  - The geometry pin (Global Constraints). Blender's y axis is −runtime z (`meadow_base.py:85-87`).
  - The meadow_base cap height and materials: the ring cap matches `meadow_base` at y = 0 along the 18-edge outline.
  - The scatter and grass rules: none baked.
  - The 16-sampled-texture budget.
- Produces:
  - Piece `massif_granite`: the ground mesh over all 7 cells, with a Meadow-matched ring and a granite peak whose height, scree and planting follow the approved reference.
  - The ring's outer border is flat at y = 0 on the flower outline, so every outer edge meets a level-equal Meadow seamlessly.
  - The ring uses the `meadow_base` texture set and masks (grass mask, AO) so runtime grass and scatter follow the Meadow rules for each ring cell (spec: Grass and scatter on the ring).
  - LOD chain, collider and sockets per the skill gates.
  - In `kit_gate.rs`, `footprint_base_border_is_the_flower_outline`: the massif's cap border vertices at y = 0 lie on the 18-segment outline within 1 mm, at the same height as `meadow_base`'s ring.

- [ ] **Step 1: Reference brief (starts at once, parallel with Task 1).** The orchestrator dispatches one Blender asset agent (Opus 5.5, xhigh, `isolation: "worktree"`) with the `blender-asset` skill brief (`.claude/skills/blender-asset/references/dispatch.md`, including part 11, the integration section). The agent produces 2 to 3 reference options for a granite massif on Meadow ground, at the play camera and at a low camera, with the footprint outline marked.
- [ ] **Step 2: CHECKPOINT C1, owner reference approval.** The references go into the owner review queue (`docs/review/<date>-owner-queue.md`, memory `owner-review-queue.md`). No generator work starts until the owner approves one. On rejection, revise and resubmit. Per memory `asset-pipeline-no-idle-on-approval.md`, the asset slot takes other queued pieces while this waits.
- [ ] **Step 3: Build.** The agent writes the headless generator. It bakes in Cycles, runs `make kit-build kit-compress kit-check kit-provenance` for the piece (all green), and renders review images into `docs/checkpoints/massif_granite/` beside a Meadow, a forest and a road tile at the same light.
- [ ] **Step 4: CHECKPOINT C2, owner piece approval.** The owner reviews the renders against the approved reference. Provenance stays pending until approval. On rejection, return to Step 3.
- [ ] **Step 5: Integrate.** The orchestrator lands the piece and the verified integration patch, removes `massif_granite` from `pending-pieces.ron`, and runs `make checkall > /tmp/mega-t9.log 2>&1; echo EXIT=$?` (expected `EXIT=0`, including `footprint_base_border_is_the_flower_outline`).
  - Re-enable ring grass and scatter: Task 7's `no_runtime_scatter_on_a_massif_yet` becomes `ring_scatter_matches_meadow_density`. Scatter instances per ring cell, outside `NOT_GROUND`, match a meadow cell's density for the same seed within the existing scatter tolerance.
  - Commit `feat(kit): massif_granite piece (pending owner approval in-engine)`.

### Task 10: Thumbnails, review shots, web smoke, perf, docs and the CI gate

**Files:**
- Modify: `crates/hex-app/src/thumbs.rs` (`LOOK_AT` :42, `plan_json` :99, `camera_transform` :117), `crates/hex-core/tests/data_validator.rs` (remove the Task 5 `#[ignore]`), `assets/thumbs/*` (via `make thumbs`)
- Create: `docs/checkpoints/massif_granite/massif-shots.json` (a shots world: a massif ringed by Meadow, a forest edge, a road passing one side cell, and a level-1 massif beside level-0 Meadow), `e2e/massif.mjs`
- Modify: `Makefile` (`shots-massif`, beside `shots-elevation` :347), `docs/agent-operability.md` (the `mega_slots`, ghost `footprint` and `outer_edges` state fields), `PRD.md` §15 P4 and the A52 decisions, `docs/save-format.md`, `handoff.md`

**Interfaces:**
- Produces:
  - `pub fn camera_for(def: &TileDef) -> (Vec3, f32)` in `thumbs.rs`: (look_at, distance). It returns today's `LOOK_AT`/`DISTANCE` for a single tile, and a frame that fits the 7-cell footprint plus the peak for a footprint tile.
  - `plan_json` writes a per-tile `camera` override only for footprint tiles, so every existing tile's plan entry and thumbnail is byte-identical.

- [ ] **Step 1: Write the failing test** in `thumbs.rs`, `single_tile_camera_is_unchanged`: `camera_for(meadow)` equals the current constants, and `camera_for(massif)` frames all 7 members' corners inside the render at its pitch and FOV. Run `cargo test -p hex-app thumbs`. Expected: FAIL. Implement, then PASS.
- [ ] **Step 2: Thumbnails.** In a booked load-sensitive window, run `caffeinate -d -u make thumbs > /tmp/mega-thumbs.log 2>&1; echo EXIT=$?`. Expected: `EXIT=0`, and `git status assets/thumbs` shows only `massif_granite-r0..r5`, the manifest and the contact sheet. Any other dirtied thumbnail is a regression to fix first. Remove the `#[ignore]`, and `cargo test -p hex-core --test data_validator` passes.
- [ ] **Step 3: Review shots.** Add `make shots-massif` (`shots-piece` with `massif-shots.json`) and run it in the booked window. It captures:
  - the play camera overview;
  - a low camera along the ring-to-Meadow seam;
  - the raised massif's cliff edge;
  - the ghost turning (two rotations);
  - the bulldoze sink.
  No frame may be black. Check the seam against the visual standard: no colour, density or shading step between ring and Meadow.
- [ ] **Step 4: Web smoke.** `e2e/massif.mjs` (Playwright GPU Chromium, after `make build-web`) drives the bridge: select the massif, hover a frontier slot, place it, R, undo, redo, bulldoze. After each step it asserts from `window.hexWorld` state that the ghost `footprint` has 7 cells, `mega_slots` updates, and the tiles list holds the anchor alone. It also asserts that the HUD Tiles sheet lists the massif exactly where `megaFit` says. Run `node e2e/massif.mjs dist` and expect exit 0.
- [ ] **Step 5: Perf and flat-world parity.** In a booked MiniMac window, load-guarded (`e2e/perf-load.mjs`):
  - Run `e2e/perf-close-camera.mjs` on `world2066` for main and this branch. The flat world must not get slower beyond the paired spread.
  - Capture the existing visual-parity flat-world shots on both and diff them. They must be pixel-identical.
  - Record the numbers and diffs in `docs/perf/mega-tiles-2026-10-29/README.md`.
- [ ] **Step 6: CHECKPOINT C3, owner in-engine review before merge.** The Step 3 shots go to the owner review queue. The branch does not merge to main until the owner approves the massif in-engine. On rejection, return to Task 9 Step 3 or the relevant presentation task.
- [ ] **Step 7: Docs.**
  - Update PRD §15 P4 with the approved A52 answers, the 18-edge correction and what landed or was deferred (owner rule: PRD tracks decisions).
  - `docs/save-format.md`: footprints are implied by the def and expanded on decode.
  - `docs/agent-operability.md`: the new state fields.
  - `handoff.md`.
- [ ] **Step 8: Gate.** Run `make checkall-ci > /tmp/mega-ci.log 2>&1; echo EXIT=$?`. Expected: `EXIT=0`.
- [ ] **Step 9: Commit** `feat(p4): mega tiles first round — massif thumbs, shots, web smoke, docs`. After the merge to main, run `make build-web` (owner rule).
