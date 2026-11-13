# Owner review queue, 2026-11-13

Requests waiting on you, collected overnight while you slept. Nothing here is approved. Paths are relative to the repo root, and a branch name means the file lives in that branch's worktree (`.claude/worktrees/<branch minus "worktree-">`). In-engine shots are taken in the 1:55–3:10 AM PST GPU window. This file lists their paths once they are taken.

## OC. Large granite boulder redo (`outcrop_granite`)

Branch `worktree-agent-a3ab85e959e4f444d`. The surface lines you said read as stamped are now a refined fracture network. Every gate is green on the branch (checkall 0). Its ROCK_BASES outline refit is committed with it.
- Review sheet, same views before and after: `docs/checkpoints/outcrop_granite/redo-sheet.jpg`.
- In-engine shots: `docs/checkpoints/outcrop_granite/redo-play/` and `redo-kit/` (pending capture).

- **OC-Q1. Does the new fracture network read as natural in engine?** Recommendation: approve if the play shots read as weathered granite and not as a pattern.
- **OC-Q2. Two outcrops in one view share one mesh.** They are rotated and scaled copies of each other, so the branching repeats. Ruling made overnight: a second variant, `outcrop_granite_b`, is being built ahead from the same approved refs, with placed outcrops alternating a/b per slot. It is pending your review, and nothing merges without it. The alternative is fainter or fewer fractures on the single mesh, which is cheaper.

## HL. Single-tile hills and peaks (P4 A40 O1), built ahead

Branch `worktree-agent-a5c9b226ddc2c72f5`. The spec, `docs/superpowers/specs/2026-11-13-single-tile-hills-design.md`, is itself pending your approval. Two pieces, `hill_meadow` (top 9.4 m) and `peak_granite` (top 17.7 m), go in the `hero` pack, so the initial download is unchanged. Every gate is green apart from `kit-provenance`, which fails, as it must, until you pick references (R2). The runtime changes are in `docs/checkpoints/hill_meadow/integration.patch`. The steps after approval are in `docs/checkpoints/hill_meadow/integration.md`.
- References: `assets-source/refs/hill_meadow/ref-1..4.jpg` and `assets-source/refs/peak_granite/ref-1..4.jpg`.
- Review sheets: `docs/checkpoints/hill_meadow/review-sheet.jpg` and `docs/checkpoints/peak_granite/review-sheet.jpg`.
- Thumbnails and in-engine shots: `docs/checkpoints/hill_meadow/` (pending capture).

- **HL-A1. Approve the hills spec?** Recommendation: approve.
- **HL-Q1. Reference picks.** Recommendation: ref-1 for both pieces.
- **HL-Q2. Pines on the hill?** Recommendation: none, matching the references. The peak keeps a pine band.
- **HL-Q3. Pack.** Recommendation: `hero`. The alternative is a new `hills` pack.
- **HL-Q4. The peak's rock.** The builder noted that the pinnacles read as rounded blocks with blotchy tint, not ref-1's jointed crag. Ruling made overnight: a jointing pass is being built ahead as an alternative (`docs/checkpoints/peak_granite/jointing-sheet.jpg` when done), so you can compare the two. Recommendation: pick whichever reads closer to ref-1 in engine.

## MT. Mega tiles (massif) merge notes

- **MT-N1. Tangent bytes changed since your C3 approval.** The massif's exported tangents were not deterministic, because Blender's threaded MikkTSpace gave a different result on each run of a large mesh. The export now runs single-threaded. The GLB bytes changed but the look did not. A parity proof against main is running, and its results go here before the merge.
