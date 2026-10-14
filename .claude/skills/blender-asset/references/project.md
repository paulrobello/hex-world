# Hex World project specifics

Everything in this file is specific to Hex World. When the skill is promoted to user level, this file stays with the project (or becomes a per-project overlay) and `SKILL.md` keeps only the portable pipeline. The frontmatter `description` in `SKILL.md` also names Hex World and is the one line to rewrite at promotion.

## Owner rulings (R1–R3, R9)
- **R1.** `.blend` sources and bake intermediates stay local, in the gitignored blend store (below), referenced by SHA-256 in provenance. External storage is deferred. The look-dev HDRI is gitignored too: `make fetch-hdri` downloads it into `assets-source/hdri/` and verifies the SHA-256 in `assets-source/lookdev/lookdev.json`.
- **R2.** Higgsfield reference images (meadow base, road straight edge/center, hero tree) are generated and owner-approved before any modeling.
- **R3.** Authoring agents are Opus 5.5 subagents. Every provenance records `"authoringAgent": {"harness": "claude-code", "model": "claude-opus-5-5"}`. At most 3 Blender asset subagents run concurrently.
- **R9.** Every timing measurement runs under `e2e/perf-load.mjs` (load ≤ 15 before and after).
- Owner checkpoints are reviewed from screenshots.

## Frame and pivot
- Constants come from `lib.hexgeo`: R = 16 m, step = 4 m, apothem 13.856406 m.
- Pointy-top, with edge 0 facing +x. Edges run clockwise viewed from +Y, and +z is south (`crates/hex-core/src/axial.rs`).
- Pivot at the hex center. Edge pieces are authored for edge 0, which faces Blender +X.
- Blender (x, y, z) exports as runtime (x, z, -y).
- Edge k's outward normal in Blender XY is at angle −60°·k.
- Base: cap border ring at y = 0 (runtime) and skirt bottom at y = −4.0. Tree: trunk base at the origin.

## Seams
- Pieces that touch the hex border build every LOD from `hexgeo.border_ring(BORDER_SUBDIV)`.
- Never decimate a border-touching piece. Regenerate it at a lower interior density instead.
- Road cross-sections at the edge are mirror-symmetric about the lane center.
- **Water beds** follow `docs/kit/water-contract.md` (the river and coast streams): every edge's bed profile depends only on its socket, and the engine computes depth and shore inputs from those analytic profiles, never from the kit geometry. So a bed is a requirement, not a look: check every exported LOD against its profile in the generator (`kit/_coast.py` `bed_profile_errors`: border exactly on it, the ocean floor within +0.10 m above -4.0).
- **A base ring below y = 0.** A base's cap ring may sit only at a contract level (`kit_gate` `BASE_RING_LEVELS`: 0 for Grass and Forest, -`LEVEL_STEP` for Ocean), flat all round. The lib's border helpers (`cdt_cap`, `set_border_normals_up`, `lod.assert_border_identical`) work at z = 0, so build each LOD there and then move its **vertex data** down (`mesh.transform`), never a node transform: the gate reads raw accessor positions (`ocean_base.py`). A new level (a beach ring that follows the coast profile, a deeper ocean) needs `BASE_RING_LEVELS` and the skirt-bottom check in `kit_gate.rs` to change with it.

