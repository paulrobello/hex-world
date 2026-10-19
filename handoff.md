# Hex World handoff (2026-10-19)

## State

- Branch `main` well ahead of `origin/main` (pushed through `bc3dea0`); count with `git log --oneline origin/main..main`. Nothing pushed since; waits on Paul. `make checkall-ci` green at `cc38bad`.
- Working tree clean apart from an untracked `.claude/launch.json` (leave it). No worktrees, no background runs.
- `dist/` rebuilt from `main` post-merge (`make build-web` green).

## Landed 2026-10-19 (afternoon), all local
- Render fixes: `c15edd8` kit normal basis returned inverse(m), mirroring normals of every piece not at 0/180 degrees since `ccfdafd` (owner's coast/river-mouth hex-edge lines and the black walls on yawed cottages/bakery/barn); `3a973ea` GPU test; `79e44ab` instanced kit draws queued after bevy's mesh queues (coast wet band vanished, water read the skirt as bed). House thumbnails regenerated `8148a31`.
- New P3 pieces: hay bales `6cf5b97`, barn `ae8f5bf`, bakery `d5fe285`, cow and sheep `c6b1598`; houses re-baked `cc38bad` (unbaked masks texels). Reviews in `docs/review/2026-10-19-owner-queue.md` (A1-A5).
- Features: grass distance setting `86a3195` (perf not measured: host load 106-215, MiniMac unreachable; command in the commit), FPS overlay hidden by default `649f068`.
- Tooling: `scripts/worktree-bootstrap.sh`, export fails on non-unit normals/tangents, `lib/budgets.py`, stale generator hashes warn instead of fail (owner ruling), Dependabot fix `e30c9b2` (sharp 0.35.5, unused gltf-transform CLI dropped; alerts close after a push).
- Grass "less lush" report: no regression found (identical blade counts pre-audit vs main); grass fades by 58 m and the default camera is ~158 m away.

## User feedback from Paul (2026-10-19, post-deploy)

- "no change all same rendering issues exist" — the forest-placement artifact is unchanged on the deployed fixed build.
- "LOD level is also broken" — new information: LOD behavior is wrong alongside the artifact.

Symptoms when placing a forest: the full tree set rises, then several trees disappear; branches render as flat panels; strange blending; LOD level wrong while running the highest detail tier (ultra). The saved-world **crash** is fixed (root cause: WGSL parameter assignment in `shade_kit_vertex` killing the COLOR_0 def-variant; card `01a1528d8bb571d8901e46ea8b1e7614` done, vault note `~/ParsidionVault/Debugging/wgsl-parameter-assignment-breaks-only-its-def-variant.md`). The **artifact** is a separate, live regression — the earlier device-poisoning explanation did not hold.

## Standing rules from the owner (still in force)

- Never push without Paul's approval. Commit locally, then rebuild with `make build-web` after every merge to `main`.
- Asset-generation agents are cleared again (Paul, 2026-10-19: "start creating more assets in blender"), on Opus 5.5 at xhigh effort.
- Blender is headless only, no live Blender or MCP (owner rule, 2026-10-13).
- Tool bugs and flaky tests are high priority, never low.
- Never record an owner approval yourself. Use pending entries in `docs/review/` and on cards only.
- Perf numbers need host 1-min load ≤ 15 before and after every run. MiniMac (base M4) is cleared for perf as a separate baseline; see `docs/perf/minimac/README.md`.
- "once the current batch of work is complete dont start any new work" — start new work only when Paul asks.

## Unfinished work

### Resolved 2026-10-19 — forest artifact + LOD, card `01a15485cb01756581d96017fea753d3` (done)
Fixed in `ab9a107`; evidence and the five root causes in `docs/checkpoints/forest-instancing/README.md`. Bisect: `218097d` and `a434c5b` render correctly, the GPU-instancing pivot `c92a961` introduced it. The defects: parent-only placement dedupe (scatter groups every tree under one entity), chunk-relative record translation, slot material key 0 plus missing `MAY_DISCARD`, `pbr_fragment`'s `mesh[]` flags read aliasing the kit record buffer, and masked slots queued into `AlphaMask3dPrepass` with the `Opaque3dPrepass` draw-function id. Debug Force LOD and LOD tint now reach settled parts through `KitLodDebug`. Same-build instanced vs per-entity A/B matches on every tier. Owner re-check on the deployed site is pending.

### Waiting on Paul (decisions and approvals)
1. Windmill kit piece, card `01a13d667322`: owner review of the renders in `docs/checkpoints/windmill/` (tile already merged).
2. Prop variants and single wildflowers, card `01a13d666eea`: owner review in play.
3. Colour grade preset, card `01a13c085851` criterion 4: owner picks one, then kit/water evidence re-shot (card `01a13ba24e9e`, A4 share image for favicon/PWA card `01a1385abd81`).
4. Grass look, card `01a13c876aaf` criterion 6; bulldoze dust, card `01a13c03f90e` (puffs 20% smaller — check in play).
5. DESIGN.md proposals from the debug tab (debug LOD colours, performance corner placement) and the new-version toast line.
6. Older `docs/review/2026-10-14-owner-queue.md` items: music audition, assumptions pane, provenance hashes (card `01a13dd3c03f`), fish/bird refs (card `01a13942e556`).

### Blocked on conditions
- web-smoke stress, card `01a139190115`: needs 3 consecutive `make web-smoke` passes at host load ABOVE 40 (one passed at 47).
- Grass density slider, card `01a13d11e932` criterion 2: load-guarded drag measurement + blade/mesh/timing numbers at 100%/150%.
- River mouth/spring tiles, card `01a13ba946e3`: waits on fish card `01a13c3a8c1f` criterion 1.

### Other open engineering
- Large play worlds slow on web, card `01a14028862a` (high): criterion 2 open targeting `renderer_extract` (~20% of frame CPU). Its web p95 17.7/18.3 ms predates `ab9a107` (foliage now uses discard pipelines, opaque draws cull per material) — re-measure before claiming anything. Criterion 3 parity redo is now unblocked.
- Dependencies, card `01a1547b6ac5747e967b94e245b57a6e` (high): 4 high Dependabot alerts (sharp ×3, braces) — dev/tooling npm deps; bump and confirm alerts clear.

## Unresolved issues

- No automated gate covers the instanced path visually: `make thumbs` renders single tiles and the e2e scatter check compares counts, not pixels. The same-build A/B used here (a temporary early return in `chunks::settle` keeping parts per-entity, then RMSE) is the reliable check; a committed play-path pixel gate would have caught all five defects.
- Camera framing / thumbs runs are susceptible to GPU contention that the CPU load guard does not see; a clean-looking failure can be environmental — re-run isolated before diagnosing.

## Next steps

1. Paul re-checks forest placement and the Debug tab's LOD tint / Force LOD on the deployed site.
2. Pivot card `01a14028862a`: fresh web/native parity shots for criterion 3, then a load-guarded re-measure of the web p95 on the post-fix build before working criterion 2.

## References

- `docs/agent-operability.md` — every CLI flag, web query param, `window.hexWorld` bridge command, script commands (`select_tile`, `place`, `wait`, `camera`, `screenshot`, `save`), `--clock fixed --exit-after N`.
- `crates/hex-render/src/kit_instances.rs` — `kit_view_ranges` (~160), `MATERIAL_DEFS`/`push_material_defs` (~262/301), `KitKey`/`specialize` (~330/347), mask binning (~846-876), `DrawKitInstances` (~1094).
- `crates/hex-render/src/kit_material.rs` — `slot_requests` (126), `params_for` (191), `build_material` (257, AlphaMode::Mask → alpha_cutoff + double_sided).
- `assets/shaders/kit_vertex.wesl`, `assets/shaders/kit_material.wesl`, `assets/shaders/kit_instances.wesl`.
- `PRD.md` §15 phases, §10 budgets; `docs/kit/` water bed + shader-hooks contracts.
- Vault: `~/ParsidionVault/Debugging/wgsl-parameter-assignment-breaks-only-its-def-variant.md` (the crash pattern + the log-grep verification recipe).
- CI: `.github/workflows/ci.yml` (auto-deploy to Pages on main push); watch with `gh run watch <id> --exit-status`.
