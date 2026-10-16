# P1 Asset Pipeline Proof: Research Brief

Date: 2026-10-13. Scope: PRD §15 P1. One kit family goes through the full Blender pipeline at final quality: the meadow base, the road straight edge and center pieces, and one hero tree made through the Blender MCP. Sources are cited inline. "Unverified" marks claims that no source in this brief backs up.

## Headline findings

1. **par-factory's compress chain is three.js-specific and must not be ported as is.** It embeds KTX2 through `KHR_texture_basisu` (`gltf-transform uastc` / `etc1s`, `~/Repos/par-factory/scripts/compress-machine-models.sh:30-36`). bevy_gltf 0.20 lists that extension as unsupported (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bevy_gltf-0.20.0/src/lib.rs:122,129`). The hex-world route (a) instead ships separate per-family texture files that the runtime binds itself (`crates/hex-render/src/kit_test.rs:41-50`, `docs/decisions/0001-engine.md` §Asset route). P1 compress must therefore emit a GLB with no embedded images, plus per-channel `<piece>.<channel>.{bc7.ktx2,astc.ktx2,png}` files and a manifest that Rust turns into materials. ETC1S has no place in route (a). PRD §9.3 and §11.2 step 6 still name ETC1S and should be corrected.
2. **Mesh quantization is unsupported and P0 is not quantized.** bevy_gltf 0.20 marks `KHR_mesh_quantization` unsupported (`bevy_gltf-0.20.0/src/lib.rs:118`), and `crates/hex-assets/src/meshopt_glb.rs` has no dequantize path. An accessor dump of `assets/kit/p0/hex_test.glb` shows only `EXT_meshopt_compression`, with POSITION, NORMAL and TEXCOORD_0 all FLOAT (componentType 5126). The P0 criterion "meshopt-compressed, quantized GLB" (PRD §15 P0) therefore holds for compression but not for quantization. The `EncoderMethod.QUANTIZE` in `scripts/assets/build-p0-assets.mjs:56` is meshopt's encoder mode and does not quantize attributes. par-factory's `meshopt --quantize-position 16` (`compress-machine-models.sh:37`) would emit an extension Bevy rejects.
3. **The reimport gate must use the real loader.** the-cube's `scripts/check-assets.ts:9,71` validates with three's GLTFLoader, which accepts both extensions above, so a direct port would pass files Bevy cannot load. The P1 reimport check has to be a Rust test that runs `MeshoptGlbReader` (`crates/hex-assets/src/bevy.rs`) and then bevy_gltf.
4. **A unique bake cannot carry texel density at hex scale.** A 2k atlas over the 27.7 m flat-to-flat cap gives at most about 74 px/m (2048 / 27.7). After roughly 75% UV fill for a hex in a square, and before the skirt claims any space, that falls to 55-64 px/m. This is far below what close-ups and the P8 first-person view need. Bases and road surfaces need tileable or trim PBR materials for micro detail, plus a low-resolution unique bake for AO, the §9.4 hook masks and macro variation. The helper library therefore needs two paths: a selected-to-active bake path and a tileable-material path.
5. **Every texture is currently encoded as sRGB.** `Makefile:31` hardcodes `--format R8G8B8A8_SRGB` for every texture. Normal, ORM and mask maps must be created UNORM with a linear transfer function. Otherwise they transcode to BC7_SRGB / ASTC_SRGB and sample wrongly. The par-factory packing lesson is primaries `srgb` for albedo and `none` for linear data (external vault note `Projects/deterministic-pbr-material-packing-pipeline.md`).

## (a) Reusable pieces

