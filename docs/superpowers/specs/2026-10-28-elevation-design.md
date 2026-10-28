# Elevation, first round (P4): design

Status: **draft, not approved.** Written during the 2026-10-28 brainstorm with Paul. Items marked *(pending)* are my guesses from the first-round playback, still waiting for his answer. Nothing here is built until he approves this document.

## Intent

Paul wants height in the world: "hills with roadways around and through them (tunnels), even elevated versions of existing tiles like Meadow", joined by "a nice slope to connect elevated tile to lower tile". The world is still built one hex tile at a time, and raised ground should feel like part of the same miniature landscape, not a separate mode. This first round proves the whole chain (levels, slopes, a raised tile, a road that climbs, a tunnel) on one tile type, Meadow, before other tiles get raised versions.

## Goals

1. Tiles sit at a level. A raised Meadow is today's Meadow lifted, with its full flat top, grass, wildflowers and scatter.
2. Ground changes level only through a **hillside tile** that slopes across its whole width, about 14% (one level over the 27.7 m flat-to-flat), the same grade as a road ramp (owner A39 O1).
3. Hillsides handle **any outline** of raised ground, bays and notches included (owner, "C"): a hillside may sit against raised ground on one to five of its sides.
4. Roads climb on hillsides (road ramps) and run on raised ground *(pending)*.
5. **Road tunnels**: an arched portal at each end, like a train tunnel, able to run underground beneath a raised area several tiles wide. Cars drive through and vanish inside.
6. Saves, undo/redo, thumbnails, the UI rules mirror and the bridge all carry levels.

## Non-goals (this round)

- Raised versions of forest, oak, farm, wheat field, houses, bakery and windmill (later rounds, owner).
- Anything other than raised Meadow above a tunnel (deferred, owner).
- Raised rivers, river flow direction and waterfalls (A41) *(pending: deferred because no river starts on high ground yet)*.
- Rail on raised ground, viaducts and rail tunnels (A39) *(pending)*.
- Tunnel lamps lighting up at night: they need the P5 day/night cycle, which is held. The portal carries an inert emissive hook (the `bush_verge_c` blossom pattern, `docs/kit/hooks-contract.md`).
- Single-tile peaks and mountains (A40) beyond what a raised Meadow plateau gives, and massifs (mega-tile card).
- Ocean and beaches above level 0 *(pending)*.

## Anti-goals

- A sheer step between levels where a slope was wanted. Paul asked for a slope, and the hillside tile is that slope.
- A palette full of hillside corner variants the player has to hunt through.
- Scatter or grass baked into hillside or raised pieces (owner 2026-10-26: scatter is runtime, never baked).
- Breaking existing saves or thumbnails: every flat world loads and looks exactly as today.

## Decided (owner)

- A38 O1: a tile takes the level its neighbours imply, the way placement already picks a rotation; R/scroll cycles the legal levels when more than one fits. Tile definitions keep edge heights relative to the tile's own level; a placed tile stores one absolute level, so per-(tile, rotation) thumbnails stay valid.
- A39 O1: a road ramp climbs one level (4 m) within one tile. Rail never changes level.
- A42 O1: organic P4 pieces are headless procedural Blender generators.
- 2026-10-28: hillside tile between levels; any outline; road tunnels with arched portals, several tiles long, lit at night once P5 lands; raised Meadow first and only Meadow over a tunnel. All recorded in PRD §15 P4.

## How it works

### Levels

- Level `L` puts a tile's flat top `4 m × L` above level 0 *(pending: levels 0 to 3, up to 12 m)*. The save format already reserves a per-tile `level` byte (`crates/hex-core/src/save.rs`, `MAX_LEVEL = 0` today); this round raises `MAX_LEVEL` and does not bump `SAVE_FORMAT_VERSION`. Old saves load with every tile at level 0.
- `EdgeSocket { height, profile }` already exists (`crates/hex-core/src/socket.rs`) with `Flat`, `Cliff` and `Ramp` profiles and `heights_compatible`. Heights in tile definitions stay relative; placement adds the placed level.

### Where a raised tile meets a lower one

- Through a hillside: the normal case.
- Directly *(pending)*: allowed at any difference, and the shared edge shows as a cliff, the existing column skirt (grass lip over dark topsoil, PRD §6 Bases) extended down to the lower tile's top. Cliffs stay possible because waterfalls (A41) will need them.
- *My call:* placement never picks a cliff by default. A tile's default level is the one that makes no cliff with its neighbours; R/scroll cycles to the others. A raised area starts either by cycling a Meadow up a level (cliff-edged until hillsides fill in) or by placing a hillside whose low side touches the world, with R/scroll cycling its shape while its high sides face empty cells.

