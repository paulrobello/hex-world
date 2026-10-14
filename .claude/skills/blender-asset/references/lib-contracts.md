# `scripts/blender/lib/` contracts

All positions are in the Blender frame (meters, Z-up) unless named `_rt`. Import as `from lib import <module>` (the runner and `make kit-build` put `scripts/blender` on `sys.path`). Tests live in `scripts/blender/tests/` and run with `make blender-test [TEST=<module>]` (local only).

Real generators (`scripts/blender/kit/<piece>.py`) must not import the `tests` package. The gate fixture generator is the one accepted exception.

## hexgeo
Hex frame shared by every kit generator. Constants are frozen (D8) and tested against `crates/hex-core/src/axial.rs`. Runtime (x, y, z) = Blender (x, z, -y). Runtime is pointy-top, edge 0 faces +x, edges run clockwise viewed from +Y, +z is south. In Blender's top view edge k's outward normal is at -60°·k.

- Constants `HEX_R = 16.0`, `LEVEL_STEP = 4.0` (= Rust `hex_core::axial::LEVEL_STEP`), `APOTHEM` (13.856406), `BORDER_SUBDIV = 16` (segments per hex edge on the border ring, identical in every LOD).
- `blender_to_rt(x, y, z) -> tuple` and `rt_to_blender(x, y, z) -> tuple`.
- `corner(i) -> (x, y)`: runtime corner i sits at 30° + 60°·i in the XZ plane (`scene.rs corner()`).
- `edge_midpoint(k) -> (x, y)`, `edge_normal(k) -> (x, y)`.
- `border_ring(subdiv=BORDER_SUBDIV) -> list[(x, y)]`: length `6*subdiv`, edge 0 first; edge k runs from corner k-1 to corner k.
- `dist_to_border(x, y) -> float`: positive inside.
- `lattice_points(spacing, margin) -> list[(x, y)]`: triangular lattice inside the hex, at least `margin` from the border, deterministic order.
- `rotate_xy(p, k) -> (x, y)`: rotate by k edge steps (edge e -> edge e+k), i.e. -60°·k.
- `lane_frame(edge=0, offset=0.0, width=6.0) -> dict(center, tangent, normal, width)`: center is the edge midpoint moved `offset` along tangent `(-n_y, n_x)`.

**Edge-generic contract (owner road requirement).** Every edge is edge 0 rotated: `border_ring(s)[k*s:(k+1)*s]` equals the edge-0 slice under `rotate_xy(·, k)` point for point, and `lane_frame(k, off, w)` equals `lane_frame(0, off, w)` rotated by k (center, tangent, normal). A turning road piece therefore reuses the edge-0 cross-section at any edge. Both are pinned by `test_hexgeo`, as is agreement with the kit_gate socket formula `(A·cos a + sin a·off, A·sin a − cos a·off)` at a = 60°·k (runtime x, z) on all six edges.

## scene
- `reset_scene(name)`: empty factory scene in metric units; no `.blend1` backups.
- `link(obj, collection=None)`: link into the collection (default: the scene collection), return obj.
- `empty(name, loc=(0,0,0), parent=None)`.
- `apply_transforms(obj)`: apply location/rotation/scale. Apply before joining.
- `select_only(objs, active=None)`: active defaults to `objs[0]`.

## geom
- `cdt_cap(name, border_pts, interior_pts, height_fn) -> Object`: constrained Delaunay of the border loop plus interior points. Border z = 0 exactly, interior z = `height_fn(x, y)`, faces up, smooth shaded. Every LOD built with the same `border_pts` shares the border ring.
- `skirt(name, border_pts, depth, rows) -> Object`: side walls from the ring (z = 0) down to z = -depth, `rows` quads high, triangulated, normals outward.
- `tri_count(obj) -> int`.
- `weld(obj, tol=0.001)`: remove doubles.
- `set_border_normals_up(obj, tol=1e-4)`: custom normals, +Z on the border ring so neighbouring caps shade identically (PRD §6).
- `join(objs, name) -> Object`: applies transforms, joins, and names both object and mesh data `name` (the runtime looks meshes up by name).

