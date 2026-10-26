# Water bed contract (river and coast kit families)

**Status: DRAFT (W3c, 2026-10-13).** The numbers below are what the engine renders in `play` today. The ones marked *may move* wait on the owner's look check of water in play. A change to any number is announced and lands in the engine and this file in the same commit.

**The profiles in §2–§4 are requirements, not illustrations.** The engine never reads the bed geometry. It computes every per-vertex water input from these analytic profiles, and it keeps doing so after a kit piece replaces the placeholder bed. Those inputs are water depth (wave damping, swash weight), shore distance (swash phase, wet line), river weight and flow. A kit bed that drifts from its profile shows as swash running up a slope that isn't there, or waves damped over the wrong depth.
- **Where the water is shallower than 2.5 m** (river banks, the beach slope and shelf): stay within ±0.03 m of the profile. Swash and wave damping act here.
- **Elsewhere:** stay within ±0.10 m. That covers the river bed below 2.5 m of water, the ocean floor, and the dry bank and beach above y = -1.0. On the ocean floor the 0.10 m is all upward (§4). Gerstner waves reach full height and the swash stops only at 2.5 m depth, so a patch raised more than 0.1 m would put the water there under 2.5 m and bring damping and swash back.
- **Border ring:** every vertex sits exactly on the profile, so neighbouring pieces meet without a step.

Sources: `crates/hex-render/src/water.rs` (the approved look-dev and the shared mesh builders), `crates/hex-render/src/water/tiles.rs` (play water and the placeholder beds), `assets/shaders/water.wesl`, `assets/shaders/water_terrain.wesl`, `crates/hex-render/src/water_common.wesl`, `scripts/blender/kit/meadow_base.py`, `data/sockets.ron`, `data/tiles/*.ron`.

Coordinates are runtime metres, Y up, tile-local before rotation, with edge 0 facing +x and edges running clockwise viewed from +Y (`hexgeo.py`). `R = 16`, apothem `13.856`, `LEVEL_STEP = 4`.

## 1. Levels

| Level | y (m) | Grounding |
|---|---|---|
| Meadow cap border ring | 0.0 | `_terrain.meadow_relief`: relief in [-0.25, 0], hollows on a plane level with the ring, faded to 0 over the last 3 m of the border |
| Water surface at rest | **-1.4** *(may move)* | `water.rs` `LAND = 1.4`: the approved look-dev puts the grass plateau 1.4 m above the water. The terrain shader's grass line (0.82-1.08 m above the water) and wet band (0.32-0.40 m) are tuned to it. -1.4 is 0.35 `LEVEL_STEP` |
| River bed, centre line | -2.95 | 1.55 m below the water (`carve`: 1.5 m bowl + 0.05 m) |
| Ocean bed | **-4.0** *(may move)* | The tile base. 2.6 m of water: Gerstner waves reach full height at 2.5 m depth (`wave_atten`) and the swash stops there (`swash_weight`), so ocean seams carry no shore-dependent motion |
| Tile base (column floor) | -4.0 | `-LEVEL_STEP`, the bottom of every column (`meadow_base` skirt) |

The prototype's open sea reaches 6 m deep and reads deeper blue than 2.6 m will. A deeper ocean needs a deeper ocean column than the -4 m base, which is the owner's call. This is the main number that may move.

## 2. River channel (`river_straight`, `river_bend`, `river_bend_60`, `river_road_bridge`)

- **Width.** The lane is 8 m (`data/sockets.ron`) and is the water width at rest: the bank crosses y = -1.4 at 4.0 m from the centre line.
- **Profile.** The look-dev's `carve` with half-width parameter `w = 4.33`. With `d` the distance to the centre line, relative to the water surface:
  - `bed(d) = -1.5 * max(0, 1 - (d/w)^2) - 0.05`
  - `bank(d) = smoothstep(0.7 w, w + 2.5, d)`, from 3.03 m to 6.83 m
  - `h(d) = min(1.4, bed + (1.4 - bed) * bank)`, so `y = -1.4 + h(d)`
  - Spot values (y): d = 0: -2.95; d = 3: -2.23; d = 4: -1.40 (waterline); d = 5: -0.69; d >= 6.83: 0.0 (meadow level).
  - The channel is 13.66 m wide at the top.
