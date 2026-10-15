# Owner answers, 2026-10-14

Answers to `2026-10-14-owner-queue.md`, given in the walkthrough (`2026-10-14-walkthrough.html`). Codes match the queue.

## Round 1

- **Q2. Push:** push after the audio rework (forest one-shots) lands, as one push.
- **Q4. Tone mapper:** TonyMcMapface at EV 12.9, O4 soil lightened ×2.5, and an explicit backdrop colour. Kit and water shots are re-taken on the new look before their approvals (A1, A2, A4, A11).
- **Q7. Frame latency:** the web frame-latency limit (`frame_latency=3`) is on by default.

## Round 2

- **A3. Hero oak:** round 2 approved against ref-1. No third pass.
- **A7. Cars:** approved. An in-engine pickup close-up is still to be added to the evidence.
- **A9. House parking:** approved, including the house_pad re-export, the driveway road node, the lawn swing and the visitor space.
- **A19. Forest-forest edges:** approved. **Q3:** O-a ships now. O-b follows when forest moves into a streamed biome pack.

## Round 3

- **A11. Beach pieces and A22. Live wet band:** both approved. The static 0.6 m damp band is retinted to dry sand in the manifest, so the live wet band owns the gradient, as in the water look-dev. The beach colour is re-checked on the Tony re-shoot.
- **A12. Fish (Q5):** ref-3's body with both its ocean and river palettes, and ref-2 for school layout only. Ref-1 is rejected.
- **A12. Birds (Q6):** ref-1 and ref-3 approved, with ref-2 for flock layout only. Birds are starling-sized and replace the placeholder gulls.
- **A1. O4 skirts and A2. River family:** shape, seams and LODs approved now. Colour is re-checked on the Tony re-shoot.

## Round 4

- **A17. P3 references:** approved with every recommendation in the walkthrough. That means:
  - **Rail:** slightly wider than standard gauge, single track, bends across adjacent edges (60°), and a 60° level crossing with field gates.
  - **Station:** a side platform of about 20 m with the ref-1 shelter.
  - **Train:** a steam tank engine with runtime body colour and two carriages.
  - **Farm:** a pitched-roof red board barn and wheat.
  - **Animals:** hornless brown-and-white cows, black-faced sheep, and ref-2's smooth form.
  - **Windmill:** a flat tile, white, with animated sails and a still fantail.
  - **Bakery:** its own tile with a road through it.
  - **All P3 tiles:** the O4 skirt.
- **A5. Water tiles in play:** approved. Colour is re-checked on the Tony re-shoot.
- **A10. Streaming placeholders:** not accepted. Hide a tile, or show a loading shimmer, until its biome pack arrives, instead of the old coloured placeholder.
- **A4. Social share image:** re-take the same composition on the Tony look, then approve.

## Round 5

- **A13. Boids:** behaviour approved, with 25% fewer birds. The placeholder gulls and koi are replaced by the A12 models at starling size.
- **A15. Atmosphere hooks:** approved, including re-approval of the 7 pieces rebuilt with prop masks.
- **A18. Prop scatter:** approved, with one change: meadow flowers grow in clustered drifts from seeded noise. The forest seam strip stays.
- **D1–D5:** all kept.

## Round 6

- **A14. Photo mode:** the deviations are accepted. Two fixes are needed: remove the about 9-frame backdrop flash during export, and cap web export at 2× until 4× is tested for GPU memory.
- **A16. Saves:** the deviations are accepted. The load confirm, import preview and refusal panel go to the backlog. Volumes are not restored from a save. Loading a world exits photo mode. The off state reads "Autosave off" everywhere, the Worlds sheet included.
- **A21. Settings sheet:** the deviations are accepted. Only an explicit tilt-shift toggle counts as the player's choice. The autosave resume does not, so tier changes keep driving tilt-shift until the player touches it.
- **A8. Shipped UI:** sign-off comes after the owner tries the running build.

## Round 7: audio (A6, replaced by the forest one-shots)

- **Forest birds:** keep variants 1, 2, 3 and 5. Drop birds-4.
- **Forest leaves:** keep leaves-1 and leaves-2. Drop leaves-3.
- **Density:** keep the intervals: birds every 12–40 s, leaves every 20–60 s, bees every 45–120 s.

