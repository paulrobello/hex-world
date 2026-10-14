# River mouth and spring: plan for the reference gate

**Status:** awaiting owner approval, together with `river_mouth/ref-1..3` and `river_spring/ref-1..3` (sheets: `river_mouth/sheet.jpg`, `river_spring/sheet.jpg`). Nothing is modeled yet. Owner request, 2026-10-14: a river must be able to reach the sea and to start inside land.

**The refs set the look, not the geometry.** Both top-down slots (ref-2) came back pointy-topped, with the river leaving at a corner rather than at an edge midpoint. The pitfalls note says the model does not hold edge geometry. The socket tables below define the layout. `river_spring/ref-3` frames the pool on a dark tray, which is not the O4 skirt. `river_mouth/ref-3` reads as a hand-built resin model (static grass, fibre surf), so take only the layout of the fan and the foam from it, not its materials.

Frame: runtime metres, tile-local, edge 0 faces +x, edges run clockwise viewed from +Y, and edge k is opposite edge k+3 (`docs/kit/water-contract.md`).

## 1. Sockets

| Tile | Edges 0-5 | Network | Node |
|---|---|---|---|
| `river_mouth` (biome Beach) | Ocean, Ocean, Beach(Right), **River**, Grass, Beach(Left) | `(kind: River, from: Edge(3), to: Node("mouth"))` | `(id: "mouth", at: (0.0, 0.0))`, the waterline at the tile centre |
| `river_spring` (biome Grass) | **River**, Grass, Grass, Grass, Grass, Grass | `(kind: River, from: Edge(0), to: Node("spring"))` | `(id: "spring", at: (-2.0, 0.0))`, the pool centre |

**Why edge 3 on the mouth.** `river_mouth` is `beach.ron` with one grass edge turned into River. The sea is on edges 0 and 1. The two land edges, 3 and 4, are directly opposite them (3 faces 0, 4 faces 1). A straight centre line from either one runs through the tile centre and points out through the opposite ocean edge. The straight coastline also passes through the centre (s = 0), so the river crosses 4.6 m of grass and 9.2 m of sand and reaches the waterline at the centre. Edge 3 is chosen because its centre line is `river_straight`'s own line (Edge(0) to Edge(3), the x axis) at rotation 0, so the channel and banks come from `_river.py` unrotated. The centre line meets the coastline at 60 degrees, not 90, so the fan opens a little skewed toward ocean edge 0. Edge 4 would give the mirror image. Placement only rotates, so this tile has one handedness (Q1).

**Seams need no rule change.** River meets only River, Beach meets only the opposite-sided Beach, and Ocean meets only Ocean. The corner shared by River edge 3 and Beach edge 2 sits at y = 0 on both rings: the river profile is at meadow height beyond 6.83 m from the lane centre, and the beach's land corner is at s = -8. `data/sockets.ron` stays unchanged.

**The loader already accepts both tiles.** `TileDef::validate` (`crates/hex-core/src/tile.rs`) checks only that each Beach edge carries a land side, not what kind of edge sits beside it. It allows River networks that end at a declared `Node`, and it requires exactly one endpoint on each lane edge, which both tiles have. Beach and Ocean edges carry no lane. `hex-sim` `network.rs` resolves Node endpoints generically, so the river graph gets a dead-end node with no destination. The validator refuses `to: Edge(0)` on an Ocean edge ("ends on edge 0 Ocean, which has no lane"), so the mouth must end at a node.

**Spring or mouth is inferred, not declared.** A River node on a tile with a coastline is a mouth (a sink). Any other River node is a spring (a source). This needs no schema field.

**Engine changes** (all in `crates/hex-render/src/water/tiles.rs`):
- `Coastline::of` counts only Grass or Forest as land beside the beach edges, so it returns `None` for the mouth. It must count River as land too.
- `water_kind` returns `River` for any tile with a River network, so the mouth would lose its coast surface and bed. It needs a combined kind, for example `RiverMouth(Coastline)`.
- `river_segments` handles only `(Edge, Edge)` and drops Node segments (`_ => None`), so neither new tile would draw any river water.