| Piece | Source | What to port |
|---|---|---|
| High-to-low Cycles bake | `~/Repos/par-factory/scripts/blender/astra_family/astra_machine_common.py:903-1620` (`derive_baked_machine`) | Order: finish the low topology, triangulate (`:1147-1150`, so bake and export tangents match), one atlas via `smart_project` + `pack_islands` (`:1162-1173`), then bake. Base color and metallic are baked as EMIT through temporary emission rewiring, so diffuse shading cannot darken them (`:1386-1443`). Pack ORM as R=AO, G=roughness, B=metallic in numpy (`:1496-1512`). Emit a bake report with the source SHA, receiver signature, device and per-channel SHA (`:1592-1610`). Port the structure and drop the machine specifics (ports, rigs, `contracts.json`). |
| Bake-cache binding | `astra_machine_common.py:54` (`bake_receiver_signature`), `:1300-1311` | Hash the receiver topology, UVs and corner normals, and refuse a cached bake if they changed. |
| Component isolation | `astra_machine_common.py:1352-1380` | Explode matched high/low pairs so rays cannot hit neighbours. Assert the gap is greater than 2 × max ray distance and restore transforms afterwards. Needed for road curbs and props on bases. |
| Budgeted decimate LOD | `astra_machine_common.py:1137-1146` (LOD0 budget), `:1237-1262` (distance LODs with `use_collapse_triangulate`, protected geometry excluded) | Ratio math that excludes protected vertices. In hex-world, the protected set is the border vertex ring and the lane geometry. |
| GLB export call | `astra_machine_common.py:640-652`, `~/Repos/the-cube/blender/build_props.py:1210-1225` | `export_format="GLB"`, `use_selection`, `use_active_scene`, `export_yup=True`, `export_apply=True`, `export_extras=True` (carries socket metadata), `export_tangents=True`, no cameras or lights. |
| Socket empties with metadata | `~/Repos/par-factory/scripts/derive-modular-machine-bodies.py:382-401` (`add_socket`), `~/Repos/the-cube/blender/countryside_road.py:37-39` (`lib.empty` hitch and seat anchors) | Named empties carrying custom properties that export as glTF extras. |
| Socket reimport tolerance | `~/Repos/par-factory/scripts/verify-modular-machine-bodies.py:25-26,156,207` | Reimport the GLB in Blender and check socket position and axis to 0.001 m and mounting to 0.002 m. |
| Deterministic scene helpers | `~/Repos/the-cube/blender/lib.py:19-26` (`reset_scene`: factory settings, metric, no .blend1), `:46-55` (`srgb` hex to linear), `:58-98` (`material`), `:101-130` (`mesh_object`, `shade_smooth` via `set_sharp_from_angle`), `:133-366` (bmesh primitives, `tri_count`) | Port nearly verbatim. Drop the clearcoat default (`coat=1.0`), which suits the-cube's plastic look but not terrain. |
| Build, stage, check, promote | `~/Repos/the-cube/Makefile:50-62` | `blender --background --factory-startup --python-exit-code 1` builds into a staging directory, a checker validates it, then files are copied to the runtime directory. A Blender-independent `assets-check` runs in `checkall`. |
| Renders | `~/Repos/the-cube/blender/build_props.py:1228-1290` (`setup_render`, `frame_camera` from bounds, `render`) | Base for the 6-view render. |
| Compress gates | `compress-machine-models.sh:38-43`, `compress-scrap-bot.sh:27-48` | `gltf-transform validate`, a hard byte budget, and a compression block in the report (tool versions, settings, runtime SHA). Keep these and drop the uastc/etc1s/resize steps. |
| KTX tool fetch | `scripts/fetch-ktx.sh` (hex-world), the same pattern as `par-factory/output/tools/ktx-4.4.2` | Already ported. Pins KTX-Software 4.4.2. |

## (b) Recommended `scripts/blender/lib/` layout

All modules run under `blender -b --factory-startup --python-exit-code 1`. Generators in `scripts/blender/kit/<piece-id>.py` import them.

