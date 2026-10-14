# Owner review queue, morning of 2026-10-14

Everything below waits on you. Work kept going overnight on everything that did not need you. Paths are relative to the repo root. The review server (`.claude/launch.json`, `hex-world-review`, http://127.0.0.1:8095/) serves the repo, so an image path opens at `http://127.0.0.1:8095/<path>`.

## Start here

These five answers unblock the most work.

1. **Q2. Push?** Main holds about 109 unpushed local commits, and `make checkall-ci` is green. CI now runs the UI checks.
2. **Q4. Tone mapper?** ACES crushes shaded walls to pure black (F1). TonyMcMapface fixes it but reopens your ACES choice. Also **Q7**: turn on the web frame-latency limit by default, which ends the stall tail (F2).
3. **Kit approvals:** O4 skirts (A1), river family (A2), oak (A3), cars (A7), parking pad (A9), beach pieces (A11) and forest edges (A19). Every MVP tile now draws built pieces; approval is what remains.
4. **Reference approvals:** fish and birds (A12) and the seven P3 families (A17). Modeling starts once these are approved.
5. **UI sign-off in the running build:** the HUD (A8), photo mode (A14) and save slots (A16). Run `make build-web`, then `python3 -m http.server 8094 --bind 127.0.0.1 --directory dist`, and open http://127.0.0.1:8094/.

## Approvals

**A1. O4 cutaway skirts, as shipped.** `docs/checkpoints/skirt-o4/sheet.jpg` (8 in-play shots). The soil set is now your O4 pick. Meadow, forest, house and river skirts sample it, and the beach and river cut walls copy it in the shader. Please re-approve `meadow_base`, `forest_base` and the river skirts on this look. Known issue: walls facing away from the sun render black. F1 explains why; it is the tone mapper, not the skirt.

**A2. River family (6 pieces: edge, straight, bend, bridge).** Merged as `4509640`. In-play shots, re-taken on main with the final bend mesh and the O4 skirts: `assets-source/kit/river_edge/shots/engine-01..09.jpg` (overview, the straight-to-bend and bend-to-bridge seams, the river mouth, the bridge arch, the bend, LOD1, LOD2). The bend's LOD2 uses a 20% triangle ceiling instead of 15% (decision D1).

**A3. Hero oak polish (round 2 done overnight).** Sheet: `assets-source/kit/tree_hero_oak/renders/p3_sheet.jpg`, with round 1 on the left and round 2 on the right. Rim crops are in `p3_rims-crop.jpg`.

**Bark AO:**
- Visible bark AO is now 0.758, meeting the 0.75 target.
- Measured over all texels it is 0.678. That is a shortfall I am explaining rather than hiding: 27% of the bark surface is buried inside other tubes and cannot be removed within the 3,000-triangle budget.

**Dome rims:**
- They now use rings at equal steps of surface angle. The top dome and most rims read round.
- One small corner remains on a lower dome in the sharp view.
- The dome undersides look faceted from below (12 columns), which paid for the rim rings.

**Your call:** accept, ask for a third pass, or approve the oak against ref-1. It still needs your re-approval.

**A4. Social share image.** `web/brand/og-image.png`, 1200x630, taken from the in-engine water overview. The B1 Lit icon is already approved, so this is the last open item on the icons card.

**A5. Water tiles in play.** `docs/checkpoints/water-play/*.jpg` (ocean, beach, river, seams across rotations, bulldoze and undo). This is the last criterion on the water-tiles card.

**A6. Audio mix (SFX and ambience).** You can only judge this by listening.
1. Build and serve it:
   - `make build-web`
   - `python3 -m http.server 8094 --bind 127.0.0.1 --directory dist`
2. Open http://127.0.0.1:8094/. The bare URL now opens play.
3. Click once. Nothing loads or plays before the first interaction.
4. Place tiles, rotate, undo and bulldoze, then zoom onto a river, coast or forest to bring its ambience up.

Mute and volume live in the console until the Settings screen exists: `hexWorld.audio.setMuted(true)` and `hexWorld.audio.setVolume(0.5)`.

Not built yet:
- town murmur, the per-tile placement stingers and all music. You generate the music in Suno.
- an audio key. Neither PRD §4.5 nor the Legend design defines one.

**A7. Cars: sedan, van and pickup with runtime paint.** In play: `docs/checkpoints/cars-kit/cars-kit-01-busy.jpg`, `-02-hub`, `-03-closeup-sedan`, `-03-closeup-van`, and night `-04`/`-05`. Blender six-views are in `assets-source/kit/car_*/renders/`. There are 12 palette colours, and the draw count does not grow with colour. There is no in-engine close-up of the pickup yet.

**A8. Shipped Legend UI against the mockups.** `docs/checkpoints/ui-followups/` has 23 `hud-<state>-mockup-vs-shipped.jpg` side-by-sides, the S11 unsupported page (`unsupported-s11-*-mockup-vs-shipped.jpg`, `unsupported-390-both-variants.jpg`) and the bare URL opening play (`default-scenario-bare-url.jpg`). The drift list from your Q7 answer is now tokens.

Two choices were made for you:
- The DESIGN.md sidecar sits at `.impeccable/design.json`, not a bare root `design.json`, because impeccable treats a root `design.json` as a legacy location.
- Native builds still default to the `single` scenario, the capture harness. Only the web defaults to play.

This item is the PRD sign-off "the owner signs off the shipped UI in the running build". Open the bare URL to try it.

**A9. House parking.** `assets-source/kit/house_pad/renders/parking_top.jpg`, `parking_three_quarter.jpg`, and `parking_paths.jpg` (blue is arriving, red is reversing out, yellow is driving onto the lane). house_pad was re-exported with three `anchor-park-<n>` sockets and needs your re-approval.

Choices to confirm:
- The cluster's road node moved from (2, 0) to (12.5, 0) on the driveway to make room for the three-point turn.
- The turn is tight (about a 2 m radius), and park-0 swings about 2.9 m onto the lawn.
- With the tile's car cap at 2 and 3 houses, the two-storey has no car and its spot is the visitor space.

In play: `docs/checkpoints/parking/parking-01..09.jpg`. Shots 01–03 show the parked cars, 04–06 a van reversing out and swinging round, 07–08 the same van nosing into a free spot at the other cluster, and 09 the settled yard. Cars keep their body and colour across trips.

**A10. Streaming placeholders.** `docs/checkpoints/streaming/streaming-{0-start,1-placeholder,2-kit-mid-rise,3-settled}.jpg`. Until a tile's biome pack arrives it draws the old coloured placeholder, then swaps to the kit look mid-rise. The initial download fell from 32.8 to about 19.9 MiB. Confirm the placeholder look is acceptable for the first few seconds on a slow connection.

**A11. Beach pieces (straight, Headland, Bay; 6 pieces).** Close-ups:
- `assets-source/kit/beach_base/shots/beach-01-seam-straight-headland.jpg`
- `beach-02-bay.jpg`
- `beach-03-wet-band.jpg`
- `beach-04-headland.jpg`
- `beach-05-island-mid.jpg`

The full island is `docs/checkpoints/coast/coast-01..08.jpg`.

**How the colours work:**
- Dry sand is the shared plaster set, tinted.
- The damp band is the same set tinted darker, from the waterline up 0.6 m.
- The seabed is tinted gravel, as on the ocean pieces.
- None of this adds a texture.

**Notes:**
- **Damp band edge:** its upper edge is a hard line. Say if you want it feathered.
- **Village pack:** plaster lives in the village pack, so placing a beach also streams the village pack. Tinted gravel would avoid that, but it read as pebbles.

**A12. Fish and bird references (reference gate; nothing is modeled yet).** Sheets: `assets-source/refs/fish/sheet.jpg` and `assets-source/refs/birds/sheet.jpg`.
- **Fish:** ref-1 is a single ocean fish in profile, ref-2 a school seen from above over sand, and ref-3 the same body in an ocean palette and a river palette.
- **Birds:** ref-1 is a gliding songbird in profile, ref-2 five seen from above with their shadows, and ref-3 three roosting on an oak branch.

Approve or reject per ref. Two choices go with it:
- **Q5. Fish body.** Ref-1 leans cartoon, with a big eye. Ref-3 is a stylized-real trout shape, closer to the house style. I recommend ref-3's body, with both of its palettes.
- **Q6. Bird size.** Starling-sized was the agent's choice, since the PRD gives none. Also, in birds ref-2 the centre bird is upside down and the feather tips would have to live in the texture at 100–200 triangles. Use ref-2 for flock layout only.

**A13. Birds and fish in play (GPU boids).** `docs/checkpoints/boids/boids-01-flock.jpg`, `-02-roost`, `-03-fish-ocean`, `-04-fish-river` (`make shots-boids`).
- **Budget:** 1,000 boids shared. Fish get 12 per water tile, up to 300, and birds get the rest. With 16 water tiles that is 808 birds and 192 fish.
- **Birds:** they roost on forest oaks and launch in waves every 36 s.
- **Fish:** they stay inside the water band between the bed and the surface, and show through the refraction. The texture budget holds at 16.
- **Meshes:** both are procedural placeholders (gulls and koi) until A12's references are approved and modeled. The gull's 2.9 m wingspan is larger than the starling size proposed in A12.

**A14. Photo mode and tilt-shift controls (PRD §4.6, §9.1).**
- **Shots:** `docs/checkpoints/photo/` has these, all from `make shots-photo`:
  - `photo-01-enter-tilt`
  - `photo-02-dof-closeup` beside `-dof-off-closeup`
  - `photo-03-tilt-roll-exposure`
  - `photo-04-export-4x`, the 5120×3200 export
  - `photo-05-exit-orbit`
- **To try it:** press P in the running build, move with WASD/QE and the mouse, and exit with P or Esc.
- **Built from the S8 and S9.4 mockups.** Deviations for you to accept or reject:
  - Settings has only the Tilt-shift section, with no tabs.
  - Export progress is indeterminate, because the engine renders in one pass.
  - Cancel discards the result but does not stop the render.
  - The saved toast has no "Show" action.
  - Scales too large for the GPU are struck through.
- **Look choice:** the manual-DOF lens uses a "miniature" sensor, 60× Bevy's default, so the diorama blurs like a model.
- **Known issues:**
  - During an export the window shows no camera for about 9 frames, which can flash the backdrop.
  - 4× export on the web is untested for GPU memory; only 2× was tested there.

**A15. Future-atmosphere hooks (§9.4), for a look only.** `docs/checkpoints/hooks/` (17 shots, `shots.sh`) shows each hook doing something:
- the kit under snow, wet, night, dusk and autumn
- the tree close-up under snow, night, dusk and autumn
- the coast under freeze, dusk and night

At the MVP values (noon, dry, summer, unfrozen) the game looks the same as before. The pixel diffs sit inside run-to-run noise, and the texture budget holds at 16. The asset gate now fails any new piece or shader that lacks a hook. The 21 small prop slots on the river, beach and ocean centre pieces now have authored masks maps (`docs/checkpoints/hooks/masks-*/`). That rebuilt 7 pieces with no change at MVP values, and they need your re-approval. One open point: a coast view shows 6 beach-dressing outline pixels off by 3–5/255, cause not found.

**A16. Save slots, autosave, and .hexworld import/export (PRD D9).**
- **Captures:** `docs/checkpoints/persistence/`.
- **Autosave:** it runs 600 ms after each edit and also flushes when the page hides. The bar reads "Riverside · 14 tiles · Saved / Saving… / Autosave off".
- **Worlds sheet:** it has new, rename, duplicate, delete, export and import (picker or drag-drop). A refused file gets a plain toast.
- **Loading:** loaded worlds appear settled, with no rise.

Differences from the mockups for you to accept or reject:
- Rename is inline, which has no mockup.
- Thumbnails are socket-colour maps, not photos.
- The load confirm (S10.3), import preview (S10.6) and refusal checks panel (S10.8) are not built.
- Duplicate was added.

Two choices to confirm:
- Volumes are not restored from a save, so a shared world can't override your audio levels.
- Loading a world keeps photo mode on.

Q1 is now settled: the bar says "Autosave off" and "Saved", but the Worlds sheet still uses the mockup's "Not saved" for the off state.

**A17. P3 references (reference gate only; nothing is modeled).** Sheets are at `assets-source/refs/<id>/sheet.jpg` for rail, rail_station, train, farm, farm_animals, windmill and bakery: 17 refs for 38 credits. Questions to answer with the approvals:
- **Rail:** standard gauge or wider so it reads at play distance? Single track? Bends across one or two edges? Is a 60° rail-road level crossing with four field gates OK (ref-3 drew it near 90°, which a hex cannot do)?
- **Station:** a side platform about 20 m long, or an island platform? Is the ref-1 shelter the house style?
- **Train:** steam tank engine, as drawn, or diesel? Which parts take the runtime colour? Always two carriages?
- **Farm:** pitched or gambrel barn roof? Red board or the cottage brick and timber? Which crop? Which edge sockets?
- **Animals:** which cow breed, horned or not? Black-faced sheep? Is ref-2's smooth form the low-poly target?
- **Windmill:** mound (ref-3) or flat tile (ref-1)? Tarred black or white? Should the fantail animate as well as the sails?
- **Bakery:** road through the tile or a stub? Its own tile, or a pad inside a house cluster?
- **All P3 tiles:** use the O4 cutaway skirt (recommended).

**A18. Prop scatter in play.** `docs/checkpoints/scatter/scatter-01..07.jpg`: overview, forest cluster, meadow, a forest beside a road and a river, a forest close-up, and the rise ripple.

**What changed:**
- A forest tile now draws 20 spruces and rocks around its oak.
- A meadow draws about 27 wildflower clumps and 2 rocks.
- Placement is seeded, so a world always looks the same, including after undo or a reload.
- Nothing lands on roads, river beds and banks, house pads, beaches or water.

**Cost:** a 20-tile world is 392 instances and 256–298 draw batches, inside the 500 budget, and the texture limit of 16 holds.

**Look calls for you:**
- **Seam strip:** where two forests meet, a strip without trees runs along the seam. Filling it would make a tile's trees depend on its neighbours.
- **Flowers:** meadow flowers sit evenly spaced. Clustered drifts would need a rule.
- **Gulls:** the placeholder gulls read large and white above the trees. See A12/A13.

**A19. Forest-forest edge pieces.** These are six per-edge variants that carry the litter across the seam between two forest tiles.
- **In engine:** `assets-source/kit/forest_forest_edge/shots/engine-01..07.jpg`. They cover two and three forests, the junction where three meet, the meadow corner, and bulldoze and undo.
- **Six-view renders:** `assets-source/kit/forest_forest_edge_e*/renders/`.

The corner look is Q3. With scatter on, a treeless strip still runs along the seam (A18).

**A20. Quality tiers and performance (for information; nothing to approve).**
- **Tier detection:** a tier is picked at startup, and on this machine it is High both natively and on the web. Boids are capped at 1,000, 500 and 250 for High, Medium and Low. An explicit `?tier=` always wins.
- **Measured** under the load guard (load 6.5–11.2 before and after every run, none discarded), on the 2,000-tile world at 2560×1440, High:
  - **Web:** p95 frame time is 3.4–3.8 ms against the 16.6 ms budget. Tilt-shift adds 0.7 ms.
  - **Native:** p95 is 10.8–11.9 ms with tilt-shift off. With it on, the worst run reached 16.5 ms, just 0.1 ms inside the budget, because native frames are bound by presentation or the CPU, not the GPU (about 4.4 ms of GPU time per frame).
  - **Per-pass GPU times:** `docs/perf/p2-passes.json`, after merge.
- **Hero water audit:** waves and flow, reflection and refraction, seams, and the rise fill all have code and shot evidence. Foam and the wet band are partial: the kit beach shows a static damp tint, not the live wet band.

## Questions

**Q1. Save label (resolved).** Autosave now exists, so the bar shows "Saved" as the brief asked, plus "Saving…" and "Autosave off". See A16.

**Q2. Push.** Main holds about 109 unpushed local commits, and nothing has been pushed. `make checkall-ci` is green on main, and CI now sets up Node and runs `make ui-check` (`93542a6`). Approve one push of main to origin?

**Q3. Forest edge corners.** Where a forest-forest edge sits next to a forest-meadow edge on the same tile, the litter has to stop at the corner. There are two options, shown in `assets-source/kit/forest_forest_edge/stills/corner-options/`: `before.jpg`, `O-a-after.jpg`, `O-b-after.jpg`, `O-b-overview.jpg`.
- **O-a (shipping now):** the litter ends in a straight slope along the line from the corner.
- **O-b:** the litter pulls back into forest_base's own wavy grass rim. It reads better, but it needs 18 more pieces (about 0.9 MB) and curve-exact cuts.

I recommend O-a now and O-b once forest moves into a streamed biome pack. The resolver passes the corner flags either way, so switching later needs no engine change.

## Findings

**F1. Walls facing away from the sun render pure black, and the ACES tone mapper causes it.** Diagnosis is in `docs/checkpoints/shade-diagnosis/`: `measurements.txt`, `frames/`, and `crop-map-*.jpg` showing where each crop sits.

Examples:
- the river mouth's cut wall (`assets-source/kit/river_edge/shots/engine-04-mouth-straight.jpg`)
- the bridge arch's underside (`engine-05`)
- the land walls beside the beach

What the diagnosis found:
- **The IBL works.** It is attached at intensity 1500 and loaded before the first shot. Native and web measure the same.
- **ACES clips them to black.** Bevy's fitted ACES curve outputs pure black for any input below about 0.0033 linear. The shaded O4 soil walls reach about 0.0030, because the "puresky" HDRI has no lit ground below the horizon to bounce light up, and O4 soil is very dark (linear albedo about 0.04).
- **SSAO is not the cause.**
- **Your approved water look-dev has the same black wall** (0.6/255).

No change limited to the walls gets close while ACES stays. They would need 10 to 15 times more light, and doubling the IBL, ambient 400, or exposure EV 11.5 reach only 1/255.

**Q4. Reopen the tone mapper?**
- **Recommended:** TonyMcMapface at EV 12.9, plus a lighter O4 soil and an explicit backdrop colour.
  - Walls go from 0 to about 9–16/255 and the bridge face from 8 to 31/255.
  - Grass and gravel move under 3%, and river water about 14% brighter.
  - The empty backdrop rises from 15 to 47 unless its colour is set.
  - Compare the `frames/l-tony-ev12.9-*` and `k-tony+soil-x2.5-*` frames against the `frames/a-current-*` frames.
- **The cost:** it reopens your 2026-10-13 choice of ACES (`docs/decisions/0002-asset-pipeline.md`) and needs re-approval of the kit and water look-dev shots.
- **Alternative:** a re-baked noon IBL with lit ground. Untested, and unlikely to be enough under ACES alone.
- Or keep ACES and accept black shaded walls.

**F2. The web stall tail is explained.** It is Chrome's WebGPU backlog when frames are uncapped. The page makes a frame in about 2.4 ms while Chrome's GPU process needs about 2.5 ms per frame, so around 109 submits pile up. About every 50 frames the compositor's buffer swap blocks for 48–67 ms, and the next frame waits behind it. At a normal 60 Hz display rate there are none.
- **Evidence:** `docs/perf/stall-tail/README.md`. The trace rules out garbage collection, wasm memory growth, the HUD, state polling and audio.
- **Fix:** an opt-in limit, `?frame_latency=3`, removes the uncapped tail (0 frames over 25 ms per 600), at a cost of about 0.4 ms mean.
- **Re-run on the merged build, under the load guard:** native tilt-on p95 is 12.1–13.3 ms, and the web is 3.6–4.2 ms. A 208-tile water and scatter world runs web p95 8.6–9.3 ms with no stalls.
- **Native:** a small new tail appeared, 2–5 frames over 25 ms in 5 of 12 runs. It is not yet investigated.

**Q7. Accept the uncapped-only web tail, or turn the frame-latency limit on by default?** I recommend turning it on by default: it is harmless at 60 Hz and removes the tail on fast GPUs with high-refresh or uncapped displays.

## Decisions I made overnight (override any)

**D1. Lane-cut LOD2 ceiling.** River bases may keep 20% of LOD0's triangles at LOD2. At 15% the bend's channel profile drifts past the water contract's 0.03 m tolerance. Every other piece stays at 15%.

**D3. The forest edge piece comes in six per-edge variants.** forest_base is not the same under each of its six edges. A single rotated edge piece therefore had to float up to 0.18 m above the litter to clear every edge (`assets-source/kit/forest_forest_edge/stills/mock-seam-after.jpg`). Six variants, one per local edge of the base, fit exactly. The edge-piece resolver picks the family by socket pair and the variant by the base's local edge.

**D2. Forest edge pieces are chosen by socket pair, base edge and corner flags.** `sockets.ron` `pair_edge_pieces`, merged as `8337a52`. The corner look is Q3.

**D5. The coast corner bases get a 20% LOD2 ceiling, like the river bases (D1).** The Headland and Bay sit at about 18%. 15% was reachable only by padding LOD0 to about 3,950 triangles, which spends triangles where nobody sees them.

## Overnight summary

Twenty-two feature merges landed on local main overnight. Each was gated on its own branch, and main was re-gated after each batch with four checks:
- `make checkall-ci`, with the initial download at 19.7 MiB against the 30 MiB budget
- `make kit-check`
- `make ui-e2e` (HUD, persistence, audio)
- `make web-smoke`, with every 16-texture case green

Nothing is pushed.

**Kit**
- River family; beach family (straight, Headland, Bay); forest-forest edges. Every MVP tile now draws built pieces, and the pending list is empty.
- The O4 skirt.
- Cars (sedan, van, pickup) with runtime paint, and house parking.
- Oak polish, round 2.
- Masks for the 21 prop slots that lacked them.

**Engine**
- Directional beach sockets and biome-pack streaming.
- Boids: birds roost, fish school.
- Prop scatter.
- The §9.4 atmosphere hooks.
- Quality tiers, GPU pass timing, and the 2,000-tile measurement under the load guard.

**UI and web**
- The Legend HUD overlay, plus follow-ups: the unsupported page, the play default, CI UI checks, and the rotation fixes.
- The audio layer.
- Photo mode.
- Save slots with autosave and `.hexworld` import and export.

**References:** fish and birds, and seven P3 families.

**Fixes found along the way:**
- A flaky compat-shim smoke check.
- A ui-e2e race under high load.
- e2e writing into tracked captures.
- The size gate counting a tile the web default no longer loads.
- The thumbnail camera clipping tall spruces.
- A helper-name clash in the kit gate.

**PRD P2 checklist:** 15 of 20 boxes are checked, with evidence in each line. The 5 open:
1. MVP tiles approved (you).
2. The hero-water bar: the live wet band is in progress.
3. Music: your Suno cues.
4. Shipped-UI sign-off (you).
5. The web stall tail: explained, waiting on Q7.

**Board:** every card that waits on you is set to blocked, pointing at its item here. New backlog cards:
- the web stall tail
- the Settings sheet (graphics tier and audio)
- a perf re-run on the merged build
- the live wet band on kit beaches
