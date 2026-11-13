# Rail at levels: reference candidates (R2, prepared ahead)

Rail at levels, rail viaducts and rail tunnels stay deferred by the owner (PRD §15 P4, first-round scope). Under A39 O1, rail never changes level and crosses height only on viaducts and in tunnels. These are reference candidates generated ahead of time, so the owner can pick when the deferral lifts. Nothing here is approved, and no `approval.json` exists for either piece.

- Sheets: `docs/checkpoints/rail_tunnel_portal/refs-sheet.jpg` and `docs/checkpoints/rail_viaduct/refs-sheet.jpg`. The top row of each sheet shows the approved family refs, and the bottom row shows the candidates.
- Sources: `assets-source/refs/rail_tunnel_portal/` and `assets-source/refs/rail_viaduct/`, each holding `ref-1..4.jpg`, `job-1..4.json` and `refs.json`. The prompts are in `scripts/refs/higgsfield-refs.sh` (`make refs PIECE=<id>`), with the re-roll history in comments there.
- Generations: 6 for the portal and 8 for the viaduct, under the cap of 12 per piece. I rejected and re-rolled any image whose hexagon was not regular, whose track ended at a corner or short of an edge, or whose tile came back square.

## Q1 (blocking, answer first): the train does not fit under one level of cover

The rail top stands 0.46 m above the tile (`_rail.RAIL_TOP`, `trains::RAIL_TOP_Y`). The engine stands 4.2 m above the rail top to its chimney cap (`train_engine.py`), so its top is 4.66 m above the tile. The engine is 2.7 m wide, and the bridge pieces allow a 2.5 m half sweep. A rail arch therefore needs its crown at about 5.0 m, with about 0.35 m over the chimney, and an opening about 5.2 m wide at the springing. Adding the voussoir ring (0.45 m), the keystone (0.12 m) and the coping (0.28 m), as in `_tunnel_portal.py`, puts the wall top near 5.9 m. One level of cover is 4.0 m (`LEVEL_STEP`). The road portal fits within that, with its crown at 3.15 m and its wall top at 4.0 m. A rail portal cannot fit while the track stays at the tile's level, and lowering the track into a dip would break A39.

- O1: rail tunnels need cover two levels (8 m) high. The wall then sits flush with the cover, as on the road portal, and the seams stay simple. The drawback is that a one-level hill can never take a rail tunnel. ref-4 shows this case, though the model drew the hill lower than asked.
- O2 (recommended): the head wall stands about 2 m above a one-level hill. The portal tile's turf heaps over it into a mound that eases back down to the cover level by edge 0. ref-3 shows this shape. The face then has to sit about 6 m inside edge 0, instead of the road portal's 2.2 m, so the mound has room to fall 2 m before the seam. Under O2 the bore, crowned near 5 m, is taller than its 4 m cover, so the tunnel would sit above the cover meadow's surface beyond edge 0. That works only because the bore is never drawn and trains fade out at the face. I recommend O2 because it keeps rail tunnels under the same one-level hills the road tunnels use, which are the hills players build most.
- O3: shrink the arch or the train. I do not recommend it, because the engine's proportions are owner-approved (A17).

## rail_tunnel_portal candidates

- ref-1: a hex tile at 65°, with a one-level hillside rising from the front flat side, the track level through a grassy cutting into a granite portal with low wing walls and two lanterns, and the coping flush with the hilltop. The arch reads short for the train, which is the Q1 problem as drawn.
- ref-2: a 35° close-up worded like road ref-2. It has a tall, narrow semicircular arch on tall jambs, granite with buff stones, lichen and moss, a keystone, a fern-hung coping, and amber lanterns on scrolled brackets, over the rail ref-1 track and ballast.
- ref-3: a hex tile at 65°. The hill crests over the portal into a turf mound above the wall, which is the O2 layout.
- ref-4: a hex tile at 65° with a steeper hillside and a smaller portal with wing walls, standing in for the two-level (O1) case.

**Recommended pick: ref-2 for the look, plus ref-3's layout if Q1 goes to O2.** ref-2 matches the approved road ref-2 almost stone for stone, so the two portals read as one family, and it is the only candidate whose arch clearly clears the train. ref-3 is the one tile view that shows the mound the clearance needs. If Q1 goes to O1, pair ref-2 with ref-4 instead.

Planned piece:
- Kind `prop` and pack `village`, the rail pack, unless it should ride `core` with the road portal. LOD0 ≤ 1,500 tris, LOD1 ≤ 50% and LOD2 ≤ 15%.
- KTX2 class `piece` (≤ 1,500,000 B), with a 512² PNG fallback albedo beside a 1024² KTX2, as the road portal has (owner O1).
- The stone takes the road portal's shared-island method: a wrap-tileable panel plus voussoir, keystone, jamb and coping islands. About 95 m² of visible stone, against the road portal's 60 m², cannot take a unique bake.
- It is a ground hero at the origin of a straight-rail portal tile.
- The new family file is `kit/_tunnel_portal_rail.py`, which imports `_tunnel_portal.py` and sets its module globals (A, J, B, FACE_TOP, TOP, X_F). The approved helper itself must not be edited.

## rail_viaduct candidates