- **Centre line.** From the lane point on the `from` edge to the lane point on the `to` edge (edge midpoint plus the lane offset, 0 for every MVP river). With no control point it is straight. With one point it is the circle through the three points: for `river_bend`, the 24 m radius arc centred on the edge-1 neighbour's centre `(13.856, 24)`, as `hex_sim::geom::segment_shape` reads it. With two or more points it is the polyline through them, and the bed's `d` is the distance to that polyline: for `river_bend_60` (lanes on adjacent edges 0 and 1), the straight `river_edge` stub from each edge midpoint in to the joint 1 m inside the border, joined by the 7.423 m radius arc tangent to both about `(12.856, 7.423)`, turning 120 degrees, sampled every 2.5 degrees (chord sag 0.002 m; `scripts/blender/kit/_river_bend_60.py`). The tight bend cannot use the arc about corner 0 through both edge midpoints: it sits 0.063 m off the frozen straight stub at the joint, about 0.06 m of bed height at the waterline. Every shape meets each lane edge at a right angle.
- **Bank tuck.** The engine's water strip runs 5 m either side of the centre line, so at `d = 5` the bank must stand at least 0.5 m above the water. The profile gives 0.71 m.
- **Width evidence.** The approved prototype river is 6 m wide (half-width 3), widening to 10 m at the mouth. 8 m sits inside that range, the road (6 m) bridges it, and the 13.66 m channel leaves at least 7 m of meadow to the tile corners, so **keep 8 m**. No change to `data/` is proposed.

## 2a. River fork (`river_fork`)

Owner request 2026-10-20 ("we need a river fork also"). River on edges 0, 2 and 4, a symmetric Y like `road_junction`: three segments from the lane edges to the node `fork` at the tile centre. A River node shared by two or more segments is a **junction** (`water::tiles::river_junction`), never a spring or a mouth, so the tile is a plain river tile (`WaterKind::River`).

- **Bed.** The §2 profile at `d` = the distance to the nearest arm (`river_height`, exactly as on `river_straight`). Within 60 degrees of an arm's axis that arm is the nearest, so in its wedge the bed depends only on the lateral offset; two wedges meet on the bisector, where the bed folds into a ridge between the arms. The water reaches 4.0 m from each axis and 4.62 m from the centre along the bisectors (4 / sin 60), so the centre reads as a widened pool; the bank tops meet on the bisectors at 7.89 m, leaving about 6 m of meadow to the grass edges 1, 3 and 5. Each River edge's ring and joint row are `river_straight_base`'s, so every seam with a river tile matches. Kit: `scripts/blender/kit/_river_fork.py` (`LAYOUTS["fork"]`, built by `_river.build_base` with `_river_flat`'s relief); `fork_bed_matches_the_kit_profile` pins the spot values on both sides.
- **Surface.** One hex-grid surface at -1.4, kept within 5.5 m of any arm (three strips would overlap at the centre), meeting each neighbour's strip at the River edge as a spring's does.
- **Flow.** Each arm's direction comes from the junction rule (§6). On the tile the flow is the arms' downstream tangents averaged with weights falling from 1 on an arm's centre line to 0 at 8 m from it (`JUNCTION_BLEND`), so it turns from the inflows into the outflows across the centre and slows to 2/3 where they meet; where fewer than two arms are within 8 m (each River edge's whole strip) it is the nearest centre line's flow, the same paths and rule the neighbour uses.
- **Fish and boats.** The three River edges are wet and continuous, so the fish region flood fill joins all three neighbours into one region (river water). The river network (`hex_sim`, `oriented_rivers`) ends each arm's path at the junction (`RiverEnd::Junction`); a boat route continues from one arm into the others there.

## 3. Coast (`beach`, `beach_outer`, `beach_inner`)

Owner decision O1 (2026-10-13): beach sockets are directional, and two coast-corner tiles join the straight beach. This section is the bed the coast kit pieces must match: `beach_base`/`beach_center`, `beach_outer_base`/`beach_outer_center` and `beach_inner_base`/`beach_inner_center` (built, `scripts/blender/kit/_beach.py`; each base is the bed and `kit_gate` holds every LOD to this profile through `water::tiles::coast_height`, `coast_bases_are_held_to_the_coast_profile`).

