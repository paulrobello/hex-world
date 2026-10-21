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
