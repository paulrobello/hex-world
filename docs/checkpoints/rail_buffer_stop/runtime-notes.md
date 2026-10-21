# rail_buffer_stop: the runtime contract

The piece (`scripts/blender/kit/rail_buffer_stop.py`, owner D2 of 2026-10-20, ref-2) is drawn at open rail ends by the rail runtime. The runtime is not part of this asset pass. This file is what the runtime needs. All numbers come from the generator and its `bake-report.json` (`bufferStop`). `docs/checkpoints/rail_buffer_stop/poses.py` reproduces the placement numbers.

## 1. Placement: one rule for every open end

An open end is a rail lane edge of a placed tile (or of the placement ghost) whose neighbour cell has no rail lane back. That is the same seam hex-sim treats as a line end (`rail.rs`: "a seam facing an empty cell").

Let `u` be the path distance from the edge midpoint into the tile, along the rail centreline that trains run (`LanePath`, `pose_at_distance`).

- **Pivot.** The track point at `u = STOP_U = 3.5 m`, at the tile's cap height: runtime `y = 0`, the same y as the rail centre piece. The piece carries the 0.46 m rail top in its own vertices.
- **Heading.** The piece's local +X (runtime +X; Blender +X) points along the chord from `P(3.5)` to `P(3.5 - CHORD)`, with `CHORD = 2.24 m`. That is toward the edge, to the track point 1.26 m in. On a straight this chord is the tangent. The buffers look along local -X, at the train.
- **No mirroring.** The piece is symmetric about its local XY plane, so only the yaw matters: `yaw = atan2` of the heading.

Why the chord and not the tangent: on the 8 m bend, the tangent at the pivot leaves the brace feet 0.39 m off their rails. The chord keeps every foot that rests on a rail head within 0.038 m of that rail's centreline. The rail head is 0.08 m wide. The generator asserts this fit (`bend_fit`, at most `HEAD_W / 2`).

| End | Rail piece, edge | Pivot in the piece's frame (runtime x, z) | yaw_deg (kit_scene convention, +90 maps +x to +z) |
|---|---|---|---|
| straight | `rail_straight_center` edge 0 (edge 3 by symmetry) | (10.356, 0.0) | 0.0 |
| bend | `rail_bend_60_center` edge 0 | (10.467, 0.753) | -17.05 (the tangent there is -25.03) |
| bend, other end | `rail_bend_60_center` edge 1 | (5.886, 8.688) | 77.05 |
| junction straight leg | `rail_junction_center` edge 3 | (-10.356, 0.0) | 180.0 |
| junction branch | `rail_junction_center` edge 1 | as the bend's edge 1 (the same 8 m arc about corner 0): (5.886, 8.688) | 77.05 |
| junction toe | `rail_junction_center` edge 0 | on the main (straight) line: as the straight | 0.0 |
| bridge, crossing | `river_rail_bridge_center` edges 1 and 4, `rail_road_crossing_center` rail edges 0 and 3 | as the straight (level track, 0.46 m rail top) | as the straight |

For any other edge k, compose with the tile's rotation as for every piece (`piece_rotation`). The kit showcase places two stops this way (`kit_scene` `BUFFER_STOPS`).

Checked fits, measured by `docs/checkpoints/rail_buffer_stop/end_fit.py` against each track piece's own LOD0 (its generator's stage GLB):

| End | Feet off a rail-head top | Stop triangles intersecting the track |
|---|---|---|
| straight (edge 0) | 0.000 m | 0 |
| bend edge 0 | 0.000 m | 0 |
| bend edge 1 | 0.000 m | 0 |
| junction toe, main line (edge 0) | 0.000 m | 0 |
| junction branch end (edge 1) | 0.000 m | 0 |
| junction straight end (edge 3) | 0.000 m | 0 |

- "Feet off a rail-head top" is the worst plan distance from a foot centreline point to the nearest `rail_head` top face, so 0 means every foot point is over a head.
- The intersection test lifts the stop 1 mm, so resting contact does not count. As a control, the same test with the stop sunk 5 cm finds 70-82 intersecting triangles at every end, all on `rail_head` and `rail_steel`.
- At the toe, the legs stand at u 3.2-3.4 m and the brace feet at u 1.1-1.55 m. The -y brace foot rests over the end of the switch blade's taper (tips at 0.25 m, full head at 1.45 m) and still clears it.
- The point lever (u 0.4-1.2 m, 2.35-2.85 m off the main line) is outside the stop's ±1.35 m footprint.
- The nearest any vertex comes to the hex border is 1.10 m on a straight and 0.84 m on a bend. Nothing reaches the skirt.
  - A train coming off the branch stops on the branch path, which runs 0.77 m off the main line at u = 3.5 m. It stands beside the stop, not against it. This is open for the rail runtime and the owner (Q3 below).