- **Sockets.** Every beach edge names the side its land is on, looking out across the edge from its own tile: `land: Right` puts the land at the corner the edge shares with the next edge clockwise (`e + 1`), `Left` at the corner it shares with `e - 1` (`data/sockets.ron` `directional: [Beach]`). A beach edge meets only a beach edge with the opposite side, so both tiles put their land corner on the same point, and it never meets grass, forest or ocean. Every legal coast seam therefore continues the same coast, and an ocean or grass neighbour only ever touches an ocean or grass edge.

| Tile | Edges 0-5 | Land right | Land left | Coastline (rotation 0) |
|---|---|---|---|---|
| `beach` (straight) | O O B G G B | 2 | 5 | Straight line through the centre along the edge-2/edge-5 axis, sea toward 30 degrees |
| `beach_outer` (headland) | O O B G B O | 2 | 4 | 24 m arc about C = (-27.713, 0), two apothems past grass edge 3, land inside |
| `beach_inner` (bay) | O B G G G B | 1 | 5 | 24 m arc about C = (27.713, 0), two apothems past ocean edge 0, sea inside |

- **Seaward distance `s`.** The signed distance from the tile's coastline, positive out to sea, with `p` the tile-local position (rotation turns the coastline with the tile). The coastline is the waterline through the midpoints of the two beach edges, crossing each at a right angle:
  - `beach`: `s = p · (cos 30°, sin 30°)`.
  - `beach_outer`: `s = |p - C| - 24`.
  - `beach_inner`: `s = 24 - |p - C|`.
  - The arc's centre lies on both beach edges' lines (they meet two apothems out, 27.713 m from the tile centre) and the radius is 1.5 R = 24 m, the distance from there to each beach-edge midpoint. So along every beach edge `s` is the distance from the edge midpoint toward the sea corner, linear from -8 at the land corner through 0 at the midpoint to +8 at the sea corner, on all three tiles. This is what makes beds meet across every legal beach seam.
  - Ocean edges lie at `s >= 8` and grass edges at `s <= -8`. The tight spots are the bay's ocean edge (8 at its corners, 10.14 at its midpoint), the headland's ocean edges next to a beach edge (8 at the shared corner) and the headland's grass edge (-8 at its corners, -10.14 at its midpoint).
  - The gradient of `s` along a beach edge is the edge direction on all three tiles, so the bed's slope across the seam matches too, not only its height.
  - The engine derives the coastline from the edge kinds (beach edges and the grass beside them), never from the `land` sides, so `coast_beds_match_across_every_legal_beach_seam` in `water/tiles.rs` checks the sides against the geometry: for all 3 x 3 tile pairs, every rotation pair the sockets accept across a beach edge (108) has matching beds, depth and shore distance along the whole edge, and every beach-to-beach pairing they refuse (108) steps by the full 4 m.
- **Profile** (y as a function of s, the same on all three tiles):
  - `s <= -8`: 0.0 (grass level)
  - `-8 < s < 0`: `-1.4 + 1.4 t (2 - t)` with `t = s / -8`. This is the look-dev beach curve over 8 m instead of 12, and it meets the grass with zero slope (0.35 m/m at the waterline).
  - `s = 0`: -1.4, the waterline: the coastline itself, through both beach-edge midpoints
  - `0 <= s <= 3`: the shelf, -0.12 m/m (the look-dev offshore slope) to -1.76
  - `3 < s < 8`: drop to the ocean bed, `-1.76 - 2.24 * smoothstep(3, 8, s)`
  - `s >= 8`: -4.0
  - Spot values (y): s = -8: 0.0; -6: -0.09; -4: -0.35; -2: -0.79; 0: -1.40; 1.5: -1.58; 3: -1.76; 4: -1.99; 5.5: -2.88; 7: -3.77; 8: -4.0.
  - The water is shallower than 2.5 m for `s < 7.36`.
  - The beach width (8 m) and shelf length (3 m) *may move* with the look check. Moving either moves the profile on all three tiles together.
