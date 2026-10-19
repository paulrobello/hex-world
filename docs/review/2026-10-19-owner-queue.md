# Owner review queue, 2026-10-19

**Answered 2026-10-19: "all approved".** A1-A5 approved and recorded in each piece's provenance (`c02a9de`); G1 approved as shipped (Very far kept, default Near), its perf run still pending; pushed (`818fc91`, CI green, deployed). The questions below are kept as the record.

New P3 kit pieces waiting on your look. Nothing here is approved; each piece is merged to main and visible in the kit scenario (`make run ARGS="--scenario kit"`), not yet in play.

## Approvals

**A1. Hay bales (`hay_bale_round`, `hay_bale_stack`), card `01a154f201e7`.** Merged as `6cf5b97`.
- Sheets: `docs/checkpoints/hay_bales/hay_bale_round-sheet.jpg`, `docs/checkpoints/hay_bales/hay_bale_stack-sheet.jpg`.
- Round bale 576 / 168 / 64 tris, stack (five square bales) 509 / 227 / 67; both well under the prop budget.

Questions:
- **Q1.** The square-bale straw reads paler and cooler than ref-2's gold. Warm it?
- **Q2.** The round bale is net-wrapped as briefed; the refs draw twine. Keep the net?
- **Q3.** Both read dark on their shade sides in engine (baked AO is near 1, so this is the scene light). Acceptable?

**A2. Red barn (`barn_red`), card `01a154f1fcab`.** Merged as `ae8f5bf`. 10 x 7 m, 45° slate roof, 1054 / 280 / 58 tris.
- Sheet: `docs/checkpoints/barn_red/sheet.jpg`. Details and the four questions: `docs/review/2026-10-19-barn_red-pending.md`.
- **Q4.** Keep the red, or lift it a little so the shade side isn't near-black?
- **Q5.** Size right beside the houses, or smaller (8 x 6 m)?
- **Q6.** Add the ref-1 lean-to shed or open doors?
- **Q7.** Accept faint batten aliasing at eye level, or widen battens to 4 cm?

**A3. Bakery (`bakery_shop`), card `01a154f1ff2e`.** Merged as `d5fe285`. 1405 / 601 / 127 tris, on road tile (1, 0) facing the road.
- Sheet: `docs/checkpoints/bakery_shop/sheet.jpg` (in-engine shots re-taken on the fixed renderer).
- **Q8.** Roof: the references' darker brown, or the cottage's orange tile?
- **Q9.** Add flower boxes under the upper windows (about 95 tris)?
- **Q10.** Bread as colour bands and masks, or some loaves as geometry?

**A2 and A3 note:** the barn's and bakery's near-black walls were mostly a renderer bug, fixed in `c15edd8` (rotated pieces had their normals mirrored since 2026-10-17). Every sheet's in-engine shots were re-taken on the fixed build; the walkthrough is `docs/review/2026-10-19-walkthrough.html`.

**A4. Cow and sheep (`animal_cow`, `animal_sheep`), card `01a155261550`.** Merged as `c6b1598`. Cow 1.45 m withers (1058 / 468 / 148 tris), sheep 0.78 m (736 / 306 / 110). Static for now, with leg and head weights baked in for a later wander and graze.
- Sheets: `docs/checkpoints/farm_animals/animal_cow-sheet.jpg`, `animal_sheep-sheet.jpg`.
- **Q11.** The sheep's wool neck reads as a flat collar where it leaves the fleece. Smooth it?
- **Q12.** Scale shown beside the oak and hay; want one beside a house?

**A5. Houses re-baked (`house_cottage`, `house_twostorey`), card `01a15544869e`.** Merged as `cc38bad`. Thin faces (glazing bars, frame returns) drew black; now they show their colour. Geometry unchanged. Before/after: `docs/checkpoints/houses-masks-fill/sheet.jpg`.

**Also since 2026-10-17:** reviews of the forest fix, hay bales and barn were taken with the rotated-normals bug, so tiles at 60/120/240/300° looked darker or mis-lit in those shots.
