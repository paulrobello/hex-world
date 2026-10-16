# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Hex World is a browser world-building game on hexagon tiles: Rust workspace compiled to WASM, Bevy 0.20 (pinned `=0.20.0`, `default-features = false`) rendering through WebGPU, served by Trunk, deployed to GitHub Pages at hex-world.pardev.net. A native desktop build of `hex-app` is kept working for fast iteration, screenshots and profiling.

## Authoritative docs (read before the relevant work)

- `PRD.md` — product plan. §15 is the phase list, §10 perf/size budgets, §12 architecture as built, §14 gates.
- `docs/agent-operability.md` — every native CLI flag, web query parameter, `window.hexWorld` API, script command and state field. The reference for driving, screenshotting and measuring the game.
- `handoff.md` — current orchestrator state: standing owner rules, open work, blocked cards. Read this first in a fresh session.
- `docs/decisions/` — ADRs (engine, asset pipeline). `docs/kit/` — water bed and shader-hooks contracts. `docs/save-format.md` — `.hexworld` format.
- `.claude/skills/blender-asset/` — project skill for all 3D kit-piece work (Blender generators, bakes, LODs, provenance, gates).
- `DESIGN.md` — the UI design system; `design/` — brand and UI sources.

## Commands

```bash
make run ARGS="--scenario play"   # native release run; all flags in docs/agent-operability.md
make build-web                    # Trunk release build into dist/
trunk serve                       # dev server on 127.0.0.1:8094 (rebuilds live, includes HUD)
python3 scripts/dev/serve.py 8094 dist   # static serve of dist/ (8095 serves the repo for review pages)

make checkall                     # fast gate: fmt-check + clippy -D warnings + typecheck (native AND wasm32) + cargo test --workspace + ui-check
make checkall-ci                  # push gate: checkall + web-size (12 MiB gzip wasm) + initial-download (30 MiB)

# Single tests
cargo test -p hex-core socket                     # one Rust test by name
cargo test -p hex-app bridge::schema              # the bridge-schema contract test
node --test web/ui/test/rules.test.ts             # one UI test (Node type stripping, no runner build)
node e2e/smoke.mjs dist                           # one e2e script (after make build-web; e2e deps: cd e2e && npm ci)
```

- UI overlay gates: `make ui-check` (tsc, Biome, node --test, esbuild bundle). If the bridge contract test fails after a Rust enum change, run `make ui-schema` to regenerate `web/ui/src/bridge-schema.json` and `web/ui/test/rules-parity.json`.
- Browser e2e/perf (`e2e/*.mjs`, Playwright) run **locally on GPU Chromium only, never in CI**. CI runs the make gates per concern (`.github/workflows/ci.yml`), with `test` on ubuntu + macos.
- Perf/timing runs are load-guarded: host 1-minute load ≤ 15 immediately before and after each run or the run is discarded (`e2e/perf-load.mjs` enforces it). Timing without the guard is not evidence.
- Unattended native captures (`make thumbs`, `make shots-*`) run under `caffeinate -d -u` — a sleeping display produces black frames.
- Asset gates: `make kit-build`, `kit-compress`, `kit-check`, `kit-provenance` (see the blender-asset skill). `make brand` rebuilds icons/favicon from `design/brand/`.

## Architecture

Workspace (crates/, dependency order):

- `hex-core` — grid math (axial coords), sockets, tile definitions, placement rules, undo/redo, save encode/decode. No Bevy, no GPU deps; native-testable.
- `hex-sim` — networks (roads/rivers), car routing, fixed-tick simulation. Depends only on hex-core.
- `hex-assets` — kit manifest parsing, meshopt GLB loading, KTX2 texture-family selection (BC7/ASTC/PNG by GPU). Bevy behind a feature flag.
- `hex-render` — all Bevy render plugins: kit material and scene, water tiles, grass, boids, cars, tilt-shift and photo post passes, quality tiers, GPU timing, debug overlays, play-scenario affordances/ghost/rise. Embedded WESL shader libraries (`wind.wesl`, `grass_common.wesl`).
- `hex-app` — the Bevy app: scenario wiring, input actions, camera, persistence, the wasm JS bridge (`bridge.rs`). Entry for both native and web.

Key cross-cutting flows:

- **Tiles are data-driven**: `data/tiles/*.ron` + `data/sockets.ron` validated by `crates/hex-core/tests/data_validator.rs`; each tile references kit pieces from `assets/kit/manifest.json`. Changing sockets or pieces requires rerunning `make thumbs` (the validator checks every (tile, rotation) has a thumbnail and that equal-socket rotations share it).
- **Kit streaming**: pieces load by pack (`core`, `props`, `forest`, `hero`, `village`, `water` in `hex_assets::PACKS`); `core` loads before READY, others stream on demand. A piece may sample only texture sets in packs already loaded (the manifest parser enforces core purity). WebGPU's default 16 sampled textures per shader stage is the design target — `make web-smoke` clamps the device to it; keep new pipelines under it.
- **Web shell**: Trunk builds `index.html` + wasm; its `pre_build` hook runs `make ui-build` (the TypeScript HUD overlay in `web/ui/src`, bundled by esbuild into `web/ui/dist`), and its `post_build` hook content-hashes every URL (`?v=` cache busting, `scripts/web/version-dist.mjs`). The HUD talks to the engine only through the typed `window.hexWorld` bridge (commands/events), whose schema is contract-tested against the Rust enums. `web/ui` also owns the audio layer and IndexedDB autosave/worlds.
- **Rust ↔ UI changes are two-sided**: a command/event enum change in `hex-app` needs `make ui-schema`, then matching `web/ui/src/bridge.ts` work; `make checkall` fails until both sides agree.
- **Saves**: `.hexworld` = postcard payload behind `HEXWORLD` magic, u16 format version, CRC-32 (`hex-core/src/save.rs`). Agents (cars/boids) are re-derived on load, never saved. Fixtures for every historical version load in tests — new format versions need a fixture and a migration.

## Conventions and traps

- `.blend` sources live only in the main checkout's gitignored `assets-source/blend/<piece-id>/` (worktrees share them; they are never committed). Generated kit artifacts under `assets/kit/` are committed and must pass the provenance gates.
- `make thumbs` is not byte-reproducible run to run (water and grass animate): restore dirtied thumbnails with `git checkout HEAD -- assets/thumbs docs/checkpoints/thumbs` unless the change is intentional.
- Shaders: kit shaders live in `assets/shaders/*.wesl`; some shader libraries are embedded in hex-render source (`.wesl` files beside `.rs`). Safari 27 compat is handled at runtime by `web/wgsl-compat.js`.
- Every merge to `main` is followed by `make build-web` (owner rule; `dist/` is the play build).
- Never push without Paul's approval; queue commits locally (see Git Workflow in the global instructions and `handoff.md`).
- Checkpoint evidence (owner review images) goes to `docs/checkpoints/<feature>/` via the `make shots-*` targets; `e2e/test-results/` is uncommitted scratch.
