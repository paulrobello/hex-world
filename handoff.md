# Hex World handoff (2026-10-21)

Start here in a fresh session (Claude Code or Claude Desktop). Then read `CLAUDE.md`, `PRD.md` §15 and `.claude/skills/blender-asset/SKILL.md` before any asset work.

## State

- `main` holds batch 26 (pushed, CI watched, auto-deployed); batch 25 is `d908e36`, CI green (run 40324963259). `dist/` rebuilt after each merge.
- Batch 26 landed: the owner's A16/A17 approvals (21 pieces), the bulldoze dust reverted to its previous size (owner A9), the close-camera MiniMac frame times (`docs/perf/close-camera-draws/`), A3/A18 answers in the PRD.
- Batch 25 landed: close-camera draws (screen-aware cascade skip, runtime slot coalescing per piece, per-tile culling): 20-tile overview 419 -> 335, forest cluster 633 -> 436, meadow 645 -> 497; MiniMac 2,066-tile close camera -3.7 ms web mean, -4.4 ms native. Cards closed: close-camera draws, draw budget.
- Batch 24 landed: kit-compress spawns the UASTC encode with ASLR off (libktx 4.4.2's RDO selector-history hash reads struct padding), so identical PNGs give identical KTX2; every committed KTX2 is pre-pin and 287 of 321 maps change on re-encode (re-shoot thumbs after).
- Batch 23 landed: overview-only 500-draw gate and scatter 60 per LOD; one-LOD band shadows; LOD0-through-LOD1 fallback; A14 engine feedback; road_bend_60 trim (A7); water review knob and options page; cars queue at crossings; every LOD0 slot in LOD1 (16 pieces, new kit gates); stopped rail ends accept non-rail tiles; beach scatter 0-6; R8 share image; share image left out of the initial-download count.
- Batch 22 landed: the instanced chunk path cross-fades settled pieces per part over the 10 m LOD bands (MiniMac timing in `docs/perf/lod-band-instanced.json`: no cost at the 152 m band camera, about +1 ms p95 on the 7-forest world at 130 m; draws 616 -> 748 at 152 m); cars over a lowered tier cap fade out over 250 ms (instant under reduced motion), with the `set_car_cap` test hook (owner: stays callable from the web), the fade evidence in `docs/checkpoints/cars-fade-spur/` and a web-smoke case drawing the blended car pipeline under the 16-texture clamp; destination spurs drive at most 20 deg/tick (bakery spur 3 m, every bakery trip at most 18.85 deg/tick on the sliding maximum, owner A13).
- Batch 21 landed: the saves follow-up (load-confirm autosave hold ends after 20 s; Replace's toast has Undo restoring the slot byte for byte), the UI motion work (Settings Controls Reduce motion: Follow system or On; tile pictures turn on R; the engine feedback pass on the HUD motion tokens; `cssMs` read minified `.24s` as 0.24 ms, fixed), the grass slider drag measurement (no hitch, no rebuild), the initial-download PNG monitor warning, the owner answers and the approvals of 27 pieces and 2 cues. Cards closed: saves follow-up, UI motion, grass density, road bend crease.
- Batch 20 landed: kit prepass COLOR_0 normal fade (mouth fan SSAO), `e2e/initial-download.mjs` in web-smoke (runtime matches the gate per texture family; PNG 29.59 MiB, headroom 433 KB), parallel single-threaded KTX2 encodes (2x faster, byte-identical), the saves screens S10.3/S10.6/S10.8, road_bend_60 crease verified gone (card blocked on re-approval), and `scripts/worktree-bootstrap.sh` no longer runs git (agent guards refused it). Cards closed: prepass fade, initial download, parallel encode, saves screens. No agents running; `git worktree list` shows main only.
- Batch 19 landed: kit-compress pins the UASTC encode to one thread, so KTX2 output is byte-deterministic (existing pieces shift bytes once on their next rebuild, look unchanged at 72.6 dB PSNR); fixed-clock boids runs are repeatable (compute pass reads a previous-frame copy, fish tracker folds readbacks in frame order). Cards closed: KTX2 determinism, boids repeatability, brake third (already done in batch 13).
- Batch 18 landed: kit pipeline hygiene (6 criteria, card closed), fish crossings through `river_mouth_l` in shots-boids (fish card closed), PRD drift items 1, 2, 5, 6 and half of 4 (car cap by tier, LOD dither in the colour pass, CI runs initial-download, ambience default 0.1, wind comment; card blocked on the owner), and the approvals page now lists pieces whose newest entry is a pending re-approval (27 pieces: the wells and 25 rebuilds). The Blender skill is promoted to user-level `~/.claude/skills/blender-kit-pieces` (hex-world's `blender-asset` is its marked overlay; skill card closed).
- Batch 17 landed: kit draws bin on a stable mesh-family key, so `make thumbs` is fully byte-reproducible (thumbs card closed; the road_bend_60 workaround is gone), and `scripts/shots/run.sh` checks frames with the Pillow check. 
- Batch 16 landed: version.mjs deflaked (waits on check outcomes; 10 runs at load 248-597), bakery visits under 20 deg/tick at rotations 0 and 3, `make shots-piece` (committed shot runner with a Pillow black-frame check, `make shots-test` in checkall), perch tests list every failure at once, forest-play poses out of the canopy, and the kit skipping shadow cascades nothing reads (20-tile overview 649 -> 407 draws). Cards closed: version flake, bakery turns, shot runner, oak perch test, forest-play poses, Dependabot (all 4 alerts already fixed 2026-10-19). Draw-call card blocked on draw Q1-Q4.
- Batch 15 landed: the stone well (`well_stone` covered, `well_stone_b` open in about one cluster in three) on house_cluster's lawn with the owner's WQ1-WQ3 answers, birds roosting on its ridge (3 perches), and the `well-windlass` one-shot pool (attempts 69-71, cap 71). The well card is closed. `docs/review/approvals.html` lists the two well pieces and the two well-windlass cues for the owner (serve with the `hex-world-review` launch entry on 8095).
- Batch 14 landed: the water drying trail on kit beaches and churn foam on river bends and the fork (water card criterion 3), and the HUD state-change feedback audit with the engine reduced-motion flag (UI card criterion 3). The web-smoke load flake card closed (3 passes at load 44-116). Water card criterion 6: `docs/perf/p2-water-world-2026-10-20.json` (web p95 10.3-10.8 ms on the 208-tile water world, +1.3 ms from the foam, budget held), blocked on water Q4.
- Batch 13 landed: river_bend_60 (the tight river bend), the buffer stop runtime (stops at open rail ends, buffer clunk, carriage order, give-way livelock fix), the boats runtime (rowing and sailing boats, landing stages, oar and sail sounds), rowing boats through river forks (link runs between arms, a seeded arm pick, one moving rowing boat per river system) and round the tight bend, boats kept out of tile thumbnails (`hex_play::boats::NoBoats`), the merged audio records (attempts 1-68), and the owner's approvals of every piece and cue. Cards closed: boats, buffer stop, river fork, whistle redo, river bend 60.
- Working tree clean apart from the untracked `.claude/launch.json`. 
- The review page generator is `python3 docs/review/approvals.py` (writes `docs/review/approvals.html`: every piece or cue without an owner approval). It is empty now.
- The boats-fork change also fixed a 0.83 m one-tick jump in `rower_pose` (a turn-back begun while easing back from a pass lane), so rowing poses in worlds with a landing stage differ from the `docs/checkpoints/boats-play/` shots frame for frame.

## Next asset work

No Blender agents are running. The stone well landed in batch 15; no other P3 piece has an approved reference set, so nothing is queued until the owner names a new piece. Recipes are deferred and nothing past P3 starts until Paul says so.

## Standing rules from the owner (still in force)

- Lifted 2026-10-25 (owner: "all asset creation and deploy limits are lifted, resume work"): the 2026-10-21 no-new-asset-work rule and the stop-new-work rule. Asset work and the two-to-three Blender agents rule are back in force; the three agents stopped on 2026-10-21 were resumed in their worktrees.
- The 2026-10-20 stop rule ("dont start anything new") was lifted by Paul's "continue work" the same day; normal throughput resumed.
- Fix friction or defects of any scope found along the way, don't only file them (Paul, 2026-10-20: "you can do fixes to any scope needed").
- Standing push approval when a batch is a significant addition or change, after `make checkall-ci` passes; watch CI after. Small or doc-only commits ride along with the next push. `make build-web` after every merge to `main`.
- Standing approval to generate sounds for new items; the owner still approves each by ear.
- Every asset brief carries the integration section (`.claude/skills/blender-asset/references/dispatch.md` part 11).
- When asset work is active: at least 2 and at most 3 Blender agents at once, on Opus 5.5 at xhigh effort, in `isolation: "worktree"`. Each new piece reuses approved helpers through module globals from a new family file (never edit an approved helper), and its runtime changes come back as a verified `integration.patch` for the orchestrator to land.
- The P3 recipe system (recipe book, hints, discovery moment, creative mode, the 6 recipes) is deferred until Paul says to start it (2026-10-20). P3 content keeps landing ahead of it.
- No work past P3: nothing from P4 (elevation) onward (P5 atmosphere, P6 content breadth, P7 mobile, P8 first-person) starts until Paul says so (2026-10-20). Owner-requested items that fit P2 or P3 still go ahead.
- Book shared-host windows on the kanban scheduler before load-sensitive or GPU work (global rule, 2026-10-25): `kanban schedule book --resource macdaddy|minimac ...`, check in, heartbeat, cancel when done. Sub-agents may not run kanban, so the orchestrator books the window for each agent wave (GPU shots, thumbs, e2e, kit bakes) and for every MiniMac perf run before dispatching it.
- Blender headless only. Never record an owner approval yourself (pending entries only). Tool bugs and flaky tests are high priority. Perf numbers need host load ≤ 15 before and after.

## Waiting on Paul

- A19: which Blender pieces next (no piece has an approved reference set, so the two-Blender-agents rule cannot be met until the owner names pieces; candidates: fish and bird kit pieces, the 7-hex mega tile, new P3 pieces).
- A20: file the libktx 4.4.2 padding-hash bug upstream (KhronosGroup/KTX-Software, public GitHub issue) or not.
- A4 UI hero-polish hallway test and sign-off; A9 grass blades look sign-off.
- Non-blocking tuning lists from 2026-10-20 and the older numbered lists below each ship with a default.

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
