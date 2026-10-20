# beach_driftwood (and _b, _c) friction

2026-10-20, beach dressing (owner D3), scripted route over `_log` via `_driftwood.py`.

- **`_log`'s butt-cap island ignores the taper** (3 min, two skip-bake builds). `Log.__init__` sizes
  the butt cap's UV rect from `r0 + flare * 1.2`, but the butt's real radius is `r0 + taper + flare`
  times the lump, so a `taper` above log_fallen's 0.016 pushes the cap's planar UVs out of its rect
  and the island assert fires ("face ... not inside one island rectangle"). Fix: keep `taper` small
  (0.004-0.012 here) or the flare small. No `_log.py` change: it backs three approved logs. Proposed
  pitfall text (Leaves, foliage and LODs, or a `_log` note): "A `_log.Spec` whose taper plus flare
  times 1.08 lump exceeds `r0 + 1.2 flare` overruns the butt cap's island; trade taper for flare."
- **Material swap by module globals.** `_log.build` calls `bark_high_material` and
  `wood_high_material` as module globals at call time, so a family file can set them at import time
  (`_driftwood.py`) and reuse every other step unchanged. Works; no skill change needed beyond the
  existing "set its module globals" rule.
- **Metal bakes under host load ~350** (saved time). `HEXW_BAKE_DEVICE=METAL` baked the full 1024²
  atlas (albedo, moss, normal, roughness, AO) in about 9 s per log while the CPU was saturated.
  Proposed SKILL text (Timing or stages): "Under heavy host load, bake on Metal
  (`HEXW_BAKE_DEVICE=METAL`); record the device, and compare bakes only on the same device."
- **Scatter yaw versus the beach slope.** The dry band slopes 0.04-0.24 across the coast, so a
  2.9 m log at a free yaw would float about 0.2 m at one end. The scatter lays driftwood along the
  coast instead (`scatter::beach_band`, `instance_yaw`, 12 degrees of jitter from the same yaw
  draw so no other rule's darts move); the sink only covers the jitter.
