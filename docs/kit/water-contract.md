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
| Meadow cap border ring | 0.0 | `meadow_base.py`: relief in [-0.25, 0], 0 within 2 m of the border |
| Water surface at rest | **-1.4** *(may move)* | `water.rs` `LAND = 1.4`: the approved look-dev puts the grass plateau 1.4 m above the water. The terrain shader's grass line (0.82-1.08 m above the water) and wet band (0.32-0.40 m) are tuned to it. -1.4 is 0.35 `LEVEL_STEP` |
| River bed, centre line | -2.95 | 1.55 m below the water (`carve`: 1.5 m bowl + 0.05 m) |
| Ocean bed | **-4.0** *(may move)* | The tile base. 2.6 m of water: Gerstner waves reach full height at 2.5 m depth (`wave_atten`) and the swash stops there (`swash_weight`), so ocean seams carry no shore-dependent motion |
| Tile base (column floor) | -4.0 | `-LEVEL_STEP`, the bottom of every column (`meadow_base` skirt) |

The prototype's open sea reaches 6 m deep and reads deeper blue than 2.6 m will. A deeper ocean needs a deeper ocean column than the -4 m base, which is the owner's call. This is the main number that may move.

## 2. River channel (`river_straight`, `river_bend`, `river_road_bridge`)

- **Width.** The lane is 8 m (`data/sockets.ron`) and is the water width at rest: the bank crosses y = -1.4 at 4.0 m from the centre line.
- **Profile.** The look-dev's `carve` with half-width parameter `w = 4.33`. With `d` the distance to the centre line, relative to the water surface:
  - `bed(d) = -1.5 * max(0, 1 - (d/w)^2) - 0.05`
  - `bank(d) = smoothstep(0.7 w, w + 2.5, d)`, from 3.03 m to 6.83 m
  - `h(d) = min(1.4, bed + (1.4 - bed) * bank)`, so `y = -1.4 + h(d)`
  - Spot values (y): d = 0: -2.95; d = 3: -2.23; d = 4: -1.40 (waterline); d = 5: -0.69; d >= 6.83: 0.0 (meadow level).
  - The channel is 13.66 m wide at the top.
- **Centre line.** From the lane point on the `from` edge to the lane point on the `to` edge (edge midpoint plus the lane offset, 0 for every MVP river). With no control point it is straight. With one point it is the circle through the three points: for `river_bend`, the 24 m radius arc centred on the edge-1 neighbour's centre `(13.856, 24)`, as `hex_sim::geom::segment_shape` reads it. Either way the centre line meets each lane edge at a right angle.
- **Bank tuck.** The engine's water strip runs 5 m either side of the centre line, so at `d = 5` the bank must stand at least 0.5 m above the water. The profile gives 0.71 m.
- **Width evidence.** The approved prototype river is 6 m wide (half-width 3), widening to 10 m at the mouth. 8 m sits inside that range, the road (6 m) bridges it, and the 13.66 m channel leaves at least 7 m of meadow to the tile corners, so **keep 8 m**. No change to `data/` is proposed.

## 3. Coast (`beach`)

- **Seaward axis.** The unit sum of the ocean edges' normals minus the grass edges' normals: 30 degrees for `beach` (ocean on 0-1, grass on 3-4). `s` is the distance along it from the tile centre. The ocean edges lie at s in [8, 16], the grass edges at s in [-16, -8], and the beach edges 2 and 5 run from s = -8 (landward corner) to s = +8 (seaward corner).
- **Profile** (y as a function of s):
  - `s <= -8`: 0.0 (grass level)
  - `-8 < s < 0`: `-1.4 + 1.4 t (2 - t)` with `t = s / -8`. This is the look-dev beach curve over 8 m instead of 12, and it meets the grass with zero slope (0.35 m/m at the waterline).
  - `s = 0`: -1.4, the waterline through the tile centre, along the edge-2/edge-5 axis
  - `0 <= s <= 3`: the shelf, -0.12 m/m (the look-dev offshore slope) to -1.76
  - `3 < s < 8`: drop to the ocean bed, `-1.76 - 2.24 * smoothstep(3, 8, s)`
  - `s >= 8`: -4.0
  - The beach width (8 m) and shelf length (3 m) *may move* with the look check.
