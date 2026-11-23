# Rivers and waterfalls at levels Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rivers run on raised ground (levels 1 to 3), drop as waterfalls where a river lane meets a lower river lane, and carry one derived flow direction per chain, with placement refusing a river that would run uphill. Flat worlds stay byte-identical.

**Architecture:** Six phases from the spec. Phase 1 moves river orientation out of `hex-play::water::tiles::oriented_rivers` into a new pure module `hex-core/src/river.rs` with today's rule, behind a parity test. Phase 2 makes every water input take the tile level (today all water sits at the world constant `WATER_Y = -1.4`) and gives the water layout a third edge state, `Waterfall`. Phase 3 adds `HeightMeet::Waterfall`, the legality rule, `Refusal::RiverUphill`, raisable river tiles, the save checks and boat run ends; phases 3 and 4 runtime pieces that affect agents merge together. Phase 4 draws the falling sheet and plunge pool inside the existing water meshes, splits fish regions, clears scatter, publishes `play.falls` and adds the waterfall sound and the ghost flow preview. Phase 5 (Blender asset work, parallel with phases 2 to 4) builds the `waterfall_l1..l3` fall-face pieces and integrates them. Docs fold into the task that changes the behaviour.

**Tech Stack:** Rust workspace (hex-core, hex-sim, hex-play, hex-render, hex-app), Bevy =0.20.1 (`default-features = false`), WESL shaders (`assets/shaders/water.wesl`), RON tile data, TypeScript HUD (`web/ui`, Node `node --test`), headless Blender generators through the blender-asset skill, ElevenLabs audio under the standing audio approval.

**Spec:** `docs/superpowers/specs/2026-11-22-rivers-at-levels-design.md` (approved 2026-11-22, every recommendation).

## Global Constraints

- Bevy is pinned `=0.20.1` with `default-features = false`. No dependency changes.
- One level is `LEVEL_STEP` = 4.0 m (`hex-core/src/axial.rs`), levels 0..=3 (`MAX_LEVEL = 3`). Water at rest is `WATER_Y` = -1.4 m relative to the tile's level, the river bed centre line -2.95, the tile base -4.0 (water contract §1).
- A waterfall drop is 1, 2 or 3 levels (4, 8 or 12 m). Only two flat River lane edges fall. Road and rail lanes at different levels stay refused (viaduct `deck_meet` unchanged).
- River lanes are 8 m wide (`data/sockets.ron`). River meets only River. No fall into the sea (RQ4 O1). Mouths, beaches and ocean stay at level 0.
- Raisable river tiles: `river_straight`, `river_bend`, `river_bend_60`, `river_fork`, `river_spring`, `river_road_bridge`, `river_rail_bridge` (RQ7 O2). Not `river_mouth`, `river_mouth_l`. No river ramps or rapids (RQ3 O1).
- A network with no waterfall is always legal and is oriented by today's rule (water contract §6), unchanged.
- Placement never picks a waterfall by default: it counts as a cliff face in `placement_key`, R/scroll reaches it (RQ1 O1). A bridge tile may be the lip or the pool (RQ8 O1). A flat chain with no spring or mouth may reverse when a waterfall fixes it (RQ12 O1).
- hex-core is the only implementation of placement rules (ENH-002). The HUD reads `play.fits`. A Rust enum change in `hex-app` or a `Refusal::codes()` change needs `make ui-schema` and matching `web/ui/src/bridge.ts` work; `make checkall` fails until both sides agree.
- Save format stays v3 (`SAVE_FORMAT_VERSION = 3`). Add the same-version fixture `crates/hex-core/tests/fixtures/save/v3-waterfall.hexworld`. Waterfalls and flow are never saved. Wire structs never change.
- No new sampled texture in any pipeline: kit forward next to refracting water already samples exactly 16, WebGPU's default per stage (ADR 0002). `make web-smoke` clamps the device to 16.
- No new draw for the sheet or the plunge pool: both ride the upper or lower tile's existing water mesh. Overview camera gate: at most 500 draws (owner A2 Q1), measured natively (`make shots-scatter`) and on WebGPU (`make web-scatter`).
- Water meshes spawn once with `NoAutoAabb` (PRD §10, `water::spawn_fixed_mesh`). A waterfall appearing or going respawns the mesh, never mutates it.
- Fall face kit piece: LOD0 at most 4k triangles, LOD1 at most 50% and LOD2 at most 15% of LOD0, textures at most 2k, KTX2 at most 1.5 MB per piece, GLB at most 2 MB, pack `water` (the 30 MiB initial download is unchanged). Headless Blender only, one generator for all three heights (RL-Q4 precedent). Look per RQ2: O1 straight granite lip for the 4 m drop, O2 stepped ledges (two or three tiers) for 8 and 12 m.
- Sheet look per RQ5: O2 (clear, refracting at the lip, white below) at High and Ultra, O1 (thick, mostly white with streaks) at Low and Medium.
- Plunge pool bed is not deepened (RQ9 O1). Fish stay on their side of a fall, no leaping (RQ6 O1).
- Waterfall sound: a positional loop at each pool, gain by drop, at most two voices near the focus, auditioned by the owner with occasional one-shots as the fallback (RQ10 O1). Generated under the standing audio approval; the owner approves by ear.
- The ghost shows the flow its placement would give (RQ11 O2).
- Scatter is never baked into a tile or a kit piece (owner 2026-10-26). Bank stones, reeds, lilies and reed beds stay runtime scatter.
- `make thumbs` is byte-reproducible and must dirty nothing for this work (thumbnails are per (tile, rotation) at level 0; waterfalls are left out by a `NoFalls` marker). The 2026-11-18 thumbs card closed with zero drift on clean main (249 images byte-identical), so a dirtied thumbnail is a real change.
- Owner-approved in-engine shots before any visual merge (owner 2026-10-29 visual standard). Never record an owner approval yourself.
- GPU captures, `make thumbs`, `make shots-*`, `make web-smoke`, `make ui-e2e` and perf runs are load-sensitive: the orchestrator books a host window. Timing needs the 1-minute load at most 15 before and after each run, else the run is discarded. Unattended native captures run under `caffeinate -d -u`.
- Every merge to `main` is followed by `make build-web`. Commit after each verified task. `make checkall-ci` green before any push; pushes follow `handoff.md` "Standing rules".
- Sub-agents run no `kanban` command and file no board items.
- Machine and host names stay out of committed docs and commits.

## Review Focus

1. **Flat networks today's rule accepts but a strict rule would refuse.** A chain between two springs, a chain between two mouths, a fork fed by springs on all three arms, a closed river loop. They must still place, decode and keep their flow. Test: `flat_networks_accepted_today_still_place_and_decode` (Task 6) and the parity test `orientation_matches_legacy_on_fixtures_and_random_flat_worlds` (Task 2).
2. **Float ties in the junction rule after the port.** Today's junction rule compares metres along centre lines and keeps the chain rule on an exact tie. A port that reorders f32 operations flips a tie. Test: `junction_ties_match_legacy_on_symmetric_forks` (Task 2), over every fork rotation with equal-length arms.
3. **Junction networks mixing fixed and free chains.** A fork with one arm fixed by a waterfall and two free arms, a fork whose arms all fall in or all fall away, two falls feeding one fork. Test: `waterfall_legality_matches_brute_force_oracle` (Task 5), exhaustive over every orientation of random networks of up to 8 river tiles with levels 0..=3.
4. **History replay order.** The backward replay inserts a tile when undoing a bulldoze, and a replace can reverse a chain. A tampered undo stack that re-forms an uphill fall must be refused, and every played history must still validate. Test: `replay_rechecks_rivers_on_every_insert` plus the extended `incremental_history_replay_matches_the_full_sweep` (Task 8).
5. **Level-blind consumers next to a raised river.** A level-3 river beside a level-0 river: fish flood across the fall, boats chain over it, the walker's ford depth is computed against -1.4, grass is culled 12 m too low. Tests: `fish_regions_split_at_a_waterfall`, `walker_fords_a_raised_river_at_its_level`, `grass_keeps_its_margin_over_a_raised_river` (Task 4), `a_rowing_run_ends_at_a_waterfall` (Task 9).

