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
