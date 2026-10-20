# bush_verge_b friction

1. **A radius is not a width.** `reachM` in the bake report (and `_shrub.build`'s `reach`) is the
   widest LOD's radius from the pivot, and an oval or leaning crown makes it say little about the
   plan size. The first trial (`oval` 0.84) printed reach 0.524 and looked in range, but the LOD0
   plan was 0.91 x 0.71 m, under the brief's 0.8 m floor. Measured per-axis extents from the GLB
   (a scratch `measure.py` over the staged GLB) and set `oval` 0.98. Lost: about 4 minutes (two
   builds). Proposed skill text (project.md, "Fast loops and runtime couplings"): "Quote a prop's
   size as LOD0 x and y extents measured from the GLB, not the bake report's radial `reachM`, when
   its plan is not round."
2. **A collider sized against `a` overhangs an oval plan.** `core_r` 0.32 sat at 0.86 a along x
   but past the leaves along y once `oval` < 1. Kept `core_r` below 0.8 x the smaller semi-axis
   (0.29 against a 0.38 x 0.37 m crown). No skill change: the family's walker rule (collider inside
   the leaves) already covers it.
3. **Replacing a family function from a variant family file.** `_shrub.build` has no outline knob,
   so `_verge_bush_b.py` swaps `_shrub.cards` for a wrapper that moves the card centres (lump, oval,
   lean) and asserts `build` still looks `cards` up by global name and the signature is unchanged.
   It goes one step past setting module globals; the asserts make a `_shrub` change fail the build,
   not the look. Card facings and vertex normals stay the family's plain ellipsoid, which read fine
   in the low-side still at lump 0.22. No time lost. Proposed skill text (SKILL.md, "Helpers"): "When
   the approved helper has no knob for the change, a variant family file may wrap one of its
   functions through the module (`_verge_bush_b.lumpy_cards` over `_shrub.cards`), asserting the
   helper still calls it by global name and its signature, rather than editing the helper."
4. **Waiting on a sibling's family file.** `_verge_bush.py` was not on main at the start. Iterated
   the look against an untracked copy taken from the sibling branch (SHA checked against its export
   report), deleted it before `git merge --ff-only main`, and ran every gated step only after the
   merge (same SHA, so the numbers held). No time lost; no skill change.
