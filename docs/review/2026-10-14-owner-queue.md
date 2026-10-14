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
2. Open http://127.0.0.1:8094/?scenario=play.
3. Click once. Nothing loads or plays before the first interaction.
4. Place tiles, rotate, undo and bulldoze, then zoom onto a river, coast or forest to bring its ambience up.

Mute and volume live in the console until the Settings screen exists: `hexWorld.audio.setMuted(true)` and `hexWorld.audio.setVolume(0.5)`.

Not built yet:
- town murmur, the per-tile placement stingers and all music. You generate the music in Suno.
- an audio key. Neither PRD §4.5 nor the Legend design defines one.

**A7. Cars: sedan, van and pickup with runtime paint.** In play: `docs/checkpoints/cars-kit/cars-kit-01-busy.jpg`, `-02-hub`, `-03-closeup-sedan`, `-03-closeup-van`, and night `-04`/`-05`. Blender six-views are in `assets-source/kit/car_*/renders/`. There are 12 palette colours, and the draw count does not grow with colour. There is no in-engine close-up of the pickup yet.

## Questions

**Q1. Save label.** The HUD bar says "Not saved" because no autosave exists yet. The brief said "Saved". I recommend keeping "Not saved" until autosave lands, so the label tells the truth.

**Q3. Forest edge corners.** Where a forest-forest edge sits next to a forest-meadow edge on the same tile, the litter has to stop at the corner. There are two options, shown in `assets-source/kit/forest_forest_edge/stills/corner-options/` after merge: `before.jpg`, `O-a-after.jpg`, `O-b-after.jpg`, `O-b-overview.jpg`.
- **O-a (shipping now):** the litter ends in a straight slope along the line from the corner.
- **O-b:** the litter pulls back into forest_base's own wavy grass rim. It reads better, but it needs 18 more pieces (about 0.9 MB) and curve-exact cuts.

I recommend O-a now and O-b once forest moves into a streamed biome pack. The resolver passes the corner flags either way, so switching later needs no engine change.

**Q2. Push.** Main holds 62+ unpushed local commits. Nothing has been pushed. CI does not run the UI checks yet, and I am fixing that parity gap before asking. Approve one push once `make checkall-ci` is green?

## Findings

**F1. Walls facing away from the sun render almost black in play.** Examples: the river mouth's cut wall (`assets-source/kit/river_edge/shots/engine-04-mouth-straight.jpg`), the bridge arch's underside (`engine-05-bridge-arch.jpg`), and the beach tile's land walls (`docs/checkpoints/skirt-o4/skirt-o4-06-close-beach.jpg`). With IBL on, kit scenes set ambient light to 0 (`crates/hex-render/src/scene.rs`, `GlobalAmbientLight`). That leaves the noon IBL as the only fill on faces the sun misses, and here it barely lifts the O4 soil off black. I have not yet confirmed whether the IBL is weak, missing in native play shots, or crushed by SSAO. My recommendation is to have an agent diagnose this and lift the fill globally, then re-shoot these three, rather than patch individual materials. Tell me if you want that, or if the dark north side is intended.

## Decisions I made overnight (override any)

**D1. Lane-cut LOD2 ceiling.** River bases may keep 20% of LOD0's triangles at LOD2. At 15% the bend's channel profile drifts past the water contract's 0.03 m tolerance. Every other piece stays at 15%.

**D3. The forest edge piece comes in six per-edge variants.** forest_base is not the same under each of its six edges. A single rotated edge piece therefore had to float up to 0.18 m above the litter to clear every edge (`assets-source/kit/forest_forest_edge/stills/mock-seam-after.jpg`, once merged). Six variants, one per local edge of the base, fit exactly. The edge-piece resolver picks the family by socket pair and the variant by the base's local edge.

**D2. Forest-forest edge piece first, engine wiring second.** The Blender piece is being built now. Choosing edge pieces by (own, neighbour) socket pair touches the same socket code as the beach stream, so that part waits for the beach stream to merge.

## Status

The status of each stream is filled in as streams land overnight; see the bottom of this file.