- `scene.py`: `reset_scene()`, `link()`, `empty()`, `collection()`, `set_units_metric()`, `apply_transforms(obj)` (apply before joining: external vault note `Patterns/blender-transform-identity-in-automated-glb-builds.md`).
- `hexgeo.py`: `HEX_R = 16.0`, `LEVEL_STEP = 4.0` (mirroring `crates/hex-core/src/axial.rs:3`), `hex_corners(r)` (pointy-top, corners at 30° + 60°·k), `edge_midpoint(k)` and `edge_normal(k)` in the Blender frame, `border_ring(subdiv)`, `rotate_edge(obj, k)`, `lane_frame(edge, offset, width)`. Constants come from one source, either read from a generated JSON or asserted against the Rust values in a test.
- `geom.py`: the the-cube bmesh primitives (`rounded_box`, `cylinder`, `lathe`, `prism`, `tube_path`, `merge`, `transform`, `solidify`), `tri_count()`, `weld(tol=0.001)`, `components()` (from external vault note `Patterns/glb/mesh-diagnosis-and-bounded-repair.md`).
- `materials.py`: `srgb()`, `pbr_material()`, `tileable_material(set_id, world_scale_m)` (the tileable path), `mask_attribute(name)`.
- `uv.py`: `atlas_unwrap(objs, margin)`, `set_texel_density(objs, px_per_m, atlas_px)`, `measure_texel_density(obj, atlas_px) -> (min, median, max)`.
- `bake.py`: `bake_setup(device="CPU", samples=16)`, `bake_selected_to_active(high, low, channels, size, cage, max_ray, margin)`, `bake_emit_channel(material_input)` (base color, metallic, night emissive), `bake_ao()`, `bake_normal_tangent()`, `pack_orm(ao, rough, metal)`, `bake_hook_masks(low, size)` (writes `masks` RGBA: R = snow accumulation, G = wetness/porosity, B = night-emissive intensity, A = reserved), `receiver_signature(meshes)`, `write_bake_report()`.
- `lod.py`: `protect_border(obj, ring_tol)`, `decimate_to(obj, target_tris, protected)`, `build_lods(lod0, ratios=(0.5, 0.15))`, `assert_border_identical(lods)`.
- `collider.py`: `collider_from(lod, kind="hull"|"box"|"cap")`, which names the node `collider` and hides it from render (pattern: `derive-modular-machine-bodies.py:602-611`).
- `export.py`: `add_socket(name, loc, yaw, **extras)` (`socket-lane-<n>`, `anchor-prop-<n>`), `set_pivot(kind="hex_center")`, `export_glb(root, path)` with no images embedded (materials keep names and factors only), `export_textures(maps, dir)` to PNG.
- `compress.py` (a Node/shell step, not bpy): `encode_texture(png, channel)` (picks sRGB or linear), `uastc()`, `transcode(bc7|astc)`, `meshopt(glb)`, `validate()`, `budget()`.
- `gate.py`: `reimport_check(glb)` (Blender side: names, sockets, bounds), `render_six_views()`, `seam_pair_render()`, `write_provenance()`.

The Rust-side gate lives in `crates/hex-assets/tests/kit_gate.rs`. It loads every staged GLB through `MeshoptGlbReader` and bevy_gltf, then checks names, sockets, LOD ratios and the border ring.

## (c) Gate checks and thresholds

