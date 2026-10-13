# Agent Operability

Hex World can be driven, observed, screenshotted and exited from the command line, natively and in the browser, with no OS screen-recording permission and no human. Screenshots are captured inside the app with Bevy's `Screenshot::primary_window()`, so they need only a window the app itself opens.

## Native CLI flags

Flags take the form `--name value`. A bare `--name` means `1`. Bad values log a warning and fall back to the default.

| Flag | Meaning | Example |
|---|---|---|
| `--scenario <name>` | Start scenario: `single` (default) or `gate2000` | `--scenario gate2000` |
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

Every exit path waits until pending screenshots have been written. The gate keys (`layout`, `chunk`, `post`, `shadows`, `shadow_casters`, `cascades`, `shadow_dist`) are read on every scene build, so a `scenario` command keeps them. Unknown values warn and fall back to the default.

## Web query parameters

`shot`, `scenario`, `view`, `diag`, `exit-after`, `seed`, `no-anim`, `layout`, `chunk`, `post`, `shadows`, `shadow_casters`, `cascades`, `shadow_dist`, `fps`, with the same meanings as the flags. Example: `http://127.0.0.1:8094/?scenario=gate2000&view=closeup&diag=1`.

`wgsl_compat` (`0` or `1`) controls `web/wgsl-compat.js`, which inlines the WGSL `let` texture and sampler handle aliases that browsers without the `texture_and_sampler_let` language extension (Safari 27) reject. `1` forces the rewrite on, `0` turns it off, and by default it is active only when the browser lacks the extension. `window.__wgslCompat` is set before the app loads (absent when WebGPU is unavailable) and holds `{ active, native, modules, rewritten, aliases, unhandled, ms }`: `active` is whether the rewrite is on, `native` whether the browser supports the extension, `modules` and `rewritten` count the shader modules seen and changed, `aliases` the aliases inlined, `unhandled` the bindless aliases left alone, and `ms` the time spent rewriting.

## JavaScript API (`window.hexWorld`)

Defined after the `TrunkApplicationStarted` event.

| Call | Returns | Example |
|---|---|---|
| `ready()` | `true` once the scene has rendered 120 frames | `await page.waitForFunction(() => hexWorld?.ready())` |
| `state()` | State object | `hexWorld.state().scenario` |
| `dumpState()` | State object: scenario, tiles, frames, seed, no_anim, camera `pos`/`forward`, queued command count, and `diag` (the latest diagnostics object, also written by native `--dump-state`) | `hexWorld.dumpState().camera.pos` |
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
| `scenario` | Despawn the scene and rebuild it as `single` or `gate2000` | `{"cmd":"scenario","name":"gate2000"}` |
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
| `make web-smoke` | Browser smoke test: WebGPU adapter (not SwiftShader unless `PW_GPU=swiftshader`), bad-scenario fallback, no-GPU redirect |

`make shots` deletes every `target/shots/gallery-*.png` first, including the `gallery-chunks-*` and `gallery-instanced-*` galleries. Run it before `make shots-chunks` and `make shots-instanced`.

### Host-load rule for timing runs

`e2e/perf-load.mjs` guards every timing run in `perf-gate.mjs` and `perf-matrix.mjs`, on web and native:

- **Limit.** The 1-minute load average must be at or below `PERF_LOAD_LIMIT` (default 15) immediately before and immediately after each run.
- **Before a run.** The guard polls every 10 s for up to 10 min, or up to `PERF_LOAD_WAIT_MIN` minutes when that is set. If the load is still over the limit, it fails loudly.
- **After a run.** If the post-run load is over the limit, the run is discarded and re-run. After 3 retries the guard fails.
- **Recorded fields.** Each recorded run carries `load_before`, `load_after` and `load_discarded`. The matrix summary carries `max_load_accepted`.

Timing taken without this guard is not evidence.
