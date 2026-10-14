# river_bend_base friction

Shared with the river family: see `assets-source/kit/river_edge/friction.md`.

1. **The contract tolerance binds every LOD** (about 30 min, owner decision). The first build met 0.03 m only at LOD0 (0.0297; LOD1 0.034, LOD2 0.32). Two error sources: linear interpolation across the bed's kink at d = 4.33 (0.027 m at the 0.488 m seam spacing) and chord sag between radial rows. Interior rows now also sample the kinks (3.031 and 4.33 m) and the check uses the contract's split (wet below y = -1.0, 0.03 m; drier bank 0.10 m) on every LOD. LOD0 33 rows x 17 points per side: 3,678 tris, wet 0.0274, dry 0.081. LOD1 18 rows: 1,704 tris, wet 0.0254, dry 0.071. LOD2 needs 16 rows; with 8 points per side it is 724 tris (19.7% of LOD0), wet 0.0283, dry 0.064, over the 15% ratio, so the owner extended the border-piece LOD floor: lane-cut bases may take LOD2 to 20%. Skill: `gates.md` (LOD floor) and `pitfalls.md` (Water beds).
2. **Dark band along the inner bank in the top render** (3 min). Checked the GLB: no down-facing face and no corner normal against its face; the band is the sun shadow of the inner bank in the look-dev light rig. No change.