| Check | Threshold | Source |
|---|---|---|
| Kit piece LOD0 tris | ≤4k typical, ≤12k hero | PRD §10 |
| Prop LOD0 / tree | ≤1.5k / ≤3k with leaf cards | PRD §10 |
| LOD1 / LOD2 | ≤50% / ≤15% of LOD0 | PRD §10; par-factory spec 09 §6 |
| LOD switch | 150 m / 400 m, band 5 m | `crates/hex-render/src/scene.rs:68-81` |
| Per-GLB bytes | ≤2 MB (par-factory uses 1.5 MB) | PRD §10; `compress-machine-models.sh:39` |
| Texture size | ≤2k per atlas | PRD §10 |
| Texture bytes per piece | none defined; see (g) | gap |
| Socket position / axis | ≤0.001 m | `verify-modular-machine-bodies.py:25` |
| Mount / plane tolerance | ≤0.002 m | `verify-modular-machine-bodies.py:26` |
| Bounds and origin | finite; base at y=0 within 0.005 m; size within 0.02 m | `the-cube/scripts/check-assets.ts:84-101` |
| Frame | marker at edge-0 midpoint reimports at (13.856, y, 0) | `axial.rs:1,11,22-26` |
| Seam: border ring | identical across LOD0/1/2; positions match within 1e-4 m after a 60°·k rotation; shared border normals | PRD §6 (seam continuity) |
| Seam: lanes | road lane offset and width at edges 0 and 3 match `data/sockets.ron` | PRD §5.2 |
| Seam: visual | two road tiles side by side, in-engine close-up, owner-approved | PRD §15 P1 |
| Tangents | TANGENT present on every primitive | `astra_machine_common.py:1580-1584` |
| Texture encoding | `ktx info`: sRGB transfer for base color and emissive only; linear for normal, ORM and masks; mip count = log2(size)+1 | finding 5; external vault packing note |
| Alpha coverage (tree) | per-mip coverage within a set tolerance of mip 0 (tolerance to be chosen in P1) | — |
| Determinism | double-run SHA identical for generator and GLB; bakes compared by hash on the same device only | external vault transform-identity and packing notes |
| Bake preservation | bake report lists device, ray distance, receiver signature, map SHAs; no directional lighting baked | `astra_machine_common.py:1592-1610` |
| Reimport | Rust loader test plus Blender reimport (names, sockets, materials, ratios) | findings 2-3 |
| Renders | 6-view render plus an in-engine `?shot=` screenshot | PRD §11.2 step 7 |
| Blender failures | the run must use `--python-exit-code 1`, because Blender otherwise exits 0 on a traceback | external vault note `Tools/blender-script-failures-exit-zero.md` |

## (d) Provenance JSON fields

These merge PRD §11.4 with `~/Repos/par-factory/docs/specs/09-asset-pipeline.md` §4 and the real records (`par-factory/assets-source/models/prime-cradle/v2/provenance.json`, `.../astra-family/smelter/v1/hashes.json`, `bake-cache.json`):

`pieceId`, `version`, `route` (`scripted`|`interactive`), `generator` {`path`, `sha256`} or `blendRef` {`store`, `key`, `sha256`, `bytes`}, `authoringAgent` {`harness`, `model`}, `blender` (`version_string`), `toolVersions` {`gltfTransform`, `meshoptimizer`, `ktxSoftware`, `node`}, `bake` {`device`, `samples`, `cageExtrusionM`, `maxRayDistanceM`, `marginPx`, `receiverSignature`, `channels{name: {size, colorspace, sha256}}`}, `lods` [{`lod`, `tris`, `ratio`}], `collider` {`kind`, `tris`}, `sockets` [{`name`, `position`, `yaw`, `extras`}], `pivot`, `texelDensityPxPerM` {`min`, `median`, `max`}, `compression` {`meshopt`, `textureEncodes[{channel, family, format, transfer, sha256, bytes}]`}, `derivedFiles` [{`path`, `sha256`, `bytes`}], `references` [{`path`, `sha256`, `provider`: `higgsfield`, `prompt`}], `license`, `validation` {`pass`, `date`, `checks[]`}, `approvals` [{`who`, `when`, `gate`}]. As in par-factory spec 09 §4, a piece with missing provenance or a failing validation must not enter `manifest.json`.

## (e) Known pitfalls and fixes

- **Blender exits 0 on script errors.** Always pass `--python-exit-code 1` (external vault note `Tools/blender-script-failures-exit-zero.md`, external local guide `~/.claude/guides/blender-mcp.md:167-173`).
- **Blender 5.2 API drift.**
  - Use `BLENDER_EEVEE`, not `BLENDER_EEVEE_NEXT` (`blender-mcp.md:120`).
  - Leave `export_loglevel` unset (`blender-mcp.md:134`).
  - An empty RNA enum list does not mean a format is missing (`blender-mcp.md:128-133`).
  - Principled inputs are named `Emission Color` and `Coat Weight` (`the-cube/blender/lib.py:85-89`).
  - Sharp edges use `set_sharp_from_angle` (`lib.py:125`).
  - SCENE-mode animation export needs `use_active_scene=True` (`astra_machine_common.py:637-639`).
  - Further 5.x renames have not been checked here (unverified).