## Textures
- The GLB embeds no images. Maps are PNG files named `<piece>.<channel>.png`, with channel in {albedo, normal, orm, emissive, masks}.
- albedo and emissive are sRGB. normal, orm and masks are linear (Non-Color).
- **Runtime obligation.** The PNG fallback carries no color space. PNGs for normal, orm and masks must load with `is_srgb = false`, and albedo and emissive with `is_srgb = true`. The KTX2 files carry their own vkFormat.
- masks is R snow, G wetness, B night emissive, A baked AO (owner-approved; 1.0 where a unique ORM carries the AO, such as leaf cards).
- **Walls under world-space tiling.** `kit_common::tile_frame` maps wall V to −y / tileM, so the cap rim (y = 0) sits exactly on the tile's V wrap, and tileM sets how many times the tile repeats down the 4 m skirt. A vertically banded set (soil strata) offsets its band coordinate so the rim lands inside a band, not on the wrap (`mat_soil.py`: `fract(V + offset + jitter)`, V recovered as atan2(w, z) / 2π). Soil uses tileM 4.0, one strata set per wall (owner, 256 px/m there).
- Each map ships as `<name>.<channel>.{bc7.ktx2,astc.ktx2,png}`. UASTC q4 + RDO + zstd 18 source, transcoded to BC7 (vkFormat 145 linear / 146 sRGB) and ASTC 4x4 (157 / 158), with a full mip chain.
- Texel density (D2): bases and road surfaces use a world-space tileable set at 512 px/m plus a unique `masks` bake at 32 px/m. Props and tree leaves use a unique atlas at ≥ 256 px/m median (measured). Density is checked **per material**, never as one pooled median, so many small faces of one material cannot hide another's low density (`kit_gate::texel_density_is_checked_per_material`).
- **Solid-dome canopy (tree, owner ruling; card-only canopies failed the look).** Clumps are closed low-poly dome shells (slot `{"unique": true}`, no masks) carrying a bake of one high-poly master dome covered in real leaf geometry over a solid dark dome at 0.97 R (`_tree.canopy_high`, saved in the `.blend` as `<id>_canopy_high`); alpha cards are only a thin fringe on the clump rims and undersides, each facing straight out from its clump centre. To keep the shell bake at ≥ 256 px/m in a 2048² atlas, the master and the shell are SECTORS-fold symmetric (3) and every sector shares one UV island set, and every clump is a transformed copy of the one baked shell. Set the shell's custom normals to the smooth dome's own normals before the bake (`shell_smooth_normals`), so the map carries leaf relief rather than low-poly facets; bend them toward up and out of the canopy after the bake. The shell slot does not sample masks, so its UVs may overlap the bark's (whose AO is in masks.A) in the same atlas region.
- **Foliage normals.** A slot with `alphaCutoff` (leaf cards) shades its back faces with the front normal (`kit_material.wesl`, `kit_prepass.wesl`), instead of Bevy's double-sided flip. Author card normals as one rounded canopy: blend the directions from the clump centre and the canopy centre with an up bias, and keep half the card's face normal so MikkTSpace never gets a normal in the card plane (`_tree.set_canopy_normals`).
- **Tree bark is a tileable set on UV0** (owner ruling, hero oak look pass): the `bark` set (1024² over 2 m = 512 px/m, `scripts/blender/kit/mat_bark.py`, `make kit-material SET=bark`) bound through the slot `{"tileable": "bark", "tileUvScale": K, "masks": true}`. World-space tiling cannot map a vertical cylinder (its tangent frame degenerates), so the bark is unwrapped per branch in metres (U around, V along: the grain follows every strand), scaled by `uvPerM` and packed unrotated into the piece atlas. At runtime StandardMaterial's `uv_transform` multiplies UV0 by K = 1 / (uvPerM · tileM) for the set's albedo, normal and ORM (forward and prepass), while `masks` keeps raw UV0, so the bark's own AO lives in masks.A. No binding is added: the set occupies StandardMaterial's existing three slots, and forward/prepass stay at 16/7 (web-smoke runtime-kit case). The oak uses uvPerM = 1/24, K = 12: bark 533 px/m median, AO 44 px/m.

## Budgets
Kit LOD0 ≤ 4000 tris and tree ≤ 3000. LOD1 ≤ 50% and LOD2 ≤ 15% of LOD0. GLB ≤ 2 MiB (2,097,152 B). Maps ≤ 2048². Full table: `gates.md`.

Buildings (houses) are kind `prop`, because PRD §6 lists houses among the props: LOD0 ≤ 1,500 tris, KTX2 class `piece`. A unique 256 px/m atlas for a house (about 150 m²) would need about 4096², far over that budget. So a house samples the world-space `plaster` set tinted per slot for its isotropic surfaces, models directional detail (tile courses, timber framing, window frames) as geometry, and bakes only a unique 512² `masks` (AO, plus night B on the glass). See `scripts/blender/kit/_house.py`.

KTX2 bytes are budgeted per family (BC7 and ASTC each) by budget class, per the owner ruling of 2026-10-13 (`KTX_BUDGETS` in `scripts/assets/kit-lib.mjs`):
- `piece` (the default): 1,500,000 B, for a piece's unique maps (final, `docs/decisions/0002-asset-pipeline.md` D3).
- `tileable`: 3,600,000 B per set. `make kit-material` assigns it automatically.
- `hero`: 11,500,000 B, for full-detail hero pieces such as the tree.

To mark a hero piece, call `export.write_export_report(..., budget_class="hero")` in its generator, which writes `"budgetClass": "hero"` into the stage's `export-report.json`. That is the durable way. A one-off run can pass `make kit-compress PIECE=<id> CLASS=hero` (`--class` on the CLI) instead. The class is recorded in `compress-report.json` under `budgets.class`, and `make kit-check` budgets the piece by that class. Hero pieces and their tileable sets ship in streamed biome packs. The 30 MB initial download keeps only the WASM, the core kit and the UI.

## Paths
| What | Where | Committed |
|---|---|---|
| Generators | `scripts/blender/kit/<piece>.py` | yes |
| Helper library | `scripts/blender/lib/` | yes |
| Blender tests | `scripts/blender/tests/test_*.py` (`make blender-test [TEST=<module>]`, local only, CI has no Blender) | yes |
| Staging | `target/kit-stage/<piece>/` | no |
| References | `assets-source/refs/<piece>/` | yes |
| Provenance, reports, renders (JPEG q85, ≤ 1024 px), friction | `assets-source/kit/<piece>/` | yes |
| Lossless six-view PNGs | `target/kit-renders/<piece>/` | no |
| `.blend` sources, bake intermediates | the blend store, `<main checkout>/assets-source/blend/<piece>/` (see below) | no (R1) |
| Runtime files | `assets/kit/` (manifest `assets/kit/manifest.json`) | yes |

