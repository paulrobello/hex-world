# car_sedan friction log

Shared by the car family (`car_sedan`, `car_van`, `car_pickup`; builder `scripts/blender/kit/_car.py`).

1. **Blender cannot export a FLOAT COLOR_0.** The 5.2 glTF exporter writes colour attributes only as
   normalized UNSIGNED_SHORT, and with `export_all_vertex_colors` (default on) it also adds a fake
   white UNSIGNED_BYTE COLOR_0, so the authored mask landed in COLOR_1. D7 (kit-compress and
   `kit_gate`) rejects every non-FLOAT vertex accessor. Fix: `_car.float_color0` rewrites the
   exported GLB (authored attribute becomes COLOR_0 FLOAT VEC4, other COLOR_n dropped, BIN
   repacked) before the export report. Folded into `pitfalls.md` ("Coordinate frame and export").
   Lost about 10 minutes (probe export).
2. **Material name collision.** Per-surface bake materials named `<id>_glass` / `<id>_tyre` made the
   export slots come out as `<id>_glass.001`. Bake materials now use `<id>_bake_<surface>`. No
   skill change: specific to a builder that bakes with one set of materials and exports another.
   Lost about 3 minutes.
3. **n-gon caps break tangents.** Loft caps and wheel hubs are n-gons; the exporter could not
   compute tangents ("triangulate the mesh first"). Triangulate every LOD before the unwrap and
   bake (pitfalls already says so for bakes). Lost about 2 minutes.
4. **LOD2 floor.** With lights, wheels and cab kept at LOD2, LOD2 sat at 162 tris, above 15 % of a
   1,022-tri LOD0. A rounder LOD0 shoulder (10-point ring) and single-face lamps at LOD2 fixed it;
   the pickup also dropped hidden bed-wall faces at LOD2. No skill change (the LOD-floor rule in
   `gates.md` covers it). Lost about 5 minutes.
5. **Worktree hook.** Inline Python heredocs that named `pid` and built paths in an f-string were
   refused as unverifiable; the same edits passed as Edit-tool calls. Already covered by the
   "Worktree sessions" rule in SKILL.md. Lost about 2 minutes.
