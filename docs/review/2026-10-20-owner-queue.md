# Owner review queue, 2026-10-20

Requests waiting on you. Nothing here is approved. Paths are relative to the repo root.

## Reference approvals

**A1. Rail buffer stop (`rail_buffer_stop`), references.** Branch `worktree-agent-a83f8dafe096bd6e1`, built on the rail network branch. No approved reference shows a buffer stop. The rail, station and train refs show running track and the engine's own buffers, so modeling waits on these (R2).
- Sheet: `assets-source/refs/rail_buffer_stop/sheet.jpg`. The single images are `ref-1.jpg` to `ref-3.jpg` in the same folder.
- ref-1: a straight rail tile whose track ends at a stop near the top edge, as the game would draw it at an open line end. Its stop has a red beam, two buffers and a round red lamp on a frame of bent rails.
- ref-2: a studio close-up of the bent-rail look. The rails bend up into legs and are braced back down behind the beam. It has a red beam, two sprung buffers and a lantern on top.
- ref-3: a sleeper-built stop. A wall of stacked dark timber sleepers stands between two posts with a red beam, two sprung buffers and a red-and-white target board. One raking strut and an earth bank sit behind it. Along the track it is the shallowest of the three.

Planned piece: kind `prop` (LOD0 at most 1,500 tris), KTX2 class `piece`. The runtime draws it across the track at every rail edge that faces an empty cell. Its buffers line up with the engine's: rail top + 1.05 m, and 0.85 m either side of the track centre (`train_engine.py`). The earth bank in ref-2 and ref-3 would cross the tile edge, so it is left out.

Questions:
- **Q1. Which look, ref-2 (bent-rail frame) or ref-3 (sleeper-built)?** Recommendation: ref-3. Its solid red-on-dark-timber face reads at play distance, where ref-2's thin rail legs thin out. It is also about half as deep along the track (Q2), so the train stops nearer the edge. It also suits curved line ends. The 60° bend and the junction's branch leave their edges on an 8 m arc, which drifts off the edge's straight line by 0.06 m at 1 m in, 0.16 m at 1.6 m and 0.56 m at 3 m. ref-3's wall stands within about 0.5 m of the edge with its buffers in the air, so one piece fits straight and curved ends. ref-2's legs stand on the running rails 2-3 m in, so bend ends would need a curved variant. At a junction's toe edge either stop stands over the switch blade tips (0.25 m in).
- **Q2. Where trains stop.** A train now stops with its front buffers 1 m inside the tile edge (`hex-sim` `rail::END_MARGIN`). A stop cannot cross the edge or hang over the skirt, and both looks are braced behind their beams, so neither fits in 1 m. The ref-3 stop needs about 1.5 m, with trains stopping about 1.6 m in. The ref-2 stop needs about 3 m, with trains stopping about 3.5 m in. It is one constant, and trains also spawn that far in. Recommendation: raise it to suit the look you pick in Q1, so the engine stands buffered up to the stop.
- **Q3. Ghost.** Draw the stop in the placement ghost at the edges that would end the line? Recommendation: yes, so the dead end shows before you place it.
- **Q4. Thumbnails.** Every tile thumbnail is drawn alone, so every rail edge in it is open. Drawing stops there would add two or three stops to every rail thumbnail. Recommendation: no stops in thumbnails, so the rail thumbnails stay as they are.
- **Q5. Sound.** A soft buffer clunk when a train reaches the stop and reverses? Recommendation: yes, as a short one-shot, generated under the audio standing approval for you to approve by ear.
Items waiting on your look. Nothing here is approved. Each entry is self-contained under its own heading, and its codes are prefixed with the entry's tag so entries from parallel agents don't collide.

## Beach dressing: reference approvals for `driftwood` and `dune_grass` (tag BD)

The beach and river-mouth tiles get two new scattered props on the dry sand between the grass line and the damp band: a bleached driftwood log and a marram dune-grass clump that sways in the wind. Today the five beach and river-mouth centres carry only a baked, untextured dressing (`_beach_dress.py`, `_mouth_dress.py`): 3 grey cylinder logs, 12-14 thin tufts of 10 flat blades and about 20 pebbles a tile. That dressing reads as bare sand at play distance, see `docs/checkpoints/hooks/masks-none/masks-coastjson-03-headland-after.jpg` and `docs/checkpoints/river-mouth/river-mouth-12-fan-sand-oblique.jpg`.

No approved reference supports modelling them, so nothing has been modelled yet. The approved beach refs show no driftwood and no marram. river_mouth ref-3 shows both, but its approval covers the fan and foam layout only, not materials.

Sheet: `docs/checkpoints/beach-dressing/refs-sheet.jpg`. The new refs are in the top two rows, with the approved river_mouth and beach refs below for comparison.

### Approvals

**BD-A1. `driftwood` references.**
- `assets-source/refs/driftwood/ref-1.jpg` shows one barkless log with a twisted spiral grain, silver-grey with beige streaks, cracks along the grain, a broken branch stub and a small root flare.
- `assets-source/refs/driftwood/ref-2.jpg` shows two logs and a forked branch half sunk in dry sand above the damp band.
- Approval would cover the wood's shape and colour. ref-2's coarse golden sand is not the tile's look: the tile keeps its approved pale sand.

**BD-A2. `dune_grass` references.**
- `assets-source/refs/dune_grass/ref-1.jpg` shows one marram clump on a small sand mound: narrow blue-green blades fading to straw tips, a few dead blades and two seed spikes.
- `assets-source/refs/dune_grass/ref-2.jpg` shows clumps on low sand hummocks between the meadow grass and the beach.
- Approval would cover the plant. A first ref-2 printed a "BEACH TILE - REAR VIEW" label on a grey block, so it was re-rolled with new wording.

### Questions

- **BD-Q1. Replace or keep the baked dressing?** The new scatter could replace the centres' baked logs and tufts, or stand around them. All five centres already await re-approval, so rebuilding them costs little. My recommendation is to remove the baked logs and tufts, keep the pebbles, and let the scatter carry the dressing. This answer blocks the scatter rules, not the modelling.
- **BD-Q2. Where does the dune grass grow?** The play grass already draws taller, sparser blades (0.26-0.5 m) on the beach's grass strip. The proposal keeps the clumps off that strip and off the grass line: they stand on the dry sand only, from a clump's width seaward of the grass line to about 2 m short of the damp band, densest near the grass and thinning toward the sea. Is that the band you want?
- **BD-Q3. Sand mounds under the clumps?** The beach surface follows a fixed coast profile, so ref-2's hummocks cannot be in the beach tile itself. Each clump could carry its own low sand mound in the sand's colour, as in ref-1. Do you want the mounds?
- **BD-Q4. How dense?** The proposal is about 3 driftwood pieces and 10-14 grass clumps on a straight beach tile, on the dry sand only. This is clear of the grass line, the damp band, the surf and the river-mouth channel, and keeps 1.5 m inside the tile border.
