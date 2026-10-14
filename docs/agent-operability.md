# Agent Operability

Hex World can be driven, observed, screenshotted and exited from the command line, natively and in the browser, with no OS screen-recording permission and no human. Screenshots are captured inside the app with Bevy's `Screenshot::primary_window()`, so they need only a window the app itself opens.

## Native CLI flags

Flags take the form `--name value`. A bare `--name` means `1`. Bad values log a warning and fall back to the default.

| Flag | Meaning | Example |
|---|---|---|
| `--scenario <name>` | Start scenario: `single` (default), `gate2000`, or `kit` (the P1 kit family from `assets/kit/manifest.json`; missing pieces warn and are skipped) | `--scenario gate2000` |
| `--kit_hooks <none\|snow\|wet\|night>` | Kit scenario only. Drives the §9.4 `masks` hook uniforms (snow, wetness, night emissive) for verification shots. Default `none` (noon) | `--kit_hooks snow` |
| `--kit_src <runtime\|fixture\|fixture_tiled>` | Kit scenario only. `runtime` (default) lays out the P1 family from `assets/kit/manifest.json`; `fixture` lays out three copies of the kit gate fixture (manifest `crates/hex-render/tests/fixtures/kit/manifest.json`, files served from `assets/kit/p1/fixture/`, kept byte-identical by `kit_scene::tests::served_fixture_matches_the_gate_fixture`); `fixture_tiled` is the same layout with the cap bound to a probe tileable set (`assets/kit/p1/fixture_tile/`, constant maps copied from the fixture masks), so the tileable forward and prepass shader branches run (web-smoke uses it) | `--kit_src fixture` |
| `--tonemap <agx\|tony\|aces>` | Kit scenario only. Camera tone mapping (D4): `aces` (default, ACES fitted, owner look-dev ruling 2026-10-13), `agx` (AgX) or `tony` (TonyMcMapface). Other scenarios keep the P0 camera | `--tonemap aces` |
| `--ibl <on\|off>` | Kit scenario only. `on` (default) adds the prefiltered noon `EnvironmentMapLight` (`kit://p1/ibl/noon.{diffuse,specular}.ktx2`, intensity `lookdev::IBL_INTENSITY`) and sets the global ambient light to 0; `off` keeps the flat 400 ambient. With IBL on the kit forward pipeline samples 16 textures per fragment stage, exactly WebGPU's default limit (14 with `off`) | `--ibl off` |
| `--ev100 <f32>` | Kit scenario only. Kit camera exposure (EV100, 0–20, default `lookdev::KIT_EV100` = 12.5, chosen from an exposure sweep and owner-approved 2026-10-13; Bevy's default 9.7 clips the 30,000 lx sun). The kit scenario also aims its sun along the HDRI sun direction `lookdev::KIT_SUN_DIR` = (0.553, 0.742, 0.379); other scenarios keep the P0 light | `--ev100 13.3` |
| `--tilt <auto\|on\|off>`, `--tilt_strength <0..1>` | Tilt-shift post pass (PRD §9.1, `hex_render::tilt_shift`): a sharp horizontal band at the camera's look-at point on the ground, blur growing with screen distance from the band and with depth difference, plus a small saturation lift. Fades out as the focus distance drops from 48 m to 12 m. `auto` (default) is on from the Medium tier up (8 taps Medium, 16 High, 24 Ultra, off on Low) and only in the kit scenario with `post=full`, so P0 baselines are unchanged; `on`/`off` force it in any scenario. `T` toggles it at runtime. One pass, 2 sampled textures in the fragment stage (colour and prepass depth). `make shots-tilt` writes the off/on pair to `docs/shots/tilt-shift/` | `--tilt off` |
| `--view <name>` | Gate camera view: `strategic` (default, whole world from 900 m) or `closeup` (40 m above tile (3, 2)). Unknown values warn and fall back | `--view closeup` |
| `--shot <name>` | Select the scenario and save `target/shots/<name>.png` at frame 120 | `--shot single` |
| `--exit-after <frames>` | Exit after N frames | `--exit-after 180` |
| `--exit-after-seconds <s>` | Exit after N wall-clock seconds | `--exit-after-seconds 10` |
| `--screenshot-after <s>` | Save one screenshot after N seconds, named by `--shot` or `screenshot-after` | `--screenshot-after 2` |
| `--script <file.json>` | Run a command list (see below). A missing or invalid file exits with code 2 before the window opens | `--script scripts/shots/gallery.json` |
| `--size <W>x<H>` | Fixed, non-resizable window at scale factor 1, so PNGs are exactly W by H | `--size 1280x800` |
| `--seed <u64>` | Stored in state for determinism; later animation and boids read it | `--seed 1` |
| `--no-anim` | Stored flag; later animation systems read it to freeze motion | `--no-anim` |
| `--dump-state <path>` | Write the state JSON on every exit path, including closing the window | `--dump-state target/state.json` |
| `--diag` | Enable diagnostics | `--diag` |
| `--layout <name>` | Gate static geometry: `entities` (default, one entity per piece per LOD), `chunks` (per chunk, LOD and material, all static pieces merged into one mesh at the chunk centroid; `VisibilityRange` applies per chunk), or `instanced` (the PRD §6 shipping shape: one GPU instance buffer of `{transform, material}` grouped by kit mesh and chunk; per view the CPU culls chunks, picks each chunk's LOD from the camera distance to its centroid at 150/400 m, and issues one draw per (kit mesh, LOD) through one pipeline that reads a 40-entry material palette; it draws into the main pass, the depth/normal/motion-vector prepass and every shadow cascade; `crates/hex-render/src/instanced.rs`). Vehicles stay individual entities | `--layout instanced` |
| `--chunk <n>` | Hexes per chunk for `layout=chunks` and `layout=instanced`: `7` (default) or `19` | `--chunk 19` |
| `--shadow_casters <all\|large>` | Which static pieces cast shadows. `all` (default) or `large`: props and every LOD2 piece stop casting (`NotShadowCaster` in the entities layout, no shadow draw in the instanced layout; the chunks layout merges props with large pieces, so there it drops LOD2 only) | `--shadow_casters large` |
| `--cascades <n>` | Directional shadow cascades: `2`, `3` or `4` (default) | `--cascades 3` |
| `--shadow_dist <m>` | Cascade maximum distance in whole metres, above the 120 m first-cascade bound. Default `2000` | `--shadow_dist 800` |
| `--post <name>` | `full` (default: TAA, SSAO, bloom, DOF) or `none` (all four off, MSAA stays off) | `--post none` |
| `--fps <on\|off>` | On-screen FPS counter (`60 fps | 16.4 ms | p95 17.1`, top-right, Bevy UI text). Default `on`; the **F3** key toggles it at runtime. It is a persistent UI entity, not a `SceneEntity`, so `scenario` commands keep it. The galleries (`make shots`, `make shots-chunks`, `make web-shots`) pass `off`; the perf gate leaves it on | `--fps off` |
| `--shadows <on\|off>` | Directional-light shadow maps (`cascades` cascades). Default `on` | `--shadows off` |
| `--present <mode>` | Native only. `fifo` (default, vsync), `immediate`, or `novsync` (Bevy `AutoNoVsync`: immediate, else mailbox, else fifo). Use `novsync` for uncapped frame times | `--present novsync` |

Every exit path waits until pending screenshots have been written. The gate keys (`layout`, `chunk`, `post`, `shadows`, `shadow_casters`, `cascades`, `shadow_dist`, `kit_hooks`, `kit_src`, `tonemap`, `ibl`, `ev100`, `tilt`, `tilt_strength`) are read on every scene build, so a `scenario` command keeps them. Unknown values warn and fall back to the default.

## Web query parameters

`shot`, `scenario`, `view`, `diag`, `exit-after`, `seed`, `no-anim`, `layout`, `chunk`, `post`, `shadows`, `shadow_casters`, `cascades`, `shadow_dist`, `fps`, `kit_hooks`, `kit_src`, `tonemap`, `ibl`, `ev100`, `tilt`, `tilt_strength`, with the same meanings as the flags. Example: `http://127.0.0.1:8094/?scenario=gate2000&view=closeup&diag=1`.

`wgsl_compat` (`0` or `1`) controls `web/wgsl-compat.js`, which inlines the WGSL `let` texture and sampler handle aliases that browsers without the `texture_and_sampler_let` language extension (Safari 27) reject. `1` forces the rewrite on, `0` turns it off, and by default it is active only when the browser lacks the extension. `window.__wgslCompat` is set before the app loads (absent when WebGPU is unavailable) and holds `{ active, native, modules, rewritten, aliases, unhandled, ms }`: `active` is whether the rewrite is on, `native` whether the browser supports the extension, `modules` and `rewritten` count the shader modules seen and changed, `aliases` the aliases inlined, `unhandled` the bindless aliases left alone, and `ms` the time spent rewriting.

## JavaScript API (`window.hexWorld`)

Defined after the `TrunkApplicationStarted` event.

| Call | Returns | Example |
|---|---|---|
| `ready()` | `true` once the scene has rendered 120 frames | `await page.waitForFunction(() => hexWorld?.ready())` |
| `state()` | State object | `hexWorld.state().scenario` |
| `dumpState()` | State object: scenario, tiles (hex cells laid out; for `kit` it follows `kit_src`), frames, seed, no_anim, `gpu` (`{ maxSampledTexturesPerShaderStage }` from the render device, `null` until it exists; also logged once at startup as `gpu: max_sampled_textures_per_shader_stage=N`), camera `pos`/`forward`, queued command count, and `diag` (the latest diagnostics object, also written by native `--dump-state`) | `hexWorld.dumpState().camera.pos` |
| `diag()` | Diagnostics object, refreshed every 30 frames: `p50`/`p95`/`p99`/`mean`/`max` frame ms and `dropped_over_25ms` (frames slower than 25 ms) over the last 600 frames from `window_start_frame` (the frame after READY) on, `layout`, `chunk_size` and `chunks` (0 for entities), `shadows` (read from the light), `shadow_casters` (the config), `cascades` and `shadow_dist` (read from the light's `CascadeShadowConfig`), `passes` (per view and phase, I4: the camera's `opaque` and `prepass` phases and each `cascade<i>` `shadow` phase, with `multidraw_sets` (one multi-draw-indirect call each, native), `batchable_bins` (one instanced draw per distinct mesh+material), `unbatchable` and `non_mesh` (the instanced layout's 15 slots, empty ones skip their draw); sampled outside the first window), `instanced` (instanced layout only, else `null`: per view the non-empty `draws`, `instances` and `tris` it issued, and over all views `union_instances_by_lod` and `union_tris`), `tiles` (tile entities plus the hexes held by chunks), `entities` (`Mesh3d` count), `pieces` (logical static pieces from the scene build), `visible_entities`, `visible_mesh_material_pairs` (distinct pairs, the most draws left after instancing), `visible_tris` and `visible_by_lod` (all from `ViewVisibility`, which counts the camera and the shadow cascades; sampled every 30 frames except during the first 600-frame window), `wasm_memory_bytes` (wasm linear memory, 0 natively), `vehicles`, `tris_lod0_per_tile` (counted from the spawned meshes), `materials`, `post` (TAA/bloom/DOF from the camera, SSAO from the render world), `ssao` (a compiled SSAO pipeline exists for a view), `gpu_preprocessing_supported` (device capability), `adapter`, `samples`, `frames`, `window_start_frame` | `hexWorld.diag().p95` |
| `cmd(json)` | Enqueues one command object or an array of them (string or object). Parse errors go to `console.error` | `hexWorld.cmd('{"cmd":"wait","frames":30}')` |
| `screenshot(name?, timeoutMs = 10000)` | Promise of a PNG data URL. The capture is queued behind earlier commands | `await hexWorld.screenshot("view")` |

## Commands

A script is a JSON array. Commands run in order, one per frame, starting once the scene is ready (frame 120). A bad command at run time is logged and skipped.

| Command | Effect | Example |
|---|---|---|
| `camera` | Move the camera to `pos`, looking at `look_at` (metres; a hex has a 16 m radius) | `{"cmd":"camera","pos":[40,30,40],"look_at":[0,2,0]}` |
| `wait` | Hold the queue for N frames | `{"cmd":"wait","frames":60}` |
| `screenshot` | Native: `target/shots/<name>.png`. Web: data URL for `hex_world_take_shot` | `{"cmd":"screenshot","name":"overview"}` |
| `scenario` | Despawn the scene and rebuild it as `single`, `gate2000` or `kit` | `{"cmd":"scenario","name":"gate2000"}` |
| `tilt` | Runtime tilt-shift toggle and strength; either field may be omitted. The state JSON reports it under `tilt` | `{"cmd":"tilt","on":false,"strength":0.5}` |
| `dump_state` | Log the state JSON; natively also write `target/shots/<name>.json` | `{"cmd":"dump_state","name":"after-move"}` |
| `exit` | Exit once pending screenshots are written (native) | `{"cmd":"exit"}` |

## Make targets

| Target | What it does |
|---|---|
| `make run ARGS="..."` | Native release run with arbitrary flags, e.g. `make run ARGS="--scenario gate2000"` |
| `make shot` / `make run-shot` | Native `--shot single`, writes `target/shots/single.png` |
| `make perf-gate` | Uncapped GPU Chromium at 2560x1440, gate2000 in both views, each measured over a full 600-frame post-READY window; writes `docs/perf/p0-engine-gate.json` (what ran, beside the budgets). It measures the superseded `entities` layout by default (`GateConfig::default()`); the shipping-layout gate is the `perf-matrix` web rows run with `MATRIX_ONLY=web MATRIX_CONFIGS=instanced-tuned,instanced-mid600,instanced-mid1200,instanced-full`, as used in `docs/decisions/0001-engine.md`. Exits 1 on a budget miss, missing SSAO, a short window or a console error |
| `make perf-matrix` | Diagnosis matrix (Task 14): four configs (entities full, instanced full, instanced `shadow_casters=large` + 3 cascades, instanced with shadows and post off) × two views, on web (uncapped GPU Chromium, a fresh browser per run) and native (`--present immediate`, 20 s, 2560x1440). Each config runs `MATRIX_RUNS` times (default 3) round-robin. `MATRIX_CONFIGS` selects configs, including the middle shadow settings `instanced-mid600` and `instanced-mid1200` (all casters, 3 cascades, `shadow_dist` 600 or 1200 m) that are not run by default. `MATRIX_ONLY=web|native` runs one platform. `MATRIX_PACED=1` keeps vsync on. `MATRIX_OUT` redirects the JSON. Writes `docs/perf/p0-gate-matrix.json` after every run: raw `rows` plus a `summary` with median and min–max p50/p95 per config, and `vsync_like_runs` (p50 within 0.3 ms of 16.67 or 8.33 ms); a run that crashes or echoes a different config is recorded, not dropped |
| `make shots-chunks` | The native gallery with `--layout chunks`, writes `target/shots/gallery-chunks-*.png` beside the entities gallery |
| `make shots-instanced` | The native gallery with `--layout instanced`, writes `target/shots/gallery-instanced-*.png` |
| `make run-diag` | Native gate2000 with `--diag`, writes and prints `target/state.json` |
| `make shots` | Native gallery from `scripts/shots/gallery.json` at 1280x800, writes `target/shots/gallery-*.png` |
| `make web-shots` | Builds the web target and runs the same gallery in GPU Chromium, writes `e2e/test-results/web-gallery-*.png` |
| `make web-smoke` | Browser smoke test: WebGPU adapter (not SwiftShader unless `PW_GPU=swiftshader`), bad-scenario fallback, no-GPU redirect, the forced WGSL compat shim, and the kit scenario with `kit_src=fixture`: kit meshes must spawn and draw with no WebGPU validation error on the adapter's limits, with the device clamped to WebGPU's default 16 sampled textures per stage (`e2e/webgpu-probes.mjs` also counts each pipeline layout's fragment textures; the kit measures 14), and under the forced shim, where the kit fragment (its `masks_texture` global) must be the module rewritten |
| `make shots-kit [ARGS="--kit_src fixture"]` | Native kit shots from `scripts/shots/kit.json` at 1600x1000: `target/shots/kit-{overview,seam-closeup,tree,tree-under,tree-game,lod1,lod2}.png` plus `kit-state.json`. The LOD shots sit about 226 m and 467 m from the tiles (LOD1 and LOD2 bands) |
| `make lookdev-ibl` | Blender (local only): runs `make fetch-hdri`, then bakes the gitignored Poly Haven HDRI `assets-source/hdri/kloofendal_48d_partly_cloudy_puresky_2k.hdr` into `assets/kit/p1/ibl/noon.specular.ktx2` (GGX-prefiltered RGBA16F cubemap, 256² with 9 mips) and `noon.diffuse.ktx2` (SH9 irradiance, 32²), records hashes in `assets-source/lookdev/lookdev.json`, then runs `ktx validate` on both |
| `make shots-lookdev [ARGS="--kit_src fixture"] [LOOKDEV_OUT=dir]` | Runs the kit shot script once per tone mapper and copies `lookdev-{agx,tony,aces}-{kit-overview,kit-seam-closeup,kit-tree}.png` into `LOOKDEV_OUT` (default `target/shots`). Check host load ≤ 15 first |
| `make web-shots-kit [QUERY="kit_src=fixture"]` | The same script in GPU Chromium, writes `e2e/test-results/web-kit-*.png`. `QUERY` is appended to the page URL |

`make shots` deletes every `target/shots/gallery-*.png` first, including the `gallery-chunks-*` and `gallery-instanced-*` galleries. Run it before `make shots-chunks` and `make shots-instanced`.

### Host-load rule for timing runs

`e2e/perf-load.mjs` guards every timing run in `perf-gate.mjs` and `perf-matrix.mjs`, on web and native:

- **Limit.** The 1-minute load average must be at or below `PERF_LOAD_LIMIT` (default 15) immediately before and immediately after each run.
- **Before a run.** The guard polls every 10 s for up to 10 min, or up to `PERF_LOAD_WAIT_MIN` minutes when that is set. If the load is still over the limit, it fails loudly.
- **After a run.** If the post-run load is over the limit, the run is discarded and re-run. After 3 retries the guard fails.
- **Recorded fields.** Each recorded run carries `load_before`, `load_after` and `load_discarded`. The matrix summary carries `max_load_accepted`.

Timing taken without this guard is not evidence.

## Asset pipeline

The kit-piece pipeline (P1). Procedure, gates and pitfalls: the `blender-asset` skill, `.claude/skills/blender-asset/SKILL.md`. Targets marked local need Blender 5.2 (`BLENDER=`) or KTX-Software 4.4.2 (`make fetch-tools`); CI runs only the Rust gate, through `make test`.

| Target | What it does |
|---|---|
| `make blender-test [TEST=<module>]` | Local. The `scripts/blender/tests/` suite in headless Blender with `--python-exit-code 1` |
| `make blender-test-selfcheck` | Local. Proves a forced Blender test failure fails make |
| `make refs PIECE=<id>` | Higgsfield reference images into `assets-source/refs/<id>/` with `refs.json`; modeling waits for the owner's `approval.json` |
| `make kit-build PIECE=<id>` | Local. Runs `scripts/blender/kit/<id>.py` into `target/kit-stage/<id>/` and copies its bake and export reports to `assets-source/kit/<id>/` |
| `make kit-determinism PIECE=<id>` | Local. Two fresh exports (bakes skipped) must give the same GLB SHA-256 |
| `make kit-material SET=<set>` | Local. Builds and compresses one tileable PBR set (`scripts/blender/kit/mat_<set>.py`) into `assets/kit/p1/materials/<set>/` |
| `make kit-compress PIECE=<id> [CLASS=hero]` | Local. Meshopt GLB, UASTC to BC7/ASTC KTX2 + zstd, PNG fallback and per-class byte budgets into `assets/kit/p1/<id>/` |
| `make kit-compress-test` | Local. The `kit-compress` and `kit-provenance` node tests |
| `make kit-renders PIECE=<id>` | Local. Blender socket round-trip check, then the six-view JPEG evidence into `assets-source/kit/<id>/renders/` |
| `make kit-provenance PIECE=<id> REFS=assets-source/refs/<ref> [ROUTE=interactive]` | Local. Writes `assets-source/kit/<id>/provenance.json`, runs the gate and the renders, then stamps `validation.pass` |
| `make kit-material-provenance SET=<set>` | Writes and validates `assets-source/kit/materials/<set>/provenance.json` |
| `make kit-check` | The Rust kit gate alone (`cargo test -p hex-render --test kit_gate`); `make test` runs it too |
| `make kit-fixture` | Local. Rebuilds the committed gate fixture and re-syncs the served copy in `assets/kit/p1/fixture/` |
| `make fetch-hdri` | Downloads the gitignored look-dev HDRI into `assets-source/hdri/` and verifies the SHA-256 in `assets-source/lookdev/lookdev.json` |
