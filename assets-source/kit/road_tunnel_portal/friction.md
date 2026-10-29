# road_tunnel_portal friction log

1. **The pinned ground field contradicts the clearance (about 25 min).** The brief and the P4 plan
   (Task 3) give the portal tile `ground: RoadRamp`, whose corridor climbs 4 m from edge 3 to edge 0,
   with the arch node at [4.0, 0.0]. The road there is 2.58 m up, so a car-height arch (crown 3.3 m)
   tops out near 6.1 m, 2 m above the 4 m cover line the wall top must meet. No geometry satisfies
   "crown <= 3.6 m over the road" and "wall top meets 4 m" on that field. A 14% slope rises only 0.6 m
   across a 4 m portal, so any portal face needs the corridor cut down to its road. Fix: one generator
   parameterised by option (`_tunnel_portal.OPTIONS`). The `cut` row is built, and the pinned `ramp`
   row is stilled on the same stand-in for the owner.
   *Proposed skill text (dispatch.md part 2, Budget):* "When a piece sits on a pinned ground field,
   tabulate the field at the piece's footprint before dispatch, and check every height constraint
   in the brief against those numbers."
2. **No references, so no green kit-check (0 min lost, a ruling needed).** `kit_gate`
   `provenance_complete_and_hashes_match` requires references plus an owner `gate: "reference"`
   approval for every manifest piece (R2). This piece has none, so `make kit-check` fails on that one
   assertion only (69 other gate tests pass). Any committed generator or folder without a manifest
   entry fails `no_kit_artifacts_outside_the_manifest`, so `make checkall` cannot be green either
   way. `make kit-provenance` without `REFS=` writes a record with `references: []` and
   `validation.pass = true` and does not refuse, although stages.md says `REFS=` is required.
   *Proposed skill text (dispatch.md part 1, Goal):* "If the piece has no approved references, say
   whether the owner waives R2 for it (and how provenance records the waiver) before dispatch.
   Otherwise kit-check cannot pass." Fixed on 2026-10-29 (owner: "reference images needed", so R2
   is not waived): `make kit-provenance` now refuses up front, writing nothing, when `REFS=` is empty,
   with one line naming R2. `make kit-fixture` calls the CLI directly and is unaffected.
   stages.md step 5 already said `REFS=` is required, and now the target enforces it.
   `kit-provenance.test.mjs` has no test for this. It spawns `kit-provenance.mjs` with `--refs` on
   every call and never goes through make, and the check lives in the Make recipe so that the
   fixture's direct CLI call keeps working.
3. **par-mesh cannot reach the orchestrator from a sub-agent (5 min).** `mesh_whoami` and
   `mesh_list` show this sub-agent registered under the parent session's address
   (`claude-f3ae@macdaddy:hex-world`), so a message to the orchestrator is a message to itself.
   The contract question goes in the report instead.
   *Proposed skill text (dispatch.md part 9):* "A sub-agent shares its parent's par-mesh address, so
   it raises a blocking contract question in its report, not over the mesh."
4. **A tile has one centre (10 min).** The plan makes the portal the tile's `center` and has the
   game draw a draped road_straight_center, but a tile cannot place both. The rail station
   platform's pattern fits: a `GROUND_PROPS` hero at the origin with a `GROUND_BOXES` box and no
   `FOOTPRINTS` entry. Recorded as wiring W1 in docs/checkpoints/road_tunnel_portal/integration.patch.
   No skill change: this is a plan-level integration choice.
5. **Pack core rules out the kit's stone treatment (5 min).** `well_stone` and `bridge_stone` tint
   the `plaster` set, which is pack `village`, and a core piece may sample only core sets
   (`kit_manifest` core purity). The portal uses colour-only stone slots with modelled blocks, as
   `_house.py` does for timber. Its turf top uses the core `grass` set.
   *Proposed skill text (project.md, Budgets):* "A core-pack piece cannot tint `plaster`: stone in
   core is colour-only slots with modelled blocks (road_tunnel_portal)."
