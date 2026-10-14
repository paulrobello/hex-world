# P1 Asset Pipeline Proof Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One kit family goes through the full Blender pipeline at final quality: a shared Blender helper library, the meadow base, the road straight edge and center (scripted), and one hero tree (interactive, via the Blender MCP). Each piece passes the §11.2 gates, renders in engine with look-dev decided, and carries complete provenance. Two road tiles show no seam in an owner-approved close-up.

**Architecture:** Headless Blender 5.2 generators (`scripts/blender/kit/<piece>.py`) import a tested helper library (`scripts/blender/lib/`). They write an uncompressed GLB with no images, PNG maps and JSON reports into `target/kit-stage/<piece>/`. A Node step (`scripts/assets/kit-compress.mjs`) meshopt-compresses the GLB with float attributes and encodes every map to UASTC, then transcodes to BC7 and ASTC KTX2 with zstd, with the correct sRGB/linear format per channel. It enforces budgets and promotes the files to `assets/kit/p1/`. A pure-Rust gate (`crates/hex-render/tests/kit_gate.rs`) loads every manifest piece through the `kit://` meshopt rewrite and `bevy_gltf`, checks geometry, sockets, LODs, seams, KTX2 headers, budgets and provenance hashes, and runs in `make test` on CI. At runtime a `kit` scenario builds `ExtendedMaterial<StandardMaterial, KitExt>` materials from `assets/kit/manifest.json`: tileable layers are sampled in world space, and the `masks` texture carries the §9.4 hooks. LODs use `VisibilityRange`.

**Tech Stack:**
- Blender 5.2.2 LTS (`/Applications/Blender.app/Contents/MacOS/Blender`) with Cycles CPU bakes and bundled numpy.
- Node 22 with `@gltf-transform/*` 4.3.0 (one pin, CLI included), `meshoptimizer` 0.25.0, `gltf-validator` 2.0.0-dev.3.10 and `sharp` 0.34.4.
- KTX-Software 4.4.2 (`make fetch-tools`).
- Rust with Bevy `=0.20.0`, `gltf` 1 and `sha2` 0.10 (dev).
- higgsfield CLI 1.1.23 (`nano_banana_pro`).
- The Blender MCP (`mcp__Blender__*`) for the hero tree.

**Spec:** `PRD.md` §5 (tiles, sockets), §6 (kit), §9.3–9.4 (formats, hooks), §10 (budgets), §11 (asset pipeline), §15/P1. Technical basis: `docs/superpowers/research/2026-10-13-p1-asset-pipeline-brief.md` (cited as "brief"). Runtime asset route: `docs/decisions/0001-engine.md` §Asset route.

## Global Constraints

- **Owner rulings are binding.** R1–R10 below are referenced by code throughout this plan.
  - **R1.** `.blend` sources and bake intermediates stay local, in the gitignored `assets-source/blend/`, referenced by SHA-256 in provenance. External storage is deferred, so do not build R2 or any upload.
  - **R2.** Higgsfield reference images (meadow base, road straight edge/center, hero tree) are generated and owner-approved before any modeling.
  - **R3.** Authoring agents are Opus 5.5 subagents. Every provenance records `"authoringAgent": {"harness": "claude-code", "model": "claude-opus-5-5"}`. At most 3 Blender asset subagents run concurrently.
  - **R4.** Every Blender task's implementer loads `.claude/skills/blender-asset/SKILL.md` first and follows it.
  - **R6.** GLBs embed no images. Each map ships as `<name>.<channel>.{bc7.ktx2,astc.ktx2,png}`. Materials are built in Rust. `normal`, `orm` and `masks` are linear. `albedo` and `emissive` are sRGB.
  - **R9.** Every timing measurement runs under `e2e/perf-load.mjs` (load ≤ 15 before and after).
- **Scale is frozen (D8).**
  - `HEX_R = 16.0` m, `LEVEL_STEP = 4.0` m and apothem `13.856406` m.
  - Pointy-top, with edge 0 facing +x. Edges run clockwise viewed from +Y, and +z is south (`crates/hex-core/src/axial.rs`).
  - In Blender (Z-up), the glTF/runtime position is `(x, y, z)_rt = (x_b, z_b, -y_b)`.
  - Edge k's outward normal in Blender XY is at angle `-60°·k`.
- **Pivots.**
  - Base: hex center, with the cap border ring at y = 0 and the skirt bottom at y = −4.0.
  - Edge and center pieces: hex center, authored for edge 0, rotated at runtime.
  - Tree: trunk base at the origin.
- **Budgets (PRD §10).**
  - Kit piece LOD0 ≤ 4,000 tris.
  - Tree LOD0 ≤ 3,000 tris.
  - LOD1 ≤ 50% and LOD2 ≤ 15% of LOD0.
  - GLB ≤ 2,097,152 B.
  - Maps ≤ 2048².
  - KTX2 bytes per piece per family ≤ 1,500,000 B (D3, provisional; re-set in Task 11 from measurement). Tileable material sets are budgeted separately, at the same 1,500,000 B per set per family.
- **Formats.**
  - No `KHR_mesh_quantization`. POSITION, NORMAL, TANGENT and TEXCOORD stay FLOAT (D7).
  - No `KHR_texture_basisu` and no ETC1S.
  - UASTC q4 + RDO + zstd 18 source, transcoded to BC7 (vkFormat 145 linear / 146 sRGB) and ASTC 4x4 (157 / 158).
  - A full mip chain: `levels = floor(log2(max(w, h))) + 1`.
- **Texel density (D2).**
  - Bases and road surfaces: a world-space tileable PBR set at 512 px/m (a 1024² tile covers 2.0 m), plus a unique `masks` bake at 32 px/m (1024² over the cap).
  - The tree and props: a unique atlas at ≥ 256 px/m median (measured, not assumed).
- **`masks` channel contract (D6).**
  - R = snow accumulation, G = wetness/porosity, B = night-emissive intensity.
  - A = baked ambient occlusion for tileable-path pieces, and 1.0 elsewhere. **The A assignment is this plan's design choice, not an owner ruling.** It saves one texture binding. It is flagged for owner review in Task 7.
- **Determinism.** Generators run with `--factory-startup` and seeded RNG. Two runs give an identical GLB SHA-256. Bakes are compared by hash only on the same device.
- **Blender invocation.** Always `"$BLENDER" -b --factory-startup --python-exit-code 1 -P <script> -- <args>`, because Blender exits 0 on a traceback without the flag. `make` exposes `BLENDER ?= /Applications/Blender.app/Contents/MacOS/Blender`.
- **Directory layout.** Runtime files live in `assets/kit/` (the repo's P0 route, shipped by Trunk's `copy-dir`), not PRD §11.3's `web/public/assets/`. Task 11 corrects the PRD.
- **Commit and board rules.**
  - Commits never carry a co-author trailer. Never push.
  - Sub-agents run no `kanban` command.
  - The controller commits each task after verifying it.
- **Gates.**
  - `make checkall` must pass at the end of every task.
  - Blender tests (`make blender-test`) are local-only, because CI has no Blender. Everything checked against committed files is pure Rust and runs in CI through `make test`.

## Review Focus

1. **A neighbor at every rotation.** A road edge piece rotated 180° (edge 3) meets an unrotated one (edge 0), and meadow bases sit at all six rotations. The border vertices coincide and the road cross-section is mirror-symmetric, so there is no crack or step. Pinned in Task 5 (`kit_gate::geometry_budgets_lods_sockets_border`, which runs the base border-ring rotation check and `check_edge_profile`) and Task 9 (seam close-up).
2. **A LOD0 tile next to a LOD2 tile.** `VisibilityRange` is per piece, so neighbors can differ in LOD. The border ring is identical in all three LODs, so no crack opens. Pinned in Task 5 (`kit_gate::geometry_budgets_lods_sockets_border`: border ring and edge cross-section identical across LODs) and Task 3 (`test_cap_lods_share_border`).
3. **A device with only ASTC, or only PNG.** Linear maps (normal, ORM, masks) must not be sampled as sRGB. KTX2 vkFormat is checked per channel, and the PNG path loads linear channels with `is_srgb = false`. Pinned in Task 5 (`kit_gate::ktx2_color_space_mips_and_budgets`, `kit_gate::gate_catches_wrong_color_space`) and Task 6 (`png_family_loads_linear_channels_linear`).
4. **A manifest entry whose file is missing or stale.** The gate fails loudly and names the file. The runtime logs a warning and skips the piece without panicking. Pinned in Task 5 (`kit_gate::gate_catches_missing_file_and_stale_provenance`) and Task 6 (`missing_piece_is_skipped_not_panic`).
5. **A Blender traceback or a stale cached bake that passes as success.** The runner exits non-zero, and a bake whose receiver signature changed is refused. Pinned in Task 1 (`make blender-test` self-check with a deliberately failing script) and Task 3 (`test_stale_bake_refused`).

---

## File Structure

```
.claude/skills/blender-asset/SKILL.md                # T1: pipeline skill (R4)
.claude/skills/blender-asset/references/stages.md    # T1: stage-by-stage commands
.claude/skills/blender-asset/references/lib-contracts.md  # T1, T3: lib function contracts
.claude/skills/blender-asset/references/gates.md     # T1, T5: gate list and thresholds
.claude/skills/blender-asset/references/provenance.md     # T1, T5: provenance schema
.claude/skills/blender-asset/references/pitfalls.md  # T1, T11: brief §(e) + P1 friction
.gitignore                                           # T1: assets-source/blend/, assets-source/hdri/
Makefile                                             # T1, T4, T5, T6, T7: blender-test, kit-*, refs, lookdev-ibl, shots-kit
scripts/assets/package.json                          # T1: + @gltf-transform/cli, gltf-validator
scripts/blender/run_tests.py                         # T1: headless unittest runner
scripts/blender/tests/test_runner_selfcheck.py       # T1
scripts/blender/lib/__init__.py                      # T3
scripts/blender/lib/hexgeo.py                        # T3: frozen constants, frame, border ring, cap points
scripts/blender/lib/scene.py                         # T3: reset_scene, empty, apply_transforms
scripts/blender/lib/geom.py                          # T3: cdt_cap, skirt, tri_count, weld, set_border_normals
scripts/blender/lib/materials.py                     # T3: pbr_material, torus-mapped tileable nodes
scripts/blender/lib/uv.py                            # T3: atlas_unwrap, planar_cap_uv, measure_texel_density
scripts/blender/lib/bake.py                          # T3: Cycles bakes, ORM pack, masks, receiver signature, report
scripts/blender/lib/lod.py                           # T3: decimate_to, assert_border_identical
scripts/blender/lib/collider.py                      # T3: collider_from
scripts/blender/lib/export.py                        # T3: add_socket, export_glb (no images), export report
scripts/blender/lib/gate.py                          # T5: six-view render, Blender reimport check
scripts/blender/lib/ibl.py                           # T7: equirect → GGX-prefiltered cubemap + irradiance
scripts/blender/lib/ktx2.py                          # T7: minimal KTX2 writer (RGBA16F cubemaps)
scripts/blender/tests/test_hexgeo.py                 # T3
scripts/blender/tests/test_geom_lod.py               # T3
scripts/blender/tests/test_bake_export.py            # T3
scripts/blender/tests/test_ibl.py                    # T7
scripts/blender/tests/fixture_piece.py               # T3: tiny fixture piece builder
scripts/blender/kit/meadow_base.py                   # T8
scripts/blender/kit/mat_grass.py, mat_soil.py        # T8: tileable sets
scripts/blender/kit/road_straight_center.py          # T9
scripts/blender/kit/road_straight_edge.py            # T9
scripts/blender/kit/mat_asphalt.py                   # T9
scripts/blender/kit/tree_hero_oak.py                 # T10: export stage from the MCP .blend
scripts/blender/lookdev_ibl.py                       # T7: drives ibl.py
scripts/assets/kit-compress.mjs                      # T4
scripts/assets/kit-provenance.mjs                    # T5
scripts/assets/kit-lib.mjs                           # T4: channel table, KTX2 header read, sha
scripts/refs/higgsfield-refs.sh                      # T2
assets-source/refs/<piece>/                          # T2: reference PNGs + refs.json (committed)
assets-source/kit/<piece>/                           # T5+: provenance, reports, renders (committed)
assets-source/lookdev/lookdev.json                   # T7
assets/kit/manifest.json                             # T5: piece + material registry
assets/kit/p1/<piece>/…, assets/kit/p1/materials/<set>/…, assets/kit/p1/ibl/…
crates/hex-core/src/axial.rs                         # T3: LEVEL_STEP
crates/hex-assets/Cargo.toml                         # T5: serde derive
crates/hex-assets/src/lib.rs                         # T5: exports
crates/hex-assets/src/kit_manifest.rs                # T5: manifest types, Channel color space, budgets
crates/hex-assets/src/ktx2_header.rs                 # T5: KTX2 header parse
crates/hex-assets/src/bevy.rs                        # T5: kit_source(root)
crates/hex-render/Cargo.toml                         # T5: dev-deps gltf, sha2, serde_json
crates/hex-render/tests/kit_gate.rs                  # T5 (+ T8–T10 pieces appear automatically)
crates/hex-render/src/kit_material.rs                # T6: KitExt + material builder
crates/hex-render/src/kit_scene.rs                   # T6: kit scenario loader/spawner
crates/hex-render/src/lookdev.rs                     # T7: Tonemap enum, IBL component
crates/hex-render/src/lib.rs, scene.rs, workload.rs  # T6, T7: Scenario::Kit, config keys
assets/shaders/kit_material.wesl                     # T6
crates/hex-app/src/args.rs                           # T6, T7: kit_hooks, tonemap keys (+ web_pairs)
scripts/shots/kit.json                               # T6, T9: kit gallery + seam close-up
docs/agent-operability.md, PRD.md                    # T11
docs/decisions/0002-asset-pipeline.md                # T11
```

---
### Task 1: The `blender-asset` project skill and tooling bootstrap

Task 1 codifies the pipeline before any Blender code exists, so later implementers follow one contract (R4). It also pins tools and adds the headless test runner.

**Files:**
- Create: `.claude/skills/blender-asset/SKILL.md`, `.claude/skills/blender-asset/references/{stages,lib-contracts,gates,provenance,pitfalls}.md`
- Create: `scripts/blender/run_tests.py`, `scripts/blender/tests/__init__.py`, `scripts/blender/tests/test_runner_selfcheck.py`, `scripts/blender/lib/__init__.py`
- Modify: `.gitignore`, `Makefile`, `scripts/assets/package.json` (+ regenerate `scripts/assets/package-lock.json`)

**Interfaces:**
- Consumes: `scripts/fetch-ktx.sh` (writes `tools/ktx-env.sh`).
- Produces:
  - `make blender-test [TEST=<module>]`: runs `scripts/blender/tests/test_*.py` inside Blender and exits non-zero on any failure.
  - `BLENDER` and `BLENDER_RUN` make variables.
  - The skill at `.claude/skills/blender-asset/SKILL.md`, which every later Blender task loads.
  - The single gltf-transform pin: `@gltf-transform/{core,extensions,functions,cli}@4.3.0`.

- [ ] **Step 1: Bootstrap the tools and record versions**

```bash
make fetch-tools
"/Applications/Blender.app/Contents/MacOS/Blender" -b --factory-startup --version > target/blender-version.txt 2>&1; echo "EXIT=$?"; cat target/blender-version.txt
```
Expected: `ktx version: v4.4.2…`, `EXIT=0` and `Blender 5.2.2…`. If Blender is not 5.2.x, stop and report: the API notes in the skill are written against 5.2.

- [ ] **Step 2: Pin gltf-transform and the validator**

Replace `scripts/assets/package.json` with:

```json
{ "private": true, "type": "module",
  "dependencies": { "@gltf-transform/core": "4.3.0", "@gltf-transform/extensions": "4.3.0",
                    "@gltf-transform/functions": "4.3.0", "@gltf-transform/cli": "4.3.0",
                    "gltf-validator": "2.0.0-dev.3.10",
                    "meshoptimizer": "0.25.0", "sharp": "0.34.4" } }
```

Run: `cd scripts/assets && npm install --no-audit --no-fund && npx --no-install gltf-transform --version`
Expected: `4.3.0`. Scripts import the libraries. The CLI is kept for ad-hoc `gltf-transform inspect`.

- [ ] **Step 3: Gitignore the local-only sources (R1)**

Append to `.gitignore`:

```
# R1: .blend sources and bake intermediates stay local, referenced by SHA-256 in provenance
/assets-source/blend/
/assets-source/hdri/
```

- [ ] **Step 4: Write the failing runner self-check**

Create `scripts/blender/tests/__init__.py` and `scripts/blender/lib/__init__.py`, both empty. Create `scripts/blender/tests/test_runner_selfcheck.py`:

```python
import os
import unittest


class RunnerSelfCheck(unittest.TestCase):
    def test_runs_inside_blender(self):
        import bpy  # fails outside Blender
        self.assertTrue(bpy.app.version >= (5, 2, 0), bpy.app.version_string)

    def test_forced_failure_when_requested(self):
        # `make blender-test-selfcheck` sets this to prove a failing test fails the make target.
        self.assertNotEqual(os.environ.get("HEXW_FORCE_FAIL"), "1", "forced failure")
```

Run: `make blender-test`
Expected: FAIL with `No rule to make target 'blender-test'`.

- [ ] **Step 5: Write the runner and the make targets**

Create `scripts/blender/run_tests.py`:

```python
"""Headless Blender unittest runner.

Usage: blender -b --factory-startup --python-exit-code 1 -P scripts/blender/run_tests.py -- [module]
Raises on failure so --python-exit-code turns it into a non-zero exit (Blender exits 0 otherwise).
"""
import pathlib
import sys
import unittest

ROOT = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT))  # `lib` and `tests` import as packages
argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
loader = unittest.TestLoader()
if argv and argv[0]:
    suite = loader.loadTestsFromName(f"tests.{argv[0]}")
else:
    suite = loader.discover(str(ROOT / "tests"), pattern="test_*.py", top_level_dir=str(ROOT))
result = unittest.TextTestRunner(verbosity=2).run(suite)
if not result.wasSuccessful():
    raise RuntimeError(f"blender tests failed: {len(result.failures)} failures, {len(result.errors)} errors")
```

In `Makefile`, add `blender-test blender-test-selfcheck` to `.PHONY`. Add these targets after `fetch-tools`:

```make
BLENDER ?= /Applications/Blender.app/Contents/MacOS/Blender
BLENDER_RUN = "$(BLENDER)" -b --factory-startup --python-exit-code 1

blender-test:
	$(BLENDER_RUN) -P scripts/blender/run_tests.py -- $(TEST)

# Proves a failing Blender test fails make (brief §(e): Blender exits 0 on tracebacks without the flag).
blender-test-selfcheck:
	@if HEXW_FORCE_FAIL=1 $(BLENDER_RUN) -P scripts/blender/run_tests.py -- test_runner_selfcheck > target/selfcheck.log 2>&1; \
	then echo "FAIL: forced failure exited 0"; exit 1; else echo "selfcheck OK: failure propagates"; fi
```

- [ ] **Step 6: Run both targets**

Run: `make blender-test TEST=test_runner_selfcheck && make blender-test-selfcheck`
Expected: `OK` (2 tests), then `selfcheck OK: failure propagates`.

- [ ] **Step 7: Write the skill**

Create `.claude/skills/blender-asset/SKILL.md`:

```markdown
---
name: blender-asset
description: Hex World kit-piece authoring pipeline in Blender. Use for any Hex World 3D asset work, such as writing or running a scripts/blender/kit/<piece>.py generator, sculpting a hero piece through the Blender MCP, baking maps, building LODs, colliders and sockets, exporting a GLB, running make kit-build / kit-compress / kit-check, writing provenance, or debugging a failed asset gate. Triggers include "kit piece", "blender generator", "bake", "LOD", "socket", "hex base", "kit-build", "kit-check" and "provenance".
---

# Hex World Blender asset pipeline

You author ONE kit piece. Read `references/stages.md` before touching Blender, and read `references/pitfalls.md` before your first bake.

## Rules
- **Concurrency.** At most 3 Blender asset subagents run at once (PRD §11.1). Own exactly one piece id. Use a scene and collection named after it, and the staging dir `target/kit-stage/<piece>/`.
- **Invocation.** Always `make kit-build PIECE=<id>`, or `"$BLENDER" -b --factory-startup --python-exit-code 1 -P …`. Blender exits 0 on tracebacks without the flag.
- **Helpers.** Import helpers from `scripts/blender/lib/` (contracts in `references/lib-contracts.md`). Do not re-implement the frame math, the border ring, bakes or export.
- **Frame and pivot.**
  - Constants come from `lib.hexgeo`: R = 16 m, step = 4 m, apothem 13.856406 m.
  - Pivot at the hex center. Edge pieces are authored for edge 0, which faces Blender +X.
  - Blender (x, y, z) exports as runtime (x, z, -y).
- **Seams.**
  - Pieces that touch the hex border build every LOD from `hexgeo.border_ring(BORDER_SUBDIV)`.
  - Never decimate a border-touching piece. Regenerate it at a lower interior density instead.
  - Road cross-sections at the edge are mirror-symmetric about the lane center.
- **Textures.**
  - The GLB embeds no images. Maps are PNG files named `<piece>.<channel>.png`, with channel in {albedo, normal, orm, emissive, masks}.
  - albedo and emissive are sRGB. normal, orm and masks are linear (Non-Color).
  - masks is R snow, G wetness, B night emissive, A baked AO (1.0 when the piece has a unique ORM).
- **Budgets.** Kit LOD0 ≤ 4000 tris and tree ≤ 3000. LOD1 ≤ 50% and LOD2 ≤ 15% of LOD0. GLB ≤ 2 MiB. KTX2 ≤ 1.5 MB per piece per family. Maps ≤ 2048².
- **Provenance.**
  - Every piece has `assets-source/kit/<piece>/provenance.json` (`references/provenance.md`), with `authoringAgent.model = "claude-opus-5-5"`.
  - `.blend` files and bake intermediates live only in `assets-source/blend/<piece>/` (gitignored). Record each one's SHA-256.
- **Done.** You are done when `make kit-check` passes and the 6-view render plus the in-engine shot exist. Owner approval is recorded by the controller. Never record it yourself.
- **Friction.** Log every snag in `assets-source/kit/<piece>/friction.md` (what happened, the fix, the minutes lost). The last P1 task folds these into this skill.

## Stage order
approved reference → high-poly → LOD0 + UVs → bake → LODs + collider → sockets + export → compress → gate → provenance → owner review. Commands for each stage are in `references/stages.md`.
```

Create the five references:
- **`references/stages.md`.** One numbered entry per stage, each giving the exact command:
  1. `make refs PIECE=<id>` (Task 2).
  2. `make kit-build PIECE=<id>`, which writes `target/kit-stage/<id>/{<id>.glb,<id>.<channel>.png,bake-report.json,export-report.json}`.
  3. `make kit-compress PIECE=<id>` (Task 4).
  4. `make kit-renders PIECE=<id>` (Task 5).
  5. `make kit-provenance PIECE=<id>` (Task 5).
  6. `make kit-check` (Task 5).
  7. `make shots-kit` (Task 6).

  Add an "Interactive route" section that Task 10 fills.
- **`references/lib-contracts.md`.** A heading per module in `scripts/blender/lib/` with the line "Filled by Task 3." Task 3 Step 13 replaces it.
- **`references/gates.md`.** The brief's §(c) table, edited to this plan's values:
  - tree LOD0 3000;
  - KTX2 ≤ 1.5 MB per family;
  - sockets 0.001 m;
  - border ring 1e-4 m;
  - border normals within 1e-3 of +Y.

  Add a "Pinned by" column, which Task 5 fills with the test names.
- **`references/provenance.md`.** The brief's §(d) field list verbatim, plus: "Missing provenance, `validation.pass != true`, or a `derivedFiles` hash that does not match the file on disk fails `make kit-check`."
- **`references/pitfalls.md`.** The brief's §(e) bullets verbatim, plus:
  - "Never decimate a border-ring piece; regenerate."
  - "PNG carries no color space, so the runtime decides from the channel name."
  - "Tileable layers are sampled in world space in `kit_material.wesl`, so neighbors line up under rotation."

- [ ] **Step 8: Run the gate**

Run: `make checkall`
Expected: PASS. No Rust changed, so this confirms nothing regressed.

- [ ] **Step 9: Commit**

```bash
git add .claude/skills/blender-asset .gitignore Makefile scripts/assets/package.json scripts/assets/package-lock.json scripts/blender
git commit -m "feat(assets): blender-asset skill, pinned gltf-transform, headless Blender test runner"
```

---

### Task 2: Higgsfield references and the owner checkpoint (R2)

**Files:**
- Create: `scripts/refs/higgsfield-refs.sh`, `assets-source/refs/{meadow_base,road_straight,tree_hero_oak}/{ref-1.png,ref-2.png,refs.json,approval.json}`
- Modify: `Makefile` (target `refs`)

**Interfaces:**
- Produces: `assets-source/refs/<id>/refs.json`, shaped as `[{"path", "sha256", "provider": "higgsfield", "model": "nano_banana_pro", "jobId", "prompt"}]`. Provenance `references` (Task 5) copies these entries. `approval.json` holds `{"who", "when", "gate": "reference", "approved": [..], "notes"}`.

- [ ] **Step 1: Check auth and cost**

Run: `higgsfield generate cost nano_banana_pro --prompt "test" --resolution 2k`
Expected: a cost line. On an auth error, stop and ask the owner to run `higgsfield auth login`. It is the owner's account, so never log in yourself.

- [ ] **Step 2: Write the script**

Create `scripts/refs/higgsfield-refs.sh`:

```bash
#!/usr/bin/env bash
# Generates owner-review reference images for one P1 piece with Higgsfield nano_banana_pro (R2).
# Usage: scripts/refs/higgsfield-refs.sh <meadow_base|road_straight|tree_hero_oak>
set -euo pipefail
id="$1"; out="assets-source/refs/$id"; mkdir -p "$out"
style="stylized-real diorama miniature, real PBR materials, slightly exaggerated proportions, soft noon sun, neutral grey studio background, 35 degree three-quarter view"
case "$id" in
  meadow_base) prompts=(
    "A single hexagonal terrain tile, 32 m across, lush meadow grass cap with gentle undulation, sparse wildflowers, visible layered soil strata on the hexagon side walls (topsoil, clay, gravel), $style"
    "Close-up of the meadow grass surface and the soil strata cut face of a hexagonal diorama tile, $style") ;;
  road_straight) prompts=(
    "A single hexagonal diorama tile with a straight two-lane asphalt country road crossing from one flat edge to the opposite flat edge, 6 m wide road, grass shoulders, faded centre line, low curbs, $style"
    "Two identical hexagonal road tiles placed edge to edge with the road continuing seamlessly across the joint, close-up at the seam, $style") ;;
  tree_hero_oak) prompts=(
    "A single hero oak tree for a stylized-real diorama, 12 m tall, broad rounded canopy of leaf clusters, gnarled trunk with root flare, $style"
    "Bark and leaf-cluster material close-up for a stylized-real diorama oak tree, $style") ;;
  *) echo "unknown piece $id" >&2; exit 2 ;;
esac
entries=()
for i in "${!prompts[@]}"; do
  p="${prompts[$i]}"; f="$out/ref-$((i+1)).png"
  higgsfield --json generate create nano_banana_pro --prompt "$p" --aspect_ratio 4:3 --resolution 2k --wait --wait-timeout 10m > "$out/job-$((i+1)).json"
  node scripts/refs/fetch-result.mjs "$out/job-$((i+1)).json" "$f"
  sha=$(shasum -a 256 "$f" | cut -d' ' -f1)
  job=$(node -e 'const j=JSON.parse(require("fs").readFileSync(process.argv[1]));console.log(j.id??j.job_id??(Array.isArray(j)?j[0].id:""))' "$out/job-$((i+1)).json")
  entries+=("$(node -e 'console.log(JSON.stringify({path:process.argv[1],sha256:process.argv[2],provider:"higgsfield",model:"nano_banana_pro",jobId:process.argv[3],prompt:process.argv[4]}))' "$f" "$sha" "$job" "$p")")
done
(IFS=,; printf '[%s]\n' "${entries[*]}") > "$out/refs.json"
echo "wrote $out"
```

Create `scripts/refs/fetch-result.mjs`. It downloads the first image URL found in the job JSON and re-encodes it to PNG:

```js
// Usage: node scripts/refs/fetch-result.mjs <job.json> <out.png>
import { readFile } from "node:fs/promises";
import sharp from "sharp";
const [jobPath, out] = process.argv.slice(2);
const job = JSON.parse(await readFile(jobPath, "utf8"));
const find = (o) => typeof o === "string" ? (/^https?:\/\/\S+\.(png|jpe?g|webp)(\?|$)/i.test(o) ? o : undefined)
  : o && typeof o === "object" ? Object.values(o).map(find).find(Boolean) : undefined;
const url = find(job);
if (!url) { console.error("no image URL in", jobPath); process.exit(1); }
const res = await fetch(url, { signal: AbortSignal.timeout(120_000) });
if (!res.ok) { console.error("download failed", res.status, url); process.exit(1); }
await sharp(Buffer.from(await res.arrayBuffer())).png().toFile(out);
console.log("wrote", out);
```

The output must land in `assets-source/refs/`, and the script imports `sharp` from `scripts/assets/node_modules`. Add the target to `Makefile` and `.PHONY`:

```make
refs:
	NODE_PATH=scripts/assets/node_modules bash scripts/refs/higgsfield-refs.sh $(PIECE)
```

If Node does not resolve a bare `sharp` import through `NODE_PATH` (ESM ignores `NODE_PATH`), import it as `await import(new URL("../assets/node_modules/sharp/lib/index.js", import.meta.url))` instead. Record that in `pitfalls.md`.

- [ ] **Step 3: Generate the three sets**

