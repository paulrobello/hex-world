# sapling_spruce_b friction

1. **`_sapling.build` reinstalls its wrappers on `_forest`, so a variant's wrappers must go in
   `_sapling`'s namespace.** `_sapling.apply` writes `_sapling.branches/core_mesh/card_mesh` onto
   `_forest` at build time, overwriting anything set on `_forest` beforehand. `_sapling_b.py` replaces
   the names in `_sapling`'s module (and `_spruce.trunk_skeleton`, which `_spruce.build` looks up by
   name), keeps the originals in module constants, and asserts after the build that `_forest` holds its
   wrappers. Caught before the first build (advisor review), so no time lost.
   Proposed skill text (project.md, Variants, after the spruce sentence): "A sapling variant wraps
   `_sapling`'s own `branches`, `core_mesh` and `card_mesh` (and `_spruce.trunk_skeleton` for a bent
   stem), never `_forest`'s: `_sapling.apply` copies `_sapling`'s names onto `_forest` at build time
   (`_sapling_b.py`)."
2. **Fixed offsets in the spruce pipeline set floors on a short spruce.** `_forest.core_mesh` builds
   its rings from `Z0 + 0.25` to `ZTOP - 0.3` (literals), so `ZTOP - Z0` under 0.55 m turns the cone
   inside out without any assert firing; `_spruce.build` asserts every needle card above 0.6 m; and
   trunk wind U at 0.5 m grows as the tree shrinks (`TRUNK_SWAY * (z + 0.15) / (HEIGHT + 0.15)`), against
   the gate's U <= 0.1 below 0.5 m. `_sapling_b` prints the core's ring heights (and asserts they
   ascend) and the per-LOD maximum U below 0.5 m. At HEIGHT 1.7, Z0 0.92, ZTOP 1.55: core rings
   0.97-1.40 ascending, lowest card 0.70 m, U 0.075. First build passed; no time lost.
   Proposed skill text (pitfalls.md, Leaves, foliage and LODs, after "Conifer fronds"): "A spruce
   under about 1.8 m hits `_forest`'s fixed offsets: keep ZTOP - Z0 >= 0.6 (the core cone's rings run
   from Z0 + 0.25 to ZTOP - 0.3), the lowest whorl high enough that drooping cards stay above 0.6 m
   (level branches with a smaller droop let it go lower), and print the trunk's wind U below 0.5 m
   per LOD (the gate's still band, 0.1)."
3. **`_spruce.build` asserts LOD0 <= 3,000 only.** A variant's own tighter budget (here 1,000) needs its
   own check after the build (`_sapling_b.build` `lod0_max`). No time lost.
4. **Staging approved neighbours for the stills rewrote their committed bake reports** (as in
   sapling_spruce friction 3). `git restore` one file per call. About 1 minute. Already covered; the
   sapling_spruce entry's proposed edit ("an approved piece", not only bases) still applies.
5. **Play close-ups of a forest sapling landed inside the 16 m spruces.** The first play cameras, 4-5 m
   from a `sapling_spruce_b` instance, sat inside a neighbouring spruce's crown (frames of needle cards
   only), and the first "sun side" pick (azimuth 210 round the sapling) faced the sun, so it was backlit.
   Fix: print the variant's instances from `scatter::layout` (a temporary ignored test, removed before
   the diff), shoot four azimuths round each at 800x500 into one contact sheet, keep the clear frames,
   and check the camera's side against the sun (200, 400, 150): sun side is the +x+z camera. About 12
   minutes. Proposed skill text (SKILL.md, Owner checkpoints): "Close-ups of an undergrowth piece in a
   dense biome: probe four azimuths per instance on a contact sheet before the full run, and pick the
   sun-side camera from the +x+z side (the sun is at (200, 400, 150)); the kit layout's open undergrowth
   cell is the clean comparison."
6. **Walk heading 0 faces -Z.** A walk-into-the-sapling block probe with heading 0 walked away from it
   (`docs/agent-operability.md`: 0 toward -Z, 90 toward +X). Dropped: the obstacle row is pinned by the
   walk unit tests and the collider reach is 0.084 m. About 3 minutes. No skill change: documented.
7. **`git status --short` once listed only one untracked file in a new 19-file checkpoint folder.**
   `git status --porcelain --untracked-files=all` and `git show --stat HEAD` after the commit showed
   all 19. Check commits with `git show --stat`. About 2 minutes; no skill change (the worktree rules
   already say to verify with `git show --stat HEAD`).
