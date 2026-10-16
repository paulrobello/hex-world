# Audit Remediation Report

> **Project**: hex-world
> **Audit Date**: 2026-10-16
> **Remediation Date**: 2026-10-16
> **Severity Filter Applied**: all
> **Plan Source**: AUDIT.md `## Remediation Plan` (no AUDIT-REMEDIATION-PLAN.md playbook was present)
> **Implementation Model**: Opus 5 (all fix agents)

---

## Execution Summary

| Phase | Status | Agent | Issues Targeted | Resolved | Partial | Manual |
|-------|--------|-------|----------------|----------|---------|--------|
| 1 — Critical Security | ⏭️ Skipped (empty) | fix-security | 0 | 0 | 0 | 0 |
| 2 — Critical Architecture | ✅ | fix-architecture | 1 | 1 | 0 | 0 |
| 3a — Security (remaining) | ✅ | fix-security | 4 | 3 | 0 | 1 |
| 3b — Architecture (remaining) | ✅ | fix-architecture | 8 | 7 | 0 | 1 |
| 3c — All Code Quality | ✅ | fix-code-quality | 9 (+ARC-007) | 8 (+1) | 0 | 1 |
| 3d — All Documentation | ✅ | fix-documentation | 10 | 10 | 0 | 0 |
| 4 — Verification | ✅ | — | — | — | — | — |

**Overall**: 29 of 32 findings resolved, 0 partial, 3 require manual intervention or a follow-up run. QA-007 is counted resolved by verified retention (investigation found live callers, so nothing was removed).