- ref-1: a low viaduct, about one level (4 m), on three broad round arches. It crosses the tile centre between the middles of the lower-left and upper-right sides, with the deck ends flush at the rim.
- ref-2: a viaduct about two levels (8 m), on three tall round arches over slender, slightly tapering piers, with ends flush at the rim and grass and ferns in the parapet joints.
- ref-3: a viaduct on six narrower tall arches, one tier rather than the two asked for. Its deck stands about as high as ref-2's, not higher, so the set has no distinct three-level (12 m) form. One would need its own roll, and 4 generations remain.
- ref-4: a 35° close-up of the deck. It shows the granite face with lichen, moss and rain streaks, dressed voussoirs, a string course and coping parapets, with the rail ref-1 track and ballast between them.

**Recommended pick: ref-2 as the form, with ref-4 for the stone and deck detail.** ref-2 has the clearest rhythm of arch and pier, and it scales by pier height alone: the same three arches give the one-, two- and three-level variants, with ref-1 showing the low end. No candidate shows a true three-level deck. ref-4 matches the portal's granite family and the approved rail track, which ties the viaduct to the tunnel and to rail ref-1.

Planned piece:
- One generator builds three pieces, `rail_viaduct_l1`, `_l2` and `_l3`, one per deck height. Kind `center`, as `river_rail_bridge_center` is, in pack `village`.
- Kit LOD0 ≤ 4,000 tris, LOD1 ≤ 50% and LOD2 ≤ 15% (border pieces are never decimated). KTX2 class `piece`.
- At about 450 m² of stone on the two faces of an 8 m span, a unique bake is impossible. The faces should sample a world-space tileable granite block (the river bridge's `STONE_UV` route), or the portal's shared islands, with masks baked or analytic.
- The track reuses `_rail.track` and `_rail.section()` at the deck height, in a new family file `kit/_rail_viaduct.py`.

## Runtime integration notes

- **Train clearance under the arch.** The engine is 4.66 m tall from the tile at the chimney, on the centreline, and 2.7 m wide (±1.35 m). A semicircular head of radius 2.6 m springing at 2.4 m crowns at 5.0 m and stands 4.62 m above the tile at ±1.35 m, which is 4.16 m above the rail top. The generator must check the cab roof's eave corners and the carriage roofs against that profile (the `_train` sweep), not the chimney alone. The carriages are lower than the engine. Trains need the cars' fade-out at the arch (`cars.rs` `fade_tag` and opacity) and a tunnel link through the rail graph between two facing portals (`trains.rs`), mirroring the road tunnel's straight-line pairing. Today the train code has no tunnel and no level handling.
- **Viaduct deck height.** The rail top sits at y = deck level × 4.0 + 0.46 (`LEVEL_STEP` plus `RAIL_TOP_Y`), and the ballast top at deck level × 4.0 + 0.22. That rail top must equal the raised neighbours' rail top exactly, so a train rides level from the raised tile across the viaduct. `trains.rs` must take the rail edge's level rather than the tile's ground level.
- **New edge kind (orchestrator item, needs a gate branch first; depends on rail on raised ground).** Both deck ends meet raised neighbours that carry rail, and rail on raised ground is itself deferred in the first-round scope, so lifting viaducts alone leaves nothing for them to connect to. A viaduct tile has its two rail edges at the neighbours' upper level and its other four edges at its own low level. Its deck ends meet the border far above the tile's ring. This conflicts with the border-ring contract (ring flat at y = 0) and with the height-aware socket rules. It needs an additive `kit_gate` branch and its failing test, plus a hex-core socket and edge-height change, before any modelling. Under the one-writer rule this goes to the orchestrator, not to an asset agent.
- **Walker** (`walk.rs`).
  - Portal: the face's step stops the walker, as in `walker_cannot_enter_the_tunnel`. A taller wall and mound change only the step height.
  - Viaduct: the piers go into `OBSTACLES`, or get colliders, so the walker passes under the arches and never through a pier. The deck must not count as ground for a walker standing below, or the walker would climb onto it. Whether a walker on the raised neighbour may walk along the deck is Q3.
  - `CAR_GROUND` needs no rows, because no road crosses either piece.
- **Scatter and grass** (`scatter.rs`, `grass.rs`).
  - Portal: as on the road portal, a `GROUND_PROPS` entry, the fern slot in `NOT_GROUND`, and the wall-top turf slot in the grass list.
  - Viaduct: `FOOTPRINTS` for each pier, so rocks, flowers and grass do not spawn inside the piers. The parapet and deck slots go in `NOT_GROUND`, so meadow scatter never lands on the deck. The meadow under the arches stays ordinary runtime scatter.
- **Boids.** The play sky band is 22–44 m, lifted by the highest tile level (`plan.rs` `SKY_LOW`/`SKY_HIGH`, `sky_lift`). A three-level deck plus the train reaches 16.7 m, so it clears the band. Parapet perches through `canopy_perches` are optional.
- **Data and thumbnails.** Each piece needs new `data/tiles/*.ron` entries (a rail portal tile on the cutting shape, and viaduct tiles per level), `make thumbs`, and `make ui-schema` if the inventory changes.

## Open owner questions

- Q1: tunnel clearance. Choose O1 (two-level cover), O2 (a proud wall under a turf mound, recommended) or O3. See above.
- Q2: pick the references. I recommend portal ref-2, with ref-3's layout under O2, and viaduct ref-2 as the form with ref-4's detail.
- Q3: may a walker cross a viaduct deck from a raised neighbour? I recommend no for the first round: piers become obstacles and the deck is not ground.
- Q4: should one viaduct design serve all three deck heights by pier height alone (recommended), or should three levels take a two-tier form? No candidate shows a two-tier or true 12 m form yet, so choosing that needs one more roll.
