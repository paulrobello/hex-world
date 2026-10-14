# meadow_base friction log

Authoring agent: claude-code / claude-opus-5-5. Covers meadow_base and the grass and soil tileable sets.
Each entry gives what happened, the fix, and the minutes lost.

1. **The plan's mat skeleton writes `generator=__file__` into a committed bake report.** Under `-P`, `__file__` can be an absolute host path, which provenance must never carry. Fix: the generators write `pathlib.Path(__file__).resolve().relative_to(export.REPO)`, and the material-mode assembler re-hashes the generator from the file instead of trusting the report. Skill gap: `lib-contracts.md` documents the relative path for `write_export_report` but not for `write_bake_report`, which tileable sets use. 3 min.
2. **3D textures silently break the wrap.** `torus_coords` only wraps when every texture is 4D (Noise and Voronoi with `*_dimensions = "4D"` and W linked). The 3D default ignores W and mirrors the tile about its middle, and the 2×2 seam test does not show that as a seam. Fix: the generators build every texture through 4D helpers. Skill gap: `lib-contracts.md` should say "4D Noise/Voronoi/White Noise only; Wave, Magic, Brick, Gradient and Checker cannot wrap". 2 min.
3. **Soil strata and the wall mapping.** `kit_common::tile_frame` maps wall V to −y / tileM, so the cap rim (y = 0) sits exactly on the tile's V = 1/0 wrap, and tileM 2.0 repeats the strata twice down the 4 m skirt. Fix: the band coordinate is `fract(V − 0.05 + jitter)`, with V recovered as atan2(w, z) / 2π so it stays a function of the torus coordinates. The rim lands in topsoil, and the topsoil-to-gravel contact sits about 1.9 m below the rim instead of on the tile edge. The 2× repeat is a D2 consequence and is raised at the look-dev checkpoint. Skill gap: `project.md` should state the wall V convention and the rim/wrap alignment for anyone authoring a set that is banded vertically. 6 min.
4. **The worktree guard rejects heredocs and compound commands.** Python heredocs that patch files, and `cmd; python3 …; cargo …` chains, were refused as "too complex to verify". Fix: write the patch scripts as files, then run them with single commands. Not a skill issue, but worth one line under the skill's invocation rules for agents running in worktrees. 4 min.
5. **The first grass bake read as polka dots.** Voronoi tuft cells with a tight distance falloff gave round dots at 512 px/m instead of a clumpy turf. Fix: a soft tuft falloff at low weight, with high-distortion, high-frequency noise blades on top. Lesson for the skill: judge a tileable set from the 2×2 tile at 1:1 before compressing, and keep Voronoi for sparse features (clover, flowers, pebbles). 8 min.

## Folded into the blender-asset skill
1. `lib-contracts.md` (`write_bake_report`): a generator named in a bake report is repo-relative, never `__file__`; the report now also records `generatorDeps`.
2. `lib-contracts.md` (`torus_coords`, 4D textures only) and `pitfalls.md` ("Tileable sets").
3. `project.md` ("Walls under world-space tiling"): the wall V convention, the rim on the wrap, and the band offset. The 2× repeat was resolved by the owner (soil tileM 4.0).
4. `SKILL.md` ("Worktree sessions").
5. `SKILL.md` ("Self-review before the owner") and `pitfalls.md` ("Judge the 2×2 tile at 1:1").