## Round 8: A8 in the running build (not signed off yet)

Owner's notes, verbatim: "there is no visual or ghost of the selected tile when hovering over empty slot so you have no idea what rotation you are using without looking at the icons in the tiles tab, birds in the trees look odd almost like fish, flocks of birds should be smaller, having the oak in the middle of the pine tree forest on every tile looks weird, i think we need to split pine and oak into their own tiles, when i mouse over an empty slot then try to move my mouse to the tile tab the available options changes as my mouse crosses other tiles on the way to the tab, the sounds are good, the tiles tab top area shows the hex and color coded sides for whats needed, the tile list uses a lines of colored squares it would be better if it showed the hex with colored sides that rotates with current tile rotation"

- **U1. Ghost:** hovering an empty slot shows no preview of the selected tile, so its rotation is unknowable. Today the ghost is a flat hex with lane strokes for road and river only.
- **U2. Birds:** birds in the trees read like fish (placeholder mesh, replaced by the A12 starling). Flocks should be smaller.
- **U3. Forest:** split pine and oak into their own tiles. An oak in the middle of every pine forest looks odd.
- **U4. Sheet filter:** moving from a hovered slot to the tile sheet crosses other slots and changes the available options.
- **U5. Sounds:** good in play (audio approved in play).
- **U6. Tile list:** show each tile as a hex with coloured sides that turns with the current rotation, like the sheet header, instead of rows of coloured squares.

## Round 9: follow-up choices for A8

- **U3. Oak tile:** the hero oak on meadow ground, with leaf litter, rocks and a few flowers. The pine tile keeps the 20 spruces and rocks with no oak. Both carry Forest edges and mix freely, joined by the forest-forest edge pieces. Birds roost on oaks wherever oak tiles are.
- **U4. Slot pinning:** clicking an empty slot pins it, so the ghost and the sheet's options stay on that slot. Clicking again places the tile. Esc or a click elsewhere unpins it.
- **U1. Ghost:** a see-through preview of the actual tile at the current rotation. The tile's own socket colours sit on each edge of the outline, showing fit or clash.
- **U2:** smaller flocks, on top of A13's 25% fewer birds. The A12 starling model replaces the fish-like placeholder.
- **U6:** each tile-list row shows a hex with coloured sides that turns with the current rotation.
- **A8 sign-off:** this waits for U1, U3, U4 and U6 in the running build.

## Round 10: river mouth and spring (owner request: rivers must reach the sea and start inside land)

- **References:** `river_mouth` and `river_spring` are approved as look references. The PLAN's socket table sets the layout.
- **Q1:** build both mouth handednesses now: the river on edge 3, plus a mirrored tile with the river on edge 4.
- **Q2, Q3, Q4 and Q6:** as recommended in `assets-source/refs/river_mouth/PLAN.md`.
- **Q5:** fish form one connected region, with the palette set by the water each fish spawns in.
- **River fish:** singles and small groups of 2 to 5, not big schools. This is handled on the pine/oak boids branch.
- **Road verge:** the dark strip beside roads is the 31° verge sink slope. Fix queued on card 01a13bad2862.
- **Ambience:** the bus defaults to 10%.

## Round 11

- **Bird and fish pieces:** both approved. Fix the faint fish head seam during the swap-in. Display scale is ×4 real size for the bird and ×2 for the fish.
- **Oak tile ground:** keep the forest_base clearing as shipped. The brown litter floor joins across pine neighbours.

## Round 12: play feedback