## 2. Kit pieces

Every slot reuses an existing tileable set (grass, plaster, gravel, soil) or is colour-only. No new sampled texture is added, so the 16-texture limit next to refracting water still holds (ADR 0002, smoke case `water_kit`). Each piece has one masks map, as its siblings do.

| Piece | Kind | Derives from | Slots | Tris LOD0/1/2 (siblings) |
|---|---|---|---|---|
| `river_mouth_base` | base (the bed) | `beach_base` coast profile (`_beach.py`) with the `river_straight_base` channel (`_river.py`) carved in and fanned | `beach_grass`, `beach_sand`, `beach_wet`, `beach_seabed`, `beach_skirt`, `river_bed` | ≤ 3,600 / 1,500 / 400 (beach_base 3,242 / 1,306 / 256; river_straight_base 3,124 / 1,332 / 390). LOD2 needs more than beach_base: `river_bend_base` LOD2 failed the ±0.03 m tolerance at first. |
| `river_mouth_center` | prop | `beach_center` (dune grass, driftwood, pebbles) on the two sand shoulders, and `river_straight_center` bank stones and reeds on the grass reach | `beach_dunegrass`, `beach_driftwood`, `beach_pebble`, `river_stone`, `river_reed` | ≤ 860 / 388 / 120 (both siblings 860 at LOD0) |
| `river_spring_base` | base (the bed) | `meadow_base` cap, with the `_river.py` carve for the stream and a round pool bowl | `river_grass`, `river_bed`, `river_skirt` | ≤ 3,124 / 1,332 / 390 (river_straight_base) |
| `river_spring_center` | prop | `river_straight_center` stones and reeds, plus a ring of lichen granite from the approved `meadow_props` rocks | `river_stone`, `river_reed`, optional colour-only `river_lily` | ≤ 860 / 320 / 120 (river_straight_center) |

- **Edge pieces.** No new ones. `river_edge` is placed on each River edge through `edge_pieces`, as on every river tile. Beach and Ocean edges have none.
- **Base and centre land in the same commit.** Each centre piece must reach the manifest in the same commit as the tile file that names its base (water contract §5). Otherwise the placeholder bed swaps out under a base that is not the bed.

## 3. Water (changes to `docs/kit/water-contract.md`)

**Mouth bed.** The border ring sits exactly on its socket profile: river `h(|o|)` on edge 3, the coast lattice on edges 2 and 5, y = 0 on edge 4, and -4.0 on edges 0 and 1. Inside the tile the bed is the coast profile, lowered by the channel:

`y = min(coast_height(s), -1.4 + h_s(d))`

- `h_s` is the contract's §2 river profile `h(d)` with two values that depend on s: the carve parameter `w` and the bowl depth.
- Where s ≤ -8 (the grass reach, which includes the whole edge-3 ring), `h_s = h` exactly, with `w = 4.33` and 1.55 m of depth. The edge-3 ring therefore matches `river_straight`.
- Across the sand, `w` grows to 5.41 at the waterline, which is a 5.0 m water half-width times 1.0825. That gives the 10 m mouth from the "Width evidence" section of the water contract (the look-dev river widens from 6 m to 10 m).
- Over the same stretch the depth shallows smoothly to the shelf depth, 0.36 m at s = 3. The channel then runs out onto the shelf instead of cutting a trench into the sea.
- Wherever the channel and fan bed sit below the coast profile, they take the `river_bed` slot, not `beach_wet`. The wet-band shader reads `s` back from the bed height (`coast_shore`), and that is wrong inside the channel.
- The fan's top edge stays at least 1 m inside beach edges 2 and 5, so their rings stay exact.
- Tolerances are as in §2 and §3 of the contract: ±0.03 m where the water is shallower than 2.5 m.

**Mouth surface.** Draw one surface on the mouth tile: the coast hex surface at -1.4, with river weight and flow added as per-vertex inputs from the centre line. Do not add a river strip. A strip at -1.4 would overlap the coast surface in the channel and z-fight. The neighbour's strip still ends at the edge-3 seam. River weight and flow fade out over s from -3 to 0, handing over to the swash.

