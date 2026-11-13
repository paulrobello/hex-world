# hill_meadow friction log

The single-tile landform family (`scripts/blender/kit/_hills.py`, hill_meadow and peak_granite).
Entries that concern the family are here; `assets-source/kit/peak_granite/friction.md` holds the
peak's own.

1. **A family helper edit stales its sibling's build.** Both pieces import `_hills.py`. The peak's
   last form pass edited it after the hill's full build, so `kit-provenance --check` refused the
   hill ("_hills.py changed since the build"), although the peak-only edit left the hill's output
   byte-identical (GLB SHA-256 `3f568986…` before and after the rebuild). Fix: rebuild the hill
   after the last helper edit (one minute here). Lost: 5 min. Proposed `SKILL.md` text, Rules:
   "Pieces that share a family helper (`kit/_<family>.py`) are rebuilt after the last edit to it,
   every one of them: `kit-provenance` refuses a helper that changed since a piece's build."
2. **The LOD floor binds a small landform.** The first hill build failed LOD2 536 > 15% of LOD0
   3548: the ring and skirt fix about 286 LOD2 tris, so the cap's own LOD2 share has to come off
   the interior. Fix: grid spacing 0.66/1.05/2.8 m, giving 3684/1758/486. Lost: 5 min. No skill
   change: `gates.md` already states the floor.
3. **A dome profile with power 1 reads as a cone.** `HILL_H · sstep(t)` with t = d / (d + r) gave a
   pointed crown in the stills; `sstep(t)^0.6` gives the broad dome ref-1 shows. Lost: one form
   pass, about 10 min. No skill change: piece-local.
4. **The flat rim band is only as flat as the LOD0 grid.** The grass function is exactly flat
   within 0.6 m of the border, but the cap's 0.66 m triangles reach from the border to the first
   inner row, so the ground rises 0.12 m at 0.3 m in and 0.25 m at 0.6 m in (the peak 0.05 and
   0.10). The massif's rim test allows 0.3 m at 0.6 m in, and the walker test holds the
   landforms to it. Lost: 10 min. No skill change.
5. **A Meadow scatter rule can have no free ground.** The outcrop rule's footprint finds no cap
   flat and wide enough on the peak, so its share is 0 and the density test divided 0 by 0. Fix: the
   test asserts a zero-area rule plans nothing. Lost: 5 min. No skill change.
6. **The review renderer's second import is renamed.** Importing the meadow GLB six times named
   the later nodes `meadow_base_lod0.001`, so a lookup by name failed. Fix: import once and
   `copy()`. The neighbours sit along the edge normals at 60k degrees (`hexgeo.edge_normal`), not
   the corners' 30 + 60k. Lost: 10 min. No skill change: `docs/checkpoints/hill_meadow/review_render.py`
   is piece-local.
7. **`Warning: No mesh data to join` prints twice in every build.** It comes from `geom.join` with
   a one-object list in `_massif.py`'s island pass (LOD1); the joined result is that object, and
   every gate passes. Lost: 2 min. No skill change; `lib/geom.py` could skip the join for one
   object.
8. **The worktree isolation refuses compound shell commands.** Heredocs, `sed -i` chains and
   multi-command pipelines in this worktree were refused as unverifiable. Fix: scratch scripts
   written with the Write tool under `/tmp/hw-hills/`, edits with the Edit tool. Lost: 5 min. No
   skill change: a harness rule.
9. **A stale base fails the lint leg.** The branch predates main's `af7c4be` (Rust 1.100 lints in
   `coalesce.rs` and `hex-play-state`), so `make lint` failed in a file this work never touched.
   The lint leg ran green with that commit applied to the working tree and reverted after. Lost:
   5 min. No skill change.
