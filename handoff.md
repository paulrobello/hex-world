# Hex World handoff (2026-10-17)

This is the orchestrator session for P2+ work: parallel worktree agents, verified and squash-merged to local `main`. The board is tracked as `KANBAN_ACTOR=hex-world-p2-orchestrator`.

## State

- Branch `main`. Pushed to `origin/main` through `2b150eb` on 2026-10-15 with Paul's approval; later local commits (check with `git rev-list --count origin/main..main`) are unpushed. Every push needs Paul's approval.
- The working tree is clean apart from an untracked `.claude/launch.json` (leave it). No other worktrees are active, and no agents or background runs are going.
- The play build (`dist/`, port 8094) was rebuilt from `main` after the last merge.
- CI is green: run 38804731279 (push of `2b150eb`) passed every job, including both test legs (ubuntu-latest and macos-latest), and deployed to Pages.

## Standing rules from the owner (still in force)

- Never push without Paul's approval. Commit locally, then rebuild with `make build-web` after every merge to `main`.
- **No new asset-generation agents until Paul gives the all clear** (token budget). Tool fixes and non-asset features are allowed.
- Blender is headless only, with no live Blender or MCP (owner rule, 2026-10-13).
- Tool bugs and flaky tests are high priority, never low.
- Never record an owner approval yourself. Use pending entries in `docs/review/` and on cards only.
- Perf numbers need host load at or below 15 before and after every run (the e2e perf scripts enforce it). MiniMac (base M4) is cleared for perf runs as a separate baseline; see `docs/perf/minimac/README.md` (keep its baseline separate from the M4 Max).
- Paul said: "once the current batch of work is complete dont start any new work". The batch is complete. Start new work only when Paul asks.

## Unfinished work

### Waiting on Paul (decisions and approvals)
1. **Push approval** for anything committed after `2b150eb`. After a push, watch the run with `gh run watch <id> --exit-status`.
2. **Windmill kit piece**, card `01a13d667322`: owner review of the renders in `docs/checkpoints/windmill/`. The windmill *tile* is already merged and closed.
3. **Prop variants and single wildflowers**, card `01a13d666eea`: owner review in play.
4. **Colour grade preset**, card `01a13c085851` criterion 4: the owner picks one, then the kit and water evidence is re-shot (card `01a13ba24e9e`, including the A4 share image for the favicon/PWA card `01a1385abd81`).
5. **Grass look**, card `01a13c876aaf` criterion 6: owner approves the look.
6. **Bulldoze dust**, card `01a13c03f90e`: owner checks the puffs, now 20% smaller, in play.
7. **DESIGN.md proposals** from the debug tab (lod-0/1/2 data colours and the performance corner placement), plus the new-version toast line. These are proposals until Paul signs off.
8. Older items in `docs/review/2026-10-14-owner-queue.md` and its answers file: music audition, the assumptions pane, comment-stripped provenance hashes (card `01a13dd3c03f`), and fish and bird refs (card `01a13942e556`).

### Blocked on conditions
- **web-smoke stress**, card `01a139190115`: the fix is merged. Criterion 2 needs 3 passing `make web-smoke` runs in a row at host load **above 40**. One passed at load 47. Re-run when load is high, and skip any run that starts below 40 (the earlier script bug).
- **Grass density slider**, card `01a13d11e932` criterion 2: needs a load-guarded measurement while dragging the slider (no rebuilds, no hitch) and blade counts, mesh bytes and per-tile build ms at 100% and 150%.
- **River mouth / spring tiles**, card `01a13ba946e3`: waits on the fish card `01a13c3a8c1f` criterion 1 (fish crossing through both mouth handednesses, with a shot and a boids check).

### Open engineering work (highest value first)
1. **Large play worlds are slow on the web**, card `01a14028862a` (high). The D1 instancing pivot shipped on branch `perf/play-rendering` @ `76c14f8` (instance records + KIT_INSTANCED shader entries; `make checkall` and `make checkall-ci` green). Canonical M4 Max measurement: web p95 17.7/18.3 ms (exact-save world, guard exact-save-retry-v2-five-consecutive, load 9→12) — 2.8× down from the pre-pivot 50.8/48.8, still above the 16.6 ms budget; criterion 2 open with `renderer_extract` (~20% of frame CPU both platforms) as the named target. Criterion 1 verified: native symbol profile of the settled world shows check_visibility 0.13%, calculate_bounds 4.41% (worst), update_frusta 0.02%, occlusion_culling 0.19% — all far under the 5% bar. **REGRESSION: the instanced path renders grounds black on WebGPU** (native fine; pre-pivot dist renders colored on web; silent — no console or validation error; not LOD-conditional; foliage slots render, ground-family slots do not). Repro: radius-8 web probe, both cameras, ~2 min. Root-cause hunting not started; suspects in the card notes (Dawn divergence candidates: static vs dynamic-offset meta binding, storage-in-vertex reads). Criterion 3's parity check needs a redo after the fix.
2. **Grass blades**, card `01a13c876aaf` criterion 4 (p95 at most 16.6 ms on a 2,000-tile world, native and web) fails on the web because of item 1. Native is not measured.
3. **P2 MVP**, card `01a131b7cbb8`: criterion 3 (GPU timestamps and stall tail) may now be checkable. The stall card `01a139d6646774f2a91cf9a983cd7937` closed with owner acceptance today. Criteria 1, 4 and 5 remain.
4. **High-priority tool backlog** (filed from the asset-agent review, `docs/review/2026-10-14-asset-agent-review.md`):
   - `01a13dd3aeb1` worktree bootstrap
   - `01a13dd3b20d` shot runner with a black-frame check
   - `01a13dd3b5ee` tangent check at kit-build
   - `01a13dd3b96c` LOD budget helper
   - `01a13dd3bcc3` perch test reports every failure
   - `01a13df91bfd` thumbs not reproducible (water and grass animate)
   - `01a13df91f1c` thumbs binary sometimes hangs after writing
   - `01a13df9374b` forest-play shot poses 04-07 sit inside the canopy
