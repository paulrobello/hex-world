# bird_goldfinch friction log

Second boid bird. Builder `scripts/blender/kit/_fauna.py` (imported unedited, so `bird_starling` and
`fish_trout` stay current), sheets `scripts/blender/fauna_sheet.py` (generalised to any bird).

1. **The approved bird references show one species.** `assets-source/refs/birds` approves the
   slate-blue, rufous and cream songbird that `bird_starling` already wears; no sparrow, finch or
   swallow reference exists in `assets-source/refs/`. The brief named those refs, so the goldfinch
   takes its form, size class, flock layout and roost posture from them and its plumage from the
   species, and the pending approval asks the owner to approve the plumage from the sheet or ask for
   a goldfinch reference. Proposed skill text (`SKILL.md`, Rules, "Reference first"): "A new species
   or colourway of an existing family needs a reference that shows it; when the brief points at the
   family's refs instead, say in the pending approval which features no reference shows." Lost about
   5 minutes.
2. **`bird_starling.py` cannot be imported.** It builds at import and reads `sys.argv`, so the
   goldfinch copies its build, paint and hooks shape and imports only `_fauna`. No change needed:
   `project.md` (Variants) already says to copy the monolithic steps. Lost about 0 minutes.
3. **A UV-island overlap check by bounding box needs the loft's end cap split out.** `_fauna.loft`
   gives the cap face the body part id but maps it with `cap_uv`, so the body's UV box spanned the
   tail island and the generator's new overlap assert failed on the first build. The check now
   classes body faces with u >= 0.8 as the cap. No skill change (generator-local). Lost about 2
   minutes.
4. **`fauna_sheet.py` hard-coded `bird_starling`.** Now any root with `form: "bird"` gets the bird
   sheets, and a second bird also gets `vs_starling.jpg` (itself beside the committed starling at
   real size). Not a hashed generator dep, so no piece goes stale. Lost about 5 minutes.
5. **Flying flocks are hard to frame in engine.** Play birds fly at 22-44 m over the whole world, and
   the script has no way to aim at a flock: a camera near flock height shows them against the black
   void (invisible), and two wide strategic frames 4-11 s after placing caught none (new flocks
   arrive from 40 m past the sky's edge). What worked: a camera about 70 m above the oaks looking
   down at the meadow during the roost wave's launch (22 s into each oak's wave), a frame every 20
   fixed frames, picking the one with both flocks; and for the flap pair, a bulldoze and undo of an
   oak, then the settling flock from above the canopy, 6 fixed frames apart. Proposed skill text
   (`pitfalls.md`, Look-dev and engine shots): "Boid flocks: shoot from above flock height (22-44 m)
   against the ground, never level with them against the void; for a known flock in frame use an
   oak's roost wave (perched from 8 s, launching at 22 s) or a bulldoze-undo of an oak for flapping
   birds settling (`docs/checkpoints/bird_goldfinch/goldfinch.json`)." Lost about 15 minutes over
   three runs.
