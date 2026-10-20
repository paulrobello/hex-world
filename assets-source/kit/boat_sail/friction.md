# boat_sail friction (2026-10-20)

Built with boat_rowing on the shared `_boat.py`, so the family entries are in `assets-source/kit/boat_rowing/friction.md`. That file covers the water drawn inside the hull, the animatable-part gate branch, the water height in `water_kit`, cell centres, and the boom against the shrouds.

1. **Frames through the floorboards.** The sail boat's raised floor (0.25 m over the keel) left the lower frame rows poking through the slats. Fix: `_boat.frames(..., z_stop=)` ends each rib at the floorboards. The rowing boat keeps full ribs, and its GLB is byte-identical across the change. About 3 min. No skill change.
2. **Draft against the coast shelf.** At 0.32 m the keel clears the river mouth fan's centre (0.58 m) but not the beach shelf joint (0.36 m, s = 3). The runtime notes give the rule: depth ≥ 0.45 m, s ≥ 3.6. No skill change: it is a routing rule for the boat runtime.
