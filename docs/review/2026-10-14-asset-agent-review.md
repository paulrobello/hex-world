# Asset sub-agent review, 2026-10-14

One reader (Opus) over 28 distilled asset sub-agent transcripts (P1 foundation, road, house,
meadow props, masks, river family and mouths, ocean, beach, forest edge, fish and starlings,
oak polish rounds, cattails, windmill, P3 references), against the `blender-asset` skill and the
40 `friction.md` logs. Transcripts were distilled with the user-level `transcript-distill` skill.
Claims were checked against the raw JSONL and the repo. Times marked "about" are estimates
from call-to-result gaps.

## Findings, ranked by time or risk

| # | Finding | Runs | Cost | Capture |
|---|---|---|---|---|
| F1 | Screenshot runs waited on host load that never dropped; most waits timed out and ran anyway at load 30-290 | 6 | about 1.5-2 h | Skill: shots are not timing measurements |
| F2 | The worktree hook refused 144 commands; the skill's description of what it refuses was wrong in both directions | 28 | about 1-2 h of turns | Skill: exact hook-safe rules |
| F3 | Brief bootstraps prescribed refused or impossible steps (git -C main, copying tools/ that main lacks) | 28 | 2-5 turns each | Dispatch template; bootstrap script card |
| F4 | Parallel agents collided in manifest, kit_scene layout, kit_gate, pending-pieces and the skill files (20 conflicts) | 6 | 10-20 min each | Skill: one writer for shared files |
| F5 | Owner redo loops on views nobody shot (shade side, close-up, eye level, scale) | oak about 6 rounds, cattail, mouth, fish, verge | whole dispatches | Skill: one standard review sheet |
| F6 | Gate assumptions found after the build (new piece kinds) | 8 | 10-15 min each | Skill: gate branch first |
| F7 | Release binary typed as target/release/hex-app (it is hex-world) | 6 (10 times) | a shot run each | project.md |
| F8 | Runtime couplings (perches, footprints, walk obstacles, layout heights) found by failing tests | 4 | 10-20 min each | project.md; perch test card |
| F9 | HEXW_SKIP_BAKE=1 fast loop undocumented outside one paragraph | n/a | | project.md |
| F10 | LOD budget hunting (spring 19 builds, mouth-bases 10, K7 8) | 5 | long loops | Dispatch template; lib.lod card |
| F11 | pipefail guard tripped on inspection greps attached to gate lines | 19 (34 times) | a turn each | Dispatch template two-call pattern |
| F12 | Review evidence left in gitignored target/ or stale after a rebuild | 3+ | rework | Review sheet; shot runner card |
| F13 | Briefs cited drafts not on main | 3 | rebuilds | Dispatch template: name the base commit |
| F14 | Zero-length tangents surface only at kit-compress | 3 | rebuilds | Tangent check card |
| F15 | kit-compress exits 0 on a stage with no GLB and deletes the shipped GLB | 1 | silent deletion | Existing kit-compress card |
| F16 | web-smoke flakes on a loaded host (sampled textures and compat counts before the IBL loads) | 2 | 5-7 min reruns | Existing web-smoke card |
| F17 | Helpers frozen by hash block comment fixes and force re-provenance cascades | 3 | 25-30 min | Comment-stripped hash card (owner approval) |
| F18 | Stopping with a measured options table worked (K5 LOD2, P3 refs, forest edge corners) | 3 | saved time | Dispatch template: expected behaviour |

## Worth keeping as defaults

- Tight sub-briefs with an ownership split (river mouth centres 11 min, bases 31 min, no
  conflicts).
- The `HEXW_SKIP_BAKE=1` geometry loop.
- Before/after pixel diffs against a run-twice noise floor.
- Measuring before fixing (canopy-gap colours, road-verge luminance ratios).
- Reference versus still versus engine sheets.
- Mutation-testing new gate checks.
- Pending approval entries only.

## Where each change landed

- `.claude/skills/blender-asset/SKILL.md`: shots without the load guard, hook-safe command rules,
  one writer for shared files, the review sheet, gate branch first, friction folding under
  parallel agents.
- `.claude/skills/blender-asset/references/stages.md`: the shots step.
- `.claude/skills/blender-asset/references/project.md`: fast loops, the binary name, runtime
  couplings, ktx by hand in a worktree.
- `.claude/skills/blender-asset/references/dispatch.md`: the brief an orchestrator gives every
  asset agent.
- Tool cards on the hex-world board (high priority): worktree bootstrap, shot runner, tangent
  check, LOD helper, perch test message, comment-stripped hashes; kit-compress and web-smoke
  cards updated.
