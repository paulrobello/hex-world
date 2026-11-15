# rail_tunnel_portal integration

The rail counterpart of `road_tunnel_portal` (RL-Q1 O2, refs `assets-source/refs/rail_tunnel_portal`
ref-2 look, ref-3 layout). It is a ground hero at the origin of the straight-rail portal tile, yaw 0,
its face toward edge 3 and the hill on edge 0, authored in absolute tile-local heights and never
draped (the road portal's pattern). The piece and its manifest entry are landed. The runtime rows
it needs are in `integration.patch` (verified below). The tile itself, the tunnel wiring and the
train fade belong to the rail-at-levels work in `crates/hex-core`, `crates/hex-sim`,
`crates/hex-play/src/trains.rs` and `data/tiles/rail_*.ron`.

## Placement (tile frame: runtime x = Blender x, runtime z = -Blender y)

| Plane or point | x (m) | Notes |
|---|---|---|
| Face (front of the head wall) | 7.8564 | `APOTHEM - 6.0`, 6 m inside edge 0 |
| Mound top (wall-top turf rises to the crest) | 10.4564 | `X_F + 2.6` |
| Arch node | 11.1 (z 0.0) | 0.36 m inside the back plane, proposed `(id: "arch", at: (11.1, 0.0))` |
| Interior back plane (lining ends, the dark `back` slot closes the bore) | 11.4564 | `X_F + 3.6` |
| Edge 0 | 13.8564 | the mound has eased to the 4 m cover here |

Section: opening 5.2 m (half span 2.6), springing 2.4 m, crown 5.0 m, voussoir ring 0.45 m, keystone
0.12 m over the ring, face top 5.57 m, coping top (wall top) 5.85 m, turf heaping 0.3 m over the
coping, crest at the back plane 5.70 m, cover level 4.0 m by edge 0. Wall half width 6.5 m.

Proposed tile data beside the road's (`data/tiles/road_tunnel_portal.ron`), for the orchestrator:

```ron
nodes: [ (id: "arch", at: (11.1, 0.0)) ],
networks: [ (kind: Rail, from: Edge(3), to: Node("arch")) ],
props: ( hero: [ (piece: "rail_tunnel_portal", at: (0.0, 0.0), yaw_deg: 0.0) ] ),
ground: Cutting,
tunnel: Some((node: "arch", edge: 0)),
```

The rail track must run on through the arch to the back plane. The stand-in in the review stills
puts the rail top at 0.46 m, ballast at 0.22 m.

## Checklist

1. **Train clearance: handled.** The generator sweeps the committed LOD0 GLBs of `train_engine`,
   `train_carriage` and `train_carriage_b` on the rail top 0.46 m and asserts a floor of 0.15 m.
   - Minimum radial clearance from the intrados: engine 0.319 m at (|y| 0.31, z 4.66), the
     chimney; carriage_b 0.459 m at (1.06, 4.26); carriage 0.702 m at (1.42, 3.66).
   - The engine's chimney top (4.66 m) is 0.34 m under the 5.0 m crown.
   - Every portal vertex ahead of the back plane, swept against the vehicles' joint upper
     envelope, clears by 0.320 m at LOD0, 0.275 m at LOD1 and 0.533 m at LOD2.
   - Evidence: `clearance-section.jpg`, `clearance.json`, and the `portal.clearance` block of
     `assets-source/kit/rail_tunnel_portal/bake-report.json`.
2. **Train fade at the face: needs work (trains.rs, orchestrator).** The bore is lit only to the
   back plane. A train entering must be fully faded by x 11.456, the plane where the `back` slot
   closes the bore. The fade should start no earlier than the face, x 7.856. The arch node at
   x 11.1 is where the network ends.
3. **Walker step and walk.rs rows: handled in the patch.** As with the road portal, the piece is a
   `GROUND_PROPS` hero (`scatter.rs`). It is not an `OBSTACLES` or `FOOTPRINT_OBSTACLES` row.
   - The walker stands on the turf mound and the coping, crossing from the cover meadow.
   - The 5.57 m face is a step it cannot climb into the arch.
   - The GLB's collider box (x 7.726 to 13.856, |s| 8, up to 5.85 m) is unused by the walker, as
     the road portal's is.
   - A rail counterpart of `walker_cannot_enter_the_tunnel` (`walk.rs`) needs the tile:
     orchestrator.
4. **Scatter and grass: handled in the patch.**
   - `NOT_GROUND` gains `rail_tunnel_portal_fern`, so the fern cards over the coping are not ground.
   - `GROUND_PROPS` gains `rail_tunnel_portal`.
   - The grass-slot test list in `grass.rs` gains `rail_tunnel_portal_turf`. With the manifest
     entry landed, this is the edit `make checkall` needs.
   - `GROUND_BOXES` and `FOOTPRINTS` do not apply: the road portal has no row in either.
   - The tile's scatter rules, and a rail analogue of the road-portal scatter test
     (`scatter.rs`, `c.get("road_tunnel_portal")`), come with the tile.
5. **Boid perches: not applicable.** Perches are tree canopies only. The road portal has none.
6. **Thumbnails: needs work (with the tile).** `crates/hex-app/src/thumbs.rs` gains a
   `(<tile id>, 6)` row like `("road_tunnel_portal", 6)`, then `make thumbs`. The data validator
   rows in `crates/hex-core/tests/data_validator.rs` and the hillside-family test in
   `crates/hex-core/src/tile.rs` also name the road tile.
7. **Draw budget: handled.** Seven slots, as on the road portal: `stone`, `fern`, `turf`,
   `lining`, `back`, `iron` and `glass`, so seven draws per LOD.
   - Tris per LOD: 1,491, 735 and 207.
   - KTX2: 1,103,227 B in BC7 and 1,101,720 B in ASTC, under the 1.5 MB `piece` class.
   - The GLB is 133,924 B.
8. **Lamp emissive hook: handled, inert until P5.** The two lantern `glass` faces carry masks
   B = 1 (`portal-night.jpg` previews them lit). No runtime emissive path reads it before P5, as
   with the road portal.
9. **Sound: not applicable.** The road portal has no sound. A tunnel rumble for trains would be a
   new audio item.

## Patch verification

`integration.patch` is a diff against 8cfd51d of `crates/hex-play/src/play/grass.rs` and
`crates/hex-play/src/play/scatter.rs`. It was produced and checked in this order:

1. The edits were made in the tree. `rustfmt --edition 2024 --check` on both files exited 0.
2. With the edits in the tree, these gates passed:
   - `make checkall` exited 0.
   - `cargo test -p hex-play -p hex-render --lib -- boids scatter walk kit_scene` exited 0, with
     146 and 22 passed.
   - `make ui-e2e` exited 0. It cannot exercise the piece until a tile references it.
3. `git diff` of the two files was saved as the patch, and both files were restored to 8cfd51d.
4. `git apply --check` of the patch against the restored tree exited 0.

The asset commit alone fails `make checkall` on `grass_slots_are_the_grass_sets_minus_foliage`.
Apply this patch in the same merge.