- **Wet band.** The placeholder bed's shader darkens and glosses sand up to 0.32-0.40 m above the water (y -1.4 to about -1.0) and follows the live swash run-up: `0.22 * swash`, from -0.09 to +0.22 m, only where the water is shallower than 2.5 m. A kit beach on `KitMaterial` does not get the live run-up line. That is open: either a static wet band baked into the kit beach's albedo and roughness up to y = -1.0, or a later hook.

## 4. Ocean (`ocean`)

- Flat bed at y = -4.0 along the whole border ring. Interior relief (sand ripples) may rise at most 0.10 m and must fall to 0 within 2 m of the border, as in `meadow_base`. The bed never goes below the base.
- **Cut faces.** On an exposed edge the engine draws the water volume's cut face from the animated surface down to the bed: depth absorption and tint, plus the bright underside of the surface at the top (PRD §9.1). With the bed at the base, the face reaches the tile base. The kit's column skirt (the O4 cutaway soil set) covers only what lies below the bed, which is nothing for the ocean.

## 5. What the engine draws and what the kit supplies

| | Engine (`water/tiles.rs`) | Kit piece |
|---|---|---|
| Water surface | Ocean and beach: a hex surface at y = -1.4 (0.5 m grid). River: a strip 10 m wide along each river segment. All waves, flow, foam, refraction and caustics in world space | none |
| Cut faces | On each water edge with no matching water across it (§6), from the surface to the bed, clipped where the bed rises above the water | none |
| Bed | Placeholder only, while the tile's centre piece is missing from the kit | All of it: banks, beach slope, shelf, sea floor and the column skirt from the bed rim down to -4.0 |

Bed requirements:
- **Opaque and in the depth prepass.** Water is not in the prepass and reads the prepass depth as its bed (refraction, absorption, foam, caustics). A bed that skips the prepass breaks all of these.
- **`KitMaterial` only, with no new sampled texture.** Next to refracting water the kit forward pipeline samples exactly 16 textures, WebGPU's baseline (ADR 0002, smoke case `water_kit`), so there is no headroom.
- **River tiles need a channel base.** River tiles name `meadow_base` as their base. Its cap covers the whole hex at y in [-0.25, 0] and would bury the channel. The river family must ship a base with the channel cut (or a centre piece that is the whole cap) and update `base` in the river tiles. Until then the engine skips the base and draws the placeholder bed.
- **When the engine switches to the kit.** It stops drawing the placeholder bed once the tile's centre piece is in `assets/kit/manifest.json`, and from then on it draws `base` and the centre from the kit like any other tile. If `river_*_center` reaches the manifest while the river tiles still name `meadow_base`, the meadow cap buries the channel. The manifest entry and the `base` change in `data/tiles/river_*.ron` must land in the same commit. The same holds for `beach_center` with `beach_base` and `ocean_center` with `ocean_base`: a base that is not the bed covers the water.
- **What decides it is water.** A tile with a River network is a river tile. A tile with Ocean on every edge is ocean. A tile with some Ocean edges is a coast. Only these tiles get water surfaces and cut faces.

## 6. Seam rules

Each edge's bed profile depends only on its socket, so it matches any legal neighbour under any rotation:

| Socket | Profile along the edge |
|---|---|
| Grass, Forest | y = 0 (the `meadow_base` border ring, 16 segments per edge) |
| River (lane) | `y = -1.4 + h(|o|)`, where `o` is the offset from the lane centre along the edge. Symmetric about the midpoint. Ring vertices sit at offsets symmetric about it, at most 0.5 m apart within 6.83 m |
| Ocean | y = -4.0 along the whole edge |
| Beach | The coast profile with s running from -8 at the landward corner to +8 at the seaward corner |

- The engine treats an edge as continuous water when River meets River, Ocean meets Ocean, or Beach meets Beach with their seaward corners on the same point. Anywhere else it draws a cut face on each wet side.
- The sockets also allow **Beach against Ocean** (a beach's edge 2 or 5 against an ocean tile) and **Beach against Beach mirrored** (seaward end against landward end). Their beds step at the seam, so the engine draws cut faces there. A directional beach socket (Beach-seaward-left / -right) would make both illegal. That is a `data/sockets.ron` change for the core stream, recommended but not made here.
- River flow runs one way along each connected chain, starting from its open end with the lowest `(cell, edge)`. Placing a tile at that end can reverse a whole chain's flow. Kit pieces carry no flow direction.
