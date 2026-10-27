# Dispatching an asset agent

For the orchestrator. Every Blender asset agent gets a brief with these ten parts. Each part
answers something past agents had to rediscover (review of 28 runs, 2026-10-14,
`docs/review/2026-10-14-asset-agent-review.md`). Dispatch on Opus 5.5 at xhigh effort with
`isolation: "worktree"`. When a revision follows owner feedback on a piece, resume the agent
that built it instead of starting a fresh one.

1. **Goal.** The piece ids, the owner's words and screenshot path, and the approved reference
   folder with its `approval.json` notes. Where the refs and the owner's answer disagree, the
   answer wins (the windmill is white per A17, though the refs draw it black).
2. **Budget.** "Finish in N minutes; hand back what is fully gated." LOD0, LOD1 and LOD2 targets
   per piece from the `gates.md` floor formula and the ceiling that applies (15%, or 20% for
   lane-cut and coast-arc bases), plus the KTX2 class. Never hand out PLAN numbers that predate
   the formula.
3. **Base.** The main commit that holds every contract, skill section and helper the brief
   cites. Never cite a draft that is not on main.
4. **Bootstrap.** Three calls:
   - `git merge --ff-only main`, then `git log --oneline -1` and confirm the SHA.
   - `./scripts/worktree-bootstrap.sh` (add `--web` when the brief needs the e2e gates; agent worktree guards refuse the `bash scripts/...` form). It
     clones main's `target/` copy-on-write and cleans `hex-core`/`hex-assets`, clones or installs
     `web/ui`, `scripts/assets` (and `e2e`) node_modules, and installs the KTX tools. About 5 s;
     a second run is a no-op. Start it only when no cargo build is running in main.
5. **Ownership.** The files the agent owns: its generator, its `_<family>.py`,
   `assets-source/kit/<id>/` and `assets/kit/p1/<id>/`. The manifest entry, layout placement,
   `data/` edits, gate changes and proposed skill text go in the report for the orchestrator to
   land (SKILL.md, "One writer for shared files"). A solo agent may land them itself; say which.
6. **Rules.**
   - Headless Blender only. No kanban, no push, no co-author trailer. Never record approvals;
     append a `pending` entry.
   - Gates take two calls: `make <gate> > <log> 2>&1; echo "EXIT=$?"`, then a separate
     `grep … <log>` or `tail`. Never attach the inspection to the gate line.
   - Hook-safe commands per SKILL.md, "Worktree sessions".
   - Shots never wait on host load; only timing measurements do.
   - The binary is `target/release/hex-world`.
   - Scratch files go under `<scratchpad>/<piece>-<pass>/`.
7. **Gate path.**
   - Iterate with `HEXW_SKIP_BAKE=1 make kit-build PIECE=<id>`.
   - Then the full `kit-build`, `kit-compress`, `kit-determinism` (never two at once) and
     `kit-renders`.
   - Then, by whoever owns the manifest: the manifest entry, `kit-provenance REFS=…`,
     `kit-check` and `checkall`.
   - After a size or footprint change: `cargo test -p hex-play -p hex-render --lib -- boids
     scatter walk kit_scene` (walk and scatter live in hex-play, as `project.md` gives it).
   - A piece that blocks the walker or changes a tile's scatter layout also runs `make ui-e2e`:
     its first-person walk check walks from a meadow toward the tile centre, and B1's outcrop
     stopped it short of 3 m after B1 had passed every other gate (2026-10-27).
8. **Before review.** The review sheet in `docs/checkpoints/<piece>/` (SKILL.md, "Owner
   checkpoints"). Flag any new gate branch before building (SKILL.md, "A new kind of piece
   needs its gate branch first").
9. **When stuck.** If a contract and a budget cannot both hold, stop and hand back a measured
   options table (tris, errors, a still per option) for the owner to pick from. This worked for
   the river-bend LOD2, the P3 references and the forest-edge corners.
10. **Report.** Tris per LOD, sizes and texel densities per piece; each gate's exit code and
    tail; review-sheet paths; friction entries and proposed skill text; open owner questions.
11. **Integration.** What the piece changes at runtime, each marked handled, not applicable, or
    needs work: car height and path (`ROAD_SURFACE_Y` is a constant 0.11 m), walker collider and
    `walk::prop_kind`, grass clear zones (`scatter::GROUND_BOXES`), boid perches, sim nodes and
    clearances, thumbnails, the draw budget, and sound (does the item want an occasional
    one-shot pool or an event cue? The orchestrator generates it under the owner's standing
    approval of 2026-10-19, following `assets-source/audio/README.md`). The orchestrator fixes or files every "needs work"
    item before the piece is called done (owner, 2026-10-19: the crossing deck sat 0.35 m above
    the fixed car height and was found only after the build).