Run: `for p in meadow_base road_straight tree_hero_oak; do make refs PIECE=$p || exit 1; done; ls assets-source/refs/*/`
Expected: `ref-1.png`, `ref-2.png` and `refs.json` per piece. View each PNG with Read and confirm the subject: a hexagonal tile, not a square, and one tree. Regenerate failures and say so in the checkpoint message.

- [ ] **Step 4: OWNER CHECKPOINT (R2). STOP.**

Send the owner the six absolute PNG paths with their prompts, and ask for approval or revision per piece. Write the answer to `assets-source/refs/<id>/approval.json`.

**Do not start Task 8, 9 or 10 until all three pieces are approved in chat.** Tasks 3–7 need no references and may proceed.

- [ ] **Step 5: Commit**

```bash
git add scripts/refs Makefile assets-source/refs
git commit -m "feat(assets): Higgsfield reference images for the P1 family (owner-approved)"
```

---

### Task 3: `scripts/blender/lib/`: hex geometry, bakes, LODs, collider and export

Load `.claude/skills/blender-asset/SKILL.md` first (R4).

Border-touching pieces get LODs by regeneration, not decimation: the cap is a constrained Delaunay triangulation of a fixed border ring plus a coarser interior lattice for each LOD. That keeps the border identical in every LOD (brief §(e) "LOD seams") without the unverified Decimate vertex-group protection. `decimate_to` is used only for pieces that do not touch the border, such as the tree.

**Files:**
- Create: `scripts/blender/lib/{hexgeo,scene,geom,materials,uv,bake,lod,collider,export}.py`
- Create: `scripts/blender/tests/{test_hexgeo,test_geom_lod,test_bake_export}.py`, `scripts/blender/tests/fixture_piece.py`
- Modify: `crates/hex-core/src/axial.rs` (add `LEVEL_STEP`), `.claude/skills/blender-asset/references/lib-contracts.md`

**Interfaces:**
- Consumes: `make blender-test` (Task 1).
- Produces. All positions are in the Blender frame (meters, Z-up) unless named `_rt`.
  - `hexgeo`:
    - Constants `HEX_R=16.0`, `LEVEL_STEP=4.0`, `APOTHEM`, `BORDER_SUBDIV=16`.
    - `blender_to_rt(x,y,z)->tuple` and `rt_to_blender(x,y,z)->tuple`.
    - `corner(i)->(x,y)`, `edge_midpoint(k)->(x,y)`, `edge_normal(k)->(x,y)`.
    - `border_ring(subdiv=BORDER_SUBDIV)->list[(x,y)]`, with length `6*subdiv` and starting at edge 0's first corner.
    - `dist_to_border(x,y)->float`, which is positive inside.
    - `lattice_points(spacing, margin)->list[(x,y)]`.
    - `rotate_xy(p, k)->(x,y)`, which rotates by edge steps.
    - `lane_frame(edge=0, offset=0.0, width=6.0)->dict(center, tangent, normal, width)`.
  - `scene`: `reset_scene(name)`, `link(obj, collection=None)`, `empty(name, loc=(0,0,0), parent=None)`, `apply_transforms(obj)`, `select_only(objs, active=None)`.
  - `geom`:
    - `cdt_cap(name, border_pts, interior_pts, height_fn)->Object`, where the border is at z = 0.
    - `skirt(name, border_pts, depth, rows)->Object`.
    - `tri_count(obj)->int`.
    - `weld(obj, tol=0.001)`.
    - `set_border_normals_up(obj, tol=1e-4)`.
    - `join(objs, name)->Object`.
  - `materials`:
    - `srgb(hex_str)->(r,g,b,1)`, which is linear.
    - `pbr_material(name, base_color, roughness, metallic=0.0, emission=None)->Material`.
    - `torus_coords(nt, scale)->(vector_socket, w_socket)`.
  - `uv`:
    - `planar_cap_uv(obj, layer="UVMap")`.
    - `atlas_unwrap(objs, margin=0.008)`.
    - `measure_texel_density(obj, atlas_px, layer="UVMap")->dict(min, median, max)`.
  - `bake`:
    - `setup_cycles(device="CPU", samples=16)`.
    - `new_image(name, size, non_color)->Image`.
    - `bake_to(low, bake_type, image, highs=(), cage_extrusion=0.0, max_ray=0.0, margin=12, clear=True)`.
    - `bake_emit_input(highs, low, input_name, image, **kw)`.
    - `save_png(image, path)->str`.
    - `pack_orm(ao_png, rough_png, metal_png, out_png)`.
    - `make_masks(size, snow, wet, night, ao_png, out_png)`.
    - `receiver_signature(objs)->str`.
    - `check_bake_cache(report_path, signature)`, which raises `StaleBakeError`.
    - `bake_tileable_set(material, size, out_dir, set_id, metallic=0.0)->dict`.
    - `write_bake_report(path, **fields)`.
    - `sha256_file(path)->str`.
  - `lod`: `triangulate(obj)`, `decimate_to(obj, target_tris)->int`, `border_verts(obj, tol=1e-4)->set`, `assert_border_identical(objs, tol=1e-4)`.
  - `collider`: `collider_from(src, kind, name)->Object`, with kind in {"cap", "hull", "box"}.
  - `export`:
    - `add_socket(name, loc, parent, yaw_deg=0.0, **extras)->Object`.
    - `export_glb(root, path)`.
    - `write_export_report(path, root, piece_id, generator_path)->dict`.
  - Rust: `pub const LEVEL_STEP: f32 = 4.0;` in `hex_core::axial`.

- [ ] **Step 1: Write the failing hexgeo test**

Create `scripts/blender/tests/test_hexgeo.py`:

```python
import math
import pathlib
import re
import unittest

from lib import hexgeo

REPO = pathlib.Path(__file__).resolve().parents[3]


def rust_const(name):
    src = (REPO / "crates/hex-core/src/axial.rs").read_text()
    m = re.search(rf"pub const {name}: f32 = ([0-9.]+);", src)
    assert m, f"{name} not found in axial.rs"
    return float(m.group(1))


class HexGeo(unittest.TestCase):
    def test_constants_match_rust(self):  # D8
        self.assertEqual(hexgeo.HEX_R, rust_const("HEX_RADIUS"))
        self.assertEqual(hexgeo.LEVEL_STEP, rust_const("LEVEL_STEP"))
        self.assertAlmostEqual(hexgeo.APOTHEM, 13.856406, places=5)

    def test_edge0_faces_plus_x_and_edge1_is_south(self):
        x, y = hexgeo.edge_midpoint(0)
        self.assertAlmostEqual(x, hexgeo.APOTHEM, places=5)
        self.assertAlmostEqual(y, 0.0, places=6)
        _, _, z_rt = hexgeo.blender_to_rt(*hexgeo.edge_midpoint(1), 0.0)
        self.assertGreater(z_rt, 0.0)  # axial.rs: edge 1 is +z (south)

    def test_frame_round_trip(self):
        p = (1.0, 2.0, 3.0)
        self.assertEqual(hexgeo.rt_to_blender(*hexgeo.blender_to_rt(*p)), p)

    def test_border_ring_on_border_and_rotation_invariant(self):
        ring = hexgeo.border_ring(8)
        self.assertEqual(len(ring), 48)
        for p in ring:
            self.assertAlmostEqual(hexgeo.dist_to_border(*p), 0.0, places=5)
        key = lambda p: (round(p[0], 4), round(p[1], 4))
        rotated = {key(hexgeo.rotate_xy(p, 1)) for p in ring}
        self.assertEqual(rotated, {key(p) for p in ring})

    def test_lattice_respects_margin(self):
        pts = hexgeo.lattice_points(2.0, 1.0)
        self.assertGreater(len(pts), 100)
        self.assertTrue(all(hexgeo.dist_to_border(*p) >= 1.0 - 1e-9 for p in pts))

    def test_lane_frame_edge0(self):
        f = hexgeo.lane_frame(0, 0.0, 6.0)
        self.assertAlmostEqual(f["center"][0], hexgeo.APOTHEM, places=5)
        self.assertEqual(f["width"], 6.0)
        self.assertAlmostEqual(math.hypot(*f["tangent"]), 1.0, places=6)
```

Run: `make blender-test TEST=test_hexgeo`
Expected: FAIL with `ImportError: cannot import name 'hexgeo'`.

- [ ] **Step 2: Add `LEVEL_STEP` to Rust and write `hexgeo.py`**

In `crates/hex-core/src/axial.rs`, after `HEX_RADIUS`:

```rust
/// Height of one elevation level in meters (PRD §5.1, frozen in P1 with HEX_RADIUS; D8).
pub const LEVEL_STEP: f32 = 4.0;
```

Create `scripts/blender/lib/hexgeo.py`:

```python
"""Hex frame shared by every kit generator. Constants are frozen (D8) and tested against axial.rs.

Blender is Z-up; glTF/runtime is Y-up: runtime (x, y, z) = Blender (x, z, -y).
Runtime: pointy-top, edge 0 faces +x, edges clockwise viewed from +Y, +z is south.
In Blender's top view edge k's outward normal is at -60°·k.
"""
import math

HEX_R = 16.0
LEVEL_STEP = 4.0
APOTHEM = HEX_R * math.sqrt(3.0) / 2.0
BORDER_SUBDIV = 16  # segments per hex edge on the border ring, identical in every LOD


def blender_to_rt(x, y, z):
    return (x, z, -y)


def rt_to_blender(x, y, z):
    return (x, -z, y)


def corner(i, r=HEX_R):
    """Runtime corner i sits at 30°+60°·i in the XZ plane (crates/hex-render/src/scene.rs corner())."""
    a = math.radians(30.0 + 60.0 * i)
    return (r * math.cos(a), -r * math.sin(a))


def edge_normal(k):
    a = math.radians(60.0 * k)
    return (math.cos(a), -math.sin(a))


def edge_midpoint(k, r=HEX_R):
    d = r * math.sqrt(3.0) / 2.0
    n = edge_normal(k)
    return (d * n[0], d * n[1])


def border_ring(subdiv=BORDER_SUBDIV, r=HEX_R):
    """Points around the border, edge 0 first; edge k runs from corner k-1 to corner k."""
    pts = []
    for k in range(6):
        a, b = corner((k - 1) % 6, r), corner(k % 6, r)
        for s in range(subdiv):
            t = s / subdiv
            pts.append((a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t))
    return pts


def dist_to_border(x, y):
    return APOTHEM - max(x * n[0] + y * n[1] for n in (edge_normal(k) for k in range(6)))


def lattice_points(spacing, margin):
    """Triangular lattice inside the hex, at least `margin` from the border. Deterministic order."""
    out = []
    row_h = spacing * math.sqrt(3.0) / 2.0
    n = int(HEX_R / row_h) + 2
    for j in range(-n, n + 1):
        y = j * row_h
        off = (spacing / 2.0) if j % 2 else 0.0
        m = int(HEX_R / spacing) + 2
        for i in range(-m, m + 1):
            x = i * spacing + off
            if dist_to_border(x, y) >= margin:
                out.append((x, y))
    return out


def rotate_xy(p, k):
    """Rotate by k edge steps (edge e -> edge e+k), i.e. -60°·k in Blender's top view."""
    a = math.radians(-60.0 * k)
    c, s = math.cos(a), math.sin(a)
    return (p[0] * c - p[1] * s, p[0] * s + p[1] * c)


def lane_frame(edge=0, offset=0.0, width=6.0):
    n = edge_normal(edge)
    t = (-n[1], n[0])
    m = edge_midpoint(edge)
    return {"center": (m[0] + t[0] * offset, m[1] + t[1] * offset), "tangent": t, "normal": n, "width": width}
```

Run: `make blender-test TEST=test_hexgeo && cargo test -p hex-core`
Expected: PASS (6 Blender tests), and the hex-core tests pass.

- [ ] **Step 3: Write the failing geometry and LOD test**

Create `scripts/blender/tests/test_geom_lod.py`:

```python
import math
import unittest

import bpy

from lib import geom, hexgeo, lod, scene


def bump(x, y):
    d = hexgeo.dist_to_border(x, y)
    fall = min(d / 2.0, 1.0)
    fall = fall * fall * (3 - 2 * fall)  # smoothstep: zero value and zero slope at the border
    return 0.3 * fall * math.sin(x * 0.5) * math.cos(y * 0.4)


class GeomLod(unittest.TestCase):
    def setUp(self):
        scene.reset_scene("test")

    def caps(self):
        ring = hexgeo.border_ring(8)
        return [geom.cdt_cap(f"cap{i}", ring, hexgeo.lattice_points(s, 1.0), bump)
                for i, s in enumerate((1.5, 3.0, 50.0))]

    def test_cap_lods_share_border(self):
        caps = self.caps()
        lod.assert_border_identical(caps)
        tris = [geom.tri_count(c) for c in caps]
        self.assertTrue(tris[0] > tris[1] > tris[2], tris)
        self.assertEqual(len(lod.border_verts(caps[2])), 48)

    def test_border_normals_up_and_faces_up(self):
        cap = self.caps()[0]
        geom.set_border_normals_up(cap)
        mesh = cap.data
        for poly in mesh.polygons:
            self.assertGreater(poly.normal.z, 0.0)
        border = {i for i, v in enumerate(mesh.vertices) if abs(hexgeo.dist_to_border(v.co.x, v.co.y)) < 1e-4}
        for li, loop in enumerate(mesh.loops):
            if loop.vertex_index in border:
                self.assertGreater(mesh.corner_normals[li].vector.z, 0.999)

    def test_skirt_outward_and_counted(self):
        ring = hexgeo.border_ring(4)
        s = geom.skirt("skirt", ring, hexgeo.LEVEL_STEP, 2)
        self.assertEqual(geom.tri_count(s), 24 * 2 * 2)
        for poly in s.data.polygons:
            c = poly.center
            self.assertGreater(poly.normal.x * c.x + poly.normal.y * c.y, 0.0)

    def test_decimate_hits_target(self):
        bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=5, radius=1.0)
        obj = bpy.context.active_object
        got = lod.decimate_to(obj, 1000)
        self.assertLessEqual(got, 1000)
        self.assertGreater(got, 900)
```

Run: `make blender-test TEST=test_geom_lod`
Expected: FAIL with `ImportError` for `geom`.

- [ ] **Step 4: Write `scene.py`, `geom.py` and `lod.py`**

Create `scripts/blender/lib/scene.py`:

```python
import bpy


def reset_scene(name):
    """Empty factory scene in metric units; no .blend1 backups (the-cube lib.py:19-26)."""
    bpy.ops.wm.read_factory_settings(use_empty=True)
    s = bpy.context.scene
    s.name = name
    s.unit_settings.system = "METRIC"
    s.unit_settings.scale_length = 1.0
    bpy.context.preferences.filepaths.save_version = 0
    return s


def link(obj, collection=None):
    (collection or bpy.context.scene.collection).objects.link(obj)
    return obj


def empty(name, loc=(0.0, 0.0, 0.0), parent=None):
    e = bpy.data.objects.new(name, None)
    e.location = loc
    e.empty_display_size = 0.5
    link(e)
    if parent is not None:
        e.parent = parent
    return e


def select_only(objs, active=None):
    for o in bpy.context.view_layer.objects:
        o.select_set(False)
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = active or (objs[0] if objs else None)


def apply_transforms(obj):
    """Apply before joining (vault Patterns/blender-transform-identity-in-automated-glb-builds.md)."""
    select_only([obj])
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
```

Create `scripts/blender/lib/geom.py`:

```python
import bmesh
import bpy
from mathutils import Vector
from mathutils.geometry import delaunay_2d_cdt

from . import hexgeo, scene


def _object(name, verts, faces):
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.validate(clean_customdata=False)
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    return scene.link(obj)


def cdt_cap(name, border_pts, interior_pts, height_fn):
    """Terrain cap: constrained Delaunay of the border loop + interior points. Border z = 0 exactly,
    interior z = height_fn(x, y). Every LOD built with the same border_pts shares the border ring."""
    nb = len(border_pts)
    coords = [Vector((x, y)) for x, y in border_pts] + [Vector((x, y)) for x, y in interior_pts]
    out_v, _e, out_f, orig_v, _oe, _of = delaunay_2d_cdt(coords, [], [list(range(nb))], 1, 1e-6)
    border_ids = {i for i, o in enumerate(orig_v) if any(j < nb for j in o)}
    verts = [(v.x, v.y, 0.0 if i in border_ids else height_fn(v.x, v.y)) for i, v in enumerate(out_v)]
    faces = []
    for f in out_f:
        a, b, c = (Vector(verts[i]) for i in f[:3])
        faces.append(list(f) if (b - a).cross(c - a).z > 0 else list(reversed(f)))
    obj = _object(name, verts, faces)
    for p in obj.data.polygons:
        p.use_smooth = True
    return obj


def skirt(name, border_pts, depth, rows):
    """Side walls from the border ring (z = 0) down to z = -depth, `rows` quads high, triangulated."""
    n = len(border_pts)
    verts = [(x, y, -depth * r / rows) for r in range(rows + 1) for (x, y) in border_pts]
    faces = []
    for r in range(rows):
        for i in range(n):
            a, b = r * n + i, r * n + (i + 1) % n
            c, d = b + n, a + n
            # Winding is normalized by the outward flip pass below (asserted in test_skirt_outward).
            faces += [[a, d, c], [a, c, b]]
    obj = _object(name, verts, faces)
    mid = Vector((0.0, 0.0, 0.0))
    for p in obj.data.polygons:
        if p.normal.dot(p.center - mid) < 0:
            p.flip()
    obj.data.update()
    return obj


def tri_count(obj):
    return sum(len(p.vertices) - 2 for p in obj.data.polygons)


def weld(obj, tol=0.001):
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=tol)
    bm.to_mesh(obj.data)
    bm.free()


def set_border_normals_up(obj, tol=1e-4):
    """Custom normals: +Z on the border ring so neighbouring caps shade identically (PRD §6)."""
    mesh = obj.data
    normals = []
    for v in mesh.vertices:
        on_border = abs(hexgeo.dist_to_border(v.co.x, v.co.y)) < tol and abs(v.co.z) < tol
        normals.append((0.0, 0.0, 1.0) if on_border else tuple(v.normal))
    mesh.normals_split_custom_set_from_vertices(normals)
    mesh.update()


def join(objs, name):
    for o in objs:
        scene.apply_transforms(o)
    scene.select_only(objs, objs[0])
    bpy.ops.object.join()
    out = bpy.context.view_layer.objects.active
    out.name = name
    out.data.name = name
    return out
```

If `skirt`'s winding test fails, keep the `flip()` pass and fix the face order rather than the test. The test is the contract.

Create `scripts/blender/lib/lod.py`:

```python
import bpy

from . import geom, hexgeo, scene


def triangulate(obj):
    scene.select_only([obj])
    m = obj.modifiers.new("Triangulate", "TRIANGULATE")
    bpy.ops.object.modifier_apply(modifier=m.name)


def decimate_to(obj, target_tris):
    """Collapse-decimate to at most target_tris. Only for pieces that do not touch the hex border."""
    triangulate(obj)
    for _ in range(4):
        cur = geom.tri_count(obj)
        if cur <= target_tris:
            break
        scene.select_only([obj])
        m = obj.modifiers.new("LOD", "DECIMATE")
        m.decimate_type = "COLLAPSE"
        m.use_collapse_triangulate = True
        m.ratio = max(0.005, (target_tris * 0.98) / cur)
        bpy.ops.object.modifier_apply(modifier=m.name)
    return geom.tri_count(obj)


def border_verts(obj, tol=1e-4):
    w = obj.matrix_world
    out = set()
    for v in obj.data.vertices:
        p = w @ v.co
        if abs(hexgeo.dist_to_border(p.x, p.y)) < tol and abs(p.z) < tol:
            out.add((round(p.x, 4), round(p.y, 4)))
    return out


def assert_border_identical(objs, tol=1e-4):
    ref = border_verts(objs[0], tol)
    if not ref:
        raise AssertionError(f"{objs[0].name}: no border ring vertices")
    for o in objs[1:]:
        got = border_verts(o, tol)
        if got != ref:
            raise AssertionError(f"{o.name}: border ring differs from {objs[0].name} "
                                 f"(missing {len(ref - got)}, extra {len(got - ref)})")
```

Run: `make blender-test TEST=test_geom_lod`
Expected: PASS (4 tests). If `delaunay_2d_cdt`'s output-type enum differs in 5.2 (`1` = triangles inside the constraint faces), check it with `mcp__Blender__search_api_docs` or the bundled `data/api` RST, and record any rename in `pitfalls.md`.

- [ ] **Step 5: Write the failing bake and export test, with its fixture**

Create `scripts/blender/tests/fixture_piece.py`:

```python
"""Tiny 3-LOD hex piece (cap + 1-row skirt), collider and socket, used by the lib tests and the gate fixture."""
from lib import collider, export, geom, hexgeo, materials, scene, uv


def build(piece_id="fixture"):
    scene.reset_scene(piece_id)
    root = scene.empty(piece_id)
    ring = hexgeo.border_ring(4)
    mat = materials.pbr_material(f"{piece_id}_cap", materials.srgb("#5a8f3a"), 0.8)
    lods = []
    for i, spacing in enumerate((4.0, 8.0, 50.0)):
        cap = geom.cdt_cap(f"{piece_id}_lod{i}_cap", ring, hexgeo.lattice_points(spacing, 1.0), lambda x, y: 0.0)
        geom.set_border_normals_up(cap)
        uv.planar_cap_uv(cap)
        cap.data.materials.append(mat)
        sk = geom.skirt(f"{piece_id}_lod{i}_skirt", ring, hexgeo.LEVEL_STEP, 1)
        sk.data.uv_layers.new(name="UVMap")
        for d in sk.data.uv_layers["UVMap"].data:
            d.uv = (0.005, 0.005)  # parked outside the hex: the cap owns the unique atlas
        sk.data.materials.append(mat)
        body = geom.join([cap, sk], f"{piece_id}_lod{i}")
        body.parent = root
        lods.append(body)
    col = collider.collider_from(lods[2], "cap", f"{piece_id}_collider")
    col.parent = root
    lane = hexgeo.lane_frame(0, 0.0, 6.0)
    export.add_socket("socket-lane-0", (*lane["center"], 0.0), root, yaw_deg=0.0,
                      kind="road", edge=0, offset=0.0, width=6.0)
    return root, lods
```

Create `scripts/blender/tests/test_bake_export.py`:

```python
import json
import pathlib
import struct
import tempfile
import unittest

from lib import bake, export, uv
from tests import fixture_piece


def glb_json(path):
    data = pathlib.Path(path).read_bytes()
    n = struct.unpack_from("<I", data, 12)[0]
    return json.loads(data[20:20 + n])


class BakeExport(unittest.TestCase):
    def setUp(self):
        self.tmp = pathlib.Path(tempfile.mkdtemp())
        self.root, self.lods = fixture_piece.build()

    def test_export_has_names_sockets_tangents_no_images_float_attrs(self):
        out = self.tmp / "fixture.glb"
        export.export_glb(self.root, str(out))
        j = glb_json(out)
        names = {n.get("name") for n in j["nodes"]}
        self.assertTrue({"fixture", "fixture_lod0", "fixture_lod1", "fixture_lod2",
                         "fixture_collider", "socket-lane-0"} <= names, names)
        self.assertNotIn("images", j)
        self.assertNotIn("textures", j)
        self.assertNotIn("KHR_mesh_quantization", j.get("extensionsUsed", []))
        sock = next(n for n in j["nodes"] if n.get("name") == "socket-lane-0")
        self.assertEqual(sock["extras"]["kind"], "road")
        self.assertAlmostEqual(sock["translation"][0], 13.856406, places=3)  # runtime +x
        for mesh in j["meshes"]:
            for prim in mesh["primitives"]:
                self.assertIn("TANGENT", prim["attributes"])
                for a in ("POSITION", "NORMAL", "TANGENT", "TEXCOORD_0"):
                    self.assertEqual(j["accessors"][prim["attributes"][a]]["componentType"], 5126)

    def test_export_is_deterministic(self):
        a, b = self.tmp / "a.glb", self.tmp / "b.glb"
        export.export_glb(self.root, str(a))
        self.root, self.lods = fixture_piece.build()
        export.export_glb(self.root, str(b))
        self.assertEqual(bake.sha256_file(a), bake.sha256_file(b))

    def test_ao_bake_masks_and_report(self):
        bake.setup_cycles(samples=4)
        img = bake.new_image("ao", 64, non_color=True)
        bake.bake_to(self.lods[0], "AO", img, margin=4)
        ao = bake.save_png(img, str(self.tmp / "ao.png"))
        bake.make_masks(64, 1.0, 0.5, 0.0, ao, str(self.tmp / "masks.png"))
        sig = bake.receiver_signature([self.lods[0]])
        rep = self.tmp / "bake-report.json"
        bake.write_bake_report(str(rep), receiverSignature=sig, device="CPU")
        bake.check_bake_cache(str(rep), sig)  # same receiver: accepted
        self.assertTrue((self.tmp / "masks.png").exists())

    def test_stale_bake_refused(self):
        sig = bake.receiver_signature([self.lods[0]])
        rep = self.tmp / "bake-report.json"
        bake.write_bake_report(str(rep), receiverSignature=sig, device="CPU")
        self.lods[0].data.vertices[0].co.x += 0.01
        with self.assertRaises(bake.StaleBakeError):
            bake.check_bake_cache(str(rep), bake.receiver_signature([self.lods[0]]))

    def test_texel_density_measured(self):
        d = uv.measure_texel_density(self.lods[0], 1024)
        self.assertAlmostEqual(d["median"], 1024 / 32.0, delta=1.0)  # planar_cap_uv: 32 m square
```