- **Tolerances for the coast kit pieces** (the general rules at the top, applied to the coast):
  - **Border ring:** exactly on the profile. Beach edges carry the profile at `s` = distance from the edge midpoint toward the sea corner, grass edges sit at y = 0 and ocean edges at y = -4.0.
  - **Where `s < 7.36`** (the beach slope, the waterline and the shelf): within ±0.03 m of the profile. At the waterline the slope is 0.35 m/m, so the kit's waterline lies within about 0.09 m of the coastline (the straight line or the 24 m arc). On the headland and the bay the waterline must curve with the arc, not run straight between the beach-edge midpoints. The chord between them sits 3.2 m off the arc halfway along it.
  - **Elsewhere** (the dry beach top and grass above y = -1.0, the drop and the sea floor): within ±0.10 m. The sea floor's 0.10 m is upward only, as in §4.
  - **Rotation:** pieces are authored at rotation 0 in the frame of the table above and turned with the tile, like every kit piece. The headland and the bay have no mirror image in the catalog. Do not mirror one to make the other: the bay's arc bends around the sea, the headland's around the land.
- **Wet band.** The placeholder bed's shader darkens and glosses sand up to 0.32-0.40 m above the water (y -1.4 to about -1.0) and follows the live swash run-up: `0.22 * swash`, from -0.09 to +0.22 m, only where the water is shallower than 2.5 m. The kit beach carries the same **live wet band** on its own material slot, which owns the whole gradient, as in the approved water look-dev:
  - The slot is `beach_wet`: the shared `plaster` set (the same set as the dry sand, `beach_sand`) with the dry-sand tint `[1.0, 0.95, 0.8]`. It was a darker, browner static damp band until the owner retinted it to dry sand on 2026-10-14 (A11, A22), with no rebuild. No texture and no vertex colour is added.
  - The slot runs from the waterline (s = 0) up to y = -0.8, 0.6 m above the water (s = -1.95), above the reach of the live band. Its edges are rows of constant `s`, so it follows the 24 m arc on the headland and the bay.
  - The live band: `beach_wet` sets `wetBand: true` (manifest), so `KitMaterial` runs the placeholder's band math on it (`kit_material.wesl`, `FLAG_WET_BAND`) with the dry-sand tint as its base: damp sand darkens and glosses up to 0.32-0.40 m, and sand the swash reaches is darker and glossier still, on the water's clock (`water::tick`). The bed carries no shore distance, so the shader reads `s` back from the bed height by inverting this profile (`water_common::coast_shore`, one to one on the beach slope and the shelf). The swash it evaluates is therefore the water surface's at the same world point, and the soaked line follows the run-up across every seam. No texture is added. Rain (§9.4 wetness) wets only what the band left dry. The band fades out on faces too steep to hold run-up (`smoothstep(0.3, 0.6, world_normal.y)`, owner 2026-10-18): cliff walls stay dry while the beach slope (normal.y ≈ 0.985) keeps the full band.
  - The seabed side: the gravel `beach_seabed` slot starts at the water at rest (`_beach.py` `slot_of`), so the retreating swash used to bare gravel against sand retinted dry on 10-14, a hard line at the waterline (owner report 2026-10-19). The seabed slots set `wetBand` and `wetBandSeabed` (`FLAG_WET_SEABED`): above the water and in the shallows the bed takes smooth wet sand (most of the gravel grain flattened to the set's mean, rescaled to dry sand's colour by `DRY_SAND_OVER_SEABED`, which hex-play's `dry_sand_over_seabed_matches_the_sets_and_tints` holds to the PNGs and the manifest tints) and darkens like the sand, fading back to its own gravel by 0.3 m deep, where the band leaves it untouched. Shader and manifest only, no rebuild.
  - **Drying trail** (water card criterion 3, 2026-10-20). The run-up covers its own soaked line, so on its own the live band showed only as the static damp gradient above the water. On every tier but Low (`FLAG_WET_TRAIL`, set by `water::tick` while the water refracts) the kit band also evaluates the band's reach 0.6, 1.2 and 1.8 s back on the same clock, each weighted by `exp(-age / 1.1 s)`: sand the wash has just left stays soaked (albedo a further x0.8, roughness toward 0.09) and dries back to the damp band as the wash retreats. Low keeps the band without the trail. The placeholder bed (`water_terrain.wesl`) keeps the band without the trail: it draws only where a kit base is missing. No texture is added. The native review flag `--water_look trail=<x>` (`docs/agent-operability.md`) replaces the 0.8 for option captures only.
  - This leans on the ±0.03 m tolerance below: 0.03 m of height error moves the band's swash phase by about 0.04 rad.
  - Below the waterline the bed takes `beach_seabed`, the tinted `gravel` set exactly as `ocean_base`, so the colour carries on across every ocean seam. The masks map carries the band as wetness (G = 1.0 on the band, 0.7 on dry sand and grass, 0 under water) and no snow under water (R = 0), for the §9.4 hooks.
  - Pieces: `scripts/blender/kit/_beach.py` (`WET_TOP_Y`, `slot_of`) and the slot tints in `assets/kit/manifest.json`.

## 3a. River mouth (`river_mouth`, `river_mouth_l`)

Owner request 2026-10-14 (`assets-source/refs/river_mouth/PLAN.md`, approved with Q1-Q6). The straight beach with one land edge turned into River: `river_mouth` has the River on edge 3, `river_mouth_l` on edge 4 (the mirror image across the coast's seaward axis, authored mirrored in Blender; the engine never mirrors). The network runs from the River edge to the node `mouth` at the tile centre. A River node on a tile with a coastline is a mouth; any other River node is a spring (§3b). No schema field and no socket rule changed: River meets only River, Beach only the opposite-sided Beach, Ocean only Ocean.

- **Coastline.** `Coastline::of` counts River as land beside a beach edge, so both mouths carry the straight beach's coastline and `s` of §3.
- **Bed** (`tiles.rs` `mouth_height`, `Mouth`; kit `scripts/blender/kit/_mouth_profile.py`, which asserts the same spot values): `y = min(coast_height(s), -1.4 + h)`, where `d` is the distance to the centre line, a ray from the River edge's lane point through the node and on out to sea, and `h` is the §2 carve with an s-dependent half-width and bowl:
  - `w(s) = 4.33 + 1.08 smoothstep(-8, 0, s)` (5.41 from the waterline on)
  - `D(s) = 1.55 - 1.19 smoothstep(-8, 3, s)`, the centre depth (0.36 m at the shelf joint, Q3)
  - `bed(d) = -(D - 0.05) max(0, 1 - (d/w)^2) - 0.05`, `bank = smoothstep(0.7 w, w + 2.5, d)`, `h = min(1.4, bed + (1.4 - bed) bank)`
  - Where `s <= -8` (the grass reach and the whole River edge) this is the §2 profile exactly, so the River edge matches `river_straight`. Past `s = 3` the coast is deeper, so the ocean and beach edges stay on the coast profile.
  - Spot values (y, `river_mouth`, runtime (x, z)): (-13.856, 0): -2.95; (-6, 0): -2.758; (0, 0): -1.977; (3.464, 0): -1.76 (s = 3); (8, 0): -3.735; (0, -8): -0.35 (the beach). The water is 8 m wide at s = -8, 8.8 m at s = -4 and 9.3 m at the waterline (the PLAN's 10 m assumed the full 1.55 m depth; the shallowing narrows it).
  - The channel and fan take the `river_bed` slot wherever they set the bed, except the fan's banks above the water outside its dark core (bed weight under 0.8, `_mouth.slot_of`): those take the coast's `beach_sand` / `beach_wet` by height. Inland, the banks beyond `river_straight`'s grass row (5.366 m from the centre line, where `river_straight` and `river_edge` put their grass edge) take `beach_grass`. That grass line is a constraint polyline in every LOD (`_mouth.grass_lines`): the row from the River edge's joint to the row s = -8, then along s = -8 to the crease, where the coast's own grass/sand line continues, with a 0.98 m fillet on the long bank. One slot rule holds on both sides of s = -8, so the hand-over has no notch (owner, 2026-10-14). COLOR_0 on these slots blends one colour field from the bed into the sand and the seabed, and its alpha fades the plaster's live wet band out before the gravel starts (`_mouth.fan_color`, `kit_material.wesl`; owner, 2026-10-14). The same alpha fades the tiled normal relief, in colour and in the normal prepass SSAO reads (`kit_common::detail_normal`, `kit_prepass.wesl`; 2026-10-21, `docs/checkpoints/mouth-fan-ssao/`). Tolerances as §2 and §3.
- **Surface.** One coast hex surface at -1.4. River weight and flow come from the centre line with `w(s)` and fade out over `s` from -3 to 0, handing over to the swash. No river strip on the mouth tile; the neighbour's strip ends at the River edge, whose 0.5 m vertices it shares with the hex grid.
- **Foam band.** Where the river meets the surf: weight `(1 - smoothstep(0, 1, |s|)) (1 - smoothstep(0.75 w, w + 1, d))`, carried in the surface's UV_0.y (`water.rs` `MEET_TAG`, 0 on every other tile) and added to the foam in `water.wesl`, surging with the swash. No texture.
- **Shore distance.** The mouth reports `s`; river tiles report `INLAND`. They differ along the River edge, where the river weight is 1 over all water, so the swash (weighted by 1 - river) does not act there.
- **Fish.** The River edge is wet and continuous, so the region flood fill joins a river chain to the sea through a mouth (Q5), and the joined region counts as sea water (not `river`). The fish band needs about 0.7 m of water (bed clearance 0.45 m, surface clearance 0.25 m, plus the wave bound where the river weight fades). Measured along the centre line (`tiles.rs` `print_fish_room_along_the_mouth`, the baked `FishField` and the analytic band agree within 0.02 m): the room falls below `SPAWN_ROOM` (0.3 m) from s = -3.2, closes (below 0) from s = -1.17 to s = 4.37 with a minimum of -0.36 m, and is about -0.3 m at the s = 3 shelf joint. So the region was one, but fish did not swim across the fan. The owner chose the relaxed band (2026-10-14; the fan is not deepened): over a mouth tile the band skims the surface where the water is shallow (the trout's fin and belly each 2 cm clear, blending back to the usual band between 0.8 and 1.2 m of water) and open skim water holds the river channel's 0.85 m of room. The same measurement now finds the band open the whole way, room never under 0.85 m (0.83 m on `river_mouth_l`'s line, `the_fish_band_is_open_along_both_mouth_centre_lines`), and 0.024 m of band (0.192 to 0.216 m under the surface) at the 0.36 m shelf joint. Fish spawned in the river and spring water of a joined region keep their singles and small groups; the sea-spawned fish school.

## 3b. River spring (`river_spring`)

River on edge 0 only, the network from edge 0 to the node `spring` at (-2, 0) (Q6).

- **Bed** (`tiles.rs` `pool_height`): `y = min(river_height(d), -1.4 + carve(1.4, r, 5.95))`, with `d` the distance to the stream's centre line (node to edge 0) and `r` the distance to the node: the §2 channel joined to a round bowl, -2.95 at the node and meadow height from 8.45 m (about 3.4 m of meadow to edge 3). The edge-0 ring is `river_straight`'s.
- **Surface.** One hex-grid surface at -1.4, kept within 6.5 m of the node and 5.5 m of the stream (a disc and a strip would overlap and z-fight). River weight 1 in the pool. The flow follows the stream and grows from 0 at the node over 6.5 m.

## 4. Ocean (`ocean`)

- Flat bed at y = -4.0 along the whole border ring. Interior relief (sand ripples) may rise at most 0.10 m and must fall to 0 within 2 m of the border, as the river bases' meadow does. The bed never goes below the base.
- **Cut faces.** On an exposed edge the engine draws the water volume's cut face from the animated surface down to the bed: depth absorption and tint, plus the bright underside of the surface at the top (PRD §9.1). With the bed at the base, the face reaches the tile base. The kit's column skirt (the O4 cutaway soil set) covers only what lies below the bed, which is nothing for the ocean.

## 5. What the engine draws and what the kit supplies

| | Engine (`water/tiles.rs`) | Kit piece |
|---|---|---|
| Water surface | Ocean, beach and river mouth: a hex surface at y = -1.4 (0.5 m grid). River: a strip 10 m wide along each river segment. Spring: the hex grid over the pool and stream. All waves, flow, foam, refraction and caustics in world space. River churn foam (2026-10-20): each river vertex carries the flow's advective acceleration `|(u · ∇) u|` (`TileField::churn`, minus it in UV_0.y; the mouth foam keeps the positive range), 0 on straights, about 0.08 m/s² round `river_bend`, 0.21 in `river_bend_60` and the fork, faded to 0 over the last 3 m inside the hex border so every river seam reads 0 on both sides; `water.wesl` adds white water from it, broken up by the ripples' flow-advected noise (drifting noise on Low) | none |
| Cut faces | On each water edge with no matching water across it (§6), from the surface to the bed, clipped where the bed rises above the water | none |
| Bed | Placeholder only, while the tile's base piece is missing from the kit. A missing or still-loading centre piece does not bring it back | All of it, in the base: banks, beach slope, shelf, sea floor and the column skirt from the bed rim down to -4.0 |

Bed requirements:
- **Opaque and in the depth prepass.** Water is not in the prepass and reads the prepass depth as its bed (refraction, absorption, foam, caustics). A bed that skips the prepass breaks all of these.
- **`KitMaterial` only, with no new sampled texture.** Next to refracting water the kit forward pipeline samples exactly 16 textures, WebGPU's baseline (ADR 0002, smoke case `water_kit`), so there is no headroom.
- **Every water tile's base is its bed.** The river tiles name their channel bases (`river_straight_base`, `river_bend_base`, `river_bend_60_base`, `river_spring_base`, `river_fork_base`, `river_mouth_base`, `river_mouth_l_base`), the coast tiles their beach bases and the ocean `ocean_base`. No land tile shares one (`water/tiles.rs` `the_stand_in_bed_is_drawn_only_where_the_base_is_missing`). A water tile naming `meadow_base` would bury its water under the meadow cap, whose relief covers the whole hex at y in [-0.25, 0].
- **When the engine draws the placeholder bed.** Only while the tile's base is missing from the kit (`water::tiles::stand_in_bed`): a base outside `assets/kit/manifest.json`, or one that still failed after the `kit://` retries (`hexWorld.state().assets.failed`). Otherwise the base comes from the kit like any other tile, and the centre once it has loaded. While the centre is missing the tile draws no river strips and no hub over the channel, only the engine's water surface. Road strips stay as the bridge deck. Owner 2026-10-14: when `river_spring_center` failed to load, the old rule (keyed on the centre) drew the stand-in over the spring's kit base, which read as a pale hex.
- **What decides it is water.** A tile with a River network is a river tile: a river mouth if it also has a coastline (§3a), a spring if its network ends at a node (§3b). A tile with Ocean on every edge is ocean. A tile with some Ocean edges is a coast when its two beach edges form one of the three coastlines of §3. Only these tiles get water surfaces and cut faces.

## 6. Seam rules

Each edge's bed profile depends only on its socket, so it matches any legal neighbour under any rotation:

| Socket | Profile along the edge |
|---|---|
| Grass, Forest | y = 0 (the `meadow_base` border ring, 16 segments per edge) |
| River (lane) | `y = -1.4 + h(|o|)`, where `o` is the offset from the lane centre along the edge. Symmetric about the midpoint. Ring vertices sit at offsets symmetric about it, at most 0.5 m apart within 6.83 m |
| Ocean | y = -4.0 along the whole edge |
| Beach | The coast profile (§3) at `s` = the distance from the edge midpoint toward the sea corner: -8 at the land corner (the side the socket's `land` names), +8 at the sea corner. The same on all three coast tiles. Kit ring vertices: the 16-per-edge lattice (integer s) plus s = -1.953 (the top of the `beach_wet` slot), 3.5, 4.5, 5.5, 6.5 and 7.5 (the drop), identical on every coast base (`kit_gate` `check_coast_seams`) |

- The engine treats an edge as continuous water when River meets River, Ocean meets Ocean, or Beach meets Beach with their seaward corners on the same point. Anywhere else it draws a cut face on each wet side.
- Since owner O1 the sockets refuse **Beach against Ocean**, **Beach against Grass or Forest** and **mirrored Beach against Beach** (the same land side on both tiles, so the land end meets the sea end). Every legal beach seam continues the coast with matching beds. If a mismatched seam is ever loaded anyway, the engine still draws cut faces on it.
- River flow runs one way along each connected chain: from its spring, else from its open end with the lowest `(cell, edge)`, else from its mouth (a chain of mouths alone). Two springs or two mouths (Q4) start at the lower cell; the other one then ends the flow. A chain with a spring never reverses when tiles are placed at its open end; one without can. Kit pieces carry no flow direction.
- **Junctions** (`river_fork`, §2a; `water::tiles::junction_sources`). The chains that meet at junctions form one network, and every chain in it runs downhill, measured in metres along the centre lines: toward the nearest mouth when the network reaches one (a fork with one arm toward the sea is a confluence; a fork whose two arms each reach a mouth first is a split); else away from the nearest spring (one spring through a fork splits, two springs join); else out through the lowest junction's local edge 0 arm, as if the sea lay past its open end (a lone fork is a confluence, and its rotation picks the outlet). A chain whose two ends are equally far keeps the chain rule above.
