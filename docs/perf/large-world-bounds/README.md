# Large world: Bevy `calculate_bounds` on the 2,066-tile save (MiniMac, 2026-10-27)

Card: "Large world: Bevy calculate_bounds costs ~4.6 ms of main thread a frame". After the `finish_kit` fix (`c032d97`, `docs/perf/large-world-cpu/`), the 2,066-tile save's web main thread ran about 16.1 ms a frame at the close camera and 15.3 ms at the overview on MiniMac. Bevy's `calculate_bounds` was the largest remaining system, at 4.6 ms (close) and 4.8 ms (overview) a frame in the V8 profiles.

## Summary

- **Which entities: every water surface, 1,658 of the save's 3,244 `Mesh3d` entities, about 4.1 M vertices, on every frame.**
- **Why:** `hex_render::water::tick` writes the water clock into every `WaterMaterial` and `TerrainMaterial` each frame through `iter_mut()`, which emits `AssetEvent::Modified` for each one. Bevy's `bevy_pbr::material::mark_meshes_as_changed_if_their_materials_changed::<M>` then calls `set_changed()` on the `Mesh3d` of every entity using a modified material (Bevy does this so `extract_meshes_for_gpu_building` re-extracts the mesh's material slot). `calculate_bounds`' update query is `Or<(AssetChanged<Mesh3d>, Changed<Mesh3d>)>`, so it recomputed each water surface's `Aabb` with `MeshAabb::get_aabb`, a walk over every vertex position (the meshes keep their CPU data, so they have no precomputed `final_aabb`).
- **Not the suspects the card listed.** No mesh asset is added or modified per frame (`AssetEvent<Mesh>` counts are 0 after load), no `Mesh3d` is added (the count stays at 3,244), nothing writes a `Mesh3d` handle, and only 5 entities have `NoFrustumCulling`. Kit, scatter and boundary entities are not involved: no `Mesh3d` other than the water surfaces is marked changed after load.
- **Fix (`1a8382f`):** water surfaces and terrain beds spawn through `hex_render::water::spawn_fixed_mesh`, which computes the `Aabb` once with Bevy's own `MeshAabb::get_aabb` before adding the mesh and inserts `NoAutoAabb`, the component Bevy provides for entities spawned with a correct `Aabb`. `calculate_bounds` filters `Without<NoAutoAabb>` by archetype, so it no longer visits them. The bounds are the same values, so culling and every image are unchanged. Culling stays on.
- **Result on MiniMac (5 load-guarded rounds, loads 3.57-4.55):** main-thread `cpu` falls **6.00 ms at the close camera (23.62 to 17.61) and 5.59 ms at the overview (19.99 to 14.42)**, both real. In the V8 profiles `calculate_bounds` goes from 6.11 to 0.01 ms a frame at both cameras. The frame mean moves -0.24 ms close (not real) and -0.23 ms overview (real), because both frames are GPU-bound. Draws are unchanged (1,472 close, 962 overview), and `make thumbs` run twice changed no thumbnail.

## Evidence: which entities, and why (native diagnostic)

`bounds_diag.rs` here is a temporary system (not in the game) that ran after `VisibilitySystems::CalculateBounds` in a native release build on macdaddy: the 2,066-tile save, play, High, grass off, `--offscreen on`, close camera then overview. Every 120 frames it logs the `Mesh3d` census, the entities whose `Mesh3d` changed and whose `Aabb` was rewritten since the previous frame, the mesh and material asset events, and the changed entities' materials and vertex counts.

| Build | `Mesh3d` | `Mesh3d` changed per frame | `Aabb` rewritten per frame | Their material | Their vertices | `NoAutoAabb` | Mesh events | Water / terrain material `Modified` |
|---|---|---|---|---|---|---|---|---|
| main (`4897f2e`), `native-diag-main.log` | 3,244 | 1,658 | **1,658** | `WaterMaterial` (all 1,658 users) | 4,097,463 (largest 3,366) | 0 | 0 | on every sample |
| fix (`1a8382f`), `native-diag-fix.log` | 3,244 | 1,658 | **0** | `WaterMaterial` | 4,097,463 | 1,658 | 0 | on every sample |

