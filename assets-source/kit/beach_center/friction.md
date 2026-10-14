# beach_center friction

Shared with the beach family: see `assets-source/kit/beach_base/friction.md`.

1. **Dressing must stay off the wet bed** (2 min). The water reads the prepass depth as its bed, so every log, tuft and pebble sits at s <= -2.4 (dry sand and grass), 1.5 m inside the border; `_beach_dress.build_centre` asserts it on every LOD. No skill change: the rule is in the module.

## §9.4 hook masks (2026-10-14)

1. **The slots had no masks, so the gate listed them as known gaps** (no time lost). The dressing already carries a planar UV0 (`uv.planar_cap_uv`) and colour-only slots, so the masks are an analytic field of (x, y) written with numpy (`masks_field`), with no Cycles bake and no geometry or UV change: the rebuilt GLB is byte-identical to the committed one. A = 1 keeps the AO the slots had before (none), so the MVP look does not change. Skill: `pitfalls.md` (Colour-only props get masks without a rebuild of the look).
