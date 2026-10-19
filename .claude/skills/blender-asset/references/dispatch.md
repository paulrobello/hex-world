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
4. **Bootstrap.** One step per tool call:
   - `git merge --ff-only main`, then `git log --oneline -1` and confirm the SHA.
   - `cp -cR /Users/probello/Repos/hex-world/target ./target` when no build is running in main,
     then `cargo clean -p hex-core -p hex-assets`.
   - `cp -cR` `web/ui/node_modules` and `e2e/node_modules` from main when the brief needs the
     web gates.
   - Main has no `tools/` or `scripts/assets/node_modules`: the first `make kit-compress`
     installs both. Run `npm ci --prefix scripts/assets` only for `make refs` or the contact
     sheet.
   - Once the bootstrap script card lands, this whole step is one plain command.
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
   - After a size or footprint change: `cargo test -p hex-render --lib -- boids scatter walk
     kit_scene`.
8. **Before review.** The review sheet in `docs/checkpoints/<piece>/` (SKILL.md, "Owner
   checkpoints"). Flag any new gate branch before building (SKILL.md, "A new kind of piece
   needs its gate branch first").
9. **When stuck.** If a contract and a budget cannot both hold, stop and hand back a measured
   options table (tris, errors, a still per option) for the owner to pick from. This worked for
   the river-bend LOD2, the P3 references and the forest-edge corners.
10. **Report.** Tris per LOD, sizes and texel densities per piece; each gate's exit code and
    tail; review-sheet paths; friction entries and proposed skill text; open owner questions.
