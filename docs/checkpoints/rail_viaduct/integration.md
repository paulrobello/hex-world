# rail_viaduct integration (rail_viaduct_l1, _l2, _l3)

The three centre pieces, their manifest entries and the removal of their `data/pending-pieces.ron`
lines are in the tree. Every other runtime edit is in `integration.patch` here, for the
orchestrator to land (the patch touches `walk.rs` and `scatter.rs`, which are being edited
concurrently, plus `grass.rs` and three HUD files). It applies on `d176a30` and on this branch.

## What the patch does

1. **The deck is not ground (owner RL-Q3).** `scatter::NOT_GROUND` gains the seven viaduct slots:
   `rail_viaduct_l1_stone`, `rail_viaduct_l2_stone`, `rail_viaduct_l3_stone`, `viaduct_ballast`,
   `viaduct_sleeper`, `viaduct_steel` and `viaduct_head`. Without it the deck and coping are the
   topmost upward triangles over the whole 6 m wide strip, so:
   - a walker on the raised neighbour steps straight onto the deck (the ballast is 0.22 m over
     the neighbour's ground, under `MAX_STEP`), which RL-Q3 rules out;
   - the walker reads the deck height under the arches (an 8 m step at deck 2), so it cannot walk
     under the viaduct and the strip is one wall;
   - grass never grows under the arches;
   - and the deck is "ground" that no other check knows about.

   With it, the tile's ground is the meadow cap everywhere, as on any meadow tile.
2. **The piers and abutments block the walker.** `walk::CENTRE_OBSTACLES` gains four rows per
   height (part `rail_viaduct_pier`, the new `walk::VIADUCT_PIER`), appended after the junction
   lever, because `the_lever_footprint_covers_the_lever_geometry` reads row 0. Each row is the
   stone's widest extent below head height (1.8 m) in the tile frame. z is ±3.0, the face.
   x depends on the height:
   - **l2 and l3:** x is the pier at its battered foot, the bake report's
     `pierFootprintsRuntimeXZ`. The arches spring at 4.5 m and 8.5 m, above head height.
   - **l1:** the arches spring at 0.5 m, so at 1.8 m the opening is narrower than at the foot
     (half width 1.897 m against 2.2875 m). The row uses the narrower opening, so a walker's head
     does not clip the arch ring.

   | piece | abutments (x) | piers (x) |
   | --- | --- | --- |
   | l1 | ±8.897 to ±13.854 | ±1.897 to ±5.103 |
   | l2 | ±9.1875 to ±13.854 | ±2.1875 to ±4.8125 |
   | l3 | ±9.0875 to ±13.854 | ±2.0875 to ±4.9125 |

   `prop_kind("rail_viaduct_pier")` falls to `"rock"`. A `"pier"` kind would be a two-sided
   change (`walk.rs` and `web/ui/src/bridge.ts`), so it is left as an open question.
3. **No blade roots inside the stone.** `grass::exclusions` adds a box per `rail_viaduct_pier`
   row of the tile's centre (plus `WALL_GAP`, as a building's walls get), so blades grow under the
   arches and along the pier faces but not through them. Only the viaduct rows are used:
   feeding every `CENTRE_OBSTACLES` row in would also change `rail_junction`'s blades under its
   lever.
4. **A walker test.** `the_viaduct_piers_block_and_the_arches_pass` runs on each height on its
   own tile:
   - under each arch (x = -7, 0, 7) the walker stands on the meadow (|y| < 0.3), not the deck;
   - each row's centre is blocked by that row;
   - every LOD0 stone vertex below 1.8 m lies inside a row (to 5 cm).