- **Cycles bake gotchas.**
  - A diffuse pass darkens base color, so bake base color and metallic through EMIT (`astra_machine_common.py:1386-1416`, `blender-mcp.md:76-81`).
  - Triangulate before baking so the bake tangents and the exported tangents agree (`:1147`).
  - Set `use_clear` only on the first group (`:1450`).
  - Keep `use_pass_direct` and `use_pass_indirect` off (`:1332-1333`).
  - Image colorspace must be sRGB for base color and emission and Non-Color for everything else (`:1397`).
  - Metal kernels recompile per channel, so `kernel_optimization_level = "OFF"`. The bake device defaults to CPU through `ASTRA_BAKE_DEVICE` (`:1315-1327`).
  - `margin=0` leaves no gutter and mips bleed. Use about 8-12 px, with two masks where preservation matters (external vault note `Patterns/bounded-gutter-texture-bake-preservation.md`).
  - Cage 0.055 m and max ray 0.14 m (`:1329-1330`) were tuned at machine scale. Re-derive them per piece for 32 m bases rather than copying them.
- **Coordinate frame.**
  - Blender Z-up exports to glTF Y-up as glTF x = Blender x and glTF z = −Blender y (`the-cube/blender/lib.py:1-4`).
  - Runtime edge 0 faces +x, edges run clockwise viewed from +Y, and +z is south (`axial.rs:1,11`). In Blender's top view, edge k's outward normal is at −60°·k.
  - Use a hex-center pivot for every piece. Author edge pieces for edge 0 and rotate them at runtime (PRD §6).
  - Apply transforms before joining. Check the result with root-relative positions on reimport, not in the viewport (external vault note `Patterns/par-factory-asset-pipeline-rules.md`).
- **GLB export.**
  - Each parentless root becomes one GLB in multi-root exports, so use one root per piece (external vault rules note).
  - `export_extras=True` is needed to carry socket metadata.
  - Hidden-from-render objects can drop out of selected exports (`blender-mcp.md:232-233`). Keep the collider render-visible during export and hide it in the engine.
  - Export no images, because textures are separate files (finding 1).
- **KTX2 BC7/ASTC: one UASTC source or separate encodes.**
  - KTX-Software 4.4.2 `ktx create --encode` accepts only `basis-lz` or `uastc`, so BC7 is reachable only by transcoding (`ktx transcode --target bc7`).
  - ASTC can be encoded directly with `--format ASTC_4x4_*_BLOCK` and the built-in astcenc (`--astc-quality`, `--astc-perceptual`), or transcoded from UASTC (`ktx create/transcode --help` from `par-factory/output/tools/ktx-4.4.2`). P0 transcodes both from one UASTC q2 source (`Makefile:31-33`).
  - Recommendation: keep the single UASTC source with `--uastc-quality 4`, `--uastc-rdo-l` 0.25-0.75 for normals, and `--zstd 18`. This gives one encode, two cheap transcodes and identical content across families.
  - Encode ASTC directly only if a measured PSNR shortfall on the P1 maps demands it.
  - Normals may use `--target bc5` (two channels). Bevy's two-component normal reconstruction for BC5 is unverified here.
  - Mips: albedo must be averaged in linear light and normals renormalized at each level (external vault packing note).
  - P0's 1024² BC7 + zstd file is 594,157 B, larger than its 371,810 B PNG (`assets/kit/p0/`). Measure in P1 how much UASTC RDO reduces transcoded BC7 size.
- **Quantization.** Either keep FLOAT attributes, or add dequantization to `meshopt_glb.rs` before quantizing. If P1 quantizes, it must use one shared quantization volume, the hex bounds. With per-mesh bounds, the border vertices of neighbouring pieces snap to different grids and the seams crack.
- **LOD seams.** `VisibilityRange` applies per chunk (`scene.rs:357`), so neighbouring tiles can sit at different LODs. Pin the border ring in every LOD and choose its subdivision so LOD2 can still reach 15%.
- **Look-dev parity.** Blender's view transform does not match Bevy's tonemapper. The 6-view Blender renders are evidence only, and approval happens in engine (par-factory spec 09 §1.4).