## materials
- `srgb(hex_str) -> (r, g, b, 1)`: `'#rrggbb'` to linear RGBA.
- `ensure_nodes(mat) -> NodeTree`: returns the node tree, touching the deprecated `use_nodes` only if there is none (Blender 5.x materials come with one).
- `pbr_material(name, base_color, roughness, metallic=0.0, emission=None) -> Material`.
- `torus_coords(nt, scale) -> (vector_socket, w_socket)`: seamless 4D coordinates `(cos 2πu, sin 2πu, cos 2πv)·s, w = sin 2πv·s` for a 1×1 UV bake plane, so the baked tile wraps on both axes. **4D textures only**: Noise, Voronoi and White Noise with `*_dimensions = "4D"` and W linked. A 3D texture ignores W and mirrors the tile; Wave, Magic, Brick, Gradient and Checker cannot wrap. Keep every feature scale ≥ 4 on a tileM 2 set, or the tile correlates with its own mirror (`pitfalls.md`, "Tileable sets").

## uv
- `planar_cap_uv(obj, layer="UVMap")`: unique UV0 for a cap, top-down planar map of the 2R × 2R square (32 m -> 0..1), U = (x + R) / 2R, V = (y + R) / 2R.
- `atlas_unwrap(objs, margin=0.008)`: one smart-project + pack atlas across objs.
- `measure_texel_density(obj, atlas_px, layer="UVMap") -> dict(min, median, max)`: px/m per face = sqrt(uv_area · atlas_px² / world_area). Faces with collapsed UVs are skipped.

## bake
- `setup_cycles(device=None, samples=16)`: device None -> `$HEXW_BAKE_DEVICE` or `"CPU"`; anything other than CPU selects Metal with `kernel_optimization_level = "OFF"`. Direct/indirect passes off, color pass on, tangent-space normals. Returns the device.
- `new_image(name, size, non_color) -> Image`.
- `bake_to(low, bake_type, image, highs=(), cage_extrusion=0.0, max_ray=0.0, margin=12, clear=True)`: bake_type in {AO, NORMAL, ROUGHNESS, EMIT}; empty `highs` bakes the low mesh onto itself. **Every scene mesh other than `low` and `highs` is `hide_render`ed during the bake and restored afterwards**, so other LODs and colliders cannot darken AO. Meshes already hidden stay hidden.
- `bake_emit_input(highs, low, input_name, image, /, **kw)`: bakes a Principled input (e.g. `'Base Color'`, `'Metallic'`) through EMIT so diffuse shading cannot darken it, then restores the materials. The four parameters are **positional-only**; passing `highs` (or `bake_type`) again in `**kw` raises `TypeError("... highs is positional ...")` before any material is touched. Build `kw` without `highs`.
- `save_png(image, path) -> str`.
- `pack_orm(ao_png, rough_png, metal_png, out_png)`: R = AO, G = roughness, B = metallic (each from the source's R channel).
- `make_masks(size, snow, wet, night, ao_png, out_png)`: linear; R snow capacity, G wetness/porosity, B night emissive, A AO (1.0 if `ao_png` is None). Channels are floats or `(size*size,)` arrays in Blender pixel order.
- `receiver_signature(objs) -> str`: hash of receiver topology, UVs and corner normals.
- `check_bake_cache(report_path, signature)`: raises `StaleBakeError` when the report is missing or its `receiverSignature` differs.
- `bake_tileable_set(material, size, out_dir, set_id, metallic=0.0) -> dict`: writes `<set>.albedo.png` (sRGB), `<set>.normal.png` and `<set>.orm.png` (linear, AO = 1) with margin 0 (a wrapping tile has no gutter). Returns `{albedo, normal, orm}` paths.
- `write_bake_report(path, **fields)`: JSON with sorted keys; adds `blender` version. A report that names its `generator` (tileable sets do) must give it repo-relative, `pathlib.Path(__file__).resolve().relative_to(export.REPO).as_posix()`, never `__file__` (an absolute host path under `-P`); the report then also records `generatorDeps`, which provenance requires.
- `load_png(path) -> (width, pixels)`: float32 RGBA `(w*h, 4)` in Blender pixel order, read Non-Color. `write_png(size, rgba, path, non_color=True) -> str`: the inverse; `non_color=False` tags it sRGB. (`_load`/`_write` are old aliases kept for already-approved generators.)
- `sha256_file(path) -> str`.

## lod
- `triangulate(obj)`.
- `decimate_to(obj, target_tris) -> int`: collapse-decimate to at most `target_tris`. Only for pieces that do not touch the hex border; border pieces regenerate their LODs with `cdt_cap` at a coarser lattice.
- `border_verts(obj, tol=1e-4) -> set`: world-space border-ring vertices at z = 0, rounded to 0.1 mm.
- `assert_border_identical(objs, tol=1e-4)`: raises `AssertionError` unless every obj has exactly `objs[0]`'s non-empty ring.

## collider
- `collider_from(src, kind, name) -> Object`: kind in {"cap" (copy of src, e.g. LOD2), "hull" (convex hull), "box" (AABB)}, world space, triangulated, custom prop `collider = kind`. Name convention `<piece>_collider`. It stays render-visible for export; the engine hides it. Unknown kinds raise `ValueError`. "hull" de-duplicates the vertices `convex_hull` lists as both unused and interior before deleting them (`test_collider`).

## paths
- `blend_store() -> Path`, `blend_path(piece) -> Path`, `blend_key(path) -> str`: the main-checkout `.blend` store (`project.md`, "The blend store"). Stdlib only, so system `python3` and the Makefile can call it: `python3 scripts/blender/lib/paths.py store | blend <id> | key <path>`.

## export
- `add_socket(name, loc, parent, yaw_deg=0.0, **extras) -> Object`: named empty, exported as a glTF node with extras. Names: `socket-lane-<n>` (network lanes), `anchor-prop-<n>` (prop anchors).
- `export_glb(root, path)`: one root per GLB, no images (`export_image_format="NONE"`), tangents on, float attributes, Y-up, extras on.
- `write_export_report(path, root, piece_id, generator_path, budget_class=None) -> dict`: `{pieceId, nodes, lodTris, sockets, generator{path, sha256}, generatorDeps, blender}`, plus `budgetClass` when `budget_class` is given (one of `BUDGET_CLASSES` = piece, tileable, hero; anything else raises `ValueError`). `kit-compress` budgets the piece's KTX2 by that class. `lodTris` keys are the `<id>_lod<n>` mesh names; socket positions are in the runtime frame. `generator.path` is stored repo-relative (POSIX, resolved from `lib`'s location) so committed provenance never carries an absolute host path; the generator must live inside the repo. `generatorDeps` comes from `generator_deps(generator_path)`: every sibling `_*.py` helper the generator imports with an absolute `import _x` / `from _x import` at any nesting depth, transitively, as sorted `[{path, sha256}]`, so a shared helper such as `kit/_road.py` is covered by provenance and re-hashed by the gate. `lib/` modules are deliberately outside this scope: they are shared by every piece and covered by `make blender-test` and `make kit-determinism`, and pinning them would stale every approved piece on any lib edit.
- **Determinism scope.** `test_export_is_deterministic` only proves same-process stability (two builds and exports in one Blender session give identical bytes). Cross-process byte determinism (fresh Blender runs) is `make kit-determinism`'s job.

