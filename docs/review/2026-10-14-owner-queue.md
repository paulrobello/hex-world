# Owner review queue, morning of 2026-10-14

Everything below waits on you. Work kept going overnight on everything that did not need you. Paths are relative to the repo root. The review server (`.claude/launch.json`, `hex-world-review`, http://127.0.0.1:8095/) serves the repo, so an image path opens at `http://127.0.0.1:8095/<path>`.

## Approvals

**A1. O4 cutaway skirts, as shipped.** `docs/checkpoints/skirt-o4/sheet.jpg` (8 in-play shots). The soil set is now your O4 pick. Meadow, forest, house and river skirts sample it, and the beach and river cut walls copy it in the shader. Please re-approve `meadow_base`, `forest_base` and the river skirts on this look. Known issue: the beach tile's land walls face north, away from the sun, so they read near-black in shot 06. Every north-facing wall in the kit does the same.

**A2. River family (6 pieces: edge, straight, bend, bridge).** Merged as `4509640`. In-play shots, re-taken on main with the final bend mesh and the O4 skirts: `assets-source/kit/river_edge/shots/engine-01..09.jpg` (overview, the straight-to-bend and bend-to-bridge seams, the river mouth, the bridge arch, the bend, LOD1, LOD2). The bend's LOD2 uses a 20% triangle ceiling instead of 15% (decision D1).

**A3. Hero oak polish.** `assets-source/kit/tree_hero_oak/renders/p2_*.jpg`. Three of the six criteria are met: canopy masks (snow on the domes), limbs and roots, and the LOD2 silhouette. The other three need you:
- Bark AO mean is 0.645 over all bark (0.721 on visible bark) against a 0.75 target. Accept the shortfall, or ask for another pass.
- Dome smoothness at 8 m is improved, but the 3-fold bake repeat is still faintly visible on a single dome (`p2_dome8m-none.jpg`).
- Your overall approval against ref-1.

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

**D5. The coast corner bases get a 20% LOD2 ceiling, like the river bases (D1).** The Headland and Bay sit at about 18%. 15% was reachable only by padding LOD0 to about 3,950 triangles, which spends triangles where nobody sees them.

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

At the MVP values (noon, dry, summer, unfrozen) the game looks the same as before. The pixel diffs sit inside run-to-run noise, and the texture budget holds at 16. The asset gate now fails any new piece or shader that lacks a hook. 21 small prop slots on the river, beach and ocean centre pieces still need authored masks maps; that Blender work runs overnight.

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

## Questions

**Q1. Save label (resolved).** Autosave now exists, so the bar shows "Saved" as the brief asked, plus "Saving…" and "Autosave off". See A16.

**Q3. Forest edge corners.** Where a forest-forest edge sits next to a forest-meadow edge on the same tile, the litter has to stop at the corner. There are two options, shown in `assets-source/kit/forest_forest_edge/stills/corner-options/` after merge: `before.jpg`, `O-a-after.jpg`, `O-b-after.jpg`, `O-b-overview.jpg`.
- **O-a (shipping now):** the litter ends in a straight slope along the line from the corner.
- **O-b:** the litter pulls back into forest_base's own wavy grass rim. It reads better, but it needs 18 more pieces (about 0.9 MB) and curve-exact cuts.

I recommend O-a now and O-b once forest moves into a streamed biome pack. The resolver passes the corner flags either way, so switching later needs no engine change.

**Q2. Push.** Main holds 62+ unpushed local commits. Nothing has been pushed. CI does not run the UI checks yet, and I am fixing that parity gap before asking. Approve one push once `make checkall-ci` is green?

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

## Decisions I made overnight (override any)

**D1. Lane-cut LOD2 ceiling.** River bases may keep 20% of LOD0's triangles at LOD2. At 15% the bend's channel profile drifts past the water contract's 0.03 m tolerance. Every other piece stays at 15%.

**D3. The forest edge piece comes in six per-edge variants.** forest_base is not the same under each of its six edges. A single rotated edge piece therefore had to float up to 0.18 m above the litter to clear every edge (`assets-source/kit/forest_forest_edge/stills/mock-seam-after.jpg`, once merged). Six variants, one per local edge of the base, fit exactly. The edge-piece resolver picks the family by socket pair and the variant by the base's local edge.

**D2. Forest-forest edge piece first, engine wiring second.** The Blender piece is being built now. Choosing edge pieces by (own, neighbour) socket pair touches the same socket code as the beach stream, so that part waits for the beach stream to merge.

## Status

The status of each stream is filled in as streams land overnight; see the bottom of this file.
