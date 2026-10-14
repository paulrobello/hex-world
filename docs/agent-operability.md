# Agent Operability

Hex World can be driven, observed, screenshotted and exited from the command line, natively and in the browser, with no OS screen-recording permission and no human. Screenshots are captured inside the app with Bevy's `Screenshot::primary_window()`, so they need only a window the app itself opens.

## Native CLI flags

Flags take the form `--name value`. A bare `--name` means `1`. Bad values log a warning and fall back to the default.

| Flag | Meaning | Example |
|---|---|---|
| `--scenario <name>` | Start scenario: `single` (default), `gate2000`, `kit` (the P1 kit family from `assets/kit/manifest.json`; missing pieces warn and are skipped), `water` (the hero-water look-dev patch: 12 hex cells with an ocean shelf, a beach and a river with a bend, `crates/hex-render/src/water.rs`), or `play` (the playable world: placement, undo/redo and bulldoze; see [Play scenario](#play-scenario)) | `--scenario play` |
| `--kit_hooks <none\|snow\|wet\|night>` | Kit scenario only. Drives the §9.4 `masks` hook uniforms (snow, wetness, night emissive) for verification shots. Default `none` (noon) | `--kit_hooks snow` |
| `--kit_src <runtime\|fixture\|fixture_tiled>` | Kit scenario only. `runtime` (default) lays out the P1 family from `assets/kit/manifest.json`; `fixture` lays out three copies of the kit gate fixture (manifest `crates/hex-render/tests/fixtures/kit/manifest.json`, files served from `assets/kit/p1/fixture/`, kept byte-identical by `kit_scene::tests::served_fixture_matches_the_gate_fixture`); `fixture_tiled` is the same layout with the cap bound to a probe tileable set (`assets/kit/p1/fixture_tile/`, constant maps copied from the fixture masks), so the tileable forward and prepass shader branches run (web-smoke uses it) | `--kit_src fixture` |
| `--tonemap <agx\|tony\|aces>` | Kit, water and play scenarios only. Camera tone mapping (D4): `aces` (default, ACES fitted, owner look-dev ruling 2026-10-13), `agx` (AgX) or `tony` (TonyMcMapface). Other scenarios keep the P0 camera | `--tonemap aces` |
| `--ibl <on\|off>` | Kit, water and play scenarios only. `on` (default) adds the prefiltered noon `EnvironmentMapLight` (`kit://p1/ibl/noon.{diffuse,specular}.ktx2`, intensity `lookdev::IBL_INTENSITY`) and sets the global ambient light to 0; `off` keeps the flat 400 ambient. With IBL on the kit forward pipeline samples 16 textures per fragment stage, exactly WebGPU's default limit (14 with `off`) | `--ibl off` |
| `--ev100 <f32>` | Kit, water and play scenarios only. Kit camera exposure (EV100, 0–20, default `lookdev::KIT_EV100` = 12.5, chosen from an exposure sweep and owner-approved 2026-10-13; Bevy's default 9.7 clips the 30,000 lx sun). The kit, water and play scenarios also aim their sun along the HDRI sun direction `lookdev::KIT_SUN_DIR` = (0.553, 0.742, 0.379); `single` and `gate2000` keep the P0 light | `--ev100 13.3` |
| `--tilt <auto\|on\|off>`, `--tilt_strength <0..1>` | Tilt-shift post pass (PRD §9.1, `hex_render::tilt_shift`): a sharp horizontal band at the camera's look-at point on the ground, blur growing with screen distance from the band and with depth difference, plus a small saturation lift. Fades out as the focus distance drops from 48 m to 12 m. `auto` (default) is on from the Medium tier up (8 taps Medium, 16 High, 24 Ultra, off on Low) and only in the kit, water and play scenarios with `post=full`, so P0 baselines are unchanged (in `play` this is the game camera's default-on tilt-shift, PRD §15 P2); `on`/`off` force it in any scenario. `T` toggles it at runtime. One pass, 2 sampled textures in the fragment stage (colour and prepass depth). `make shots-tilt` writes the off/on pair to `docs/shots/tilt-shift/` | `--tilt off` |
| `--tier <low\|medium\|high\|ultra>` | Quality tier (PRD §9.2), default `high`. The water scenario picks its variant from it: `low` has no refraction (alpha-blended, no transmission texture, no caustics) and 4 Gerstner waves; `medium` refracts with one caustic layer and 6 waves; `high`/`ultra` add the second caustic layer, the screen-space reflection march and 8 waves | `--tier medium` |
| `--water_clock <real\|fixed>` | Water and play scenarios. `fixed` advances water time by exactly 1/30 s per frame (motion captures); `real` (default) follows the frame clock | `--water_clock fixed` |
| `--clock <real\|fixed>` | `fixed` advances every Bevy clock (real, virtual and fixed) by exactly 1/60 s per frame (`TimeUpdateStrategy::ManualDuration`), so the 20 Hz car tick lands on every third frame and a scripted run replays identically; wall-clock flags such as `--exit-after-seconds` then count these 1/60 s frames. `real` (default) follows the system clock | `--clock fixed` |
| `--view <name>` | Gate camera view: `strategic` (default, whole world from 900 m) or `closeup` (40 m above tile (3, 2)). Unknown values warn and fall back | `--view closeup` |
| `--shot <name>` | Select the scenario and save `target/shots/<name>.png` at frame 120 | `--shot single` |
| `--exit-after <frames>` | Exit after N frames | `--exit-after 180` |
| `--exit-after-seconds <s>` | Exit after N wall-clock seconds | `--exit-after-seconds 10` |
| `--screenshot-after <s>` | Save one screenshot after N seconds, named by `--shot` or `screenshot-after` | `--screenshot-after 2` |
| `--script <file.json>` | Run a command list (see below). A missing or invalid file exits with code 2 before the window opens | `--script scripts/shots/gallery.json` |
| `--thumbs` | Native only. Renders the Legend inventory's tile thumbnails and exits (`crates/hex-app/src/thumbs.rs`, PRD §5.3). Forces the play scenario at 384x384 with `--post none --tilt off --fps off --clock fixed`, a magenta clear colour, no deband dither and the boids hidden, then shows each catalog tile alone at the origin by writing `PlayView`, so `hex_render::play` draws its kit pieces or placeholders exactly as in the game. One fixed camera on the play start camera's side (pitch 58°, 20° field of view, edge 0 on the right) frames every tile at one scale. It requests every kit pack and starts once all of them have loaded. Each tile is shown once to warm up pipelines and textures; then each distinct rotation (rotations with equal world-edge sockets share one image) is captured at a fixed frame to `target/shots/thumbs/<tile>-r<k>.png`, with the mapping in `target/shots/thumbs/plan.json`. It overrides `--scenario`, `--size`, `--post`, `--tilt`, `--fps`, `--clock`, `--shot`, `--script` and the exit and screenshot timers; look-dev and quality keys such as `--tonemap`, `--ibl`, `--ev100`, `--shadows`, `--cascades`, `--tier` and `--seed` pass through and change the images, so `make thumbs` passes none. Use `make thumbs` | `--thumbs` |
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

Every exit path waits until pending screenshots have been written. The gate keys (`layout`, `chunk`, `post`, `shadows`, `shadow_casters`, `cascades`, `shadow_dist`, `kit_hooks`, `kit_src`, `tonemap`, `ibl`, `ev100`, `tilt`, `tilt_strength`, `tier`, `water_clock`) are read on every scene build, so a `scenario` command keeps them. Unknown values warn and fall back to the default.

## Web query parameters

`shot`, `scenario`, `view`, `diag`, `exit-after`, `seed`, `no-anim`, `layout`, `chunk`, `post`, `shadows`, `shadow_casters`, `cascades`, `shadow_dist`, `fps`, `kit_hooks`, `kit_src`, `tonemap`, `ibl`, `ev100`, `tilt`, `tilt_strength`, `tier`, `water_clock`, `clock`, with the same meanings as the flags. Example: `http://127.0.0.1:8094/?scenario=gate2000&view=closeup&diag=1`.

`wgsl_compat` (`0` or `1`) controls `web/wgsl-compat.js`, which inlines the WGSL `let` texture and sampler handle aliases that browsers without the `texture_and_sampler_let` language extension (Safari 27) reject. `1` forces the rewrite on, `0` turns it off, and by default it is active only when the browser lacks the extension. `window.__wgslCompat` is set before the app loads (absent when WebGPU is unavailable) and holds `{ active, native, modules, rewritten, aliases, unhandled, ms }`: `active` is whether the rewrite is on, `native` whether the browser supports the extension, `modules` and `rewritten` count the shader modules seen and changed, `aliases` the aliases inlined, `unhandled` the bindless aliases left alone, and `ms` the time spent rewriting.

## JavaScript API (`window.hexWorld`)

Defined after the `TrunkApplicationStarted` event.

| Call | Returns | Example |
|---|---|---|
| `ready()` | `true` once the scene has rendered 120 frames | `await page.waitForFunction(() => hexWorld?.ready())` |
| `state()` | State object | `hexWorld.state().scenario` |
| `dumpState()` | State object: scenario, tiles (hex cells laid out; for `kit` it follows `kit_src`), frames, seed, no_anim, `gpu` (`{ maxSampledTexturesPerShaderStage }` from the render device, `null` until it exists; also logged once at startup as `gpu: max_sampled_textures_per_shader_stage=N`), camera `pos`/`forward`, queued command count, and `diag` (the latest diagnostics object, also written by native `--dump-state`) | `hexWorld.dumpState().camera.pos` |
| `diag()` | Diagnostics object, refreshed every 30 frames: `p50`/`p95`/`p99`/`mean`/`max` frame ms and `dropped_over_25ms` (frames slower than 25 ms) over the last 600 frames from `window_start_frame` (the frame after READY) on, `layout`, `chunk_size` and `chunks` (0 for entities), `shadows` (read from the light), `shadow_casters` (the config), `cascades` and `shadow_dist` (read from the light's `CascadeShadowConfig`), `passes` (per view and phase, I4: the camera's `opaque` and `prepass` phases and each `cascade<i>` `shadow` phase, with `multidraw_sets` (one multi-draw-indirect call each, native), `batchable_bins` (one instanced draw per distinct mesh+material), `unbatchable` and `non_mesh` (the instanced layout's 15 slots, empty ones skip their draw); sampled outside the first window), `instanced` (instanced layout only, else `null`: per view the non-empty `draws`, `instances` and `tris` it issued, and over all views `union_instances_by_lod` and `union_tris`), `tiles` (tile entities plus the hexes held by chunks), `entities` (`Mesh3d` count), `pieces` (logical static pieces from the scene build), `visible_entities`, `visible_mesh_material_pairs` (distinct pairs, the most draws left after instancing), `visible_tris` and `visible_by_lod` (all from `ViewVisibility`, which counts the camera and the shadow cascades; sampled every 30 frames except during the first 600-frame window), `wasm_memory_bytes` (wasm linear memory, 0 natively), `vehicles` (the gate scenario's moving boxes), `cars` (play-scenario car entities), `tris_lod0_per_tile` (counted from the spawned meshes), `materials`, `post` (TAA/bloom/DOF from the camera, SSAO from the render world), `ssao` (a compiled SSAO pipeline exists for a view), `gpu_preprocessing_supported` (device capability), `adapter`, `samples`, `frames`, `window_start_frame` | `hexWorld.diag().p95` |
| `cmd(json)` | Enqueues one command object or an array of them (string or object). Parse errors go to `console.error` | `hexWorld.cmd('{"cmd":"wait","frames":30}')` |
| `events()` | Drains the game event queue: an array of typed event objects, oldest first (see [Play scenario](#play-scenario)). The queue keeps the newest 256. The HUD overlay reads every batch through `watchEvents` without draining this queue, so callers still see every event | `hexWorld.events().filter(e => e.type === "tilePlaced")` |
| `watchEvents(cb)`, `pumpEvents()` | For the HUD overlay: `pumpEvents()` moves the wasm queue into the queue `events()` drains and calls each `watchEvents` callback with the new batch | `hexWorld.watchEvents(b => console.log(b))` |
| `cellRect(q, r)` | `[left, top, right, bottom]` in CSS pixels around cell `(q, r)` (its hex at ground and 2 m), from the camera of the last frame; `null` before the camera exists or when the cell is behind it. The HUD anchors its tags with it, and the UI e2e aims the mouse with it | `hexWorld.cellRect(1, 0)` |
| `selectTile(id)`, `hover(q, r)`, `rotateGhost(dir)`, `placeTile(q, r)`, `undo()`, `redo()`, `bulldoze(q, r)`, `setTool(mode)`, `toggle(what)` | Typed wrappers over `cmd` for the play commands below, queued like any command | `hexWorld.placeTile(1, 0)` |
| `screenshot(name?, timeoutMs = 10000)` | Promise of a PNG data URL. The capture is queued behind earlier commands | `await hexWorld.screenshot("view")` |

## Commands

A script is a JSON array. Commands run in order, one per frame, starting once the scene is ready (frame 120). A bad command at run time is logged and skipped.

| Command | Effect | Example |
|---|---|---|
| `camera` | Move the camera to `pos`, looking at `look_at` (metres; a hex has a 16 m radius) | `{"cmd":"camera","pos":[40,30,40],"look_at":[0,2,0]}` |
| `wait` | Hold the queue for N frames | `{"cmd":"wait","frames":60}` |
| `screenshot` | Native: `target/shots/<name>.png`. Web: data URL for `hex_world_take_shot` | `{"cmd":"screenshot","name":"overview"}` |
| `scenario` | Despawn the scene and rebuild it as `single`, `gate2000`, `kit` or `water` | `{"cmd":"scenario","name":"gate2000"}` |
| `tilt` | Runtime tilt-shift toggle and strength; either field may be omitted. The state JSON reports it under `tilt` | `{"cmd":"tilt","on":false,"strength":0.5}` |
| `dump_state` | Log the state JSON; natively also write `target/shots/<name>.json` | `{"cmd":"dump_state","name":"after-move"}` |
| `exit` | Exit once pending screenshots are written (native) | `{"cmd":"exit"}` |
| `select_tile` | Play: select the tile type `id` (a `data/tiles/` id). Unknown ids are refused at run time | `{"cmd":"select_tile","id":"road_straight"}` |
| `hover` | Play: hover cell `(q, r)` as the pointer does. Hovering a slot auto-rotates the ghost to its first legal rotation | `{"cmd":"hover","q":1,"r":0}` |
| `rotate` | Play: cycle the ghost through its legal rotations, `cw` (R) or `ccw` (Shift+R). With no legal rotation it still turns one step | `{"cmd":"rotate","dir":"cw"}` |
| `place` | Play: place the selected tile at `(q, r)` with the current ghost rotation. A cell other than the hovered one is hovered first, which auto-rotates | `{"cmd":"place","q":1,"r":0}` |
| `undo`, `redo` | Play: undo or redo the latest placement or bulldoze | `{"cmd":"undo"}` |
| `bulldoze` | Play: remove the tile at `(q, r)`. Refused for the last tile or when the world would split | `{"cmd":"bulldoze","q":1,"r":2}` |
| `set_tool` | Play: `place` or `bulldoze`. Left-click applies the tool at the hovered cell | `{"cmd":"set_tool","mode":"bulldoze"}` |
| `toggle` | Play: `bulldoze` (B), `inventory` (Tab) or `photo` (P) | `{"cmd":"toggle","what":"inventory"}` |
| `set_reduced_motion` | Reduced motion, for the overlay to send from `prefers-reduced-motion` or the Settings override (`hexWorld.cmd({...})`). On: the staged rise is an instant cut (every `riseStage` cue arrives at once with `skipped: true`), there is no dust, camera shake or sink, and every affordance fade, delay, breathing loop and pulse is an instant cut. The state JSON reports it as `play.reduced_motion` | `{"cmd":"set_reduced_motion","on":true}` |

The play commands parse into the same input actions as their key and mouse bindings, and they act only in the play scenario (elsewhere they are logged and ignored). Bad `dir`, `mode` or `what` values, or a missing `q`/`r`, fail at parse time like any malformed command.

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
| `make web-smoke` | Browser smoke test: WebGPU adapter (not SwiftShader unless `PW_GPU=swiftshader`), bad-scenario fallback, no-GPU redirect, the forced WGSL compat shim, and the kit scenario with `kit_src=fixture`: kit meshes must spawn and draw with no WebGPU validation error on the adapter's limits, with the device clamped to WebGPU's default 16 sampled textures per stage (`e2e/webgpu-probes.mjs` also counts each pipeline layout's fragment textures; the kit measures 14), and under the forced shim, where the kit fragment (its `masks_texture` global) must be the module rewritten; and the water scenario at High, Medium and Low clamped to 16 sampled textures (water and terrain pipeline counts pinned exactly) and under the forced shim; `water_kit` (water and kit pieces in one view, kit forward exactly 16 with the transmission texture); and `play` with river, beach and ocean tiles placed through `hexWorld`, clamped to 16 (kit forward exactly 16 with transmission, water and terrain at the water scenario's High counts); and kit streaming in `play` (see [Kit streaming](#kit-streaming)): core ready at READY, only core kit requests in the network log before any interaction, a placed forest tile requesting the forest and hero packs (held by `page.route` so it is caught on its placeholder, `e2e/test-results/smoke-streaming-placeholder.png`), then leaving `pack_waiting` with its rise running on to an unskipped `settled` cue (`smoke-streaming-kit.png`), and every other pack streaming in the background. It also checks that the HUD's loading layer is gone and no play HUD covers the Single scenario |
| `make shots-kit [ARGS="--kit_src fixture"]` | Native kit shots from `scripts/shots/kit.json` at 1600x1000: `target/shots/kit-{overview,seam-closeup,tree,tree-under,tree-game,lod1,lod2}.png` plus `kit-state.json`. The LOD shots sit about 226 m and 467 m from the tiles (LOD1 and LOD2 bands) |
| `make lookdev-ibl` | Blender (local only): runs `make fetch-hdri`, then bakes the gitignored Poly Haven HDRI `assets-source/hdri/kloofendal_48d_partly_cloudy_puresky_2k.hdr` into `assets/kit/p1/ibl/noon.specular.ktx2` (GGX-prefiltered RGBA16F cubemap, 256² with 9 mips) and `noon.diffuse.ktx2` (SH9 irradiance, 32²), records hashes in `assets-source/lookdev/lookdev.json`, then runs `ktx validate` on both |
| `make shots-lookdev [ARGS="--kit_src fixture"] [LOOKDEV_OUT=dir]` | Runs the kit shot script once per tone mapper and copies `lookdev-{agx,tony,aces}-{kit-overview,kit-seam-closeup,kit-tree}.png` into `LOOKDEV_OUT` (default `target/shots`). Check host load ≤ 15 first |
| `make web-shots-kit [QUERY="kit_src=fixture"]` | The same script in GPU Chromium, writes `e2e/test-results/web-kit-*.png`. `QUERY` is appended to the page URL |
| `make shots-water [ARGS="--tier medium"]` | Water owner checkpoint: native stills from `scripts/shots/water.json` at 1600x1000 (overview, ocean, river, beach, ocean and river seam close-ups) and a 3 s motion clip per view (90 frames at `--water_clock fixed`, 960x600, `scripts/shots/water-motion.mjs`), all copied or encoded (MP4 and GIF, ffmpeg) into `docs/checkpoints/water/` |
| `make shots-play [ARGS=...]` | Play checkpoint: runs `docs/checkpoints/play/play.json` natively at 1280x800 with `--tilt off` and copies JPEG stills and the state dumps to `docs/checkpoints/play/` (see [Play scenario](#play-scenario)) |
| `make shots-cars [ARGS=...]` | Cars checkpoint: runs `docs/checkpoints/cars/cars.json` natively at 1280x800 with `--tilt off --clock fixed`: two house clusters joined by a road chain with a 120° bend, cars driving across the tile seams (overview, close-up and seam stills), a refused bulldoze of the middle road, a filler meadow that makes it legal, the bulldoze that disconnects the houses (cars despawn, spawn rate 0) and its undo. Copies JPEG stills and the state dumps to `docs/checkpoints/cars/` (see [Cars](#cars)) |
| `make shots-rise [ARGS=...]` | Staged rise and Legend affordances: `docs/checkpoints/rise/rise.mjs` writes the script (a meadow, a house cluster, a river and an ocean rise captured on every frame, the fast-forward, undo-mid-rise and reduced-motion cases, and the affordance set), the app runs it at 1280x800 with `--tilt off --clock fixed --water_clock fixed`, and `encode.sh` writes a stage sheet and an MP4 per case, the fast-forward and undo clips and stills, the affordance stills and the state dumps to `docs/checkpoints/rise/`. Stage sheets read left to right, top to bottom at t = 0.1, 0.3, 0.5, 0.7, 0.9, 1.1, 1.3, 1.6 and 2.2 s |
| `make shots-water-play [ARGS=...]` | Water tiles in play: runs `docs/checkpoints/water-play/water-play.json` natively at 1600x1000 with `--tilt off --water_clock fixed` (a river chain of straight, bend, bridge and straight under different rotations; a coast of beaches, oceans and a meadow; overview, coast, river, seam and cut-face close-ups; bulldoze, undo and redo of an ocean tile with its neighbours' cut faces), copies JPEG stills and the state dumps to `docs/checkpoints/water-play/`, then encodes a 3 s motion clip per view (90 frames at 960x600, `docs/checkpoints/water-play/motion.mjs`) to MP4 |
| `make ui-check` | The HUD overlay's gates (part of `make checkall`): `ui-typecheck` (tsc), `ui-lint` (Biome), `ui-test` (`node --test` on `web/ui/test/*.test.ts`) and `ui-build`. See [UI overlay](#ui-overlay-webui) |
| `make ui-build` | Bundles `web/ui/src` with esbuild into `web/ui/dist/` (`hud.js`, `hud.css`, `fonts/`), baking in `assets/thumbs/manifest.json` when it exists. Trunk runs it as a `pre_build` hook, so `trunk build` and `trunk serve` always carry the current HUD |
| `make ui-schema` | Rewrites `web/ui/src/bridge-schema.json` and `web/ui/test/rules-parity.json` from the Rust enums and hex-core (`HEXW_UPDATE_SCHEMA=1 cargo test -p hex-app bridge::schema::schema_matches_web_ui`). Without the variable that test fails while either file is stale |
| `make ui-e2e` | Local only (GPU Chromium, never CI): builds the site, then `e2e/ui.mjs` drives the play HUD with the real keyboard and mouse (place, rotate, a refused place, undo, redo, a refused split bulldoze and a removal, Tab, H, the 1280 x 720 layout), asserts through `hexWorld.state()` and the event queue, fails on any console error, and writes `e2e/test-results/ui/<state>.jpg` named after the mockup states in `design/ui/legend/shots/` |
| `make thumbs` | Legend inventory thumbnails: runs `hex-world --thumbs`, then `scripts/thumbs/build.py` (uv, Pillow 12.2.0 pinned) keys the magenta background to alpha (translucent water seen against it is unmixed into partial alpha), box-filters each frame 4x in premultiplied space to 96x96 (the 48 CSS px `.row__glyph` at 2x) and writes `assets/thumbs/<tile>-r<k>.webp` (lossy WebP q85 with lossless alpha), `assets/thumbs/manifest.json` (`tiles[id][k]` is rotation `k`'s image) and `docs/checkpoints/thumbs/contact-sheet.png`. It fails if a tile touches the frame edge or a frame's background is not the key (a frame captured black: rerun). Black frames also come from the display sleeping while the user is idle: every capture of a 2026-10-13 coast run was black at host load 24 while `pmset -g assertions` showed `com.apple.powermanagement.delayDisplayOff`, and the same run under `caffeinate -d -u make shots-coast` captured every frame. Run unattended native captures (`thumbs`, `shots-*`) under `caffeinate -d -u`. Two runs give byte-identical files. `make initial-download` counts `assets/thumbs/`, since the inventory sheet is open at first paint. The data validator (`crates/hex-core/tests/data_validator.rs`) requires an existing image for every (tile, rotation) and that rotations share an image exactly when their world-edge sockets are equal, so rerun it after changing a tile's sockets or kit pieces |
| `make shots-coast [ARGS=...]` | Coast checkpoint for the directional beach sockets (owner O1): runs `docs/checkpoints/coast/coast.json` natively at 1600x1000 under `caffeinate -d -u` with `--tilt off --clock fixed --water_clock fixed`. An island of four meadows is closed by 12 coast tiles (2 straight beaches, 8 headlands, 2 bays, under 10 tile-rotation pairs) inside a ring of 18 ocean tiles. Before that, three placements are refused and dumped: ocean against a beach edge, grass against a beach edge, and a beach edge against grass. The run writes an overview, headland and bay views, and straight-to-headland seam close-ups (oblique and top-down), and the last headland 59 frames into its rise with its water filling from the base (`coast-08-rise-headland-fill`), and copies JPEG stills and the state dumps to `docs/checkpoints/coast/`. `water::tiles` test `coast_proof_script_closes_an_island` replays the script on the rules |
| `make initial-download` | PRD §10 gate, run by `make checkall-ci` after `web-size` (`scripts/size/initial-download.mjs`; builds `dist/` only when missing). Sums what Pages serves before the first interaction, per texture family: wasm, JS glue, the HTML and web shell, the `web/ui` overlay when built, shaders, the kit manifest, the IBL, the p0 tile, the `core` pack's pieces, maps and sets, and the tile thumbnails. It fails when the larger of BC7 and ASTC is over 30 MiB (PNG is report only), prints each streamed pack's bytes per family (with sets it loads from another pack), and exits 2 when a manifest piece or set has no `pack` or a core piece samples a non-core set. `INITIAL_DOWNLOAD_BUDGET_BYTES` overrides the budget for a self-test |
| `make perf-water` | Water frame time at 2560x1440: web (uncapped GPU Chromium) and native (`--present immediate`), per tier (`WATER_TIERS`, default `high,low`) and view (start camera, ocean close-up), each run inside the host-load guard; writes `docs/perf/p2-water.json` |

`make shots` deletes every `target/shots/gallery-*.png` first, including the `gallery-chunks-*` and `gallery-instanced-*` galleries. Run it before `make shots-chunks` and `make shots-instanced`.

### Host-load rule for timing runs

`e2e/perf-load.mjs` guards every timing run in `perf-gate.mjs` and `perf-matrix.mjs`, on web and native:

- **Limit.** The 1-minute load average must be at or below `PERF_LOAD_LIMIT` (default 15) immediately before and immediately after each run.
- **Before a run.** The guard polls every 10 s for up to 10 min, or up to `PERF_LOAD_WAIT_MIN` minutes when that is set. If the load is still over the limit, it fails loudly.
- **After a run.** If the post-run load is over the limit, the run is discarded and re-run. After 3 retries the guard fails.
- **Recorded fields.** Each recorded run carries `load_before`, `load_after` and `load_discarded`. The matrix summary carries `max_load_accepted`.

Timing taken without this guard is not evidence.

## Play scenario

`--scenario play` draws the game world held by `hex-app` (`crates/hex-app/src/game.rs`). It opens with one meadow at `(0, 0)` and follows the PRD §4.1–4.2 rules from `hex-core`. Tiles use kit pieces from `assets/kit/manifest.json` where the tile references them and the piece's pack has loaded (see [Kit streaming](#kit-streaming)), and plain placeholders otherwise: a coloured prism per biome, dark road and blue river strips meeting at a hub, and beige blocks for pending houses, pads and hero props.

The in-world affordances are the Legend direction's (`design/ui/legend/in-world.md`, `crates/hex-render/src/play/affordances.rs`):

- every free slot: a neutral 2 px outline dashed 6 on 7 off at ground height, fading out (160 ms) in bulldoze and photo mode
- the hovered slot: an ink outline at 90% and a 4% ink fill, crossfading in 120 ms
- the ghost, 150 ms after a slot is hovered: a ghost-fill cap breathing 100% to 55% over 2.4 s, a 2 px ghost-line outline dashed 10 on 8 off, and road (18 px) and river (22 px) lane strokes at 60% standing in for the tile's meshes
- edge glow on the ghost's rim: compatible 7 px solid with two halos, incompatible 7 px dashed 14 on 9 off with two halos and a 14 px cross tick, no neighbour 3 px neutral; drawn emissive (×2.5) for bloom. The glow fades in over 120 ms whenever the ghost changes (rotate, retarget)
- socket key marks: on each touched edge, a 4 px line in the neighbour's socket colour along the middle 40% of the edge, 11 px inside the slot
- a refused placement or bulldoze pulses the incompatible glow or the blocked outline once over 240 ms (core 7 to 13 px)
- bulldoze: a removable tile gets a 4 px solid incompatible outline with an 8 px halo and a 12% fill; a tile whose removal would split the world gets a 4 px incompatible outline dashed 10 on 8 off, and the tiles it would cut off (every part but the largest) a 45° ink hatch every 10 px over sheet blue at 35% with a 2 px dotted ink outline; the last tile gets the dashed outline alone
- a placement flashes the slot's rim in ink (×4) for 80 ms

Widths are screen-space pixels at a device pixel ratio of 1 (multiplied by the window's scale factor), constant at every zoom: each line segment is a quad whose vertex shader (`assets/shaders/affordance.wesl`) projects both endpoints and pushes the corners out along the screen-space normal by the half-width in pixels. Dashes, the key marks' inset, the ticks and the hatch are in pixels too. The material binds no texture and draws without a depth test, over the world.

Water tiles (river, the three coast tiles `beach`, `beach_outer` and `beach_inner`, and ocean) draw the approved water material (`crates/hex-render/src/water/tiles.rs`): a hex water surface on ocean and coast tiles, a 10 m strip along each river segment, and the underwater cut face on every water edge with no matching water across it, rebuilt on place, bulldoze, undo and redo (each rebuild logs `play water: (q, r) <tile> r<rotation> exposed [...]`). Until a water tile's centre piece is in the kit, it gets a placeholder bed (river channel, beach slope, ocean floor) instead of its base and river strips. The levels and profiles are the bed contract in `docs/kit/water-contract.md`. The play camera always carries the depth prepass and, from Medium up, screen-space transmission, and has no depth of field.

A placed tile plays the staged rise of PRD §4.4 from its `TileDef.rise` timeline (`crates/hex-render/src/play/rise.rs`; every MVP tile uses the default 2.0 s: column 0–0.6 s, cap 0.45–0.85 s, edges 0.8–1.2 s, props 1.1–1.7 s, agents 1.6–2.0 s):

1. Column: the tile root rises from 4.6 m below rest (ease-out) with a dust puff around the rim and a camera micro-shake (about 3 px, decaying, added before transform propagation and removed at the start of the next frame, so input, picking and the `camera` command never see it). Water beds rise with the column.
2. Cap: the column overshoots by up to 0.35 m and settles, with a slight dip.
3. Edges: centre and edge pieces (roads, pads, lane strips) snap in (ease-out-back), and water surfaces and cut faces fill from the tile base up to the water level.
4. Props: hero props grow in a staggered order, trees by scale, buildings floor by floor (two steps for a cottage, three for the two-storey).
5. Agents: the car sim mirrors the tile only from this stage (see [Cars](#cars)).

A new placement fast-forwards every running rise to its end state. Undo or bulldoze takes the tile down: it sinks 4.6 m over 400 ms (ease-in) with a smaller dust puff, its water going with it, then despawns; placing on that cell again replaces the sinking tile at once. Rebuilds (the core pack arriving, a switch into the scenario) draw every tile at rest; a streamed pack arriving swaps its tiles to the kit look in place, mid-rise included (see [Kit streaming](#kit-streaming)). Photo mode is only a flag so far: it hides the ghost and the slot ring.

### Kit streaming

Kit content loads by pack (PRD §10, `crates/hex-render/src/play/packs.rs`). Every piece and tileable set in `assets/kit/manifest.json` names its `pack`, one of `hex_assets::PACKS` in stream order:

| Pack | Pieces and sets | Why |
|---|---|---|
| `core` | meadow_base, the five road pieces; grass, soil, asphalt | The start meadow and its sets, and the road family every build starts from. Loaded on entry, before anything else |
| `props` | rock_small, wildflower_clump | Scatter props: play does not draw scatter yet, so the first screen never shows them. Streamed first |
| `forest` | forest_base, tree_forest; litter, bark | The forest tile and its spruce |
| `hero` | tree_hero_oak (budget class hero) | The forest tile's hero oak (also loads bark) |
| `village` | house_pad, house_cottage, house_twostorey; plaster | The house cluster (also loads gravel for the driveway) |
| `water` | ocean_base, ocean_center, river_edge, river_straight_base, river_straight_center, river_bend_base, river_bend_center, river_road_bridge_center; gravel | Ocean and river pieces; gravel is their seabed and river bed (also loads plaster for the bridge stone) |

- **Core.** The play scenario requests `core` on entry and draws the world once it has loaded. A core piece may sample only core sets (`parse_manifest` rejects it otherwise), and `kit_gate` requires a pack on every runtime entry, core for everything the start tile draws, and no hero-class piece in core.
- **On demand.** A pack is requested the moment a tile that draws one of its pieces is selected (inventory, number keys, `select_tile`; `hex-app` hands the selection over in `PackRequests`) or in the view: placed (also by undo, redo or a script) or previewed as the ghost.
- **Background.** After the first interaction (selecting another tile, or the first change to the view after the start world, such as a ghost or a placement; camera moves alone do not count), the remaining packs stream one at a time in `PACKS` order. Nothing streams before it, so the initial download is core alone (`make initial-download`).
- **Loading.** A pack fetches its pieces' GLBs, every map their slots sample (sets of other packs included) and the sets it owns, all at once. It is `ready` only when every file has loaded, so a tile goes from placeholder straight to the textured kit look.
- **Placeholders and the swap.** Until its pack arrives a tile draws the play placeholders (biome prism, lane strips and hub, prop blocks, the water placeholder bed). When the pack arrives the tile's children are respawned with the kit pieces under the same root, so a running rise keeps its pose and clock; water cells whose centre piece arrived are respawned by `water::tiles::sync`.
- **Status.** The state JSON reports `packs` and `pack_waiting` (see [State](#state)), and the log prints `play: pack <id> requested` and `play: pack <id> ready`.
- **Exceptions.** The `kit` and `water_kit` scenarios still load every piece they lay out at once, since they are gates. `--thumbs` requests every pack and waits for all of them.
- **Hold (native only).** `HEXW_PACK_HOLD_FRAMES=<n>` holds every streamed pack for `n` frames after its request before fetching it, so a capture can catch a tile on its placeholder. Disk loads otherwise land within a few frames. The streaming checkpoint uses it: `HEXW_PACK_HOLD_FRAMES=45 cargo run -p hex-app --release -- --scenario play --script docs/checkpoints/streaming/streaming.json --size 1280x800 --seed 1 --fps off --tilt off --clock fixed --water_clock fixed`. That run writes `target/shots/streaming-*.png` and `streaming-state-*.json`; their JPEG and JSON copies are in `docs/checkpoints/streaming/`.

### Controls (input-action layer)

Every input becomes an action (`crates/hex-app/src/actions.rs`), and scripts and the bridge enqueue the same actions.

| Action | Binding |
|---|---|
| Orbit | Right-drag |
| Pan | Middle-drag, WASD or arrow keys |
| Zoom | Wheel (12 m to 2,500 m from the focus point) |
| Primary (place, or bulldoze in bulldoze mode) | Left-click |
| Rotate ghost | R (clockwise) / Shift+R, or Ctrl+wheel (one step per notch) |
| Undo / redo | Ctrl/Cmd+Z / Ctrl/Cmd+Shift+Z |
| Bulldoze tool toggle | B |
| Inventory toggle | Tab (the `inventory_open` flag; the HUD's Tiles sheet follows it) |
| Photo mode toggle | P (native only for now: the HUD swallows P until photo mode has a screen) |
| Select tile type | 1–9, 0, -, = pick inventory tiles 1–12 in catalog order natively; in the browser the HUD maps 1–9 to its "Fits here" order |
| Hover | Pointer over the ground plane (y = 0) |

In the browser the HUD overlay owns R, Shift+R, B, Tab, P, H, F6, Esc, Enter, the digit keys and Ctrl/Cmd+Z: it catches them on the window in the capture phase, before the canvas, and sends the matching bridge command, so one press acts once. Camera keys (WASD, arrows), F3 and T still reach the engine, and clicking a HUD control keeps keyboard focus on the canvas.

The camera rig keeps no state. Each orbit, pan or zoom derives its focus point from where the view centre meets the ground, so the `camera` command keeps working. Pointer hover is emitted only when the cell under the pointer changes, so a scripted `hover` holds while the cursor rests over the window. F3 (FPS counter) and T (tilt-shift) stay direct key reads in `hex-render`, because they are debug and look-dev toggles rather than §4.5 player actions.

### State

In the play scenario the state JSON's `tiles` is the world's tile count, and `play` holds the game (it is `null` in other scenarios). `events_pending` counts undrained events. Two top-level fields report kit streaming in play (both `null` elsewhere): `packs` maps every pack id to `idle` (not requested), `loading` (requested, fetching or held) or `ready`, and `pack_waiting` lists the placed tiles drawn with a placeholder until packs arrive, as `[{q, r, tile, packs}]`.

| Field | Meaning |
|---|---|
| `tool` | `place` or `bulldoze` |
| `selected`, `rotation` | Selected tile id and ghost rotation (0–5) |
| `inventory_open`, `photo_mode` | Toggle flags |
| `tiles` | `[{q, r, tile, rotation}]` |
| `frontier` | Free cells next to the world, `[{q, r}]` |
| `hovered`, `hovered_kind` | `{q, r}` or `null`, and `slot`, `tile` or `empty` |
| `ghost` | At a hovered slot in place mode: `{q, r, tile, rotation, valid, legal_rotations, edges}`, where `edges` holds six `Compatible`/`Incompatible`/`Neutral` values by world edge. Otherwise `null` |
| `inventory` | Tile ids with a legal rotation at the hovered slot |
| `bulldoze` | In bulldoze mode over a tile: `{q, r, allowed, reason}` |
| `can_undo`, `can_redo` | History availability |
| `last_result` | Latest game action except hover: `{action, ok, cell, reason}` |
| `recent_events` | The last 16 events, not drained, for native runs |
| `cars` | Live cars in the simulation |
| `connected_destinations` | Houses that can reach at least one other house by road |
| `spawn_rate_hz` | Cars spawned per second while under the cap: `connected_destinations` × 0.05 |
| `rising` | Running rises: `[{q, r, tile, stage, t}]`, `stage` one of `column`, `cap`, `edges`, `props`, `agents`, `t` seconds since the placement |
| `reduced_motion` | The `set_reduced_motion` flag |

### Events

Each event is an object whose `type` names it:

| Type | Fields |
|---|---|
| `tilePlaced` | `q, r, tile, rotation` |
| `placementRefused` | `q, r, tile, rotation, reason` |
| `tileBulldozed` | `q, r, tile` |
| `bulldozeRefused` | `q, r, reason` |
| `undone`, `redone` | none |
| `tileSelected` | `tile` |
| `ghostRotated` | `rotation, valid` |
| `hoverChanged` | `cell` (`{q, r}` or `null`) |
| `toolChanged` | `mode` |
| `inventoryToggled` | `open` |
| `photoModeToggled` | `on` |
| `actionRefused` | `action, reason`, for any other refusal: an empty undo stack, an unknown tile id, a click with nothing hovered |
| `riseStage` | `q, r, tile, stage, skipped`: a staged-rise cue for the audio layer (the rumble) and the HUD. `stage` is `column`, `cap`, `edges`, `props`, `agents`, then `settled`, or `sink` when an undo or a bulldoze takes the tile down. `skipped` cues come together in one frame from a fast-forward or under reduced motion. Not kept in `recent_events` |

Natively each event is also logged as `event: {...}`.

### UI overlay (web/ui)

The player HUD is the Legend direction (`design/ui/legend/`, PRD D18 and §4.7) in vanilla TypeScript, an HTML layer over the canvas that changes the game only through bridge commands. `index.html` loads `web/ui/hud.css` and `web/ui/hud.js` and copies `web/` piecewise (`brand/`, `unsupported.html`, `wgsl-compat.js`, and `web/ui/dist` as `web/ui/`), so the overlay's sources and tooling never ship.

- **Loading.** The layer shows the stage (engine download, then the renderer warming until `ready()`), the bytes actually downloaded (Resource Timing), and a Retry panel on a failed wasm download or after 120 s without `ready()`. It clears in every scenario. Only the play scenario gets the HUD; other scenarios poll the state once a second.
- **Per frame (play).** `pumpEvents()`, then `state().play`. The Tiles sheet's fit rules run in TypeScript from the catalog and socket table in `bridge-schema.json`; `web/ui/test/rules.test.ts` checks them against hex-core's legal rotations at every frontier slot of four scripted worlds (`rules-parity.json`: the play, water-play and coast checkpoints and a part-built coast). Undo and redo toasts name the action from a before/after diff of `play.tiles`.
- **Sticky slot.** The sheet keeps the last slot pointed at until another slot, Esc or a placement replaces it. While the pointer is off the canvas (over the HUD) the overlay re-sends `hover` for that slot, so R and the ghost keep working there.
- **Startup.** The engine's `inventory_open` starts `false` and the sheet is open by default, so the overlay sends one `toggle inventory` when it first sees the play state.
- **Reduced motion.** Once the engine is ready, in every scenario, the overlay sends `set_reduced_motion` with the OS `prefers-reduced-motion` value, and again whenever that setting changes. A scripted `set_reduced_motion` holds until the OS setting next changes. The HUD's own motion follows the same media query in CSS.
- **Type sync.** `web/ui/src/bridge.ts` types every command, event and play-state field; its field tables must equal `bridge-schema.json`, which the hex-app test `bridge::schema::schema_matches_web_ui` regenerates from the Rust enums. A Rust change fails `cargo test` until `make ui-schema` runs, then `make ui-test` until `bridge.ts` matches.
- **Not built yet.** Photo mode, Settings and Worlds: their bar entries are disabled with a tooltip, and the save state reads "Not saved" because autosave does not exist.

### Cars

`crates/hex-app/src/sim.rs` keeps a `hex_sim::Simulation` in step with the world: whenever the game state changes it diffs the world's tiles against the ones it last mirrored, so a removed tile is a bulldoze and a new or changed tile a place, whichever action caused it (place, bulldoze, the undo of either, redo). A tile edit despawns every car routed over that tile. The sim ticks at 20 Hz in Bevy's `FixedUpdate` (`Time<Fixed>` is set to 20 Hz; nothing else uses the fixed schedule) and only while the play scenario is shown. Each frame the car poses are interpolated between the last two ticks with `Time<Fixed>::overstep_fraction()` and drawn by `crates/hex-render/src/cars.rs`: one entity per car id, all sharing one placeholder mesh and material (a 4.2 × 1.5 × 1.8 m low-poly car, neutral body, dark cabin and roof) so they draw as one instanced batch, standing on the road crown at y = 0.11 m. On tiles whose road centre is still a placeholder strip (bends, house pads) the 0.5 m strip hides the wheels. The default spawn rate is 0.05 cars per second per connected house, up to 300 cars.

Agents wait for the rise: in the play scenario the mirror diffs against the world minus every tile whose rise has not reached its agents stage (`CarSim::mirror_gated`, `PlayRises::agents_ready`), so a road joins the car network 1.6 s after it lands, or at once when the next placement fast-forwards it. Removals still apply at once. `SimPlugin` orders the render's play systems between the view writer and the mirror, so the gate reads the same frame's rise state.

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
