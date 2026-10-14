# ocean_center friction

- **A lane-less centre fails the road joint check.** `kit_gate::check_center_edge_joint` treats every `center` piece without `socket-lane-*` nodes as the road hub and demands sections at x = ±6 that match the edge pieces ("a LOD has no vertices on x = ±6"). Seabed detail has no lanes, so the piece is kind `prop` (1,500-tri LOD0 budget), which carries no border or joint checks; the scatter was trimmed from 72 to 46 pebbles and 7 to 6 rocks to fit. About 10 minutes. Folded into `pitfalls.md`.
- **Parallel `make kit-determinism` runs race.** Both pieces' determinism checks share `target/kit-det`, so running them at once deleted one run's output ("GLB differs", missing file). Run them one at a time. About 3 minutes. Folded into `pitfalls.md`.
- **First stones were too small to read.** 6-20 cm pebbles were sub-pixel at tile scale in the six-view render; widened to 15-35 cm pebbles and 0.55-1.0 m flat rocks (diorama exaggeration, as in ref-1). One rebuild.
- **Stones first kept to 0.25 m above the bed** as "detail, not floor". The water reads the prepass depth as its bed, and the stones are opaque and in the prepass, so they now keep to the floor's 0.10 m allowance (`_coast.BED_TOL_M`): rocks are 4-11 cm slabs. One rebuild of both pieces, about 10 minutes.

## §9.4 hook masks (2026-10-14)

1. **Snow and wetness are meaningless on the stones** (5 min deciding). Every pebble and rock lies within 0.10 m of the ocean floor, under at least 2.5 m of water at every sea level, which is the contract's case for R 0 and G 0 (the seabeds). The masks map is uniform (0, 0, 0, 1) at 32 px, so the `atlas_unwrap` UVs do not matter and the GLB is byte-identical; `coast_pebble` and `coast_rock` join the gate's `SUBMERGED` list. Under the snow hook the stones now stay dark under the water instead of taking the uniform default's full snow. No skill change: the contract already says R and G are 0 under the sea.
