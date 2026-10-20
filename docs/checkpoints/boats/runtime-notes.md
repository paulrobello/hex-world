# Boats runtime contract (`boat_rowing`, `boat_sail`, `landing_stage`)

What the boat runtime needs from the three P3 pieces (owner D4, 2026-10-20: moored boats plus rowing and sailing routes, counts scaling with the connected water, at most 3 of each kind; rowing boats on rivers turning back at a bridge; sailing boats on the ocean and river mouths). Numbers are in the runtime frame (x, y up, z; Blender (x, y, z) exports as runtime (x, z, -y)) and come from the committed export reports (`assets-source/kit/<id>/export-report.json`, keys `geometry`, `parts`, `sockets`).

## Common to all three

- **Waterline.** The origin plane is the water surface at rest. Place the origin at `y = water::tiles::WATER_Y` (-1.4 m) plus the live surface height at the boat (rivers are flat with flow; the ocean has Gerstner waves).
- **The water shows through a hull that sinks.** The engine draws its water surface straight through the hull, so every inboard surface (floor, frames, thwarts, baskets) sits at least 0.03 m above the origin plane (`_boat.INTERIOR_MIN_Z`, asserted by the generators). Keep each boat riding its local surface: heave to the surface height at the hull centre, and pitch and roll to its slope. Sinking the origin more than 3 cm below the local surface floods the boat visually.
- **Riding the surface is not enough on swell.** Gerstner waves vary along a 3.5-5 m hull by more than the floor's 3 cm, so the water still shows inside the hull on coast and ocean tiles, even when the boat heaves, pitches and rolls with the surface. The `water_kit` close-ups show the look-dev swell's foam inside the static rowing boat (`docs/checkpoints/boats/boat_rowing-water-close-sun.jpg`). The water pass also has to drop its fragments inside each boat's hull: a plan-view ellipse per boat (centre, heading, half length, half beam), below the sheer. Alternatively, damp the swell under the boats. Until one of these lands, moored and anchored boats on coast and ocean tiles show water inside. Rivers are flat, so river boats are fine.
- **Draw scale 1.0.** The pieces are true size against the 32 m tile and the 8 m river.
- **Animatable parts.** A part is a node and mesh `<id>_<part>_lod<n>` beside each `<id>_lod<n>`, in every LOD, with `extras.kind` naming it (`hex_assets::part_of`, `part_mesh_name`). Its node sits on the pivot, and its mesh is authored in the node frame, so draw it at `placement * node.transform * swing`. Swap it with its LOD body. The kit scenario draws parts at rest (`kit_scene::spawn`, integration patch). `play.rs build_piece` draws only bodies and rotors, so the boat runtime spawns the parts itself. A part never uses the `<id>_rotor_lod<n>` name, because both runtimes spin that name nonstop.
- **Budget.** Each LOD's budget counts its body plus its parts. The gate counts both (`kit_gate::part_tris_count_toward_the_lod_budget`).
- **Anchors.** `anchor-prop-*` nodes with `extras.kind` mark seats, painters, perches and moorings (below). No runtime reads them yet.

## `boat_rowing`

| | Value |
|---|---|
| Hull | 3.57 m overall (x -1.65 transom to +1.92 stem head), beam 1.41 m, bow toward +x |
| Draft | keel bottom 0.15 m under the water; transom foot 0.06 m |
| Freeboard | sheer 0.42 m midships, 0.64 m at the stem head (`anchor-prop-1`) |
| Height | 0.72 m (stem head), clears the bridges' 0.95 m arch crown; D4 still turns it back at bridges |
| Collider | `boat_rowing_collider`, the LOD2 hull's box (picking only; not a walker obstacle on water) |
| Seat | `anchor-prop-0` (kind `seat`) at (0.15, 0.25, 0), facing aft (yaw 180): the rowing thwart for a later rower figure. None is modelled: no ref draws one |
| Painter | `anchor-prop-1` (kind `painter`) at (1.905, 0.64, 0), the stem head |

Oars: `boat_rowing_oar_port_lod<n>` and `boat_rowing_oar_stbd_lod<n>` (kind `oar`), 2.37 m long, both nodes with the hull's orientation.

| | Port (runtime -z side) | Starboard (+z side) |
|---|---|---|
| Pivot (rowlock pin) | (-0.18, 0.461, -0.685) | (-0.18, 0.461, 0.685) |
| Loom direction, node frame | (0, -0.235, -0.972) | (0, -0.235, 0.972) |
| Authored pose | square to the hull, loom sloping 13.6° down, the feathered (flat) blade's tip 0.05 m over the water | mirror |
| Sweep (about node +Y) | + trails the blade aft | - trails the blade aft |
| Dip (about node +X) | + lifts the blade | - lifts the blade |