### The blend store (owner ruling O2, 2026-10-13)
- Every `.blend` (and any `.blend1`) lives ONLY in the **main checkout's** gitignored `assets-source/blend/<id>/`, shared by every worktree, so a worktree removal never deletes a source. The high-poly is `<store>/<id>/<id>_high.blend`. Never save a `.blend` anywhere else, including the worktree's own `assets-source/blend/`.
- Resolve it through `scripts/blender/lib/paths.py` (stdlib only, tested by `test_paths`): `blend_store()` is `$HEXW_BLEND_STORE` or the parent of `git rev-parse --path-format=absolute --git-common-dir`, plus `assets-source/blend`; `blend_path(id)` and `blend_key(path)` (store-relative). CLI: `python3 scripts/blender/lib/paths.py store | blend <id> | key <path>`.
- Users: the export stage's `BLEND` default, the live save step, and `make kit-provenance ROUTE=interactive` (which defaults `BLEND` and passes `--blend-store`). Provenance records `blendRef {store: "local:assets-source/blend", key: "<id>/<id>_high.blend", sha256, bytes}`, never a host path.
- `.gitignore` ignores `*.blend` and `*.blend[0-9]*` everywhere as a backstop.
- If a `.blend` already sits in a worktree, `mv` it into the store (no second copy) and confirm its SHA-256 is unchanged.

## Tree wind data (TEXCOORD_1, controller ruling 2026-10-13)
Trees carry per-vertex wind data for a later vertex-shader wind pass (PRD §9.1, P2), so the asset needs no re-authoring when wind lands.
- **Attribute.** Blender UV map 2, named `wind`, exported as glTF `TEXCOORD_1` (FLOAT) on every primitive of every `<id>_lod{0,1,2}`. `UVMap` (TEXCOORD_0, the atlas) stays first and active: create `wind` after the atlas unwrap and pack.
- **U = sway weight** in [0, 1]: 0 at the trunk base (and on roots), rising along the branch hierarchy (trunk top 0.12, limb tips 0.65, twig tips 0.88, tertiary tips 1.0, each child starting at its parent's value at the fork), and toward 1 on the clump's outer leaf cards. Canopy dome shells carry `SHELL_SWAY` (0.35) times their clump's sway, one value per clump (the solid domes move less than their fringe). Derived from the skeleton (`_tree.skeleton()` `sway`), not world height. After decimation, U below 0.01 snaps to 0 so collapsed root tips stay still.
- **V = phase** in [0, 1]: a seeded hash per leaf card (all four corners equal, so the card moves as a unit) and per branch. The glTF exporter writes V as 1 − v; phase is cyclic, so the shader uses V as read.
- **Never COLOR_0.** StandardMaterial multiplies vertex colour into base colour and would tint the tree.
- **No new bindings.** It is a vertex attribute only: no texture, and the 16/7 sampled-texture counts are unchanged.
- **Shader use (P2).** Displace in object space by `wind_dir * strength * U * sin(time * freq + V * 2π)`, with U squared or scaled per LOD as tuning wants. LOD decimation interpolates U, so it is clamped back to [0, 1] after the LOD joins.
- **Trees need alpha-cutoff leaf cards.** The gate measures the sway of the fringe through the slots with `alphaCutoff`: their median U must be ≥ 0.5, and a tree with no alpha-cutoff slot fails. U must also stay ≤ 0.1 below runtime y 0.5 m.
- **Pinned by** `kit_gate::tree_wind_uv_contract` and the tree branch of `kit_gate::geometry_budgets_lods_sockets_border` (present on every tree LOD primitive, both channels in [0, 1], U = 0 on the lowest vertices). Meshopt QUANTIZE keeps it FLOAT. Debug renders: `scripts/blender/tree_wind_debug.py`.

## Tools and pins
- Blender 5.2.x at `BLENDER ?= /Applications/Blender.app/Contents/MacOS/Blender`. The make variable `BLENDER_RUN` expands to `"$(BLENDER)" -b --factory-startup --python-exit-code 1`.
- KTX-Software 4.4.2 from `make fetch-tools` (`. tools/ktx-env.sh` puts `ktx` on PATH).
- One gltf-transform pin, in `scripts/assets/package.json`: `@gltf-transform/{core,extensions,functions,cli}@4.3.0`, plus `gltf-validator` 2.0.0-dev.3.10, `meshoptimizer` 0.25.0 and `sharp` 0.34.4. Install with `cd scripts/assets && npm ci --no-audit --no-fund`. Run the CLI from that directory with `npx --no-install gltf-transform …`, never a global install.
- **Node dependency resolution.** Only `scripts/assets/` has a `node_modules`. ESM ignores `NODE_PATH`, so a script outside `scripts/assets/` (for example `scripts/refs/*.mjs`) loads a pinned dependency with:

  ```js
  import { createRequire } from "node:module";
  const require = createRequire(new URL("../assets/package.json", import.meta.url));
  const sharp = require("sharp");
  ```
