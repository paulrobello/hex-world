# Rail tunnel bore and open portal: integration (2026-11-15, section O1)

The owner picked O1 on 2026-11-14 (a turf ridge over the bore). The coordinator gave the slot contract
and Q4 (no tiles on the cover cells; it clears scatter and grass in the ridge corridor). This branch
builds both pieces: `rail_tunnel_bore` (new) and `rail_tunnel_portal` (open slots added). Every number
below is read from the build (`assets-source/kit/<piece>/bake-report.json`) unless it says otherwise.

## What lands with the branch

- Generators: `scripts/blender/kit/_tunnel_portal_rail.py` (the shared open-tunnel contract: `open_ring`,
  `open_lining`, `ridge_profile`, `add_ridge`, plus the portal's `_back` and `_open` slots),
  `rail_tunnel_portal.py` (colours for the new slots), `_rail_tunnel_bore.py` and `rail_tunnel_bore.py`.
- Kit: both GLBs, both texture sets and the manifest entries (`assets/kit/manifest.json`). The portal's
  maps and KTX2 files are byte-identical to the approved build. Only its GLB changed.
- Not on the branch: `integration.patch` (below). Without it, `kit-check` fails on the portal's LOD0
  budget and `checkall` fails on the grass slot list.
- Review sheet: `sheet.jpg` (`sheet.py` over the `tunnel-*.jpg` stills from `tunnel_still.py`, a
  Blender EEVEE stand-in with a hill of plain hex cells; not an engine capture). It shows the complete
  tunnel from the orbit camera, the ridge close up, the lone portals (closed state), the mouth, the
  engine in the bore, a lamp beside the engine, the edge-0 seam, night, and the orbit at LOD1 and LOD2.

## Slot contract (the runtime wires it)

| Piece | Hidden while the tunnel is complete (`_back`) | Shown only while complete (`_open`) | Always drawn |
|---|---|---|---|
| `rail_tunnel_portal` | `_back` (the back plane, x 11.456), `_lining_back` (the dark lining, was `_lining`), `_barrel_back` (the 0.75 m barrel, LOD0 and LOD1 only), `_stone_back` (the back face's cells over the opening at x 13.856) | `_stone_open` (stone lining from the soffit, x 8.306, to edge 0), `_turf_open` (the ridge from x 10.456 to edge 0) | `_stone`, `_fern`, `_turf`, `_iron`, `_glass` |
| `rail_tunnel_bore` | none | none (the piece only exists while complete) | all 8 slots |

- `rail_tunnel_portal_lining` is renamed `rail_tunnel_portal_lining_back`, same colour and roughness.
  Nothing in `crates/` names it.
- Closed state proof: with the `_open` faces removed, the six `make kit-renders` views of the new GLB
  are byte-identical to the approved look-1 JPEGs (all six, max pixel difference 0). The receiver
  signature, bake stats, atlas islands, KTX2 sizes (1,103,227 B bc7) and `colliderX` [7.726, 13.856]
  are unchanged. Rerun: `state_render.py` (the six kit cameras with one state's slots removed) and
  `diff_renders.py` (against the JPEGs at 170094d) in this folder. The committed
  `assets-source/kit/rail_tunnel_portal/renders/*.jpg` are plain `make kit-renders` output with every
  slot drawn, so they show the `_open` ridge and lining too.
- Draws per portal: 9 closed (the approved 7 plus `_barrel_back` and `_stone_back`, split from
  `_stone`), 7 complete. Per bore: 8 at LOD0 and LOD1, 6 at LOD2 (no sleepers, no lamp iron).

## Placement

- Kit showcase: no `kit_scene.rs` `kit_layout` cell (and no `tile_counts` change), the same as
  `road_tunnel_portal` and `rail_tunnel_portal`. The bore needs its portals, cover cells and cut
  columns around it to read, so it is reviewed in tunnel shots (`sheet.jpg` here, the engine capture
  later), not on the showcase ring.
- Runtime placement per cover cell over a complete rail tunnel: `rail_tunnel_bore` at the cell centre,
  y = 4T (the tunnel's level, not the cover's), yaw = the tunnel axis (either direction).
- Bore origin: the cover cell's centre at the tunnel's (portal's) level, axis along the tunnel, edge 3
  at x = -13.856 and edge 0 at x = +13.856. The piece is symmetric under a half turn (ring, girth v
  measured from the crown, lamps, ridge and track), so either yaw along the axis fits.
- Seams (asserted in both generators, every LOD): at x = +-13.856 the bore's lining equals the portal's
  `open_section(lod)` and its ridge `ridge_section()`, the track `_rail.section()`. The portal's
  `_stone_open` and `_turf_open` end at its edge 0 on the same sections. Ring points per LOD: 29, 15, 9
  (the jamb feet, the portal lining's own 22, 7 and 5 arch divisions, the crown, and at LOD0 and LOD1
  four girth cuts, every panel height of 2.32 m from the crown).
- The stone continues across edge 0: the bore's maps are the portal's, baked from the same templates and
  materials. The panel island's pixels are identical in albedo, normal, ORM and masks (max difference 0),
  and both pieces end every lining cell on the island's u wrap.

## Ridge (O1)

- Section (s, h) in metres over the tunnel level, the same in every LOD of both pieces:
  (0, 5.75), (+-1.5, 5.543), (+-3.0, 5.061), (+-4.5, 4.510), (+-6.0, 4.097), (+-7.0, 4.0), (+-7.4, 3.94).
  It eases with zero slope to the 4.0 m cover at |s| = 7 and dives 6 cm under the cover meadow by 7.4,
  so it never lies coplanar on the meadow cap.
- One cell along x in each piece (the turf set is sampled in world space). Smooth analytic normals, +Z
  at the crest and at |s| >= 7.
- Cover over the lining: 0.725 m at LOD0 (0.735 and 0.767 m at LOD1 and LOD2), over the 0.3 m floor.
- In the portal it starts at x 10.456 (the mound's top), buried in the mound there (asserted), and breaks
  out of the mound as the mound falls to 4.0 at edge 0.

## Cuts in `spawn_column` and the skirts (read from the code, not rendered)

For a tunnel at level T (world height 4T) under cover cells at level T + 1, in the bore's local frame
(s across the axis):

1. Cover cell, base skirt: the 4 m skirt (`meadow_base_skirt`, or the runtime slope skirt) on edges 0
   and 3, |s| <= 2.6, from 4T - 0.3 up to the cap at 4T + 4.0. This is the cut that opens the tube.
2. Cover cell, cliff column (`column_mesh`): sides k = 5 (edge 0) and k = 2 (edge 3) in the cell's
   frame turned with the bore, |s| <= 2.6. For a level-1 cell over a level-0 tunnel the column spans world
   0.05 down to -4.0, so the cut is only its top band, 0.05 down to -0.3 (it would show 5 to 10 cm over
   the ballast at the walls).
3. Cover cell, cap: the meadow cap at 4T + 4.0 passes through the tube where |s| < 2.05 (the arch is above
   4.0 there). It faces up, so it is culled from inside the tube and hidden under the ridge from above.
   Nothing to cut, but nothing may grow or stand on it there: no grass blades, scatter or tiles within
   |s| <= 7.4 of the axis (the ridge's footprint).
4. Portal tile, edge 0: the Cutting field is 0 for |s| <= 4 all the way to edge 0, so its skirt there runs
   from 4T down to 4T - 4 and crosses the tube only at the buried lining feet. Cut |s| <= 2.6 from
   4T - 0.3 to 4T (under the ballast except a 5 cm band at the walls).

## Walker and train corridor

- Free section: half width 2.60 m from the floor to the springing at 2.4 m, then the semicircle
  s² + (h - 2.4)² = 2.6² to the crown at 5.0 m (the LOD0 ring; LOD1 and LOD2 chords sit up to 6.5 and
  12.7 cm inside it).
- Floor (both ends exactly `_rail.section()`): rail top 0.46 m, ballast top 0.22 m for |s| <= 1.75 m,
  falling to 0.03 m at |s| = 2.5 m and -0.05 m at the wall; sleeper tops 0.30 m.
- Lamps at x = -10.392 (+s wall), -3.464 (-s), 3.464 (+s), 10.392 (-s), 6.928 m apart across joints.
  At a lamp the free half width drops to 2.175 m between h 2.0 and 2.52 m.
- Trains: the lining stays 0.340 m over the vehicles' envelope at LOD0, 0.275 m at LOD1 and 0.213 m at
  LOD2 (the 0.15 m floor holds). Lamps stand 0.695 m clear of the widest vehicle (half width 1.48 m).
- The portal's corridor runs from the face (x 7.856) to edge 0; with the tunnel complete nothing stands
  in it from x 8.306 on (the back plane and back face's opening are `_back`).

## Runtime consequences to wire (not in the patch)

- `scatter.rs` `GROUND_PROPS` lists `rail_tunnel_portal`, so the walker and scatter take every LOD0
  triangle of the portal as ground. Without slot filtering, the hidden `_turf_open` ridge (up to 5.75 m)
  becomes walkable ground and scatter keeps off it even on a lone portal. `_open` slots need to leave the
  ground surface unless the tunnel is complete, and `_back` slots need to leave it while it is.
- Grass: `rail_tunnel_portal_turf_open` and `rail_tunnel_bore_turf` are grass slots (`tileable: grass`),
  so blades would root on them. Per Q4 they stay bare: exclude both slots from blade planting (and
  `_turf_open` in any case while the tunnel is incomplete).
- The bore is not a ground prop and has no walker rows. Its `<id>_collider` is the box of the LOD2 lining
  (the whole tube, x +-13.856, |s| <= 2.6, h -0.3 to 5.0), so `walk.rs` `piece_shape` would make the
  whole tube an obstacle. A walkable tunnel needs the bore skipped as an obstacle (or wall-only rows).
- Shading inside the tube: the lining's masks AO is 1 (it is the portal face's panel island), so the tube
  takes the same ambient as the face. Only the sun shadow of the hill darkens it.

## LOD seams

Neighbouring pieces at the same LOD meet vertex for vertex. Where two neighbours draw different LODs,
the arch polylines differ: gaps up to 6.5 cm between LOD0 and LOD1 and 12.7 cm between LOD1 and LOD2
(the coarser LOD's chord sagitta), visible as slivers from inside the tube. Choosing one LOD for every
piece of a tunnel (the nearest cell's) removes them.

## Budgets (prop: LOD0 <= 1,500, LOD1 <= 0.5 x LOD0, LOD2 <= 0.15 x LOD0)

| Piece | LOD0 | LOD1 | LOD2 | Notes |
|---|---|---|---|---|
| `rail_tunnel_bore` | 1,454 | 462 (cap 727) | 110 (cap 218) | stone 896, turf 24, glass 32, iron 64, track 438 at LOD0 |
| `rail_tunnel_portal` approved | 1,491 | 735 | 207 | |
| `rail_tunnel_portal` open | 1,739 | 815 (cap 869) | 247 (cap 260) | +248 / +80 / +40: `_stone_open` 224 / 56 / 16, `_turf_open` 24 each |

The ridge's turf adds 24 triangles per LOD in each piece. The portal's +248 at LOD0 breaks both the
1,500 prop cap and the brief's ~100-triangle allowance, so `integration.patch` requests a scoped LOD0
ceiling of 1,800 for `rail_tunnel_portal` (an owner decision; not granted). The alternative that keeps
the portal at 1,491: move the two `_open` slots into their own prop (`rail_tunnel_portal_open`, about
250 tris) that the runtime draws on a complete portal's tile.

Textures (KTX2 class `piece`, bc7): portal 1,103,227 B (unchanged), bore 1,070,924 B, under 1.5 MB.
GLB: portal 168,836 B, bore 113,236 B.
Texel density at LOD0: bore stone 263.1 px/m median, glass 258.1; portal stone 258.1, `_stone_open`
true scale (258.1).

## `integration.patch`

Applies on 170094d (`git apply --check`). Contents:

1. `scripts/blender/lib/budgets.py` and `crates/hex-play/tests/kit_gate.rs`: `OPEN_PORTAL_LOD0_MAX = 1800`
   for `OPEN_PORTAL_PROPS = ("rail_tunnel_portal",)`, the parity check and the scoped-budget test.
2. `crates/hex-play/src/play/grass.rs`: the grass slot list gains `rail_tunnel_bore_turf` and
   `rail_tunnel_portal_turf_open`.

The gates in this report ran with the patch applied in the worktree.

## Gates (2026-11-15, macdaddy, load-tolerant)

| Gate | Exit | Tail |
|---|---|---|
| `make kit-build PIECE=rail_tunnel_portal` | 0 | `tris per LOD [1739, 815, 247]`, stone median 258.1 px/m, portal sweep margin 0.320 / 0.275 / 0.213 m |
| `make kit-build PIECE=rail_tunnel_bore` | 0 | `tris per LOD [1454, 462, 110]`, stone 263.1, glass 258.1 px/m, lamp margin 0.695 m, ridge cover 0.725 / 0.735 / 0.767 m, `section errors []` |
| `make kit-compress` portal / bore | 0 / 0 | `glb 168836 B, ktx bc7 1103227 B, astc 1101720 B` / `glb 113236 B, ktx bc7 1070924 B, astc 1069605 B` |
| `make kit-determinism` portal / bore | 0 / 0 | `deterministic 8b86d958...` / `deterministic 1292ec5d...` |
| `make kit-renders` portal / bore | 0 / 0 | six views each |
| `make kit-provenance ... REFS=assets-source/refs/rail_tunnel_portal` portal / bore | 0 / 0 | `wrote .../provenance.json (validation.pass=true)` |
| closed state: `state_render.py` + `diff_renders.py` (rebuilt GLB vs 170094d) | 0 / 0 | 6 of 6 `bytes identical True, max 0` |
| `make kit-check` | 0 | `73 passed; 0 failed; 2 ignored` |
| `make checkall` | 0 | kit_gate `73 passed`, Biome `Checked 58 files`, UI `tests 244`, shots `Ran 13 tests ... OK` |
| `cargo test -p hex-play -p hex-render --lib -- boids scatter walk kit_scene` | 0 | hex-play `147 passed; 1 ignored`, hex-render `22 passed` |
| `git apply --check --cached integration.patch` on a 170094d index | 0 | budgets.py, kit_gate.rs, grass.rs |
| `make ui-e2e` | 2 | `e2e/ui.mjs`: `ui e2e FAILED (20)`, from `timed out: slot key prints what the slot needs` on, plus a HUD `TypeError: Cannot read properties of undefined (reading 'toLowerCase')`. The make target stops there. |
| `node e2e/ui.mjs dist` rerun (load about 40 to 60) | 1 | the same 20 |
| `node e2e/ui.mjs` on a `make build-web` of 170094d (scratch worktree, since removed) | 1 | the same 20, outcome list identical line for line: the failure is on the base, not this branch |
| `node e2e/photo-fx.mjs dist` / `saves-ui.mjs dist` | 0 / 0 | `photo-fx e2e: all ok` / `saves-ui e2e OK` |
| `node e2e/ui-motion.mjs dist` / `persist.mjs dist` | 1 / 1 | `FAILED (20)` / `FAILED (3)`, each with the same HUD `toLowerCase` TypeError; not rerun on the base build |

Not run: `make checkall-ci` (not in the brief) and any engine capture (needs a booked GPU window).
