# Rivers and waterfalls at levels

Date: 2026-11-22. P4, deferral lifted by the owner 2026-11-22.

Status: **approved 2026-11-22.** The owner answered "go with the recommendations" for every item marked *(recommendation)* and for RQ1 to RQ12: RQ1 O1, RQ2 O1 for the 4 m drop and O2 for 8 and 12 m, RQ3 O1, RQ4 O1 (sea-cliff tile is a later card), RQ5 O2 at High/Ultra and O1 at Low/Medium (picked from in-engine options), RQ6 O1, RQ7 O2, RQ8 O1, RQ9 O1, RQ10 O1 (auditioned, O2 fallback), RQ11 O2, RQ12 O1. A41 O1 (PRD §15 P4 decisions) is already decided and is not reopened.

## Intent

Rivers follow the land. A river runs on raised ground, and where it meets lower river across a cliff edge it drops as a waterfall into a plunge pool and flows on. The owner decided the rules on 2026-10-27 (A41 O1): river lanes carry a flow direction, saves load with the flow derived from the sea and height (flat rivers keep flowing as now), new placement refuses a river that would run uphill, and waterfalls form where a river meets a cliff edge. The elevation spec kept cliffs possible "because waterfalls will need them" and held the sea and rivers at level 0 until now.

## Goals

1. River straight, both bends, the fork and the spring place on raised ground (levels 1 to 3), and their water, bed, foam and cut faces stand at the tile's level.
2. A river lane meeting a river lane one, two or three levels lower forms a **waterfall seam**: a falling sheet of water from the upper channel's lip into a foaming plunge pool on the lower tile, in front of a rock face.
3. Every river network has one flow direction per chain, derived from springs, mouths and falls. Flat worlds flow exactly as today.
4. Placement refuses any tile that would make a river run uphill, and the HUD says why (`play.fits`, a new refusal code).
5. Boats, fish, the walker, scatter, sound and the kit showcase all respect falls.
6. Saves stay at format v3 and every existing save loads with the same flow and look.

## Non-goals

- Rivers changing level inside a tile (rapids, or a river ramp on a hillside). A river crosses height only at a fall, as rail crosses it only by viaduct or tunnel (A39 O1). See RQ3.
- Ocean, beaches and river mouths above level 0 (elevation round one, unchanged).
- A river falling straight into the sea off a sea cliff. River meets only River (`data/sockets.ron`), so that needs a new coastal tile. See RQ4.
- Rivers on, across or under mega tiles, hills, peaks or tunnel cover.
- Frozen falls and seasonal flow (P5).
- Fish leaping falls in this round (RQ6).

## Anti-goals

- Any change to a flat world: flow, captures, thumbnails and saves stay byte-identical. This is the parity gate for every phase.
- Scatter baked into a waterfall piece (owner 2026-10-26). Boulders and reeds round the pool are runtime scatter.
- A waterfall that reads as a cut face or a soil wall behind the water.
- A new sampled texture in the kit forward or water pipelines (exactly 16 next to refracting water, ADR 0002, water contract §5).

## What the code does today