## gate
- `import_glb(path) -> list[Object]`: resets the scene and imports a GLB (the importer converts back to Z-up).
- `render_six_views(objs, out_dir, png_dir, size=1024) -> list[str]`: EEVEE (`BLENDER_EEVEE` in Blender 5.2), AgX, date and render-time metadata off (byte-identical re-renders), ortho camera framed on the LOD0 bounds, with only `*_lod0` render-visible. Renders lossless `{front,back,left,right,top,three_quarter}.png` into `png_dir` (keep it under `target/`; `make kit-renders` uses `target/kit-renders/<id>/`), then writes the committed `<view>.jpg` (JPEG q85, at most 1024 px wide, about 22 KB each for the fixture) into `out_dir` and returns those paths. Evidence only; look-dev approval happens in engine.
- `reimport_check(glb_path, export_report_path, tol=0.001)`: every export-report node exists after reimport and every socket lands within `tol` of its exported runtime position; raises `AssertionError` listing all misses. `make kit-renders PIECE=<id>` runs it on `target/kit-stage/<id>/` before rendering.

## tests.fixture_piece (gate fixture only)
- `build(piece_id="fixture", spacings=(4.0, 8.0, 50.0)) -> (root, lods)`: 3-LOD hex (cap + one-row skirt, `border_ring(4)`), cap-copy collider, `socket-lane-0`. The committed gate fixture (`scripts/blender/kit/fixture.py`, `make kit-fixture`) passes `spacings=(1.5, 8.0, 50.0)` so LOD1/LOD2 stay within 50%/15% of LOD0; the lib tests keep the defaults.
