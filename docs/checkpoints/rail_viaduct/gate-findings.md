# rail_viaduct: gate findings (stub probe, 2026-11-14)

## Probe

A rough `rail_viaduct_l2` went through the gate path before any bake:

1. `HEXW_SKIP_BAKE=1 make kit-build PIECE=rail_viaduct_l2`: exit 0. Tris per LOD were [3224, 1546, 474].
2. `make kit-compress PIECE=rail_viaduct_l2`: exit 0.
3. The l2 entry went into `assets/kit/manifest.json`, and l2 came out of `data/pending-pieces.ron`.
4. `cargo test -p hex-play --test kit_gate -- --skip provenance`: exit 0, with 66 passed and 2 ignored.
5. `cargo test -p hex-play --lib`: exit 0, with 347 passed. That run includes `trains::tests::a_train_on_a_viaduct_rides_the_deck`, which removing l2 from the pending list switched on.

No gate tripped on the deck height. Under the brief's rule, this is not the stop case.

## Why nothing trips, and what is unguarded

- **Sockets.** `kit_gate::socket_errors` (line 324) holds a lane socket to the lane contract at runtime y = 0 (tolerance 0.002) and never reads a `deck` extra. The viaduct's `socket-lane-0` (edge 0) and `socket-lane-1` (edge 3) sit at y = 0 on the lane centre, and they carry `deck: N` as an extra alongside `kind: rail`, `edge`, `offset: 0` and `width: 3.5`. A socket placed at the deck height (y = N x 4) would fail this check.
- **Border ring.** A centre piece's ring check is LOD sameness only. The viaduct's stone stops 2 mm inside the border (`X_END = APOTHEM - 0.002`), so only the track reaches a border.
- **Rail joints.** `lane_joint_errors` (line 1454) checks a centre lane against the edge pieces of the lane's kind. No rail edge pieces exist, so no rail lane on any piece is held to a neighbour's section.
- **The deck section itself.** No gate checks that a viaduct's border section is the rail section lifted by N x LEVEL_STEP, or that its rail top stands at N x 4 + 0.46. The generator asserts both on every LOD (`_rail_viaduct.deck_section_errors`), and it records them in the bake report's `viaduct` block (`railTopOverTileM`, `ballastTopOverTileM`). Nothing on the Rust side enforces them.

## Proposed additive gate branch (orchestrator's call)

This branch applies to a centre piece whose tile declares a lane with `deck: Some(N)`, N > 0 (`LaneSpec.deck`). The branch reads the lanes from `center_lanes` / `tile_lane_kinds`, as the tile defines them. For every such lane k, in every LOD, it checks:

1. The set of (t, y) points of the vertices on lane k's border plane (|x' - APOTHEM| < 1e-3 after rotating edge k onto edge 0), rounded to 0.1 mm, equals `_rail.section()` with every y raised by N x 4.0. That is the ballast and both rail profiles of `rail_straight_center`, lifted.
2. The highest vertex within 0.1 m of the lane centre line stands at N x 4.0 + 0.46 (`RAIL_TOP_Y`) to within 1e-3.
3. No vertex lies within 1 mm of any border plane outside |t| <= SINK_T on its deck lanes. Stone, parapets and coping stay inside the border, so nothing pokes through the neighbour's column face.
4. Optional, and only if the orchestrator wants sockets to state the meeting height: the socket y equals deck x LEVEL_STEP, with `socket_errors` taking its `want[1]` from the `deck` extra (default 0).

The failing test this branch needs is a viaduct whose track is not lifted, which would fail check 1 at every LOD.