- **Sockets.** `EdgeSocket { kind, lane, land, height, profile }` (`crates/hex-core/src/socket.rs`). Profiles are `Flat`, `Rise` and `Fall` (a sloping edge, not a waterfall). A cliff is not a profile: `heights_meet` returns `HeightMeet::Cliff` only for two flat edges **without lanes** at different absolute heights, and `Refused` for any lane edge at different heights. The test `lane_edges_never_cliff` pins that. The one exception is the viaduct: `deck_meet` lets a rail lane carrying `LaneSpec::deck` meet rail at the same absolute top across a cliff seam.
- **River tiles.** `river_straight`, `river_bend`, `river_bend_60`, `river_fork`, `river_spring`, `river_road_bridge`, `river_rail_bridge`, `river_mouth` and `river_mouth_l` carry 8 m River lanes at relative height 0. None sets `raisable`, so `level_allowed` (`crates/hex-core/src/world.rs`) pins them to level 0. Their bases are the channel beds (water contract §5). Bank stones, reeds, lilies and reed beds are runtime scatter with seeded caps (`RIVER_MAX`, `WATER_MAX`).
- **Flow.** Nothing in hex-core, hex-sim or the tile data carries a direction. `oriented_rivers` in `crates/hex-play/src/water/tiles.rs` derives it at presentation time with the rule of water contract §6: from a chain's spring, else from its open end with the lowest `(cell, edge)`, else from its mouth. Junction networks run toward the nearest mouth, else away from the nearest spring, else out through the lowest junction's local edge 0 arm. Boats (`boats/plan.rs`) and the water layout read it. hex-sim's river `Network` (`sim.rs`) is the generic undirected graph.
- **Water height is world-space and level-blind.** `WATER_Y = -1.4` (`water/tiles.rs`). Water meshes spawn at `Transform::from_xyz(c.x, 0.0, c.y)`, `TileField::level()` returns `WATER_Y`, and the bed profiles (`river_height`, `pool_height`) are absolute. A raised river tile would today keep its water at level 0, under its own raised bed.
- **Water continuity is level-blind.** `EdgeWater::continues` says River meets River at any levels. `CellWater::exposed` (cut faces) follows it, `FishWorld::new` (`boids/plan.rs`) floods fish regions across every non-exposed wet edge, and boat runs chain across `oriented_rivers` links.
- **Cliff columns.** A raised tile stands on a cliff column in the meadow skirt soil down to the level-0 floor (`hex-play/src/play.rs`, `slope_ground::SKIRT_SLOT`). A tunnel may open that wall (`play::tunnels` `cut_walls`).
- **Refusals.** `Refusal` (`world.rs`): `InnerSlot`, `Missing`, `DeckEnd`, `Clash`, `FootprintEdges`, `NoFootprintRoom`. `placement_refusal` runs only when no orientation is legal and falls back to `socket_refusal`, which tries level-0 rotations only. `Refusal::codes()` feeds `bridge-schema.json` `enums.refusal`, the `RefusalCode` union in `web/ui/src/bridge.ts` and `REFUSAL_WORDS` in `words.ts`. The HUD reads `play.fits` (`hex-app/src/game.rs`), hex-core being the only rules implementation (ENH-002).
- **Saves.** v3 (`SAVE_FORMAT_VERSION = 3`), the per-tile level byte live with `MAX_LEVEL = 3`. Decode step 10 checks every seam with the socket table, and step 11 replays history with a local, per-footprint check (decoder-cost decision 2026-11-15).
- **Audio.** `web/ui/src/audio-mix.ts` gives every tile with a River edge the positional `river-babble` loop and the river biome bed, computed from the play-state tiles each frame.

## Approaches considered

- **A. Falls form at seams, flow derived in hex-core (chosen).** River tiles become raisable. A River lane against a lower River lane is a new legal seam. The fall is a derived seam feature on the field-boundary precedent (PRD §6, B2): derived from the placed tiles, drawn under the upper tile's root, never saved, left out of thumbnails, redrawn on a neighbour edit. This matches A41: "waterfalls form where a river meets a cliff edge".
- **B. Waterfall tiles with lanes at two heights, as the viaduct does for rail.** Rejected. A fall becomes a palette item the player has to find, needs one tile per drop and per shape (straight, bend, fork, mouth), each with thumbnails, and contradicts "form where a river meets a cliff edge".
- **C. Flow authored on sockets (directional River edges, like Beach `land`).** Rejected. Every river tile would need an upstream and a downstream variant, doubling tiles and thumbnails, and old saves would need a migration to pick one. It also fights "flat rivers keep flowing as now". A derived direction per lane, published with the river network, meets "river lanes carry a flow direction".
- **D. Store flow in the save (v4).** Rejected while flow is fully derivable from tiles and levels. The level byte already carries the height.

## Design

### 1. Data model: river level and flow