5. Other high backlog: hero water shader `01a137160846`, hero-polish UI `01a13793f737`, kit pipeline hygiene `01a137f8ae05`, terrain cap hump `01a13d875a6b`, PRD drift fixes `01a13d8883ea` (medium), Dependabot alerts `01a13ba26760` (medium).
6. The P3 to P8 phase cards are in the backlog. The P6 lineup now includes a lighthouse with an animated beam (card `01a13e01b79d`; needs owner-approved refs first, and asset generation is paused).

## Unresolved issues

- **CI shape** (`11522df`): one job per concern (fmt, ui, lint, typecheck, test, web, deploy), a test matrix of ubuntu-latest and macos-latest, and a `workflow_dispatch` `os-scope` input to re-run one OS (`gh workflow run CI -f os-scope=macos-latest`). First run green on 2026-10-15. GitHub notes that `ubuntu-latest` moves to Ubuntu 26 from October 19, 2026; if the apt step or a test breaks after that, pin `ubuntu-24.04` or update the package list.
- **The Ubuntu Parallels VM's disk is full** (62 GB, 0 free), so local Linux checks are impossible until space is freed. Don't delete VM files without Paul's approval.
- **Thumbs:** captures are not reproducible run to run (water and grass animate), so `make thumbs` dirties `assets/thumbs/` for water, beach and some land tiles. Restore them with `git checkout HEAD -- assets/thumbs docs/checkpoints/thumbs` unless you intend to commit them.
- **perf-grass on MiniMac** builds slowly. It now completes with the `993c6d6` wait, but runs take about 25 minutes for 3 arms.

## References

- Product and design: `PRD.md` (phases P2 to P8 and their checkboxes), `DESIGN.md`, `PRODUCT.md`.
- Agent interface: `docs/agent-operability.md` covers bridge commands, URL keys, make targets, the debug overlays, `version_poll`, and what `state().queued` counts.
- Perf evidence: `docs/perf/`:
  - `p2-grass-2026-10-15*.json`
  - `grass-instancing-ab-2026-10-15/`
  - `stall-tail/` (README plus `probe-2026-10-15.json`)
  - `minimac/`
- Perf scripts: `e2e/perf-grass.mjs`, `e2e/stall-probe.mjs`, `e2e/perf-load.mjs` (load guard), `e2e/build-world.mjs` (the build-and-wait helper for scripted worlds).
- Owner review: `docs/review/2026-10-14-owner-queue.md`, `docs/review/2026-10-14-owner-answers.md`.
- Asset pipeline: `.claude/skills/blender-asset/` (SKILL.md, `references/dispatch.md` for briefing asset agents, `references/project.md`, `references/stages.md`).
- New this session:
  - `web/ui/src/debug.ts` and `crates/hex-play/src/debug.rs` (Debug tab)
  - `web/ui/src/version-check.ts` and `scripts/web/version-dist.mjs` (new-version check, `dist/version.json`)
  - `data/tiles/windmill.ron` (windmill tile)
- Auto-memory: `~/.claude/projects/-Users-probello-Repos-hex-world/memory/MEMORY.md`.

## Next steps

1. Run `kanban item list --project hex-world --open` as `KANBAN_ACTOR=hex-world-p2-orchestrator`, and read the owner queue before choosing work.
2. Before any push, run `make checkall-ci` and a gitleaks scan of the unpushed range (`gitleaks git --log-opts="origin/main..main"`), ask Paul, then watch the CI run to green.
3. When Paul gives the go-ahead for new work, start with the large-play-world regression bisect (card `01a14028862a`). It is the biggest perf gap and has a concrete lead.
4. Integration flow for any agent branch:
   1. `git merge main` into the branch.
   2. `make ui-schema`.
   3. Gates: `make checkall-ci`, `make web-smoke`, `make ui-e2e`, plus `make kit-check` and `make thumbs` when kit or render code changes (restore thumbs after).
   4. `git merge --squash` onto `main` and check the exit code.
   5. Commit, then `make build-web`.
   6. Remove the worktree and branch.
   7. Check card criteria one at a time with evidence.

   Agents fork from the stale `origin/main`, so every brief must tell them to `git merge --ff-only main` and verify the base SHA first.