**Foam where the river meets the surf.** Add a foam band across the fan's sea end (s from -1 to +1, inside the fan width). It is driven by flow speed against the swash, through the existing foam inputs in `water.wesl`, with no texture.

**Spring pool.** The pool has 5.5 m water radius (11 m across). Its bowl is the river carve on the distance to the node, with `w = 5.95`, the same 4.33/4.0 ratio as the channel. It reaches -2.95 at the centre and meadow height at 8.45 m from the node (w + 2.5), which leaves about 3.4 m of meadow to edge 3. The water is a disc surface at -1.4 of radius 6.5 m, joined to the stream's 10 m strip. Flow starts at 0 at the node and grows outward toward the stream. A slow ring ripple can mark the welling point.

**Flow direction.** `oriented_rivers` starts each chain at its open end with the lowest (cell, edge). The new rule:
- A chain flows away from a spring and into a mouth, and from spring to mouth when it has both.
- A chain with one spring or one mouth and one open end flows from the spring, or toward the mouth.
- Chains with two springs or two mouths are Q4.

**Cut faces.** These follow the sockets as today: River to River, Ocean to Ocean, and Beach to Beach with matching seaward corners. `edge_water` must also accept the mouth's new kind wherever it now requires `WaterKind::Coast`.

**Fish.** `wet()` in `boids/plan.rs` already treats River-lane, Beach and Ocean edges as wet. Once `tiles::layout` includes the mouth cell, the region flood fill joins the river chain to the sea. The fan's minimum depth (0.36 m at the shelf joint) is above `SPAWN_ROOM` (0.3 m). Check it against the baked `FishField` so that fish can pass through the mouth. The spring pool joins its stream's region.

## 4. Open questions for the owner

1. **Should there be a mirrored mouth, with the river on edge 4?** Recommendation: not now. Ship one handedness, and add `river_mouth_l` later only if play shows friction. It would be generated mirrored in Blender, because the engine never mirrors.

Here is what one handedness allows. Take a land cell L behind a straight coast, in the coast's frame:
- L's edges 0 and 1 face the coast, and its edges 2 and 5 face the next cells along the land row.
- L can feed a mouth only through its edge 0, into the coast cell whose edge 3 faces it. Every land cell behind the coast has exactly one such coast cell, so a mouth always fits in front of L.
- The river must leave L through edge 0. It can arrive from edge 3 (straight) or from edge 2 or 4 (a 120-degree `river_bend`).
- It cannot arrive from edge 5, because there is no 60-degree river bend.

So a river that runs inland toward the coast always reaches the sea. A river that runs along the land row can turn in only when it travels from the edge-2 side. Travelling the other way, it needs a loop of a few cells inland first. The mirrored tile would remove that detour.
2. **Should there be mouths on the headland and the bay (`beach_outer`, `beach_inner`)?** Recommendation: no. Only the straight beach has a land edge opposite the sea and a coastline through the centre.
3. **Should the channel shallow to the shelf at the mouth?** Recommendation: yes. It is 1.55 m deep inland and 0.36 m at s = 3, as in ref-1 and ref-3, rather than a deep cut into the sea. This number may move at the look check.
4. **What should chains with two springs or two mouths do?** Recommendation: allow them, and keep the current deterministic rule (the flow starts at the lowest cell). Refusing them would need a whole-map placement rule, which the socket system does not have.
5. **Which fish live in the joined region?** Recommendation: one connected region. Each fish takes the palette of the water it spawns in: the river palette (fish ref-3, lower fish) on river and spring cells, and the ocean palette on coast and ocean cells. Mixing at the mouth is acceptable.
6. **What size and where should the spring pool be?** Recommendation: 11 m of water centred 2 m behind the tile centre, which leaves about 10 m of stream before the edge. Add reeds, a granite ring and a few lily pads (one colour-only slot).