- **Rivers rise.** *(recommendation)* Set `raisable: true` on `river_straight`, `river_bend`, `river_bend_60`, `river_fork` and `river_spring`, levels 0 to 3 (top 0, so `level_allowed` admits all four). `river_road_bridge` follows, since raised roads exist (a flag flip). `river_rail_bridge` was held at level 0 by the rail-at-levels spec, so raising it is RQ7. `river_mouth` and `river_mouth_l` stay at level 0 with the beaches.
- **Heights.** Tile definitions are unchanged: every River edge keeps relative height 0, so per-(tile, rotation) thumbnails stay valid (A38 O1). A river's absolute height is its tile's level.
- **The fall seam.** `heights_meet` gains a branch beside `deck_meet`: two flat River lane edges whose absolute ground differs by 1 to 3 levels return a new meeting, called `HeightMeet::Waterfall { drop }` here so it is not confused with `EdgeProfile::Fall`. `edges_compatible` accepts it. Every other lane height mismatch stays `Refused`. `lane_edges_never_cliff` is narrowed to road and rail lanes, and a new test pins the River case. Flow across a waterfall seam always runs from the higher side to the lower.
- **Flow moves to hex-core.** *(recommendation)* A new module `hex-core/src/river.rs` owns orientation. It takes the placed tiles with their levels and returns a direction for every river segment (the `up` and `down` ends `RiverPath` has now) plus the list of waterfall seams `(upper cell, world edge, drop)`. `oriented_rivers` becomes a thin adapter over it. This is required because placement must refuse uphill rivers and hex-core is the only rules implementation (ENH-002).
- **Legality.** A river network is the chains joined at junctions. **A network with no waterfall is always legal** and takes today's rule unchanged. That matters because today's rule accepts flat cases no stricter rule would: a chain between two springs flows into one of them, a chain between two mouths flows out of one, and a fork fed by springs on all three arms is a sink (water contract §6, "the other one then ends the flow"). All of these place today and may be in saves. A network with at least one waterfall is legal when:
  1. every waterfall is crossed from high to low,
  2. no directed cycle passes through a waterfall (a loop that falls somewhere cannot flow),
  3. a spring or a mouth is constrained only where a waterfall's forced direction reaches it along its own chain: a chain whose waterfall flows toward a spring at the chain's end is refused (water would fall onto the spring), and a chain whose waterfall flows away from a mouth at its end is refused (the river would climb out of the sea).
- **Choosing among legal orientations.** A chain with a waterfall has its direction fixed by it. A chain without one takes today's rule from water contract §6, with the fixed chains treated as given. A flat network is untouched, so the result is today's exactly: a parity test runs the new module against today's `oriented_rivers` on every save fixture and on random flat worlds, and a regression test places and decodes the flat cases above (two springs, two mouths, a fork fed by three springs).
- **Reversals.** Today "a chain with a spring never reverses when tiles are placed at its open end; one without can". A chain with no spring or mouth also reverses when a waterfall placed at one end fixes it the other way. *(recommendation, RQ12)* Allow it: such a chain has no source to contradict. Boats re-derive their runs on every placement already.

### 2. Placement legality and refusal codes

- `World::fits` and `legal_orientations` add the legality rule for any orientation that touches a river network: the tile's own lanes and every network it joins. The check floods only the affected networks, so it is linear in the size of the river networks the tile touches.
- **Ranking.** *(recommendation, RQ1)* A waterfall seam counts as a cliff face in `placement_key`, so placement never picks one by default. R/scroll steps to it, as it steps to a cliff today. A river tile dropped beside a raised river takes that river's level with no fall.
- **New refusal `river_uphill`.** `Refusal::RiverUphill`, code `river_uphill`, returned by `placement_refusal` when the tile meets every socket at some orientation but every such orientation breaks the legality rule. It is checked before the `socket_refusal` fallback, since `socket_refusal` only tries level-0 rotations and would return `None`. This is a two-sided change: `Refusal::codes()`, `make ui-schema`, the `RefusalCode` union in `bridge.ts` and a wording in `words.ts` (for example "Rivers can't run uphill").
- **Refused as `river_uphill`:** a chain with waterfalls facing both ways along it, a waterfall whose lower side runs on to a spring (water falling onto the spring), a waterfall whose upper side runs back to a mouth, and a loop containing a waterfall. Through a junction the fixed directions propagate only as far as rule 3 needs, and the builder's oracle (below) settles the rest.
- **Bulldoze and replace.** A removal splits networks. A part with no waterfall left is legal by definition. A part that keeps a waterfall keeps its forced directions, and a removal adds no spring, mouth or cycle, so rules 1 to 3 still hold. Bulldoze therefore needs no new check. Replace runs the placement check.
- **Ghost.** The ghost's faces show a waterfall seam as compatible with a waterfall marker, the way `Face::cliff` flags cliffs today (marker look left to the builder).