Run: `make blender-test TEST=test_bake_export`
Expected: FAIL with `ImportError` (`collider`, `export`, `materials`, `uv` and `bake` don't exist yet).

- [ ] **Step 6: Write `materials.py` and `uv.py`**

Create `scripts/blender/lib/materials.py`:

```python
import math

import bpy


def srgb(hex_str):
    """'#rrggbb' -> linear RGBA (the-cube lib.py:46-55)."""
    h = hex_str.lstrip("#")
    c = [int(h[i:i + 2], 16) / 255.0 for i in (0, 2, 4)]
    lin = [x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c]
    return (*lin, 1.0)


def pbr_material(name, base_color, roughness, metallic=0.0, emission=None):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    p = next(n for n in m.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    p.inputs["Base Color"].default_value = base_color
    p.inputs["Roughness"].default_value = roughness
    p.inputs["Metallic"].default_value = metallic
    if emission is not None:
        p.inputs["Emission Color"].default_value = emission
        p.inputs["Emission Strength"].default_value = 1.0
    return m


def torus_coords(nt, scale):
    """Seamless 4D coordinates for a 1x1 UV bake plane: (cos 2πu, sin 2πu, cos 2πv)·s, w = sin 2πv·s.
    Feed into 4D Noise/Voronoi so the baked tile wraps on both axes."""
    tc = nt.nodes.new("ShaderNodeTexCoord")
    sep = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(tc.outputs["UV"], sep.inputs[0])

    def op(kind, a, b=None):
        n = nt.nodes.new("ShaderNodeMath")
        n.operation = kind
        nt.links.new(a, n.inputs[0])
        if b is not None:
            if isinstance(b, float):
                n.inputs[1].default_value = b
            else:
                nt.links.new(b, n.inputs[1])
        return n.outputs[0]

    r = scale / (2.0 * math.pi)
    au = op("MULTIPLY", sep.outputs["X"], 2.0 * math.pi)
    av = op("MULTIPLY", sep.outputs["Y"], 2.0 * math.pi)
    comb = nt.nodes.new("ShaderNodeCombineXYZ")
    nt.links.new(op("MULTIPLY", op("COSINE", au), r), comb.inputs["X"])
    nt.links.new(op("MULTIPLY", op("SINE", au), r), comb.inputs["Y"])
    nt.links.new(op("MULTIPLY", op("COSINE", av), r), comb.inputs["Z"])
    return comb.outputs[0], op("MULTIPLY", op("SINE", av), r)
```

Create `scripts/blender/lib/uv.py`:

```python
import statistics

import bpy

from . import hexgeo, scene


def planar_cap_uv(obj, layer="UVMap"):
    """Unique UV0 for a cap: top-down planar map of the 2R x 2R square (32 m -> 0..1).
    U = (x + R) / 2R, V = (y + R) / 2R in Blender's frame."""
    mesh = obj.data
    uvl = mesh.uv_layers.get(layer) or mesh.uv_layers.new(name=layer)
    s = 2.0 * hexgeo.HEX_R
    for loop in mesh.loops:
        co = mesh.vertices[loop.vertex_index].co
        uvl.data[loop.index].uv = ((co.x + hexgeo.HEX_R) / s, (co.y + hexgeo.HEX_R) / s)


def atlas_unwrap(objs, margin=0.008):
    """One atlas across objs (astra_machine_common.py:1162-1173)."""
    scene.select_only(objs)
    for o in objs:
        if not o.data.uv_layers:
            o.data.uv_layers.new(name="UVMap")
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=1.152, island_margin=margin, area_weight=0.2)
    bpy.ops.uv.pack_islands(rotate=True, margin=margin)
    bpy.ops.object.mode_set(mode="OBJECT")


def measure_texel_density(obj, atlas_px, layer="UVMap"):
    """px/m per face = sqrt(uv_area * atlas_px² / world_area); returns min/median/max over faces."""
    mesh = obj.data
    uvl = mesh.uv_layers[layer]
    w = obj.matrix_world
    vals = []
    for p in mesh.polygons:
        if p.area <= 1e-9:
            continue
        uvs = [uvl.data[li].uv for li in p.loop_indices]
        a = 0.0
        for i in range(1, len(uvs) - 1):
            e1, e2 = uvs[i] - uvs[0], uvs[i + 1] - uvs[0]
            a += abs(e1.x * e2.y - e1.y * e2.x) / 2.0
        if a <= 1e-12:
            continue  # collapsed UVs (e.g. skirts parked at one texel) are intentionally unmapped
        world_area = p.area * abs(w.determinant()) ** (2.0 / 3.0)
        vals.append((a * atlas_px * atlas_px / world_area) ** 0.5)
    return {"min": min(vals), "median": statistics.median(vals), "max": max(vals)}
```

- [ ] **Step 7: Write `bake.py`**

Create `scripts/blender/lib/bake.py`:

```python
"""Cycles bakes (ported from par-factory astra_machine_common.py:903-1620, machine specifics dropped)."""
import hashlib
import json
import os
import pathlib

import bpy
import numpy as np

from . import scene


class StaleBakeError(RuntimeError):
    pass


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def setup_cycles(device=None, samples=16):
    s = bpy.context.scene
    s.render.engine = "CYCLES"
    s.cycles.samples = samples
    device = device or os.environ.get("HEXW_BAKE_DEVICE", "CPU")
    prefs = bpy.context.preferences.addons["cycles"].preferences
    if device != "CPU":
        prefs.compute_device_type = "METAL"
        prefs.kernel_optimization_level = "OFF"  # Metal kernels otherwise recompile per channel
        prefs.get_devices()
        for d in prefs.devices:
            d.use = d.type == "METAL"
    s.cycles.device = "GPU" if device != "CPU" else "CPU"
    b = s.render.bake
    b.use_pass_direct = False  # no directional lighting baked into colour
    b.use_pass_indirect = False
    b.use_pass_color = True
    b.normal_space = "TANGENT"
    return device


def new_image(name, size, non_color):
    img = bpy.data.images.new(name, size, size, alpha=True, float_buffer=False)
    img.colorspace_settings.name = "Non-Color" if non_color else "sRGB"
    return img


def _target(low, image):
    """Make `image` the active Image Texture node in every material of the low mesh."""
    if not low.data.materials:
        low.data.materials.append(bpy.data.materials.new(f"{low.name}_bake"))
    for m in low.data.materials:
        m.use_nodes = True
        node = m.node_tree.nodes.get("HEXW_BAKE_TARGET") or m.node_tree.nodes.new("ShaderNodeTexImage")
        node.name = "HEXW_BAKE_TARGET"
        node.image = image
        m.node_tree.nodes.active = node


def bake_to(low, bake_type, image, highs=(), cage_extrusion=0.0, max_ray=0.0, margin=12, clear=True):
    """bake_type in {AO, NORMAL, ROUGHNESS, EMIT}. highs empty = bake the low mesh onto itself."""
    _target(low, image)
    b = bpy.context.scene.render.bake
    b.use_selected_to_active = bool(highs)
    b.cage_extrusion = cage_extrusion
    b.max_ray_distance = max_ray
    b.margin = margin
    b.use_clear = clear
    scene.select_only(list(highs) + [low], low)
    bpy.ops.object.bake(type=bake_type)


def bake_emit_input(highs, low, input_name, image, **kw):
    """Bake a Principled input (e.g. 'Base Color', 'Metallic') through EMIT so diffuse shading
    cannot darken it (astra_machine_common.py:1386-1443). Restores the materials afterwards."""
    sources = list(highs) or [low]
    restored = []
    for mat in {m for o in sources for m in o.data.materials if m}:
        nt = mat.node_tree
        out = next(n for n in nt.nodes if n.type == "OUTPUT_MATERIAL")
        old = out.inputs["Surface"].links[0].from_socket
        p = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
        emit = nt.nodes.new("ShaderNodeEmission")
        src = p.inputs[input_name]
        if src.is_linked:
            nt.links.new(src.links[0].from_socket, emit.inputs["Color"])
        else:
            v = src.default_value
            emit.inputs["Color"].default_value = (v, v, v, 1.0) if isinstance(v, float) else v
        nt.links.new(emit.outputs[0], out.inputs["Surface"])
        restored.append((nt, out, old, emit))
    try:
        bake_to(low, "EMIT", image, highs=highs, **kw)
    finally:
        for nt, out, old, emit in restored:
            nt.links.new(old, out.inputs["Surface"])
            nt.nodes.remove(emit)


def save_png(image, path):
    pathlib.Path(path).parent.mkdir(parents=True, exist_ok=True)
    image.filepath_raw = str(path)
    image.file_format = "PNG"
    image.save()
    return str(path)


def _load(path):
    img = bpy.data.images.load(str(path), check_existing=False)
    img.colorspace_settings.name = "Non-Color"
    a = np.empty(img.size[0] * img.size[1] * 4, np.float32)
    img.pixels.foreach_get(a)
    return img.size[0], a.reshape(-1, 4)


def _write(size, rgba, path, non_color=True):
    img = bpy.data.images.new(pathlib.Path(path).stem, size, size, alpha=True)
    img.colorspace_settings.name = "Non-Color" if non_color else "sRGB"
    img.pixels.foreach_set(np.ascontiguousarray(rgba, np.float32).ravel())
    return save_png(img, path)


def pack_orm(ao_png, rough_png, metal_png, out_png):
    """ORM: R = AO, G = roughness, B = metallic (glTF / Bevy metallic_roughness reads G, B)."""
    s, ao = _load(ao_png)
    _, r = _load(rough_png)
    _, m = _load(metal_png)
    rgba = np.stack([ao[:, 0], r[:, 0], m[:, 0], np.ones_like(ao[:, 0])], axis=1)
    return _write(s, rgba, out_png)


def make_masks(size, snow, wet, night, ao_png, out_png):
    """masks (linear): R snow capacity, G wetness/porosity, B night emissive, A AO (1.0 if None).
    snow/wet/night are floats or (size*size,) arrays in Blender pixel order."""
    n = size * size
    ch = [np.broadcast_to(np.asarray(c, np.float32), (n,)) for c in (snow, wet, night)]
    if ao_png:
        s, ao = _load(ao_png)
        if s != size:
            raise ValueError(f"AO size {s} != masks size {size}")
        a = ao[:, 0]
    else:
        a = np.ones(n, np.float32)
    return _write(size, np.stack([*ch, a], axis=1), out_png)


def receiver_signature(objs):
    """Hash of receiver topology, UVs and corner normals; a changed receiver invalidates cached bakes."""
    h = hashlib.sha256()
    for o in sorted(objs, key=lambda o: o.name):
        m = o.data
        h.update(o.name.encode())
        co = np.empty(len(m.vertices) * 3, np.float64)
        m.vertices.foreach_get("co", co)
        h.update(np.round(co, 6).tobytes())
        for p in m.polygons:
            h.update(np.asarray(p.vertices, np.int64).tobytes())
        for layer in m.uv_layers:
            uv = np.empty(len(m.loops) * 2, np.float64)
            layer.data.foreach_get("uv", uv)
            h.update(np.round(uv, 6).tobytes())
        cn = np.empty(len(m.loops) * 3, np.float64)
        m.corner_normals.foreach_get("vector", cn)
        h.update(np.round(cn, 5).tobytes())
    return h.hexdigest()


def write_bake_report(path, **fields):
    fields.setdefault("blender", bpy.app.version_string)
    pathlib.Path(path).parent.mkdir(parents=True, exist_ok=True)
    pathlib.Path(path).write_text(json.dumps(fields, indent=2, sort_keys=True) + "\n")


def check_bake_cache(report_path, signature):
    p = pathlib.Path(report_path)
    if not p.exists():
        raise StaleBakeError(f"no bake report at {p}")
    old = json.loads(p.read_text()).get("receiverSignature")
    if old != signature:
        raise StaleBakeError(f"receiver changed since bake ({old} != {signature}); rebake")


def bake_tileable_set(material, size, out_dir, set_id, metallic=0.0):
    """Bake a seamless tileable PBR set from a material built on lib.materials.torus_coords.
    Writes <set>.albedo.png (sRGB), <set>.normal.png and <set>.orm.png (linear). AO = 1 (flat plane)."""
    out = pathlib.Path(out_dir)
    bpy.ops.mesh.primitive_plane_add(size=1.0, location=(0, 0, 0))
    plane = bpy.context.active_object
    plane.name = f"{set_id}_bakeplane"
    plane.data.materials.append(material)
    albedo = new_image(f"{set_id}.albedo", size, non_color=False)
    bake_emit_input([], plane, "Base Color", albedo, margin=0)
    rough = new_image(f"{set_id}.rough", size, non_color=True)
    bake_to(plane, "ROUGHNESS", rough, margin=0)
    normal = new_image(f"{set_id}.normal", size, non_color=True)
    bake_to(plane, "NORMAL", normal, margin=0)
    files = {"albedo": save_png(albedo, out / f"{set_id}.albedo.png"),
             "normal": save_png(normal, out / f"{set_id}.normal.png")}
    rough_png = save_png(rough, out / f"{set_id}.rough.tmp.png")
    s, r = _load(rough_png)
    rgba = np.stack([np.ones_like(r[:, 0]), r[:, 0], np.full_like(r[:, 0], metallic), np.ones_like(r[:, 0])], axis=1)
    files["orm"] = _write(s, rgba, out / f"{set_id}.orm.png")
    os.remove(rough_png)
    return files
```

Bake margin 0 is correct for tileable sets, because a wrapping tile has no gutter. Unique atlases use margin 12 (brief §(e)).

- [ ] **Step 8: Write `collider.py` and `export.py`**

Create `scripts/blender/lib/collider.py`:

```python
import bmesh
import bpy

from . import scene


def collider_from(src, kind, name):
    """Collider mesh node named `name` (convention: <piece>_collider), custom prop collider=kind.
    Stays render-visible for export (hidden objects can drop out of selected exports); the engine
    hides it. kind: 'cap' (copy of src, e.g. LOD2), 'hull' (convex hull), 'box' (AABB)."""
    bm = bmesh.new()
    bm.from_mesh(src.data)
    bm.transform(src.matrix_world)
    if kind == "hull":
        res = bmesh.ops.convex_hull(bm, input=bm.verts)
        bmesh.ops.delete(bm, geom=res["geom_unused"] + res["geom_interior"], context="VERTS")
    elif kind == "box":
        xs, ys, zs = zip(*[v.co[:] for v in bm.verts])
        bm.clear()
        bmesh.ops.create_cube(bm, size=1.0)
        for v in bm.verts:
            v.co.x = (min(xs) + max(xs)) / 2 + v.co.x * (max(xs) - min(xs))
            v.co.y = (min(ys) + max(ys)) / 2 + v.co.y * (max(ys) - min(ys))
            v.co.z = (min(zs) + max(zs)) / 2 + v.co.z * (max(zs) - min(zs))
    elif kind != "cap":
        raise ValueError(f"unknown collider kind {kind}")
    bmesh.ops.triangulate(bm, faces=bm.faces)
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    obj = scene.link(bpy.data.objects.new(name, mesh))
    obj["collider"] = kind
    return obj
```

Create `scripts/blender/lib/export.py`:

```python
import json
import math
import pathlib

import bpy

from . import bake, geom, hexgeo, scene


def add_socket(name, loc, parent, yaw_deg=0.0, **extras):
    """Named empty exported as a glTF node with extras (export_extras=True).
    Names: socket-lane-<n> (network lanes), anchor-prop-<n> (prop anchors)."""
    e = scene.empty(name, loc, parent)
    e.rotation_euler = (0.0, 0.0, math.radians(yaw_deg))
    for k, v in extras.items():
        e[k] = v
    return e


def _tree(root):
    out = [root]
    for c in root.children:
        out += _tree(c)
    return out


def export_glb(root, path):
    """One root per GLB; no images (textures ship as sidecar KTX2/PNG, R6); tangents on; Y-up."""
    pathlib.Path(path).parent.mkdir(parents=True, exist_ok=True)
    objs = _tree(root)
    for o in objs:
        o.hide_render = False
        o.hide_set(False)
    scene.select_only(objs, root)
    bpy.ops.export_scene.gltf(
        filepath=str(path), export_format="GLB", use_selection=True, use_active_scene=True,
        export_yup=True, export_apply=True, export_extras=True, export_tangents=True,
        export_normals=True, export_texcoords=True, export_cameras=False, export_lights=False,
        export_image_format="NONE", export_materials="EXPORT", export_animations=False)


def write_export_report(path, root, piece_id, generator_path):
    nodes, lods, sockets = [], {}, []
    for o in _tree(root):
        nodes.append(o.name)
        if o.type == "MESH" and o.name.startswith(f"{piece_id}_lod"):
            lods[o.name] = geom.tri_count(o)
        if o.name.startswith(("socket-lane-", "anchor-prop-")):
            p = o.matrix_world.translation
            sockets.append({"name": o.name, "position": list(hexgeo.blender_to_rt(p.x, p.y, p.z)),
                            "yaw": math.degrees(o.matrix_world.to_euler().z),
                            "extras": {k: o[k] for k in o.keys() if not k.startswith("_")}})
    rep = {"pieceId": piece_id, "nodes": sorted(nodes), "lodTris": lods, "sockets": sockets,
           "generator": {"path": generator_path, "sha256": bake.sha256_file(generator_path)},
           "blender": bpy.app.version_string}
    pathlib.Path(path).write_text(json.dumps(rep, indent=2, sort_keys=True) + "\n")
    return rep
```

If `export_image_format="NONE"` is rejected by Blender 5.2's exporter, use `export_materials="PLACEHOLDER"` instead, which keeps material slots with no textures. The test asserts no `images` key, so either path must satisfy it. Record the outcome in `pitfalls.md`. Mesh data names must equal the object names (`<piece>_lod<n>`), because the runtime looks meshes up by name (Task 6). `geom._object` and `geom.join` already do this.

- [ ] **Step 9: Run the full Blender suite**

Run: `make blender-test`
Expected: PASS, all modules. Fix library code, not tests. The two exceptions are a confirmed Blender 5.2 API rename (record it in `pitfalls.md`) and a determinism failure. For the latter, find the non-deterministic input (unseeded RNG, set iteration order, or timestamped extras) and remove it.

- [ ] **Step 10: Document the lib contracts**

Replace `.claude/skills/blender-asset/references/lib-contracts.md` with this task's **Interfaces → Produces** list, plus each function's docstring.

- [ ] **Step 11: Gate and commit**

Run: `make checkall`
Expected: PASS.

```bash
git add scripts/blender crates/hex-core/src/axial.rs .claude/skills/blender-asset/references
git commit -m "feat(assets): Blender helper library (hex frame, CDT caps, bakes, LOD, collider, export) with headless tests"
```

---

### Task 4: Compress and encode: meshopt GLB, BC7/ASTC KTX2, PNG fallback and budgets

**Files:**
- Create: `scripts/assets/kit-lib.mjs`, `scripts/assets/kit-compress.mjs`, `scripts/assets/kit-compress.test.mjs`
- Modify: `Makefile` (`kit-build`, `kit-compress`, `kit-material`, `kit-determinism`, `kit-compress-test`)

**Interfaces:**
- Consumes:
  - The stage layout from Task 3 generators: `target/kit-stage/<id>/{<id>.glb,<id>.<channel>.png,bake-report.json,export-report.json}`, and for tileable sets `target/kit-stage/materials/<set>/<set>.<channel>.png`.
  - `tools/ktx-4.4.2` from `make fetch-tools`.
- Produces:
  - `kit-lib.mjs`:
    - `CHANNELS` (per channel: `{srgb: bool, rdo: number, normalize: bool}`).
    - `VK = {bc7: {srgb: 146, linear: 145}, astc: {srgb: 158, linear: 157}}`.
    - `ktx2Header(buf) -> {vkFormat, width, height, layers, faces, levels, supercompression}`.
    - `sha256(buf) -> hex`.
    - `GLB_MAX = 2097152`, `KTX_PIECE_MAX = 1500000`.
  - `kit-compress.mjs`:
    - `compressGlb(srcPath, dstPath) -> {bytes, sha256}`.
    - `encodeMap(pngPath, channel, outStem) -> [{family, path, bytes, sha256, vkFormat, levels}]`.
    - `compressDir({stageDir, outDir, reportPath, name}) -> report`.
    - CLI: `node scripts/assets/kit-compress.mjs piece <id>` or `node scripts/assets/kit-compress.mjs material <set> [--out dir --report path --stage dir]`.
  - Runtime files:
    - `assets/kit/p1/<id>/<id>.glb`.
    - `assets/kit/p1/<id>/<id>.<channel>.{bc7.ktx2,astc.ktx2,png}`.
    - `assets/kit/p1/materials/<set>/<set>.<channel>.{…}`.
  - `assets-source/kit/<id>/compress-report.json` (and `assets-source/kit/materials/<set>/compress-report.json`). Each holds `{toolVersions, glb: {bytes, sha256}, textureEncodes: [{channel, family, format, transfer, sha256, bytes, levels}], budgets: {glbBytes, ktxBytes: {bc7, astc}}}`.

- [ ] **Step 1: Write the failing Node test**

Create `scripts/assets/kit-compress.test.mjs`:

```js
// Local-only (needs tools/ktx-4.4.2): node --test scripts/assets/kit-compress.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Document, NodeIO } from "@gltf-transform/core";
import sharp from "sharp";
import { compressDir, encodeMap } from "./kit-compress.mjs";
import { ktx2Header, VK } from "./kit-lib.mjs";

async function stage(dir) {
  const doc = new Document();
  const buf = doc.createBuffer();
  const tri = (n, t, a) => doc.createAccessor().setType(t).setArray(a).setBuffer(buf);
  const prim = doc.createPrimitive()
    .setAttribute("POSITION", tri(3, "VEC3", new Float32Array([0, 0, 0, 1, 0, 0, 0, 0, 1])))
    .setAttribute("NORMAL", tri(3, "VEC3", new Float32Array([0, 1, 0, 0, 1, 0, 0, 1, 0])))
    .setAttribute("TANGENT", tri(3, "VEC4", new Float32Array([1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1])))
    .setAttribute("TEXCOORD_0", tri(3, "VEC2", new Float32Array([0, 0, 1, 0, 0, 1])));
  doc.createScene().addChild(doc.createNode("t").setMesh(doc.createMesh("t_lod0").addPrimitive(prim)));
  await new NodeIO().write(join(dir, "t.glb"), doc);
  const px = Buffer.alloc(64 * 64 * 4, 128);
  for (const ch of ["albedo", "normal", "masks"])
    await sharp(px, { raw: { width: 64, height: 64, channels: 4 } }).png().toFile(join(dir, `t.${ch}.png`));
}

test("linear channels encode UNORM, sRGB channels encode SRGB, full mips, zstd", async () => {
  const dir = await mkdtemp(join(tmpdir(), "kitc-"));
  await stage(dir);
  for (const [ch, srgb] of [["albedo", true], ["normal", false], ["masks", false]]) {
    const outs = await encodeMap(join(dir, `t.${ch}.png`), ch, join(dir, `out.${ch}`));
    for (const o of outs.filter((o) => o.family !== "png")) {
      const h = ktx2Header(await readFile(o.path));
      assert.equal(h.vkFormat, VK[o.family][srgb ? "srgb" : "linear"], `${ch} ${o.family}`);
      assert.equal(h.levels, 7); // 64 -> 1
      assert.equal(h.supercompression, 2); // zstd
    }
  }
});

test("compressDir writes a float-attribute meshopt GLB with no images and a report", async () => {
  const dir = await mkdtemp(join(tmpdir(), "kitc-"));
  await stage(dir);
  const out = join(dir, "out");
  const rep = await compressDir({ stageDir: dir, outDir: out, reportPath: join(dir, "rep.json"), name: "t" });
  const glb = await readFile(join(out, "t.glb"));
  const n = glb.readUInt32LE(12);
  const j = JSON.parse(glb.subarray(20, 20 + n).toString());
  assert.deepEqual(j.extensionsUsed, ["EXT_meshopt_compression"]);
  assert.equal(j.images, undefined);
  for (const a of j.accessors.filter((a) => a.type !== "SCALAR")) assert.equal(a.componentType, 5126);
  assert.ok(rep.textureEncodes.length === 9);
});

test("an embedded image fails the compress step", async () => {
  const dir = await mkdtemp(join(tmpdir(), "kitc-"));
  await stage(dir);
  const io = new NodeIO();
  const doc = await io.read(join(dir, "t.glb"));
  doc.createTexture("x").setImage(new Uint8Array(await readFile(join(dir, "t.albedo.png")))).setMimeType("image/png");
  await io.write(join(dir, "t.glb"), doc);
  await assert.rejects(compressDir({ stageDir: dir, outDir: join(dir, "o"), reportPath: join(dir, "r.json"), name: "t" }), /embeds images/);
});
```

Run: `make kit-compress-test`
Expected: FAIL with `No rule to make target` (then with `Cannot find module './kit-compress.mjs'` once the target exists).

- [ ] **Step 2: Write `kit-lib.mjs`**

```js
// Shared tables for the kit compress step and checks. Mirrors crates/hex-assets/src/kit_manifest.rs.
import { createHash } from "node:crypto";

// R6: base colour and emissive are sRGB; normal, ORM and masks are linear.
export const CHANNELS = {
  albedo: { srgb: true, rdo: 0.5, normalize: false },
  emissive: { srgb: true, rdo: 0.5, normalize: false },
  normal: { srgb: false, rdo: 0.25, normalize: true },
  orm: { srgb: false, rdo: 0.5, normalize: false },
  masks: { srgb: false, rdo: 0.5, normalize: false },
};
export const VK = { bc7: { srgb: 146, linear: 145 }, astc: { srgb: 158, linear: 157 } };
export const GLB_MAX = 2_097_152; // PRD §10
export const KTX_PIECE_MAX = 1_500_000; // D3, provisional per piece per family
const ID = [0xab, 0x4b, 0x54, 0x58, 0x20, 0x32, 0x30, 0xbb, 0x0d, 0x0a, 0x1a, 0x0a];

export function ktx2Header(buf) {
  if (buf.length < 48 || !ID.every((b, i) => buf[i] === b)) throw new Error("not a KTX2 file");
  const u = (o) => buf.readUInt32LE(o);
  return { vkFormat: u(12), width: u(20), height: u(24), layers: u(32), faces: u(36), levels: u(40), supercompression: u(44) };
}
export const sha256 = (buf) => createHash("sha256").update(buf).digest("hex");
export const fullLevels = (w, h) => Math.floor(Math.log2(Math.max(w, h))) + 1;
```

- [ ] **Step 3: Write `kit-compress.mjs`**

```js
// Stage -> runtime: meshopt GLB (FLOAT attributes, D7) + per-channel UASTC -> BC7/ASTC KTX2 (zstd) + PNG.
import { execFileSync } from "node:child_process";
import { copyFile, mkdir, readdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { basename, dirname, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { NodeIO } from "@gltf-transform/core";
import { EXTMeshoptCompression } from "@gltf-transform/extensions";
import { reorder } from "@gltf-transform/functions";
import { MeshoptEncoder } from "meshoptimizer";
import validator from "gltf-validator";
import { CHANNELS, GLB_MAX, KTX_PIECE_MAX, VK, fullLevels, ktx2Header, sha256 } from "./kit-lib.mjs";

const ROOT = resolve(dirname(new URL(import.meta.url).pathname), "../..");
const KTX = join(ROOT, "tools/ktx-4.4.2");
const ktxEnv = {
  ...process.env,
  PATH: `${KTX}/KTX-Software-4.4.2-Darwin-arm64-tools.pkg/Payload/usr/local/bin:${process.env.PATH}`,
  DYLD_LIBRARY_PATH: `${KTX}/KTX-Software-4.4.2-Darwin-arm64-library.pkg/Payload/usr/local/lib`,
};
const ktx = (...args) => execFileSync("ktx", args, { env: ktxEnv, stdio: ["ignore", "pipe", "pipe"] }).toString();

export async function compressGlb(src, dst) {
  const bytes = await readFile(src);
  const report = await validator.validateBytes(new Uint8Array(bytes));
  if (report.issues.numErrors > 0)
    throw new Error(`${src}: glTF validation errors: ${JSON.stringify(report.issues.messages.filter((m) => m.severity === 0))}`);
  await MeshoptEncoder.ready;
  const io = new NodeIO().registerExtensions([EXTMeshoptCompression]).registerDependencies({ "meshopt.encoder": MeshoptEncoder });
  const doc = await io.readBinary(new Uint8Array(bytes));
  if (doc.getRoot().listTextures().length > 0) throw new Error(`${src} embeds images (R6: textures ship as sidecar files)`);
  await doc.transform(reorder({ encoder: MeshoptEncoder }));
  // QUANTIZE is the P0 setting; it leaves attributes FLOAT (asserted below), no KHR_mesh_quantization.
  doc.createExtension(EXTMeshoptCompression).setRequired(true)
    .setEncoderOptions({ method: EXTMeshoptCompression.EncoderMethod.QUANTIZE });
  const out = Buffer.from(await io.writeBinary(doc));
  const j = JSON.parse(out.subarray(20, 20 + out.readUInt32LE(12)).toString());
  if ((j.extensionsUsed ?? []).some((e) => e !== "EXT_meshopt_compression"))
    throw new Error(`${src}: unexpected extensions ${j.extensionsUsed}`);
  for (const a of j.accessors.filter((a) => a.type !== "SCALAR"))
    if (a.componentType !== 5126) throw new Error(`${src}: non-float vertex accessor (D7)`);
  if (out.length > GLB_MAX) throw new Error(`${src}: GLB ${out.length} B > ${GLB_MAX} B budget`);
  await mkdir(dirname(dst), { recursive: true });
  await writeFile(dst, out);
  return { bytes: out.length, sha256: sha256(out) };
}

export async function encodeMap(png, channel, outStem) {
  const c = CHANNELS[channel];
  if (!c) throw new Error(`unknown channel ${channel}`);
  const tmp = `${outStem}.uastc.ktx2`;
  await mkdir(dirname(outStem), { recursive: true });
  ktx("create", "--format", c.srgb ? "R8G8B8A8_SRGB" : "R8G8B8A8_UNORM", "--assign-tf", c.srgb ? "srgb" : "linear",
    ...(c.normalize ? ["--normalize"] : []), "--encode", "uastc", "--uastc-quality", "4",
    "--uastc-rdo", "--uastc-rdo-l", String(c.rdo), "--zstd", "18", "--generate-mipmap", png, tmp);
  const outs = [];
  for (const family of ["bc7", "astc"]) {
    const path = `${outStem}.${family}.ktx2`;
    ktx("transcode", "--target", family, "--zstd", "18", tmp, path);
    const buf = await readFile(path);
    const h = ktx2Header(buf);
    const want = VK[family][c.srgb ? "srgb" : "linear"];
    if (h.vkFormat !== want) throw new Error(`${path}: vkFormat ${h.vkFormat}, expected ${want} (${channel})`);
    if (h.levels !== fullLevels(h.width, h.height)) throw new Error(`${path}: ${h.levels} mip levels`);
    if (Math.max(h.width, h.height) > 2048) throw new Error(`${path}: ${h.width}x${h.height} > 2048`);
    outs.push({ channel, family, path, format: `${family.toUpperCase()}_${c.srgb ? "SRGB" : "UNORM"}`,
      transfer: c.srgb ? "srgb" : "linear", vkFormat: h.vkFormat, levels: h.levels, bytes: buf.length, sha256: sha256(buf) });
  }
  await rm(tmp);
  const pngOut = `${outStem}.png`;
  await copyFile(png, pngOut);
  const pbuf = await readFile(pngOut);
  outs.push({ channel, family: "png", path: pngOut, format: "PNG", transfer: c.srgb ? "srgb" : "linear", bytes: pbuf.length, sha256: sha256(pbuf) });
  return outs;
}

export async function compressDir({ stageDir, outDir, reportPath, name }) {
  const files = await readdir(stageDir);
  const report = { name, date: new Date().toISOString(), toolVersions: {
      gltfTransform: "4.3.0", meshoptimizer: "0.25.0", ktxSoftware: ktx("--version").trim(), node: process.version },
    settings: { meshopt: "EXT_meshopt_compression QUANTIZE (FLOAT attributes)", uastcQuality: 4, zstd: 18, rdo: CHANNELS },
    textureEncodes: [], budgets: { ktxBytes: { bc7: 0, astc: 0 } } };
  if (files.includes(`${name}.glb`)) {
    report.glb = await compressGlb(join(stageDir, `${name}.glb`), join(outDir, `${name}.glb`));
    report.budgets.glbBytes = report.glb.bytes;
  }
  for (const f of files.sort()) {
    const m = f.match(new RegExp(`^${name}\\.(albedo|normal|orm|emissive|masks)\\.png$`));
    if (!m) continue;
    for (const o of await encodeMap(join(stageDir, f), m[1], join(outDir, `${name}.${m[1]}`))) {
      report.textureEncodes.push({ ...o, path: o.path.startsWith(ROOT) ? o.path.slice(ROOT.length + 1) : o.path });
      if (o.family !== "png") report.budgets.ktxBytes[o.family] += o.bytes;
    }
  }
  for (const [fam, b] of Object.entries(report.budgets.ktxBytes))
    if (b > KTX_PIECE_MAX) throw new Error(`${name}: ${fam} KTX2 total ${b} B > ${KTX_PIECE_MAX} B (D3)`);
  await mkdir(dirname(reportPath), { recursive: true });
  await writeFile(reportPath, JSON.stringify(report, null, 2) + "\n");
  return report;
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [kind, name, ...rest] = process.argv.slice(2);
  const opt = (k, d) => { const i = rest.indexOf(`--${k}`); return i >= 0 ? rest[i + 1] : d; };
  const sub = kind === "material" ? `materials/${name}` : name;
  if (!["piece", "material"].includes(kind) || !name) {
    console.error("usage: kit-compress.mjs <piece|material> <name> [--stage d --out d --report f]"); process.exit(2);
  }
  const rep = await compressDir({ name,
    stageDir: join(ROOT, opt("stage", `target/kit-stage/${sub}`)),
    outDir: join(ROOT, opt("out", `assets/kit/p1/${sub}`)),
    reportPath: join(ROOT, opt("report", `assets-source/kit/${sub}/compress-report.json`)) });
  console.log(`${name}: glb ${rep.glb?.bytes ?? "-"} B, ktx bc7 ${rep.budgets.ktxBytes.bc7} B, astc ${rep.budgets.ktxBytes.astc} B`);
}
```

- [ ] **Step 4: Add the make targets**

Add to `Makefile` and `.PHONY`:

```make
kit-build:
	@test -n "$(PIECE)" || { echo "usage: make kit-build PIECE=<id>"; exit 2; }
	rm -rf target/kit-stage/$(PIECE) && mkdir -p target/kit-stage/$(PIECE) assets-source/kit/$(PIECE)
	$(BLENDER_RUN) -P scripts/blender/kit/$(PIECE).py -- target/kit-stage/$(PIECE)
	cp target/kit-stage/$(PIECE)/bake-report.json target/kit-stage/$(PIECE)/export-report.json assets-source/kit/$(PIECE)/

kit-determinism:
	@test -n "$(PIECE)" || { echo "usage: make kit-determinism PIECE=<id>"; exit 2; }
	rm -rf target/kit-det && mkdir -p target/kit-det/a target/kit-det/b
	HEXW_SKIP_BAKE=1 $(BLENDER_RUN) -P scripts/blender/kit/$(PIECE).py -- target/kit-det/a
	HEXW_SKIP_BAKE=1 $(BLENDER_RUN) -P scripts/blender/kit/$(PIECE).py -- target/kit-det/b
	@a=$$(shasum -a 256 target/kit-det/a/$(PIECE).glb | cut -d' ' -f1); b=$$(shasum -a 256 target/kit-det/b/$(PIECE).glb | cut -d' ' -f1); \
	test "$$a" = "$$b" && echo "deterministic $$a" || { echo "FAIL: GLB differs $$a $$b"; exit 1; }

kit-compress: fetch-tools
	@test -n "$(PIECE)" || { echo "usage: make kit-compress PIECE=<id>"; exit 2; }
	cd scripts/assets && npm ci --no-audit --no-fund
	node scripts/assets/kit-compress.mjs piece $(PIECE)

kit-material: fetch-tools
	@test -n "$(SET)" || { echo "usage: make kit-material SET=<set>"; exit 2; }
	rm -rf target/kit-stage/materials/$(SET) && mkdir -p target/kit-stage/materials/$(SET)
	$(BLENDER_RUN) -P scripts/blender/kit/mat_$(SET).py -- target/kit-stage/materials/$(SET)
	cd scripts/assets && npm ci --no-audit --no-fund
	node scripts/assets/kit-compress.mjs material $(SET)
	cp target/kit-stage/materials/$(SET)/bake-report.json assets-source/kit/materials/$(SET)/

kit-compress-test: fetch-tools
	cd scripts/assets && npm ci --no-audit --no-fund
	node --test scripts/assets/kit-compress.test.mjs
```

Generators honor `HEXW_SKIP_BAKE=1`: they skip bakes and write geometry and the GLB only, so determinism is checked on geometry (bakes are compared by hash only on the same device).

- [ ] **Step 5: Run the tests**

Run: `make kit-compress-test`
Expected: PASS (3 tests). If `ktx create` rejects `--assign-tf` together with `--format …_SRGB`, drop `--assign-tf` for sRGB channels only. Linear channels need it. Keep the vkFormat assertion. If `validator.validateBytes` reports the default-scene or extras as errors on Blender output, list the messages and fix the exporter options. Do not filter errors out.

- [ ] **Step 6: Gate and commit**

Run: `make checkall`
Expected: PASS.

```bash
git add scripts/assets/kit-lib.mjs scripts/assets/kit-compress.mjs scripts/assets/kit-compress.test.mjs Makefile
git commit -m "feat(assets): kit compress step (meshopt float GLB, UASTC->BC7/ASTC KTX2 with per-channel color space, budgets)"
```

---

### Task 5: Gates: the Rust reimport gate, six-view renders, provenance and `make kit-check`

The gate is pure Rust, so it runs in CI's `make test` with no Blender or KTX tools. A committed fixture piece (the Task 3 fixture, compressed) keeps the gate exercised before any real piece exists.

**Files:**
- Create: `crates/hex-assets/src/ktx2_header.rs`, `crates/hex-assets/src/kit_manifest.rs`
- Modify: `crates/hex-assets/src/lib.rs`, `crates/hex-assets/src/bevy.rs`, `crates/hex-assets/Cargo.toml`
- Create: `crates/hex-render/tests/kit_gate.rs`, `crates/hex-render/tests/fixtures/kit/manifest.json`, plus the fixture files under `crates/hex-render/tests/fixtures/kit/p1/fixture/`
- Modify: `crates/hex-render/Cargo.toml` (dev-deps)
- Create: `scripts/blender/lib/gate.py`, `scripts/blender/kit_render.py`, `scripts/blender/kit/fixture.py`, `scripts/assets/kit-provenance.mjs`, `assets/kit/manifest.json`
- Modify: `Makefile` (`kit-renders`, `kit-provenance`, `kit-check`, `kit-fixture`; add `kit-check` to `checkall`), `.claude/skills/blender-asset/references/gates.md`

**Interfaces:**
- Consumes:
  - Task 3: `export.write_export_report`, `bake.write_bake_report`, `fixture_piece.build`.
  - Task 4: `compressDir`, `compress-report.json`.
  - Task 2: `refs.json`, `approval.json`.
- Produces:
  - Rust, `hex_assets`:
    - `Ktx2Info { vk_format, width, height, layers, faces, levels, supercompression }`.
    - `ktx2_info(&[u8]) -> Option<Ktx2Info>`, `ktx2_is_srgb(vk: u32) -> Option<bool>`, `full_mip_count(w: u32, h: u32) -> u32`.
    - `Channel { Albedo, Normal, Orm, Emissive, Masks }` with `is_srgb()` and `as_str()`.
    - `PieceKind { Base, Edge, Center, Prop, Tree, Hero }` with `lod0_budget()` and `touches_border()`.
    - `KitManifest { version, materials: BTreeMap<String, TileSet>, pieces: Vec<PieceEntry> }`.
    - `TileSet { tile_m: f32, maps: BTreeMap<Channel, String> }`.
    - `PieceEntry { id, kind, glb, provenance, slots: BTreeMap<String, Slot>, maps: BTreeMap<Channel, String> }`.
    - `Slot { tileable: Option<String>, unique: bool, masks: bool, color: Option<[f32; 3]>, roughness: Option<f32>, alpha_cutoff: Option<f32> }`.
    - `parse_manifest(&str) -> Result<KitManifest, String>`, `map_file(stem: &str, family: TextureFamily) -> String`.
    - Constants `GLB_MAX_BYTES`, `KTX_PIECE_MAX_BYTES`, `LOD1_MAX_RATIO`, `LOD2_MAX_RATIO`, `APOTHEM`.
  - Rust, `hex_assets::bevy`: `register_kit_source(app: &mut App, root: String)`. `MeshoptGlbPlugin` calls it with `kit_root()`.
  - `make kit-check` = `cargo test -p hex-render --test kit_gate`, which `checkall` depends on.
  - `make kit-renders PIECE=<id>` writes `assets-source/kit/<id>/renders/{front,back,left,right,top,three_quarter}.png`.
  - `make kit-provenance PIECE=<id>` writes `assets-source/kit/<id>/provenance.json`.
  - Manifest slot names are the Blender material names exported into the GLB.

- [ ] **Step 1: Write the failing KTX2 header and manifest unit tests**

Create `crates/hex-assets/src/ktx2_header.rs`:

```rust
//! Minimal KTX2 header reader for the asset gate (the runtime loads KTX2 through Bevy).

const IDENTIFIER: [u8; 12] = [
    0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
];
pub const VK_BC7_UNORM: u32 = 145;
pub const VK_BC7_SRGB: u32 = 146;
pub const VK_ASTC_4X4_UNORM: u32 = 157;
pub const VK_ASTC_4X4_SRGB: u32 = 158;
pub const VK_R16G16B16A16_SFLOAT: u32 = 97;
pub const SUPERCOMPRESSION_ZSTD: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ktx2Info {
    pub vk_format: u32,
    pub width: u32,
    pub height: u32,
    pub layers: u32,
    pub faces: u32,
    pub levels: u32,
    pub supercompression: u32,
}

pub fn ktx2_info(b: &[u8]) -> Option<Ktx2Info> {
    if b.len() < 48 || b[..12] != IDENTIFIER {
        return None;
    }
    let u = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    Some(Ktx2Info {
        vk_format: u(12),
        width: u(20),
        height: u(24),
        layers: u(32),
        faces: u(36),
        levels: u(40),
        supercompression: u(44),
    })
}

/// `Some(true)` for the sRGB block formats the kit ships, `Some(false)` for their UNORM twins.
pub fn ktx2_is_srgb(vk: u32) -> Option<bool> {
    match vk {
        VK_BC7_SRGB | VK_ASTC_4X4_SRGB => Some(true),
        VK_BC7_UNORM | VK_ASTC_4X4_UNORM => Some(false),
        _ => None,
    }
}

pub fn full_mip_count(w: u32, h: u32) -> u32 {
    32 - w.max(h).max(1).leading_zeros()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(vk: u32, w: u32, levels: u32) -> Vec<u8> {
        let mut b = IDENTIFIER.to_vec();
        for v in [vk, 1, w, w, 0, 0, 1, levels, SUPERCOMPRESSION_ZSTD] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b
    }

    #[test]
    fn parses_fields() {
        let i = ktx2_info(&header(VK_BC7_SRGB, 1024, 11)).unwrap();
        assert_eq!((i.vk_format, i.width, i.levels), (VK_BC7_SRGB, 1024, 11));
        assert_eq!(i.supercompression, SUPERCOMPRESSION_ZSTD);
    }

    #[test]
    fn rejects_non_ktx2_and_short_input() {
        assert!(ktx2_info(b"\x89PNG\r\n\x1a\n").is_none());
        assert!(ktx2_info(&header(VK_BC7_SRGB, 4, 3)[..40]).is_none());
    }

    #[test]
    fn color_space_table() {
        assert_eq!(ktx2_is_srgb(146), Some(true));
        assert_eq!(ktx2_is_srgb(158), Some(true));
        assert_eq!(ktx2_is_srgb(145), Some(false));
        assert_eq!(ktx2_is_srgb(157), Some(false));
        assert_eq!(ktx2_is_srgb(37), None);
    }

    #[test]
    fn mip_counts() {
        assert_eq!(full_mip_count(1024, 1024), 11);
        assert_eq!(full_mip_count(64, 32), 7);
        assert_eq!(full_mip_count(1, 1), 1);
    }
}
```

Create `crates/hex-assets/src/kit_manifest.rs`:

```rust
//! `assets/kit/manifest.json`: kit pieces, their material slots and sidecar maps (R6).
use crate::{TextureFamily, family_suffix};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const GLB_MAX_BYTES: u64 = 2_097_152; // PRD §10
/// D3: provisional, per piece (and per tileable set) per texture family. Re-set after P1 measurement.
pub const KTX_PIECE_MAX_BYTES: u64 = 1_500_000;
pub const LOD1_MAX_RATIO: f32 = 0.5;
pub const LOD2_MAX_RATIO: f32 = 0.15;
pub const APOTHEM: f32 = hex_core_apothem();

const fn hex_core_apothem() -> f32 {
    16.0 * 0.866_025_4
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Albedo,
    Normal,
    Orm,
    Emissive,
    Masks,
}

impl Channel {
    /// R6: base colour and emissive are sRGB; normal, ORM and masks are linear.
    pub fn is_srgb(self) -> bool {
        matches!(self, Channel::Albedo | Channel::Emissive)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Albedo => "albedo",
            Channel::Normal => "normal",
            Channel::Orm => "orm",
            Channel::Emissive => "emissive",
            Channel::Masks => "masks",
        }
    }
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PieceKind {
    Base,
    Edge,
    Center,
    Prop,
    Tree,
    Hero,
}

impl PieceKind {
    pub fn lod0_budget(self) -> usize {
        match self {
            PieceKind::Base | PieceKind::Edge | PieceKind::Center => 4_000,
            PieceKind::Hero => 12_000,
            PieceKind::Prop => 1_500,
            PieceKind::Tree => 3_000,
        }
    }

    /// Pieces whose geometry reaches the hex border and must keep the border ring in every LOD.
    pub fn touches_border(self) -> bool {
        matches!(self, PieceKind::Base | PieceKind::Edge | PieceKind::Center)
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TileSet {
    /// World meters covered by one repeat of the tile (D2: 1024 px over 2 m = 512 px/m).
    pub tile_m: f32,
    pub maps: BTreeMap<Channel, String>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Slot {
    /// World-space tileable set id (bases, road surfaces).
    #[serde(default)]
    pub tileable: Option<String>,
    /// Use the piece's own albedo/normal/orm/emissive on UV0 (props, trees).
    #[serde(default)]
    pub unique: bool,
    /// Bind the piece's `masks` map (§9.4 hooks + AO in A).
    #[serde(default)]
    pub masks: bool,
    /// Linear RGB base colour factor (default white). The GLB's own material factors are ignored.
    #[serde(default)]
    pub color: Option<[f32; 3]>,
    /// Perceptual roughness factor for untextured slots (default 1.0).
    #[serde(default)]
    pub roughness: Option<f32>,
    /// Alpha-test cutoff for leaf cards (AlphaMode::Mask); implies double-sided, no culling.
    #[serde(default)]
    pub alpha_cutoff: Option<f32>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PieceEntry {
    pub id: String,
    pub kind: PieceKind,
    /// Path under `assets/kit/` (the `kit://` root).
    pub glb: String,
    /// Repo-relative provenance path.
    pub provenance: String,
    /// Keyed by the glTF material name exported from Blender.
    pub slots: BTreeMap<String, Slot>,
    /// Map stems under `assets/kit/`, without the family suffix.
    #[serde(default)]
    pub maps: BTreeMap<Channel, String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KitManifest {
    pub version: u32,
    pub materials: BTreeMap<String, TileSet>,
    pub pieces: Vec<PieceEntry>,
}

pub fn parse_manifest(json: &str) -> Result<KitManifest, String> {
    let m: KitManifest = serde_json::from_str(json).map_err(|e| format!("manifest: {e}"))?;
    for p in &m.pieces {
        for (name, slot) in &p.slots {
            if let Some(set) = &slot.tileable
                && !m.materials.contains_key(set)
            {
                return Err(format!("{}: slot {name} uses unknown tileable set {set}", p.id));
            }
            if slot.masks && !p.maps.contains_key(&Channel::Masks) {
                return Err(format!("{}: slot {name} wants masks but the piece has none", p.id));
            }
        }
    }
    Ok(m)
}

pub fn map_file(stem: &str, family: TextureFamily) -> String {
    format!("{stem}.{}", family_suffix(family))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = r#"{"version":1,
      "materials":{"grass":{"tileM":2.0,"maps":{"albedo":"p1/materials/grass/grass.albedo"}}},
      "pieces":[{"id":"meadow_base","kind":"base","glb":"p1/meadow_base/meadow_base.glb",
        "provenance":"assets-source/kit/meadow_base/provenance.json",
        "slots":{"meadow_base_cap":{"tileable":"grass","masks":true}},
        "maps":{"masks":"p1/meadow_base/meadow_base.masks"}}]}"#;

    #[test]
    fn parses_and_resolves() {
        let m = parse_manifest(OK).unwrap();
        assert_eq!(m.pieces[0].kind, PieceKind::Base);
        assert_eq!(m.materials["grass"].tile_m, 2.0);
        assert_eq!(
            map_file(&m.pieces[0].maps[&Channel::Masks], TextureFamily::Astc),
            "p1/meadow_base/meadow_base.masks.astc.ktx2"
        );
    }

    #[test]
    fn unknown_set_or_missing_masks_is_an_error() {
        assert!(parse_manifest(&OK.replace(r#""tileable":"grass""#, r#""tileable":"sand""#)).is_err());
        assert!(parse_manifest(&OK.replace(r#""masks":"p1/meadow_base/meadow_base.masks""#, r#""orm":"x""#)).is_err());
    }

    #[test]
    fn channel_color_spaces() {
        assert!(Channel::Albedo.is_srgb() && Channel::Emissive.is_srgb());
        assert!(!Channel::Normal.is_srgb() && !Channel::Orm.is_srgb() && !Channel::Masks.is_srgb());
    }

    #[test]
    fn budgets() {
        assert_eq!(PieceKind::Base.lod0_budget(), 4_000);
        assert_eq!(PieceKind::Tree.lod0_budget(), 3_000);
        assert!((APOTHEM - 13.856_406).abs() < 1e-4);
    }
}
```

`deny_unknown_fields` on `PieceEntry` turns a misspelled key into an error instead of a silent default.

Modify `crates/hex-assets/src/lib.rs`:

```rust
//! Hex World asset loading: meshopt GLB rewrite, texture-family selection, kit manifest.
#[cfg(feature = "bevy")]
pub mod bevy;
mod kit_manifest;
mod ktx2_header;
mod meshopt_glb;
mod texture_family;

pub use kit_manifest::*;
pub use ktx2_header::*;
pub use meshopt_glb::{GlbError, rewrite_glb};
pub use texture_family::{TextureFamily, family_suffix, select_family};
```

In `crates/hex-assets/Cargo.toml` `[dependencies]`, add `serde = { version = "1", features = ["derive"] }`.

Run: `cargo test -p hex-assets`
Expected: PASS (the new unit tests plus the existing meshopt tests). If a test fails, fix the module, not the test.

- [ ] **Step 2: Expose a rooted `kit://` source**

In `crates/hex-assets/src/bevy.rs`, replace the `Plugin` impl with:

```rust
impl Plugin for MeshoptGlbPlugin {
    fn build(&self, app: &mut App) {
        register_kit_source(app, kit_root());
    }
}

/// Registers the `kit` source at `root` (tests pass an absolute fixture or repo path).
pub fn register_kit_source(app: &mut App, root: String) {
    app.register_asset_source(
        "kit",
        AssetSourceBuilder::new(move || {
            Box::new(MeshoptGlbReader(AssetSource::get_default_reader(
                root.clone(),
            )()))
        }),
    );
}
```

Run: `cargo check --workspace --all-targets`
Expected: PASS.

- [ ] **Step 3: Build and commit the gate fixture**

Create `scripts/blender/kit/fixture.py`, the generator form of `tests/fixture_piece.py`:

```python
"""Gate fixture piece: tiny base-like hex with 3 LODs, a collider and one lane socket."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from lib import bake, export  # noqa: E402
from tests import fixture_piece  # noqa: E402

out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
root, lods = fixture_piece.build("fixture")
bake.make_masks(64, 1.0, 0.5, 0.0, None, str(out / "fixture.masks.png"))
export.export_glb(root, str(out / "fixture.glb"))
export.write_export_report(str(out / "export-report.json"), root, "fixture", __file__)
bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature(lods), device="CPU", note="fixture: no bake")
```

The fixture builds the cap material `fixture_cap` (Task 3, `fixture_piece.build`).

Add to `Makefile` (and `.PHONY`):

```make
KIT_FIXTURE = crates/hex-render/tests/fixtures/kit
kit-fixture: fetch-tools
	rm -rf target/kit-stage/fixture && mkdir -p target/kit-stage/fixture
	$(BLENDER_RUN) -P scripts/blender/kit/fixture.py -- target/kit-stage/fixture
	cd scripts/assets && npm ci --no-audit --no-fund
	node scripts/assets/kit-compress.mjs piece fixture --out $(KIT_FIXTURE)/p1/fixture --report $(KIT_FIXTURE)/source/fixture/compress-report.json
	cp target/kit-stage/fixture/bake-report.json target/kit-stage/fixture/export-report.json $(KIT_FIXTURE)/source/fixture/
	node scripts/assets/kit-provenance.mjs fixture --source $(KIT_FIXTURE)/source/fixture --runtime $(KIT_FIXTURE) --validated "fixture gate"
```

Create `crates/hex-render/tests/fixtures/kit/manifest.json`:

```json
{ "version": 1, "materials": {},
  "pieces": [ { "id": "fixture", "kind": "base", "glb": "p1/fixture/fixture.glb",
    "provenance": "crates/hex-render/tests/fixtures/kit/source/fixture/provenance.json",
    "slots": { "fixture_cap": { "masks": true } },
    "maps": { "masks": "p1/fixture/fixture.masks" } } ] }
```

Create `assets/kit/manifest.json` as `{ "version": 1, "materials": {}, "pieces": [] }`. Tasks 8–10 add entries.

- [ ] **Step 4: Write `kit-provenance.mjs`**

Create `scripts/assets/kit-provenance.mjs`:

```js
// Assembles assets-source/kit/<id>/provenance.json from stage reports (brief §(d), PRD §11.4).
// Usage: node scripts/assets/kit-provenance.mjs <id> [--source dir] [--runtime dir] [--route scripted|interactive]
//        [--refs assets-source/refs/<ref-id>] [--blend path.blend] [--validated "<check list>"]
import { existsSync } from "node:fs";
import { readFile, writeFile } from "node:fs/promises";
import { dirname, join, relative, resolve } from "node:path";
import { sha256 } from "./kit-lib.mjs";

const ROOT = resolve(dirname(new URL(import.meta.url).pathname), "../..");
const [id, ...rest] = process.argv.slice(2);
const opt = (k, d) => { const i = rest.indexOf(`--${k}`); return i >= 0 ? rest[i + 1] : d; };
const src = join(ROOT, opt("source", `assets-source/kit/${id}`));
const runtime = join(ROOT, opt("runtime", "assets/kit"));
const read = async (p) => JSON.parse(await readFile(p, "utf8"));
const exp = await read(join(src, "export-report.json"));
const bk = await read(join(src, "bake-report.json"));
const cmp = await read(join(src, "compress-report.json"));
const file = async (p) => { const b = await readFile(p); return { path: relative(ROOT, p), sha256: sha256(b), bytes: b.length }; };
const derived = [await file(join(runtime, `p1/${id}/${id}.glb`))];
for (const t of cmp.textureEncodes) derived.push(await file(join(ROOT, t.path)));
const refsDir = opt("refs");
const refs = refsDir ? await read(join(ROOT, refsDir, "refs.json")) : [];
const approvals = existsSync(join(src, "approvals.json")) ? await read(join(src, "approvals.json")) : [];
const blend = opt("blend");
const lod = Object.entries(exp.lodTris).sort().map(([name, tris]) => ({ lod: Number(name.slice(-1)), tris }));
const prov = {
  pieceId: id, version: 1, route: opt("route", "scripted"),
  ...(blend
    ? { blendRef: { store: "local:assets-source/blend", key: relative(ROOT, resolve(blend)), ...(await file(resolve(blend))) } }
    : { generator: exp.generator }),
  authoringAgent: { harness: "claude-code", model: "claude-opus-5-5" },
  blender: exp.blender, toolVersions: cmp.toolVersions, bake: bk,
  lods: lod.map((l) => ({ ...l, ratio: l.tris / lod[0].tris })),
  collider: { node: `${id}_collider` }, sockets: exp.sockets, pivot: "hex_center",
  texelDensityPxPerM: bk.texelDensityPxPerM ?? null,
  compression: { meshopt: cmp.settings.meshopt, textureEncodes: cmp.textureEncodes.map(({ path, ...t }) => ({ path, ...t })) },
  derivedFiles: derived, references: refs, license: "MIT (original work, Hex World repo)",
  validation: opt("validated") ? { pass: true, date: new Date().toISOString(), checks: opt("validated").split(",") } : { pass: false },
  approvals,
};
await writeFile(join(src, "provenance.json"), JSON.stringify(prov, null, 2) + "\n");
console.log(`wrote ${relative(ROOT, join(src, "provenance.json"))} (validation.pass=${prov.validation.pass})`);
```

- [ ] **Step 5: Write the failing Rust gate**

Add to `crates/hex-render/Cargo.toml`:

```toml
[dev-dependencies]
gltf = "1"
serde_json = "1"
sha2 = "0.10"
```

Create `crates/hex-render/tests/kit_gate.rs`:

```rust
//! Kit asset gate (PRD §11.2 step 7, R7). Every manifest piece is loaded through the `kit://`
//! meshopt rewrite and bevy_gltf, and through the `gltf` crate for exact geometry checks.
//! Runs in `make test` (CI) and `make kit-check`. Fixture manifest: tests/fixtures/kit.
use bevy::{
    asset::{AssetPlugin, AssetServer, Assets, Handle, LoadState},
    gltf::{Gltf, GltfNode, GltfPlugin},
    mesh::MeshPlugin,
    prelude::*,
    world_serialization::WorldSerializationPlugin,
};
use hex_assets::*;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

struct Kit {
    /// Directory served as `kit://` (holds manifest.json).
    root: PathBuf,
    manifest: KitManifest,
}

fn kits() -> Vec<Kit> {
    [
        repo().join("crates/hex-render/tests/fixtures/kit"),
        repo().join("assets/kit"),
    ]
    .into_iter()
    .map(|root| {
        let json = std::fs::read_to_string(root.join("manifest.json")).unwrap();
        let manifest = parse_manifest(&json).unwrap_or_else(|e| panic!("{}: {e}", root.display()));
        Kit { root, manifest }
    })
    .collect()
}

fn fail(errors: Vec<String>) {
    assert!(errors.is_empty(), "kit gate:\n  {}", errors.join("\n  "));
}

/// The decoded (plain) glTF: document + buffers, via hex-assets' meshopt rewrite.
fn plain(kit: &Kit, p: &PieceEntry) -> Result<(gltf::Document, Vec<gltf::buffer::Data>, u64), String> {
    let path = kit.root.join(&p.glb);
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let out = rewrite_glb(&bytes).map_err(|e| format!("{}: {e}", p.id))?;
    let (doc, buffers, _) = gltf::import_slice(&out).map_err(|e| format!("{}: {e}", p.id))?;
    Ok((doc, buffers, bytes.len() as u64))
}

fn mesh_named<'a>(doc: &'a gltf::Document, name: &str) -> Option<gltf::Mesh<'a>> {
    doc.meshes().find(|m| m.name() == Some(name))
}

fn tris(m: &gltf::Mesh) -> usize {
    m.primitives()
        .map(|p| p.indices().map_or(0, |i| i.count()) / 3)
        .sum()
}

fn positions(m: &gltf::Mesh, buffers: &[gltf::buffer::Data]) -> Vec<([f32; 3], [f32; 3])> {
    let mut out = vec![];
    for prim in m.primitives() {
        let r = prim.reader(|b| Some(&buffers[b.index()]));
        let pos: Vec<[f32; 3]> = r.read_positions().map(|i| i.collect()).unwrap_or_default();
        let nrm: Vec<[f32; 3]> = r.read_normals().map(|i| i.collect()).unwrap_or_default();
        out.extend(pos.into_iter().zip(nrm));
    }
    out
}

/// Runtime frame: edge k's outward normal is at 60°·k in XZ (axial.rs: edge 0 = +x, edge 1 = +z).
fn dist_to_border(x: f32, z: f32) -> f32 {
    let m = (0..6)
        .map(|k| {
            let a = (60.0 * k as f32).to_radians();
            x * a.cos() + z * a.sin()
        })
        .fold(f32::MIN, f32::max);
    APOTHEM - m
}

fn border_ring(v: &[([f32; 3], [f32; 3])]) -> Vec<(i64, i64)> {
    let mut ring: Vec<(i64, i64)> = v
        .iter()
        .filter(|(p, n)| dist_to_border(p[0], p[2]).abs() < 1e-3 && p[1].abs() < 1e-3 && n[1] > 0.5)
        .map(|(p, _)| ((p[0] * 1e4).round() as i64, (p[2] * 1e4).round() as i64))
        .collect();
    ring.sort();
    ring.dedup();
    ring
}

fn check_geometry(kit: &Kit, p: &PieceEntry) -> Vec<String> {
    let mut e = vec![];
    let (doc, buffers, glb_bytes) = match plain(kit, p) {
        Ok(x) => x,
        Err(err) => return vec![err],
    };
    if glb_bytes > GLB_MAX_BYTES {
        e.push(format!("{}: GLB {glb_bytes} B > {GLB_MAX_BYTES}", p.id));
    }
    if doc.images().count() > 0 || doc.textures().count() > 0 {
        e.push(format!("{}: GLB embeds images (R6)", p.id));
    }
    if doc.extensions_used().any(|x| x == "KHR_mesh_quantization") {
        e.push(format!("{}: KHR_mesh_quantization is unsupported by bevy_gltf (D7)", p.id));
    }
    let lods: Vec<_> = (0..3).map(|i| mesh_named(&doc, &format!("{}_lod{i}", p.id))).collect();
    let Some(lod0) = &lods[0] else {
        return vec![format!("{}: mesh {}_lod0 missing", p.id, p.id)];
    };
    let t0 = tris(lod0);
    if t0 > p.kind.lod0_budget() {
        e.push(format!("{}: LOD0 {t0} tris > {}", p.id, p.kind.lod0_budget()));
    }
    for (i, max) in [(1, LOD1_MAX_RATIO), (2, LOD2_MAX_RATIO)] {
        match &lods[i] {
            None => e.push(format!("{}: mesh {}_lod{i} missing", p.id, p.id)),
            Some(m) if tris(m) as f32 > max * t0 as f32 => {
                e.push(format!("{}: LOD{i} {} tris > {max} x LOD0 {t0}", p.id, tris(m)))
            }
            _ => {}
        }
    }
    for m in lods.iter().flatten() {
        for prim in m.primitives() {
            if prim.get(&gltf::Semantic::Tangents).is_none() {
                e.push(format!("{}: {:?} primitive without TANGENT", p.id, m.name()));
            }
            for (sem, acc) in prim.attributes() {
                if acc.data_type() != gltf::accessor::DataType::F32 {
                    e.push(format!("{}: {sem:?} is not FLOAT (D7)", p.id));
                }
            }
            if let Some(name) = prim.material().name()
                && !p.slots.contains_key(name)
            {
                e.push(format!("{}: material {name} has no manifest slot", p.id));
            }
        }
        for (pos, _) in positions(m, &buffers) {
            if !pos.iter().all(|c| c.is_finite()) {
                e.push(format!("{}: non-finite position", p.id));
                break;
            }
        }
    }
    if !doc.nodes().any(|n| n.name() == Some(&format!("{}_collider", p.id))) {
        e.push(format!("{}: node {}_collider missing (D14)", p.id, p.id));
    }
    if p.kind.touches_border() {
        let rings: Vec<_> = lods.iter().flatten().map(|m| border_ring(&positions(m, &buffers))).collect();
        if rings.iter().any(|r| r != &rings[0]) {
            e.push(format!("{}: border ring differs across LODs", p.id));
        }
    }
    if p.kind == PieceKind::Base {
        let v = positions(lod0, &buffers);
        let ring = border_ring(&v);
        if let Some((p_bad, n_bad)) = v.iter().find(|(q, n)| {
            dist_to_border(q[0], q[2]).abs() < 1e-3 && q[1].abs() < 1e-3 && n[1] > 0.5 && n[1] < 0.999
        }) {
            e.push(format!("{}: cap border normal {n_bad:?} at {p_bad:?} is not +Y (seam shading)", p.id));
        }
        if ring.len() < 24 {
            e.push(format!("{}: border ring has {} vertices", p.id, ring.len()));
        }
        let rot: Vec<(i64, i64)> = {
            let (s, c) = 60f32.to_radians().sin_cos();
            let mut r: Vec<_> = ring
                .iter()
                .map(|&(x, z)| {
                    let (x, z) = (x as f32 / 1e4, z as f32 / 1e4);
                    (((x * c - z * s) * 1e4).round() as i64, ((x * s + z * c) * 1e4).round() as i64)
                })
                .collect();
            r.sort();
            r
        };
        if rot.iter().zip(&ring).any(|(a, b)| (a.0 - b.0).abs() > 1 || (a.1 - b.1).abs() > 1) {
            e.push(format!("{}: border ring is not invariant under 60° rotation", p.id));
        }
        let min_y = v.iter().map(|(p, _)| p[1]).fold(f32::MAX, f32::min);
        if (min_y + hex_core::LEVEL_STEP).abs() > 0.005 {
            e.push(format!("{}: skirt bottom {min_y} != -{}", p.id, hex_core::LEVEL_STEP));
        }
    }
    for node in doc.nodes().filter(|n| n.name().is_some_and(|s| s.starts_with("socket-lane-"))) {
        let name = node.name().unwrap();
        let extras: serde_json::Value = node
            .extras()
            .as_ref()
            .and_then(|r| serde_json::from_str(r.get()).ok())
            .unwrap_or_default();
        let (edge, offset) = (extras["edge"].as_u64(), extras["offset"].as_f64());
        let (Some(edge), Some(offset)) = (edge, offset) else {
            e.push(format!("{}: {name} extras lack edge/offset: {extras}", p.id));
            continue;
        };
        let a = (60.0 * edge as f32).to_radians();
        // hexgeo.lane_frame: Blender tangent t = (-n_y, n_x) = (sin a, cos a), which is runtime
        // (sin a, -cos a) in XZ for the runtime edge normal (cos a, sin a).
        let want = [
            APOTHEM * a.cos() + a.sin() * offset as f32,
            0.0,
            APOTHEM * a.sin() - a.cos() * offset as f32,
        ];
        let got = node.transform().decomposed().0;
        if (0..3).any(|i| (got[i] - want[i]).abs() > if i == 1 { 0.002 } else { 0.001 }) {
            e.push(format!("{}: {name} at {got:?}, lane contract {want:?}", p.id));
        }
    }
    e
}

/// Road edge pieces: the cross-section at the edge-0 border is mirror-symmetric about the lane
/// center (z = 0 for offset 0), so a 180°-rotated neighbour's section coincides, and it is the
/// same in every LOD, so neighbours at different LODs meet (Review Focus 2).
fn check_edge_profile(kit: &Kit, p: &PieceEntry) -> Vec<String> {
    if p.kind != PieceKind::Edge {
        return vec![];
    }
    let Ok((doc, buffers, _)) = plain(kit, p) else {
        return vec![];
    };
    let section = |m: &gltf::Mesh| {
        let mut sec: Vec<(i64, i64)> = positions(m, &buffers)
            .iter()
            .filter(|(q, _)| (q[0] - APOTHEM).abs() < 1e-3)
            .map(|(q, _)| ((q[1] * 1e4).round() as i64, (q[2] * 1e4).round() as i64))
            .collect();
        sec.sort();
        sec.dedup();
        sec
    };
    let secs: Vec<_> = (0..3)
        .filter_map(|i| mesh_named(&doc, &format!("{}_lod{i}", p.id)))
        .map(|m| section(&m))
        .collect();
    let Some(sec) = secs.first() else {
        return vec![];
    };
    let mut mirrored: Vec<_> = sec.iter().map(|&(y, z)| (y, -z)).collect();
    mirrored.sort();
    let mut e = vec![];
    if sec.is_empty() {
        e.push(format!("{}: no vertices on the edge-0 border", p.id));
    } else if sec != &mirrored {
        e.push(format!("{}: edge cross-section is not mirror-symmetric", p.id));
    }
    if secs.iter().any(|s| s != sec) {
        e.push(format!("{}: edge cross-section differs across LODs (LOD seam)", p.id));
    }
    e
}

fn stems(kit: &Kit, p: &PieceEntry) -> Vec<(Channel, String, &'static str)> {
    let mut v: Vec<_> = p.maps.iter().map(|(c, s)| (*c, s.clone(), "piece")).collect();
    for slot in p.slots.values() {
        if let Some(set) = &slot.tileable {
            v.extend(kit.manifest.materials[set].maps.iter().map(|(c, s)| (*c, s.clone(), "set")));
        }
    }
    v
}

fn check_textures(kit: &Kit, p: &PieceEntry) -> Vec<String> {
    let mut e = vec![];
    for fam in [TextureFamily::Bc7, TextureFamily::Astc] {
        let mut piece_bytes = 0u64;
        for (ch, stem, owner) in stems(kit, p) {
            let path = kit.root.join(map_file(&stem, fam));
            let Ok(bytes) = std::fs::read(&path) else {
                e.push(format!("{}: missing {}", p.id, path.display()));
                continue;
            };
            if owner == "piece" {
                piece_bytes += bytes.len() as u64;
            }
            let Some(info) = ktx2_info(&bytes) else {
                e.push(format!("{}: {} is not KTX2", p.id, path.display()));
                continue;
            };
            if ktx2_is_srgb(info.vk_format) != Some(ch.is_srgb()) {
                e.push(format!(
                    "{}: {} vkFormat {} wrong color space for {} (R6)",
                    p.id, path.display(), info.vk_format, ch.as_str()
                ));
            }
            if info.levels != full_mip_count(info.width, info.height) || info.width.max(info.height) > 2048 {
                e.push(format!("{}: {} has {} levels at {}x{}", p.id, path.display(), info.levels, info.width, info.height));
            }
            if info.supercompression != SUPERCOMPRESSION_ZSTD {
                e.push(format!("{}: {} is not zstd-supercompressed", p.id, path.display()));
            }
        }
        if piece_bytes > KTX_PIECE_MAX_BYTES {
            e.push(format!("{}: {fam:?} maps {piece_bytes} B > {KTX_PIECE_MAX_BYTES} (D3)", p.id));
        }
        for (_, stem, _) in stems(kit, p) {
            let png = kit.root.join(map_file(&stem, TextureFamily::Png));
            if !png.exists() {
                e.push(format!("{}: missing PNG fallback {}", p.id, png.display()));
            }
        }
    }
    e.sort();
    e.dedup();
    e
}

fn sha(path: &Path) -> Option<String> {
    let b = std::fs::read(path).ok()?;
    Some(Sha256::digest(&b).iter().map(|x| format!("{x:02x}")).collect())
}

fn check_provenance(p: &PieceEntry) -> Vec<String> {
    let path = repo().join(&p.provenance);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return vec![format!("{}: provenance missing at {}", p.id, path.display())];
    };
    let v: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(err) => return vec![format!("{}: provenance JSON: {err}", p.id)],
    };
    let mut e = vec![];
    for key in [
        "pieceId", "route", "authoringAgent", "blender", "toolVersions", "bake", "lods",
        "collider", "sockets", "pivot", "compression", "derivedFiles", "license", "validation",
    ] {
        if v.get(key).is_none() {
            e.push(format!("{}: provenance lacks {key}", p.id));
        }
    }
    if v.get("generator").is_none() && v.get("blendRef").is_none() {
        e.push(format!("{}: provenance needs generator or blendRef (R1)", p.id));
    }
    if v["authoringAgent"]["model"] != "claude-opus-5-5" {
        e.push(format!("{}: authoringAgent.model must be claude-opus-5-5 (R3)", p.id));
    }
    if v["validation"]["pass"] != true {
        e.push(format!("{}: validation.pass is not true", p.id));
    }
    for f in v["derivedFiles"].as_array().into_iter().flatten() {
        let rel = f["path"].as_str().unwrap_or_default();
        match sha(&repo().join(rel)) {
            None => e.push(format!("{}: derived file {rel} missing", p.id)),
            Some(h) if h != f["sha256"].as_str().unwrap_or_default() => {
                e.push(format!("{}: {rel} changed since provenance (stale)", p.id))
            }
            _ => {}
        }
    }
    e
}

/// Loads every piece through `kit://` (MeshoptGlbReader) and bevy_gltf, then checks names/extras.
fn check_bevy(kit: &Kit) -> Vec<String> {
    if kit.manifest.pieces.is_empty() {
        return vec![];
    }
    let mut app = App::new();
    hex_assets::bevy::register_kit_source(&mut app, kit.root.to_string_lossy().into_owned());
    app.add_plugins((
        TaskPoolPlugin::default(),
        AssetPlugin::default(),
        WorldSerializationPlugin,
        MeshPlugin,
        GltfPlugin::default(),
    ));
    app.finish();
    app.cleanup();
    let server = app.world().resource::<AssetServer>().clone();
    let handles: Vec<(String, Handle<Gltf>)> = kit
        .manifest
        .pieces
        .iter()
        .map(|p| (p.id.clone(), server.load(format!("kit://{}", p.glb))))
        .collect();
    let mut e = vec![];
    for _ in 0..10_000 {
        app.update();
        let states: Vec<_> = handles.iter().map(|(_, h)| server.get_load_state(h.id())).collect();
        if states.iter().all(|s| matches!(s, Some(LoadState::Loaded) | Some(LoadState::Failed(_)))) {
            break;
        }
    }
    let gltfs = app.world().resource::<Assets<Gltf>>();
    let nodes = app.world().resource::<Assets<GltfNode>>();
    for ((id, h), p) in handles.iter().zip(&kit.manifest.pieces) {
        match server.get_load_state(h.id()) {
            Some(LoadState::Failed(err)) => {
                e.push(format!("{id}: bevy_gltf load failed: {err}"));
                continue;
            }
            Some(LoadState::Loaded) => {}
            other => {
                e.push(format!("{id}: not loaded ({other:?})"));
                continue;
            }
        }
        let g = gltfs.get(h).unwrap();
        for want in [id.clone(), format!("{id}_lod0"), format!("{id}_lod1"), format!("{id}_lod2"), format!("{id}_collider")] {
            if !g.named_nodes.contains_key(want.as_str()) {
                e.push(format!("{id}: bevy node {want} missing"));
            }
        }
        for lod in 0..3 {
            if !g.named_meshes.contains_key(format!("{id}_lod{lod}").as_str()) {
                e.push(format!("{id}: bevy mesh {id}_lod{lod} missing (runtime looks meshes up by name)"));
            }
        }
        for (name, nh) in &g.named_nodes {
            if name.starts_with("socket-lane-") || name.starts_with("anchor-prop-") {
                let ok = nodes
                    .get(nh)
                    .and_then(|n| n.extras.as_ref())
                    .and_then(|x| serde_json::from_str::<serde_json::Value>(&x.value).ok())
                    .is_some_and(|v| v.get("kind").is_some());
                if !ok {
                    e.push(format!("{id}: {name} has no extras.kind through bevy_gltf"));
                }
            }
        }
        if p.kind == PieceKind::Edge && !g.named_nodes.keys().any(|k| k.starts_with("socket-lane-")) {
            e.push(format!("{id}: edge piece without socket-lane-*"));
        }
    }
    e
}

#[test]
fn manifests_parse() {
    assert!(!kits()[0].manifest.pieces.is_empty(), "fixture manifest must hold the fixture piece");
}

#[test]
fn geometry_budgets_lods_sockets_border() {
    let mut e = vec![];
    for k in kits() {
        for p in &k.manifest.pieces {
            e.extend(check_geometry(&k, p));
            e.extend(check_edge_profile(&k, p));
        }
    }
    fail(e);
}

#[test]
fn ktx2_color_space_mips_and_budgets() {
    let mut e = vec![];
    for k in kits() {
        for p in &k.manifest.pieces {
            e.extend(check_textures(&k, p));
        }
    }
    fail(e);
}

#[test]
fn provenance_complete_and_hashes_match() {
    let mut e = vec![];
    for k in kits() {
        for p in &k.manifest.pieces {
            e.extend(check_provenance(p));
        }
    }
    fail(e);
}

#[test]
fn bevy_gltf_reimport_names_and_extras() {
    let mut e = vec![];
    for k in kits() {
        e.extend(check_bevy(&k));
    }
    fail(e);
}

// Negative cases: the gate must catch these (Review Focus 3 and 4).
fn fixture() -> (Kit, PieceEntry) {
    let k = kits().remove(0);
    let p = k.manifest.pieces[0].clone();
    (k, p)
}

#[test]
fn gate_catches_wrong_color_space() {
    let (k, p) = fixture();
    let stem = &p.maps[&Channel::Masks];
    let tmp = std::env::temp_dir().join(format!("kitgate-{}", std::process::id()));
    std::fs::create_dir_all(tmp.join(Path::new(stem).parent().unwrap())).unwrap();
    for fam in [TextureFamily::Bc7, TextureFamily::Astc, TextureFamily::Png] {
        let mut b = std::fs::read(k.root.join(map_file(stem, fam))).unwrap();
        if fam == TextureFamily::Bc7 {
            b[12..16].copy_from_slice(&VK_BC7_SRGB.to_le_bytes());
        }
        std::fs::write(tmp.join(map_file(stem, fam)), b).unwrap();
    }
    let bad = Kit { root: tmp, manifest: k.manifest.clone() };
    let e = check_textures(&bad, &p);
    assert!(e.iter().any(|m| m.contains("wrong color space for masks")), "{e:?}");
}

#[test]
fn gate_catches_missing_file_and_stale_provenance() {
    let (k, mut p) = fixture();
    p.glb = "p1/fixture/nope.glb".into();
    assert!(check_geometry(&k, &p).iter().any(|m| m.contains("nope.glb")));
    let (_, mut p) = fixture();
    let tmp = std::env::temp_dir().join(format!("kitprov-{}.json", std::process::id()));
    let text = std::fs::read_to_string(repo().join(&p.provenance)).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["derivedFiles"][0]["sha256"] = "00".into();
    std::fs::write(&tmp, v.to_string()).unwrap();
    p.provenance = tmp.to_string_lossy().into_owned();
    assert!(check_provenance(&p).iter().any(|m| m.contains("stale")));
}
```

`check_provenance` joins `p.provenance` onto `repo()`. `Path::join` with an absolute path replaces the base, so the temp-file test works.

Run: `cargo test -p hex-render --test kit_gate`
Expected: FAIL. `manifests_parse` fails because the fixture files don't exist yet (or the panic names the missing file).

- [ ] **Step 6: Generate the fixture and make the gate pass**

Run: `make kit-fixture && cargo test -p hex-render --test kit_gate`
Expected: all 7 tests PASS.

Some failures need specific treatment:
- **Fixture border ring fails.** The fixture's 4-segment ring has 24 vertices, which meets the `>= 24` floor. Any other border failure is a real `geom` bug: go back to Task 3.
- **Bevy reports a missing plugin resource.** For example, `GltfPlugin` needs the `Image` assets. Add `bevy::image::ImagePlugin::default()` to the tuple, and record it in `pitfalls.md`.
- **Socket `want` mismatch.** The fixture uses offset 0, so the tangent sign only shows on a non-centered lane. The formula mirrors `hexgeo.lane_frame`. If the two disagree, fix the one that contradicts `test_lane_frame_edge0`, and keep the two in step.

- [ ] **Step 7: Six-view renders and the Blender reimport check**

Create `scripts/blender/lib/gate.py`:

```python
"""Blender-side gate helpers: six-view evidence renders and a socket round-trip check.
Renders are evidence only; look-dev approval happens in engine (brief §(e) look-dev parity)."""
import json
import math
import pathlib

import bpy
from mathutils import Vector

from . import scene

VIEWS = {"front": (0, -1, 0.35), "back": (0, 1, 0.35), "left": (-1, 0, 0.35), "right": (1, 0, 0.35),
         "top": (0, 0, 1), "three_quarter": (0.8, -0.8, 0.6)}


def import_glb(path):
    scene.reset_scene("gate")
    bpy.ops.import_scene.gltf(filepath=str(path))
    return [o for o in bpy.context.scene.objects]


def render_six_views(objs, out_dir, size=1024):
    s = bpy.context.scene
    s.render.engine = "BLENDER_EEVEE"
    s.render.resolution_x = s.render.resolution_y = size
    s.view_settings.view_transform = "AgX"
    world = bpy.data.worlds.new("gate") if not s.world else s.world
    s.world = world
    world.color = (0.18, 0.18, 0.2)
    sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
    sun.data.energy = 4.0
    sun.rotation_euler = (math.radians(50), 0, math.radians(30))
    scene.link(sun)
    meshes = [o for o in objs if o.type == "MESH" and o.name.endswith("_lod0")] or [o for o in objs if o.type == "MESH"]
    pts = [o.matrix_world @ Vector(c) for o in meshes for c in o.bound_box]
    lo = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
    hi = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
    center, radius = (lo + hi) / 2, (hi - lo).length / 2
    for o in objs:
        if o.type == "MESH":
            o.hide_render = not o.name.endswith("_lod0")
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    cam.data.type = "ORTHO"
    cam.data.ortho_scale = radius * 2.2
    scene.link(cam)
    s.camera = cam
    out = pathlib.Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    for name, d in VIEWS.items():
        cam.location = center + Vector(d).normalized() * radius * 4
        cam.rotation_euler = (center - cam.location).to_track_quat("-Z", "Y").to_euler()
        s.render.filepath = str(out / f"{name}.png")
        bpy.ops.render.render(write_still=True)
    return sorted(str(p) for p in out.glob("*.png"))


def reimport_check(glb_path, export_report_path, tol=0.001):
    """Socket names/positions survive export → import within tol (par-factory verify-modular-machine-bodies.py:25)."""
    rep = json.loads(pathlib.Path(export_report_path).read_text())
    objs = {o.name: o for o in import_glb(glb_path)}
    errors = [f"node {n} missing after reimport" for n in rep["nodes"] if n not in objs]
    for s in rep["sockets"]:
        o = objs.get(s["name"])
        if o is None:
            continue
        p = o.matrix_world.translation
        rt = (p.x, p.z, -p.y)
        if max(abs(a - b) for a, b in zip(rt, s["position"])) > tol:
            errors.append(f"{s['name']} at {rt}, exported at {s['position']}")
    if errors:
        raise AssertionError("; ".join(errors))
```

The glTF importer converts back to Z-up, so the round-trip compares in the runtime frame.

Create `scripts/blender/kit_render.py`:

```python
"""make kit-renders PIECE=<id>: reimport check + six-view renders of the staged export."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from lib import gate  # noqa: E402

piece = sys.argv[sys.argv.index("--") + 1]
stage = pathlib.Path("target/kit-stage") / piece
gate.reimport_check(stage / f"{piece}.glb", stage / "export-report.json")
objs = gate.import_glb(stage / f"{piece}.glb")
print("\n".join(gate.render_six_views(objs, pathlib.Path("assets-source/kit") / piece / "renders")))
```

Add to `Makefile` (and `.PHONY`), and make `checkall` run the gate:

```make
kit-renders:
	@test -n "$(PIECE)" || { echo "usage: make kit-renders PIECE=<id>"; exit 2; }
	$(BLENDER_RUN) -P scripts/blender/kit_render.py -- $(PIECE)

kit-check:
	cargo test -p hex-render --test kit_gate

# Writes provenance with validation.pass only after every non-provenance gate passes.
kit-provenance:
	@test -n "$(PIECE)" || { echo "usage: make kit-provenance PIECE=<id> [REFS=assets-source/refs/<ref>] [BLEND=path] [ROUTE=interactive]"; exit 2; }
	node scripts/assets/kit-provenance.mjs $(PIECE) $(if $(REFS),--refs $(REFS)) $(if $(BLEND),--blend $(BLEND)) $(if $(ROUTE),--route $(ROUTE))
	cargo test -p hex-render --test kit_gate -- --skip provenance
	$(BLENDER_RUN) -P scripts/blender/kit_render.py -- $(PIECE)
	node scripts/assets/kit-provenance.mjs $(PIECE) $(if $(REFS),--refs $(REFS)) $(if $(BLEND),--blend $(BLEND)) $(if $(ROUTE),--route $(ROUTE)) \
	  --validated "kit_gate geometry,kit_gate textures,kit_gate bevy reimport,blender reimport sockets,six-view render"
```

Change `checkall: fmt-check lint typecheck test` to `checkall: fmt-check lint typecheck test kit-check`.

Run: `make kit-renders PIECE=fixture` (`target/kit-stage/fixture` exists from `make kit-fixture`)
Expected: six PNG paths printed under `assets-source/kit/fixture/renders/`. View one with Read to confirm the hex is framed. Then delete `assets-source/kit/fixture/`, because fixture evidence is not committed.

- [ ] **Step 8: Update `gates.md` and gate everything**

Fill the "Pinned by" column of `.claude/skills/blender-asset/references/gates.md` with the test names above.

Run: `make checkall`
Expected: PASS, including `kit_gate` (7 tests).

- [ ] **Step 9: Commit**

```bash
git add crates/hex-assets crates/hex-render/Cargo.toml crates/hex-render/tests scripts/blender scripts/assets/kit-provenance.mjs assets/kit/manifest.json Makefile .claude/skills/blender-asset/references/gates.md Cargo.lock
git commit -m "feat(assets): Rust kit gate (meshopt + bevy_gltf reimport, LOD/border/socket/KTX2/provenance checks) wired into checkall"
```

---

### Task 6: Runtime kit material, piece loader and the `kit` scenario

**Files:**
- Create: `crates/hex-render/src/kit_material.rs`, `crates/hex-render/src/kit_scene.rs`, `assets/shaders/kit_material.wesl`, `scripts/shots/kit.json`
- Modify:
  - `crates/hex-render/src/lib.rs` (`Scenario::Kit`, plugin registration);
  - `crates/hex-render/src/scene.rs` (setup arm; `lod_range` → `pub(crate)`);
  - `crates/hex-render/src/workload.rs` (`KitHooks` in `GateConfig`);
  - `crates/hex-app/src/args.rs` (`kit_hooks` key; `web_pairs`);
  - `Makefile` (`shots-kit`, `web-shots-kit`);
  - `docs/agent-operability.md` (scenario + key rows).

**Interfaces:**
- Consumes:
  - `hex_assets::{KitManifest, PieceEntry, Slot, Channel, parse_manifest, map_file, select_family, TextureFamily}` (Task 5).
  - `scene::lod_range(usize) -> VisibilityRange`.
- Produces:
  - `pub type KitMaterial = ExtendedMaterial<StandardMaterial, KitExt>`.
  - `KitExt { params: KitParams, masks, tile_albedo, tile_normal, tile_orm }`, with bindings 100–106.
  - `KitParams { tile_inv_m, flags, snow, wetness, night, _pad0, _pad1, _pad2 }`.
  - Flags `FLAG_TILEABLE = 1` and `FLAG_MASKS = 2`.
  - `MapRequest { path: String, srgb: bool, repeat: bool }`.
  - `slot_requests(&KitManifest, &PieceEntry, &Slot, TextureFamily) -> SlotRequests { masks: Option<MapRequest>, tile: Option<([MapRequest; 3], f32)>, unique: Vec<(Channel, MapRequest)> }`.
  - `build_material(&AssetServer, &Slot, &SlotRequests, KitHooks) -> KitMaterial`. It applies `Slot.color`, `roughness` and `alpha_cutoff`.
  - `Placement { piece: &'static str, cell: Axial, rot: u8, offset: [f32; 2] }`.
  - `kit_layout() -> Vec<Placement>`.
  - `resolve_placements(&KitManifest, &[Placement]) -> (Vec<Placement>, Vec<String>)`.
  - `piece_rotation(rot: u8) -> Quat`.
  - `Scenario::Kit` (`"kit"`).
  - `KitHooks { None, Snow, Wet, Night }` (`kit_hooks=` key).
  - Shot names `kit-overview`, `kit-seam-closeup`, `kit-lod1`, `kit-lod2`, `kit-tree`.

- [ ] **Step 1: Write the failing unit tests**

Create `crates/hex-render/src/kit_material.rs` with only its test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use hex_assets::{TextureFamily, parse_manifest};

    const M: &str = r#"{"version":1,
      "materials":{"grass":{"tileM":2.0,"maps":{"albedo":"s/g.albedo","normal":"s/g.normal","orm":"s/g.orm"}}},
      "pieces":[{"id":"b","kind":"base","glb":"b.glb","provenance":"p.json",
        "slots":{"b_cap":{"tileable":"grass","masks":true},"b_tree":{"unique":true}},
        "maps":{"masks":"b/b.masks","albedo":"b/b.albedo","normal":"b/b.normal","orm":"b/b.orm"}}]}"#;

    #[test]
    fn png_family_loads_linear_channels_linear() {
        let m = parse_manifest(M).unwrap();
        let p = &m.pieces[0];
        let cap = slot_requests(&m, p, &p.slots["b_cap"], TextureFamily::Png);
        assert!(!cap.masks.as_ref().unwrap().srgb);
        let (tile, inv) = cap.tile.unwrap();
        assert_eq!(inv, 0.5);
        assert!(tile[0].srgb && !tile[1].srgb && !tile[2].srgb); // albedo, normal, orm
        assert!(tile.iter().all(|r| r.repeat && r.path.ends_with(".png")));
        let tree = slot_requests(&m, p, &p.slots["b_tree"], TextureFamily::Png);
        for (ch, r) in &tree.unique {
            assert_eq!(r.srgb, ch.is_srgb(), "{ch:?}");
            assert!(!r.repeat);
        }
    }

    #[test]
    fn ktx_families_pick_suffix() {
        let m = parse_manifest(M).unwrap();
        let p = &m.pieces[0];
        let r = slot_requests(&m, p, &p.slots["b_cap"], TextureFamily::Astc);
        assert_eq!(r.masks.unwrap().path, "b/b.masks.astc.ktx2");
    }

    #[test]
    fn params_flags() {
        let m = parse_manifest(M).unwrap();
        let p = &m.pieces[0];
        let r = slot_requests(&m, p, &p.slots["b_cap"], TextureFamily::Bc7);
        assert_eq!(params_for(&r, KitHooks::None).flags, FLAG_TILEABLE | FLAG_MASKS);
        assert_eq!(params_for(&r, KitHooks::Snow).snow, 1.0);
    }
}
```

Create `crates/hex-render/src/kit_scene.rs` with only its test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use hex_assets::parse_manifest;

    #[test]
    fn rotation_maps_edge0_to_edge_k() {
        // Edge k's outward direction in XZ is at 60°·k (axial.rs: edge 1 = +z).
        for k in 0..6u8 {
            let v = piece_rotation(k) * Vec3::X;
            let a = (60.0 * k as f32).to_radians();
            assert!((v - Vec3::new(a.cos(), 0.0, a.sin())).length() < 1e-5, "k={k} {v}");
        }
    }

    #[test]
    fn road_tiles_meet_at_edge0() {
        let l = kit_layout();
        let roads: Vec<_> = l.iter().filter(|p| p.piece == "road_straight_center").collect();
        assert_eq!(roads.len(), 2);
        assert_eq!(roads[0].cell.neighbor(0), roads[1].cell);
    }

    #[test]
    fn missing_piece_is_skipped_not_panic() {
        let m = parse_manifest(r#"{"version":1,"materials":{},"pieces":[]}"#).unwrap();
        let (ok, warn) = resolve_placements(&m, &kit_layout());
        assert!(ok.is_empty());
        assert!(warn.iter().any(|w| w.contains("meadow_base")));
    }
}
```

Add `mod kit_material;` and `mod kit_scene;` to `crates/hex-render/src/lib.rs`.

Run: `cargo test -p hex-render --lib kit_`
Expected: FAIL to compile (`slot_requests`, `piece_rotation` and the rest are undefined).

- [ ] **Step 2: Write `kit_material.rs`**

Put this above the test module:

```rust
//! Kit material (R6): StandardMaterial + `KitExt`. Tileable layers are sampled in world space in
//! `kit_material.wesl` so rotated/translated neighbours line up; `masks` (UV0) carries the §9.4
//! hooks (R snow, G wetness, B night emissive) and baked AO in A.
use bevy::{
    image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};
use hex_assets::{Channel, KitManifest, PieceEntry, Slot, TextureFamily, map_file};

use crate::workload::KitHooks;

pub const SHADER: &str = "shaders/kit_material.wesl";
pub const FLAG_TILEABLE: u32 = 1;
pub const FLAG_MASKS: u32 = 2;

pub type KitMaterial = ExtendedMaterial<StandardMaterial, KitExt>;

#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct KitParams {
    pub tile_inv_m: f32,
    pub flags: u32,
    pub snow: f32,
    pub wetness: f32,
    pub night: f32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct KitExt {
    #[uniform(100)]
    pub params: KitParams,
    #[texture(101)]
    #[sampler(102)]
    pub masks: Option<Handle<Image>>,
    #[texture(103)]
    #[sampler(104)]
    pub tile_albedo: Option<Handle<Image>>,
    #[texture(105)]
    pub tile_normal: Option<Handle<Image>>,
    #[texture(106)]
    pub tile_orm: Option<Handle<Image>>,
}

impl MaterialExtension for KitExt {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapRequest {
    pub path: String,
    pub srgb: bool,
    pub repeat: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SlotRequests {
    pub masks: Option<MapRequest>,
    /// Tileable albedo, normal, orm and 1 / tile_m.
    pub tile: Option<([MapRequest; 3], f32)>,
    pub unique: Vec<(Channel, MapRequest)>,
}

fn req(stem: &str, ch: Channel, family: TextureFamily, repeat: bool) -> MapRequest {
    MapRequest {
        path: map_file(stem, family),
        srgb: ch.is_srgb(),
        repeat,
    }
}

/// Which files a material slot needs and how to load them. Color space comes from the channel,
/// never the file: PNG has no color-space field (Review Focus 3).
pub fn slot_requests(m: &KitManifest, p: &PieceEntry, slot: &Slot, family: TextureFamily) -> SlotRequests {
    let mut out = SlotRequests::default();
    if slot.masks {
        out.masks = p.maps.get(&Channel::Masks).map(|s| req(s, Channel::Masks, family, false));
    }
    if let Some(set) = slot.tileable.as_ref().and_then(|id| m.materials.get(id)) {
        let get = |ch: Channel| req(&set.maps[&ch], ch, family, true);
        out.tile = Some((
            [get(Channel::Albedo), get(Channel::Normal), get(Channel::Orm)],
            1.0 / set.tile_m,
        ));
    }
    if slot.unique {
        for ch in [Channel::Albedo, Channel::Normal, Channel::Orm, Channel::Emissive] {
            if let Some(s) = p.maps.get(&ch) {
                out.unique.push((ch, req(s, ch, family, false)));
            }
        }
    }
    out
}

pub fn params_for(r: &SlotRequests, hooks: KitHooks) -> KitParams {
    let mut flags = 0;
    if r.tile.is_some() {
        flags |= FLAG_TILEABLE;
    }
    if r.masks.is_some() {
        flags |= FLAG_MASKS;
    }
    KitParams {
        tile_inv_m: r.tile.as_ref().map_or(0.0, |t| t.1),
        flags,
        snow: (hooks == KitHooks::Snow) as u32 as f32,
        wetness: (hooks == KitHooks::Wet) as u32 as f32,
        night: (hooks == KitHooks::Night) as u32 as f32,
        ..default()
    }
}

pub fn load_map(server: &AssetServer, r: &MapRequest) -> Handle<Image> {
    let (srgb, repeat) = (r.srgb, r.repeat);
    server.load_with_settings(format!("kit://{}", r.path), move |s: &mut ImageLoaderSettings| {
        s.is_srgb = srgb;
        let mut d = ImageSamplerDescriptor::linear();
        d.anisotropy_clamp = 16;
        if repeat {
            d.address_mode_u = ImageAddressMode::Repeat;
            d.address_mode_v = ImageAddressMode::Repeat;
        }
        s.sampler = ImageSampler::Descriptor(d);
    })
}

pub fn build_material(server: &AssetServer, slot: &Slot, r: &SlotRequests, hooks: KitHooks) -> KitMaterial {
    let mut base = StandardMaterial {
        perceptual_roughness: 1.0,
        metallic: 1.0, // multiplied by the ORM texture; tileable path overrides in the shader
        ..default()
    };
    for (ch, mr) in &r.unique {
        let h = load_map(server, mr);
        match ch {
            Channel::Albedo => base.base_color_texture = Some(h),
            Channel::Normal => base.normal_map_texture = Some(h),
            Channel::Orm => {
                base.metallic_roughness_texture = Some(h.clone());
                base.occlusion_texture = Some(h);
            }
            Channel::Emissive => {
                base.emissive_texture = Some(h);
                base.emissive = LinearRgba::WHITE;
            }
            Channel::Masks => {}
        }
    }
    if r.unique.iter().all(|(c, _)| *c != Channel::Orm) {
        base.metallic = 0.0;
        base.perceptual_roughness = slot.roughness.unwrap_or(1.0);
    }
    if let Some([cr, cg, cb]) = slot.color {
        base.base_color = Color::linear_rgb(cr, cg, cb);
    }
    if let Some(c) = slot.alpha_cutoff {
        base.alpha_mode = AlphaMode::Mask(c);
        base.double_sided = true;
        base.cull_mode = None;
    }
    let tile = r.tile.as_ref().map(|(t, _)| t.clone().map(|m| load_map(server, &m)));
    let [tile_albedo, tile_normal, tile_orm] = match tile {
        Some([a, n, o]) => [Some(a), Some(n), Some(o)],
        None => [None, None, None],
    };
    KitMaterial {
        base,
        extension: KitExt {
            params: params_for(r, hooks),
            masks: r.masks.as_ref().map(|m| load_map(server, m)),
            tile_albedo,
            tile_normal,
            tile_orm,
        },
    }
}
```

- [ ] **Step 3: Write the shader**

Create `assets/shaders/kit_material.wesl`:

```wesl
// Kit pieces (crates/hex-render/src/kit_material.rs). StandardMaterial + KitExt (bindings 100-106).
import bevy_pbr::render::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
};

@if(PREPASS_PIPELINE)
import bevy_pbr::{
    prepass::io::{VertexOutput, FragmentOutput},
    deferred::functions::deferred_output,
};
@else
import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
};

struct KitParams {
    tile_inv_m: f32,
    flags: u32,
    snow: f32,
    wetness: f32,
    night: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

const FLAG_TILEABLE: u32 = 1u;
const FLAG_MASKS: u32 = 2u;

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> kit: KitParams;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var masks_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var masks_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(103) var tile_albedo: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(104) var tile_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(105) var tile_normal: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(106) var tile_orm: texture_2d<f32>;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    if ((kit.flags & FLAG_TILEABLE) != 0u) {
        // World-space tiling: texture "up" is -Z on up-facing surfaces (the Blender bake plane's
        // +Y) and +Y on walls. Image rows run top-down, so v is negated. T, B, N match Blender's
        // tangent space for the bake plane, so the OpenGL-convention normal map applies directly.
        let N = normalize(in.world_normal);
        let P = in.world_position.xyz;
        var up = vec3(0.0, 1.0, 0.0);
        if (abs(N.y) >= 0.5) {
            up = vec3(0.0, 0.0, -1.0);
        }
        let B = normalize(up - N * dot(N, up));
        let T = cross(B, N);
        let uv = vec2(dot(P, T), -dot(P, B)) * kit.tile_inv_m;
        let albedo = textureSample(tile_albedo, tile_sampler, uv);
        let orm = textureSample(tile_orm, tile_sampler, uv);
        let tn = textureSample(tile_normal, tile_sampler, uv).xyz * 2.0 - 1.0;
        pbr_input.material.base_color = vec4(pbr_input.material.base_color.rgb * albedo.rgb, pbr_input.material.base_color.a);
        pbr_input.material.perceptual_roughness = orm.g;
        pbr_input.material.metallic = orm.b;
        pbr_input.N = normalize(T * tn.x + B * tn.y + N * tn.z);
    }

    @if(VERTEX_UVS_A) {
    if ((kit.flags & FLAG_MASKS) != 0u) {
        let m = textureSample(masks_texture, masks_sampler, in.uv);
        pbr_input.diffuse_occlusion *= m.a;
        // §9.4 hooks: zero at the fixed noon; `kit_hooks=snow|wet|night` drives them for verification.
        let cover = m.r * kit.snow * smoothstep(0.55, 0.85, pbr_input.N.y);
        let c0 = mix(pbr_input.material.base_color.rgb, vec3(0.90, 0.92, 0.95), cover);
        pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, 0.55, cover);
        let wet = m.g * kit.wetness * (1.0 - cover);
        pbr_input.material.base_color = vec4(c0 * mix(1.0, 0.55, wet), pbr_input.material.base_color.a);
        pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, 0.12, wet);
        pbr_input.material.emissive = vec4(pbr_input.material.emissive.rgb + c0 * m.b * kit.night * 4.0, pbr_input.material.emissive.a);
    }
    }

    pbr_input.material.base_color = alpha_discard(pbr_input.material.flags, pbr_input.material.alpha_cutoff, pbr_input.material.base_color);
    var out: FragmentOutput;
@if(PREPASS_PIPELINE) {
    out = deferred_output(in, pbr_input);
} @else {
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
}
    return out;
}
```

`KitParams` uses three scalar pads, not an array. An array in a uniform needs a 16-byte element stride, so scalars keep the Rust and WGSL layouts at 32 bytes.

- [ ] **Step 4: Write `kit_scene.rs`, the scenario wiring and the hooks key**

Put this above the test module in `kit_scene.rs`:

```rust
//! `scenario=kit`: the P1 family laid out for look-dev, the seam close-up and LOD checks.
//! Pieces come from `assets/kit/manifest.json` (compiled in); missing pieces warn and are skipped.
use bevy::{
    gltf::{Gltf, GltfMesh},
    prelude::*,
    render::{renderer::RenderDevice, settings::WgpuFeatures},
};
use hex_assets::{KitManifest, TextureFamily, parse_manifest, select_family};
use hex_core::{Axial, HEX_RADIUS};
use std::collections::HashMap;

use crate::{
    SceneEntity,
    kit_material::{KitMaterial, build_material, slot_requests},
    scene::lod_range,
    workload::GateConfig,
};

pub const MANIFEST: &str = include_str!("../../../assets/kit/manifest.json");

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub piece: &'static str,
    pub cell: Axial,
    /// Rotation k: edge e maps to edge (e + k) mod 6 (PRD §5.1).
    pub rot: u8,
    /// Offset from the cell center in meters (x, z), applied after rotation.
    pub offset: [f32; 2],
}

const fn place(piece: &'static str, q: i32, r: i32, rot: u8) -> Placement {
    Placement { piece, cell: Axial { q, r }, rot, offset: [0.0, 0.0] }
}

/// Two road-straight tiles meeting at (0,0) edge 0 / (1,0) edge 3, ringed by meadow at every
/// rotation (Review Focus 1), and the hero tree on (0,1).
pub fn kit_layout() -> Vec<Placement> {
    let mut v = vec![];
    for (q, r) in [(0, 0), (1, 0)] {
        v.push(place("meadow_base", q, r, 0));
        v.push(place("road_straight_center", q, r, 0));
        v.push(place("road_straight_edge", q, r, 0));
        v.push(place("road_straight_edge", q, r, 3));
    }
    for (k, (q, r)) in [(0, 1), (1, -1), (-1, 0), (0, -1), (-1, 1), (2, -1)].into_iter().enumerate() {
        v.push(place("meadow_base", q, r, k as u8));
    }
    v.push(Placement { offset: [3.0, -2.0], ..place("tree_hero_oak", 0, 1, 0) });
    v
}

pub fn piece_rotation(rot: u8) -> Quat {
    // Edges run clockwise viewed from +Y: +x -> +z is a negative rotation about +Y.
    Quat::from_rotation_y(-(60.0 * (rot % 6) as f32).to_radians())
}

pub fn resolve_placements(m: &KitManifest, layout: &[Placement]) -> (Vec<Placement>, Vec<String>) {
    let mut warn = vec![];
    let ok = layout
        .iter()
        .filter(|p| {
            let found = m.pieces.iter().any(|e| e.id == p.piece);
            if !found && !warn.iter().any(|w: &String| w.contains(p.piece)) {
                warn.push(format!("kit: piece {} not in manifest, skipped", p.piece));
            }
            found
        })
        .copied()
        .collect();
    (ok, warn)
}

#[derive(Resource)]
pub struct KitPending;

#[derive(Resource)]
pub struct KitLoading {
    items: Vec<(Placement, Handle<Gltf>)>,
    family: TextureFamily,
}

pub fn start_kit(
    mut commands: Commands,
    pending: Option<Res<KitPending>>,
    device: Option<Res<RenderDevice>>,
    server: Res<AssetServer>,
) {
    let (Some(_), Some(device)) = (pending, device) else {
        return;
    };
    commands.remove_resource::<KitPending>();
    let f = device.features();
    let family = select_family(
        f.contains(WgpuFeatures::TEXTURE_COMPRESSION_BC),
        f.contains(WgpuFeatures::TEXTURE_COMPRESSION_ASTC),
    );
    let manifest = match parse_manifest(MANIFEST) {
        Ok(m) => m,
        Err(e) => {
            warn!("kit: {e}");
            return;
        }
    };
    let (placements, warnings) = resolve_placements(&manifest, &kit_layout());
    for w in warnings {
        warn!("{w}");
    }
    let items = placements
        .into_iter()
        .map(|p| {
            let glb = &manifest.pieces.iter().find(|e| e.id == p.piece).unwrap().glb;
            (p, server.load(format!("kit://{glb}")))
        })
        .collect();
    info!("kit: texture family {family:?}");
    commands.insert_resource(KitLoading { items, family });
}

pub fn finish_kit(
    mut commands: Commands,
    loading: Option<Res<KitLoading>>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    mut materials: ResMut<Assets<KitMaterial>>,
    config: Res<GateConfig>,
) {
    let Some(loading) = loading else {
        return;
    };
    if loading.items.iter().any(|(_, h)| !server.is_loaded_with_dependencies(h) && !server.load_state(h).is_failed()) {
        return;
    }
    commands.remove_resource::<KitLoading>();
    let manifest = parse_manifest(MANIFEST).expect("parsed in start_kit");
    let mut cache: HashMap<(String, String), Handle<KitMaterial>> = HashMap::new();
    for (p, h) in &loading.items {
        let Some(g) = gltfs.get(h) else {
            warn!("kit: {} failed to load, skipped", p.piece);
            continue;
        };
        let entry = manifest.pieces.iter().find(|e| e.id == p.piece).unwrap();
        let [x, z] = p.cell.to_world(HEX_RADIUS);
        let rot = piece_rotation(p.rot);
        let pos = Vec3::new(x, 0.0, z) + rot * Vec3::new(p.offset[0], 0.0, p.offset[1]);
        for lod in 0..3usize {
            let Some(mh) = g.named_meshes.get(format!("{}_lod{lod}", p.piece).as_str()) else {
                warn!("kit: {} has no {}_lod{lod}", p.piece, p.piece);
                continue;
            };
            let Some(gm) = gltf_meshes.get(mh) else { continue };
            for prim in &gm.primitives {
                let slot_name = prim
                    .material
                    .as_ref()
                    .and_then(|mh| g.named_materials.iter().find(|(_, v)| *v == mh).map(|(k, _)| k.to_string()))
                    .unwrap_or_default();
                let Some(slot) = entry.slots.get(&slot_name) else {
                    warn!("kit: {} material {slot_name} has no manifest slot", p.piece);
                    continue;
                };
                let mat = cache
                    .entry((p.piece.to_string(), slot_name.clone()))
                    .or_insert_with(|| {
                        let r = slot_requests(&manifest, entry, slot, loading.family);
                        materials.add(build_material(&server, slot, &r, config.kit_hooks))
                    })
                    .clone();
                commands.spawn((
                    Mesh3d(prim.mesh.clone()),
                    MeshMaterial3d(mat),
                    Transform::from_translation(pos).with_rotation(rot),
                    lod_range(lod),
                    SceneEntity,
                ));
            }
        }
    }
}
```

If `server.load_state(h).is_failed()` is not the 0.20 spelling, use `matches!(server.get_load_state(h.id()), Some(LoadState::Failed(_)))`, the form `kit_gate.rs` uses. `GltfPrimitive` field names (`mesh`, `material`) follow bevy_gltf 0.20 `assets.rs`. Check there if they don't compile.

Wiring:
- **`lib.rs`.**
  - Add the `Scenario::Kit` variant. `parse("kit") => Some(Scenario::Kit)`, and `tile_count(Kit) => 8` (the distinct cells in `kit_layout`).
  - Register `MaterialPlugin::<kit_material::KitMaterial>::default()` in `HexRenderPlugin::build`.
  - Add `(kit_scene::start_kit, kit_scene::finish_kit)` to `Update`.
  - Extend `parses_known_and_rejects_unknown` with `assert_eq!(Scenario::parse("kit"), Some(Scenario::Kit));`.
  - Extend `tile_counts` with:

    ```rust
    let mut cells: Vec<_> = kit_scene::kit_layout().iter().map(|p| (p.cell.q, p.cell.r)).collect();
    cells.sort();
    cells.dedup();
    assert_eq!(Scenario::Kit.tile_count(), cells.len());
    ```
- **`scene.rs`.**
  - Make `fn lod_range` `pub(crate)`.
  - In `setup`, add the arm below. Add `commands.remove_resource::<crate::kit_scene::KitPending>(); commands.remove_resource::<crate::kit_scene::KitLoading>();` to the `Single` and `Gate2000` arms.

  ```rust
  Scenario::Kit => {
      commands.remove_resource::<crate::instanced::InstancedScene>();
      commands.remove_resource::<crate::kit_test::KitTestPending>();
      commands.insert_resource(crate::kit_scene::KitPending);
      Transform::from_xyz(14.0, 38.0, 62.0).looking_at(Vec3::new(14.0, 0.0, 0.0), Vec3::Y)
  }
  ```

- **`workload.rs`.** Add the `kit_hooks: KitHooks` field to `GateConfig` (default `KitHooks::None`) and the enum below. Re-export `KitHooks` from `lib.rs`.

  ```rust
  /// `kit_hooks=`: drives the §9.4 hook uniforms for verification shots (default none = noon).
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
  pub enum KitHooks {
      #[default]
      None,
      Snow,
      Wet,
      Night,
  }

  impl KitHooks {
      pub fn parse(s: &str) -> Option<KitHooks> {
          match s {
              "none" => Some(KitHooks::None),
              "snow" => Some(KitHooks::Snow),
              "wet" => Some(KitHooks::Wet),
              "night" => Some(KitHooks::Night),
              _ => None,
          }
      }
  }
  ```

- **`args.rs`.** Add `"kit_hooks" => set_or_warn(&mut f.gate.kit_hooks, KitHooks::parse(v), k, v, &mut warnings),` to `flags_from_pairs`, and `"kit_hooks"` to the `web_pairs` key list. Add tests:

  ```rust
  #[test]
  fn kit_scenario_and_hooks() {
      let (f, w) = flags_from_pairs(&p(&[("shot", "kit"), ("kit_hooks", "snow")]));
      assert_eq!(f.scenario, Scenario::Kit);
      assert_eq!(f.gate.kit_hooks, hex_render::KitHooks::Snow);
      assert!(w.is_empty());
      let (f, w) = flags_from_pairs(&p(&[("kit_hooks", "hail")]));
      assert_eq!(f.gate.kit_hooks, hex_render::KitHooks::None);
      assert_eq!(w.len(), 1);
  }
  ```

  Fix any exhaustive `match` on `Scenario` elsewhere (in `hex-app` `control.rs` or `main.rs`) by adding the `Kit` arm with the same behavior as `Single`.

Run: `cargo test --workspace`
Expected: PASS, including `kit_material::tests` (3), `kit_scene::tests` (3) and the new args test.

- [ ] **Step 5: Shot script and make targets**

Create `scripts/shots/kit.json`. The seam sits at x = 13.856 on z = 0, between cells (0,0) and (1,0).

```json
[
  { "cmd": "scenario", "name": "kit" },
  { "cmd": "wait", "frames": 120 },
  { "cmd": "camera", "pos": [14, 38, 62], "look_at": [14, 0, 0] },
  { "cmd": "wait", "frames": 20 },
  { "cmd": "screenshot", "name": "kit-overview" },
  { "cmd": "camera", "pos": [8.5, 2.2, 5.5], "look_at": [13.86, 0.1, 0] },
  { "cmd": "wait", "frames": 20 },
  { "cmd": "screenshot", "name": "kit-seam-closeup" },
  { "cmd": "camera", "pos": [6, 7, 30], "look_at": [3, 5, 22] },
  { "cmd": "wait", "frames": 20 },
  { "cmd": "screenshot", "name": "kit-tree" },
  { "cmd": "camera", "pos": [14, 160, 160], "look_at": [14, 0, 0] },
  { "cmd": "wait", "frames": 20 },
  { "cmd": "screenshot", "name": "kit-lod1" },
  { "cmd": "camera", "pos": [14, 330, 330], "look_at": [14, 0, 0] },
  { "cmd": "wait", "frames": 20 },
  { "cmd": "screenshot", "name": "kit-lod2" },
  { "cmd": "dump_state", "name": "kit-state" },
  { "cmd": "exit" }
]
```

The LOD cameras sit about 226 m and 467 m from the tiles, inside the LOD1 (150–400 m) and LOD2 (> 400 m) bands. The tree camera looks at cell (0,1), whose center is (13.86, 0, 24) offset by the placement. If the tree is off-frame, aim at the tree's world position printed in `kit-state.json`. Do not move the placement.

Add to `Makefile` (and `.PHONY`):

```make
KIT_SHOTS = kit-overview kit-seam-closeup kit-tree kit-lod1 kit-lod2
shots-kit:
	mkdir -p target/shots
	rm -f target/shots/kit-*.png
	cargo run -p hex-app --release -- --script scripts/shots/kit.json --size 1600x1000 --seed 1 --fps off $(ARGS)
	@for n in $(KIT_SHOTS); do test -s target/shots/$$n.png || { echo "missing $$n.png"; exit 1; }; done
	@echo "wrote target/shots/kit-*.png"

web-shots-kit: build-web
	cd e2e && npm ci --no-audit --no-fund
	mkdir -p e2e/test-results
	node e2e/shots.mjs dist scripts/shots/kit.json
```

- [ ] **Step 6: Run the scenario before any piece exists**

Run: `make shots-kit > target/shots-kit.log 2>&1; ec=$?; grep -c "not in manifest, skipped" target/shots-kit.log; echo "EXIT=$ec"`
Expected: `EXIT=0`, the five PNGs exist (empty ground and sky), and the log warns once per missing piece id (4) with no panic.

Then run: `make web-shots-kit`
Expected: `web-shots OK`, with no console errors. A WebGPU "too many sampled textures" validation error here means the extension's four textures exceed the stage limit. If that happens, stop and report it with the exact message. The fix (merging `tile_orm` into `tile_normal.ba`, or into StandardMaterial bindings) changes the R6 file layout and needs a controller decision.

- [ ] **Step 7: Docs, gate, commit**

In `docs/agent-operability.md`, add these rows:
- `kit` to the `--scenario` row: "the P1 kit family from `assets/kit/manifest.json`; missing pieces warn and are skipped".
- A `--kit_hooks <none|snow|wet|night>` row.
- The `make shots-kit` and `make web-shots-kit` targets.

Add `kit_hooks` to the web query parameter list.

Run: `make checkall`
Expected: PASS.

```bash
git add crates/hex-render crates/hex-app assets/shaders/kit_material.wesl scripts/shots/kit.json Makefile docs/agent-operability.md
git commit -m "feat(render): kit material (world-space tileable + masks hooks), manifest-driven kit scenario with VisibilityRange LODs"
```

---

### Task 7: Look-dev: tone mapping, IBL and texel density (owner checkpoint)

Load `.claude/skills/blender-asset/SKILL.md` first (R4). The IBL bake runs in Blender.

**Files:**
- Create: `scripts/blender/lib/ibl.py`, `scripts/blender/lib/ktx2.py`, `scripts/blender/tests/test_ibl.py`, `scripts/blender/lookdev_ibl.py`, `crates/hex-render/src/lookdev.rs`, `assets-source/lookdev/lookdev.json`, `assets/kit/p1/ibl/noon.{specular,diffuse}.ktx2`
- Modify: `crates/hex-render/src/{lib.rs,scene.rs,workload.rs}`, `crates/hex-app/src/args.rs`, `crates/hex-render/tests/kit_gate.rs` (IBL file check), `Makefile` (`lookdev-ibl`, `shots-lookdev`), `docs/agent-operability.md`

**Interfaces:**
- Produces:
  - `Tonemap { AgX, TonyMcMapface, AcesFitted }`, parsed from `tonemap=agx|tony|aces` (default `AgX`).
  - `Ibl { On, Off }`, parsed from `ibl=on|off` (default `On`).
  - `lookdev::camera_lookdev(config) -> impl Bundle`, which adds `Tonemapping` and, when IBL is on, `EnvironmentMapLight` from `kit://p1/ibl/noon.{diffuse,specular}.ktx2`.
  - `IBL_INTENSITY: f32`, chosen at the checkpoint.
  - `ktx2.write_cubemap_rgba16f(path, levels: list[np.ndarray (6, n, n, 4) float32])`.
  - `ibl.prefilter(equirect, base=256, samples=128) -> list[levels]`.
  - `ibl.irradiance(equirect, size=32) -> list[one level]`.

- [ ] **Step 1: Choose and fetch the HDRI (D5)**

Fetch one CC0 noon sky from Poly Haven into the gitignored `assets-source/hdri/`:

```bash
mkdir -p assets-source/hdri
curl -fL --max-time 300 -o assets-source/hdri/kloofendal_48d_partly_cloudy_puresky_2k.hdr \
  https://dl.polyhaven.org/file/ph-assets/HDRIs/hdr/2k/kloofendal_48d_partly_cloudy_puresky_2k.hdr
shasum -a 256 assets-source/hdri/kloofendal_48d_partly_cloudy_puresky_2k.hdr
```

Expected: a file of about 6 MB, plus its SHA-256. Record the URL, the CC0 license (https://polyhaven.com/license) and the SHA in `assets-source/lookdev/lookdev.json` under `"hdri"`. If that asset is gone, pick another Poly Haven CC0 "puresky" with a high sun, and record it.

- [ ] **Step 2: Write the failing IBL test**

Create `scripts/blender/tests/test_ibl.py`:

```python
import pathlib
import struct
import tempfile
import unittest

import numpy as np

from lib import ibl, ktx2


class Ibl(unittest.TestCase):
    def test_face_directions_follow_bevy_lookup(self):
        # Bevy samples the cube at (w.x, w.y, -w.z) (environment_map.wesl), so the +X face center
        # holds world +X and the +Z face center holds world -Z.
        d = ibl.face_dirs(0, 1)[0, 0]
        self.assertTrue(np.allclose(d, [1, 0, 0]))
        w = ibl.cube_to_world(ibl.face_dirs(4, 1)[0, 0])
        self.assertTrue(np.allclose(w, [0, 0, -1]))

    def test_equirect_follows_cycles_mapping(self):
        # Cycles: u = 0.5 - atan2(y_b, x_b) / 2pi; runtime = (x_b, z_b, -y_b).
        eq = np.zeros((8, 16, 3), np.float32)
        eq[4, 8] = 1.0  # u = 0.5, v = 0.5: Blender +X = runtime +X
        self.assertEqual(float(ibl.sample_equirect(eq, np.array([1.0, 0.0, 0.0]))[0]), 1.0)
        eq = np.zeros((8, 16, 3), np.float32)
        eq[4, 4] = 1.0  # u = 0.25: Blender +Y = runtime -Z
        self.assertEqual(float(ibl.sample_equirect(eq, np.array([0.0, 0.0, -1.0]))[0]), 1.0)
        self.assertEqual(float(ibl.sample_equirect(eq, np.array([0.0, 0.0, 1.0]))[0]), 0.0)

    def test_blender_plus_x_half_lights_runtime_plus_x_face(self):
        eq = np.zeros((16, 32, 3), np.float32)
        eq[:, 8:24] = 1.0  # u in [0.25, 0.75): the Blender +X hemisphere
        irr = ibl.irradiance(eq, size=4)[0]
        self.assertGreater(irr[0, ..., :3].mean(), irr[1, ..., :3].mean() + 0.1)
        spec = ibl.prefilter(eq, base=4, samples=8)[0]
        self.assertGreater(spec[0, ..., :3].mean(), spec[1, ..., :3].mean() + 0.5)

    def test_constant_sky_prefilters_to_constant(self):
        eq = np.full((32, 64, 3), 2.0, np.float32)
        levels = ibl.prefilter(eq, base=16, samples=16)
        self.assertEqual(len(levels), 5)  # 16 -> 1
        for lv in levels:
            self.assertTrue(np.allclose(lv[..., :3], 2.0, atol=1e-3))
        irr = ibl.irradiance(eq, size=8)[0]
        self.assertTrue(np.allclose(irr[..., :3], 2.0, atol=2e-2))  # SH9 of a constant = constant

    def test_ktx2_writer_header(self):
        p = pathlib.Path(tempfile.mkdtemp()) / "c.ktx2"
        ktx2.write_cubemap_rgba16f(str(p), [np.zeros((6, 4, 4, 4), np.float32), np.zeros((6, 2, 2, 4), np.float32)])
        b = p.read_bytes()
        vk, _, w, h, _, layers, faces, levels, sc = struct.unpack_from("<9I", b, 12)
        self.assertEqual((vk, w, h, layers, faces, levels, sc), (97, 4, 4, 0, 6, 2, 0))

    def test_ktx2_writer_clamps_to_half_max(self):
        # The HDRI sun exceeds float16's 65504; an inf texel would poison the specular lookup.
        p = pathlib.Path(tempfile.mkdtemp()) / "c.ktx2"
        lv = np.full((6, 1, 1, 4), 1e6, np.float32)
        lv[0, 0, 0, 3] = np.nan
        lv[1, 0, 0, 3] = 1.0
        counts = ktx2.write_cubemap_rgba16f(str(p), [lv])
        self.assertEqual(counts, {"clamped": 22, "nonFinite": 1})
        b = p.read_bytes()
        off, length, _ = struct.unpack_from("<QQQ", b, 80)
        a = np.frombuffer(b[off:off + length], np.float16).reshape(6, 4)
        self.assertTrue(np.all(np.isfinite(a)))
        self.assertEqual(float(a[0, 3]), 0.0)  # NaN -> 0
        self.assertEqual(float(a[1, 3]), 1.0)
        self.assertEqual(float(a[2, 0]), 65504.0)
```

Run: `make blender-test TEST=test_ibl`
Expected: FAIL with `ImportError`.

- [ ] **Step 3: Write `ktx2.py` and `ibl.py`**

Create `scripts/blender/lib/ktx2.py`:

```python
"""Minimal KTX2 writer: uncompressed VK_FORMAT_R16G16B16A16_SFLOAT cubemaps (IBL, D5).
Layout per the KTX2 spec: header, level index, basic DFD, then level data smallest-first."""
import struct

import numpy as np

VK_R16G16B16A16_SFLOAT = 97
_ID = bytes([0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A])


def _dfd():
    samples = b""
    for i, ch in enumerate((0, 1, 2, 15)):  # R, G, B, A
        ctype = ch | 0x80 | 0x40  # FLOAT | SIGNED
        samples += struct.pack("<IIII", (i * 16) | (15 << 16) | (ctype << 24), 0, 0xBF800000, 0x3F800000)
    block = struct.pack("<IIIIII", 0, 2 | ((24 + len(samples)) << 16),
                        1 | (1 << 8) | (1 << 16),  # RGBSDA, BT709 primaries, linear transfer
                        0, 8, 0) + samples
    return struct.pack("<I", 4 + len(block)) + block


def write_cubemap_rgba16f(path, levels):
    """levels[i]: (6, n_i, n_i, 4) float array, faces +X,-X,+Y,-Y,+Z,-Z, rows top-down.
    Returns {"clamped", "nonFinite"} texel-channel counts: finite values beyond float16's 65504
    are clamped (the HDRI sun), NaN is written as 0 and +-inf as +-65504."""
    n0 = levels[0].shape[1]
    dfd = _dfd()
    index_off = 80
    dfd_off = index_off + 24 * len(levels)
    data_off = dfd_off + len(dfd)
    counts = {"clamped": 0, "nonFinite": 0}
    blobs = []
    for lv in levels:
        a = np.asarray(lv, np.float32)
        finite = np.isfinite(a)
        counts["clamped"] += int(np.count_nonzero(finite & (np.abs(a) > 65504)))
        counts["nonFinite"] += int(np.count_nonzero(~finite))
        a = np.clip(np.nan_to_num(a, nan=0.0, posinf=65504, neginf=-65504), -65504, 65504)
        blobs.append(np.ascontiguousarray(a, np.float16).tobytes())
    offsets = [0] * len(levels)
    cur = data_off
    for i in reversed(range(len(levels))):  # smallest level first in the file
        cur = (cur + 7) & ~7
        offsets[i] = cur
        cur += len(blobs[i])
    head = _ID + struct.pack("<9I", VK_R16G16B16A16_SFLOAT, 2, n0, n0, 0, 0, 6, len(levels), 0)
    head += struct.pack("<IIIIQQ", dfd_off, len(dfd), 0, 0, 0, 0)
    assert len(head) == 80
    index = b"".join(struct.pack("<QQQ", offsets[i], len(blobs[i]), len(blobs[i])) for i in range(len(levels)))
    out = bytearray(head + index + dfd)
    for i in reversed(range(len(levels))):
        out += b"\0" * (offsets[i] - len(out))
        out += blobs[i]
    with open(path, "wb") as f:
        f.write(out)
    return counts
```

Create `scripts/blender/lib/ibl.py`:

```python
"""Equirect HDRI -> GGX-prefiltered specular cubemap + SH9 irradiance cubemap for Bevy (D5).
Bevy samples cubemaps at (w.x, w.y, -w.z) for world direction w (environment_map.wesl), and
picks mip = perceptual_roughness * (levels - 1)."""
import math

import numpy as np

# wgpu/Vulkan cube face basis for face-local (u, v) in [-1, 1], v down (rows top-down).
_FACES = [
    lambda u, v: np.stack([np.ones_like(u), -v, -u], -1),   # +X
    lambda u, v: np.stack([-np.ones_like(u), -v, u], -1),   # -X
    lambda u, v: np.stack([u, np.ones_like(u), v], -1),     # +Y
    lambda u, v: np.stack([u, -np.ones_like(u), -v], -1),   # -Y
    lambda u, v: np.stack([u, -v, np.ones_like(u)], -1),    # +Z
    lambda u, v: np.stack([-u, -v, -np.ones_like(u)], -1),  # -Z
]


def face_dirs(face, n):
    c = (np.arange(n) + 0.5) / n * 2 - 1
    u, v = np.meshgrid(c, c)
    d = _FACES[face](u, v)
    return d / np.linalg.norm(d, axis=-1, keepdims=True)


def cube_to_world(d):
    return d * np.array([1.0, 1.0, -1.0])


def load_equirect(path):
    import bpy
    img = bpy.data.images.load(str(path))
    w, h = img.size
    a = np.empty(w * h * 4, np.float32)
    img.pixels.foreach_get(a)
    return a.reshape(h, w, 4)[::-1, :, :3].copy()  # Blender rows are bottom-up


def sample_equirect(eq, w):
    """Runtime world dir (Y-up) -> Blender dir (x, -z, y) -> Blender's equirect mapping
    (Cycles projection.h: u = 0.5 - atan2(y, x) / 2pi)."""
    bx, by, bz = w[..., 0], -w[..., 2], w[..., 1]
    u = np.arctan2(-by, bx) / (2 * math.pi) + 0.5
    v = np.arctan2(bz, np.hypot(bx, by)) / math.pi + 0.5
    h, wd = eq.shape[:2]
    x = np.clip((u * wd).astype(int) % wd, 0, wd - 1)
    y = np.clip(((1 - v) * h).astype(int), 0, h - 1)
    return eq[y, x]


def _downsample(eq):
    h, w = eq.shape[:2]
    return eq[: h // 2 * 2, : w // 2 * 2].reshape(h // 2, 2, w // 2, 2, 3).mean(axis=(1, 3))


def prefilter(eq, base=256, samples=128):
    levels_n = int(math.log2(base)) + 1
    pyramid = [eq]
    while min(pyramid[-1].shape[:2]) > 8:
        pyramid.append(_downsample(pyramid[-1]))
    i = np.arange(samples)
    xi1 = (i + 0.5) / samples
    xi2 = np.array([int(f"{k:032b}"[::-1], 2) / 2**32 for k in i])  # Hammersley
    out = []
    for m in range(levels_n):
        n = base >> m
        rough = m / (levels_n - 1)
        a = max(rough * rough, 1e-4)
        faces = []
        for f in range(6):
            N = cube_to_world(face_dirs(f, n))
            if m == 0:
                faces.append(sample_equirect(eq, N))
                continue
            up = np.where(np.abs(N[..., 1:2]) < 0.999, [[0, 1, 0]], [[1, 0, 0]])
            T = np.cross(up, N)
            T /= np.linalg.norm(T, axis=-1, keepdims=True)
            B = np.cross(N, T)
            acc = np.zeros(N.shape[:2] + (3,))
            wsum = np.zeros(N.shape[:2] + (1,))
            phi = 2 * math.pi * xi1
            cos_t = np.sqrt((1 - xi2) / (1 + (a * a - 1) * xi2))
            sin_t = np.sqrt(1 - cos_t * cos_t)
            src = pyramid[min(len(pyramid) - 1, int(rough * (len(pyramid) - 1) + 0.5))]
            for s in range(samples):
                Hl = np.array([sin_t[s] * math.cos(phi[s]), sin_t[s] * math.sin(phi[s]), cos_t[s]])
                H = T * Hl[0] + B * Hl[1] + N * Hl[2]
                L = 2 * np.sum(N * H, -1, keepdims=True) * H - N
                nl = np.sum(N * L, -1, keepdims=True)
                wgt = np.clip(nl, 0, None)
                acc += sample_equirect(src, L) * wgt
                wsum += wgt
            faces.append(acc / np.maximum(wsum, 1e-6))
        rgba = np.concatenate([np.stack(faces), np.ones((6, n, n, 1))], -1)
        out.append(rgba.astype(np.float32))
    return out


def _sh9(d):
    x, y, z = d[..., 0], d[..., 1], d[..., 2]
    return np.stack([0.282095 * np.ones_like(x), 0.488603 * y, 0.488603 * z, 0.488603 * x,
                     1.092548 * x * y, 1.092548 * y * z, 0.315392 * (3 * z * z - 1),
                     1.092548 * x * z, 0.546274 * (x * x - y * y)], -1)


def irradiance(eq, size=32):
    h, w = eq.shape[:2]
    th = (np.arange(h) + 0.5) / h * math.pi
    ph = (np.arange(w) + 0.5) / w * 2 * math.pi
    P, T = np.meshgrid(ph, th)
    u, v = P / (2 * math.pi), 1 - T / math.pi
    bx, by, bz = np.cos((u - 0.5) * 2 * math.pi) * np.cos((v - 0.5) * math.pi), \
        -np.sin((u - 0.5) * 2 * math.pi) * np.cos((v - 0.5) * math.pi), np.sin((v - 0.5) * math.pi)
    wdir = np.stack([bx, bz, -by], -1)  # Blender -> runtime
    dA = (np.sin(T) * (math.pi / h) * (2 * math.pi / w))[..., None]
    coeffs = np.einsum("hwk,hwc->kc", _sh9(wdir) * dA, eq)
    band = np.array([math.pi, 2 * math.pi / 3, 2 * math.pi / 3, 2 * math.pi / 3, math.pi / 4, math.pi / 4,
                     math.pi / 4, math.pi / 4, math.pi / 4])
    faces = []
    for f in range(6):
        N = cube_to_world(face_dirs(f, size))
        faces.append(np.einsum("hwk,kc->hwc", _sh9(N) * band, coeffs) / math.pi)
    rgba = np.concatenate([np.stack(faces), np.ones((6, size, size, 1))], -1)
    return [np.clip(rgba, 0, None).astype(np.float32)]
```

The irradiance is radiance-convolved (cosine lobe divided by π), which is what Bevy's diffuse map expects.

Run: `make blender-test TEST=test_ibl`
Expected: PASS (6 tests).

- [ ] **Step 4: Bake the noon IBL**

Create `scripts/blender/lookdev_ibl.py`:

```python
"""make lookdev-ibl: HDRI -> assets/kit/p1/ibl/noon.{specular,diffuse}.ktx2 (+ hashes to lookdev.json)."""
import json
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from lib import bake, ibl, ktx2  # noqa: E402

hdri = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
out = pathlib.Path("assets/kit/p1/ibl")
out.mkdir(parents=True, exist_ok=True)
eq = ibl.load_equirect(hdri)
counts = {
    "noon.specular.ktx2": ktx2.write_cubemap_rgba16f(str(out / "noon.specular.ktx2"),
                                                     ibl.prefilter(eq, base=256, samples=128)),
    "noon.diffuse.ktx2": ktx2.write_cubemap_rgba16f(str(out / "noon.diffuse.ktx2"), ibl.irradiance(eq, size=32)),
}
for name, c in counts.items():
    print(f"lookdev-ibl: {name}: {c['clamped']} channel values clamped to 65504, {c['nonFinite']} non-finite")
meta = pathlib.Path("assets-source/lookdev/lookdev.json")
doc = json.loads(meta.read_text()) if meta.exists() else {}
doc["ibl"] = {"source": str(hdri), "sourceSha256": bake.sha256_file(hdri), "specularBase": 256, "samples": 128,
              "files": {p.name: bake.sha256_file(p) for p in sorted(out.glob("*.ktx2"))},
              "float16": counts}
meta.parent.mkdir(parents=True, exist_ok=True)
meta.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
```

Add `lookdev-ibl` to `Makefile` and `.PHONY`:

```make
lookdev-ibl:
	$(BLENDER_RUN) -P scripts/blender/lookdev_ibl.py -- assets-source/hdri/kloofendal_48d_partly_cloudy_puresky_2k.hdr
	. tools/ktx-env.sh && ktx validate assets/kit/p1/ibl/noon.specular.ktx2 && ktx validate assets/kit/p1/ibl/noon.diffuse.ktx2
```

Run: `make lookdev-ibl`
Expected: two files. Specular is about 4.2 MB (RGBA16F, 256² × 6 × 9 levels) and diffuse about 49 KB. `ktx validate` must report no errors. If it flags the DFD, fix `ktx2._dfd` against the message. This 4.2 MB counts toward the 30 MB initial download, and Task 11 records it.

- [ ] **Step 5: Engine side: tone mapping and IBL keys**

In `workload.rs`, add the enums below plus the fields `tonemap: Tonemap` and `ibl: bool` (default `true`) on `GateConfig`, with `parse_ibl` accepting `on|off` like `parse_shadows`:

```rust
/// D4 candidates; AgX is the default unless saturated greens clip (owner decides in Task 7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tonemap {
    #[default]
    AgX,
    TonyMcMapface,
    AcesFitted,
}

impl Tonemap {
    pub fn parse(s: &str) -> Option<Tonemap> {
        match s {
            "agx" => Some(Tonemap::AgX),
            "tony" => Some(Tonemap::TonyMcMapface),
            "aces" => Some(Tonemap::AcesFitted),
            _ => None,
        }
    }
}
```

Create `crates/hex-render/src/lookdev.rs`:

```rust
//! Look-dev camera components: tone mapping (D4) and the noon IBL (D5).
use bevy::{core_pipeline::tonemapping::Tonemapping, light::EnvironmentMapLight, prelude::*};

use crate::workload::{GateConfig, Tonemap};

/// cd/m² scale for the prefiltered HDRI; set at the Task 7 owner checkpoint.
pub const IBL_INTENSITY: f32 = 1500.0;

pub fn tonemapping(t: Tonemap) -> Tonemapping {
    match t {
        Tonemap::AgX => Tonemapping::AgX,
        Tonemap::TonyMcMapface => Tonemapping::TonyMcMapface,
        Tonemap::AcesFitted => Tonemapping::AcesFitted,
    }
}

pub fn environment(server: &AssetServer) -> EnvironmentMapLight {
    EnvironmentMapLight {
        diffuse_map: server.load("kit://p1/ibl/noon.diffuse.ktx2"),
        specular_map: server.load("kit://p1/ibl/noon.specular.ktx2"),
        intensity: IBL_INTENSITY,
        ..default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_every_tonemap() {
        assert_eq!(tonemapping(Tonemap::AgX), Tonemapping::AgX);
        assert_eq!(tonemapping(Tonemap::TonyMcMapface), Tonemapping::TonyMcMapface);
        assert_eq!(tonemapping(Tonemap::AcesFitted), Tonemapping::AcesFitted);
        assert_eq!(GateConfig::default().tonemap, Tonemap::AgX);
    }
}
```

Re-export `Tonemap` and `parse_ibl` from `hex-render/src/lib.rs` beside the existing `workload` re-exports, and add `pub mod lookdev;`.

In `scene.rs` `setup`, add `server: Res<AssetServer>`. After spawning the camera:

```rust
// Look-dev applies to the kit scenario only: single/gate2000 keep the P0 camera, so the P0 perf
// baseline and the instanced pipeline's mesh-view bind-group layout are unchanged.
let kit = scenario.0 == Scenario::Kit;
if kit {
    cam.insert(crate::lookdev::tonemapping(config.tonemap));
    if config.ibl {
        cam.insert(crate::lookdev::environment(&server));
    }
}
```

Then make the existing `GlobalAmbientLight` brightness `if kit && config.ibl { 0.0 } else { 400.0 }`, because the IBL replaces the flat ambient in the kit scenario. Run `make shots && make shots-instanced && make web-smoke` once after this step. They must pass unchanged, which proves the P0 scenarios are untouched. Extending look-dev to the gate scenarios is a P2 decision, with a perf re-run under the load guard (R9).

In `args.rs`, add the `"tonemap"` and `"ibl"` keys to `flags_from_pairs` (with `set_or_warn`) and to `web_pairs`, plus a test:

```rust
#[test]
fn tonemap_and_ibl_keys() {
    let (f, w) = flags_from_pairs(&p(&[("tonemap", "aces"), ("ibl", "off")]));
    assert_eq!(f.gate.tonemap, hex_render::Tonemap::AcesFitted);
    assert!(!f.gate.ibl && w.is_empty());
}
```

In `kit_gate.rs`, add an IBL file check:

```rust
#[test]
fn ibl_cubemaps_are_valid() {
    let meta: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo().join("assets-source/lookdev/lookdev.json")).unwrap(),
    )
    .unwrap();
    for (name, size) in [("noon.specular.ktx2", 256), ("noon.diffuse.ktx2", 32)] {
        let p = repo().join("assets/kit/p1/ibl").join(name);
        let b = std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        let i = ktx2_info(&b).expect("KTX2");
        assert_eq!(
            (i.vk_format, i.faces, i.width, i.height, i.supercompression),
            (VK_R16G16B16A16_SFLOAT, 6, size, size, 0),
            "{name}"
        );
        assert_eq!(
            sha(&p).as_deref(),
            meta["ibl"]["files"][name].as_str(),
            "{name}: SHA-256 differs from assets-source/lookdev/lookdev.json (re-run make lookdev-ibl)"
        );
        assert_eq!(
            i.levels,
            if name.contains("specular") {
                full_mip_count(i.width, i.height)
            } else {
                1
            }
        );
    }
}
```

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 6: Comparison shots under the load guard**

Shots are not timing runs, but run them on a quiet host so a starved frame doesn't bias exposure. Check `node -e 'console.log(require("os").loadavg()[0])'` ≤ 15 first (R9's threshold).

Add to `Makefile` and `.PHONY`:

```make
shots-lookdev:
	@for t in agx tony aces; do \
	  rm -f target/shots/kit-*.png; \
	  cargo run -p hex-app --release -- --script scripts/shots/kit.json --size 1600x1000 --seed 1 --fps off --tonemap $$t || exit 1; \
	  for n in kit-overview kit-seam-closeup kit-tree; do cp target/shots/$$n.png target/shots/lookdev-$$t-$$n.png; done; \
	done
	@echo "wrote target/shots/lookdev-{agx,tony,aces}-*.png"
```

With no kit pieces committed yet, these shots show only the ground and lighting. In this task, only check that `make shots-lookdev` produces nine files with no panic. **Task 8 Step 8 runs the comparison and the checkpoint below**, once the meadow base exists to judge greens on.

- [ ] **Step 7: Look-dev checkpoint content (executed in Task 8 Step 8)**

Send the owner:
- the nine `target/shots/lookdev-{agx,tony,aces}-{kit-overview,kit-seam-closeup,kit-tree}.png` paths, with the note "AgX is the default unless saturated greens clip (D4)";
- the HDRI name and license, and the specular file size;
- the measured texel density from Task 8's provenance `texelDensityPxPerM`, against D2: 512 px/m tileable, 32 px/m unique masks, ≥ 256 px/m median for the tree;
- the `masks.A = AO` channel assignment, for explicit approval (a design choice, not a ruling).

Record the answers in `assets-source/lookdev/lookdev.json` as `"tonemap"`, `"iblIntensity"`, `"texelDensity"`, `"masksAlpha"` and `"approvals": [{"who", "when", "gate": "look-dev"}]`. If the owner picks Tony or ACES, change `Tonemap`'s `#[default]`. If they set a new intensity, change `IBL_INTENSITY`.

**Task 9 does not start until this look-dev checkpoint is approved.** Commit Task 7 now with AgX and `IBL_INTENSITY = 1500.0`. Task 8 Step 8 records the owner's answer and adjusts the defaults.

- [ ] **Step 8: Docs, gate, commit**

Add `--tonemap <agx|tony|aces>` and `--ibl <on|off>` rows, and the `make lookdev-ibl` and `make shots-lookdev` targets, to `docs/agent-operability.md`.

Run: `make checkall`
Expected: PASS.

```bash
git add scripts/blender crates/hex-render crates/hex-app assets/kit/p1/ibl assets-source/lookdev Makefile docs/agent-operability.md
git commit -m "feat(render): look-dev tone mapping keys and prefiltered noon IBL (owner-approved)"
```

---

### Task 8: Meadow base, scripted, through every gate (plus the look-dev checkpoint)

**Authoring task.** The implementer is an Opus 5.5 subagent that loads `.claude/skills/blender-asset/SKILL.md` first (R3, R4) and owns only `meadow_base`, `grass` and `soil`. This plan fixes the interfaces, dimensions, budgets, gates and acceptance checks. The look of the surfaces is authored against the approved references in `assets-source/refs/meadow_base/`.

**Files:**
- Create: `scripts/blender/kit/meadow_base.py`, `scripts/blender/kit/mat_grass.py`, `scripts/blender/kit/mat_soil.py`
- Create (generated): `assets/kit/p1/meadow_base/*`, `assets/kit/p1/materials/{grass,soil}/*`, `assets-source/kit/meadow_base/{provenance.json,bake-report.json,export-report.json,compress-report.json,renders/*.png,friction.md,approvals.json}`, `assets-source/kit/materials/{grass,soil}/{compress-report.json,bake-report.json}`
- Modify: `assets/kit/manifest.json`, `assets-source/lookdev/lookdev.json`

**Interfaces:**
- Consumes: Task 3's lib (all of it), Task 4's `make kit-build|kit-compress|kit-material|kit-determinism`, Task 5's `make kit-renders|kit-provenance|kit-check`, Task 6's `make shots-kit`, and Task 7's `make shots-lookdev`.
- Produces:
  - Node names: `meadow_base` (root), `meadow_base_lod{0,1,2}`, `meadow_base_collider`, `anchor-prop-{0..5}`. Each anchor carries extras `{"kind": "prop", "radius": <m>}`.
  - Material slots: `meadow_base_cap` (`{"tileable": "grass", "masks": true}`) and `meadow_base_skirt` (`{"tileable": "soil"}`).
  - Maps: `p1/meadow_base/meadow_base.masks`.
  - Tileable sets: `grass` and `soil`, each `tileM: 2.0` (1024² over 2 m = 512 px/m, D2).
- Geometry contract. Task 9's road pieces rely on these numbers.
  - The cap border ring is `hexgeo.border_ring(16)` at z = 0 in every LOD.
  - Cap relief z ∈ [−0.25, 0.0], with a smoothstep falloff to exactly 0 (value and slope) within 2.0 m of the border. Relief never rises above 0, so a road bed at +0.08 m covers it.
  - The skirt runs from z = 0 to z = −4.0 (`LEVEL_STEP`).
  - The interior lattice spacing per LOD is 0.9 m, 1.8 m and none (border ring only). Skirt rows are 4, 2 and 1.
  - Skirt UVs collapse to (0.005, 0.005), outside the hex, so the cap's unique bake owns the atlas.

- [ ] **Step 1: Confirm the R2 reference approval**

Run: `cat assets-source/refs/meadow_base/approval.json`
Expected: an `approved` list that is not empty. If it is missing, STOP: Task 2's checkpoint has not cleared.

- [ ] **Step 2: Write the tileable set generators**

Create `scripts/blender/kit/mat_grass.py`. `mat_soil.py` has the same skeleton with `SET = "soil"`, and its node graph authors horizontal strata (topsoil, clay, gravel) banded along V, because walls map texture-up to world +Y.

```python
"""Tileable 'grass' PBR set (D2: 1024² over 2 m). Scripted; seamless via lib.materials.torus_coords."""
import pathlib
import random
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from lib import bake, materials, scene  # noqa: E402

SET, SIZE, TILE_M = "grass", 1024, 2.0
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
scene.reset_scene(f"mat_{SET}")
random.seed(SET)
bake.setup_cycles(samples=32)

mat = materials.pbr_material(f"mat_{SET}", materials.srgb("#4f7d32"), 0.85)
nt = mat.node_tree
vec, w = materials.torus_coords(nt, scale=TILE_M)
# AUTHOR (against assets-source/refs/meadow_base/): build the grass look from 4D Noise/Voronoi
# nodes driven by (vec, w) only. Never use Object/Generated coords: they break the wrap.
# Required outputs, linked into the Principled BSDF:
#   Base Color: stylized-real meadow greens with dry-tip and clover variation, mean sRGB value
#     between 0.20 and 0.45, no baked lighting.
#   Roughness: 0.75-0.95.
#   Normal: a Bump node (strength ≤ 0.6) from a height field built from the same 4D textures.
bake.bake_tileable_set(mat, SIZE, out, SET)
bake.write_bake_report(str(out / "bake-report.json"), generator=__file__, generatorSha256=bake.sha256_file(__file__),
                       set=SET, size=SIZE, tileM=TILE_M, device="CPU", samples=32)
```

Run: `make kit-material SET=grass && make kit-material SET=soil`
Expected: each set's three maps encode under budget, and the console prints the byte counts.

**Acceptance (seamless).** Tile `assets/kit/p1/materials/grass/grass.albedo.png` 2×2 with sharp and inspect it for a visible cross:

```bash
node -e 'const s=require("sharp");const f="assets/kit/p1/materials/grass/grass.albedo.png";s(f).metadata().then(m=>s({create:{width:m.width*2,height:m.height*2,channels:3,background:"#000"}}).composite([0,1,2,3].map(i=>({input:f,left:(i%2)*m.width,top:(i>>1)*m.height}))).png().toFile("target/grass-2x2.png"))'
```

(Run it with `NODE_PATH=scripts/assets/node_modules`.) View `target/grass-2x2.png` with Read. Any visible seam line is a failure, and its cause is non-4D coordinates.

- [ ] **Step 3: Write the meadow base generator skeleton, then author it**

Create `scripts/blender/kit/meadow_base.py`:

```python
"""meadow_base: hex column (soil-strata skirt) + meadow terrain cap. Scripted route (PRD §11.1).
Geometry contract: plan Task 8 Interfaces. Border ring identical in every LOD; never decimated."""
import math
import os
import pathlib
import random
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from lib import bake, collider, export, geom, hexgeo, lod, materials, scene, uv  # noqa: E402

PIECE = "meadow_base"
SPACING = (0.9, 1.8, None)
SKIRT_ROWS = (4, 2, 1)
MASK_SIZE = 1024  # 32 px/m over the 32 m square (D2)
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
scene.reset_scene(PIECE)
random.seed(PIECE)


def falloff(x, y):
    t = min(max(hexgeo.dist_to_border(x, y) / 2.0, 0.0), 1.0)
    return t * t * (3 - 2 * t)


def relief(x, y):
    # AUTHOR: gentle meadow undulation in [-0.25, 0.0] m (contract: never above 0), deterministic
    # (no unseeded RNG; sum of sines or a seeded value-noise table). Must be multiplied by falloff.
    return -0.25 * falloff(x, y) * (0.5 + 0.5 * math.sin(0.31 * x + 1.7) * math.cos(0.27 * y - 0.4))


root = scene.empty(PIECE)
cap_mat = materials.pbr_material(f"{PIECE}_cap", materials.srgb("#4f7d32"), 0.9)
skirt_mat = materials.pbr_material(f"{PIECE}_skirt", materials.srgb("#6b4a2f"), 0.95)
ring = hexgeo.border_ring(hexgeo.BORDER_SUBDIV)
lods = []
for i, (spacing, rows) in enumerate(zip(SPACING, SKIRT_ROWS)):
    interior = hexgeo.lattice_points(spacing, spacing * 0.5) if spacing else []
    cap = geom.cdt_cap(f"{PIECE}_lod{i}_cap", ring, interior, relief)
    geom.set_border_normals_up(cap)
    uv.planar_cap_uv(cap)
    cap.data.materials.append(cap_mat)
    sk = geom.skirt(f"{PIECE}_lod{i}_skirt", ring, hexgeo.LEVEL_STEP, rows)
    sk.data.uv_layers.new(name="UVMap")
    for d in sk.data.uv_layers["UVMap"].data:
        d.uv = (0.005, 0.005)
    sk.data.materials.append(skirt_mat)
    body = geom.join([cap, sk], f"{PIECE}_lod{i}")
    lod.triangulate(body)
    body.parent = root
    lods.append(body)
lod.assert_border_identical(lods)
tris = [geom.tri_count(o) for o in lods]
assert tris[0] <= 4000 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris

col = collider.collider_from(lods[2], "hull", f"{PIECE}_collider")
col.parent = root
# AUTHOR: 6 prop anchors (seeded scatter rules hook, PRD §6) at least 2.5 m inside the border,
# spread around the cap; z = relief(x, y).
for n in range(6):
    a = math.radians(60 * n + 30)
    x, y = 7.5 * math.cos(a), 7.5 * math.sin(a)
    export.add_socket(f"anchor-prop-{n}", (x, y, relief(x, y)), root, kind="prop", radius=2.0)

texel = uv.measure_texel_density(lods[0], MASK_SIZE)
if not skip_bake:
    bake.setup_cycles(samples=64)
    ao = bake.new_image(f"{PIECE}.ao", MASK_SIZE, non_color=True)
    bake.bake_to(lods[0], "AO", ao, margin=12)
    ao_png = bake.save_png(ao, str(out / f"{PIECE}.ao.tmp.png"))
    # AUTHOR: snow capacity (R) and porosity (G) may vary per texel (e.g. lower porosity in
    # compacted spots) as (MASK_SIZE²,) arrays; constants are acceptable for P1.
    bake.make_masks(MASK_SIZE, 1.0, 0.6, 0.0, ao_png, str(out / f"{PIECE}.masks.png"))
    os.remove(ao_png)
export.export_glb(root, str(out / f"{PIECE}.glb"))
export.write_export_report(str(out / "export-report.json"), root, PIECE, __file__)
bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                       device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=64, marginPx=12,
                       channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                       texelDensityPxPerM=texel, skipped=skip_bake)
```

This skeleton already passes the geometry contract. The authoring work is `relief`, the anchors and the masks content, judged against the approved references. If `lod.assert_border_identical` fails after `geom.join`, the join merged or moved border vertices. Fix `geom.join` (Task 3) rather than relaxing the check.

- [ ] **Step 4: Build, determinism, compress**

Run: `make kit-build PIECE=meadow_base && make kit-determinism PIECE=meadow_base && make kit-compress PIECE=meadow_base`
Expected:
- the build exits 0;
- `deterministic <sha>`;
- the compress line shows the GLB under 2,097,152 B and each KTX2 family under 1,500,000 B.

- [ ] **Step 5: Register in the manifest and run the gates**

Set `assets/kit/manifest.json` to:

```json
{ "version": 1,
  "materials": {
    "grass": { "tileM": 2.0, "maps": { "albedo": "p1/materials/grass/grass.albedo", "normal": "p1/materials/grass/grass.normal", "orm": "p1/materials/grass/grass.orm" } },
    "soil":  { "tileM": 2.0, "maps": { "albedo": "p1/materials/soil/soil.albedo", "normal": "p1/materials/soil/soil.normal", "orm": "p1/materials/soil/soil.orm" } } },
  "pieces": [
    { "id": "meadow_base", "kind": "base", "glb": "p1/meadow_base/meadow_base.glb",
      "provenance": "assets-source/kit/meadow_base/provenance.json",
      "slots": { "meadow_base_cap": { "tileable": "grass", "masks": true }, "meadow_base_skirt": { "tileable": "soil" } },
      "maps": { "masks": "p1/meadow_base/meadow_base.masks" } } ] }
```

Run: `make kit-provenance PIECE=meadow_base REFS=assets-source/refs/meadow_base && make kit-check`
Expected: `wrote assets-source/kit/meadow_base/provenance.json (validation.pass=true)`, then the 8 `kit_gate` tests PASS. View `assets-source/kit/meadow_base/renders/three_quarter.png` with Read.

- [ ] **Step 6: In-engine shots**

Run: `make shots-kit` (only the meadow tiles render; road and tree warn as missing)
Expected: `target/shots/kit-overview.png` shows eight meadow hexes with no cracks or lighting steps between tiles. Zoom (crop) on a three-tile corner to confirm. Also run `make web-shots-kit`, which must end with `web-shots OK` with no console errors.

- [ ] **Step 7: Hook verification shots**

Run: `for h in snow wet night; do make shots-kit ARGS="--kit_hooks $h" && cp target/shots/kit-overview.png target/shots/kit-hooks-$h.png || exit 1; done`
Expected:
- the `snow` shot whitens the up-facing cap and not the skirt walls;
- `wet` darkens the cap;
- `night` shows no emission, because masks B = 0 on the meadow, as expected.

These prove the masks bind and the hooks are wired (PRD §9.4).

- [ ] **Step 8: OWNER CHECKPOINT: look-dev (Task 7 Step 7 content). STOP.**

Run `make shots-lookdev` on a host with load ≤ 15. Send the owner:
- the nine look-dev PNGs;
- `kit-hooks-snow.png`;
- the measured `texelDensityPxPerM` from the provenance;
- the HDRI facts;
- the masks.A = AO question.

Record the answers in `assets-source/lookdev/lookdev.json` and adjust the `Tonemap` default and `IBL_INTENSITY` if the owner says so. Also write the meadow base piece approval, if given, to `assets-source/kit/meadow_base/approvals.json` (`[{"who", "when", "gate": "piece"}]`). Then re-run `make kit-provenance PIECE=meadow_base REFS=assets-source/refs/meadow_base` so provenance carries it.

**Do not start Task 9 until look-dev is approved.**

- [ ] **Step 9: Gate and commit**

Run: `make checkall`
Expected: PASS.

```bash
git add scripts/blender/kit assets/kit assets-source/kit/meadow_base assets-source/kit/materials assets-source/lookdev crates/hex-render/src
git commit -m "feat(kit): meadow base and grass/soil tileable sets through every P1 gate; look-dev approved"
```

---

### Task 9: Road straight edge and center, scripted, plus the seam close-up (owner checkpoint)

**Authoring task.** Load the `blender-asset` skill (R4). One Opus 5.5 subagent owns `road_straight_center`, `road_straight_edge` and `asphalt`. The two pieces share one profile module, so their joint matches by construction.

**Files:**
- Create: `scripts/blender/kit/_road.py`, `scripts/blender/kit/road_straight_center.py`, `scripts/blender/kit/road_straight_edge.py`, `scripts/blender/kit/mat_asphalt.py`
- Create (generated): `assets/kit/p1/road_straight_{center,edge}/*`, `assets/kit/p1/materials/asphalt/*`, `assets-source/kit/road_straight_{center,edge}/*`
- Modify: `assets/kit/manifest.json`

**Interfaces:**
- Consumes: Task 8's meadow contract (cap relief ≤ 0 m and a border ring at 0 m), plus every pipeline target.
- Produces:
  - `_road.profile(lod) -> list[(t, z)]`: a lateral offset t (m, across the road) and a height z. It is symmetric (`(t, z)` and `(-t, z)` both present) and identical for all LODs at the piece ends.
  - `_road.build(name, x0, x1, segments, lod) -> Object`: a road bed swept along +X from `x0` to `x1`.
  - `ROAD_WIDTH = 6.0`, `VERGE = 1.0`, `BED_TOP = 0.08`, `BED_SINK = -0.30`, `CENTER_HALF = 6.0`.
  - Nodes:
    - `road_straight_center` with `_lod{0,1,2}` and `_collider`, spanning x ∈ [−6, 6].
    - `road_straight_edge` with `_lod{0,1,2}`, `_collider` and `socket-lane-0`, spanning x ∈ [6, APOTHEM]. The socket's extras are `{"kind": "road", "edge": 0, "offset": 0.0, "width": 6.0}` (PRD §5.2 lane).
  - Slots:
    - `road_asphalt`: `{"tileable": "asphalt"}`.
    - `road_verge`: `{"tileable": "soil"}`.
    - `road_paint`: `{"color": [0.80, 0.78, 0.70], "roughness": 0.6}`. This is a centre-line strip lifted 0.005 m with dashes 3 m on and 3 m off, phase-locked to x so dashes continue across tiles.

- [ ] **Step 1: Confirm approvals**

Run: `cat assets-source/refs/road_straight/approval.json assets-source/lookdev/lookdev.json`
Expected: the R2 references are approved and look-dev is approved (Task 8 Step 8). Otherwise STOP.

- [ ] **Step 2: Write the shared profile module**

Create `scripts/blender/kit/_road.py`:

```python
"""Shared road cross-section for road_straight_center and road_straight_edge.
Ends are identical in every LOD and mirror-symmetric (kit_gate check_edge_profile)."""
import bmesh
import bpy

from lib import scene

ROAD_WIDTH, VERGE, BED_TOP, BED_SINK, CENTER_HALF = 6.0, 1.0, 0.08, -0.30, 6.0


def profile(lod):
    """(t, z) points across the road, left to right. LOD only changes interior detail (crown,
    curb bevels); the outermost points (sink and verge edge) are the same at every LOD."""
    h = ROAD_WIDTH / 2
    edge = h + VERGE
    # AUTHOR: crown and curb detail by LOD, kept symmetric. Minimal valid profile below.
    inner = {0: [(-h, BED_TOP), (-h * 0.5, BED_TOP + 0.02), (0.0, BED_TOP + 0.03), (h * 0.5, BED_TOP + 0.02), (h, BED_TOP)],
             1: [(-h, BED_TOP), (0.0, BED_TOP + 0.03), (h, BED_TOP)],
             2: [(-h, BED_TOP), (h, BED_TOP)]}[lod]
    return [(-edge, BED_SINK), (-edge, BED_TOP - 0.02)] + inner + [(edge, BED_TOP - 0.02), (edge, BED_SINK)]


def build(name, x0, x1, segments, lod, materials_by_span):
    """Sweep the profile along +X. Face material per profile span via materials_by_span(i, t0, t1)."""
    prof = profile(lod)
    bm = bmesh.new()
    rows = []
    for s in range(segments + 1):
        x = x0 + (x1 - x0) * s / segments
        rows.append([bm.verts.new((x, t, z)) for (t, z) in prof])
    mats = []
    for s in range(segments):
        for i in range(len(prof) - 1):
            f = bm.faces.new((rows[s][i], rows[s + 1][i], rows[s + 1][i + 1], rows[s][i + 1]))
            f.material_index = materials_by_span(i, prof[i][0], prof[i + 1][0])
            mats.append(f)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bmesh.ops.triangulate(bm, faces=bm.faces)
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    return scene.link(bpy.data.objects.new(name, mesh))
```

The profile's left and right halves mirror each other. The Blender +Y lateral axis becomes runtime −Z, and the mirror check is symmetric, so the sign does not matter.

- [ ] **Step 3: Write the asphalt set and the two piece generators**

`mat_asphalt.py` is the Task 8 `mat_grass.py` skeleton with `SET = "asphalt"`. Author its look as dark aggregate asphalt with fine cracks and wear, roughness 0.6–0.85, and mean sRGB value 0.12–0.25. Run `make kit-material SET=asphalt` and do the 2×2 seam inspection from Task 8 Step 2.

`road_straight_edge.py` and `road_straight_center.py` follow the meadow skeleton's structure: reset, seeded RNG, root, three LODs, assert budgets, collider, sockets, masks, export, reports. They differ in these specifics:
- **Materials.** Build `road_asphalt`, `road_verge` and `road_paint` with `materials.pbr_material`. `materials_by_span` assigns asphalt where `|t| ≤ 3.0`, otherwise verge.
- **Length segments per LOD.** The edge piece uses (8, 4, 1) segments over its 7.856 m length, and the center piece (12, 6, 2) over 12 m.
- **Paint.** The paint strip is a separate thin quad strip in LOD0 and LOD1 only, 0.15 m wide at z = BED_TOP + 0.035. Join it into the LOD object, and omit it from LOD2.
- **Collider.** `collider_from(lods[2], "box", ...)`.
- **Sockets.** The edge piece adds `export.add_socket("socket-lane-0", (*hexgeo.lane_frame(0, 0.0, 6.0)["center"], 0.0), root, kind="road", edge=0, offset=0.0, width=6.0)`. Lane sockets mark the lane on the ground plane (y = 0), and the gate checks y = 0 ± 0.002. The center piece adds `anchor-prop-0` and `anchor-prop-1` (verge sign posts) at t = ±4.2.
- **Masks.** Bake AO into a 512² masks map using `uv.atlas_unwrap` on LOD0 (the road is not a hex cap, so it doesn't use the planar UV). Use snow 1.0 on the asphalt, porosity 0.3 on asphalt and 0.7 on the verge, and night 0.
- **Budgets.** At most 4000 LOD0 tris, with LOD1 ≤ 50% and LOD2 ≤ 15%. The profile above, at 12 segments, is well under budget. The authored curb and crown detail must stay inside it.

- [ ] **Step 4: Build, determinism, compress, register, gate**

```bash
for p in road_straight_center road_straight_edge; do
  make kit-build PIECE=$p && make kit-determinism PIECE=$p && make kit-compress PIECE=$p || exit 1
done
```

Add `asphalt` to `materials`. Add both pieces to `pieces` with kinds `center` and `edge`, slots `road_asphalt` (`{"tileable": "asphalt", "masks": true}`), `road_verge` (`{"tileable": "soil", "masks": true}`) and `road_paint` (`{"color": [0.80, 0.78, 0.70], "roughness": 0.6, "masks": true}`), and `maps.masks`.

Run: `for p in road_straight_center road_straight_edge; do make kit-provenance PIECE=$p REFS=assets-source/refs/road_straight || exit 1; done; make kit-check`
Expected: PASS, including `check_edge_profile` (mirror-symmetric, and identical across LODs) and the `socket-lane-0` position check at (13.856, 0, 0) ± 0.001 m.

- [ ] **Step 5: Seam close-up**

Run: `make shots-kit`
Expected: `target/shots/kit-seam-closeup.png` shows the joint at x = 13.856 between cell (0,0)'s edge piece (rot 0) and cell (1,0)'s edge piece (rot 3):
- no gap or step in the bed, verge or paint;
- dash phase continuous;
- no lighting discontinuity in the asphalt (world-space tiling);
- no crack between the road verge sink and the meadow cap.

Crop the joint region with Pillow or sharp at 4× and view it. If a seam is visible, find the cause with the evidence before changing anything:
- **Geometry** (the gate passed, but the profile sinks differently): fix `_road.py`.
- **Normals**: check that the bed's end vertices share normals.
- **Texture** (a visible step in the asphalt pattern at x = 13.856): the shader's world-space mapping is wrong. Debug in `kit_material.wesl`.

Then run `make web-shots-kit`. The web close-up `e2e/test-results/web-kit-seam-closeup.png` must match.

- [ ] **Step 6: OWNER CHECKPOINT: seam close-up (R8, PRD §15 P1). STOP.**

Send the owner:
- the native and web `kit-seam-closeup.png`;
- `kit-overview.png`;
- the two pieces' `renders/three_quarter.png`.

Record the approval in `assets-source/kit/road_straight_{center,edge}/approvals.json` with `"gate": "seam"`, then re-run `make kit-provenance` for both pieces (with `REFS=`). **Do not mark the P1 seam criterion until the owner approves in chat.**

- [ ] **Step 7: Gate and commit**

Run: `make checkall`
Expected: PASS.

```bash
git add scripts/blender/kit assets/kit assets-source/kit
git commit -m "feat(kit): road straight edge/center and asphalt set; owner-approved seam close-up"
```

---

### Task 10: Hero tree through the Blender MCP (interactive route; owner checkpoint)

**Authoring task.** Load the `blender-asset` skill (R4). One Opus 5.5 subagent owns `tree_hero_oak`. The interactive part happens in a live Blender through the MCP. Everything after the high-poly `.blend` reuses the scripted bake, LOD, export and gate stages.

**Files:**
- Create (local only, R1): `assets-source/blend/tree_hero_oak/tree_hero_oak_high.blend`
- Create: `scripts/blender/kit/tree_hero_oak.py` (the export stage, which reads the `.blend`)
- Create (generated): `assets/kit/p1/tree_hero_oak/*`, `assets-source/kit/tree_hero_oak/*`
- Modify: `assets/kit/manifest.json`, `.claude/skills/blender-asset/references/stages.md` (the Interactive route section)

**Interfaces:**
- Consumes:
  - MCP tools `mcp__Blender__get_objects_summary`, `mcp__Blender__execute_blender_code`, `mcp__Blender__get_screenshot_of_window_as_image`, `mcp__Blender__render_viewport_to_path`, `mcp__Blender__search_api_docs`. Load them with `ToolSearch` (`select:` with those names).
  - The Task 3 lib (`bake.bake_to`, `bake.bake_emit_input`, `bake.pack_orm`, `lod.decimate_to`, `uv.atlas_unwrap`, `uv.measure_texel_density`, `collider.collider_from`, `export.*`).
- Produces:
  - Nodes: `tree_hero_oak`, `tree_hero_oak_lod{0,1,2}`, `tree_hero_oak_collider` (a hull of the trunk only).
  - Slots: `tree_hero_oak_bark` (`{"unique": true, "masks": true}`) and `tree_hero_oak_leaves` (`{"unique": true, "masks": true, "alphaCutoff": 0.5}`).
  - Maps: `albedo` (RGBA, with the leaf alpha in A), `normal`, `orm` and `masks`, sharing one 2048² atlas.
  - Budgets: LOD0 ≤ 3000 tris with leaf cards, LOD1 ≤ 1500, LOD2 ≤ 450. Median texel density ≥ 256 px/m (D2).
  - Pivot at the trunk base (origin, z = 0). Height 11–13 m and canopy diameter 9–12 m (the approved reference, at the 16 m hex scale).

- [ ] **Step 1: Confirm approvals, then connect to the MCP**

Run: `cat assets-source/refs/tree_hero_oak/approval.json`. STOP if it is not approved.

Load the MCP tools with ToolSearch, then call `mcp__Blender__get_objects_summary`.

**Fallback (R10).** If the tools are absent or the call errors, ask the owner in chat: "Please start Blender 5.2 with the Blender MCP add-on enabled and connected, then reply." Wait, then retry. Do not substitute headless authoring for the interactive route without the owner's say-so.

- [ ] **Step 2: Author the high-poly tree interactively**

In the live session, inspect the scene first. Never modify the user's objects. Create a scene named `tree_hero_oak`. Using `execute_blender_code`, plus screenshots after each major step for self-review against the references:
- trunk with root flare and gnarled limbs, built as a skin-modifier or curve armature, then converted to a mesh with sculpted bark displacement;
- 2–4 branching levels;
- leaf clusters as alpha cards with a leaf texture painted or generated procedurally on a 4D-noise leaf mask.

The high-poly has no triangle budget. Use `search_api_docs` for any 5.2 API doubt, and log renames to `pitfalls.md`.

Save to `assets-source/blend/tree_hero_oak/tree_hero_oak_high.blend` (gitignored). Then run `shasum -a 256` on it.

- [ ] **Step 3: Write the export stage**

Create `scripts/blender/kit/tree_hero_oak.py`:

```python
"""tree_hero_oak export stage: opens the MCP-authored high-poly .blend (R1: local, hashed) and runs
the shared low/bake/LOD/export pipeline headless. Usage: make kit-build PIECE=tree_hero_oak."""
import os
import pathlib
import sys

import bpy

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from lib import bake, collider, export, geom, lod, scene, uv  # noqa: E402

PIECE, ATLAS = "tree_hero_oak", 2048
BLEND = pathlib.Path(os.environ.get("HEXW_BLEND", f"assets-source/blend/{PIECE}/{PIECE}_high.blend"))
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
bpy.ops.wm.open_mainfile(filepath=str(BLEND))
highs = [o for o in bpy.data.scenes[PIECE].objects if o.type == "MESH"]
# AUTHOR: build the LOD0 receiver from `highs`: decimate the trunk/branches to <= 1800 tris and
# keep the leaf cards as quads (<= 1200 tris), as two objects `bark_low` and `leaves_low`, each
# with one material named tree_hero_oak_bark / tree_hero_oak_leaves. Triangulate before UVs.
bark_low, leaves_low = bpy.data.objects["bark_low"], bpy.data.objects["leaves_low"]
for o in (bark_low, leaves_low):
    lod.triangulate(o)
uv.atlas_unwrap([bark_low, leaves_low], margin=0.004)
col = collider.collider_from(bark_low, "hull", f"{PIECE}_collider")  # trunk only, before the join
lod0 = geom.join([bark_low, leaves_low], f"{PIECE}_lod0")
assert geom.tri_count(lod0) <= 3000, geom.tri_count(lod0)
texel = uv.measure_texel_density(lod0, ATLAS)
assert texel["median"] >= 256, texel
sig = bake.receiver_signature([lod0])
if os.environ.get("HEXW_SKIP_BAKE") != "1":
    bake.setup_cycles(samples=32)
    kw = dict(highs=highs, cage_extrusion=0.03, max_ray=0.08, margin=12)  # AUTHOR: re-derive per mesh scale
    maps = {}
    for name, fn in [("albedo", lambda im: bake.bake_emit_input(highs, lod0, "Base Color", im, **kw)),
                     ("normal", lambda im: bake.bake_to(lod0, "NORMAL", im, **kw)),
                     ("rough", lambda im: bake.bake_to(lod0, "ROUGHNESS", im, **kw)),
                     ("ao", lambda im: bake.bake_to(lod0, "AO", im, **kw)),
                     ("metal", lambda im: bake.bake_emit_input(highs, lod0, "Metallic", im, **kw))]:
        im = bake.new_image(f"{PIECE}.{name}", ATLAS, non_color=name != "albedo")
        fn(im)
        maps[name] = bake.save_png(im, str(out / f"{PIECE}.{name}.png"))
    # AUTHOR: write the leaf alpha into albedo A (bake 'Alpha' through bake_emit_input into a
    # Non-Color image, then merge with numpy as bake.pack_orm does).
    bake.pack_orm(maps["ao"], maps["rough"], maps["metal"], str(out / f"{PIECE}.orm.png"))
    bake.make_masks(ATLAS, 1.0, 0.5, 0.0, None, str(out / f"{PIECE}.masks.png"))
    for k in ("ao", "rough", "metal"):
        os.remove(maps[k])
root = scene.empty(PIECE)
lod0.parent = root
lods = [lod0]
for i, target in ((1, geom.tri_count(lod0) // 2), (2, int(geom.tri_count(lod0) * 0.15))):
    c = lod0.copy()
    c.data = lod0.data.copy()
    c.name = c.data.name = f"{PIECE}_lod{i}"
    scene.link(c)
    lod.decimate_to(c, target)
    c.parent = root
    lods.append(c)
col.parent = root
export.export_glb(root, str(out / f"{PIECE}.glb"))
export.write_export_report(str(out / "export-report.json"), root, PIECE, __file__)
bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=sig, samples=32, marginPx=12,
                       cageExtrusionM=0.03, maxRayDistanceM=0.08, texelDensityPxPerM=texel,
                       device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"),
                       blend={"path": str(BLEND), "sha256": bake.sha256_file(BLEND)})
```

Leaf cards decimate poorly. If `decimate_to` destroys the canopy silhouette at LOD1/LOD2, build those LODs instead by keeping a subset of leaf cards (every 2nd or every 7th card, scaled up 1.3× to keep coverage) and decimating only the bark. Record that as an authoring decision in `friction.md`. The ratios gate the result either way.

The collider is a hull of the trunk and branches only, built before the join. If it exceeds 64 vertices, build it from the trunk's lowest 4 m instead. It is a stroll-blocker for P8 (D14), not a leaf volume.

- [ ] **Step 4: Build, compress, register, gate**

Run: `make kit-build PIECE=tree_hero_oak && make kit-compress PIECE=tree_hero_oak`
Expected: the build exits 0, and compress reports the KTX2 total per family. A 2048² RGBA atlas set is near the 1.5 MB D3 line. If it fails the budget, use a 1024² normal and ORM. The median density must stay ≥ 256 px/m, which the generator asserts. Record the outcome as D3 measurement evidence.

Add the piece to the manifest (kind `tree`, the slots above, `maps` for albedo, normal, orm and masks).

Run:

```bash
make kit-provenance PIECE=tree_hero_oak ROUTE=interactive REFS=assets-source/refs/tree_hero_oak BLEND=assets-source/blend/tree_hero_oak/tree_hero_oak_high.blend
make kit-check
```

Expected: PASS. Provenance carries `blendRef` (local store, key, sha256, bytes) and `route: "interactive"`.

- [ ] **Step 5: In-engine shots**

Run: `make shots-kit`
Expected: `target/shots/kit-tree.png` shows the tree on cell (0,1) with alpha-tested leaves, no sorting artifacts, and shadows from the leaf cards. Also check `kit-lod1.png` and `kit-lod2.png`: the canopy silhouette holds and there is no popping hole.

- [ ] **Step 6: OWNER CHECKPOINT: hero tree (R8). STOP.**

Send the owner:
- `kit-tree.png`, `kit-lod1.png` and `kit-lod2.png`;
- `assets-source/kit/tree_hero_oak/renders/three_quarter.png`;
- one `mcp__Blender__render_viewport_to_path` render of the high-poly.

Record the approval in `approvals.json` and re-run the provenance command.

- [ ] **Step 7: Document the interactive route, gate and commit**

Fill the "Interactive route" section of `references/stages.md`:
1. Check the approved references.
2. Load the MCP and connect, with the owner fallback.
3. Author in the scene named after the piece.
4. Save the `.blend` to `assets-source/blend/<id>/`.
5. Write the `kit/<id>.py` export stage, which opens that file.
6. Run the normal build, compress, provenance (`ROUTE=interactive BLEND=…`) and check stages.

Run: `make checkall`
Expected: PASS.

```bash
git add scripts/blender/kit/tree_hero_oak.py assets/kit assets-source/kit/tree_hero_oak .claude/skills/blender-asset/references/stages.md
git commit -m "feat(kit): hero oak via the Blender MCP interactive route, through every gate (owner-approved)"
```

---

### Task 11: Skill tuning, PRD corrections (D10) and docs

**Files:**
- Modify: `.claude/skills/blender-asset/SKILL.md`, `.claude/skills/blender-asset/references/*.md`, `PRD.md` (§9.3, §11.2, §11.3, §11.6, §15 P0/P1), `docs/agent-operability.md`
- Create: `docs/decisions/0002-asset-pipeline.md`
- Modify (if the measurement warrants): `crates/hex-assets/src/kit_manifest.rs` (`KTX_PIECE_MAX_BYTES`), `scripts/assets/kit-lib.mjs` (`KTX_PIECE_MAX`)

**Interfaces:**
- Consumes: every `assets-source/kit/*/friction.md`, every `compress-report.json`, `assets-source/lookdev/lookdev.json`.
- Produces: the tuned skill, which a later card promotes to user level (out of scope here), and the corrected PRD.

- [ ] **Step 1: Fold the P1 friction into the skill**

Run: `cat assets-source/kit/*/friction.md`

For each entry, either fix the cause in the skill (a rule, a stage command or a pitfall) or state in the entry why no change is needed. Delete statements that P1 proved wrong, such as an exporter option that did not exist. Keep `SKILL.md` under 120 lines. Detail goes in `references/`.

- [ ] **Step 2: Re-set the D3 budget from measurement**

Run: `node -e 'for (const f of process.argv.slice(1)) { const r = require(require("path").resolve(f)); console.log(f, JSON.stringify(r.budgets)); }' assets-source/kit/*/compress-report.json assets-source/kit/materials/*/compress-report.json`

Propose a final per-piece number to the owner in chat, with these figures:
- the measured maximum per family;
- the 30 MB initial-download arithmetic: wasm 8.5 MB + IBL 4.2 MB + pieces + sets, with BC7 and ASTC counted once each, since a device downloads one family.

Change both constants only after the owner answers. Until then the 1,500,000 B value stands and the ADR says it is provisional.

- [ ] **Step 3: PRD corrections (D10) and P1 status**

Edit `PRD.md`:
- **§9.3, first bullet.** Replace "`KHR_texture_basisu` (KTX2: UASTC for normals, ETC1S or UASTC+RDO for color maps)" with "sidecar KTX2 textures (UASTC+RDO source, transcoded offline to BC7 and ASTC with zstd, plus PNG), bound by the runtime; GLBs embed no images".
- **§11.2 step 6.** Replace it with: "Compress: gltf-transform 4.3.0 meshopt (`EXT_meshopt_compression`, FLOAT attributes, no `KHR_mesh_quantization`), glTF validation, per-channel UASTC → BC7/ASTC KTX2 + zstd with sRGB (base color, emissive) or linear (normal, ORM, masks) formats, PNG fallback, and hard GLB and texture byte budgets."
- **§11.3.** Change `web/public/assets/` to `assets/kit/` (+ `manifest.json`), and add `assets-source/blend/` (local-only).
- **§11.6.** Add: "P1 (owner, 2026-10-13): external storage deferred. `.blend` sources and bake intermediates stay local in the gitignored `assets-source/blend/`, referenced by SHA-256 in provenance."
- **§15 P0, asset criterion.** Append: "(The P0 GLB is meshopt-compressed but not quantized; bevy_gltf 0.20 rejects `KHR_mesh_quantization`; see D7 in `docs/decisions/0002-asset-pipeline.md`.)"
- **§15 P1.** Check each criterion only with its evidence path:
  - library: the Task 3 tests plus `make blender-test`;
  - pieces: `make kit-check` plus the provenance paths;
  - look-dev: `lookdev.json` approvals;
  - seam: the Task 9 approval;
  - provenance: the three `provenance.json` files.

  Leave "The external storage for `.blend` files is chosen and working" **unchecked**, annotated "deferred by owner (R1), local SHA-256-referenced storage in use".
- **§5.1.** Change "finalized during the P1 scale review" to "finalized in P1: R = 16 m, step 4 m (`hexgeo.py` asserted against `axial.rs`)".

- [ ] **Step 4: Write ADR 0002**

Create `docs/decisions/0002-asset-pipeline.md`, recording the owner rulings D2–D8 as applied, each with its evidence file:
- texel density (measured values);
- the D3 budget (status and measured maxima);
- the tone map choice;
- the IBL source, license, sizes and intensity;
- the masks contract, including the A = AO approval;
- float attributes;
- the frozen scale.

Then record:
- the world-space tileable decision and its reason (rotation-invariant seams);
- the CDT-regeneration LOD decision and its reason (an exact border ring; the Decimate protection was unverified);
- the unclosed items:
  - `data/sockets.ron` does not exist until P2, so lane widths are checked against socket extras only;
  - the alpha-coverage-per-mip check for leaf cards was not built;
  - the `.blend` external storage is deferred.

- [ ] **Step 5: Docs pass**

Update `docs/agent-operability.md` so every key and target added in Tasks 6–7 is listed:
- keys: `kit`, `kit_hooks`, `tonemap`, `ibl`;
- targets: `shots-kit`, `web-shots-kit`, `shots-lookdev`, `lookdev-ibl`.

Add a short "Asset pipeline" section listing `blender-test`, `blender-test-selfcheck`, `refs`, `kit-build`, `kit-determinism`, `kit-material`, `kit-compress`, `kit-compress-test`, `kit-renders`, `kit-provenance`, `kit-check` and `kit-fixture`, one line each, pointing to the skill.

- [ ] **Step 6: Final gate and commit**

Run: `make checkall-ci > target/checkall-ci.log 2>&1; ec=$?; tail -5 target/checkall-ci.log; echo "EXIT=$ec"`
Expected: `EXIT=0`. The wasm gzip stays under 12 MiB. Note: `kit_material` and `lookdev` add little code. The IBL KTX2 adds to the deployed payload, not to the wasm.

```bash
git add .claude/skills/blender-asset PRD.md docs crates/hex-assets/src/kit_manifest.rs scripts/assets/kit-lib.mjs
git commit -m "docs: P1 asset pipeline ADR, PRD corrections (D10), skill tuned from P1 friction"
```
