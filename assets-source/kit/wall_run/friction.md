# B2 friction log (walls and hedges, 2026-10-27)

One log for both families; every wall and hedge piece shares these.

1. **Face strips at 270 px/m do not hold the junction.** `_drystone.py` maps each battered face in
   metres into strips of the 1024 atlas (three face rows, a 2.0 m and a 1.66 m column). The
   junction's six mitred faces are about 1.86 m each, 11.2 m with gutters against 11.0 m of strips.
   Fix: `set_texel(258)` for the junction only (median 258, gate 256). About 10 minutes.
   Skill: no change; the density budget arithmetic is in `references/project.md` (Textures).
2. **A chunk rounded a pixel past its face image.** Cutting a face into chunks with `round` and
   `ceil` read one column past either end (a broadcast error in `Atlas.put`). Fix: draw each face
   4 cm longer than it is, and edge-pad every copy by 4 px. About 10 minutes.
   Proposed skill text (pitfalls, Tileable sets or a new "Numpy-drawn unique maps" bullet): "A face
   longer than one atlas strip: draw it as one continuous image a few centimetres longer than the
   face and cut the chunks from it, so the stones run on across the cut; edge-pad every copy."
3. **`make kit-provenance` gates the whole manifest.** With 12 new entries in the manifest, the
   first piece's provenance failed on the 11 GLBs not yet compressed. Fix: build and compress all
   of them, then stamp provenance. About 5 minutes. Skill: `stages.md` step 5 already says to add
   the manifest entry first; proposed addition: "when several pieces land together, build and
   compress every one before the first `make kit-provenance`".
4. **A gateway centre polygon.** The top polygon that closes a junction's middle was built from
   every arm's start points, so on the gateway (two stubs) it spanned the opening as a slab at
   0.84 m. Fix: only arms that all start at the pivot get it. About 5 minutes (seen in the still).
5. **The farm's paddock fences its own edges 1 and 2.** A gate there opened into the 1.6 m strip
   between the paddock fence and the wall. Fix (runtime, `hex_core::boundary`): a gate edge needs
   a 3.5 m approach inside the farm clear of heroes (the paddock fence, the barn). About 15 minutes.
6. **Courses read as brick rows.** The first stone faces laid one stone per course height and
   similar lengths; at eye level in play they read as dressed brick. Fix: about one stone in four
   split into two thinner ones, wider length spread and steeper joint slopes. Wall rebuild about 40
   minutes of compress and provenance.
7. **Worktree hook.** Heredocs and compound edit lines were refused (existing SKILL.md rule);
   every scratch script went through the Write tool into `/tmp/b2-walls/`. No skill change.
