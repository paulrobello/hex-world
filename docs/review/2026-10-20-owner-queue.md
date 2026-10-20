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
## Boats and lighthouse references (reference gate)

Higgsfield references for the P3 boats (PRD §7.2: "Drifts on ocean tiles and travels connected rivers") and the P6 lighthouse. Nothing here is approved and nothing is modelled. Modelling starts only after you approve the refs, and the controller writes each `assets-source/refs/<id>/approval.json`.

- Sheet (all eight images): `docs/checkpoints/refs-boats-lighthouse/refs-sheet.jpg`.
- Sources: `assets-source/refs/boat_rowing/`, `assets-source/refs/boat_sail/`, `assets-source/refs/lighthouse/` (each with `refs.json`, its prompts and hashes). Prompts are the new arms in `scripts/refs/higgsfield-refs.sh`.
- Land walls use the O4 cutaway skirt, water walls use the approved water-column cut, as on every P3 tile.

### Approvals

**BA1. Rowing boat (`boat_rowing`), refs 1-3.**
- ref-1: studio view. A white hull with one dark blue band, a varnished interior, two thwarts, slatted floorboards and two oars in bronze rowlocks.
- ref-2: in place on a straight river tile, tied to a small timber landing stage.
- ref-3: pulled up on the sand of a coast tile, roped to a stake.

Questions:
- **BQ1.** Size: about 3.5 m long and 1.4 m wide, so it sits in under half the 8 m river. Is that right, or should it be smaller?
- **BQ2.** Colour: keep white with a blue band, or recolour per instance like cars and trains? Recolouring needs a plain neutral base, as the vehicle refs use.
- **BQ3.** Where does it move? The options are fixed moorings only (landing stage, beach), rowing along connected rivers (animated oars), or both. Depth is no problem: river centre depth is 1.55 m (`docs/kit/water-contract.md`). Bridges are: the road and rail river bridges share one stone arch whose crown is only 0.95 m over the water and which springs at the waterline (`scripts/blender/kit/_river_bridge.py`, `_rail_bridge.py`). A rowing boat with a seated rower does not pass under it. River boats would need to stop at bridges (turn back or stay between them), or the bridges a taller arch.
- **BQ4.** The ref-1 hull is smooth planked rather than the overlapping clinker the prompt asked for. Is smooth fine? It is cheaper to model.
- **BQ5.** Should the ref-2 landing stage become its own kit prop (river bank), or only appear as part of a mooring tile?

**BA2. Sailing fishing boat (`boat_sail`), refs 1-2.**
- ref-1: studio view. A deep blue hull with a white band, red-ochre tan sails, rope coils and two wicker fish baskets.
- ref-2: under sail on a deep ocean tile with a short wake (a re-roll; see note).

Questions:
- **BQ6.** Size: about 5 m long with a 6 m mast. Is that right against a 32 m tile?
- **BQ7.** Should it be ocean only? Both river bridges clear only 0.95 m at the arch crown (BQ3), so a 6 m mast can never pass. Ocean only, plus the river mouth, is the simple rule.
- **BQ8.** Where does it move? The options are wandering free across connected ocean tiles, sailing set routes (between moorings, or around the lighthouse), or anchored off a beach. And how many boats: one per N ocean tiles, sharing the vehicle cap?
- **BQ9.** Rig and colour: ref-1 carries a mainsail and two headsails, ref-2 a main and one jib. Simplify to one gaff main and one jib? Keep tan sails and a blue hull, or use white sails?

**BA3. Lighthouse coast tile (`lighthouse`), refs 1-3.**
- ref-1: ocean on two adjacent edges, a granite headland, a white tower with one red band, a black gallery and glazed lamp room, and a slate-roofed keeper's cottage inside a low white wall.
- ref-2: lamp-room close-up, showing the glazing bars, faceted lens, gallery on stone brackets and domed cap.
- ref-3: dusk mood with the lamp lit, one warm beam over the sea and glowing cottage windows.

Questions:
- **BQ10.** Bands: one wide red band (ref-1), or several narrow bands (ref-2, ref-3)?
- **BQ11.** ref-3 draws a different tower (a brick-red lower half and a red cap). Approve it for the beam and dusk lighting only, with ref-1 as the tower's look?
- **BQ12.** Keep the keeper's cottage and wall? Height: about 20 m tower against the 18 m windmill. Is that right?
- **BQ13.** Shore: ref-1 has a rocky shore in place of sand. Should its two ocean edges use the existing beach socket kind, so it joins ocean and beach tiles? Or is it a new rocky-coast edge?
- **BQ14.** Beam timing: one rotating lamp and beam, about 10 s per turn, or faster? Warm white or cold white? The beam needs the P5 day/night cycle to read. Until then, should it show only under `--kit_hooks night`, or as a faint beam in daylight too?

### Notes

- **Rejected re-roll.** `boat_sail` ref-2's first image came back as an eight-sided tile with an oversized boat. It was re-rolled with a high 65° camera, "exactly six straight flat edges and six corners" and "the hull only about one sixth of the tile's width".
- **Text check.** Every kept image was cropped at full resolution where text would land: each boat's hull, transom and sails in all five boat images, the lighthouse and cottage doors in ref-1 and ref-3, and the gallery in ref-2. None shows printed text, labels or numbers. The small mark on the bow of `boat_rowing` ref-2 is a metal fitting. Every tile outline is six-sided.