Phase execution ran in two waves: security + architecture + documentation concurrently (each in its own worktree off the integration branch), then code quality + the reassigned ARC-007 after architecture merged, because both touch `control.rs`, hex-render, and the Makefile. Every phase merged into the integration branch `fix/audit-remediation` (36 commits over `main`'s `4cb58dd`; 51 files, +2524/−1227) and was gated before the next phase branched.

---

## Resolved Issues ✅

### Architecture

- **[ARC-001]** Per-frame full-state JSON serialization — `crates/hex-app/src/control.rs`, `bridge.rs`, `game.rs`, `index.html`, `web/ui/test/state-publication.test.ts` (commit `32786f2`). Publication is now revision-gated (`world_rev` for world-sized fields, a game revision for feedback), sub-reports are forwarded verbatim instead of parse→re-serialize, and the bridge drains coalesced changed-field patches instead of cloning full snapshots per poll. JSON schema unchanged; new regression tests cover patch merging, unchanged-array reuse, and scenario transitions. Note: this lands on the pre-existing "Large play worlds are slow on the web" card (`01a14028…`), whose own acceptance criteria (Chrome trace, load-guarded p95 ≤ 16.6 ms, native numbers) remain open — the root cause is fixed, the measurements are not done.
- **[ARC-003]** Per-feature play plugins — `crates/hex-render/src/play/plugins.rs` (new), `lib.rs`, `play.rs` (commit `d714ef7`). Tile, scatter, grass, rise, ghost, and affordance sub-plugins with explicitly ordered sets; `HexRenderPlugin` composes `PlayPlugin` instead of hand-registering 14 resources and 12 systems.
- **[ARC-004]** hex-app plugins with named system sets — `crates/hex-app/src/app_plugins.rs` (new), `main.rs` (commit `fab2ddf`). Five feature plugins with named, chained sets; the original 20-system order preserved exactly.
- **[ARC-005]** Makefile capture DRY + clean — `Makefile`, `scripts/shots/run.sh`, `scripts/shots/run.test.mjs` (commits `af856a6`, `8071f34`). Native captures centralized with caffeinate and black-frame rejection enforced for all targets; `clean` target added; four regression tests.
- **[ARC-006]** Snapshot resources + scoped clippy allows — `control.rs`, `actions.rs`, `photo.rs`, `persist.rs`, root `Cargo.toml`, `crates/hex-render/Cargo.toml` (commit `e569b41`). Publication parameters bundled into cohesive SystemParam snapshots; workspace-wide `too_many_arguments`/`type_complexity` allows removed — the root now *warns*, hex-render carries the only scoped allow, remaining app exceptions are per-system.
- **[ARC-007]** Single-wgpu CI assertion — `Makefile`, `.github/workflows/ci.yml`. New `make check-wgpu` target wired into `checkall-ci` and the CI workflow; passes with exactly one `wgpu v30.0.1`.
- **[ARC-008]** persist.rs split — `crates/hex-app/src/persist/settings.rs` (new), `persist.rs` (commit `bd98d97`). Settings model and camera pose codec extracted; importer APIs preserved via re-exports.

### Security

- **[SEC-002]** CSP hardening — `index.html`, `scripts/web/version-dist.mjs` + test (commit `925b471`). `script-src` uses SHA-256 hashes of both source inline scripts plus `wasm-unsafe-eval`; `base-uri 'none'`, `form-action 'none'`, `object-src 'none'` added. The post-build hook now hashes build-generated inline scripts too (they would otherwise be blocked), verified by seven hook tests.
- **[SEC-003]** Trunk tarball checksum — `.github/workflows/ci.yml` (commit `3d7bb05`). CI downloads Trunk v0.21.14 to a file and verifies pinned SHA-256 `f2b4680c…` before extraction.
- **[SEC-004]** Hostile-save replay risk documented — `docs/save-format.md` "Import threat model" section (commit `da378b1`). Accepted-risk rationale recorded (16 MiB encoded cap, 128-action history, per-step validation, local tab-only impact).

### Code Quality

- **[QA-001]** Monolithic dispatchers decomposed — `args.rs` handler table, `game.rs` per-action handlers (recursive re-dispatch removed, QuickPlace walking-refusal regression test added), `hud.ts` named keyboard guards, `meshopt_glb.rs` extracted decode helpers, `play.rs` `sync` split into refresh/removal spans in place.
- **[QA-002]** `run_commands` unified — `control.rs`. Photo/Walk special cases and the action early-return folded into the main match; command-system parameters bundled; 134 app tests pass including bridge-schema and publication contracts.
- **[QA-003]** hud.ts split — `web/ui/src/sheet-controller.ts` (new), `hud.ts`, `photo.ts`, `walk.ts`, `sheet.ts`. Mode keys delegated to Photo/Walk/Sheet controllers; inventory state extracted into `SheetController`; 149 UI tests pass including rules parity.
- **[QA-005]** Poisoning-tolerant locks — `bridge.rs`, `control.rs`. `lock().unwrap_or_else(|p| p.into_inner())` on the shared statics.
- **[QA-006]** Duplicate `specialize()` hoisted — `post.rs` shared `specialize_target`, used by `photo_fx.rs` and `tilt_shift.rs`.
- **[QA-007]** `kit_refs` resolved by verified retention — parsight impact analysis and grep found live callers in the runtime, the data validator, and the kit gate. The audit's structural no-caller claim was a false absence (the known parsight pattern). Functions retained; nothing removed.
- **[QA-008]** `plan_boids` SystemParam bundles — `boids.rs`. Now `fn plan_boids(world: BoidWorld, outputs: BoidOutputs)`; the `too_many_arguments` allow removed; 232 render tests pass.
- **[QA-009]** Persistence narrowed — `web/ui/src/persist-ux.ts` (new), `persist.ts`. Naming, export presentation, notification effects, and slot summaries moved behind a narrow `PersistenceUx` host; the save state machine stays in `Persistence`.

### Documentation

- **[DOC-001]** README Prerequisites + Getting started (toolchain derived from Makefile/CI; `trunk serve`, `make checkall`).
- **[DOC-002]** `CONTRIBUTING.md` (worktrees, `make ui-schema` two-sided contract, gates, checkpoint evidence, commit style).
- **[DOC-003]** 51 rustdoc additions across hex-core (`world.rs`, `catalog.rs`, `axial.rs`, `socket.rs`, `tile.rs`); doc-comments only, mechanically verified; `kit_refs` deliberately undocumented (retained, see QA-007).
- **[DOC-004]** Private vault wikilinks replaced with external-note markers or inlined lessons (`handoff.md`, blender-asset references, research brief); verified absent after.
- **[DOC-005]** README "Change history" section states the git-log-plus-PRD discipline (the audit's alternate remedy; no CHANGELOG.md created).
- **[DOC-006]** Environment reference table in `docs/agent-operability.md` — 52 verified controls with consumers, defaults, effects.
- **[DOC-007]** Stale kit_gate path fixed to `crates/hex-render/tests/kit_gate.rs`.
- **[DOC-008]** Built-site (`web/ui/hud.js`) vs source (`web/ui/src`) paths clarified in `agent-operability.md` and the stall-tail README.
- **[DOC-009]** README License section (MIT) + CI badge.
- **[DOC-010]** `docs/DOCUMENTATION_STYLE_GUIDE.md` capturing the de-facto conventions.

---

## Requires Manual Intervention 🔧

### [SEC-001] ruzstd decompression bomb (RUSTSEC-2026-0344, High) — BLOCKED ON UPSTREAM
- **Why**: `bevy_image 0.20.0` requires `ruzstd ^0.8.0`, which excludes the fixed 0.9.1. `cargo update` cannot move it; forcing a different version range would need a `[patch]` fork of bevy_image. Verified by explicit precise-version attempt; no files changed.
- **Recommended approach**: watch bevy_image for a release allowing ruzstd ≥ 0.9 (or the Bevy 0.21 line), then `cargo update -p ruzstd` and re-run `make checkall`. Risk today is limited (all compressed inputs are same-origin first-party assets), but it is the audit's only High security finding and the card stays `blocked` on the board.
- **Estimated effort**: S once upstream moves.

### [ARC-002] hex-render crate split (Medium) — DEFERRED WITH SCOPE EVIDENCE
- **Why**: hex-render has internal cycles the audit's remedy did not see. Reverse dependencies of play: boids (`PlayView`/`PlayCatalog`/`PlayKit`/`PlaySystems`), rises and scatter layouts, cars (play pack resources), tier (`PlaySpawned`), walk/debug (play data), workload (grass density), scene (setup). play depends on: renderer materials, kit-scene rotor helpers and manifest, scene LOD helpers, water, scenario types, `GateConfig`. Extracting a crate across those cycles means designing trait/event boundaries first. Evidence recorded on the board card.
- **Recommended approach**: a dedicated run: (1) break the play↔renderer seams with boundary traits/events, (2) extract `hex-play`, (3) move gate/lookdev tooling behind a feature. ARC-003 (landed) already provides the plugin composition the extraction will need.
- **Estimated effort**: L.

### [QA-010] Shared Blender kit helpers (Low) — DEFERRED
- **Why**: the `_shared.py` extraction trips the kit hash/provenance gates; completing it requires the Blender rebuild + provenance workflow (`make kit-build/kit-compress/kit-check/kit-provenance`), which is asset-pipeline tooling outside a code-quality pass. The change was implemented, verified syntactically, and cleanly reverted; no generator changes remain.
- **Recommended approach**: do it alongside the next deliberate kit-piece work so the provenance rebuild is amortized; the audit notes likely overlap with the "Kit pipeline hygiene follow-ups" card.
- **Estimated effort**: M (mostly the rebuild, not the code).

---

## Verification Results

- Build: ✅ Pass (native + wasm32 typecheck via `make checkall` and `make checkall-ci`)
- Tests: ✅ Pass — `cargo test --workspace`, UI gate 149 tests (incl. bridge-schema, state-publication, rules parity), persistence (14), control (10), render (232), capture helper (4); the docs agent additionally ran a green `make checkall-ci` on its branch pre-merge
- Lint: ✅ Pass — clippy `-D warnings` with the new scoped lint policy (workspace warns on `too_many_arguments`)
- Format: ✅ Pass — fmt-check
- CI-parity: ✅ Pass — `make checkall-ci` exit 0 on the final tree; initial download 20.56 MiB vs 30 MiB budget
- Per-issue validation: every merged issue spot-verified by direct inspection (grep for the named change) in addition to agent gates; per-issue evidence recorded on each board card

No regressions observed; the combined integration tree was gated (`make checkall`) after the first merge wave and (`make checkall-ci`) at the end.

---

## Files Changed

36 commits, 51 files, +2524/−1227 over `4cb58dd` (branch `fix/audit-remediation`, integration worktree `.claude/worktrees/fix-audit-remediation`):

Created: `CONTRIBUTING.md`, `docs/DOCUMENTATION_STYLE_GUIDE.md`, `crates/hex-app/src/app_plugins.rs`, `crates/hex-app/src/persist/settings.rs`, `crates/hex-render/src/play/plugins.rs`, `scripts/shots/run.sh`, `scripts/shots/run.test.mjs`, `web/ui/src/persist-ux.ts`, `web/ui/src/sheet-controller.ts`, `web/ui/test/state-publication.test.ts`

Modified: `.claude/skills/blender-asset/references/gates.md`, `.claude/skills/blender-asset/references/pitfalls.md`, `.github/workflows/ci.yml`, `Cargo.toml`, `Makefile`, `README.md`, `crates/hex-app/src/{actions,args,bridge,control,game,main,photo}.rs`, `crates/hex-app/src/persist.rs`, `crates/hex-assets/src/meshopt_glb.rs`, `crates/hex-core/src/{axial,catalog,socket,tile,world}.rs`, `crates/hex-render/Cargo.toml`, `crates/hex-render/src/{boids,lib,photo_fx,play,post,tilt_shift}.rs`, `docs/agent-operability.md`, `docs/perf/stall-tail/README.md`, `docs/save-format.md`, `docs/superpowers/research/2026-10-13-p1-asset-pipeline-brief.md`, `handoff.md`, `index.html`, `scripts/web/version-dist.mjs`, `scripts/web/version-dist.test.mjs`, `web/ui/package.json`, `web/ui/src/{hud,photo,persist,sheet,walk}.ts`

---

## Next Steps

1. Review the three manual-intervention items above; SEC-001 is the one with security weight (blocked on upstream Bevy).
2. The "Large play worlds are slow on the web" card remains the tracked follow-up for the ARC-001 fix's measurements (Chrome trace, load-guarded p95, native numbers) — the root-cause fix is in; the numbers are not.
3. Re-run `/audit` after the follow-ups land to get an updated AUDIT.md.
4. ARC-002's seam inventory (on its board card) is the starting point for the future hex-play extraction.
