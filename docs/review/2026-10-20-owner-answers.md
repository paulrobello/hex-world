# Owner answers to the 2026-10-20 walkthrough

Paul answered `docs/review/2026-10-20-walkthrough.html` on 2026-10-20. Codes follow the page. "Default" marks a question he left open, where the page's recommendation or the orchestrator's pick stands until he says otherwise.

## Decisions

- **D1 hero oak `tree_hero_oak_c`:** approved, lands as built. Q2 dead wood stays bark-coloured, no fourth slot (default). Q3 perches unchanged (default).
- **D2 rail buffer stop:** Q1 ref-2 (bent rail). Q2 trains stop 3.5 m inside the edge (default, follows ref-2). Q3 stops drawn in the placement ghost, Q4 left out of thumbnails, Q5 soft buffer clunk on reversal (defaults, as recommended).
- **D3 beach dressing:** Q1 driftwood and dune_grass refs approved. Q2 replace the centres' baked logs and tufts with the scatter, keep the pebbles. Q3 about 3 logs and 10-14 grass clumps per straight beach tile. Q4 each grass clump on its own low sand mound. BD-Q2 (where the grass grows) takes the proposal: dry sand only, densest near the grass line.
- **D4 boats and lighthouse:** Q1 rowing boat, sailing boat and lighthouse refs approved (BA1-BA3). Q2 the orchestrator's pick, asked for: sailing boats keep to ocean and river mouths (a 6 m mast never clears the 0.95 m arch); rowing boats use rivers and turn back at a bridge, so the two approved river bridges keep their arch. Q3 both moored boats and rowing and sailing routes; the count scales with the connected water, at most 3 of each kind. Q4 and BQ10-BQ14 (lighthouse look and beam) still open; the lighthouse waits for them (P6). Unanswered BQ1, BQ2, BQ4, BQ5, BQ6, BQ9 take the queue's lean: rowing boat about 3.5 m, white with a blue band, smooth planked hull, the landing stage as its own prop; sailing boat about 5 m with a 6 m mast, one gaff main and a jib, tan sails, blue hull.
- **D5 wheat field:** three distinct rotations.
- **D6 terrain border rise:** Q1 flatten the 0.11 m rise in geometry too (rebuilds the meadow, forest, river and spring bases and `fence_paddock`, `wheat_field` and two hay bales). Q2 the field in play approved (card criterion 5).

## Checks

- **C1-C4:** all good. C4 confirms the rail defaults: one livery per train from the six-colour palette, crossing gates open, right-hand turnout only, red lever, carriage as built.

## Pieces

- **A1 `barn_red_b`:** Q1 the walker collider follows the closed walls, so the walker can enter the open shed. Q2 the dark interior accepted. Q3 both ridge and cupola become bird perches.
- **A2 `bakery_shop_b`:** Q1 the orchestrator's pick, asked for: pull the back-left roof corner in to 0.12 m from the border, as on the original. Q2 the eave accepted.
- **A3 `house_twostorey_c`:** Q1 box collider accepted. Q2 roof tone stays.
- **A4 `windmill_b`:** Q1 buff brick base approved. Q2 shutters fine. Q3 the stage keeps its dark timber.
- **A5 `tree_oak_c`:** Q1 re-pick accepted. Q2 no perches until Q33.
- **A6 `bush_verge_c`:** the taller form is fine.
- **A7 `shrub_forest_c`:** Q1 re-pick accepted. Q2 walker stop accepted.
- **A8 `sapling_spruce_c`:** re-pick accepted.
- **A9 `wildflower_single_e` (poppy):** Q1 approved from the sheet. Q2 one in ten. Q3 hide outside summer in P5. Q4 more poppies on road and rail verges, which needs per-tile weights.
- **A10 `animal_cow_c`:** Q1 keep the LOD2 head fix and its 1.4 cm collider move. Q2 the darker red-brown is inside the look.
- **A11 `bird_goldfinch`:** Q1 approved from the sheet. Q2 draw it at 5x. Q3 starlings stay the majority.
- **A12 `fish_mackerel`:** Q1 approved from the sheet. Q2 the ocean trout leaving play accepted. Q3 brighten sea fish like river fish.
- **A13 `train_carriage_b`:** Q1 approved as built. Q2 a mix per train, not always last. Q3 the lookout height is fine.
- **A14 LOD2 neck fix:** nothing asked.

