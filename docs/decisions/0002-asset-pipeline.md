# 0002 — Asset pipeline (P1)

Date: 2026-10-13  Status: Accepted.

P1 took one kit family through the full pipeline at final quality: the meadow base, the road straight edge and centre (scripted), the hero oak (interactive), and the tileable sets grass, soil, asphalt and bark. This record lists the owner rulings as applied, with the file that evidences each, and the engineering decisions the pipeline rests on. The procedure itself lives in the `blender-asset` skill (`.claude/skills/blender-asset/`).

## Owner rulings as applied

### D2. Texel density

- **Standard.** Bases and road surfaces use a world-space tileable set at 512 px/m (1024² over tileM 2.0 m) plus a unique `masks` bake. Props and trees use a unique atlas at ≥ 256 px/m median.
- **Checked per material** (owner, hero oak look pass): one pooled median let the many leaf cards hide bark at 108 px/m. `kit_gate::texel_density_is_checked_per_material` measures every UV0 slot against the texture it samples.
- **Exception (owner, 2026-10-13).** Soil is tileM 4.0, so 256 px/m, giving one strata set per 4 m wall (`assets-source/lookdev/lookdev.json` `d2Exceptions`, `soilTileM`).
- **Measured** (`texelDensityPxPerM` in each `provenance.json`, `lookdev.json` `texelDensity`):

| Surface | Median px/m |
|---|---|
| grass, asphalt (tileable, world space) | 512 |
| soil (tileable, world space) | 256 (exception) |
| meadow_base masks | 31.99 |
| road_straight_edge / _center masks | 16.0 |
| tree_hero_oak bark (bark set on UV0, `tileUvScale` 12) | 532.7 |
| tree_hero_oak canopy shell | 339.5 |
| tree_hero_oak leaves (fringe cards) | 391.6 |

The road masks are 16 px/m rather than the plan's 32: the road `masks` PNG is 512² over the same 32 m square. The masks carry low-frequency snow, wetness, night and AO, and the owner approved the roads with them.

### D3. Texture budget

KTX2 bytes are budgeted per texture family (BC7 and ASTC each) by class: `piece` 1,500,000 B, `tileable` 3,600,000 B per set, `hero` 11,500,000 B (owner, 2026-10-13: full detail over initial size; hero pieces and their sets ship in streamed biome packs, not the initial download). The constants are `KTX_BUDGETS` in `crates/hex-assets/src/kit_manifest.rs` and `scripts/assets/kit-lib.mjs`, kept equal by `kit_gate::budgets_match_kit_lib`. **Final (owner, 2026-10-13): `piece` stays 1,500,000 B**, set from the measurements below; the constants did not change.

Measured (`budgets` in each `compress-report.json`):

| Class | Item | BC7 B | ASTC B |
|---|---|---|---|
| piece | meadow_base (max) | 235,073 | 243,545 |
| piece | road_straight_center | 8,252 | 8,122 |
| piece | road_straight_edge | 7,042 | 6,923 |
| hero | tree_hero_oak | 5,444,564 | 5,500,554 |
| tileable | grass (max) | 2,733,759 | 2,790,020 |
| tileable | asphalt | 2,384,685 | 2,413,052 |
| tileable | soil | 2,206,092 | 2,263,204 |
| tileable | bark | 2,040,344 | 2,043,045 |

Initial-download arithmetic, one family (a device downloads BC7 or ASTC, never both; PNG fallbacks are not fetched when a KTX2 family is supported), in decimal MB:

| Item | ASTC B | BC7 B |
|---|---|---|
| wasm (gzip, `make web-size`, this build) | 8,566,770 | 8,566,770 |
| IBL (`noon.specular` 4,194,680 + `noon.diffuse` 49,352) | 4,244,032 | 4,244,032 |
| core pieces' KTX2 (meadow, two roads) | 258,590 | 250,367 |
| core sets' KTX2 (grass, soil, asphalt) | 7,466,276 | 7,324,536 |
| core GLBs | 127,344 | 127,344 |
| **core total** | **20,663,012 (20.7 MB)** | **20,513,049 (20.5 MB)** |
| hero oak KTX2 + GLB + bark set (streamed pack) | 7,804,811 | 7,746,120 |
| **everything** | **28,467,823 (28.5 MB)** | **28,259,169 (28.3 MB)** |

