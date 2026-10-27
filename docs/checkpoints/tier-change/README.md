# Quality-tier change stall (2026-10-27)

Card: "Shorten the tier-change stall: precompile tier pipelines, keep tiles across a tier change". The 2026-10-14 evidence beside this file (`stall-before.json`, `stall-after.json`, `tier-change-layer.jpg`) is the loading-panel work of `a6b3a1f`, measured on macdaddy.

## Summary

Measured on MiniMac (Apple M4, web, 208-tile world, 3 rounds per arm, loads 1.93-3.50). "Longest" is the longest frame from the pick; "drawn" is the pick to the engine reporting the change drawn on the GPU (`tier.rebuilding` false). Medians over rounds:

| Change | Before (cold) | After (cold) | Before (warm) | After (warm) |
|---|---|---|---|---|
| First High to Medium, longest | **9,701 ms** | **455 ms** | 694 ms | 41 ms |
| First High to Medium, drawn | 10,445 ms | 151 ms | 861 ms | 153 ms |
| Medium to High, longest | 556 ms | 43 ms | 180 ms | 41 ms |
| Repeated High to Medium, longest | 143 ms | 43 ms | 141 ms | 41 ms |
| Repeated High to Medium, drawn | 199 ms | 104 ms | 198 ms | 119 ms |
| Pipelines a change created | not reported | 0, 0, 0 | not reported | 0, 0, 0 |