### 3. Save migration

- **Format stays v3.** *(recommendation)* The level byte is live, and falls and flow are implied by tiles and levels, as tunnels are. A save written before this work holds every river tile at level 0, so it has no waterfall and derives today's flow (A41: "saves load with flow derived from the sea and height").
- **Fixture.** Add `v3-waterfall.hexworld` (a spring at level 2, waterfalls of 1 and 2 levels, a fork and a mouth) as a same-version fixture like `v3-massif.hexworld`, with a test of its derived waterfalls and flow. `every_fixture_loads_and_every_version_has_one` then holds it byte for byte. Add the row to `docs/save-format.md`.
- **Decode.** Step 10 gains `WorldIssue::RiverUphill` for a tampered file whose rivers break the legality rule. The seam check accepts the new meeting through the socket table.
- **History replay cost.** Uphill is a property of a whole river network, not of one footprint, so it does not fit the local per-step replay behind the 2026-11-15 decoder-cost decision. *(recommendation)* Every replay step that inserts a tile (a place, a replace, and an undone bulldoze in the backward replay) re-checks only the river networks the inserted tile touches, bounded by the 20,000-tile cap. Steps that only remove a tile need no river check (§2). This follows the removal and insertion split that `docs/save-format.md` already uses for the local replay. `validate_history_full` stays the oracle, and `incremental_history_replay_matches_the_full_sweep` gains river worlds with waterfalls.
- A pre-feature build reading a save with raised rivers reports `LevelOutOfRange` (rivers not raisable there), as `docs/save-format.md` already describes for levels.

### 4. Waterfall geometry

| Case | Upper tile | Lower tile | Notes |
|---|---|---|---|
| 1-level drop (4 m) | any raised river tile | river tile one level down | The common case |
| 2-level drop (8 m) | level 2 or 3 | level 0 or 1 | Same piece family, taller |
| 3-level drop (12 m) | level 3 | level 0 | Lip water at y = 10.6, pool at y = -1.4 |
| Onto a mouth | raised river tile | `river_mouth` or `_l` river edge, level 0 | The only route to the sea. The pool sits on the mouth's channel about 14 m from its centre node, before the fan |
| From a spring | raised `river_spring` | river below its edge 0 | The stream runs about 16 m from the pool node at (-2, 0) to the edge-0 lip |
| At a fork arm | raised fork, or a fork below | either | Legality rule 4 applies |
| At a road bridge | `river_road_bridge` as lip or pool | river | The road crosses through the tile centre at 60 degrees to the river, so its centre line passes 12 m from each river edge midpoint, clear of the lip. A pool on the bridge tile sits beside the arch (RQ8) |
| At a rail bridge | as the road bridge | | Only if RQ7 raises it |
| Staircase | one tile per level | | One fall per seam, 27.7 m apart |
| Onto a spring, or into the sea | | | Refused (§2), or not a socket pair (RQ4) |

**Kit piece and runtime water, both** *(recommendation)*:

- **Kit piece: the fall face.** `waterfall_l1`, `_l2` and `_l3`: a granite lip and wet rock face covering the upper tile's cliff column across the channel and a few metres either side, from the channel bed down to the lower tile's bank, so no soil shows behind the sheet. One headless Blender generator, `scripts/blender/kit/_waterfall.py` (A42 O1), builds all three heights from one design scaled by drop, as the viaduct does (RL-Q4 precedent), through the blender-asset skill: references and owner approval, bakes, LODs, collider, sockets, `make kit-build`, `kit-compress`, `kit-check`, `kit-provenance`. It shares the granite palette of the boulders, field walls and massif. It must meet the column skirt and the channel base's border ring without a crack, so an additive `kit_gate` branch and its failing test come before modelling, as for the viaduct. Pack: `water`. Its emissive mask stays 0 and its masks carry wetness near 1 on the face (hooks contract). The piece is drawn under the upper tile's root, so it rises, sinks and bulldozes with that tile, and it goes when either tile goes.
- **Runtime: the falling sheet.** A curved ribbon from the upper channel's water surface at the seam (`level_hi * 4 - 1.4`) over the lip and down to the lower water surface, 8 m wide at the lip, its edges on the channel profile so they meet the banks. It is emitted into the upper tile's existing water mesh with a vertex flag, the way churn rides in `UV_0.y`, so it costs no new draw or material. `water.wesl` reads the flag: fast downward flow-map advection with the existing ripple noise, aerated white foam, no wave displacement, refraction only where the sheet is thin at the lip (RQ5). Water meshes are spawned once with `NoAutoAabb` (PRD §10), so a waterfall appearing or going respawns the upper tile's water mesh, never mutates it.
- **Plunge pool.** The lower tile's water gets a churn foam disc and outward ripple rings within about 6 m of the seam, and a mist band (a soft fade in the same water pass; the engine has no particle system). Water contract §5 forces churn to 0 within 3 m of every seam so continuous seams match. A waterfall seam is not continuous, so it is exempt. *(recommendation, RQ9)* The lower bed is not deepened in this round: the channel base's border ring is fixed by the contract (§6), and the pool reads through foam and ripples.
- **Spray on the face.** The face's wet slot darkens and glosses through the wetness hook (masks G), with no new texture.
- **Cut faces.** On a waterfall seam the upper tile draws the sheet instead of a cut face, and the lower tile draws none (its water edge faces the upper tile's column). `EdgeWater::continues` takes the level, and the layout gets a third edge state beside continuous and exposed: `Waterfall { drop, upper }`.

### 5. Water rendering at levels

- **Every water input takes the tile level.** `TileField::level()` returns `level * LEVEL_STEP + WATER_Y`, the bed profiles add the same offset, and the surface, placeholder bed and cut-face meshes are built at that height, or under a transform lifted by the level (builder's call, one convention for all water). Water entities are not children of the tile root: the staged rise fills them from the tile base (`play/rise.rs` `RiseRole::Water`, `FLOOR_Y = -LEVEL_STEP`) and a bulldozed tile's released water entities sink on their own. Whichever convention is chosen, the fill must start at the raised tile's own base and the sink must follow its root. The terrain shader's grass line and wet band are relative to the water (0.82 to 1.08 m and 0.32 to 0.40 m above it), so the water level reaches them through the mesh, never through a new binding.
- **Raised river beds.** The channel bases lift with the tile root as every kit piece does. Their own skirt from the bed rim to the foot (-4 m relative) sits on the cliff column. On an exposed raised river edge the water cut face runs from the surface to the bed, then the base skirt and the cliff column carry on to the level-0 floor, so an open-ended raised river reads as the approved cut face over soil.
- **Consumers of `WATER_Y`.** Each must use the water level of the cell it samples:

| Consumer | Today | Change |
|---|---|---|
| `water/tiles.rs` surfaces, beds, `TileField::level` | world `WATER_Y` | add `level * LEVEL_STEP` |
| `boats.rs` water height (lines 399, 404) and boat transform (503) | world | per cell |
| `walk/ground.rs` ford depth (531, 546, 644) and `STAGE_DECK` | world | per cell |
| `boids/plan.rs` fish band and ceiling (574 to 645, 950) | world | per cell |
| `play/grass.rs` water margin (814) | world | per cell, for grass on a raised river bank |
| `play/ghost.rs` `water_surface` (built from a level-0 layout of the tile alone, tested at `WATER_Y` at line 1084) | world | lifted by the ghost's level, so a raised river previews its water at its level |
| `kit_scene.rs`, `grass_bench.rs` | level-0 showcase and bench | unchanged, checked |

### 6. Runtime consequences

- **Boats** (phase 3). A run ends at a waterfall (`RunEnd::Fall`): a rowing boat turns back short of the lip as at a line end and never rows onto a pool's foam disc. Runs above and below are separate river systems for the boat count. No boat moors in a pool.
- **Fish.** A waterfall seam is not continuous water, so `FishWorld::new` splits regions there by construction, and each region keeps its own allocation. *(recommendation, RQ6)* No leaping in this round.
- **Birds.** The sky band already lifts over the highest level (`boids/plan.rs`). Nothing new, checked by a test.
- **Cars and trains on bridges at levels.** A raised road bridge carries cars at its level through `WalkGround::ride_car`, as raised roads do. A raised rail bridge (RQ7) rides `WalkGround::level_y`. A bridge meets road or rail only on its own level, so its river and its road share a level.
- **Walker.** It fords raised rivers at their level (the ford depth fix in §5). At a waterfall seam it meets the cliff as a wall, as at any cliff. The fall face collider blocks the rock. The sheet has no collider and is out of reach. In the pool it fords at `MAX_WADE`.
- **Scatter** (never baked, owner 2026-10-26). River scatter is tile-local, so bank stones and reeds follow a raised tile. New rules: no bank props on a strip about 6 m deep each side of a waterfall seam, lily clusters and reed beds kept out of the pool's foam disc, and an optional rule placing the existing `river_stones` clusters round the pool's edge, derived from waterfall seams. Grass keeps its margin above each cell's own water.
- **Piece tables** (CLAUDE.md "Piece tables"), returned by the asset agent as `docs/checkpoints/waterfall/integration.patch`: `OBSTACLES` (the face collider), `prop_kind` (a label, or `every_collider_piece_has_a_label` panics), `NOT_GROUND` (the lip and face are not walker or scatter ground), a `FOOTPRINTS` row if it is classed as a hero or scatter piece, and in `kit_scene.rs` a `block_*` showing a staircase of 1, 2 and 3-level falls with `tile_counts` updated, or an entry in `NOT_SHOWCASED` with its reason. `CAR_GROUND` and `HERO_VARIANTS` are not touched.
- **Sound.** River babble stays per river tile. A waterfall gets its own cue. The engine publishes waterfall seams in play state (`play.falls`: cell, edge, drop and pool position), a bridge-schema change (`make ui-schema`, `bridge.ts`). Generated under the standing audio approval, approved by the owner by ear. Loop or one-shots is RQ10, since the owner hears generated loop beds as fan noise.
- **Thumbnails.** River tiles are thumbnailed at level 0 per (tile, rotation), and waterfalls exist only between placed tiles, kept out of thumbnails by a `play::NoFalls` marker as `NoBoundaries` and `NoBoats` are. `make thumbs` must dirty nothing.
- **Debug overlay.** The routing-graph overlay draws flow arrows on river segments and marks waterfall seams.

### 7. Perf budget impact (PRD §10)

- **Textures.** No new sampled texture anywhere. The sheet and the pool are water-pass math on existing bindings, and the fall face is a `KitMaterial` slot with the standard maps. `make web-smoke` (device clamped to 16 per stage) gets a waterfall in its `water_kit` case.
- **Draws.** The sheet and pool ride the existing water meshes, so no extra draw. Each fall face piece and LOD in view adds draws in the main opaque, prepass and shadow passes, about 3 to 6, shared across seams that use the same piece through Bevy's automatic instancing. The overview gate is 500 draws, and the mixed river, beach and ocean world already measures 487 to 549 (tracked card on shared scatter materials). *(recommendation)* Measure that world plus a three-fall staircase at the overview in `make shots-scatter` and `make web-scatter` before merge and report before and after counts.
- **Triangles.** Fall face LOD0 at most 4k triangles, LOD1 50%, LOD2 15%.
- **Size.** The piece streams in the `water` pack, so the 30 MiB initial download is unchanged. KTX2 at most 1.5 MB per piece.
- **CPU.** Orientation runs once per change of the placed tiles, never per frame, and the placement check floods only the touched networks. Measure `play.fits` cost on the 2,066-tile save before and after.
- **GPU.** The sheet adds overdraw face-on. Time a close camera on a 12 m fall on the perf host under the load guard (1-minute load at most 15 before and after), against the low-camera baseline.

## Phases

Each phase ends with `make checkall-ci` green, a local commit, `make build-web` after the merge to main, and the flat-world parity check (captures, thumbnails, fixtures, flow). Captures go in a booked load-sensitive window.

Dependency: the thumbnail parity check needs the open high card on `make thumbs` drift to close first (handoff 2026-11-18: `make thumbs` on main regenerates 250 drifted files). Until then "`make thumbs` dirties nothing" cannot be verified, and the phases compare against a fresh baseline run on main instead.

1. **Flow in hex-core, no level change.** Add `hex-core::river` with today's rule, switch `oriented_rivers` to it, draw flow on the debug overlay. Verify: the parity test against the old function on every fixture and on random flat worlds, and byte-identical water scenario captures.
2. **Water at levels.** Water inputs take the tile level (§5 table), and `EdgeWater::continues` takes the level and the waterfall edge state. No tile is raisable yet. Verify: unit tests at levels 1 to 3 for surfaces, beds, cut faces, ford depth, boat height and the fish band, and a flat-world capture parity run.
3. **Raised rivers and the waterfall rule.** `raisable` on the five river tiles and the road bridge, `HeightMeet::Waterfall`, the legality rule, `Refusal::RiverUphill` with `make ui-schema`, `bridge.ts` and `words.ts`, the ranking (RQ1), `WorldIssue::RiverUphill`, the replay extension and `v3-waterfall.hexworld`. Waterfalls draw a plain cut face for now. Boats end their runs at waterfalls here (`RunEnd::Fall`, moved forward from phase 4), since `boats/plan.rs` `runs()` chains through `oriented_rivers` links with no level check and would otherwise row over a 4 m drop. Verify: a hex-core test per refused case in §2, a `play.fits` e2e step showing `river_uphill`, the fixture test and the replay oracle test.
4. **Waterfall runtime without the piece.** The sheet, the pool foam and mist, split fish regions (checked), scatter exclusions, `play.falls` and the sound, `NoFalls`. Soil still shows behind the sheet. Verify: boats and fish unit tests, owner in-engine shots of each drop height, and the §7 measurements.
5. **Fall face kit piece** (an asset agent, alongside phase 4). References (RQ2), the `kit_gate` branch and its failing test, the `_waterfall.py` generator, gates, the integration patch and the showcase block. Verify: the review sheet and in-engine shots for owner approval before merge, `make kit-check` green, `make thumbs` dirtying nothing.
6. **Docs.** Water contract (§1 levels, a waterfall row in §5 and §6, the churn exemption), `docs/save-format.md` (fixture row, river issue), `docs/agent-operability.md` (`play.falls`, the refusal code), PRD §5.2 (the lane exceptions), PRD §15 P4 (the rivers and waterfalls box) and the README P4 line.

## Owner questions

- **RQ1. Default level when a waterfall is possible.** O1: a waterfall counts as a cliff, never picked by default, R/scroll reaches it, as cliffs today. O2: placement prefers continuing the river at its own level but picks a waterfall over a grass cliff. **Recommended: O1.**
- **RQ2. Fall face look.** O1: grey granite ledges in the boulder and massif palette, moss and dark wet streaks, a straight lip. O2: stepped ledges that break the sheet into two or three tiers. O3: a soil and turf bank with exposed roots. **Recommended: O1 for the 4 m drop and O2 for 8 and 12 m, one generator, through reference approval.**
- **RQ3. Rapids.** O1: rivers change level only at waterfalls in this round. O2: also a river ramp tile on the straight-bank hillside, with white water down a 14% channel. **Recommended: O1**, since a ramp is a second river family with its own bed contract.
- **RQ4. Waterfalls into the sea.** O1: rivers reach the sea only through a level-0 mouth, and a waterfall may land on the mouth's river edge. O2: a new sea-cliff tile (raised land meeting ocean at a rock face) with a waterfall straight into the surf. **Recommended: O1 now, O2 as a later card.**
- **RQ5. Sheet look.** O1: a thick, mostly white sheet with streaks, readable from the play camera. O2: a clear, refracting sheet at the lip turning white below. **Recommended: O2 at High and Ultra, O1 at Low and Medium**, chosen from in-engine options as the water foam was (A3).
- **RQ6. Fish at waterfalls.** O1: fish stay in the region on their side. O2: trout sometimes leap a waterfall upstream on a scripted arc between the two regions. **Recommended: O1 now, O2 later.**
- **RQ7. River rail bridge at levels.** O1: it stays at level 0, as the rail-at-levels spec set. O2: it becomes raisable with the road bridge, since raised rail exists. **Recommended: O2.**
- **RQ8. Waterfalls at bridges.** O1: a bridge tile may be the lip or the pool. O2: only the lip (a pool beside the arch is refused). O3: no waterfall on a bridge tile. **Recommended: O1**, checked in engine for the pool beside the arch.
- **RQ9. Plunge pool depth.** O1: no bed change, the pool reads through foam and ripples. O2: new deepened-pool base variants for every river tile a waterfall can land on. **Recommended: O1.**
- **RQ10. Waterfall sound.** O1: a quiet positional loop at each pool, gain by drop, at most two voices near the focus. O2: occasional one-shots (a splash surge), like land ambience. O3: a low loop with occasional surges. **Recommended: O1**, auditioned against the fan-noise concern, with O2 as the fallback.
- **RQ11. Showing flow to the player.** O1: no flow indicator in play (the water's motion shows it), arrows only in the debug overlay. O2: the ghost shows the flow its placement would give. **Recommended: O2**, since the new refusal depends on a direction the player cannot otherwise predict.
- **RQ12. A chain that reverses.** O1: a flat chain with no spring or mouth may reverse when a waterfall is placed at one end, and boats and foam re-derive. O2: once a chain has a direction it never reverses, and placing against it is refused. **Recommended: O1.**

## Left to the builder

- Whether the water lift lives in mesh vertices or the transform (one convention for all water).
- The sheet's curve, thickness and lip overhang, within the channel profile at the lip.
- The junction rule's tie-breaks once waterfalls fix some chains, checked by a brute-force oracle over small networks.
- The waterfall marker on the ghost and the overlay arrows.
- The scatter exclusion shape at the lip and pool.

## Where the code contradicts the PRD and earlier specs

1. **"River lanes carry a flow direction"** (PRD §15 P4 box and A41 O1). No lane carries one. `LaneSpec` has `offset`, `width` and `deck`, flow is derived in hex-play (`oriented_rivers`), and water contract §6 says "Kit pieces carry no flow direction". This spec keeps flow derived (approach A), which also meets A41's saves clause.
2. **PRD §5.2: "a lane edge (road, river, rail) always meets at equal absolute height."** `deck_meet` already lets a viaduct's rail lane meet rail across a cliff seam, and this spec adds the River waterfall. §5.2 needs both exceptions.
3. **Elevation spec: "cliffs stay possible because waterfalls will need them."** A cliff is accepted only between laneless flat edges (`lane_edges_never_cliff`), and every river edge has a lane, so no existing cliff can host a waterfall. Waterfalls need the new meeting.
4. **Elevation spec "Levels": profiles `Flat`, `Cliff` and `Ramp`.** The code has `Flat`, `Rise` and `Fall`, and a cliff is not a profile (PRD §15 P4 matches the code). `EdgeProfile::Fall` (a falling edge) also collides with "waterfall", hence the name `HeightMeet::Waterfall`.
5. **A waterfall into the sea.** A41 and the P4 box imply waterfalls wherever a river meets a cliff, but River meets only River in `data/sockets.ron`, so a river reaches the sea only through a level-0 mouth (RQ4).
6. **CLAUDE.md: "hex-sim: networks (roads/rivers)"** (a nuance, not a contradiction). hex-sim does hold an undirected river `Network`, but all river orientation, junction and spring logic is in `hex-play::water::tiles`.
7. **Elevation spec: "the sea and rivers stay at level 0; water skirts unchanged".** True as data (rivers are not raisable), but every water height is the world constant `WATER_Y`, so raising a river is a water rework (phase 2), not a flag flip.