5. **The Tiles sheet crashes on the viaduct entry (in `d176a30`, not from this branch).** The
   symptom is that `make ui-e2e` fails 20 checks in `e2e/ui.mjs` with the page error
   `TypeError: Cannot read properties of undefined (reading 'toLowerCase')`. Make stops there, so
   the other seven scripts never run. The cause is in `web/ui/src/rules.ts` `fit()`:
   - `clashes()` counts a deck end that faces an empty cell as a clash. That is right, and it
     mirrors hex-core `fits`.
   - Beside the lone start meadow every turn of the viaduct ties at two clashes, so the reason is
     read at edge 0, a deck end facing nothing.
   - `clashPhrase("Rail", undefined)` then calls `KIND_NAME[undefined].toLowerCase()`.
   - The throw is inside the sheet's render, so the slot key, the Doesn't-fit-here list and the
     ghost tag all stop drawing.

   It is not this branch: `git diff --stat d176a30 HEAD -- web crates data/tiles` is empty. A
   Node run of `fitEntry` over every `PICKER` entry, on the unchanged sources, throws on the
   viaduct entry alone.

   The fix leaves `clashes()` and the parity data alone. Where the chosen edge is a deck end facing
   an empty cell, `fit()` returns the new `words.ts` `DECK_END_REFUSAL` ("Needs raised rail at its
   deck height on both ends."). Every other case still goes through `clashPhrase`. A picker test,
   `a viaduct beside the start meadow is refused for its open deck end, not a clash`, fails with
   the TypeError on the old sources and passes with the fix. These hunks do not depend on items 1
   to 4.

## Section 11 checklist

| item | status |
| --- | --- |
| Train height | Handled in the tree. Trains ride `WalkGround::level_y` = level + deck x 4.0 (`walk.rs` `rail_lift`, `trains.rs`), and the generator asserts the rail top at N x 4.0 + 0.46 and the ballast at 0.22 over the deck on both borders, on every LOD (`deck_section_errors`). |
| Pier colliders | Patch: `CENTRE_OBSTACLES` rows (above). The GLB collider is the cap hull of the piers, abutments and a deck slab. The walker does not read it for a centre piece. |
| Scatter | Patch: `NOT_GROUND`. Wildflowers keep the rail lane's clearance (half width 1.75 + `LANE_GAP` 1.0 + their 0.25 m footprint), so a stem stands at least 3.0 m off the centreline, at the pier faces and never inside them. No `FOOTPRINTS` or `GROUND_BOXES` row: those are for props. |
| Grass | Patch: pier boxes in `grass::exclusions`. |
| Cars | No `CAR_GROUND` row: the viaduct carries no road. |
| Boids | The l3 parapet top is 13.11 m over its tile. The sky band starts at `SKY_LOW` 22 m and lifts by the highest placed level x 4.0, and a viaduct always has rail at its deck level on both ends. The viaduct is not a perch piece. |
| Thumbnails | The orchestrator's (`data_validator` is green on this branch with the pieces in). |
| Draw budget | One centre piece, five slots (stone and four track slots), on the shared meadow base. |
| Lamps | None. |
| Sound | None from the piece. The rail family's audio is the UI's. |
| Cliff seam | The stone stops 2 mm inside the border (`X_END` = apothem - 0.002), and the deck ends are flush with the raised neighbour's rail section. The neighbour's cliff skirt closes the face under the deck end. See the `viaduct-l*-seam` stills. |
| kit_scene | No cell. A viaduct places only with rail at its deck level on both ends (the `road_tunnel_portal` precedent), so `tile_counts` is unchanged. |

## Verification (2026-11-14, patch applied to this branch, then reverted)

- `git apply --check` is clean. The three files are byte-identical between `d176a30` and this
  branch, so it applies on `d176a30` too.
- `cargo fmt --all -- --check` exits 0.
- `cargo clippy -p hex-play --all-targets -- -D warnings` exits 0.
- `cargo test -p hex-play --lib` exits 0: 349 passed, 5 ignored. That includes the two new tests,
  `walk::tests::the_viaduct_piers_block_and_the_arches_pass` and
  `play::grass::tests::grass_clears_the_viaduct_piers_not_the_arches`, and the orchestrator's
  `trains::tests::a_train_on_a_viaduct_rides_the_deck`.

The HUD hunks (item 5) were checked on a scratch copy of `web/ui` outside the worktree (the
worktree's HUD files are not this branch's to edit). Applying the patch's HUD diffs to the
worktree's files reproduces that copy byte for byte.

- `node --test web/ui/test/*.test.ts`: 206 passed, 0 failed. The new picker test fails with the
  TypeError on the unchanged sources.
- `tsc --noEmit -p .` exits 0.
- `biome check src test` exits 0.
- All eight `make ui-e2e` scripts ran on this branch's `dist` with the fixed `hud.js` swapped in
  (`hud.js` carries no SRI): `ui`, `ui-motion`, `photo-fx`, `persist`, `saves-ui`, `audio`,
  `debug` and `version`. Each exits 0 with no FAIL line. Unfixed, `ui.mjs` fails 20 checks. The
  1-minute load was 23 to 105 during the run. These are functional checks, not timings.

The in-engine stills here were shot from the unpatched tree. Under the arches the walker,
grass and wildflowers behave as the patch describes only once it lands. In the stills the deck is
still the topmost surface, so no blades grow in the 6 m strip under it.

## Open questions for the owner

1. **Look against ref-4.** The build has ref-2's form and the portal's granite, lichen and moss.
   It has none of ref-4's deck-edge ferns or drip staining: the portal's fern cards were dropped to
   fit the track in the atlas. Add a fern pass on the coping?
2. **One map set or three.** The three heights share one bake and their maps are byte-identical
   (990,202 B BC7 each). Should they share one map set in the manifest?
3. **Pier obstacle kind.** The walker reports a pier as `rock`. Add a `pier` kind (a Rust and UI
   change)?
4. **Deck 1 proportions.** Deck 1's arches spring at 0.5 m (crown 2.8 m), which reads as a culvert
   row more than a viaduct. That is "one design, the pier height scales" taken literally. Keep it?