## Work this opens

1. Land `tree_hero_oak_c` with its integration patch.
2. Model `rail_buffer_stop` from ref-2; move the train stop to 3.5 m; draw stops in the ghost; add the clunk.
3. Model `beach_driftwood` and `beach_dunegrass` scatter pieces with mounds; rebuild the five beach centres without baked logs and tufts; scatter rules on dry sand.
4. Model `boat_rowing`, `boat_sail` and the landing stage; boat runtime (moorings, river rowing that turns at bridges, ocean sailing, count by water, at most 3 of each).
5. Wheat field three rotations.
6. Flatten the 0.11 m border rise in the four bases and rebuild the dependent pieces.
7. `barn_red_b` wall-following collider and ridge and cupola perches.
8. `bakery_shop_b` corner pulled in.
9. Per-tile scatter weights with more poppies on road and rail verges.
10. Goldfinch drawn at 5x.
11. Brighter sea fish.
12. Brake third mixed per train.
13. Record the approvals above in each piece's `approvals.json` and in the refs' `approval.json`.

## Later answers (2026-10-20, in chat)

- **Farm animals:** F1 drive the leg and head motion data (in progress). F2 Q42 and Q57 closed: animals stand on the drawn ground, worst hoof 2.5 mm. F3 Q43 and Q59 closed: no grass clear under moving animals. F4 walking and grazing only, no run state. F5 the current liveliness stays.
- **Boats:** Q2 flat sails are fine. Q3 no rower figure for now. Q4 ref-1's red-ochre tan on both sails. Q5 the sailing boat keeps seaward of the beach shelf (s about 3.6) on coast tiles.
- **Defaults taken while waiting (override any):** buffer stop Q1 trains stop by body position on bends so buffers meet; Q2 a junction-end stop sits on the path a train takes; Q3 a stop beside a station ramp is fine. Boats Q1 water inside hulls on swell is fixed in the water pass by the boat runtime.

## Later the same day

