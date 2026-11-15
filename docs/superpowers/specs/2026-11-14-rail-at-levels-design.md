# Rail at levels: raised rail, rail tunnels, viaducts (P4, deferred work lifted 2026-11-14)

Status: approved by the owner 2026-11-14 ("approved") in chat, after the short design in the same conversation.

## Intent

Rail follows the land. Players put track on raised ground, drive trains into hills through tunnels, and span a low valley on a stone viaduct. The owner lifted the P4 deferral on 2026-11-14. The look and layout decisions were made on 2026-11-13 (RL-Q1..Q4, `assets-source/refs/rail_tunnel_portal/approval.json`, `assets-source/refs/rail_viaduct/approval.json`) and are not reopened.

## Goals

- Rail straight, bend 60, bend 120 and junction place on raised ground, and trains ride at the edge level.
- A rail tunnel runs under one level of raised Meadow between two facing rail portals, as the road tunnel does.
- A viaduct carries rail over a lower tile between two raised neighbours at level 1, 2 or 3.

## Non-goals and anti-goals

- No rail ramps: rail never changes level inside a tile (A39 O1). Height is crossed only by tunnel or viaduct.
- No walking on a viaduct deck from a raised neighbour (RL-Q3). Piers are obstacles, and the deck is not ground.
- No second viaduct design for 12 m (RL-Q4). One form scales by pier height.
- The station, level crossing and river rail bridge stay level 0 only.
- A road portal never links to a rail portal. A tunnel joins portals of the same network kind.
- Rivers, waterfalls and flow direction at levels stay deferred (A41).
- Anti-goal: any change to a flat world. A flat world stays byte-identical in saves, captures and thumbnails.

## Constraints

- RL-Q1 O2: the rail portal head wall stands about 2 m above a one-level hill under a turf mound, its face about 6 m inside the tile edge. The steam engine is 4.66 m tall and 2.7 m wide, so the arch crown is about 5.0 m and the wall top about 5.9 m. One level is 4.0 m (`LEVEL_STEP`). The rail top is 0.46 m over the tile (`RAIL_TOP_Y`).
- Viaduct rail top must equal the raised neighbours' rail top exactly: y = deck level x 4.0 + 0.46.
- Save format v3 stays unchanged: the level byte is already live, and `rail_tunnel_portal` and `rail_viaduct_l*` are new tile ids, which need a fixture only if a format version changes.
- Kit rules: assets are headless Blender generators, with the owner approving each look in engine before merge. New pieces carry the integration section of the dispatch brief (`.claude/skills/blender-asset/references/dispatch.md` part 11).
- A viaduct tile has two rail edges at the neighbours' upper level and four edges at its own low level. That conflicts with the border-ring contract in `kit_gate` and with `heights_meet`, so the gate branch and its failing test come before modelling.

## Approaches considered

- A. Three phases in dependency order: raised rail, then tunnel, then viaduct (chosen). Each phase is shippable and gated. Phases 2 and 3 each have a Blender asset agent that can run alongside runtime work.
- B. Viaduct first. Rejected: its deck ends meet raised neighbours that carry rail, which does not exist until phase 1.
- C. One branch for all three. Rejected: a failing gate in the viaduct blocks the tunnel, and the asset work is independent of the runtime work.

## Phases

1. **Rail on raised ground (no new assets).** Set `raisable: true` on `rail_straight`, `rail_bend_60`, `rail_bend_120` and `rail_junction`. `trains.rs` takes the rail edge's level for the rail top. Picking, the simulation graph, the saves, boids lift and affordances follow, as they do for roads. Tests: place rail on raised Meadow and run a train across it, plus a flat-world parity check.
2. **Rail tunnels.** Generalize `hex-core/src/tunnel.rs` so a portal pairs only with a portal of the same network kind (rail with rail, road with road). Add the `rail_tunnel_portal` tile on the cutting shape with `tunnel: Some(...)`, whose arch node sits about 6 m inside edge 0. The Blender agent builds the portal piece from `kit/_tunnel_portal_rail.py`, which sets the module globals of `_tunnel_portal.py` and never edits it. Trains fade out and in at the face and link through the rail graph between facing portals. The walker stops at the face. Scatter rows follow the road portal (`GROUND_PROPS`, `NOT_GROUND`, the grass list).
3. **Viaducts.** An additive `kit_gate` branch with its failing test, then the hex-core edge-height support for a tile whose rail edges sit above its own level. Add `rail_viaduct_l1`, `_l2` and `_l3` tiles and one generator (`kit/_rail_viaduct.py`) that builds all three pieces, with a world-space tileable granite for the faces. Pier colliders go in `OBSTACLES` and footprints in `FOOTPRINTS`. The deck and parapet slots go in `NOT_GROUND`. The train follows the deck level.

## Verification

Each phase ends with `make checkall-ci` green, a local commit and a fresh `make build-web` after the merge to main. Pieces get a review sheet and in-engine shots for the owner before they merge. Perf-sensitive work books a host window, and the flat-world capture parity is re-checked at every phase.

## Left to the builder

Tile edge-height encoding details for the viaduct, the exact portal mound profile, the order of the tunnel fade and the graph link in `trains.rs`, and thumbnail regeneration.