---

## Phase 1: Flow in hex-core, no level change

### Task 1: `hex-core::river` with today's orientation rule

**Files:**
- Create: `crates/hex-core/src/river.rs`
- Modify: `crates/hex-core/src/lib.rs` (`pub mod river;` and re-exports)
- Read (the rule being ported): `crates/hex-play/src/water/tiles.rs` lines 405 to 830 (`RiverEnd`, `river_segments`, `arc_through`, `oriented_rivers`, `junction_sources`)
- Docs: none (water contract §6 already states the rule)

**Interfaces:**
- Consumes: `TileDef::networks`, `TileDef::nodes`, `NetworkKind::River`, `Endpoint`, `TileDef::edge`, `Axial::neighbor`, `opposite_edge`, `world_edge`, `HEX_RADIUS`/apothem constants.
- Produces:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
  pub enum RiverEnd { Edge(u8), Spring, Mouth, Junction } // Edge holds the world edge
  #[derive(Clone, Copy, Debug)]
  pub struct RiverTile<'a> { pub cell: Axial, pub def: &'a TileDef, pub rotation: u8, pub level: u8 }
  #[derive(Clone, Debug, PartialEq)]
  pub struct RiverSeg { pub up: RiverEnd, pub down: RiverEnd, pub length: f32, pub reversed: bool }
  #[derive(Clone, Debug, Default, PartialEq)]
  pub struct Rivers { pub segs: BTreeMap<(Axial, usize), RiverSeg>, pub falls: Vec<Waterfall> }
  #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
  pub struct Waterfall { pub upper: Axial, pub edge: u8, pub lower: Axial, pub drop: u8 }
  pub fn river_segment_ends(def: &TileDef, rotation: u8) -> Vec<(RiverEnd, RiverEnd, f32)>;
  pub fn orient(tiles: &[RiverTile]) -> Rivers; // phase 1: falls always empty
  ```
  `reversed` is true when the flow runs against the segment's authored `from -> to`, so the hex-play adapter flips its points.

- [ ] **Step 1: Write the failing tests** in `river.rs` `#[cfg(test)]`, using `TileCatalog::builtin()`:
  ```rust
  #[test]
  fn a_spring_chain_flows_away_from_its_spring() {
      // river_spring at (0,0) rot 0, river_straight at (1,0) rot 0 (edges 0 and 3 face each other)
      let r = orient(&tiles(&[("river_spring", (0, 0), 0), ("river_straight", (1, 0), 0)]));
      assert_eq!(r.segs[&(ax(0, 0), 0)].up, RiverEnd::Spring);
      assert_eq!(r.segs[&(ax(1, 0), 0)].down, RiverEnd::Edge(0));
      assert!(r.falls.is_empty());
  }
  #[test]
  fn an_open_chain_starts_at_its_lowest_open_end() { /* two straights, up == lowest (cell, edge) */ }
  #[test]
  fn a_mouth_chain_flows_into_the_mouth() { /* straight + river_mouth: mouth tile's down == Mouth */ }
  #[test]
  fn a_lone_fork_leaves_through_local_edge_zero() { /* fork rot k: the arm on world edge k is the outflow */ }
  #[test]
  fn segment_lengths_match_the_lane_geometry() {
      // river_straight: 2 * apothem = 27.7128; river_bend: the 24 m arc through 60 deg = 25.1327
      assert!((len("river_straight") - 27.712_8).abs() < 1e-3);
      assert!((len("river_bend") - 25.132_7).abs() < 1e-3);
  }
  ```
- [ ] **Step 2: Run** `cargo test -p hex-core river::`. Expected: compile failure (module missing).
- [ ] **Step 3: Implement** the module: port `river_segments` (ends and lengths, with `arc_through` in plain `[f32; 2]` math keeping glam's operation order exactly, since the junction rule's ties depend on bit-equal lengths), `source_rank`, the chain walk and `junction_sources`, returning `Rivers`. No Bevy, no glam.
- [ ] **Step 4: Run** `cargo test -p hex-core river::` and `cargo test -p hex-core`. Expected: pass.
- [ ] **Step 5: Commit** `feat(core): river orientation in hex-core (today's rule)`.

### Task 2: hex-play adapter, parity test and flow arrows in the debug overlay

