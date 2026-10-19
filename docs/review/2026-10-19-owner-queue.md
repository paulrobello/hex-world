# Owner review queue, 2026-10-19

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
- Sheet: `docs/checkpoints/bakery_shop/sheet.jpg`. Its in-engine wall shading is wrong until card `01a155602a2f` (yawed pieces render unlit walls) lands; judge the EEVEE stills for colour.
- **Q8.** Roof: the references' darker brown, or the cottage's orange tile?
- **Q9.** Add flower boxes under the upper windows (about 95 tris)?
- **Q10.** Bread as colour bands and masks, or some loaves as geometry?

**A2 and A3 note:** the barn's and bakery's near-black shade sides are at least partly a renderer regression (rotated pieces' walls lose their light). Re-check both after card `01a155602a2f`.

Still to come in this batch: the cow and sheep.
