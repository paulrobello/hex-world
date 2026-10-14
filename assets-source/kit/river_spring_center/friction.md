# river_spring_center friction

- **Lily pads float over water, but the masks field has R = 0 under water.** The pads sit at r < 4 m, where the field gives R (snow) = 0. The gate's hook check would then report "R is 0 everywhere" for `river_lily`. Fix (orchestrator ruling): every lily UV points at texel (0, 0) of the 256² masks. That texel lies at Blender (-15.94, -15.94), off the hex, where the field reads as dry meadow: R 1, G 0.8, B 0, A 1. No `SUBMERGED` entry is needed. 0 minutes lost.
- **The `_river_dress` helpers reuse cleanly.** `_stone`, `_reed_clump` and `_cattail` import without change, seated on `_spring.bed_z`. No skill change is needed.
