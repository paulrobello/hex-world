# Hex World handoff (2026-10-20)

Start here in a fresh session (Claude Code or Claude Desktop). Then read `CLAUDE.md`, `PRD.md` §15 and `.claude/skills/blender-asset/SKILL.md` before any asset work.

## State

- `main` at `d5cbad7` is pushed, CI green (run 39540131987), auto-deployed (batches 11-13 on 2026-10-20). `dist/` rebuilt from it.
- Batch 13 landed: river_bend_60 (the tight river bend), the buffer stop runtime (stops at open rail ends, buffer clunk, carriage order, give-way livelock fix), the boats runtime (rowing and sailing boats, landing stages, oar and sail sounds), rowing boats through river forks (link runs between arms, a seeded arm pick, one moving rowing boat per river system) and round the tight bend, boats kept out of tile thumbnails (`hex_play::boats::NoBoats`), the merged audio records (attempts 1-68), and the owner's approvals of every piece and cue. Cards closed: boats, buffer stop, river fork, whistle redo, river bend 60.
- Working tree clean apart from the untracked `.claude/launch.json`. No agents running; `git worktree list` shows main only.
- The review page generator is `python3 docs/review/approvals.py` (writes `docs/review/approvals.html`: every piece or cue without an owner approval). It is empty now.
- The boats-fork change also fixed a 0.83 m one-tick jump in `rower_pose` (a turn-back begun while easing back from a pass lane), so rowing poses in worlds with a landing stage differ from the `docs/checkpoints/boats-play/` shots frame for frame.

## Next asset work

No Blender agents are running: every P3 piece waits on the owner. The stone well (card `01a15bdfd04e`) needs WL-A1 (approve `assets-source/refs/well_stone/` refs 1-3, sheet `docs/checkpoints/refs-well/refs-sheet.jpg`) and WQ1 placement (recommended: one well on house_cluster's lawn, farm yard fallback), WQ2 covered piece with the open one as a `_b` variant, WQ3 1.5 m wide, 0.9 m tall, 2.4 m ridge. Recipes are deferred and nothing past P3 starts until Paul says so.

## Standing rules from the owner (still in force)

- The 2026-10-20 stop rule ("dont start anything new") was lifted by Paul's "continue work" the same day; normal throughput resumed.
- Fix friction or defects of any scope found along the way, don't only file them (Paul, 2026-10-20: "you can do fixes to any scope needed").
- Standing push approval when a batch is a significant addition or change, after `make checkall-ci` passes; watch CI after. Small or doc-only commits ride along with the next push. `make build-web` after every merge to `main`.
- Standing approval to generate sounds for new items; the owner still approves each by ear.
- Every asset brief carries the integration section (`.claude/skills/blender-asset/references/dispatch.md` part 11).
- When asset work is active: at least 2 and at most 3 Blender agents at once, on Opus 5.5 at xhigh effort, in `isolation: "worktree"`. Each new piece reuses approved helpers through module globals from a new family file (never edit an approved helper), and its runtime changes come back as a verified `integration.patch` for the orchestrator to land.
- The P3 recipe system (recipe book, hints, discovery moment, creative mode, the 6 recipes) is deferred until Paul says to start it (2026-10-20). P3 content keeps landing ahead of it.
- No work past P3: nothing from P4 (elevation) onward (P5 atmosphere, P6 content breadth, P7 mobile, P8 first-person) starts until Paul says so (2026-10-20). Owner-requested items that fit P2 or P3 still go ahead.
- Blender headless only. Never record an owner approval yourself (pending entries only). Tool bugs and flaky tests are high priority. Perf numbers need host load ≤ 15 before and after.

## Waiting on Paul

New today (2026-10-20, later): flatten Q1 meadow mostly flat at ring height, Q2 hollow depth 0.25/0.18/0.15, Q3 forest root pits; farm motion Q1 TAA leg ghosting, Q2 standing animals graze, Q3 trot vs four-beat, Q4 tuning values; river fork Q1 plain Y vs round pool, Q2 flow rule, Q3 three-spring edge cases, Q4 crisp ridge; rail bend 120 Q1 single 24 m arc; river bend 60 Q1 LOD2 0.033 m water error, Q2 seventh river-dressing budget, Q3 name; boats Q1 keep sailing routes apart, Q2 boats re-derive on any water edit, Q3 anchored sails set 10 degrees, Q4 boat-sail-2 double luff; whistle retake 0.5 s lead-in.

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