**Files:**
- Modify: `crates/hex-play/src/water/tiles.rs` (`oriented_rivers` at line 540 becomes an adapter over `hex_core::river::orient`; keep `RiverPath`, re-export `hex_core::river::RiverEnd` as `tiles::RiverEnd` so `boats/plan.rs` compiles unchanged)
- Create: `crates/hex-play/src/water/legacy.rs` (`#[cfg(test)]` frozen copy of today's `oriented_rivers` as the oracle)
- Modify: `crates/hex-play/src/debug.rs` (routing-graph overlay: a chevron every 6 m along each river path, pointing downstream)

**Interfaces:**
- Consumes: Task 1 `orient`, `RiverTile`, `RiverSeg::reversed`.
- Produces: `pub fn oriented_rivers(tiles: &[PlayTile], catalog: &TileCatalog) -> BTreeMap<(Axial, usize), RiverPath>` (signature unchanged).

- [ ] **Step 1: Write the failing tests** in `water/tiles.rs` tests:
  ```rust
  #[test]
  fn orientation_matches_legacy_on_fixtures_and_random_flat_worlds() {
      for world in fixture_worlds().chain(random_flat_river_worlds(512, 0xA41)) {
          assert_eq!(oriented_rivers(&world, &cat()), legacy::oriented_rivers(&world, &cat()));
      }
  }
  #[test]
  fn junction_ties_match_legacy_on_symmetric_forks() {
      // a fork at every rotation with three equal straights on its arms, then with springs on two arms
      for k in 0..6 { for w in symmetric_fork_worlds(k) {
          assert_eq!(oriented_rivers(&w, &cat()), legacy::oriented_rivers(&w, &cat()));
      } }
  }
  ```
  `fixture_worlds()` decodes every `crates/hex-core/tests/fixtures/save/*.hexworld` (include_bytes) into `PlayTile`s. `random_flat_river_worlds` grows connected worlds of river straights, bends, forks, springs, mouths and bridges at level 0 by legal placement from a seed.
- [ ] **Step 2: Run** `cargo test -p hex-play orientation_matches_legacy junction_ties_match`. Expected: compile failure (`legacy` missing), then pass of the legacy-vs-legacy baseline once `legacy.rs` exists and before the adapter swap. Then swap the adapter and run again; a failure here is a port bug in Task 1, fixed there.
- [ ] **Step 3: Implement** the adapter and the overlay chevrons (debug overlay only; the overlay is never in captures or thumbs).
- [ ] **Step 4: Run** `cargo test -p hex-play` (boats, boids and water tests unchanged) and `make checkall`. Expected: green.
- [ ] **Step 5 (booked capture):** `make shots-water` before and after on the same build host. Expected: byte-identical frames (flat parity).
- [ ] **Step 6: Commit** `refactor(play): river flow comes from hex-core; flow arrows in the routing overlay`.

---

## Phase 2: Water at levels

### Task 3: Water surfaces, beds, cut faces, rise and ghost water take the tile level; the waterfall edge state

**Files:**
- Modify: `crates/hex-play/src/water/tiles.rs`: `EdgeWater` (line 340), `edge_water` (377), `continues` (368), `layout` (836), `CellWater` (817), `TileField::level` (972) and the bed profile calls, `spawn_cell` (1255)
- Modify: `crates/hex-play/src/play/rise.rs` (water fill starts at the raised tile's base: `FLOOR_Y` line 39 becomes per tile)
- Modify: `crates/hex-play/src/play/ghost.rs` (`water_surface` line 323 lifted by the ghost level; test at 1084)
- Docs: `docs/kit/water-contract.md` §1 (water and bed heights are relative to the tile level) and §6 (a waterfall seam row: River at different levels is neither continuous nor exposed)

**Interfaces:**
- Produces:
  ```rust
  pub fn water_y(level: u8) -> f32 { f32::from(level) * LEVEL_STEP + WATER_Y }
  enum EdgeWater { Dry, River(u8 /* tile level */), Ocean, Coast(Vec2) }
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub struct FallEdge { pub edge: u8, pub drop: u8, pub upper: bool }
  pub struct CellWater { /* existing fields */ pub falls: Vec<FallEdge> }
  ```
  `continues(River(a), River(b))` is `a == b`. A wet River edge facing River at another level goes in `falls`, never in `exposed`, so it draws no cut face on either side.
- Consumes: `PlayTile::level`.

- [ ] **Step 1: Write the failing tests** in `water/tiles.rs` tests:
  ```rust
  #[test]
  fn a_raised_river_surface_and_bed_stand_at_its_level() {
      for level in 1..=3u8 {
          let lay = layout(&[river("river_straight", (0, 0), 0, level)], &cat(), |_| true);
          let f = lay[&ax(0, 0)].field();
          assert_eq!(f.level(), water_y(level));
          assert!((f.height(0.0, 0.0) - (-2.95 + 4.0 * f32::from(level))).abs() < 1e-3);
      }
  }
  #[test]
  fn river_at_different_levels_is_a_fall_not_a_cut_face() {
      let lay = layout(&[river("river_straight", (0, 0), 0, 2), river("river_straight", (1, 0), 0, 0)], &cat(), |_| true);
      assert_eq!(lay[&ax(0, 0)].falls, vec![FallEdge { edge: 0, drop: 2, upper: true }]);
      assert_eq!(lay[&ax(1, 0)].falls, vec![FallEdge { edge: 3, drop: 2, upper: false }]);
      assert!(!lay[&ax(0, 0)].exposed.contains(&0) && !lay[&ax(1, 0)].exposed.contains(&3));
  }
  #[test]
  fn a_level_zero_world_lays_out_exactly_as_before() { /* every CellWater of the mixed water world: falls empty, exposed unchanged */ }
  ```
  In `rise.rs` tests: `water_of_a_raised_tile_fills_from_its_own_base` (fill starts at `4.0 * level - 4.0`). In `ghost.rs`: change `the_water_surface_covers_where_the_water_goes` to assert `v[1] == water::tiles::water_y(level)` for levels 0 and 2.
- [ ] **Step 2: Run** `cargo test -p hex-play water::tiles rise ghost`. Expected: the new tests fail (level ignored), the ghost test fails at level 2.
- [ ] **Step 3: Implement.** One convention for all water: meshes built in world space at `water_y(level)` offsets (bed profiles add `f32::from(level) * LEVEL_STEP`). The released water entities' sink follows the same offset.
- [ ] **Step 4: Run** `cargo test -p hex-play` and `make checkall`. Expected: green.
- [ ] **Step 5: Commit** `feat(play): water stands at its tile's level; waterfall edge state`.

### Task 4: Level-aware water consumers (boats, walker, fish, grass)

**Files:**
- Modify: `crates/hex-play/src/boats.rs` lines 391 to 404 (water height) and 503 (boat transform)
- Modify: `crates/hex-play/src/walk/ground.rs` lines 274 (`STAGE_DECK` becomes relative), 365 to 368, 514 to 546, 644
- Modify: `crates/hex-play/src/boids/plan.rs` lines 574 to 645 and 950 (fish band and ceiling per cell), and the region flood in `FishWorld::new` (line 798): a wet edge in `cw.falls` stops the flood like an exposed one
- Modify: `crates/hex-play/src/play/grass.rs` line 814 (margin over each cell's own water)

**Interfaces:**
- Consumes: Task 3 `water_y`, `CellWater::falls`.
- Produces: no new public items.

- [ ] **Step 1: Write the failing tests:**
  ```rust
  // boids/plan.rs
  #[test]
  fn fish_regions_split_at_a_waterfall() {
      let w = FishWorld::new(&[river("river_straight", (0, 0), 0, 1), river("river_straight", (1, 0), 0, 0)], &cat());
      assert_eq!(w.regions.len(), 2);
  }
  #[test]
  fn the_fish_band_of_a_raised_river_is_one_step_up() { /* band top and floor at (0,0) level 1 = level 0 values + 4.0 */ }
  // walk/ground.rs (tests in walk/tests.rs)
  #[test]
  fn walker_fords_a_raised_river_at_its_level() { /* centre of a level-2 river_straight: depth == level-0 depth, head y + 8.0 */ }
  // boats.rs
  #[test]
  fn a_raised_river_carries_its_boat_at_its_level() { /* water height at a level-1 river centre == WATER_Y + 4.0 within wave amplitude */ }
  // grass.rs
  #[test]
  fn grass_keeps_its_margin_over_a_raised_river() { /* no blade on a level-1 river tile below water_y(1) + WATER_MARGIN */ }
  ```
- [ ] **Step 2: Run** `cargo test -p hex-play fish_regions_split fish_band walker_fords a_raised_river_carries grass_keeps`. Expected: all fail.
- [ ] **Step 3: Implement** with `water_y(level)` of the sampled cell. No signature change outside the modules.
- [ ] **Step 4: Run** `cargo test -p hex-play` and `make checkall`. Expected: green, every level-0 test unchanged.
- [ ] **Step 5: Commit** `feat(play): boats, walker, fish and grass follow the water level`.

---

## Phase 3: Raised rivers and the waterfall rule

### Task 5: `HeightMeet::Waterfall` and the legality rule with its oracle

**Files:**
- Modify: `crates/hex-core/src/socket.rs` (`HeightMeet` line 137, `heights_meet` line 150, new `river_fall` beside `deck_meet` line 176, test `lane_edges_never_cliff` line 697)
- Modify: `crates/hex-core/src/river.rs` (falls found and legality checked in `orient`)
- Docs: PRD §5.2 height rules: add the River waterfall exception beside the viaduct deck one

**Interfaces:**
- Produces:
  ```rust
  pub enum HeightMeet { Seamless, Cliff, Waterfall { drop: u8 }, Refused }
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub struct Uphill { pub cell: Axial, pub edge: u8 }
  pub fn orient(tiles: &[RiverTile]) -> Result<Rivers, Uphill>; // replaces the phase-1 signature
  ```
  `heights_meet`: both edges `kind == River`, `profile == Flat`, lanes present, `deck == 0`, absolute ground differing by 1..=3 gives `Waterfall { drop }`. `SocketTable::edges_compatible` accepts it (it only rejects `Refused`).
  Legality (spec §1, as amended): a network with no waterfall is always legal. A network with at least one waterfall is legal iff some orientation of its chains has (1) every waterfall crossed from high to low, (2) no directed cycle through a waterfall, (3) no waterfall-fixed chain flowing into a spring at its end or out of a mouth at its end; among legal orientations, chains not fixed by a waterfall take today's rule with the fixed chains as given. `Uphill` names the first offending seam in (cell, edge) order.
- Consumes: Task 1 module.

- [ ] **Step 1: Write the failing tests:**
  ```rust
  // socket.rs
  #[test]
  fn river_lanes_fall_one_to_three_levels() {
      let r = river_lane();
      for d in 1..=3 { assert_eq!(heights_meet(&r, d, &r, 0), HeightMeet::Waterfall { drop: d }); }
      assert_eq!(heights_meet(&r, 0, &r, 3), HeightMeet::Waterfall { drop: 3 });
      assert_eq!(heights_meet(&r, 1, &r, 1), HeightMeet::Seamless);
  }
  #[test]
  fn only_river_lanes_fall() { // was lane_edges_never_cliff, narrowed
      let (road, rail) = (lane(Road, 6.0), lane(Rail, 3.5));
      assert_eq!(heights_meet(&road, 1, &road, 0), HeightMeet::Refused);
      assert_eq!(heights_meet(&rail, 1, &rail, 0), HeightMeet::Refused);
      assert_eq!(heights_meet(&river_lane(), 1, &EdgeSocket::new(Grass), 0), HeightMeet::Refused);
  }
  // river.rs
  #[test]
  fn a_fall_fixes_its_chain_downhill() { /* straight L1 -> straight L0 -> open: up on the L1 tile is its far edge */ }
  #[test]
  fn falls_facing_both_ways_are_uphill() { /* L0 - L1 - L0 chain: Err(Uphill) */ }
  #[test]
  fn a_fall_onto_a_spring_is_uphill() { /* spring L0 below a straight L1 whose lane faces the spring's edge 0 */ }
  #[test]
  fn a_fall_onto_a_mouth_is_legal_and_a_mouth_never_feeds_a_fall() { /* L2 straight onto river_mouth: Ok, drop 2 */ }
  #[test]
  fn a_loop_through_a_fall_is_uphill() { /* ring of bends with one fall */ }
  #[test]
  fn a_bridge_tile_may_be_lip_or_pool() { /* river_road_bridge at L1 over a straight at L0, and at L0 under a straight at L1 */ }
  #[test]
  fn a_flat_chain_without_source_reverses_when_a_waterfall_fixes_it() { /* RQ12 O1 */ }
  #[test]
  fn waterfall_legality_matches_brute_force_oracle() {
      // up to 8 river tiles (straight, bends, fork, spring, mouth) at levels 0..=3, 2048 seeded worlds:
      // orient(..).is_ok() == oracle_exists_legal_orientation(..), and when Ok every fall descends.
  }
  ```
- [ ] **Step 2: Run** `cargo test -p hex-core river socket::`. Expected: fail (variant and checks missing).
- [ ] **Step 3: Implement** `river_fall` in socket.rs and the waterfall pass in `orient`: collect falls from adjacent River lanes at different levels, fix those chains, propagate through junctions, check (1) to (3), then run today's rule on the free chains. The oracle enumerates every chain direction in the test module only.
- [ ] **Step 4: Run** `cargo test -p hex-core`. Expected: green, including the Task 1 and every existing world test.
- [ ] **Step 5: Commit** `feat(core): river lanes fall across levels; waterfall legality`.

### Task 6: Placement: legality in `fits`, the cliff ranking and `Refusal::RiverUphill`

**Files:**
- Modify: `crates/hex-core/src/world.rs`: `Refusal` (118), `code` (152), `codes` (167), `Face` (68), `face` (505), `cliffs` (539), `placement_key` (567), `cliff_edges` (607), `fits` (624), `placement_refusal` (735)
- Add: `World::rivers(&self, catalog: &TileCatalog) -> Rivers` for the runtime and the state
- Modify: `web/ui/src/bridge-schema.json` (`make ui-schema`), `web/ui/src/bridge.ts`, `web/ui/src/words.ts`, `web/ui/test/rules.test.ts`

**Interfaces:**
- Produces:
  ```rust
  pub enum Refusal { /* existing */ RiverUphill }   // code "river_uphill"
  pub struct Face { /* existing */ pub waterfall: bool }
  impl World {
      pub fn rivers(&self, catalog: &TileCatalog) -> Rivers;
      fn rivers_legal_with(&self, catalog: &TileCatalog, cell: Axial, def: &TileDef, o: Orientation) -> bool;
  }
  ```
  `Refusal::codes()` fixed codes become `["inner_slot", "deck_end", "footprint_edges", "no_footprint_room", "river_uphill"]`, then the `missing_` and `clash_` codes as now. `fits` also requires `rivers_legal_with`, which orients only the river networks the tile's River lanes join (flood from its lanes). `cliffs` counts a `Waterfall` face as a cliff face with its drop. `placement_refusal` returns `RiverUphill` when some orientation meets every socket (`clash_count == 0` at its level) but none is river-legal, checked before the `socket_refusal` fallback.
- Consumes: Task 5.

- [ ] **Step 1: Write the failing tests** in world.rs tests:
  ```rust
  #[test]
  fn flat_networks_accepted_today_still_place_and_decode() {
      // two springs joined by straights; two mouths joined by straights; a fork with springs on all three arms;
      // a closed ring of river_bend; each built by place() at level 0, encoded and decoded
      for w in flat_cases() { assert!(SaveData::decode(&encode(&w), &cat()).is_ok()); }
  }
  #[test]
  fn placement_never_picks_a_waterfall_by_default() {
      // a level-1 river_straight with its lane open: auto_orient of river_straight beside it picks level 1, rotation aligned
      // R cycling (ordered orientations) reaches level 0 (the fall) after the fall-free ones
  }
  #[test]
  fn an_uphill_river_is_refused_as_river_uphill() {
      // The pinch: cell C between a level-0 river_spring whose lane faces C and a level-1 river_straight
      // whose lane faces C from the opposite side. A river_straight at C meets the sockets only at its two
      // aligned rotations. Level 0: water falls from the straight through C onto the spring. Level 1: C
      // falls onto the spring. Levels 2 and 3: falls run both ways along the chain. All river-illegal.
      let (w, c_cell) = pinch_world();
      let d = def("river_straight");
      for o in aligned_rotations(&w, c_cell, d).into_iter().flat_map(|k| (0..=3).map(move |l| Orientation::new(k, l))) {
          assert!(w.clash_count(&c, c_cell, d, o) == 0, "{o:?} meets the sockets");
          assert!(!w.fits(&c, c_cell, d, o), "{o:?} must be river-illegal");
      }
      assert_eq!(w.placement_refusal(&c, c_cell, d), Some(Refusal::RiverUphill));
      assert_eq!(Refusal::RiverUphill.code(), "river_uphill");
      assert!(Refusal::codes().contains(&"river_uphill".to_owned()));
  }
  #[test]
  fn bulldozing_never_makes_a_river_uphill() { /* random legal fall worlds: every can_bulldoze removal leaves rivers() legal */ }
  #[test]
  fn a_fall_face_is_flagged_on_the_ghost() { /* ghost_faces(...).iter().any(|f| f.waterfall && !f.cliff) */ }
  ```
- [ ] **Step 2: Run** `cargo test -p hex-core world::`. Expected: new tests fail; the flat case test must pass before and after (it pins today's behaviour).
- [ ] **Step 3: Implement** per the interfaces. Keep `Face::cliff` false on a waterfall seam (it is a river seam, not ground) and set `waterfall`. Cost: `legal_orientations` runs for every orientation of every inventory entry on each hover, so `World` caches its `Rivers` per world change (invalidated by place, bulldoze, replace, undo, redo) and `rivers_legal_with` re-floods only the networks the candidate's lanes touch. Task 15 measures it.
- [ ] **Step 4: Two-sided code change in this task:** `make ui-schema`, add `"river_uphill"` to the `RefusalCode` union and `REFUSAL_CODES` in `web/ui/src/bridge.ts` (lines 275 to 300), and `REFUSAL_WORDS.river_uphill = "Rivers can't run uphill."` in `web/ui/src/words.ts` (lines 160 to 180), with the test `test("river_uphill has a wording", () => assert.equal(refusalWord("river_uphill"), "Rivers can't run uphill."))` in `web/ui/test/rules.test.ts`.
- [ ] **Step 5: Run** `cargo test -p hex-core`, `cargo test -p hex-app bridge::schema`, `node --test web/ui/test/rules.test.ts`, `make ui-check`, `make checkall`. Expected: green. (The pinch test needs raisable river tiles, so it lands here with a test-local catalog that flips `raisable` on `river_straight`, and moves to the builtin catalog in Task 7.)
- [ ] **Step 6: Commit** `feat(core): river legality in placement; river_uphill refusal`.

### Task 7: Raisable river tiles

**Files:**
- Modify: `data/tiles/river_straight.ron`, `river_bend.ron`, `river_bend_60.ron`, `river_fork.ron`, `river_spring.ron`, `river_road_bridge.ron`, `river_rail_bridge.ron` (add `raisable: true` after `props`, as `road_straight.ron` does)
- Modify: `web/ui/src/bridge-schema.json` only if `make ui-schema` regenerates it (the inventory carries tile data)
- Test: `crates/hex-core/tests/data_validator.rs`, `crates/hex-core/src/world.rs` (the pinch test moves to the builtin catalog), `crates/hex-play/src/trains.rs` tests

**Interfaces:**
- Consumes: Task 6 `Refusal::RiverUphill`.
- Produces: the seven raisable river defs.

- [ ] **Step 1: Write the failing tests:**
  ```rust
  // data_validator.rs
  #[test]
  fn river_tiles_rise_and_mouths_stay_on_the_ground() {
      for id in ["river_straight","river_bend","river_bend_60","river_fork","river_spring","river_road_bridge","river_rail_bridge"] {
          assert!(level_allowed(c.get(id).unwrap(), 3), "{id}");
      }
      for id in ["river_mouth", "river_mouth_l"] { assert!(!c.get(id).unwrap().raisable, "{id}"); }
  }
  // trains.rs (RQ7 O2)
  #[test]
  fn a_train_crosses_a_raised_river_rail_bridge_at_its_level() { /* level-1 bridge: placed(..).translation.y ~ LEVEL_STEP + RAIL_TOP_Y */ }
  ```
- [ ] **Step 2: Run** `cargo test -p hex-core river_tiles_rise`, `cargo test -p hex-play a_train_crosses`. Expected: fail.
- [ ] **Step 3: Implement**: the RON flags, then `make ui-schema`. Check `data_validator` still holds every (tile, rotation) thumbnail rule (relative heights unchanged, so no new thumbnails).
- [ ] **Step 4: Run** `make ui-check`, `cargo test -p hex-app bridge::schema`, `cargo test --workspace`, `make checkall`. Expected: green.
- [ ] **Step 5: Commit** `feat(p4): river tiles rise; waterfalls form between them`.

### Task 8: Save: `WorldIssue::RiverUphill`, the replay extension and the `v3-waterfall` fixture

**Files:**
- Modify: `crates/hex-core/src/save.rs` (`WorldIssue` line 130, the step-10 world check, `validate_history` line 544, `validate_history_full` line 760, `incremental_history_replay_matches_the_full_sweep` line 1981)
- Modify: `crates/hex-core/tests/save_fixtures.rs` (writer and test), `crates/hex-core/tests/save_props.rs` (river worlds in the round-trip property)
- Create: `crates/hex-core/tests/fixtures/save/v3-waterfall.hexworld` (written once by the `#[ignore]` writer)
- Docs: `docs/save-format.md` (versions table: the fixture beside `v3-massif.hexworld`; decode step 10: `RiverUphill`; history replay: insertions re-check river networks)

**Interfaces:**
- Produces: `WorldIssue::RiverUphill { cell: Axial, edge: u8 }`. Every replay step that inserts a tile (place, replace, and an undone bulldoze in the backward replay) orients the river networks the inserted tile touches; removal-only steps do not.
- Consumes: Task 6 `World::rivers` and the river-legal check.

- [ ] **Step 1: Write the failing tests:**
  ```rust
  // save.rs
  #[test]
  fn a_tampered_uphill_river_is_refused() {
      let bytes = encode_unchecked(&uphill_world()); // L0 - L1 - L0 river chain, built with World::undo-free inserts
      assert!(matches!(SaveData::decode(&bytes, &cat()), Err(SaveError::InvalidWorld(WorldIssue::RiverUphill { .. }))));
  }
  #[test]
  fn replay_rechecks_rivers_on_every_insert() {
      // a played world whose undo stack is edited so undoing a bulldoze re-inserts a tile that makes a fall run uphill
      assert!(matches!(decode(..), Err(SaveError::InvalidHistory(HistoryIssue::InvalidState { .. }))));
  }
  // save_fixtures.rs
  #[test]
  fn v3_waterfall_fixture_derives_its_falls_and_flow() {
      let s = load("v3-waterfall.hexworld");
      let r = s.world.rivers(&cat());
      assert_eq!(r.falls.iter().map(|f| f.drop).collect::<Vec<_>>(), vec![1, 2]);
      // spring at level 2 is the source; the mouth's segment ends at Mouth
  }
  ```
  Extend `incremental_history_replay_matches_the_full_sweep` with random played worlds containing waterfalls and one-action tampering.
- [ ] **Step 2: Run** `cargo test -p hex-core a_tampered_uphill replay_rechecks incremental_history`. Expected: fail. Run the writer once: `cargo test -p hex-core --test save_fixtures write_v3_waterfall -- --ignored` (refuses to overwrite).
- [ ] **Step 3: Implement** the step-10 check (whole-world `rivers` once) and the per-insert network check in `validate_history`.
- [ ] **Step 4: Run** `cargo test -p hex-core` (with `PROPTEST_CASES=1024` once locally). Expected: green, `every_fixture_loads_and_every_version_has_one` re-encodes the new fixture byte for byte.
- [ ] **Step 5: Commit** `feat(save): river legality on decode and replay; v3-waterfall fixture`.

### Task 9: Boat runs end at waterfalls

**Files:**
- Modify: `crates/hex-play/src/boats/plan.rs` (`RunEnd` line 179, `runs` line 221 and its `across` closure line 235, `systems` line 406)
- Test: `crates/hex-play/src/boats/tests.rs`

**Interfaces:**
- Produces: `RunEnd::Fall` ("a waterfall at its end: the boat turns back short of the lip, or short of the pool's foam disc below"). `across` returns `None` for a River edge in the cell's `falls`. Runs above and below a fall are separate systems.
- Consumes: Task 3 `CellWater::falls`.

- [ ] **Step 1: Write the failing tests:**
  ```rust
  #[test]
  fn a_rowing_run_ends_at_a_waterfall() {
      let tiles = [river("river_straight",(0,0),0,1), river("river_straight",(1,0),0,1), river("river_straight",(2,0),0,0), river("river_straight",(3,0),0,0)];
      let runs = runs(&tiles, &cat(), &layout(&tiles, &cat(), |_| true));
      assert_eq!(runs.len(), 2);
      assert!(runs.iter().any(|r| r.ends.contains(&RunEnd::Fall)));
      assert_ne!(systems(&runs)[0], systems(&runs)[1]);
  }
  #[test]
  fn no_boat_moors_in_a_plunge_pool() { /* mooring candidates exclude the lower tile of a fall within 6 m of the seam */ }
  ```
- [ ] **Step 2: Run** `cargo test -p hex-play a_rowing_run_ends_at_a_waterfall no_boat_moors`. Expected: fail (one run crosses the drop).
- [ ] **Step 3: Implement.** The turn-back inset at a `Fall` end is the bridge's (`inset`, line 395) on the upper side and 6.0 m from the seam on the lower side.
- [ ] **Step 4: Run** `cargo test -p hex-play boats` and `make checkall-ci`. Expected: green.
- [ ] **Step 5: Commit** `feat(boats): rowing runs end at waterfalls`. Phase 3 merges to main only with Tasks 5 to 9 complete (no boat ever crosses a drop on main).

### Task 10: Phase 3 evidence and merge (booked: `make ui-e2e`, captures)

**Files:**
- Modify: `e2e/elevation.mjs` (a step placing a spring below a raised river and checking `play.fits`)
- Docs: `docs/agent-operability.md` (refusal code list gains `river_uphill`)

- [ ] **Step 1:** Add the e2e step with Task 6's pinch: a level-0 `river_spring` and a level-1 `river_straight` whose lanes face the same empty cell from opposite sides; select `river_straight`, hover that cell, assert its fits entry carries refusal `river_uphill` and the HUD sheet shows "Rivers can't run uphill.".
- [ ] **Step 2 (booked window):** `make ui-e2e` and `make shots-water` (flat parity). Expected: green, water frames byte-identical.
- [ ] **Step 3:** `make checkall-ci`, merge to main, `make build-web`, commit `docs(p4): river_uphill in agent-operability`.

---

## Phase 4: Waterfall runtime (no fall-face piece yet)

### Task 11: The falling sheet, the plunge pool and mist in the water meshes; `NoFalls`

**Files:**
- Modify: `crates/hex-play/src/water/tiles.rs` (sheet vertices appended to the upper cell's surface mesh in `spawn_cell`; pool churn term in `TileField::churn`; `NoFalls` honoured)
- Modify: `crates/hex-render/src/water.rs` (vertex tag constant beside `MEET_TAG`/`SKIRT_TAG`, lines 263 to 274 and 464), `assets/shaders/water.wesl` (sheet branch near lines 60 to 98 and the foam sum at 244 to 254), `crates/hex-render/src/water_common.wesl` if shared math moves
- Modify: `crates/hex-play/src/play.rs` (`pub struct NoFalls;` beside `NoBoundaries` line 567), `crates/hex-app/src/thumbs.rs` (insert `NoFalls`)
- Modify: `e2e/smoke.mjs` (the `water_kit` case gains a two-level fall)
- Docs: `docs/kit/water-contract.md` §5 (sheet and pool rows; churn exempt at a waterfall seam)

**Interfaces:**
- Produces:
  ```rust
  pub const SHEET_TAG: f32; // UV_0.y offset marking a sheet vertex, disjoint from MEET_TAG and SKIRT_TAG ranges
  pub const PLUNGE_R: f32 = 6.0;     // pool foam and ripple radius from the seam midpoint, lower tile
  pub fn sheet_mesh(upper: &CellWater, fall: FallEdge) -> (Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<[f32; 4]>); // positions, uv0, flow
  #[derive(Resource, Default)] pub struct NoFalls;
  ```
  Sheet: 8 m wide at the lip (the 8 m lane), edges on the channel profile, from `water_y(upper.level)` at the seam down to `water_y(lower.level)`, at least 8 rows per level of drop, flow pointing down the sheet. Pool: churn `>= 0.22` (full white) at the seam midpoint easing to 0 at `PLUNGE_R`, ripple rings and mist band in the same pass. Tier: refraction on the sheet only where its row is within 1.5 m of the lip, and only at High and Ultra (RQ5 O2); Low and Medium draw the sheet opaque white with streaks (RQ5 O1). No new binding, no new vertex buffer beyond the existing attribute layout.
- Consumes: Task 3 `FallEdge`.

- [ ] **Step 1: Write the failing tests:**
  ```rust
  #[test]
  fn sheet_vertices_are_tagged_and_drop_from_lip_to_pool() {
      for drop in 1..=3u8 { let (p, uv, _) = sheet_mesh(&upper_cell(drop), fall(drop));
          assert!(uv.iter().all(|u| u[1] >= SHEET_TAG));
          let (top, bot) = min_max_y(&p);
          assert!((top - water_y(drop)).abs() < 1e-3 && (bot - water_y(0)).abs() < 1e-3);
          assert!((width_at_top(&p) - 8.0).abs() < 0.05);
      }
  }
  #[test]
  fn plunge_churn_is_full_at_the_seam_and_zero_past_plunge_r() { /* lower cell field churn: >= 0.22 at seam midpoint, 0 at 6.01 m */ }
  #[test]
  fn churn_still_fades_at_continuous_seams() { /* existing 3 m fade on River-River seams at equal level unchanged */ }
  #[test]
  fn thumbs_world_draws_no_falls() { /* with NoFalls, spawn_cell emits no sheet vertices */ }
  ```
  The hooks gate (`crates/hex-play/tests/hooks/mod.rs`) must still pass: `water.wesl` keeps every hook.
- [ ] **Step 2: Run** `cargo test -p hex-play sheet_vertices plunge_churn churn_still_fades thumbs_world_draws` and `cargo test -p hex-render`. Expected: fail.
- [ ] **Step 3: Implement** per the interfaces.
- [ ] **Step 4: Run** `cargo test --workspace`, `make kit-check` (hooks gate), `make checkall`. Expected: green.
- [ ] **Step 5 (booked window, GPU):** `make web-smoke` (16-texture clamp with the fall in `water_kit`), and native captures of a 1, 2 and 3-level fall and a fall onto a mouth at High and at Low into `docs/checkpoints/waterfall/runtime/` (manifest rows in `scripts/shots/targets.json`, a `waterfall` script world). Owner reviews these before Task 15 merges.
- [ ] **Step 6: Commit** `feat(water): waterfall sheet and plunge pool`.

### Task 12: Scatter clears the lip and pool; the `waterfall_rocks` rule

**Files:**
- Modify: `crates/hex-play/src/play/scatter/bands.rs` (bank props and `WATER_MAX` props, lines 244 to 400), `crates/hex-play/src/play/scatter/mod.rs` (fall exclusion derived from `World::rivers().falls`, redrawn on a neighbour edit like field boundaries), `crates/hex-play/src/play/scatter/tests.rs`

**Interfaces:**
- Produces:
  ```rust
  pub const FALL_CLEAR: f32 = 6.0;   // no bank prop within 6 m of a waterfall seam, either side
  pub const FALL_ROCKS_MAX: usize = 2; // river_stones clusters round a pool, first value pending the owner's look
  pub const FALL_ROCK_RING: (f32, f32) = (5.0, 6.5); // m from the seam midpoint on the lower tile, on bank floors
  pub fn fall_rocks(seed: u64, fall: &Waterfall) -> Vec<ScatterInstance>;
  ```
  Lily clusters and reed beds keep out of the pool disc (`PLUNGE_R`). Grass margin is Task 4's.
- Consumes: Task 6 `World::rivers`, Task 11 `PLUNGE_R`.

- [ ] **Step 1: Write the failing tests:**
  ```rust
  #[test]
  fn no_bank_props_within_six_metres_of_a_waterfall_seam() { /* 256 seeds, both tiles of a fall: min distance to the seam line >= FALL_CLEAR */ }
  #[test]
  fn lilies_and_reed_beds_stay_out_of_the_plunge_disc() { /* a spring falling onto a bend: 256 seeds */ }
  #[test]
  fn waterfall_rocks_ring_the_pool() { /* 0..=FALL_ROCKS_MAX river_stones* within FALL_ROCK_RING of the seam midpoint */ }
  #[test]
  fn a_flat_river_scatters_exactly_as_before() { /* byte-identical layout over 256 seeds of every river tile at level 0 */ }
  ```
- [ ] **Step 2: Run** `cargo test -p hex-play scatter`. Expected: the three new tests fail, the flat test passes.
- [ ] **Step 3: Implement.** `FOOTPRINTS` needs no row (`river_stones` already has one).
- [ ] **Step 4: Run** `cargo test -p hex-play` and `make checkall`. Expected: green.
- [ ] **Step 5: Commit** `feat(scatter): clear waterfall lips and pools; rocks round the pool`.

### Task 13: `play.falls` state and the waterfall sound

**Files:**
- Modify: `crates/hex-app/src/game.rs` (`state_json` line 1146, beside `"tunnels"` line 1183)
- Modify: `web/ui/src/bridge.ts` (`PlayFall` type and the world state field), `web/ui/src/bridge-schema.json` (`make ui-schema`), `web/ui/src/audio-mix.ts` (fall emitters), `web/ui/src/audio.ts` (voice the cue), `web/ui/test/audio.test.ts`, `web/ui/test/state-publication.test.ts`
- Create (audio, standing approval): `assets-source/audio/ambience/waterfall/` (generation.json, provenance.json, mastered loop), a `waterfall` row in `assets-source/audio/cues.json`, the runtime file the build serves beside `river-babble`; via the `create-game-audio-assets` and `elevenlabs` skills
- Docs: `docs/agent-operability.md` (the `falls` state field)

**Interfaces:**
- Produces:
  ```json
  "falls": [{ "q": 0, "r": 0, "edge": 0, "drop": 2, "x": 13.9, "z": 0.0 }]
  ```
  (upper cell, its world edge, the drop, the pool's world XZ at the seam midpoint on the lower side), sorted by (q, r, edge).
  ```ts
  export interface PlayFall { q: number; r: number; edge: number; drop: number; x: number; z: number }
  export const MAX_FALL_VOICES = 2;
  export const FALL_GAIN: readonly [number, number, number] = [0.6, 0.8, 1.0]; // by drop 1..3
  export function fallEmitters(falls: readonly PlayFall[], v: View): { cue: "waterfall"; x: number; z: number; gain: number }[];
  ```
- Consumes: Task 6 `World::rivers`.

- [ ] **Step 1: Write the failing tests:**
  ```rust
  // hex-app game.rs tests
  #[test]
  fn state_publishes_falls_sorted_with_their_pool() { /* level-2 straight over a level-0 straight: one entry, drop 2, x,z at the seam midpoint */ }
  ```
  ```ts
  // web/ui/test/audio.test.ts
  test("a far fall gets no voice and at most two voices play", () => {
    const e = fallEmitters([fall(1, 10, 0), fall(3, -10, 0), fall(2, 400, 0)], view(0, 0));
    assert.equal(e.length, 2);
    assert.ok(e.every((x) => x.x !== 400));
  });
  test("at equal distance the gain follows the drop", () => {
    const [a, b] = fallEmitters([fall(1, 10, 0), fall(3, -10, 0)], view(0, 0));
    const ratio = Math.max(a.gain, b.gain) / Math.min(a.gain, b.gain);
    assert.ok(Math.abs(ratio - FALL_GAIN[2] / FALL_GAIN[0]) < 1e-6);
  });
  ```
- [ ] **Step 2: Run** `cargo test -p hex-app state_publishes_falls`, `node --test web/ui/test/audio.test.ts`. Expected: fail.
- [ ] **Step 3: Implement** the state field, `make ui-schema`, the TS type and emitters. Generate the loop (positional, short, low and broad, no fan hiss: screen with `scripts/audio/screen.py`), master it, record provenance. Keep the one-shot fallback (RQ10 O2) as a note in `cues.json` only.
- [ ] **Step 4: Run** `make ui-check`, `cargo test --workspace`, `make checkall`. Expected: green.
- [ ] **Step 5 (booked window):** `make audio-e2e`. Then the owner auditions the cue in play (approval by ear, never recorded by the agent).
- [ ] **Step 6: Commit** `feat(audio): waterfall emitters from play.falls`.

### Task 14: The ghost shows its flow (RQ11 O2)

**Files:**
- Modify: `crates/hex-core/src/world.rs` (`World::ghost_flow`), `crates/hex-app/src/game.rs` (ghost JSON gains `flow`), `crates/hex-play/src/play/ghost.rs` (chevrons on the ghost's water using the affordance material, `assets/shaders/affordance.wesl`, no new binding), `web/ui/src/bridge.ts`, `web/ui/src/bridge-schema.json`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
  pub enum FlowDir { In, Out }
  impl World { pub fn ghost_flow(&self, catalog: &TileCatalog, cell: Axial, def: &TileDef, o: Orientation) -> Vec<(u8, FlowDir)>; }
  ```
  One entry per River lane world edge of the ghost, from orienting the world with the ghost placed. Empty for an illegal orientation.
  ```json
  "ghost": { ..., "flow": [{ "edge": 0, "dir": "out" }] }
  ```
- Consumes: Task 6.

- [ ] **Step 1: Write the failing tests:** `ghost_flow_follows_the_spring` (a straight ghosted next to a spring's lane: its lane toward the spring is `In`), `ghost_flow_follows_a_fall` (ghost at level 0 below a level-1 straight: the shared edge is `In`), `ghost_flow_is_empty_when_uphill`, in world.rs. In ghost.rs: `ghost_draws_one_chevron_per_river_lane`.
- [ ] **Step 2: Run** `cargo test -p hex-core ghost_flow`, `cargo test -p hex-play ghost_draws_one_chevron`. Expected: fail.
- [ ] **Step 3: Implement**, `make ui-schema`.
- [ ] **Step 4: Run** `make ui-check`, `cargo test --workspace`, `make checkall`. Expected: green.
- [ ] **Step 5 (booked window):** `make shots-ghost` with a river ghost beside a fall, into `docs/checkpoints/waterfall/ghost/`, for the owner.
- [ ] **Step 6: Commit** `feat(ghost): show the flow a river placement gives`.

### Task 15: Phase 4 evidence, perf and merge (booked: shots, perf)

**Files:**
- Modify: `scripts/shots/targets.json` (a `waterfall-staircase` row: the mixed scatter world plus a three-fall staircase), `docs/checkpoints/waterfall/`, `docs/perf/waterfall/`
- Docs: PRD §10 (the measured overview draws with falls), §15 P4 note (runtime landed, piece pending)

- [ ] **Step 1 (booked window, load-guarded):** overview draws on the mixed river, beach and ocean world before and after adding the staircase, native `make shots-scatter` and WebGPU `make web-scatter`. Expected: no sheet or pool draw added (the counts differ only by the fall-face pieces, none yet). Record numbers.
- [ ] **Step 2 (booked window, load-guarded):** `play.fits` cost on the 2,066-tile save before and after (native, `--script` hover sweep, p95 of the state publish), and a close camera on the 12 m fall at High (`e2e/perf-close-camera.mjs` with the staircase save) against the low-camera baseline. Record load before and after.
- [ ] **Step 3:** Owner reviews the runtime shots from Task 11 and the ghost shots. Fix what the owner asks, re-shoot.
- [ ] **Step 4:** `make checkall-ci`, merge to main, `make build-web`, commit `docs(p4): waterfall runtime evidence and perf`.

---

## Phase 5: Fall-face kit piece (Blender asset work, runs in parallel with phases 2 to 4)

All of Phase 5 uses the **blender-asset** skill (`.claude/skills/blender-asset/`): headless Blender only, the dispatch brief's integration section (`.claude/skills/blender-asset/references/dispatch.md` part 11), `.blend` sources in the main checkout's gitignored `assets-source/blend/<piece-id>/`.

### Task 16 (Blender): References and owner reference approval

**Files:**
- Create: `assets-source/refs/waterfall/` (PLAN.md, ref images, `approval.json` left pending for the owner)

- [ ] **Step 1:** Draft PLAN.md: three pieces `waterfall_l1` (4 m, straight granite lip, RQ2 O1), `waterfall_l2` and `_l3` (8 and 12 m, stepped ledges in two or three tiers, RQ2 O2), one design scaled by drop; granite in the boulder, wall and massif palette, moss and dark wet streaks; face width 8 m channel plus about 3 m each side; sits on the upper tile's cliff column at the seam, from the channel bed down to the lower tile's bank; no baked scatter.
- [ ] **Step 2:** Generate 3 to 4 reference images per height (art skill), contact sheet, queue for the owner. Phase 5 modelling starts only after the owner approves a reference (never recorded by the agent).
- [ ] **Step 3: Commit** `assets(refs): waterfall references for owner review`.

### Task 17: `kit_gate` branch for the fall face, failing test first

**Files:**
- Modify: `crates/hex-play/tests/kit_gate.rs` (a `waterfall_*` branch beside the viaduct's border-ring branch and `BASE_RING_LEVELS` line 110)

**Interfaces:**
- Produces: `fn check_waterfall_face(piece: &str, drop: u8)`: the face's top ring lies on the river channel profile (water contract §2) at the upper level within ±0.03 m below 2.5 m of water and ±0.10 m elsewhere, its bottom ring on the lower tile's bank profile, its side ends flush with the column skirt plane, LOD budgets 4k / 50% / 15%.

- [ ] **Step 1:** Write `waterfall_faces_meet_the_channel_and_the_column` for `waterfall_l1..l3`, and `gate_catches_a_waterfall_face_off_the_channel` (a synthetic mesh shifted 0.2 m fails).
- [ ] **Step 2: Run** `cargo test -p hex-play --test kit_gate waterfall`. Expected: fail (pieces missing; the synthetic case passes the catch test).
- [ ] **Step 3: Commit** `test(kit): waterfall face gate (red until the pieces land)` on the asset branch only.

### Task 18 (Blender): `_waterfall.py` generator and the three pieces

**Files:**
- Create: `scripts/blender/kit/_waterfall.py` (family helper), `scripts/blender/kit/waterfall_l1.py`, `waterfall_l2.py`, `waterfall_l3.py` (set module globals, never edit approved helpers)
- Create: `assets/kit/<pack dir>/waterfall_l*.glb`, KTX2 maps, `assets-source/kit/waterfall_l*/` provenance and `approvals.json`
- Modify: `assets/kit/manifest.json` (pack `water`, slots with `masks`, wetness G near 1 on the face, emissive B 0)

- [ ] **Step 1:** Write the generator per the approved reference: high-poly, LOD0 to LOD2, UVs, Cycles bakes (normal, AO, ORM, masks), collider (one box per ledge), sockets, GLB export.
- [ ] **Step 2: Run** `make kit-build`, `make kit-compress`, `make kit-check` (Task 17 gate now green, hooks gate, budgets), `make kit-provenance`. Expected: green.
- [ ] **Step 3:** Review sheet `docs/checkpoints/waterfall/sheet.jpg` (`make shots-piece`, booked window).
- [ ] **Step 4: Commit** `assets(kit): waterfall_l1..l3 fall faces` on the asset branch, with `docs/checkpoints/waterfall/integration.patch` for Task 19.

### Task 19: Fall faces in play, piece tables and the showcase (booked: shots, thumbs)

**Files:**
- Modify: `crates/hex-play/src/play.rs` (spawn one `waterfall_l{drop}` per `Waterfall` under the upper tile's root at the seam, rotated to its edge, skipped under `NoFalls`, redrawn when either tile changes, gone with either tile)
- Modify (from the integration patch): `crates/hex-play/src/walk/tables.rs` (`OBSTACLES` row for the face collider, `prop_kind` label `"waterfall"`), `crates/hex-play/src/play/scatter/tables.rs` (`NOT_GROUND` for the lip and face slots), `crates/hex-render/src/kit_scene.rs` (`block_waterfalls`: a staircase of 1, 2 and 3-level falls; `tile_counts` in `crates/hex-render/src/lib.rs`)

**Interfaces:**
- Consumes: Task 6 `World::rivers().falls`, Task 11 `NoFalls`, Task 18 pieces.
- Produces: `pub fn fall_face_piece(drop: u8) -> &'static str` (`"waterfall_l1"`, `"waterfall_l2"`, `"waterfall_l3"`).

- [ ] **Step 1: Write the failing tests:** `a_fall_spawns_its_face_under_the_upper_root` and `bulldozing_either_tile_removes_the_face` (play.rs tests), `walker_is_stopped_by_a_fall_face` (walk/tests.rs), and the existing `every_collider_piece_has_a_label`, `every_showcase_piece_is_placed_or_listed` must pass with the new piece.
- [ ] **Step 2: Run** `cargo test -p hex-play fall_face walker_is_stopped_by_a_fall`, `cargo test -p hex-render every_showcase_piece`. Expected: fail.
- [ ] **Step 3: Implement** and land the integration patch.
- [ ] **Step 4: Run** `cargo test --workspace`, `make kit-check`, `make checkall`. Expected: green.
- [ ] **Step 5 (booked window, GPU):** `make thumbs` under `caffeinate -d -u`. Expected: no file dirtied. `make shots-kit` and the `waterfall` shots rows at each drop and onto a mouth, overview draws again (`make shots-scatter`, `make web-scatter`) with the faces; the overview must stay at or under 500, else stop and raise it with the owner.
- [ ] **Step 6:** Owner approves the pieces in engine (never recorded by the agent).
- [ ] **Step 7: Commit** `feat(p4): waterfall fall faces in play`.

---

## Phase 6: Close-out

### Task 20: Docs and merge

**Files:**
- Modify: PRD §15 P4 (tick "Rivers that flow downhill ... Waterfalls at cliff edges", record the measured draws and the RQ decisions already in the spec), README P4 line, `handoff.md`
- Already folded: water contract (Tasks 3, 11), save format (Task 8), agent-operability (Tasks 10, 13), PRD §5.2 (Task 5), PRD §10 (Task 15)

- [ ] **Step 1:** Re-verify per criterion against the spec's Goals 1 to 6, each with its test or capture path.
- [ ] **Step 2:** `make checkall-ci`, merge to main, `make build-web`.
- [ ] **Step 3: Commit** `docs(p4): rivers and waterfalls at levels landed`. Push per the standing rule (significant addition, gate green), then watch CI.

---

## Parallelism and collisions

- **Sequential spine:** Task 1, then 2, then 3, then 4. Task 5 needs Task 1. Task 6 after 5, Task 7 after 6. Task 8 after 6. Task 9 after 3 and 6. Task 10 after 4 to 9.
- **Parallel worktrees:**
  - Task 5 runs beside Tasks 3 and 4 (socket.rs and river.rs against water/tiles.rs and consumers). No shared file.
  - Tasks 16, 17 and 18 (Blender) run from the start beside phases 2 to 4. They touch only `assets-source/`, `scripts/blender/`, `assets/kit/` and `crates/hex-play/tests/kit_gate.rs`.
  - Task 12 (scatter) beside Task 11 (water), after Task 6.
  - Task 8 (save.rs) beside Task 9 (boats/plan.rs).
- **Collisions (sequence these):**
  - `crates/hex-play/src/water/tiles.rs`: Tasks 2, 3, 11.
  - `crates/hex-core/src/world.rs`: Tasks 6, 14.
  - `crates/hex-core/src/river.rs`: Tasks 1, 5.
  - `crates/hex-app/src/game.rs` and `web/ui/src/bridge.ts`, `bridge-schema.json`: Tasks 6, 7, 13, 14 (each runs `make ui-schema`; rebase and regenerate, never hand-merge the JSON).
  - `crates/hex-play/src/play.rs`: Tasks 11 (`NoFalls`), 19.
  - `crates/hex-play/src/play/ghost.rs`: Tasks 3, 14.
  - `crates/hex-play/src/boids/plan.rs`, `walk/ground.rs`: Task 4 only, but Task 19 adds walk tables (different file, `walk/tables.rs`).
  - `docs/kit/water-contract.md`: Tasks 3, 11.
  - `docs/agent-operability.md`: Tasks 10, 13.
- **Booked host windows (orchestrator books):** Task 2 step 5, Task 10 step 2, Task 11 step 5, Task 13 step 5, Task 14 step 5, Task 15 steps 1 and 2, Task 18 step 3, Task 19 step 5.

## Self-review against the spec

- **Coverage:** §1 data model (Tasks 1, 5, 7), flow in hex-core (1, 2), legality (5), reversals RQ12 (5); §2 placement, ranking RQ1, refusal, bulldoze, ghost marker (6, 7), ghost flow RQ11 (14); §3 save (8); §4 geometry: cases table (5, 11, 19), piece (16 to 19), sheet RQ5 (11), pool RQ9 (11), spray (18 masks), cut faces (3); §5 water at levels and the consumer table (3, 4); §6 boats (9), fish RQ6 (4), birds (checked by existing tests, no task), bridges and trains RQ7 RQ8 (5, 7), walker (4, 19), scatter (12), piece tables (19), sound RQ10 (13), thumbnails (11, 19), overlay (2); §7 perf (11, 15, 19); docs (folded). Birds: the spec says nothing new; Task 4's run of `cargo test -p hex-play` covers the existing sky-band tests.
- **Type consistency:** `RiverEnd` moves to hex-core and is re-exported as `water::tiles::RiverEnd` (Task 2), so `boats/plan.rs` keeps its imports. `Waterfall` (hex-core, seam) and `FallEdge` (hex-play, per cell) describe the same seam from two sides; `FallEdge::drop` equals `Waterfall::drop`, and `FallEdge::upper` is `cell == Waterfall::upper`. `HeightMeet::Waterfall` (not `Fall`) avoids `EdgeProfile::Fall`. `orient` changes from `-> Rivers` (Task 1) to `-> Result<Rivers, Uphill>` (Task 5); Task 2's adapter unwraps a flat world's `Ok` and the runtime treats an `Err` (never reachable through placement or decode) as today's rule without falls.
- **Proportion:** the spec's six phases map to 20 tasks. Each task ends green on its own gate, Task 6 included (it carries the two-sided refusal code change). The heaviest are Task 5 (the rule and its oracle) and Task 11 (shader work). Phase 6 docs are folded, leaving one close-out task.
