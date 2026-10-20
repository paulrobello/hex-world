# fern_clump_b friction

Variant of fern_clump on the new family helper `kit/_ferns_var.py` (shared with fern_clump_c).

- **The approved generator is monolithic, so the variant pipeline is a copy** (design, 0 min lost).
  `fern_clump.py` builds at module level and cannot be imported, and the skill forbids editing it, so
  `_ferns_var.py` carries its card, normal, wind, texture and collider steps with the frond layout as
  a parameter (the `_rock.py` pattern). It drives the approved `_ferns` tile helper unchanged and
  asserts the constants the copied card code depends on (`ATLAS`, `TILES`, `TILE_H`, `ASPECT`). No
  skill change: project.md "Variants" already names this route.
- **Arch probe first, again** (5 min, no rebuilds lost). A plain-Python probe of the frond paths
  (height, reach, tip height) set the young fern's lengths and elevations before the first Blender
  run; the first build landed inside its asserts (0.48 m tall, 0.34 m reach). This confirms the
  fern_clump friction entry's proposed pitfall text; it is still unfolded, so it is repeated in the
  report.
- **A smaller fern keeps the family's wind U** (design). The tip U stays the original's
  sway x min(1, s^1.5 x 1.05), so a 0.48 m fern bends its tips as far in U as a 0.76 m one; the
  manifest amplitude (0.15 m per unit push) is the same, so its tips may read livelier in play. Raised
  as an owner question rather than changing the family's wind data.