- **Colour grade:** "the new color grading feels a bit washed out, i liked the richer colors in previous one, not saying previous one is what to use, but something between the two". Grade presets between ACES and Tony are in progress, and the owner picks one.
- **Pine ghost:** shows no trees (the spruces are scatter). Fix in progress.
- **Oak canopy:** make the space between the leaves darker and not shiny. In progress.
- **Perched birds:** "nesting birds do not look good" (placeholder gulls lie flat on the canopy). Handled in the starling swap.
- **Bulldoze:** add a dust cloud during the destruction. In progress.
- **Mega tiles:** a 7-hex centre plus ring with a huge feature. Filed as a low-priority backlog idea.
- **Fish at river mouths:** let the fish skim the shallow fan by relaxing the fish band over mouth tiles. The fan is not deepened.
- **Colour grade default:** rich-3, which is ACES at EV100 12.5 with the shadow lift (owner, after playing ad423f2).
- **Spring tile:** "spring color does not match surounding tiles". Fix in progress.
- **Music bitrates:** keep Opus 128k and AAC 160k for now (32 MB across 12 files). Revisit once the full payload is known, and make cuts then if needed.

## Round 13: fish branch questions

- **Spring fish palette:** keep the agent's change. Fish spawned in a spring use the river palette and river grouping, like the river they swim down.
- **River fish visibility:** leave as shipped. Fish read clearly close up, are borderline at 50 m and cannot be identified from the 145 m opening camera. No further brightening or growth beyond 30%.
- **Grass:** blades on High and Ultra near the camera, with wind built from several noise layers (large travelling gusts, medium patches, small flutter). In progress.
- **River mouth fan:** "the river mouth coloring needs to be blended more where it meets the sand". In progress.

## Round 14: play review (owner, 2026-10-14)

- **Cattail budget:** raise the LOD0 limit to 2,500 triangles for the river dressing centres only (straight, bend, spring, both mouths, bridge), so about 30 detailed heads per tile come back. LOD1 and LOD2 unchanged.
- **R1 river mouth fan blend:** looks good (approved).
- **R3 bulldoze dust:** reduce size and duration by 20%.
- **R4 oak:** canopy looks good; the trunk looks odd (collar where the trunk meets the root flare); make the tree 20% bigger.
- **R5 birds and fish:** look good (approved, including the fish seam).
- **R6 road verge:** looks good (approved).
- **R7 HUD (A8):** looks good (signed off).
- **R9 car sounds:** sound good; single-variant start and drive pools accepted.
- **Walk mode:** double walk and sprint speed; sprint always double walk; add a walk speed setting with its minimum at the new doubled speed.

## Round 15 (owner, 2026-10-14)

- **River mouth grass line:** looks good (approved, a36663b).
- **Grass:** double the density at every setting, then make density a slider (0-150% of the new Normal); adopt small-world's thick-grass techniques (crossed blades, clump height noise, per-blade colour).
- **Cache busting:** hash all resources so they cache bust when updated.
- **Camera:** save the camera (orbit or walker) at regular intervals so a reload puts the viewer back.

## Round 16 (owner, 2026-10-14)

- **Hero oak:** "oak tree looks good" after playing 87e627b: the trunk without the root collar and the 1.2x size are approved (p5-trunk-scale; the canopy gaps, p4, were approved in round 14 R4).
- **O1 bark streaks:** leave them (the shared bark set's own fissure plates, also on tree_forest).
- **O2 root creases:** leave them (no welded root flare).
- **Wildflowers:** "the wild flowers have no grass around them". Grass now grows in among a flower clump up to a 0.2 m centre instead of keeping the clump's whole 0.9 m footprint bare.
- **Photo mode:** add click to set the focal point for the tilt-shift option. In progress.

## Round 17 (owner, 2026-10-14, after playing a1084fc)

- **Bulldoze dust:** "the dust particles / circles are still really big reduce the largest size by 20%". Each puff's full-grown radius is cut by a further 20%.
- **Cattails and reeds:** look good (approved, a36663b).
- **River spring and river mouth (R2):** look good (approved).
- **Grass slider:** works well.
- **Foliage sway:** looks good (approved, 9ddb35f).
- **Photo mode:** add a hotkey to take the photo, because the UI is sometimes hidden (C saves a PNG at the chosen resolution, also with H hiding the UI). Dragging the Focus distance slider releases a click focus (the band returns to the screen centre).
- **Photo export (A14):** no flash when clicking Save PNG; the exported image looks good (approved).
- **Social share image (R8):** re-take it, framed like the owner's play screenshot (house cluster with road, a river bend around the oak from a spring pool, pine forest behind, meadows and a beach in front).