## (f) Tooling inventory (measured 2026-10-13)

| Tool | Status | Version / note |
|---|---|---|
| Blender | installed | 5.2.2 LTS (build 2026-09-15), `/Applications/Blender.app/Contents/MacOS/Blender` |
| gltf-transform CLI | on PATH (nvm node 22.20.0) | 4.3.0. hex-world `scripts/assets/package.json` pins the libraries at 4.3.0, par-factory pins the CLI at 4.5.0. Pick one pin and run it via `npx --package @gltf-transform/cli@<pin>`. |
| meshoptimizer (npm) | project dependency | 0.25.0 (`scripts/assets/package.json`). Rust decoder `meshoptimizer-rs` =0.2.1. |
| ktx / toktx | not on PATH, and `hex-world/tools/` does not exist | 4.4.2 is present only under `par-factory/output/tools/ktx-4.4.2`. `make fetch-tools` installs it for hex-world. |
| higgsfield CLI | installed (nvm node 24.18.0) | 1.1.23 (built 2026-08-08) |
| basisu, gltfpack | missing | Not needed under route (a). |
| sharp | project dependency | 0.34.4 |

## (g) Open decisions for the owner

- **D1. External `.blend` storage.** Recommendation: a Cloudflare R2 bucket keyed by SHA-256, with the key and hash in provenance. An `assets-pull` target verifies the hash on download. This follows PRD §11.6's no-LFS rule. par-factory's spec 09 §9 left the same question open after LFS quota pressure.
- **D2. Texel density standard.** Recommendation:
  - Unique bakes of AO, masks and macro color at about 32 px/m for bases (a 1k atlas suffices).
  - Tileable or trim materials at 512 px/m for terrain and road surfaces.
  - About 256 px/m unique for props and the hero tree.
  - Enforce these with `measure_texel_density` in the gate.
  - Owner input is needed on how close the P8 camera gets.
- **D3. Texture byte budget.** PRD §10 caps GLB bytes, but sidecar textures are not counted. Recommendation: at most 1.5 MB of KTX2 per piece per family, and the core kit within the 30 MB initial download. Set the final number after measuring the RDO size in P1.
- **D4. Tone mapping.** Bevy 0.20 defaults to `TonyMcMapface`, which needs the `tonemapping_luts` feature (`bevy_render-0.20.0/src/view/mod.rs:283-330`). hex-world sets no `Tonemapping` today. Recommendation: compare AgX, TonyMcMapface and AcesFitted in engine on the P1 family, and choose AgX unless saturated greens clip. Confirm that the feature set includes `tonemapping_luts`.
- **D5. IBL.** Recommendation: one prefiltered HDRI (CC0) baked to KTX2 environment and irradiance maps, fixed at the noon time-of-day value (§9.4). Record its hash in look-dev provenance.
- **D6. Hook-mask channel contract.** Recommendation: a separate linear `masks` RGBA texture (R snow, G wetness/porosity, B night-emissive intensity), read by an `ExtendedMaterial` over StandardMaterial. Snow could instead live in a vertex color, as §9.4 allows. Vertex color saves a texture but has too coarse a resolution on 4k-tri bases.
- **D7. Quantization.** Recommendation: ship FLOAT attributes in P1, because meshopt still compresses them. Add dequantization in `meshopt_glb.rs` later if the measured byte gain justifies it.
- **D8. Scale constants.** Keep R = 16 m and the 4 m level step unless the P1 look-dev or the seam close-ups show a reason to change them. Freeze both in `hexgeo.py` and assert them against `axial.rs`.
- **D9. Authoring agent.** `~/.claude/guides/game-assets.md` routes Blender authoring to `gpt-6-astra`. PRD §11.1 overrides this with Opus 5.5 subagents. Confirm that the PRD wins for hex-world, and record it in each provenance `authoringAgent`.
- **D10. PRD corrections.** Remove ETC1S from §9.3 and §11.2 step 6, because route (a) is UASTC transcoded to BC7/ASTC. Note in the P0 asset criterion that the GLB is not quantized.