- All models approved from `docs/review/approvals.html` (66 pieces); all audio approved except `train-whistle-1` (redone as attempt 66).
- Buffer stop Q1 (stop at a terminal station's open platform end reaches about 1.0 m past the tile border, every axle on the level section): accepted (owner, 2026-10-20).
- All audio and pieces approved from `docs/review/approvals.html` on land-b13: rail_bend_120_center, river_bend_60_base/_center, river_fork_base/_center, buffer-clunk-1/-2, the new train-whistle-1, boat-oar-1..3, boat-sail-1/-2.
- The rail bend report was the road 120 bend on rail (owner's mistake); the rail 120 bend landed anyway.

## Evening (2026-10-20)

- Stone well references approved (WL-A1, `assets-source/refs/well_stone/approval.json`). WQ1-WQ3 still open; the build takes the recommendations meanwhile.
- Boats-fork Q1: keep a single moving rowing boat per river system for now.
- Thumbnails: keep six images per rotation for the hero tiles (oak, windmill, farm, house_cluster).
- Stone well (sheet `docs/checkpoints/well_stone/sheet.jpg`): Q1/WQ1 placement on house_cluster's lawn, about 6.5 m from cottage 0's door, yes. Q2/WQ2 covered `well_stone` as the piece with `well_stone_b` in about one cluster in three, yes. Q3/WQ3 1.58 m across, 0.9 m wall, 2.4 m ridge, yes. Q4 apron 5 cm above the lawn: keep as is. Q5 shaft water on the iron slot: see how it looks as is. Q6 the roof ridge is a bird perch: yes. Q7 shade-side stone read: acceptable (colour grade question).

## 2026-10-21

- Saves Q1-Q3 ("yes to all"): Q1 keep the load confirm as built; Q2 the autosave hold while the confirm is open gets a time limit; Q3 Replace gets an Undo on its toast (orchestrator's pick between the two options offered; the owner can switch it to a second confirm).
- Download Q1: the initial-download budget is 30 MiB, an MVP target that can be raised as needed. Q2: keep the core PNGs as they are (the lossless re-encode is a lever for later). Q3: the PNG fallback stays report-only and is monitored (the gate warns within 1 MiB of the budget or over it).
- UI (A4): Q1 build the Settings "Reduce motion" control; Q2 its options are Follow system or On (no Off override); Q3 the instant thumbnail swap on R is right under reduced motion (with full motion the picture turns); Q4 do the engine-side feedback pass, owner reviews when ready.
- PRD drift (A5): Q1 settled pieces on the instanced chunk path get a dithered LOD band; Q2 on a tier drop the excess cars fade out; Q3 CI does not check audio, video or browser (they cost too much), so audio-check stays local and drift item 4 is settled.
- Road bend crease (A7): trim the stub overlap (held: the owner's no-new-asset-work rule of the same day; confirm before starting).
- Bakery (A8): Q1 keep park-1's 0.10 m clearance; Q2 revisit inner-corner rounding for short spurs; Q3 update the PRD "Destination tiles" paragraph. Standing: always keep the PRD updated with new or changed decisions.
- Approvals page (2026-10-21): "all approved on approvals page": the 27 pieces (well_stone, well_stone_b and 25 rebuilds: the road verge sink normals on road_bend_60/120/junction centres and house_pad, fish_trout's seam, the forest-forest edges and forest/meadow bases, farm animals and bales, bakery_shop_b, beach and river centres, the river road bridge centre) and the well-windlass-1/-2 cues, recorded in each approvals.json and provenance.json.
- Spur and bakery (A13, cars branch): Q1 accept park-1's east departures sharpening from 16.15 to 16.70 deg/tick; Q2 accept park-1's rotation-0 departure passing the kerb east of the forecourt mouth at 0.113 m (was 0.350); Q3 yes, fix the bakery departures' 21.8 deg/tick peak around the reverse cusp (target at most 20 deg/tick on the sliding maximum, all rotations).
- Saves follow-up (A12): Q1 no, Undo on Replace does not restore the replaced world's camera view; Q2 keep the 4 s Undo toast as is (pauses on hover only); Q3 yes, the 20 s hold and "Your changes to {current} are saved." with Load and Cancel are right.
- Road bend trim (A7): keep holding until the no-new-asset-work rule lifts. A13 confirmed: yes to all.
- Cars `set_car_cap` test hook: stays callable from the web bridge (owner, 2026-10-21: no need to hide it).

## 2026-10-21 (afternoon)

- Draw budget (A2): Q1 the orchestrator's pick: the 500-draw budget gates the overview camera; close cameras are tracked, not gated. Q2 yes, the scatter pair limit becomes 180 (60 per LOD). Q3 pick whichever structural fix benefits the whole game most (not a new budget). Q4 every piece should have a middle LOD (LOD1).
- Water (A3): criterion 6 is met by the 208-tile water world (revisit if the large world becomes a problem). Q1 drying trail strength and Q2 foam level: show options. Q3 no drying trail on the fallback river bed.
- Fish (A10): sea-to-river crossings covered by unit tests are accepted.
- Engine feedback (A14): Q1 keep the 180 ms ghost turn; Q2 yes, the edge glow fades in when the turn lands; Q3 the ghost meshes fade out when the pointer leaves; Q4 the commit flash moves from 80 to 120 ms; Q5 Reduce motion stays under Accessibility.
- LOD band (A15): Q1 band parts cast shadows from one LOD only; Q2 per-part LOD choice (more draws on small worlds) is acceptable.
- Colour re-check on rich-3 (`docs/review/2026-10-21-rich3-recheck.html`): looks good.

## 2026-10-26

- Water looks (A3, `docs/review/2026-10-21-water-options.html`): Q1 drying trail O3, the current 0.8 darkening; Q2 churn foam O3, the current 1.0 scale; Q3 confirmed, no drying trail on the plain fallback river bed. No code change; the debug-only `--water_look` knob stays for later comparisons.
- Road bend trim (A16): all approved: road_bend_60_center and the trimmed stubs, the +8 draws per pass while a road_bend_60 is on screen, and e1's AO noise.
- LOD slot pass (A17): all approved: the 16 rebuilt pieces (plus the two byte-identical river mouth refreshes listed with them), the platform's flat LOD1 flower mound and the wells' LOD1 courses.
- Share image (A9): the R8 play shot (`docs/checkpoints/share-image/og-image-after.png`) is approved.
- Bulldoze dust (A9): revert the 2026-10-21 "another 20%" particle cut; the particles go back to their previous size.
- Rail ends (A18): approved one-way: a tile without rail may go against a stopped rail end, but a new rail tile may not point its end at an existing meadow.
- Spring (A21): keep the granite ring and the lily pads built into river_spring_center for now.
- UI (A4): approved: the hallway test and the sign-off of the shipped UI (build 041ad32).
- Grass blades (A9): look approved.
- Beach and ocean scatter (A24, 2026-10-26): Q1 yes, about half of beach tiles keeping one pebble cluster is enough. Q2 yes, seabed stones showing 2-4 cm on swell crests is acceptable. Q3 yes, the uniform pebble wetness is fine. Q4 yes, place the new props in the kit showcase. Q5: the owner asked why the scatter (mesh, material) cap exists and said rivers and oceans in one world must be supported.
- Scatter pair cap (A24 Q5, 2026-10-26): go with the orchestrator's recommendation. The pair count is tracked, not gated. A mixed river, beach and ocean world with every scatter piece keeps its overview within 500 draws. If it goes over, scatter props move to shared materials rather than losing content.
- Open questions page (2026-10-26, "all approved with your recomendations"): A19 build B1 (boulder and outcrop set) and B3 (lily pads and reed beds) first, then B2 (dry-stone walls and hedgerows). A22 Q1 the five riverbank props and the rebuilt bridges and spring approved; Q2 the reed share goes up to half and half. A23 Q1 the matte verge bushes approved; Q2 yes, shrub_forest and shrub_forest_c get matte leaves too (holly stays glossy). All 17 pieces then pending on docs/review/approvals.html are approved (the A22 and A23 pieces and the six beach and ocean pieces of A24).
- Open questions page, second round (2026-10-27, "All recommendations approved"): A25 the B1 boulder references and every recommended default (sizes, tiles and counts, coast band inside the beach cap, LOD, unique bakes, outcrop not walkable). A26 the B3 lily and reed references and every recommended default (spring's pad colour, four pieces, spring and river-bend tiles with their own 0-2 cap, never on waves, LOD). A27 the matte hazel and bramble shrubs approved. A28: stop at the per-cell cascade skip if the MiniMac before/after confirms no camera is slower; revisit shared scatter materials (O2) when scatter grows again.
- Frame rate (A29, 2026-10-27): keep the full frame rate while a HUD control has focus (O1), and add a Settings option to cap the frame rate.
- Open questions page, third round (2026-10-27, "all approved"): A30 B1 approved with every recommended default (the five pieces; O1 boulders and outcrop lead their tiles' scatter rules; every thumbnail shift and the 2 m higher thumbnail look-at accepted; O1 add tiles to the mixed world so every scatter piece draws; no outcrop on the coast band, no boulders on beach_outer, the outcrop's 109 px/m bake and small boulders ending at 400 m accepted). A31 B2 references and layout approved as recommended (see assets-source/refs/walls_hedges/approval.json). A32 B3 approved as built (far pads as quads, reeds mostly on bends, plume reach accepted). A27 was already recorded in the second round.
- P2 and P3 scope (2026-10-27, in chat): "all p2 approved" (the grass and wind-sway look, and the native stall tail accepted; the UI was already signed off). P3: build boats on the ocean and rivers, and bridges and level crossings where rail meets road or river. "keep the recipe system deferred until we get more game assets done".
- A33 (2026-10-27, "O2"): the tier-change shader warm-up is off by default (`tier_warm=on` turns it on); the in-place quality change lands. Audio (2026-10-27): "all audio approved except for town-murmur-2 (i dont like the high pitch screetch at the start)": town-murmur-3 to -6 and the eight place-stinger cues approved; town-murmur-2 (door latch) to be redone.
- A34 (2026-10-27, "all approved"): the twelve B2 wall and hedge pieces approved; placement as built accepted (field tiles farm and wheat_field against meadow, forest, oak and windmill; one gate edge per farm clear of heroes; about one hedge edge in three with the hedgerow oak); Q3 O1: start shared scatter materials now (A28 O2: colour-only pieces sharing plus texture arrays per variant group) since a mixed water, coast and farm world measures 549 overview draws against the 500 budget.
- Audio (2026-10-27, "town-murmur-2 approved"): the redone door cue approved; every P2 audio cue is approved.