### Hillside tile

- One **Hillside** entry in the palette. On placement it takes its shape from its neighbours: which of its six sides face ground one level up. There are about 12 shapes up to rotation (every non-empty, non-full set of raised sides); placement picks shape and rotation together, the way it already picks a rotation.
- *My call:* a hillside keeps its shape once placed, like every tile today. A later placement that contradicts it (raised ground against a side it committed as low) is refused like any socket mismatch; to change it, bulldoze and re-place. So each shape is stored as one tile definition with a rotation, and undo and the UI rules mirror stay one tile per action.
- Its low sides sit at the hillside's own level, its high sides one level up. Sides that face neither (a hillside beside another hillside) slope along their length so neighbouring hillsides join without a seam.
- Ground: Meadow grass and soil, with Meadow's runtime scatter and grass blades following the slope.
- *(pending)* The hillside ground surface is generated at runtime from the shape (a deformed hex ground mesh using the Meadow ground material), not authored as 12 Blender pieces, so grass and scatter flow over it and new shapes cost no bake.

### Road ramp and raised road

- A road ramp is a hillside with a road on it: the road runs from a low side up to the opposite high side. *My call:* it is its own palette entry, **Road ramp**, using only the straight-bank shape (raised and low sides opposite), and placement picks its rotation from the road and the ground around it. Its surface follows the hillside slope at about 14%.
- *(pending)* Straight road, bends and junctions get raised versions so a ramp leads somewhere.

### Road tunnel

- *My call:* a **Tunnel portal** is its own palette entry: a straight-bank hillside with a road running into an arch in its raised face. A tunnel forms when two portals face each other across raised Meadow, *(pending: in a straight line)*, so the route is implied by the portals alone and the save format needs no new data. A lone portal may be placed while building; it shows as an unfinished tunnel and cars do not route into it.
- *My call:* bulldozing a raised Meadow above a finished tunnel, or the ground behind a portal, is refused while the tunnel exists; bulldoze a portal first.
- While placing, the hidden stretch shows as a faint dotted line. Cars enter the portal, are hidden underground, and come out the far portal on the route the road network already computes.
- Clearance: the road runs at the lower level, so 4 m of ground sits over it. The portal arch must fit inside one level.

## Assets

1. **Road-tunnel portal** (Blender, headless procedural per A42): a stone arch set into the hillside face, with a recessed dark interior, an inert emissive lamp hook, LODs, collider and sockets, through the `blender-kit-pieces` pipeline and owner approval. Rail can reuse its style later.
2. **Hillside ground** *(pending: runtime-generated)* using the existing Meadow ground material.
3. **Road on a slope**: the existing road surface laid on the hillside slope (runtime deformation of the road piece or a dedicated ramp piece; builder's call, below).
4. **Cliff face**: the existing column skirt, extended.
5. Thumbnails for Hillside shapes and raised road tiles (`make thumbs`). The data validator demands one per (tile, rotation).
6. Sound: none required. A tunnel echo on cars is optional, under the standing audio approval.

## Integration consequences (to check up front)

- **Cars**: `CAR_GROUND` road-surface heights must follow the ramp slope and the raised level; tunnel stretches hide cars and must not route through raised ground without portals.
- **Walkers**: `walk.rs` ground must follow levels and hillside slopes; cliffs are walls; walkers don't enter tunnels *(builder's call)*.
- **Scatter and grass**: sample the hillside surface height; footprints and `GROUND_BOXES` on slopes.
- **Boids and camera**: flight and camera height clearance over raised ground.
- **Water**: the sea and rivers stay at level 0; water skirts unchanged.
- **UI**: `web/ui/src/rules.ts` mirrors placement rules and is parity-tested (`web/ui/test/rules-parity.json`); level cycling (R/scroll), the ghost preview at level, and the tunnel's dotted line. Bridge enums change, so `make ui-schema`.
- **Saves and undo**: level per tile; tunnel state derived from the road network on load (agents are re-derived, never saved).
- **Perf**: raised tiles and cliff skirts add vertices; hillside grass and scatter count against the P2 grass budget on MiniMac.

## Left to the builder

- Exact hillside surface shape between sides (curvature profile, how sloping sides blend), as long as neighbours join without seams or cracks and the grade is about 14%.
- Whether the road ramp deforms the existing road piece or is its own piece.
- How the ghost and the R/scroll level cycle are shown.
- Tunnel interior visibility when the camera looks into a portal.
- Test plan, phases and card breakdown (writing-plans).
