# Hex World handoff (2026-10-20)

Start here in a fresh session (Claude Code or Claude Desktop). Then read `CLAUDE.md`, `PRD.md` §15 and `.claude/skills/blender-asset/SKILL.md` before any asset work.

## State

- `main` pushed and every 2026-10-20 push passed CI and auto-deployed to hex-world.pardev.net. Count unpushed with `git log --oneline origin/main..main`; check the last run with `gh run list -R paulrobello/hex-world --branch main --limit 1`.
- `dist/` rebuilt from `main` after every merge (`make build-web`).
- Working tree clean apart from an untracked `.claude/launch.json` (leave it). No worktrees, no agents running.
- Piece tables are slices now (`&[T]`), so a new asset adds a row without touching a length line.

## Landed 2026-10-19 evening to 2026-10-20 (all pushed)

Assets (every piece's owner approval is still **pending**; review sheets under `docs/checkpoints/<piece>/sheet.jpg`):
- Rail: river rail bridge `d6eb22a` (plus the train, crossing, station, junction from 2026-10-19).
- Houses: `house_longhouse` `de3bf7e`, `house_twostorey_b` `885458b`, drawn through the hero variant resolver `e24a97c` (`crates/hex-core/src/variant.rs` `HERO_VARIANTS`, about one slot in three).
- Oak tile: scattered `tree_oak`/`tree_oak_b` `2b6919b`; hero variant `tree_hero_oak_b` `36aa4a4` (12 bird perches).
- Pine forest floor: ferns and log `261dc10`, variants `4aadbfb`, shrub and sapling `8eafaac`, `shrub_forest_b` (holly) `9f7ae7c`, `sapling_spruce_b` `576760e`, `log_fallen_c` (hollow log).
- Houses (late): `house_cottage_b`, a slate-roofed rendered cottage with a gabled porch; the cottage slot now splits its one-in-three variant between the longhouse and `_b` (about 1/6 each).
- Tooling: piece tables as slices (card `01a15994825570`).
- Farm: paddock fence in play `f865be6` (per-part walker colliders, post-only grass clears, line-clear scatter), grazing cow and lamb `ae8e404`, hay bale variants `2da86b8`, grazing sheep `12d0d8d`.
- Roads: verge bushes `bush_verge` `b8ad8e6` and `bush_verge_b` `8c5ad9b` (verge band scatter, none on parking tiles).
- Audio: 13 one-shot cues for farm, windmill, wheat, bakery and train `2f40c53` (owner approves by ear on `assets-source/audio/review.html`).

## In flight at handoff

Nothing. Every agent finished and landed: the piece tables are slices (card `01a15994825570` done), `log_fallen_c` and `house_cottage_b` are on main. `git worktree list` should show only the main checkout.

Landing recipe for future asset agents (the one used all session):
1. `git -C <worktree> merge main`, then `git apply --3way docs/checkpoints/<piece>/integration.patch`, resolve, `cargo test -p hex-play -p hex-render --lib`, commit on the branch.
2. `caffeinate -d -u make thumbs` in the worktree; thumbs are byte-reproducible, so only the tiles the piece changes (and the contact sheet) should come out dirty, apart from an occasional one-pixel `road_bend_60` difference (restore it); any other dirtied thumbnail is a real render change to look at.
3. `make checkall-ci > log 2>&1; echo EXIT=$?`, then squash-merge into `main`, `make build-web`, remove the worktree (`git worktree unlock` first if a finished agent left it locked), delete the branch, push, watch CI with `gh run watch <id> --exit-status`.

## Standing rules from the owner (still in force)

- The 2026-10-20 stop rule ("dont start anything new") was lifted by Paul's "continue work" the same day; normal throughput resumed.
- Fix friction or defects of any scope found along the way, don't only file them (Paul, 2026-10-20: "you can do fixes to any scope needed").
- Standing push approval when a batch is a significant addition or change, after `make checkall-ci` passes; watch CI after. Small or doc-only commits ride along with the next push. `make build-web` after every merge to `main`.
- Standing approval to generate sounds for new items; the owner still approves each by ear.
- Every asset brief carries the integration section (`.claude/skills/blender-asset/references/dispatch.md` part 11).
- When asset work is active: at least 2 and at most 3 Blender agents at once, on Opus 5.5 at xhigh effort, in `isolation: "worktree"`. Each new piece reuses approved helpers through module globals from a new family file (never edit an approved helper), and its runtime changes come back as a verified `integration.patch` for the orchestrator to land.
- The P3 recipe system (recipe book, hints, discovery moment, creative mode, the 6 recipes) is deferred until Paul says to start it (2026-10-20). P3 content keeps landing ahead of it.
- Blender headless only. Never record an owner approval yourself (pending entries only). Tool bugs and flaky tests are high priority. Perf numbers need host load ≤ 15 before and after.

## Waiting on Paul

All pieces landed since 2026-10-19 await look approval (sheets in `docs/checkpoints/`). Open questions, numbered as asked in the session:
- Station/rail: Q10 bogie or 4-wheel carriage; Q11 train palette; Q13 21% crossing ramp; Q14 gates open or closed; Q18 1.67 m unsupported branch run; Q19 mirrored turnout; Q20 red lever; Q27 bridge parapet height; Q28 provenance with several refs.
- Audio: Q21 bees on farm tiles; Q22 wheat rustle sounds chopped; Q23 windmill creak floor; Q24 two variants alternating; Q45 lamb bleats higher.
- Forest: Q25 log moss baked into bark; Q26 box spacing for long props; Q31 oaks on the pine tile; Q32 oak density; Q33 birds on scattered oaks; Q34 young fern wind; Q35 wide fern spacing; Q46 keep undergrowth out from under spruce crowns (59% of saplings within 2 m of a trunk); Q47 shrubs on the oak tile; Q48 shrubs report "shrub" not "tree"; Q60 holly too dark in shade; Q61 holly berries all year; Q62 relax `_spruce` height checks for a shorter sapling.
- Houses: Q29 longhouse per slot or fixed slot; Q30 one box or two for the L-plan; Q40 house_cluster thumbnails show variants (done) or the canonical houses; Q41 flagstone apron under the jetty.
- Oak: Q49 `_b` 0.27 m taller than the hero; Q50 random yaw for the oak tile hero.
- Farm: Q36 close the paddock gate; Q37 tinted rails or wood grain; Q38 paddock south of the yard or round it; Q39 three rails or two; Q42 grazing cow placement (3.9 cm float kept) or move the original cow; Q43 and Q59 grass clear under grazing muzzles (shrink or offset the disc); Q44 lamb texture to 512; Q55 fix the round bale's sector seams; Q56 stack_b top bale crosswise or along; Q57 grazing sheep placement (2.8 cm) or move the lamb; Q58 narrower hood so the grazing sheep's black face shows from above.
- Late: Q63 the third log changes the forest pick from 1-in-2 to 1-in-3, so logs, shrubs and saplings move on some seeds; Q64 existing worlds change look (about half the former longhouse cells now draw `house_cottage_b`); Q65 fixed: `house_cottage_b`'s porch posts block the walker (`walk.rs` `PIECE_POSTS`).
- Roads: Q51 shrink the 1024 shrub atlas (re-bakes `shrub_forest`); Q52 verge bush density (about 4 per straight tile); Q53 verge bushes on house/bakery tiles after parking-path clearance; Q54 verge bush variant mix.
- Older queues: `docs/review/2026-10-19-owner-queue.md` (Q1-Q12) and `docs/review/2026-10-14-owner-queue.md`.

Earlier carry-over still open: windmill review (card `01a13d667322`), prop variants (card `01a13d666eea`), colour grade pick (card `01a13c085851`), grass look and bulldoze dust, DESIGN.md debug-tab proposals, forest re-check on the deployed site.

## Filed follow-ups

- Rail network (Rail socket, tiles, switch routing, junction lever), card `01a1579e911c`.
- Station tile integration (platform walker ground, shelter obstacle, train stop), card `01a157994d7a`.
- Cars follow road height (`ROAD_SURFACE_Y` is a fixed 0.11 m; the crossing deck is 0.46 m), card `01a1575a45d4`.
- Large play worlds slow on web, card `01a14028862a` (high); Dependabot alerts, card `01a1547b6ac5747e967b94e245b57a6e` (high).
- Known small items: LOD2 dark neck patches on the grazing animals (draws past 400 m only).

## References

- `docs/agent-operability.md`: CLI flags, web query params, `window.hexWorld` bridge, script commands, shot targets.
- `.claude/skills/blender-asset/`: the asset pipeline; `references/dispatch.md` is the brief template (11 parts).
- `crates/hex-core/src/variant.rs` (hero variants), `crates/hex-play/src/play/scatter.rs` (`VARIANTS`, `FOOTPRINTS`, `GROUND_BOXES`, `FENCES`, verge band), `crates/hex-play/src/walk.rs` (`OBSTACLES`, `prop_kind`, per-part colliders), `crates/hex-render/src/kit_scene.rs` (kit showcase layout; `tile_counts` in `lib.rs`).
- CI: `.github/workflows/ci.yml` (auto-deploy to Pages on a `main` push).