- Every sampled frame after load (41 samples on main, 39 on the fix, both cameras) reads exactly these numbers. The first sample (frame 120) is the load frame.
- The material event counts are cumulative over the diagnostic's reader window (10-26 water and 5-13 terrain events per sample, always two to one), so they show the materials are modified continuously, not an exact count per frame.
- One `get_aabb` pass over the 1,658 meshes took a median of 4.8 ms natively (`get_aabb_us`, 4.6-5.4 ms on main; macdaddy at a load of about 500-600, so not timing evidence). That is the same order as `calculate_bounds` in the web profiles: 4.6-4.8 ms in the card's session and 6.1 ms in this one.
- The fix still marks the 1,658 `Mesh3d` changed every frame (Bevy's material system does that), but `calculate_bounds` rewrites none of their bounds.
- No terrain bed is spawned in this save (0 `TerrainMaterial` users). Beds take the same path when present, so they get the same fix.

## Results: main against the fix on MiniMac

### Timing (`bounds-web.json`, `bounds-web-summary.json`, `bounds-web.log`; 5 rounds, arms rotated, one warm-up discarded)

| Camera | Arm | Mean ms | p95 ms | Main-thread `cpu` mean ms | `cpu` p95 ms | Draws (kit) |
|---|---|---|---|---|---|---|
| close | main | 37.19 | 39.5 | 23.62 | 24.8 | 1,472 (1,337) |
| close | fix | 37.15 (-0.24 [-0.39, +0.42]) | 39.8 (+0.3 [+0.1, +0.9]) | **17.61 (-6.00 [-6.29, -5.78], real)** | 18.5 (-6.3, real) | 1,472 (1,337) |
| overview | main | 45.07 | 48.0 | 19.99 | 21.0 | 962 (881) |
| overview | fix | 44.81 (**-0.23** [-0.34, -0.18], real) | 48.6 (+0.5 [+0.3, +0.9]) | **14.42 (-5.59 [-5.76, -5.55], real)** | 15.2 (-5.8, real) | 962 (881) |

- Every run's 1-minute load stayed between 3.57 and 4.55, before and after. No sample was discarded or failed, and the console logged no error.
- Draw counts are identical in every round.
- The close camera's mean does not move, and the overview's moves 0.23 ms. Both frames are GPU-bound (every window frame is over 25 ms, with the main thread 20 to 30 ms under the frame), so cutting main-thread time shows in `cpu`, not in the mean.

### Which functions (`profiles/`, `profile-web.json`; one profiled round on the named builds, loads 3.36-3.82)

| System (inclusive ms per frame) | main close | fix close | main overview | fix overview |
|---|---|---|---|---|
| `bevy_camera::visibility::calculate_bounds` | **6.11** | **0.01** | **6.10** | **0.01** |
| render schedule (all render-world systems) | 12.94 | 13.00 | 9.32 | 9.66 |
| `bevy_pbr::render::mesh::extract_meshes_for_gpu_building` | 0.20 | 0.19 | 0.15 | 0.14 |
| Bevy's `Query<&mut Mesh3d>` marking systems (largest instance) | 0.02 | 0.02 | 0.02 | 0.02 |
| `hex_render::water::tick` | 0.01 | 0.01 | 0.01 | 0.01 |
| profiled `cpu` mean | 24.4 | 18.2 | 20.61 | 14.94 |

- `calculate_bounds` was almost all self time (6.09 of 6.11 ms): the inlined vertex walk of `get_aabb`.
- What remains of the chain is small. The water surfaces' `Mesh3d` is still marked changed every frame, so `extract_meshes_for_gpu_building` re-extracts them (0.19 ms inclusive at the close camera on the fix, for all meshes it extracts, against 0.20 on main), and Bevy's systems that mark them take about 0.02 ms each. Removing that would mean moving the water clock out of the material assets into a global uniform, a shader binding change that is not worth 0.2 ms.
- The other per-system differences between the two profiles are within single-round noise (the render schedule's +0.06 and +0.34 ms are not confirmed by the timing runs, whose `cpu` drop matches the `calculate_bounds` drop to within 0.1-0.5 ms).

### Main has grown since the card's numbers (not investigated)

The card quotes 16.1 ms (close) and 15.3 ms (overview) of main-thread time for `c032d97`, with a 30.4 ms close mean and 1,422 close draws, measured on MiniMac earlier the same day (`docs/perf/large-world-cpu/`). This session's main (`4897f2e`) reads 23.62 and 19.99 ms, a 37.2 ms close mean, and 1,472 close draws. In the profiles, `calculate_bounds` read 4.6 ms then and 6.1 ms now, and the render schedule 8.7 then and 12.9 ms now. Those are two sessions and different commits, so the growth is not paired evidence. The draw count shows main changed in between (the scatter changes after `c032d97`), and whether the rest is code or host state was not measured. It is outside this card and is left for a paired run of `c032d97` against main.

## Criteria

1. **Which entities have bounds recomputed each frame, and why.** All 1,658 water surfaces (every `WaterMaterial` user, 4.1 M vertices) and nothing else, measured per frame by the native diagnostic. The chain is `water::tick` modifying the water and terrain materials each frame, then Bevy's `mark_meshes_as_changed_if_their_materials_changed` marking their `Mesh3d` changed, then `calculate_bounds`' `Changed<Mesh3d>` arm re-walking every vertex. The web profile puts that at 6.1 ms a frame on main and 0.01 ms on the fix. Static kit, scatter and boundary entities are not recomputed: their `Mesh3d` never changes after spawn.
2. **Cost cut, with MiniMac before and after.** Main-thread `cpu` -6.00 ms close and -5.59 ms overview (5 load-guarded rounds, loads 3.57-4.55, both real), draws unchanged, frame mean -0.24 ms close (not real) and -0.23 ms overview (real). `make thumbs` run twice changed no thumbnail. Culling is unchanged: the `Aabb` values are Bevy's own, computed once.

## Gates

All on macdaddy, on the fix commit `1a8382f` (this directory's commit adds only documentation), each gate's output redirected to a file and its exit code read directly:

| Gate | Exit | Notes |
|---|---|---|
| `make thumbs` (first run) | 0 | 0 thumbnails changed |
| `make thumbs` (second run) | 0 | 0 thumbnails changed |
| `make checkall-ci` | 0 | initial download 21.38 MiB (astc) |
| `make shots-scatter` | 0 | The first attempt exited 2 with the first capture all black for 268 s: its window opened behind another window on a host at a load of about 500 (the thumbnails window floats, this one does not). The rerun passed every check: overview 344 draws (gated), the mixed world's overview 485 (gated), pairs 173 and 267. Its JSON reports differ from the committed ones only in the grass build times (`last_build_ms`, `last_frame_build_ms`). Its images differ by JPEG noise (max channel difference 1-16) except `scatter-02-forest-cluster.jpg` (82), where a few small moving specks over the canopy sit at other positions; that view holds no water. The committed checkpoint files were restored. |
| `make ui-e2e` | 0 | |
| `make web-smoke` | 0 | |

## Inferred, not measured

- Per-function times come from one profiled round per build under a sampling profiler. Timing comes from the unprofiled runs.
- Which entities are recomputed was measured natively (macdaddy), not in the browser. The web build runs the same ECS code, and the profile's 6.1 ms drop to 0.01 ms in `calculate_bounds` matches it, but no web run counted entities.
- The native `get_aabb_us` figures were taken at a host load of about 500 and are attribution, not timing.
- That no water or bed mesh is modified in place or swapped onto another entity after spawn (which `NoAutoAabb` relies on) comes from reading the code: no `Assets<Mesh>::get_mut` and no `Mesh3d` write exists in the workspace. The regression test `water::tests::fixed_mesh_bounds_survive_a_changed_mesh3d` covers the spawn helper, not that invariant.
- The ghost preview's water piece uses the ghost's own material, not `WaterMaterial`, so it was left as it was. The tier warm-up's copy of a water surface (off by default since owner A33 O2) still has its bounds recomputed for the few frames it lives.
- The growth of main since the card's numbers is a cross-session comparison (see above).
- `build-arm.sh` and `minimac-run.sh` are a record of how the arms were built and run. `build-arm.sh` hardcodes this work's worktree path.

## Setup

- **Host.** MiniMac (Mac16,10, Apple M4 with a 10-core GPU), booked 23:00Z-01:00Z. All runs took place 23:00-23:19Z, and `~/perf/bounds` was deleted at 23:19Z with no test browser or `caffeinate` left running.
- **Harness.** `e2e/perf-close-camera.mjs` with `WORLDS=world2066`, the cameras and settings of `docs/perf/large-world-o4o5/README.md`: headless Chromium, uncapped, 2560x1440, play, High, grass off, photo mode with DOF off. The arms rotate each round, and one warm-up run is discarded.
- **Load guard.** `e2e/perf-load.mjs` requires a 1-minute load of 15 or less before and after every run.
- **Builds.** Each arm was built on macdaddy from `git archive` of its commit, with `target/` and `web/ui/node_modules` cloned in, the sources touched, the seven workspace crates cleaned for the wasm target, and `make build-web` (`build-arm.sh`). main is `4897f2e`, the fix `1a8382f`. The named arms add `data-keep-debug data-wasm-opt-params="-g"` so the wasm keeps its name section (58-59 MB instead of 28 MB, same code), and are used only for the V8 profiles.

Deltas are paired within a round, given as the median [min, max]. "Real" follows the close-camera-draws rule: every paired delta has the same sign, and the median is larger than their spread.

## Reproduce

```bash
# macdaddy
git archive <sha> -o /tmp/<arm>.tar
bash docs/perf/large-world-bounds/build-arm.sh <arm> /tmp/<arm>.tar [named]
# native diagnostic: copy bounds_diag.rs into crates/hex-app/src, wire it as its header says, then
HEXW_BOUNDS_DIAG=1 ./target/release/hex-world --scenario play --load docs/perf/post-arc001-2026-10-16/local-world.hexworld \
  --script <camera script> --tier high --grass off --tilt off --seed 1 --size 1280x720 --offscreen on --present novsync --exit-after-seconds 60
# MiniMac, inside a booked window: arms/, e2e/ without node_modules, web/ui/src, scripts/perf, the save and
# docs/checkpoints/scatter/scatter-mixed.json under ~/perf/bounds, then npm ci in e2e/
bash minimac-run.sh timing      # 5 rounds, main against the fix
bash minimac-run.sh profile     # 1 round, named arms, PROFILE_DIR
node e2e/perf-close-camera-summary.mjs results/bounds-web.json main results/bounds-web-summary.json
python3 scripts/perf/cpuprofile-functions.py A.cpuprofile B.cpuprofile --match '^system '
```