6. **A new `grass`-set slot fails a hex-play unit test (5 min).** `grass::tests::
   grass_slots_are_the_grass_sets_minus_foliage` pins every manifest slot that samples the `grass`
   set, so the turf top failed `make checkall` in the hex-play lib tests, not in kit-check. The test
   list needs the slot: the hunk is in integration.patch.
   *Proposed skill text (dispatch.md part 11, Integration):* "A new slot on the `grass` set is a grass
   slot (runtime blades): add it to `grass_slots_are_the_grass_sets_minus_foliage` and say whether
   blades should grow on it."
   Item 5's colour-only stone was superseded on 2026-10-29: the owner asked for a hi-res source,
   baked textures and a reduced poly count. See item 7.
7. **A unique bake cannot hold 256 px/m on a large piece (40 min of design).** The portal shows about
   60 m² of stone, and a 1024² albedo, the largest the `piece` class affords next to its normal and
   ORM, holds 16 m² at 256 px/m. The fix is shared atlas islands:
   - a wrap-tileable wall panel, two voussoirs, a keystone, a jamb and a coping segment, each
     Cycles-baked from a straight high-poly template;
   - every LOD face mapped onto its island at true scale;
   - one face function per part type, used for both the receiver and the LODs, so the UV
     orientation (u to the viewer's right, v up) matches by construction.

   The first shelf packer overflowed at 88% fill. A skyline packer fits after trimming the panel to
   3.0 m, the soffits to 0.45 m and the coping template to 1.0 m.
   *Proposed skill text (project.md, Budgets):* "A stone piece with more than about 16 m² of visible
   unique surface cannot hold the 256 px/m median on a 1024² albedo. Bake straight high-poly
   templates onto shared islands, a wrap panel plus one template per part, and map every LOD face
   at true scale (`kit/_tunnel_portal.py`). The gate's median is per face, so buried faces, wall
   ends and edge bands must map at true scale too."
8. **Object-space noise breaks a wrap panel (no time lost: designed in).**
   `_rock.granite_material` samples Object coordinates, so the wrap copies of a block match only if
   they are linked duplicates of one mesh with one local origin. Each block's mesh is offset by its
   own random vector, so blocks differ from each other while their copies agree. The mortar plane's
   noise runs on `materials.torus_coords`, with one panel period per UV unit. Pass `soil_z` far below
   the blocks, or the granite's soil band browns every block. The build fails when the panel's
   gutters differ from the opposite interior strips (`seam_errors`). On the final bake the mean
   errors were about 0.001 across and 0.007 vertically in albedo, 0.0001 and 0.012 in normal, and
   0.015 and 0.049 in AO (32-sample noise).
   *Proposed pitfall text (Tileable sets):* "A wrap panel baked from separate blocks needs every
   copy to be a linked duplicate (object-space noise), and periodic coordinates on any shared
   backing plane."
9. **A neighbour inside a side receiver's cage bakes its own interior (10 min).** A ring-stone or
   coping template with neighbours on both sides would bake the neighbour's inside faces into its
   joint-side and end-cap islands, because the cage starts 0.05 m out from the receiver. The ring
   templates now stand alone, and the coping keeps only its right-hand neighbour.
   *Proposed pitfall text (Cycles bakes):* "Keep every occluder outside each receiver face's cage
   extrusion. A neighbour block touching a baked side face bakes its interior there."
10. **`make initial-download` measures a stale `dist/` (5 min).** The target builds `dist/` only
    when it is absent, so a second run after a texture change reported the first run's bytes: the
    albedo PNG had grown by 96 KB and the total was unchanged. Run `make build-web` before
    re-measuring.
    *Proposed skill text (dispatch.md part 7, Gate path):* "After a texture change, `make
    build-web` before `make initial-download`. It reads `dist/` and rebuilds it only when it is
    absent."
11. **A smaller PNG fallback failed the texel-density gate (10 min).** `kit_gate`'s density rule
    read the albedo PNG's width, so the 512 PNG fallback (owner O1) measured the stone at 129 px/m
    while the GPU samples the 1024 KTX2. The gate now reads the BC7 KTX2 header's width, falling
    back to the PNG only where no KTX2 ships (`kit_gate::texture_width`, with a test).
    *Proposed skill text (gates.md, texel density):* "Density is measured on the KTX2 the GPU
    samples. A `pngFallbackSize` in the export report shrinks only the fallback PNG."
