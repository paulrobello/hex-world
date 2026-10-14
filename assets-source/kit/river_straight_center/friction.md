# river_straight_center friction

Shared with the river family: see `assets-source/kit/river_edge/friction.md`. The dressing is `kind: prop` (colour-only slots, no maps): it sits on the base and touches no border or lane, so the centre/edge joint does not apply.

## §9.4 hook masks (2026-10-14)

1. **The slots had no masks, so the gate listed them as known gaps** (no time lost). The dressing already carries a planar UV0 (`uv.planar_cap_uv`) and colour-only slots, so the masks are an analytic field of (x, y) written with numpy (`masks_field`), with no Cycles bake and no geometry or UV change: the rebuilt GLB is byte-identical to the committed one. A = 1 keeps the AO the slots had before (none), so the MVP look does not change. Skill: `pitfalls.md` (Colour-only props get masks without a rebuild of the look).