- **Stroke.** Sweep within ±40° and dip within ±8°. The generator asserts that the blade and loom stay outside the hull (worst margin 0.30 m) and the grips stay over the thwarts (lowest grip 0.48 m against the seat's 0.25 m). A dip of about 4.3° buries the blade's tip 0.08 m. Feathering would be a third rotation, about the loom direction above.
- **Trailing, for moored or drifting boats.** Sweep 80° aft and lift 5°. The blades then lie alongside the hull, 0.30 m outside the gunwale. Use this pose when moored: in the authored pose the oars reach 2.4 m to each side, into the stage and the bank.
- **Rowing routes.** The boat with its oars out spans 4.8 m, 60% of the 8 m river. Two boats rowing past each other need their own halves of the channel (centre lines at about ±1.9 m off the river centre line), and a moored boat keeps its trailing pose.

## `boat_sail`

| | Value |
|---|---|
| Hull | 5.0 m (x -2.35 transom to +2.65 stem head); 6.13 m overall with the bowsprit (to x 3.385) and the rudder (to x -2.77); beam 1.90 m; bow toward +x |
| Draft | keel bottom 0.32 m under the water; rudder foot 0.28 m |
| Freeboard | sheer 0.58 m midships |
| Mast | axis at x 0.9 on the centreline, top 6.0 m over the water (`anchor-prop-2`, kind `perch`, for gulls) |
| Collider | `boat_sail_collider`, the LOD2 box, mast included |
| Helm | `anchor-prop-0` (kind `helm`) at (-1.95, 0.40, 0) on the stern sheets |
| Painter | `anchor-prop-1` (kind `painter`) at (2.65, 0.92, 0) |

Sails (kind `sail`), both authored on the centreline (sheeted amidships):

| Part | Pivot | Swing axis | Swing | Sign |
|---|---|---|---|---|
| `boat_sail_main_lod<n>` (gaff main, boom, gaff) | (0.9, 1.05, 0), the gooseneck on the mast axis; node with the hull's orientation | node +Y (the mast, runtime up) | ±70°; the shrouds stand 85.8° off the boom's rest bearing | + swings the boom to +z (starboard) |
| `boat_sail_jib_lod<n>` | (3.35, 1.0, 0), the tack at the bowsprit end | node +Y, which the node's rotation lays along the forestay: runtime (-0.514, 0.857, 0) | ±45° | + to +z |

- **Asserted clearances.** Every vertex over the hull's plan stays at least 0.28 m over the sheer at every 5° of either sweep (main 0.39 m, jib 0.31 m), and the jib stays 0.28 m off the mast.
- **Sailing.** Swing both sails to leeward, by about half the angle between the wind and the heading, capped at the limits above. The boat heels about its centreline. That is runtime work, since no heel is authored.
- **Where it sails.** Ocean tiles have 2.6 m of water. On beach and river-mouth tiles, keep at least 0.13 m of water under the keel: depth ≥ 0.45 m, which on the coast profile means s ≥ 3.6, just seaward of the shelf. The shelf joint (s = 3) is only 0.36 m deep. The mouth fan's centre, 0.58 m deep, is fine only along its centre line. The 6 m mast never clears a bridge.

## `landing_stage`

| | Value |
|---|---|
| Frame | origin on the bank's waterline (runtime y = WATER_Y), 4.0 m from the river centre line; local +x toward the centre line, perpendicular to the flow; local z along the flow, so the profile distance is d = 4 - x (water contract §2) |
| Deck | top 0.45 m over the water (runtime y -0.95); x from -0.75 to 1.40, z from -1.4 to 1.4 |
| Bank | meets the deck's underside at x -0.41 (d 4.41); the landward 0.34 m of deck tucks into the bank |
| Outer edge | x 1.40 (d 2.6, 1.0 m of water); fender board at x 1.43 |
| Posts | outer row x 1.3 at z -1.25, 0, 1.25 (bollards standing to 1.0 m, 0.55 m over the deck), inner row x 0.45 at z ±1.25; feet 0.15 m into the bed at their own d |
| Mooring | `anchor-prop-0` (kind `mooring`) at (2.535, 0, 0): d 1.465 with 1.38 m of water. It is the rowing boat's origin with its bow toward -z (the node's yaw), 0.37 m between the hull side and the fender (room for the stage-side oar in its trailing pose) |
| Bollard | `anchor-prop-1` (kind `bollard`) at (1.3, 1.0, -1.25), the outer post at the boat's bow end; the painter runs about 1.4 m from the stem head |
| Step | `anchor-prop-2` (kind `step`) at (-0.45, 0.45, 0), where a walker steps on from the bank |
| Collider | `landing_stage_collider`, the LOD2 box (deck plus two corner posts down to the bed), for picking |

- **Placement.** Authored against the straight channel. On the 24 m bend it fits either bank: the 2.8 m deck drifts at most about 0.05 m in d at its ends. Do not place it on mouth or spring tiles, whose beds differ.
- **Clearance.** The moored boat takes d 0.76-2.17 on its side, 0.46-2.47 with its oars trailed, which leaves 4.5 m of channel (d 0.46 to -4.0). A boat rowing past with its oars out spans 4.8 m and does not fit there: let it trail its oars past a mooring, or keep moorings off the rowing routes.
- **Walker.** The deck is walker ground, as the station platform is: add `landing_stage` to `scatter::GROUND_PROPS`, or the boat runtime's own ground list, for whatever places it. Leave it out of `walk::OBSTACLES`, because its box collider covers the deck itself. The bollards (0.55 m) are taller than `MAX_STEP` (0.4 m) and could be walker obstacles as small cylinders (r 0.09).