- **Station tile.** The platform coping is at 1.66 m off the centreline. The stop's beam ends at 1.35 m. They do not touch.

## 2. Where trains stop (needs work: rail runtime)

The owner's Q2 answer is a 3.5 m stop: `hex-sim` `rail::END_MARGIN` goes from 1.0 to 3.5. Spawning uses the same constant.

- **Buffer contact plane.** This is the pivot plane, local x = 0. The buffer head faces lie in it.
- **Buffer heads.** Discs of radius 0.20 m, centred at local runtime (0, 1.51, ±0.85). That is 1.05 m above the 0.46 m rail top and 0.85 m either side of the centreline. These are the engine's and both carriages' `BUF_Z` and `BUF_Y`, so the brake third meets the stop the same way when it leads.
- **The stop's face inside the edge.** On a straight, the buffer faces sit exactly 3.5 m inside the edge, and the engine (axles on the track, body on their chord) stands with its buffer heads touching the stop's.
- **Bend ends: the engine cannot square up (owner question Q1 below).** With its outer axles on the 8 m arc at u 5.97 and 9.97 m, the engine body lies on their chord, as `Railway::poses` places it. Its buffer-face centre ends up:
  - 0.45 m short of the stop's contact plane;
  - 0.89 m to the side;
  - skewed 40° to the stop (32° to the track tangent).
  - One engine buffer head sits 0.13 m past the contact plane, between the stop's buffers in front of the beam (0.27 m clear of the beam). The other is 1.66 m off to the side. Nothing interpenetrates, but the buffers do not meet.
  - The bend shots (`bufstop-bend-*`) show this as it is.
  - A leading carriage (half wheelbase 1.5 m) is 0.63 m off and skewed 26°.

## 3. Footprint and walker collider (needs work: rail runtime)

- **Collider.** The GLB carries `rail_buffer_stop_collider`, the LOD2 axis-aligned box: local runtime x ∈ [0, 2.488], z ∈ [-1.35, 1.35], y ∈ [0.46, 2.12].
- **LOD0 extent.** x ∈ [0, 2.40], z ∈ [-1.35, 1.35], y ∈ [0.46, 2.30] (lamp ring top).
- **In the tile.** The footprint runs from u = 1.01 m to u = 3.5 m, ±1.35 m across the track.
- **Walker.** Walk mode blocks the walker only at hero and scatter instances listed in `walk::OBSTACLES` (`WalkGround`). A runtime-spawned stop needs its own obstacle: `shape("rail_buffer_stop")` placed with the stop's transform, as `place` does for heroes.
  - `prop_kind`: adding `rail_buffer_stop` to `OBSTACLES` alone fails the walk tests. `walk.rs` `every_obstacle_reports_its_own_kind` (its loop over `OBSTACLES`) requires every obstacle whose id does not start with `rock_` to report a kind other than `rock`, and the stop would fall through to `rock`. So the change that adds the obstacle must add a `prop_kind` mapping in the same change. The clean mapping is a new kind such as `buffer_stop`. It is two-sided: add it to the `PropKind` union in `web/ui/src/bridge.ts`, which walk mode's blocked reason and photo mode's focus hit carry. The patch adds neither, because the obstacle only takes effect once the runtime spawns the stop.
- **Grass.** The stop stands inside the ballast top (±1.35 m against the 1.75 m shoulder). Grass grows only on grass-tileable slots, so nothing needs clearing (no `GROUND_BOXES` row).

## 4. Ghost (owner Q3: yes; needs work)

Draw the stop at the ghost's open rail edges with the same rule, from the ghost tile's rail path at the hovered rotation.

- An edge that faces a placed rail tile gets no stop.
- Placing a rail tile next to an existing open end removes that end's stop.

## 5. Thumbnails (owner Q4: excluded)

The thumbnail renderer draws each tile alone, so every rail edge in it is open. It must not spawn stops. With that, `make thumbs` stays byte-reproducible and no rail thumbnail changes.

## 6. Sound (owner Q5: a soft buffer clunk; the orchestrator generates it)

- **Event.** One positional one-shot when a train comes to rest at a line end, at the moment its leading buffers reach the stop. That is the moment `Blocker::LineEnd` brings it to 0 m/s at `END_MARGIN`, before the `DWELL_SECONDS` wait and the run back.
- **Position.** The stop's pivot.
- **Sound.** A soft, low, damped timber-and-steel "clunk" of about 0.3-0.5 s: two sprung buffers taking up, no squeal. Played once per arrival, at about the level of the crossing gate cues.
- **Optional.** A fainter second clunk as the couplings take up when the train pulls away.
- **Bridge.** A `trainAtLineEnd { train, cell, edge }` event would carry it, or the sim's existing line-end stop can raise it.