- **Criterion 1** (before/after, first and repeated, load-guarded): met, table above and "Results".
- **Criterion 2** (a cold first change no longer spends seconds compiling on the pick): met. The cold first change to Medium drops from a 9.7 s frozen page (10.4 s until drawn) to a 455 ms longest frame (drawn in 151 ms), and it creates no pipeline. **The compile is moved, not removed.** A warm-up starts after READY whenever 60 frames pass without a new pipeline. That is not player idleness: it also fires while the world is being built and mid-play whenever a new piece or pack creates pipelines. On a cold Metal cache the four warm-ups of a session froze the page 12.0-12.9 s in total on MiniMac: 3.25 s right after READY (the start tile's pipelines), 1.55-2.41 s and 0.53-0.54 s while the world was built, and 6.68 s once the 208-tile world had settled. With a warm cache they cost 0.42-0.52 s in total. The saving is 9.7 s cold and 0.65 s warm, and only for a player who changes quality. READY itself does not move (below). This conflicts with "do not slow initial load" on a cold cache, and it is the owner's call (PRD §9.2): `tier_warm` defaulting off keeps the in-place gains but leaves criterion 2 unmet.
- **Criterion 3** (tiles survive a tier change, with a test): met. `tier::tests::a_play_tier_change_keeps_every_tile_and_matches_a_fresh_scene` keeps every tile root and child across High, Medium and Low; in a native run of the 59-tile world, no water cell was respawned after `set_tier` (0 `play water:` spawn lines after the change, 71 before it). The in-place change alone (`after_nowarm`, the warm-up off) cuts a repeated High to Medium from 141 ms to 40 ms longest (198 to 114 ms drawn, warm cache).

Not covered, by design or for lack of time:

- Low is never warmed: Medium does not warm Low, so a change to Low still compiles on the pick (High and Ultra warm Medium, Medium warms High, Low warms Medium).
- A session that starts at Medium has no grass, so its warm-up of High does not cover the grass pipelines; the first change to High compiles them.
- While a warm camera draws (10 frames each time the pipeline count grows), the view's shadow cascades and the whole scene queue run twice. That cost is not measured on the 2,000-tile world; it is inferred to be small at 10 frames per warm-up, and it overlaps the main-thread growth card.

## What changed

1. **A change in play keeps the scene** (`hex_play::tier::retier_play`). Before, `set_tier` despawned every scene entity and ran `scene::setup` again, so every tile, its water, scatter, grass, agents and the camera were rebuilt. Only these depend on the tier in play, so only these change now, to what `scene::setup` builds at the tier:
   - the camera's SSAO, removed with the normal prepass SSAO requires (`post::set_camera_ssao`; a camera spawned at Medium has no normal prepass, so leaving it would change every pipeline key);
   - the camera's screen-space transmission, which follows the water's refraction (`water::tiles::set_transmission`; Low drops it);
   - the water material's shader variant and wave count (`water::retier_materials`), applied two frames after the camera (`WaterRetier`, see below);
   - the tilt-shift default (`settings_for`, kept by `kept_tilt` when the player chose it);
   - the flock size (`boids::set_budget`, as before). Grass and the car cap already follow `GateConfig::tier`.

   The kit, water and look-dev scenarios keep the full rebuild. The test `tier::tests::a_play_tier_change_keeps_every_tile_and_matches_a_fresh_scene` changes High, Medium and Low into each other and checks that every tile root and child survives, that the kept camera has exactly the component set of a camera built at the new tier, and that the water material and tilt-shift equal a fresh scene's.
2. **Play warms the neighbouring tier's pipelines** (`hex_play::tier::warm`, `tier_warm`, on by default on the web only). After READY, once the pipeline count has held for 60 frames with none being created (not player idleness: the build of a world and a newly placed piece both end in such a pause), a hidden clone of the game camera with the neighbouring tier's view components (High and Ultra warm Medium, Medium warms High, Low warms Medium) draws the same view into a 256-pixel-wide image for at least 10 frames. Beside it, a copy of a water surface on that tier's variant is on a layer only it sees. Bevy keeps the specialized pipelines, so a later change to that tier finds them all in its caches. It runs again whenever pipelines were created since (a pack, a new piece, a tier change). Systems that act on the game camera skip the warm camera (`post::GameCamera`).
3. **Three things the first build got wrong, found by logging every pipeline natively:**
   - Cloning the camera while denying `Camera` also dropped `Camera3d` and everything else that requires `Camera` (Bevy's cloner drops required-by components of a denied one), so the warm view was not a 3D view at all. It now clones with `without_required_by_components`.
   - A new view's key gains `ENVIRONMENT_MAP` a frame or two after it first draws (Bevy gathers its light probes late). A 3-frame warm-up specialized for a key no game camera has, and the first change still created 35 pipelines. The warm-up now draws at least 10 frames.
   - Changing the water material and the camera in one frame drew the new water variant under the old view for a frame: a pipeline no tier draws again, which the warm-up never makes. The water now follows two frames after the camera, drawing the old variant under the new view in between, which the warm-up did make. The change counts as drawn only after that.

## Look at each tier

The rendered look at each tier is unchanged. Evidence:

- `make thumbs` twice on the final code: 0 thumbnails changed each time.
- The test above (component sets, water material, tilt-shift).
- Native captures (`look/`, `look.json`): the 59-tile water world, 1280x800, fixed water clock, a fresh start at a tier against a start at the other tier with `set_tier` 300 frames in, shot 60 frames after. The tier difference itself shows as a signed mean shift of -0.66/-0.83/-0.67 (RGB) between fresh High and fresh Medium (SSAO darkening). An in-place change against a fresh start at the same tier shifts by at most 0.03, and each in-place image is nearest to the fresh image of its own tier. What remains (mean absolute 0.84-1.02 against a 0.19-0.23 fresh-to-fresh floor) sits on tree edges, boats and wash lines: wind sway and boats follow real time, which differs between the runs. The captures ran on the build before the water-ordering fix; that fix changes only the two frames after a change.
- `e2e/smoke.mjs` (web-smoke's smoke step, with the new case 8b) and `e2e/initial-download.mjs` on the measured wasm, both passing. The last whole `make web-smoke` run failed in case 8b before the `tier.warm.current` fix; the two steps were then run separately on the fixed build.

WebGPU's 16 sampled textures per stage still hold: web-smoke clamps the device to 16, and the warm-up's Medium pipelines sample 14 (kit forward) and 8 (water), inside it.

## Setup

- **Host.** MiniMac (Apple M4, macOS 27.0.1), booked 17:05-18:35Z; the measured runs took 17:13-17:37Z. A first run (17:05-17:12Z) was stopped and discarded: the first step of a session could start while a warm-up was still compiling, so its freeze landed on the pick. The harness now settles before every step. Its cache trap restored the test browser's Metal cache, checked before the restart. Every session's load stayed between 1.93 and 3.50 (guard: 15 before and after each step and around each warm-up). No sample was discarded.
- **Browser.** Playwright 1.63 Chromium (Chrome for Testing 153, `chromium-1243`), headless, GPU (`--use-angle=metal --enable-gpu`), 1440x900 at DPR 1, WebGPU adapter `apple metal-3`.
- **Builds.** Before: `0244573` (main). After: `c161daa`, the same wasm as the final SHA (later commits change only the harness and docs). After without the warm-up: the after build with `?tier_warm=off`.
- **Harness.** `e2e/tier-stall.mjs` via `scripts/perf/tier-change-rounds.sh 3` with `TIER_WINDOW_MS=10000`: per round and arm (order rotated), one cold and one warm session; each session is a fresh profile, the 208-tile world, then High to Medium (first), Medium to High, High to Medium (repeated). Every step starts with the world settled and any warm-up finished, and recording runs past the window until the change is drawn. Raw sessions in `minimac/results/`, `minimac/summary.json` and `minimac/summary.txt`.
- **Cold cache.** A fresh browser profile alone leaves the macOS Metal shader cache warm: it is per app bundle (`$(getconf DARWIN_USER_CACHE_DIR)com.google.chrome.for.testing.helper`). The script moved it aside, emptied it before each cold session and restored it at the end (restored and checked at 17:38Z). The cold sessions are cold: READY is 8.7-8.9 s against 2.9 s warm in every arm, and the before build's first change freezes 9.7 s cold against 0.69 s warm.
- **Metric.** MiniMac draws this world with frames over 25 ms now and then even when idle, so the harness's `stall_ms` (the pick to the end of the last frame over 25 ms in the window) mostly measures that noise here (for example 4.0 s with a 41 ms longest frame). The longest frame and the drawn time are the measures used.

## Results

Medians [min, max] over 3 rounds. Longest frame / drawn, ms.

| Arm, cache | READY | #1 High to Medium | #2 Medium to High | #3 High to Medium | Pipelines created by #1, #2, #3 |
|---|---|---|---|---|---|
| before, cold | 8,764 [8,730, 8,854] | 9,701 [9,568, 9,730] / 10,445 [10,312, 10,461] | 556 [552, 558] / 761 [760, 762] | 143 [135, 143] / 199 [193, 199] | not reported |
| after, cold | 8,806 [8,786, 8,914] | 455 [450, 466] / 151 [118, 156] | 43 [43, 54] / 137 [135, 185] | 43 [38, 59] / 104 [100, 142] | 0, 0, 0 |
| after without warm-up, cold | 8,853 [8,811, 8,866] | 9,719 [9,601, 9,720] / 10,810 [10,716, 10,824] | 466 [465, 471] / 598 [597, 600] | 35 [33, 35] / 110 [96, 115] | 39, 1, 0 |
| before, warm | 2,920 [2,918, 2,922] | 694 [686, 699] / 861 [834, 944] | 180 [178, 181] / 263 [247, 266] | 141 [139, 141] / 198 [182, 200] | not reported |
| after, warm | 2,907 [2,907, 2,913] | 41 [40, 41] / 153 [151, 156] | 41 [36, 42] / 135 [130, 136] | 41 [37, 47] / 119 [113, 120] | 0, 0, 0 |
| after without warm-up, warm | 2,918 [2,909, 2,921] | 567 [563, 568] / 829 [792, 861] | 58 [55, 63] / 158 [149, 166] | 40 [37, 48] / 114 [100, 116] | 39, 1, 0 |

The warm-ups themselves (after, 4 per session: right after READY, two while the world was built, one once it had settled), longest frame per warm-up, ms:

| Cache | Right after READY | After the world settled |
|---|---|---|
| cold | 3,248 [3,225, 3,259] | 6,680 [6,522, 6,765] |
| warm | 217 [216, 224] | 182 [100, 183] |

Reading the arms:

- The first change to Medium compiles 39 pipelines (`after_nowarm`), the cost before: 9.7 s cold, 0.57-0.69 s warm. With the warm-up it creates none.
- The repeated change (#3) was a scene rebuild before (141-143 ms longest) and is a component swap now (35-43 ms, the same as an ordinary frame here).
- The after cold #1's 455 ms frame comes about 15 engine frames after the pick, after the change is drawn (151 ms), with no pipeline created, and only with a cold cache (41 ms warm). It is too early to be the warm-up of High that follows (60 frames). Its cause was not traced; the car fade over the lowered cap (250 ms) is a candidate, not checked.
- READY (navigation to `ready()`) is within 2% between arms in both cache states: 8.76 against 8.81 s cold, 2.92 against 2.91 s warm.

## READY

READY does not move: the warm-up never runs before READY (it waits for frame 120, the READY frame, and then 60 idle frames), and the medians above are 8,764 ms before against 8,806 ms after on a cold cache and 2,920 against 2,907 ms warm. What moves is the cost after READY: the first warm-up starts in the first frames after READY (the 60 frames without a new pipeline have already passed before it), and on a cold Metal cache it holds the page for 3.25 s on MiniMac (217 ms warm). On the start tile alone that is the price of having Medium's pipelines ready, paid once per new shader set (a build whose shaders changed, or a cleared cache). Whether that is better than paying on the pick, behind the quality panel, is an owner call (PRD §9.2); `tier_warm=off` turns it off.

## Reproduce

Build both dists (`make build-web`; the before arm from `0244573`). On the timing host, in a directory holding `before/`, `after/`, `e2e/` (without `node_modules`), `web/ui/src/` and the two scripts:

```bash
(cd e2e && npm ci --no-audit --no-fund)
ulimit -n 10240
TIER_WINDOW_MS=10000 bash tier-change-rounds.sh 3 before=before after=after "after_nowarm=after?tier_warm=off"
node tier-change-summary.mjs results summary.json
```

`tier-change-rounds.sh` runs, per round and arm (rotated), a cold session then a warm one, each one `e2e/tier-stall.mjs` round: a fresh browser profile, the 208-tile world of `e2e/water-world.mjs`, then High to Medium, Medium to High and High to Medium. The macOS Metal shader cache lives per app bundle under `DARWIN_USER_CACHE_DIR` (`com.google.chrome.for.testing.helper` for Playwright's Chromium), not in the browser profile, so a fresh `--user-data-dir` alone leaves it warm. The script moves the test browser's cache directory aside for the whole run, empties it before each cold session, and puts the original back on exit. It never touches Google Chrome's own cache.
