---
name: blender-asset
description: Hex World kit-piece authoring pipeline in Blender. Use for any Hex World 3D asset work, such as writing or running a scripts/blender/kit/<piece>.py generator, sculpting a hero piece through the Blender MCP, baking maps, building LODs, colliders and sockets, exporting a GLB, running make kit-build / kit-compress / kit-check, writing provenance, or debugging a failed asset gate. Triggers include "kit piece", "blender generator", "bake", "LOD", "socket", "hex base", "kit-build", "kit-check" and "provenance".
---

# Hex World Blender asset pipeline

You author ONE kit piece (or one tileable material set). Read `references/stages.md` before touching Blender, and read `references/pitfalls.md` before your first bake.

This skill is project-level for now and a later card promotes it to user level. The rules below are the portable pipeline. Everything specific to Hex World (frame, constants, seam rules, texture channels, budgets, paths and make targets) lives in **`references/project.md`**, and you must read it too.

## Rules
- **Reference first.** Modeling starts only after the owner has approved the Higgsfield reference images for the piece (`make refs PIECE=<id>`, then the owner's `approval.json`). No approved reference, no high-poly.
- **Effort.** Every Blender asset subagent is dispatched at high effort (owner, 2026-10-13). In Claude Code, pass `effort: "high"` on the `Agent` call.
- **Concurrency.** At most 3 Blender asset subagents run at once (PRD §11.1). Own exactly one piece id. Use a scene and collection named after it, and the staging dir `target/kit-stage/<piece>/`.
- **Invocation.** Always `make kit-build PIECE=<id>`, or `"$BLENDER" -b --factory-startup --python-exit-code 1 -P …`. Blender exits 0 on tracebacks without the flag.
- **Worktree sessions.** A worktree-isolation hook refuses commands it cannot verify: `for`/`while` loops, `;`-chained git commands with pipes, long inline heredocs and long `node -e` programs, `sed` with shell variables, and `magick` with parenthesised sub-images (use a small PIL script for tile previews). Simple `cd <worktree> && make …` and `git add … && git commit …` chains pass. Write patch scripts and edits as files, then run each with one plain command.
- **Helpers.** Import helpers from `scripts/blender/lib/` (contracts in `references/lib-contracts.md`). Do not re-implement the frame math, the border ring, bakes or export. Real piece generators never import the `tests` package. Shared per-family code (`kit/_<family>.py`) is fine: provenance hashes it as `generatorDeps`.
- **Frame, pivot, seams, textures and budgets.** Follow `references/project.md` exactly. In short: Blender (x, y, z) exports as runtime (x, z, -y), the pivot is the hex center (trees: the trunk base), border-touching pieces are never decimated, and the GLB embeds no images.
- **Provenance.**
  - Every piece has `assets-source/kit/<piece>/provenance.json` and every set `assets-source/kit/materials/<set>/provenance.json` (`references/provenance.md`), with `authoringAgent = {"harness": "claude-code", "model": "claude-opus-5-5"}`.
  - `generator` and `generatorDeps` are mandatory and re-hashed by the gate. Once a piece is approved, editing its generator or a `_*.py` helper (even a comment) fails `make kit-check` until it is rebuilt and re-approved.
  - `.blend` files and bake intermediates live only in the main checkout's blend store, `assets-source/blend/<piece>/` (gitignored). Record each one's SHA-256. External `.blend` storage is deferred: do not upload them anywhere or build an upload step.
- **Timing.** Every timing measurement (bake time, build time, frame time) runs with host load ≤ 15 before and after, checked with `node e2e/perf-load.mjs`. A number taken outside that window is not reported. Build first and guard only the measured or shot run: a wasm rebuild alone can push the load from 14 to 50.
- **Self-review before the owner.** Judge a tileable set from its 2×2 tile at 1:1 before compressing, and a piece from stills against its approved reference. Read the numbers too (AO mean and p05, alpha coverage per mip, texel density per slot), not just the image.
- **Owner checkpoints use screenshots.** Present the 6-view render and the in-engine shot as image files the owner can open. Never ask for approval from a text description.
- **Done.** You are done when `make kit-check` passes and the 6-view render plus the in-engine shot exist. Owner approval is recorded by the controller. Never record it yourself.
- **Friction.** Log every snag in `assets-source/kit/<piece>/friction.md` (what happened, the fix, the minutes lost). Fold each one into this skill (a rule, a stage command or a pitfall), or note in the entry why no change is needed.

## Stage order
approved reference → high-poly → LOD0 + UVs → bake → LODs + collider → sockets + export → compress → gate → provenance → owner review. Commands for each stage are in `references/stages.md`. Hero pieces take the interactive route for the high-poly only (`references/stages.md`, "Interactive route").

## References
- `references/project.md`: Hex World frame, constants, seams, textures, budgets, paths, the blend store, tree wind data and tool pins.
- `references/stages.md`: the command for every stage, plus the interactive (Blender MCP) route.
- `references/lib-contracts.md`: the `scripts/blender/lib/` function contracts.
- `references/gates.md`: every gate check and its threshold.
- `references/provenance.md`: the provenance schema and what fails `make kit-check`.
- `references/pitfalls.md`: known Blender, bake, tileable-set, foliage, export and KTX2 traps.