## 7. Draw budget

- **Per stop.** Three primitives per LOD: `rail_buffer_stop_paint`, `_steel` and `_glass`. That is three draws per visible stop (plus shadow passes). A line has at most two ends.
- **Triangles.** 938 / 298 / 108 per LOD. LOD2 is 11.5% of LOD0 against the 15% ceiling; the floor formula gives LOD0 ≥ 720.
- **Textures.** One 512² masks map: BC7 123,888 B, ASTC 119,172 B, piece class.
- **Night.** The lamp lens's masks B = 1, so it glows red at night (`night × B × 4` on the slot colour). That is one emissive surface, with no light source.

## Open owner questions

1. **Bend ends.** The engine stops 0.9 m to the side of a bend-end stop, skewed 40° (section 2). Two options:
   - Accept it. Bend ends only occur where a line stops on a curve.
   - Have the rail runtime stop a train at a curved end so that its body front, rather than its path point, reaches the stop's contact plane. That still leaves the skew.
2. **The station tile's platform end.** The ramp runs to 0.56 m inside the border, so a stop at a station's open end stands beside the platform ramp. That is fine to the eye. Confirm it is wanted there.
3. **Junction toe.** A train coming off the branch stands 0.77 m beside the stop. Should the toe stop be placed on the branch path instead, or should trains on the branch stop short?

## As built (rail runtime, 2026-10-20)

- **Placement.** `hex_core::rail_end` holds the rule in the tile frame (`buffer_stops`, `open_stops`). Its test matches every row of the table in section 1 within 2 cm and 0.2°. The junction toe stop stands on the main line. Placed tiles draw their stops under the tile root with the pair edge pieces (`play::respawn_pairs`), so they rise with the tile and are redrawn when a neighbour changes. The thumbnail renderer runs the same play sync, so `--thumbs` inserts `play::NoBufferStops` and that path draws no stops (owner Q4). This guard is checked in code only, because `make thumbs` was not run.
- **Ghost.** The ghost draws a stop at each rail edge whose world edge is not `Compatible` (owner Q3; `ghost::open_mask`).
- **Stopping.** `END_MARGIN` is 3.5 m, and spawning uses it. At an end, `Railway` stops the train when its foremost leading buffer head (disc included) reaches the stop's contact plane, with the axles on the track and the body on their chord. On a straight that is the path point 3.5 m in. On the bend the engine stops with its face centre 0.84 m to the side, and a leading carriage stops 0.53 m to the side (orchestrator's default for Q1). Squaring the face centre instead would put the forward buffer head into the beam. A train arriving off the junction's branch meets the main-line toe stop with its face centre 0.21 m to the side.
- **Station ends.** A train is 24.22 m and its axles span 19.68 m. The level section runs from 3.856 m to 23.856 m in from each edge. With a stop at 3.5 m, no train standing at the buffers keeps every axle on the level section: the rear axle reaches 25.65 m. A station tile's platform-end stop therefore stands `STATION_LEAD` (2.47 m, the engine's buffer-to-axle overhang) short of the level section, at 1.386 m in. A train centres on the platform before it reaches the stop, either way round, and every axle stays on the level section. The cost is that the stop's brace feet reach about 1.0 m past the tile border at an open platform end.
- **Capacity.** A train standing at a straight tile's stop ends 1 cm past the tile's far seam, so body overlaps under 5 cm (`rail::TOUCH`) no longer foul a junction or a crossing.
- **Giving way.** A train that has given way twice takes the nearest station that no other train stands at. Taking the nearest one by route livelocked the 211 m two-station dumbbell, where the two trains want each other's terminal across one headshunt: the yielding train kept choosing the terminal the other train stood at, followed it in and out of that branch, and never arrived.
- **Walker.** `rail_buffer_stop` is in `walk::OBSTACLES` with prop kind `buffer_stop`. `WalkGround::build` places one collider at each open end.
- **Sound.** `GameEvent::TrainAtLineEnd { id, x, z }` (bridge `trainAtLineEnd`) fires once when a train comes to rest against a stop, at the stop's pivot. The audio layer plays one pick from the `buffer-clunk` pool, at level 0.35, on the train voices.
- **Evidence.** Run `node e2e/rail-buffer-stop-shots.mjs dist` to write `runtime-straight.jpg`, `runtime-bend.jpg`, `runtime-ghost.jpg` and `runtime-web.json`.
