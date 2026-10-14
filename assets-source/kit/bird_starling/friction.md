# bird_starling friction log

Shared with `fish_trout` (builder `scripts/blender/kit/_fauna.py`, sheets `scripts/blender/fauna_sheet.py`).

1. **Worktree hook.** A Python heredoc and a `sed -n "$(grep …)"` read were refused as unverifiable.
   Writing the script to the scratchpad with the Write tool and running it as one plain command passed.
   Already covered by the "Worktree sessions" rule in SKILL.md. Lost about 2 minutes.
2. **First perch fold read as streamers.** Compressing the wing's span toward the shoulder and sliding it
   back 0.075 m left the full 0.08 m chord flat at shoulder height, so the folded wings showed edge-on
   as thin blades past the tail (`vs_ref3_perch.jpg`, first pass). The fold now also rolls the
   compressed wing 60 degrees down onto the upper flank (coverts forward, flight feathers back, as in
   ref-3) and slides it back only 0.015 m. No skill change: the fold is this piece's runtime contract,
   documented in the generator and the root extras. Lost about 4 minutes.
3. **The six-view render is untextured,** so the vs-ref sheets need their own textured EEVEE stills
   with the stage PNGs bound (`pitfalls.md` already says so). The sheets come from
   `scripts/blender/fauna_sheet.py`, which also applies the flap and fold contract to LOD0 from
   TEXCOORD_1, so the owner sees exactly what the runtime formulas produce. Lost about 0 minutes.
4. **Black atlas background at deep mips.** The first build filled only a 6 px gutter around each
   island and left the rest of the albedo black; box-filtered to 16², the island luma fell from 0.471
   to 0.429 (fish: 0.579 to 0.516). Boids sample those mips. `_fauna.fill_index` now fills every texel
   from its nearest island (0.474 and 0.586 at 16²). Folded into `pitfalls.md` ("Small or distant
   pieces"). Caught in review before the owner checkpoint; cost one rebuild, about 8 minutes.