Against 30 MB (30,000,000 B), the core is 20.7 MB with about 9.3 MB of headroom, and even with the hero pack loaded up front the family is 28.5 MB, 1.5 MB to spare. The largest measured piece uses 16% of the `piece` ceiling, and the ceiling still admits a 512² four-map unique atlas (about 1.15 MB at the measured 0.90-1.25 B per base pixel, `docs/perf/p1-texture-budget-probe.md`). What can break 30 MB is the sum of P2's pieces and sets, not one piece, so the owner ruled for a P2 gate that sums the initial download (wasm, IBL, core pieces and sets, one texture family) and fails above 30 MB, with hero assets streaming after the first interaction (PRD §15 P2).

### D4. Tone mapping

ACES fitted is the kit camera default (owner look-dev checkpoint, 2026-10-13), chosen over AgX and TonyMcMapface from the three-way shots. The kit camera exposure is EV100 12.5 (`lookdev::KIT_EV100`, from a sweep: Bevy's default 9.7 clips the 30,000 lx sun). Evidence: `assets-source/lookdev/lookdev.json` (`tonemap: "aces"`, `ev100: 12.5`, `approvals`), `assets-source/kit/meadow_base/lookdev-checkpoint.md`.

**Reopened 2026-10-14 (owner, review queue Q4): TonyMcMapface at EV100 12.9.** Walls facing away from the sun rendered pure black (0/255). Bevy's fitted ACES curve outputs black for any input below about 0.0033 linear. The shaded O4 soil walls reach about 0.0030, because the puresky HDRI has no lit ground to bounce light up and the soil's linear albedo is about 0.04. No change limited to the walls came close under ACES: doubling the IBL, ambient 400 or EV 11.5 reached about 1/255. The owner chose three changes together:

- `Tonemap::TonyMcMapface` is the default and `lookdev::KIT_EV100` is 12.9, for every kit, water and play camera (`GateConfig::default`). Superseded as the default by `rich-3` the same day (below); `--grade tony` keeps this look.
- The O4 soil is lightened ×2.5 in linear albedo: `"color": [2.5, 2.5, 2.5]` on every `soil` slot in `assets/kit/manifest.json`, mirrored by `WALL_TINT` in `water_terrain.wesl`. The set's textures are unchanged.
- The backdrop is explicit: `lookdev::BACKDROP` is the app's `ClearColor`. Under TonyMcMapface, Bevy's default clear colour would rise from about 15 to 47/255.

Measured on the shipped build against the ACES frames (`docs/checkpoints/shade-diagnosis/measurements.txt`, `a-current` against `o-shipped`, sRGB luma /255):

| Crop | ACES, EV 12.5 | Shipped |
|---|---|---|
| River-mouth cut walls | 0.01, 0.00 | 12.1, 12.0 |
| Bridge arch face | 7.8 | 31.1 |
| Meadow wall beside the beach | 2.2 | 27.8 |
| Water look-dev river cut wall | 0.6 | 8.0 |
| Grass | 128.2 | 124.9 |
| Gravel | 121.6 | 121.8 |
| River water (mouth, bridge) | 85.6, 86.8 | 99.7, 99.5 |
| Backdrop | 15.4 | 16.0 |

The water look-dev's river wall stays dark because its crop sits on the O4 profile's near-black foot. A ×20 `WALL_TINT` probe lifted it to 22.6/255, which shows that the factor reaches the shader. Plain ACES stays selectable with `--grade aces`. The kit and water look-dev shots are re-taken on this look before their approvals.

**Grades (owner, 2026-10-14 feedback: the Tony look feels washed out).** The tone mapper, exposure, Bevy `ColorGrading` and backdrop now come from one named preset, `hex_render::grade::Grade`, read by every kit, water and play camera: `tony` (the ruling above, the default until the pick below), `aces` (the 2026-10-13 look, with Bevy's default clear colour so its backdrop reads as before) and three in-between candidates, `rich-1` to `rich-3`. `--grade`, `?grade=`, the `set_grade` command and the Settings sheet switch them live. Settings and measurements: `docs/checkpoints/grades/`; comparison page: `docs/review/2026-10-14-grade-compare.html`. The default is the owner's pick.

**Default: `rich-3` (owner, 2026-10-14, after the grade comparison: "make color grade Rich 3 the default").** ACES fitted at EV100 12.5 with a +0.013 linear lift on the shadow and highlight sections before the curve (`RICH3_LIFT`), and its own backdrop, linear (0.0110, 0.0118, 0.0142), which reads about 15/255 under that curve. `Grade::default()` is `Rich3`, and `GateConfig::default()` takes its grade, tone mapper and exposure from it, so every kit, water and play camera, the look-dev shots, photo mode and the Legend thumbnails get it unless `--grade`/`?grade=` or a viewer's stored Settings choice picks another; `--tonemap` and `--ev100` still override the grade's own values. The Settings Colour grade rows label it Default (`state().grade.default`). It keeps ACES's colour and lifts the shaded walls out of black, though they stay darker than under `tony`. Measured in `docs/checkpoints/grades/measurements.txt` (sRGB luma /255 and HSV saturation):

| Crop | `aces` | `tony` | `rich-3` |
|---|---|---|---|
| River-mouth cut walls | 0.65, 0.63 | 12.2, 12.0 | 8.6, 8.5 |
| Bridge arch face | 7.8 | 31.1 | 17.8 |
| Meadow wall beside the beach | 22.4 | 30.8 | 22.2 |
| River-mouth grass | 128.0 | 125.3 | 128.9 |
| Backdrop | 15.4 | 16.0 | 15.3 |
| Meadow grass saturation | 0.629 | 0.510 | 0.599 |
| Roof saturation | 0.613 | 0.518 | 0.608 |
| Sea saturation | 0.399 | 0.334 | 0.396 |

### D5. IBL

- Source: Poly Haven `kloofendal_48d_partly_cloudy_puresky`, 2k equirect HDR, CC0 (https://polyhaven.com/license), 5,451,493 B, SHA-256 `5244534e…d198`. Gitignored. `make fetch-hdri` downloads it from the URL in `lookdev.json` and verifies the hash.
- Baked by `make lookdev-ibl` (`scripts/blender/lookdev_ibl.py`) into `assets/kit/p1/ibl/noon.specular.ktx2` (GGX-prefiltered RGBA16F cubemap, 256², 9 mips, 4,194,680 B) and `noon.diffuse.ktx2` (SH9 irradiance, 32², 49,352 B), pinned by `kit_gate::ibl_cubemaps_are_valid`. The equirect mapping matches Cycles (`test_ibl`).
- Intensity 1500 (`lookdev::IBL_INTENSITY`). The kit's directional sun is aimed at the HDRI sun, `sunDirection` in `lookdev.json`, 10.23° from the baked sun.
- With IBL on, the kit forward pass samples exactly 16 textures and the prepass 7 (`web-smoke` pins both).

### D6. `masks` channel contract

R = snow capacity, G = wetness/porosity, B = night emissive, A = baked ambient occlusion. A = AO was the plan's own choice to save a binding; the owner approved it at the look-dev checkpoint (`lookdev.json` `masksAlpha: "ao"`). Where a unique ORM carries the AO (leaf cards), masks A is 1.0. The tree bark carries its AO in masks A, because its ORM comes from the tileable bark set.

### D7. Float vertex attributes

Every vertex attribute stays FLOAT and no GLB uses `KHR_mesh_quantization`, because bevy_gltf 0.20 rejects that extension. Meshopt runs with its `QUANTIZE` method, which here only reorders and compresses and leaves the accessors FLOAT (asserted in `scripts/assets/kit-compress.mjs`, gated by `kit_gate::geometry_budgets_lods_sockets_border`). This also covers the P0 GLB (PRD §15 P0).

### D8. Frozen scale

R = 16 m, level step 4 m, apothem 13.856406 m, pointy-top, edge 0 facing +x. `scripts/blender/lib/hexgeo.py` is asserted against `crates/hex-core/src/axial.rs` (`test_hexgeo.test_constants_match_rust`). PRD §5.1 records them as final.

## Further owner rulings (2026-10-13)

- **Shadows.** The kit scene's first shadow cascade ends at 25 m (`lookdev::KIT_FIRST_CASCADE_FAR_BOUND`), which removed the kerb-wall acne at Bevy's default depth and normal bias. Raising the depth bias instead hid the acne but detached the kerb shadow. P0 keeps 120 m.
- **Tree bark.** The bark is the tileable `bark` set (1024² over 2 m) mapped on UV0 through the slot's `tileUvScale` (StandardMaterial `uv_transform`, no new binding). The piece atlas carries only the canopy, leaves and masks (bark AO). World-space tiling cannot map a vertical trunk.
- **Hero canopy.** Baked solid dome shells (one SECTORS-fold master covered in real leaf geometry, instanced per clump), plus outward-facing alpha fringe cards. The first card-only canopies failed the look. Approved in `assets-source/kit/tree_hero_oak/approvals.json`. Polish (canopy masks, smoother domes) is a P2 vegetation card.
- **Wind data.** Per-vertex wind is TEXCOORD_1 (U = sway weight, V = phase), not COLOR_0, which StandardMaterial would multiply into the base colour. The gate checks presence, range, a still trunk base and a swaying fringe (`kit_gate::tree_wind_uv_contract`).
- **Foliage back faces.** Alpha-masked slots shade their back faces with the front normal (`kit_material.wesl`, `kit_prepass.wesl`), so canopy undersides are not lit as if facing the ground. The rule keys on alpha mode Mask, so a future non-foliage cutout slot (a fence) would inherit it. Revisit with a slot flag if one appears.
- **Blend store.** Every `.blend` lives only in the main checkout's gitignored `assets-source/blend/<id>/`, resolved through `scripts/blender/lib/paths.py` (override `HEXW_BLEND_STORE`), so removing a worktree never deletes a source. Evidence: `scripts/blender/lib/paths.py` and `test_paths`, the `*.blend`, `*.blend[0-9]*` and `/assets-source/blend/` lines in `.gitignore`, and `blendRef` in `assets-source/kit/tree_hero_oak/provenance.json`.
- **Authoring.** Blender asset subagents run at high effort. Interactive sessions use EEVEE in Rendered viewport shading, with a prefixed showcase rig (lights, camera, ground) that the export stage deletes before any bake or export. Evidence: the Effort rule in `.claude/skills/blender-asset/SKILL.md`, `stages.md` ("Interactive route", steps 4 and 6), `assets-source/kit/tree_hero_oak/live/01_author.py` and the object filter at the top of `scripts/blender/kit/tree_hero_oak.py`.

## Engineering decisions

- **World-space tileable sets.** Bases and road surfaces sample their PBR set in world space (`kit_common::tile_frame`), so a piece rotated by 60°·k meets its neighbour with the same texels at the seam. A per-piece UV atlas would put a texture seam at every rotated edge. Only the unique `masks` bake uses UV0.
- **LODs by CDT regeneration.** Border-touching pieces rebuild every LOD with `geom.cdt_cap` from the same `hexgeo.border_ring(16)` at a coarser interior lattice, instead of decimating. That gives a bit-exact border ring in every LOD (`kit_gate::ring_must_match_across_lods_and_rotate_onto_itself`). Decimate's boundary protection was never verified to keep the ring exact.
- **Provenance pins the generator.** Every piece and set records `generator` and `generatorDeps` (the `_*.py` helpers it imports, transitively), and the gate re-hashes them and checks the list against the generator's imports. An approved piece's generator is therefore frozen until its next rebuild. `scripts/blender/lib/` is deliberately out of scope. Lib is shared by every piece and covered by `make blender-test` and `make kit-determinism`, and pinning it would stale every approved piece on any lib edit. The shipped bytes stay pinned by `derivedFiles`. For the same reason, `meadow_base.py` and `tree_hero_oak.py` still call `lib.bake._load`/`_write`, now aliases of the public `load_png`/`write_png`.
- **Provenance migration.** The provenance of `meadow_base`, the four sets and the gate fixture predates the `generatorDeps` field. During P1 close-out those files gained the list computed by `lib.export.generator_deps` in Blender (empty for all six), with every other field unchanged and no rebuild.
- **Interactive provenance.** An interactive piece must carry a store-relative `blendRef`, and its SHA-256 must equal the bake report's `blend.sha256`, which CI can check without the gitignored file. `make kit-provenance ROUTE=interactive` fails when the `.blend` does not resolve, instead of writing a record without `blendRef`.
- **Deterministic evidence renders.** Two six-view renders of one GLB had identical pixels but different bytes, because Blender writes the date and render time into PNG/JPEG metadata. `gate.render_six_views` turns both stamps off. The JPEGs committed before this change still carry the old metadata and change once, at each piece's next render.
- **Live steps are portable.** `scripts/blender/mcp_live.py exec` prepends `REPO` (the checkout root), so the committed `assets-source/kit/tree_hero_oak/live/*.py` steps hold no host path and replay after the P1 worktree is removed (`test_mcp_live`).
- **Bevy prepass tripwire.** `assets/shaders/kit_prepass.wesl` copies part of bevy_pbr's prepass shaders. `crates/hex-render/tests/bevy_prepass_tripwire.rs` hashes the upstream `render/pbr_prepass.wesl` and `render/pbr_prepass_functions.wesl` from the resolved crate, so a Bevy upgrade that edits them fails CI until the copy is re-ported.
- **Fixture copy in `dist`.** `assets/kit/p1/fixture/` (17 KB) is a byte copy of the gate fixture, so `kit_src=fixture` can serve it. Trunk copies all of `assets/`, so it ships in the production `dist`. It stays: it is 0.06% of the initial download, and the browser smoke test's fixture cases (including the clamped 16-texture case) run against `dist`. `make kit-fixture` now re-syncs it, and `served_fixture_matches_the_gate_fixture` fails on drift.

## Not closed in P1

- **Lane sockets.** `data/sockets.ron` does not exist until P2, so lane offset and width are checked against each socket's own extras only.
- **Alpha coverage per mip.** The leaf strip's coverage is measured by the generator (`bakeStats.leafAlphaCoverageByMip`) but not gated, and `kit-compress` mips are box-filtered, not coverage-preserving.
- **External `.blend` storage** is deferred by the owner (R1). The local SHA-256-referenced store is in use.

## Notes for P2

- **Road bends.** A bend from edge 0 to edge 2 (a 60° heading change, radius 24 m, arc 25.133 m) keeps the straight pieces' cross-section at both edges, but a whole number of dashes gives it a 5.027 m dash period, 9.3% shorter than the straight pieces' (n = round(25.133 / P) = 5 dashes over the arc). Decide in the bend task whether that is acceptable or the dash pattern changes.
- **Water and the texture limit.** The water prototype's refraction transmission texture would push the kit forward pipeline to 17 sampled textures when water and kit share a view. The baseline is 16, and the measured targets are 44-48. Resolved in P2 (2026-10-13): Bevy's `bluenoise_texture` feature bound a blue-noise texture (view binding 34) that nothing here reads. Without it the kit forward pipeline is exactly 16 next to refracting water, so there is no headroom: any new view texture (for example contact shadows, which need blue noise) breaks it again, and `make web-smoke` case `water_kit` catches that.
