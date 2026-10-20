# Hex World — Product Requirements Document

| Field | Value |
|---|---|
| Product | Hex World |
| Repo | `paulrobello/hex-world` (public, MIT) |
| Hosting | GitHub Pages, custom domain `hex-world.pardev.net` |
| Owner | Paul Robello |
| Status | Approved v1 (2026-10-12) |
| Date | 2026-10-12 |
| Last synced with main | 2026-10-14 (`9ddb35f`; drift report [`docs/review/2026-10-14-prd-drift.md`](docs/review/2026-10-14-prd-drift.md)) |

## 1. Summary

Hex World is a browser-based world-building game. The world starts as a single hex tile. The player picks tiles from an inventory, rotates them, and places them next to existing tiles where the edges are compatible. Each placed tile rises out of the ground in a staged assembly animation, and the world comes alive: cars drive the roads, trains cross tile boundaries, rivers flow, windmills turn, animals roam farms, and birds flock overhead.

The target is a "stylized-real diorama" at AAA visual quality: real PBR materials on slightly exaggerated miniature proportions. It is rendered with Bevy 0.20 (Rust, compiled to WebAssembly on WebGPU) extended with custom render passes, with a custom wgpu renderer as the fallback route (D2). Every 3D asset is authored high-poly in Blender, baked to PBR textures, and reduced to runtime LODs. Sound effects and ambience come from ElevenLabs. Music comes from Suno, using a prompt pack kept in the repo.

## 2. Goals and non-goals

### Goals

- G1. Placing a tile is satisfying on its own: legal-slot feedback, rotation, a staged rise animation, sound, and the tile coming alive.
- G2. A massive variety of tiles from a modular kit, so variety grows faster than modeling effort.
- G3. A world that feels alive: network-routed vehicles that cross tile boundaries, ambient animals and birds, animated props, and spatial ambience.
- G4. AAA-grade visuals at 60 fps on desktop with worlds of about 2,000 tiles.
- G5. A reproducible, gated asset pipeline (Blender high-poly → bake → LOD → compressed GLB) that subagents can run.
- G6. A zero-backend static deploy on GitHub Pages.

### Non-goals (for now)

- Multiplayer, accounts, cloud saves, or a world gallery.
- A resource economy, costs, or fail states.
- Browsers without WebGPU (they get an unsupported page; see D2).
- WASM threads / `SharedArrayBuffer` (see TC1).

## 3. Decisions

Decisions made during the brainstorm. Codes are stable and referenced throughout.

| Code | Decision |
|---|---|
| D1 | Core loop: sandbox + progression. No fail state. New tiles unlock through combo recipes, which arrive after the MVP. |
| D2 | Engine: Bevy `=0.20.0` (wgpu 30) → WASM, WebGPU required, extended with custom render-graph nodes for water, VAT creatures, compute boids and any post effect Bevy lacks on the web. No WebGL2 fallback, so compute shaders may be used freely. P0 engine gate outcome (2026-10-12): Bevy stays, conditionally. In Chrome the instanced shipping layout holds the frame budget with TAA, SSAO, bloom and DOF active, using a reduced shadow setting, and full shadows miss it. Safari 27.0.1 renders through `web/wgsl-compat.js`, a page shim that works around Bevy's PBR shaders depending on the WGSL extension `texture_and_sampler_let`, which Safari lacks ([`docs/perf/p0-safari-diagnosis.md`](docs/perf/p0-safari-diagnosis.md)). On the tuned instanced rows its median mean frame time is 16.67 / 16.69 ms (strategic / close-up), and strategic run 2 had 15 frames over 25 ms. The custom wgpu 30 renderer with standalone `bevy_ecs` remains the documented fallback. Decision record: [`docs/decisions/0001-engine.md`](docs/decisions/0001-engine.md). Research basis: §18. |
| D3 | Adjacency: typed edge sockets on all 6 edges, with a compatibility table. Rotation rotates the sockets. |
| D4 | Inventory: every unlocked tile type is unlimited. The panel filters or highlights tiles that are legal at the hovered slot. |
| D5 | Elevation: discrete height levels (target 0–3) with cliff and ramp sockets. The MVP ships at a single level, and the schema reserves the height fields from day one. |
| D6 | Agents: vehicles route on stitched road, rail and river graphs between real destinations. Animals wander within their biome, and birds flock across the map freely. |
| D7 | Atmosphere: photo mode ships in the MVP. Day/night, weather and seasons come later, but MVP assets carry their hooks (§9.4). |
| D8 | Platforms: desktop first. Mobile and tablet come in a later phase, so input and UI are designed touch-ready. |
| D9 | Persistence: IndexedDB autosave with multiple slots, plus `.hexworld` file export and import. No backend. |
| D10 | Art direction: stylized-real diorama. |
| D11 | Tile composition: a hex base, 6 edge pieces, a center piece and scattered props, assembled from data-defined kit pieces. |
| D12 | MVP content: about 10 tiles covering grass/meadow, forest, house, road, river, beach and ocean. Live agents are cars, birds and fish (fish added by the owner, 2026-10-13). |
| D13 | Audio: Suno music via a repo prompt pack. The owner generates the tracks, and repo scripts master and validate them. ElevenLabs makes the SFX and ambience, which are spatial and mixed by camera focus. |
| D14 | Camera: orbit, pan and zoom. First-person stroll and drive come later, so assets need close-range LOD0 quality and per-tile colliders from day one. The walk shipped early at the owner's request (2026-10-14, §15 P8); drive is still later. |
| D15 | Placement animation: staged assembly over about 1.5–2.5 s, skippable by placing the next tile. |
| D16 | Scale: about 2,000 tiles at 60 fps on desktop. |
| D17 | Asset authoring: Opus 5.5 subagents. Structural pieces come from scripted headless Blender generators, and organic hero pieces are made interactively through the Blender MCP with `.blend` sources kept. |
| D18 | UI: an HTML/CSS overlay (TypeScript) over the game canvas, talking to Rust through wasm-bindgen. Bevy UI and egui are not used for player-facing UI. |
| D19 | Name "Hex World". Public repo under MIT. Deployed to GitHub Pages at `hex-world.pardev.net`. |
| D20 | Players get an undo stack and a bulldoze tool. Bulldozing is blocked if it would split the world into disconnected islands. |
| D21 | Audio rights: the owner holds paid ElevenLabs and Suno subscriptions and owns the rights to all generated audio, so it may be committed to the public repo and served from the public site. Each audio asset's provenance records the provider and subscription. |

### Assumptions (confirm or correct)

| Code | Assumption |
|---|---|
| A1 | The world's first tile is grass/meadow. |
| A2 | Farm (with animals), windmill and rail/trains are not in the MVP. They ship in P3 alongside recipes. Exceptions: the windmill tile (owner request, 2026-10-14), the farm tile with static animals and the wheat field tile (owner requests, 2026-10-19) are in the inventory ahead of P3, with no recipe yet. |
| A3 | No WASM threads (TC1). Heavy simulation runs in a dedicated Web Worker running a second WASM instance, or on the GPU. |
| A4 | High-poly `.blend` files and bake intermediates are kept out of git. Scripted pieces are rebuilt from their generators, and interactive hero `.blend` files live in the local, gitignored blend store, referenced by SHA-256 (external storage deferred in P1, §11.6). Only runtime derivatives are committed. |
| A5 | Resolved, see D21. |

### Technical constraints

| Code | Constraint |
|---|---|
| TC1 | GitHub Pages cannot set COOP/COEP headers, so `SharedArrayBuffer` and WASM threads are unavailable. The `coi-serviceworker` workaround exists, but it is not used by default (it breaks on first load and complicates caching). |
| TC2 | GitHub Pages limits: the published site should stay under 1 GB, individual files under 100 MB, and soft bandwidth is about 100 GB/month. The runtime asset payload has a hard budget (§10). |
| TC3 | par-factory's runtime decoders (meshopt and KTX2 through three.js JS decoders) do not carry over. `bevy_gltf` supports neither `EXT_meshopt_compression`, `KHR_mesh_quantization` nor `KHR_texture_basisu` (bevy#19104), and Bevy's `basis-universal` feature wraps C++ that is unproven on wasm32. The asset route must be proven in P0 (§9.3). |
| TC5 | Bevy on wasm32 runs single-threaded: no pipelined rendering, and pipeline compilation blocks the frame. All render pipelines are warmed behind the loading screen. Bevy breaks its API about every 4 months, so the version is pinned exactly and upgrades are deliberate tasks. |
| TC4 | WebGPU availability: Chrome/Edge 113+, Safari 26+, and Firefox 141+ on Windows. Feature detection gates the app. |

## 4. Gameplay

### 4.1 Core loop

1. The world opens with one grass/meadow tile at axial `(0,0)` (A1).
2. The player opens the inventory and picks a tile type. While hovering an empty slot next to the world, the inventory shows which types are legal there, and a ghost preview of the tile appears in the slot.
3. The player rotates the ghost in 60° steps. Edges glow green (compatible), red (incompatible) or neutral (no neighbor).
4. The player places the tile. It is accepted only if every touching edge is compatible.
5. The tile plays its staged rise (§4.4), and then its agents and ambience start.
6. After the MVP (D1), placements are checked against recipes. A newly matched recipe unlocks a tile type, with a discovery moment and a recipe-book entry.

### 4.2 Placement rules

- A slot is legal if it is empty, adjacent to at least one placed tile, and at least one rotation of the chosen tile is compatible with every placed neighbor.
- The inventory offers auto-rotate: when a slot is hovered, the ghost snaps to the first compatible rotation, and the player can cycle through the other legal rotations. As built, it snaps to the rotation the player last chose for that tile when that is legal there, and R steps only through distinct legal orientations.
- Undo (Ctrl/Cmd+Z) reverts placements, replacements and bulldozes in LIFO order. Redo is supported. The stack depth is at least 100 actions (128 in `hex-core`, `HISTORY_DEPTH`) and persists with the save.
- Pin, quick place and replace (owner, 2026-10-14). A click on a free slot pins it, so the ghost, its rotation and the inventory's fit list stay on that slot while the pointer moves; a second click on it places, and Esc or a click elsewhere unpins. Shift+click places the selected tile at once, so a run of Shift-clicks places a run. Ctrl/Cmd+click on a placed tile replaces it with the selected tile when that fits its neighbours, as one undoable step: the old tile sinks with no dust and the new one rises. The same tile in the same orientation is a no-op.
- Bulldoze removes a tile unless removing it would disconnect the world graph (checked with a flood fill) or it is the last tile left.
- The empty slots offered for placement are always the ring of free cells adjacent to placed tiles.

### 4.3 Progression (P3)

- Recipes are data-defined neighbor patterns: a center tile type plus required neighbor types or features within radius 1–2. Example: farm + windmill + road → bakery.
- The recipe book lists discovered recipes, with hints (silhouette plus one known ingredient) for nearby undiscovered ones.
- Unlocks persist with the save. A "creative mode" toggle unlocks everything.

### 4.4 Staged rise animation

The rise is a timeline about 1.5–2.5 s long, data-driven per tile type:

1. The hex column pushes up out of the ground plane with dust particles, a camera micro-shake and a rumble SFX.
2. The terrain cap settles with a slight overshoot.
3. The edge pieces snap in, and water fills its channels (river, coast).
4. Props pop or grow in a staggered order: trees scale up, buildings rise floor by floor.
5. The tile's agents spawn and its ambient emitters fade in.

Placing another tile fast-forwards any tiles that are still animating to their end state. Below-surface features (river beds, craters, coast shelves) rise together with the column, so the cutaway reads correctly during the animation.

### 4.5 Camera and controls (desktop)

| Action | Input |
|---|---|
| Orbit | Right-drag |
| Pan | Middle-drag, or WASD / arrow keys |
| Zoom | Wheel (from far strategic down to a street-level close-up) |
| Place | Left-click a free slot to pin it, then left-click it again (or Enter on the web) to place |
| Quick place | Shift+click a free slot |
| Replace a placed tile | Ctrl/Cmd+click it |
| Unpin | Esc, or a click elsewhere |
| Select tile type | 1–9 (the web follows the sheet's "Fits here" order) |
| Rotate ghost | R / Shift+R, or Ctrl+wheel |
| Undo / redo | Ctrl/Cmd+Z / Ctrl/Cmd+Shift+Z |
| Bulldoze tool | B |
| Inventory | Tab |
| Photo mode | P |
| Walk (first person, P8) | F; WASD walk, Shift sprints, Space hops, the mouse looks; F or Esc leaves |
| Hide UI | H (web overlay) |
| Move focus between panels | F6, Shift+F6 back (web overlay) |
| Photo mode camera | WASD fly (Shift 4× faster), Q / E down and up, drag to look, wheel to dolly |

Every action routes through an input-action layer, so touch gestures (P7) and a first-person controller (P8) map onto the same actions.

### 4.6 Photo mode (MVP)

- A free camera that is detached from the orbit constraints.
- Depth of field (focus distance and aperture, with a tilt-shift preset), exposure, FOV and roll. A click on the scene focuses on the surface under it (ground, water, or the nearest house, tree including its crown, or rock); with the tilt-shift preset the sharp band also moves to the clicked row, and dragging the Focus distance slider returns it to the screen centre. C saves a PNG at the chosen resolution, also while the photo UI is hidden (owner requests, 2026-10-14).
- Hide UI, pause agents, and set the time of day once day/night exists (P5). Until then the panel shows the time-of-day slider disabled at Noon.
- Export PNG at 1×, 2× or 4× resolution through an offscreen render target (the web build offers up to 2× until 4× is tested for GPU memory there).

### 4.7 UI quality bar (owner, 2026-10-13)

The player-facing UI (D18 overlay) is held to a hero standard, like water:

- **Polished:** a deliberate visual system (type scale, spacing, colour tokens, iconography) that frames the diorama instead of competing with it. No stock component-library look.
- **Subtle interactions and motion:** every state change (hover, press, select, open, close, place, undo) has short, eased feedback. Motion explains cause and effect and never delays input. Reduced-motion is respected.
- **Highly intuitive:** a first-time player can place, rotate, undo and bulldoze without reading help text. Affordances and the legal-slot filter make valid actions obvious.
- **Mockups first:** before any UI is implemented, the owner reviews and approves mockups of every screen and the key interaction states, including motion (clips or interactive prototypes). UI work goes through the `impeccable` skill with a named design direction.
- **Direction (owner, 2026-10-13):** Legend, a survey-map margin with a socket-colour code ([`design/ui/legend/direction.md`](design/ui/legend/direction.md); product brief [`PRODUCT.md`](PRODUCT.md)). The inventory sheet stays open until Tab closes it. Tiles that do not fit stay listed, dimmed with their reason, below the tiles that fit.

### 4.8 Settings

The Settings sheet (Legend S9, owner deviations accepted as A21 on 2026-10-14) has four tabs. The mockup's Controls tab is not built.

- **Graphics.** Quality: Auto, Low, Medium, High, and Ultra where detection lands on High (§9.2). A quality change rebuilds the scene behind a loading panel ("Changing quality") that takes the pointer and the keys until the new tier has drawn (owner report, 2026-10-14). Tilt-shift: a switch, off by default, and a strength slider (§9.1). Colour grade: the named grades, `rich-3` by default (§9.1). Grass density: 0–150% of Normal in 5% steps, 100% by default, drawn on High and Ultra only (§9.1).
- **Audio.** Mute and five levels: master 80%, music 55%, effects 75%, interface 50%, and ambience 10% (owner, 2026-10-14).
- **World.** Walk speed: 2.8–8.0 m/s in 0.1 m/s steps, 2.8 m/s by default (§15 P8). Start over: after a confirm, it clears the current world to the start tile in place, keeping its name and save slot (owner, 2026-10-14).
- **Debug.** Developer overlays, all off by default and never saved (owner request, 2026-10-14): LOD tint with its key and drawn counts, Force LOD (Auto, 0, 1, 2), a performance corner, the hex grid with axial labels, edge sockets, walk colliders, scatter footprints with their grass exclusion discs, and the road and river routing graph. Also driven by the `debug` command and `--debug` / `?debug=` ([`docs/agent-operability.md`](docs/agent-operability.md#debug-overlays)).

Audio levels, the colour grade, the grass density and the walk speed are kept per viewer in `localStorage`, outside the world save. Quality and the tilt-shift choice travel with the save (§12.2). A save also carries the volumes, but loading it does not apply them (owner decision A16).

## 5. Tile system

### 5.1 Grid

- Pointy-top hexes with axial coordinates `(q, r)`. Storage is a hash map from coordinate to placed tile.
- World scale is real meters, so first person (P8) works without rescaling. Hex circumradius R = 16 m (about 27.7 m flat-to-flat), and the height level step is 4 m. Both values are constants, finalized in P1: R = 16 m, step 4 m (`hexgeo.py` asserted against `axial.rs`).
- Edge indices 0–5 run clockwise from east. Rotation `k` maps edge `e` to `(e + k) mod 6`.

### 5.2 Sockets

Each edge carries a socket descriptor:

```
EdgeSocket {
  kind: SocketKind,        // terrain/feature type at this edge
  lane: Option<LaneSpec>,  // road/river crossing (rail later): offset along the edge and width
  height: u8,              // edge height level (0 in MVP; reserved for D5)
  profile: EdgeProfile,    // Flat | Cliff | Ramp (MVP: Flat only)
  land: Option<Side>,      // directional kinds only (Beach): Left | Right, which side the land is on looking out across the edge
}
```

- `SocketKind` (MVP): `Grass`, `Forest`, `Road`, `River`, `Beach`, `Ocean`. Planned later: `Rail`, `Farm`, `Cliff`, `Mountain`, `Lava`, `Town` and others.
- A data table (`data/sockets.ron`) defines which socket kinds may meet, symmetrically. For example, `Road`↔`Road` is legal, `Road`↔`Grass` is not (roads cannot dead-end into an edge), and `Ocean`↔`Grass` is not (coasts need a beach). Beach sockets are directional (owner, 2026-10-13, option O1): a beach edge's ground runs from grass height at its land end to the seabed at its sea end, so it only meets a beach edge whose coast continues the same way (`land: Left` against `land: Right`, by which side the land is on looking out across the edge; `data/sockets.ron` lists `Beach` as directional). Beach edges never meet grass or ocean edges, so coastlines are continuous with no steps.
- Grass and Forest edges meet themselves and each other. Road, River, Beach and Ocean meet only their own kind.
- Lane sockets must also match in lane offset and width, so roads and rivers line up exactly across the seam. Road lanes are 6 m wide and river lanes 8 m, both centred on the edge (`data/sockets.ron`).
- A forest edge against a forest edge also gets a pair edge piece (`pair_edge_pieces` in `data/sockets.ron`): `forest_forest_edge`, one variant per local edge of the forest base, carries the leaf litter across the seam, ending in a straight slope at a corner beside a meadow edge (owner D3, A19 and Q3 option O-a, 2026-10-14; O-b follows when forest moves into its own streamed pack).
- Height rules (P4): edges must have equal height, unless the edge profile is `Cliff` (any difference) or `Ramp` (a difference of exactly 1).

### 5.3 Tile definitions (data)

Tile types live in `data/tiles/*.ron`:

```
TileDef {
  id: "road_bend_60",
  biome: Grass,
  base: "meadow_base",
  edges: [EdgeSocket; 6],
  center: Option<KitRef>,            // junction, building pad, lake bowl, …
  edge_pieces: EdgePieces,           // Derived from sockets (the default) | Explicit([Option<KitRef>; 6])
  props: Props,                      // scatter: [ScatterRule] (seeded, density per 100 m²) + hero: [HeroProp]
  networks: [NetworkSegment],        // road/rail/river graph edges inside the tile
  nodes: [NetworkNode],              // tagged destinations (houses) on the networks
  parking: [ParkingSpot],            // per-house spots (§7.1)
  emitters: [AudioEmitter],
  agents: [AgentSpawnRule],
  rise: RiseTimeline,
  unlock: Unlock,                    // Default | Recipe(id)
}
```

A build-time validator checks every tile definition: socket and lane consistency with its network segments, existence of every referenced kit piece, and the presence of every rotation's thumbnail. It also checks network endpoints and nodes, parking, beach land sides and the rise timeline (1.5–2.5 s, stages in order), and it freezes each tile's edge kinds (`TileDef::validate`, `crates/hex-core/tests/data_validator.rs`). A kit piece a tile names before it is built must be listed in `data/pending-pieces.ron`, which is empty now.

### 5.4 MVP tile list (D12)

About 10 types. The exact variants are frozen at the start of P2. With the owner's additions (pine and oak, the coast corners, the river spring and both river mouths) and the windmill, the farm and the wheat field ahead of P3, 21 types ship in `data/tiles/`, each one's edge kinds frozen by `crates/hex-core/tests/data_validator.rs`.

| # | Tile | Notes |
|---|---|---|
| 1 | Meadow | The starting tile. Grass on all edges, with seeded wildflower drifts (clustered from seeded noise, owner A18), rock scatter, and about 40 single wildflowers spread one by one across the grass, their density varied gently by seeded noise (owner, 2026-10-14). |
| 2 | Pine forest | Forest on all edges: about 20 scattered spruces and rocks on leaf litter, no oak (owner U3, 2026-10-14), with an undergrowth of fern drifts, a few fallen logs, shrubs (hazel and holly) and spruce saplings (2026-10-19 and 20, approvals pending). Id `forest`, so older saves load as pine forests. |
| 2a | Oak | Forest on all edges: the hero oak (or, on about one oak tile in three, its forked variant `tree_hero_oak_b`) on the forest base's litter and grass rim, with a few smaller scattered oaks, rocks and a few flowers. The bird roost (owner U3). Mixes freely with the pine forest through the forest-forest edge pieces. The oak is 1.2× its first build (13.3 m tall, crown 8.5 m), with no collar where the trunk meets the root flare (owner, approved 2026-10-14). |
| 2b | Windmill | Grass on all edges: the white tower windmill (P3 piece, refs A17) on the meadow base, its sails turning at 4 rpm in play, with a light drift of wildflowers and a few rocks kept clear of the tower. In the inventory ahead of P3 (owner request, 2026-10-14); no recipe uses it yet. The tower blocks the walker as a building. |
| 2c | Farm | Grass on all edges: the red barn (P3 piece, refs A17) along one side by the border, its doors toward three hay bales (two round, one stack; about one bale in three draws an upright round or a three-bale stack variant), and two cows (one grazing), two sheep, a grazing sheep and a lamb in a post-and-rail paddock (`fence_paddock`, one piece carrying the meadow relief, its 3 m gate toward the yard), on the meadow base with a light drift of wildflowers and a few rocks kept clear of every piece and off the fence line. The bales and animals stand where the meadow relief is flattest, so none floats. The animals stand still until P3's wander work. The wheat is its own tile (2d). In the inventory ahead of P3 (owner request, 2026-10-19); no recipe uses it yet. The barn blocks the walker as a building, the bales and animals as props, and the fence run by run as `fence`, so the walker enters the paddock through the gate. |
| 2d | Wheat field | Grass on all edges: ripe golden wheat in parallel rows on ploughed dark-brown earth (the `wheat_field` centre piece, refs A17 "wheat crop"), covering the tile inside a 3 m grass margin with a few wildflowers and rocks. Rows run along the tile's x, 1.2 m apart, so neighbouring fields continue each other's rows. The wheat sways in the wind, and the walker walks through it on the earth. In the inventory ahead of P3 (owner request, 2026-10-19); no recipe uses it yet. Piece approval pending. |
| 3 | House cluster | Grass edges plus 1 road edge, with 3 houses (two cottages and a two-storey) and 3 parking spots. A car destination. About one house slot in three draws a variant (`house_longhouse` for a cottage, `house_twostorey_b` for the two-storey), so neighbouring clusters differ. |
| 4 | Road straight | Road on opposite edges. The road tiles' grass carries a sparse spread of single wildflowers, clear of the road, and the straight, bend and junction tiles small verge bushes (`bush_verge`, `bush_verge_b`) in a band beyond the kerb (2026-10-20, approvals pending); tiles with parking get none. |
| 5 | Road bend | Road on edges 60° or 120° apart. |
| 6 | Road junction | A 3-way junction. |
| 7 | River straight | River lanes on opposite edges. The bed is cut below the surface. |
| 8 | River bend | Lanes 120° apart. |
| 9 | River + road bridge | A road crossing a river: shows bridges and cross-network seams. |
| 9a | River spring | River on one edge: a stream wells up from an 11 m pool inside the land (owner, 2026-10-14). The river flows away from it. |
| 10 | Beach | A straight coast: ocean, ocean, beach, grass, grass, beach. Surf foam. |
| 10a | Beach outer corner | A headland: ocean, ocean, beach, grass, beach, ocean. The coast turns around a point of land (owner, O1). |
| 10b | Beach inner corner | A bay: ocean, beach, grass, grass, grass, beach. The coast turns inward (owner, O1). |
| 10c | River mouth | The straight beach with one land edge turned into River: ocean, ocean, beach, river, grass, beach. The channel fans out over the sand into the surf (owner, 2026-10-14). Both handednesses ship: `river_mouth` (River on edge 3) and `river_mouth_l` (River on edge 4), authored mirrored. |
| 11 | Ocean | Ocean on all edges. Deep water with a visible shelf below the surface. |

## 6. Modular asset kit (D11)

- **Bases** are the hex column (side skirt in the owner's O4 "cutaway" look, 2026-10-13: a grass lip over root-dark topsoil that darkens to a near-black foot, no bright strata bands) plus a terrain cap per biome and height. One base mesh per biome is shared through instancing.
- **Edge pieces** are 6 per tile, chosen by socket: road stub, river mouth, coast transition, grass blend, and later cliff and ramp pieces. They are authored for edge 0 and rotated at runtime.
  - Built so far: the road and river lane edge pieces, drawn on lane edges only, and the forest-forest pair pieces (§5.2), which come in six per-edge variants because the forest base differs under each edge (decision D3 in the 2026-10-14 owner queue). Coast transitions are part of the beach and river mouth bases, and grass meets grass with no edge piece.
- **Center pieces** are junctions, bends, building pads, lake bowls and river beds. Below-surface geometry (river beds, crater bowls, coast shelves) belongs to the center or edge piece, and the hex column is cut to fit it.
- **Props** are trees, rocks, fences, houses, windmills, lamps and so on. They are placed by seeded scatter rules (density, exclusion masks around lanes) plus fixed hero placements.
- **Assembly.** At placement the runtime resolves the tile definition into a list of kit-piece instances. Static pieces are batched into per-chunk GPU instance buffers.
  - Built so far: the play world spawns each tile's pieces as entities under the tile root and relies on Bevy's automatic instancing (a 20-tile world draws 256–298 batches). The per-chunk instance buffers (`crates/hex-render/src/instanced.rs`, `layout=instanced`) run only in the `gate2000` perf scenario.
- **Seam continuity.** Kit pieces snap to a shared edge contract (lane offsets, edge heights, a vertex ring at the hex border), and the terrain cap's normals are blended at the hex border so seams don't show.
- **Variety.** Each kit piece has 2–4 variants, and props get a per-instance seed for hue, scale and rotation jitter.
  - Built so far: the scattered props ship three variants each, `tree_forest` with a tall slender `_b` and a young short `_c`, `rock_small` with a boulder `_b` and a low flat cluster `_c`, and `wildflower_clump` with a yellow and white `_b` and a blue and purple `_c`; the single-stem wildflowers come as four pieces (daisy, buttercup, cornflower, clover). A tile rule names the first piece and each instance draws one piece of its group from a hash of the tile seed (`crates/hex-play/src/play/scatter.rs` `VARIANTS`), so a seed always draws the same world. Scatter also gives each instance a seeded yaw and a scale jitter (±12% for trees, ±15% otherwise), with no hue jitter. Hero props have variants: a `house_cottage` slot of a house cluster draws the single-storey L-plan `house_longhouse` or the slate-roofed rendered `house_cottage_b` with its gabled porch (each about one time in six, splitting the one in three), the `house_twostorey` slot the jettied, hip-roofed timber-framed `house_twostorey_b`, the oak tile's hero slot the forked, east-leaning `tree_hero_oak_b`, and the farm's bale slots an upright `hay_bale_round_b` or a three-bale `hay_bale_stack_b`, each about one time in three, picked per slot from a hash of the cell and the slot index (`crates/hex-core/src/variant.rs` `HERO_VARIANTS`, `hero_piece`), so saves need no change and the ghost shows the same houses. The forest floor's `fern_clump` comes with a young upright `_b` and a wide low `_c`, and `log_fallen` with a short broken butt-end `_b` and an old hollow `_c`; the oak tile's scattered `tree_oak` with a younger, narrower `_b`; the forest undergrowth's `shrub_forest` (hazel) with a holly-like `_b` and `sapling_spruce` with a shorter, bushier `_b`; and the road verge's `bush_verge` with a smaller, lighter `_b`. The farm's static animals are separate heroes rather than variants: a grazing cow, a lamb and a grazing sheep stand beside the cow and two sheep. Every other kit piece ships one variant. The new pieces await the owner's look approval (`docs/checkpoints/prop-variants/`).
- **Runtime recolour (owner, 2026-10-13).** Vehicles, starting with the car, are recoloured at runtime from one model and one texture set. The model marks its paintable surfaces in a mask that needs no extra texture: a vertex-colour channel, or a reserved region of its existing atlas. Glass, tyres, lights and trim stay unpainted. Each instance carries its own paint colour, picked from a curated palette by the instance seed, as per-instance data that keeps GPU instancing intact, so it never needs one material per colour.
  - Built (`3dfc734`): the sedan, van and pickup mark paint in vertex colour alpha, and each car carries its colour from a 12-colour palette as a per-entity `MeshTag`, so every car of a type and LOD draws in one batch.

## 7. Living world

### 7.1 Networks

- Road, rail (P3) and river segments inside each tile are spline graphs with endpoints at the edge lanes. On placement, endpoints are stitched to the neighbors' matching endpoints to form a global graph per network type.
- Destinations are tagged nodes: houses and shops (the bakery), later stations, farms and factories.
- **Destination tiles (owner, 2026-10-19).** Every tile with a destination node carries its own road and parking: at least one road edge reaches the node through the tile's road segments, and at least one parking spot (an `anchor-park-<n>` socket on the tile's centre piece, `park-<n>` in the tile data, n the spot's index) joins it, so cars always arrive in and leave from a spot. There is no separate parking-lot tile. A road through the tile with a short spur into a forecourt or yard keeps every arrival on the same heading, so spots use the same nose-in, three-point-turn-out paths as house parking. Houses own the cars (car cap 1 up to their spots); a shop owns none (cap 0), and a visiting car parks for a short stay (20 s) and drives home. Each parking tile declares its buildings, kerbs and hedges for the clearance sweep (hex-sim `tests/parking.rs`), which a pickup-sized car (5.4 x 2.0 m) must clear on every path. The farm tile is pasture, not a destination, until the P3 farmyard (owner, 2026-10-19). Enforced by `TileDef::validate` (`validate_destinations`, `validate_parking`).
- **House parking (owner, 2026-10-13).** Each house on a house pad has its own parking spot, marked by an `anchor-park-<n>` socket on the pad, with a short path from the spot to the cluster's road node. A car leaves by pulling out of a spot onto the driveway and arrives by parking in a spot, so cars never pop in or out at the yard edge. Each house owns its car, and the tile's `AgentSpawnRule` max caps the cars per cluster. The house cluster's cap is 2 for its 3 houses, so the two-storey has no car and its spot is the visitor space (owner A9, 2026-10-14).

### 7.2 Agents

| Agent | Behavior | Phase |
|---|---|---|
| Car | Routes between destination nodes on the road graph (A* over a cached graph) and follows lane splines. Starts and ends in a house's parking spot (§7.1). Three body types to start: sedan, van and pickup truck (owner, 2026-10-13), each one model recoloured per instance at runtime (§6). Spawn rate scales with connected destinations. | P2 |
| Bird | Boids flying in small flocks over the whole map, roosting on oak tiles. Runs on GPU compute. The count scales with the map: a flock of 12 per 4 land tiles, none over the start tile, flocks flying in and away as tiles are placed and bulldozed (owner, 2026-10-14). | P2 |
| Fish | Schools that swim inside ocean and river water, visible through the clear surface. The same GPU boids pass as birds, confined to the water volume of connected water tiles and kept below the surface and above the bed (`docs/kit/water-contract.md`). They share the bird budget (owner, 2026-10-13). Fish skim a river mouth's shallow fan to pass between river and sea, and fish spawned in river water keep singles and small groups where a river joins the sea (owner, 2026-10-14). | P2 |
| Boat | Drifts on ocean tiles and travels connected rivers. | P3 |
| Train | Routes between stations on the rail graph. Multi-car consists that cross tile boundaries. | P3 |
| Farm animals | Wander within their farm tile's bounds, using idle, graze and run states. | P3 |
| Pedestrians | Walk sidewalk lanes in towns. | P6 |

- Agent caps scale with the quality tier (reference target: 300 vehicles, 500 animals and 1,000 boids at High, split between birds and fish: fish scaled to the water tiles in the world up to 300, birds to the land tiles up to 75% of the slots the fish leave).
  - Built so far: the boid budget scales (Low 250, Medium 500, High and Ultra 1,000; fish up to 75, 150 and 300). The car cap is a fixed 300 at every tier, which deviates from this rule (`docs/review/2026-10-14-prd-drift.md`).
- Agents are simulated at a fixed 20 Hz tick and interpolated at render time. Off-screen agents drop to a coarse simulation LOD (not built yet).
- Cars follow lane paths built as polylines, with arcs sampled every 5°, driving up to 1.5 m right of the road's centre line.
- Skinned creatures use baked animation (vertex animation textures or a bone-transform atlas) so they can be GPU-instanced. No per-agent CPU skinning.
  - The birds and fish need neither: they are rigid boid pieces posed in the vertex shader from per-vertex hinge data (wing flap, perch fold, tail wag). Farm animals (P3) will be the first skinned creatures.
- Animated props (windmill blades, water wheels, flags) use rigid object transforms or vertex animation textures.

### 7.3 Simulation architecture

The deterministic simulation core (grid, sockets, networks, pathing, agents) runs in Rust. Rendering-only motion runs on the GPU: boids in a compute pass, and grass and foliage sway and water in their shaders. If profiling shows the main thread is over budget, the simulation moves into a Web Worker running its own WASM instance, with state messages over `postMessage` (TC1).

## 8. Audio (D13)

### 8.1 SFX and ambience (ElevenLabs)

- Generated through the `elevenlabs` and `create-game-audio-assets` skills.
- **Per-tile emitters:** river babble, surf, town murmur, and later the windmill creak and farm animals. Emitters are positional and attenuate with distance. Built: river babble and surf (surf also plays on river mouths). The town murmur has no mastered cue yet, so house clusters are silent.
- **Cars (owner request, 2026-10-14):** a start sound when a car pulls out of its spot, a door when it parks, and one drive pass-by per trip, positional and following the car, on the ambience bus. At most three car voices play, one per car, within three hex steps of the focus.
- **Occasional one-shots:** not every tile type has a constant sound (owner, 2026-10-14). Forests play a short birdsong phrase or a leaf rustle, and wildflower meadows a passing bee swarm, positional at a tile near the camera focus at random intervals. `hexWorld.audio.play` fires one on demand.
- **Focus mix:** emitter gain is weighted by the camera's focus point and zoom, so zooming in on a tile brings up its sounds. A global bed of the river and coast loops follows the biome mix currently in view. Meadows and forests have no bed.
- **Placement:** a shared rumble and thunk, plus a short stinger per tile type. Built: the rumble and thunk, also on a replace. The per-tile stingers are not built.
- **UI:** select, rotate, invalid placement, undo, bulldoze and recipe-discovery cues. Built: all but recipe discovery, which comes with recipes in P3 (`discovery-stinger` is mastered with no hook yet).
- Uses the Web Audio API (HRTF or equal-power panning) through a thin TypeScript audio layer driven by events from Rust. Built: HRTF panning for every positional cue. Nothing is fetched before the first pointer or key press in play.

### 8.2 Music (Suno)

- `audio/music/prompts/*.md` holds one file per cue, with style tags, BPM, key, mood, instrumentation, structure tags, an instrumental flag, a loop intent and a target length.
- The owner generates the tracks in Suno and drops the WAV or MP3 into `assets-source/audio/music/<cue-id>/` in the main checkout. That folder's audio is git-ignored (a local store shared by worktrees), and provenance records each source's SHA-256 and size.
- `make audio-master` normalizes loudness (music −18 LUFS integrated), sets loop points, encodes the runtime format, and writes the provenance JSON (provider and subscription, per D21).
- The playlist crossfades between cues by world mood (the biome mix in view). Adaptive stems are a P5 option.
- Built (2026-10-14, `f0bc0c1`): six Suno cues are mastered. `meadow-day` (meadow, forest and river), `coast-breeze` (coast) and `hearth-town` (houses) crossfade at equal power over 6 s by the biome mix in view, after a 30 s dwell. `photo-mode` takes over while photo mode is open. `title-theme` and `discovery-stinger` have no hook yet. Music streams through media elements.

### 8.3 Runtime formats

- Opus in WebM is the primary runtime format, with an AAC (m4a) fallback for Safari if it is needed. Ambience loops are sample-accurate and gapless.
- Loudness targets follow par-factory: music −18 LUFS, SFX and loops −16 LUFS, UI cues −20 LUFS, with a true-peak ceiling of −1 dBTP.
- Built: every cue is encoded in both formats, and the page picks m4a when the browser refuses WebM. Bitrates are Opus 64k mono / 128k stereo and AAC 96k / 160k, with mono SFX and UI cues at 160k in AAC (owner, 2026-10-14: keep Opus 128k and AAC 160k for now, revisit once the full payload is known). Masters are limited to −2 dBTP, under the −1 dBTP ceiling (`assets-source/audio/cues.json`).

## 9. Rendering (D2, D10)

### 9.1 Pipeline

- Bevy `=0.20.0` on wgpu's WebGPU backend, compiled to `wasm32-unknown-unknown` with wasm-bindgen and built with Trunk (as in `~/Repos/suck-it-up`). Bevy provides PBR, cascaded shadows, bloom, tone mapping, TAA, SSAO and DOF. Custom render-graph nodes add water, VAT creatures, compute boids and anything the P0 spike shows is missing on the web.
- Default features are disabled and only the needed Bevy features are enabled, to control wasm size. The game crate keeps `opt-level=3` while dependencies use `"z"` (suck-it-up measured a 2× slowdown in hot loops at `"z"`).
- Frame latency (owner decision Q7, 2026-10-14): on the web, `web/frame-latency.js` keeps at most 3 frames of WebGPU work in flight by default, the bound wgpu keeps natively, which removes the uncapped stall tail (§15 P2). `?frame_latency=N` (1–8) changes it and `?frame_latency=off` turns it off.
- A clustered-forward PBR pipeline (metallic-roughness, glTF-conformant) with an HDR target.
- Lighting: a directional sun with cascaded shadow maps (3–4 cascades, PCF/PCSS soft shadows), image-based lighting from a prefiltered environment map plus irradiance, and SSAO (GTAO). Screen-space GI is evaluated in P5.
  - Built so far: 4 cascades out to 2,000 m at every tier, the first ending at 25 m in the kit and play scenes so road kerbs shadow cleanly, with Bevy's default shadow filter and map size (no PCSS). The noon IBL runs at intensity 1,500, and Bevy's SSAO runs on High and Ultra.
- Water: shared shaders for rivers and ocean, with flow maps on rivers, Gerstner or FFT waves on the ocean, depth-based color absorption, shoreline foam, and screen-space refraction plus planar or SSR reflection.
  - Built so far: Gerstner waves (4, 6 or 8 by tier), two-phase flow-map ripples on rivers, foam from shore contact, wash lines, wave crests, fast river flow and the river-mouth meeting band, refraction with depth absorption, caustics, and a screen-space reflection march on High and Ultra. Planar reflection and FFT waves are not built. Kit beaches show the live wet band from the water look-dev (owner A22, `c9a4d03`).
- Water is a hero feature, owner-approved like the look-dev. The quality bar covers seven areas.
  - **Animation.** Layered wave motion that never visibly loops: summed Gerstner or FFT waves on the ocean, and flow-map advection on rivers using two phase-offset layers to hide the reset.
  - **Optics.** Fresnel reflection of the IBL sky and nearby geometry, sun glints, refraction with depth-tinted absorption and in-scattering, and animated caustics on shallow beds and shelves.
  - **Foam.** Foam from water depth, wave crests and flow speed, plus shoreline wash and a wet-sand band on beaches.
  - **Continuity.** World-space coordinates, so water continues across tile seams and rotations with no visible joint. The same seam close-up standard as the roads applies.
  - **Exposed edges.** Where a water tile's edge has no water neighbour, its cut side renders as the water seen from inside: a skirt whose top follows the animated surface exactly, with depth absorption and tint down to the tile base and the surface's underside visible at the top. Never empty space.
  - **Placement.** Water fills its channels during the staged rise (§4.4).
  - **Quality tiers.** The effect scales per tier (§9.2). It must hold the P2 frame budget and fit inside WebGPU's 16 sampled-texture baseline per stage.
  - **Prototype first.** Prototype the look in its own scenario before building the tile content.
- Foliage: GPU-instanced with wind sway in the vertex shader. Alpha-tested leaves with dithered LOD transitions.
  - Built so far (owner request, 2026-10-14, `9ddb35f`): the oak, the spruces, wildflowers, reeds and cattails sway in the vertex stage of the forward pass and the prepass, so depth, motion vectors and every shadow cascade move with the leaves. Each piece's kit manifest slot carries a `wind` amplitude (trees 0.3, wildflowers 0.15, reeds and cattails 0.5) and its mesh a baked bend weight in TEXCOORD_1: trees lean from the trunk base, flex and flutter their outer cards, and plants bend by the square of their height. Leaves are alpha-tested. Foliage draws as ordinary entities batched by Bevy's automatic instancing. Evidence: `docs/checkpoints/foliage-wind/` (`make shots-foliage-wind`).
- Grass (owner request, 2026-10-14, `a1c35ef` and `5560267`): blades on the grass-covered ground of placed tiles near the camera, on High and Ultra only.
  - Density: Normal is 28 blades per m², and the Settings slider runs 0–150% of it (§4.8). Every lower density is a subset of the seeded 150% set, so a slider move swaps meshes and never rebuilds the scene.
  - Shape: each blade is two crossed tapered planes about 0.3 of their height wide, with a seeded scale (0.7–1.4), yaw, lean, hue and lightness jitter, a world-space clump height noise (0.75–1.45) and a lighter, less saturated tip, after the owner's small-world reference.
  - Where: the meadow cap, the forest rim and edges, road verges, river grass and beach grass, up to the hex border, in among wildflower clumps (a 0.2 m centre stays clear) and right up to single wildflowers, and never under rocks, trunks, houses or spruces or below the water line plus 0.3 m. The ghost draws none.
  - Drawing: one mesh per tile, built on the CPU for tiles within 60 m of the camera (dropped beyond 72 m). Blades thin from 16 m to 46 m and shrink out by 58 m. They write the prepass and receive shadows but cast none, with the §9.4 season tint. Evidence: `docs/checkpoints/grass/` (`make shots-grass`). Their frame cost in play is not yet measured under the load guard (`make perf-grass` exists).
- Wind: one shared field, `hex_render::wind` (`crates/hex-render/src/wind.wesl`), drives the grass and the foliage sway. It is integer-hashed value noise with no texture and no binding: a prevailing direction (0.8, 0.6) turned by a 90 m field and a 20 m detail field, gusts in two octaves at 24 m and 12 m scrolling downwind at 6.5 m/s, 7 m strength patches, and 0.9 m flutter across the wind, on Bevy's `Time` clock. Its strength, gusts and direction are fixed constants today; following the season and the weather is on the P5 roadmap (§15). The water keeps its own wave direction.
- Post-processing: TAA, bloom, filmic tone mapping (the `rich-3` grade by default: ACES fitted at EV100 12.5 with a shadow lift before the curve, owner 2026-10-14; named grades switch the tone mapper, exposure, grading and backdrop live; D4 in [`docs/decisions/0002-asset-pipeline.md`](docs/decisions/0002-asset-pipeline.md)), color grading by LUT, and DOF (used by photo mode).
  - Built so far: the grades are `rich-3` (the default), `tony` (TonyMcMapface at EV100 12.9), `aces`, `rich-1` and `rich-2`, chosen by `--grade` / `?grade=`, the `set_grade` command or Settings (§4.8). They use Bevy's colour grading; LUT grading is not built. TAA and bloom run at every tier and SSAO on High and Ultra. DOF runs only on photo mode's Manual lens.
- Tilt-shift: a miniature-diorama look for the normal game camera, not only photo mode. A custom post node blurs by distance from a tilted focal plane that follows the orbit camera's focus point, with the blur strength scaled by zoom (strong when zoomed out, off near ground level) plus an optional saturation and contrast lift. It is a player toggle with a strength slider, it is off by default (owner, 2026-10-14; it was on from the Medium tier), and it shares its kernel with photo mode's tilt-shift preset. Bevy's DOF has no tilted plane, so this is a custom node.
  - Built so far: a sharp horizontal band at the camera's look-at point on the ground, with blur growing with screen distance from the band and with depth difference, which approximates the tilted plane. It fades out as the focus distance drops from 48 m to 12 m. It takes 8, 16 or 24 taps on Medium, High and Ultra, and adds a 0.12 saturation lift (no contrast lift). A tier change resets it to the tier's default until the player toggles it; only the toggle counts as their choice (owner A21).
- Bulldoze dust: a bulldozed tile sinks inside a dust cloud of at most 120 particles, at 80% of its first size and duration and clear 2.24 s after the bulldoze (owner, 2026-10-14). A tile taken down by undo or redo sinks with a smaller puff around its rim, and a replaced tile sinks with no dust.
- Culling: GPU frustum and Hi-Z occlusion culling with indirect draws, organized in chunks (for example 7- or 19-hex clusters).
- LOD: three mesh LODs per kit piece with distance and screen-size selection and dithered cross-fade. Far chunks use merged meshes.
  - Built so far: GPU and Hi-Z occlusion culling with indirect draws are not built. Chunks of 7 or 19 hexes, merged chunk meshes and CPU chunk culling exist only in the `gate2000` perf layouts (`layout=chunks`, `layout=instanced`); play uses neither. LODs switch by distance only, at 150 m and 400 m with a 10 m `VisibilityRange` band. The band dithers in the prepass but not in the kit forward shader, so both LODs draw in colour across it (a deviation, see the drift report).

### 9.2 Quality tiers

| Tier | Resolution scale | Shadows | AO | Water | Agent caps |
|---|---|---|---|---|---|
| Low | 0.67 | 2 cascades, 1k | off | no refraction | 25% |
| Medium | 0.85 | 3 cascades, 2k | half-res | SSR off | 50% |
| High | 1.0 | 4 cascades, 2k | full | full | 100% |
| Ultra | 1.0 + TAA upsampled detail | 4 cascades, 4k | full + bent normals | full + planar | 100% |

The tier is auto-detected on first run from adapter info and a short benchmark, and the player can override it.

As built (2026-10-14; `hex-play/src/tier.rs` and `hex-play/src/boids/plan.rs`, plus `hex-render/src/post.rs` and `hex-render/src/water.rs`, under `crates/`):

| Tier | Resolution scale | Shadows | AO | Water | Boids (fish at most) | Grass | Tilt-shift taps |
|---|---|---|---|---|---|---|---|
| Low | 1.0 | 4 cascades, default map | off | no refraction, 4 waves | 250 (75) | off | off |
| Medium | 1.0 | same | off | refraction, one caustic layer, 6 waves | 500 (150) | off | 8 |
| High | 1.0 | same | SSAO | refraction, two caustic layers, SSR, 8 waves | 1,000 (300) | on | 16 |
| Ultra | 1.0 | same | SSAO | as High | 1,000 (300) | on | 24 |

- Not built yet: the resolution scales, per-tier shadow cascades and map sizes, Medium's half-resolution AO, and Ultra's TAA-upsampled detail, 4k shadows, bent normals and planar reflection. With tilt-shift off by default, Ultra renders the same as High. The car cap does not follow the tier (§7.2).
- Detection never picks Ultra: a player picks it in Settings (offered when detection lands on High), or `?tier=ultra` sets it. On the web a warm-up probe reads frames 50–109 and steps down one tier when their median exceeds 25 ms. An explicit `tier` key always wins, and loading a save applies its quality unless a `tier` key is set. Rules: [`docs/agent-operability.md`](docs/agent-operability.md#quality-tier).
- A quality change rebuilds the scene behind the loading panel (§4.8): one blocking rebuild frame, 0.21–0.48 s warm on the 208-tile water world, and about 3.2 s more in the browser's GPU process with a cold Metal shader cache (`docs/checkpoints/tier-change/`, `e2e/tier-stall.mjs`).

### 9.3 Asset formats at runtime

- glTF 2.0 binary (GLB) with `EXT_meshopt_compression` and sidecar KTX2 textures (UASTC+RDO source, transcoded offline to BC7 and ASTC with zstd, plus PNG), bound by the runtime; GLBs embed no images.
- Decided in P0 (TC3, Q3; [`docs/decisions/0001-engine.md`](docs/decisions/0001-engine.md)): route (a), with meshes decoded by the pure-Rust `meshoptimizer-rs` GLB rewrite in `crates/hex-assets` and textures shipped as BC7/ASTC KTX2 + zstd with a PNG fallback. Route (a) as specified: textures are shipped as pre-transcoded KTX2 + zstd in two families, BC7 (desktop) and ASTC (Apple mobile, P7), selected at runtime from the device's `texture-compression-bc` / `-astc` features, so no Basis transcoder is needed and Bevy's `ktx2` + pure-Rust `zstd_rust` loader works as-is; meshes are meshopt-compressed and decoded before `bevy_gltf` sees them, either by a custom asset loader that rewrites the GLB using the pure-Rust `meshoptimizer-rs` decoder or by the official `meshopt_decoder.js` called through wasm-bindgen. Fallbacks: (b) Basis Universal through `bevy_basisu_loader` or the pure-Rust `basisu` crate; (c) uncompressed vertex data with only zstd at the transport level.

### 9.4 Hooks for the future atmosphere phases (D7)

Every MVP asset and shader carries these from the start:

| Hook | Purpose |
|---|---|
| Emissive map/slot with a night-only intensity channel | Day/night: windows, lamps, headlights |
| Snow-accumulation mask (vertex color channel or texture) + up-facing coverage in the shader | Seasons/weather: snow |
| Wetness/porosity mask (ORM-adjacent channel) | Weather: rain darkening, puddles |
| Foliage seasonal tint index (per-instance) | Seasons: autumn color, bare branches |
| Time-of-day uniform block driving sun, sky, fog and exposure | Day/night, with a fixed noon value in the MVP |
| Water freeze parameter | Seasons: frozen rivers |

Built: every hook is present and inert at the MVP values (noon, dry, summer, unfrozen), checked by the asset gate (`docs/kit/hooks-contract.md`). Time of day is a Rust resource fixed at noon rather than a uniform block. The `kit_hooks` key drives the masks for verification shots only.

## 10. Performance and size budgets

| Item | Budget |
|---|---|
| Frame time on the reference desktop (Apple M-series, High tier, 1440p, 2,000 tiles) | ≤16.6 ms at the 95th percentile |
| Main-thread simulation tick | ≤4 ms |
| Draw calls after instancing and indirect draws | ≤500 |
| Initial download before first interaction | ≤30 MB (WASM + core kit + UI) |
| WASM binary over the wire (after `wasm-opt -Oz`, gzip as served by Pages) | ≤12 MB (Bevy's measured floor is about 7.6 MB) |
| Total deployed site at the full game | ≤600 MB (TC2) |
| Kit piece LOD0 | ≤4k tris typical, ≤12k for hero pieces (landmarks, volcano) |
| LOD1 / LOD2 | ≤50% / ≤15% of LOD0 (LOD2 ≤20% for the lane-cut river bases and the coast-corner bases, whose channel and arc profiles drift past the water contract's tolerance at 15%: overnight decisions D1 and D5 in the 2026-10-14 owner queue) |
| Prop LOD0 | ≤1.5k tris (trees ≤3k, with leaf cards; the river dressing centres ≤2.5k for close-up reeds and cattails, owner 2026-10-14) |
| Textures | ≤2k per kit-piece atlas (KTX2) |
| Per-GLB size | ≤2 MB |
| KTX2 bytes | ≤1.5 MB per piece, ≤3.6 MB per tileable set, ≤11.5 MB for a hero-class piece (`crates/hex-assets/src/kit_manifest.rs`) |
| Audio decoded in memory | ≤96 MB |

Kit content streams in by biome pack after the first interaction.

## 11. Asset pipeline (D17)

This adapts par-factory's pipeline (`~/Repos/par-factory/docs/specs/09-asset-pipeline.md`). Where par-factory routes modeling to a `gpt-6-astra` subagent, Hex World uses Opus 5.5 subagents.

### 11.1 Authoring routes

- **Scripted (structural):** bases, edge pieces, road, rail, river and coast pieces, buildings. An Opus 5.5 subagent writes a reproducible `scripts/blender/kit/<piece-id>.py` generator and runs it headless (`blender -b -P`). Rebuilding from the script must be deterministic.
- **Interactive (organic heroes):** mountains, volcano, cliffs, animals, hero trees. An Opus 5.5 subagent sculpts through the Blender MCP in a live session. The `.blend` source is saved to the local blend store and referenced by SHA-256 (§11.6), and the piece is exported through the same scripted bake and LOD stage.
- **Headless only for now (owner, 2026-10-13):** the owner paused the live route ("go headless until I say otherwise"). Blender work since then runs headless (`blender -b`), and the hero oak's later rounds replay its live steps headless from its stored `.blend` (`assets-source/kit/tree_hero_oak/friction.md`, entry 33). An organic hero piece needs a headless plan or the owner's say-so before it starts.

One subagent works on one kit piece at a time. At most 3 Blender asset subagents run concurrently, to avoid exhausting the machine's resources. The owner reviews each family together.

Concept art, reference images and texture references are generated with Higgsfield (`higgsfield` CLI).

### 11.2 Stages

1. **High-poly:** sculpt or modifier-stack geometry at final detail (no tri budget). Bevels, damage and surface detail are modeled.
2. **Low-poly cage / LOD0:** a retopologized or decimated mesh within budget, with clean UVs (one atlas per piece) and a texel density standard decided in P1.
3. **Bake:** high-poly → LOD0 using Cycles bakes: base color, tangent-space normal, ORM (AO / roughness / metallic), emissive, plus the hook masks (§9.4).
4. **LODs:** LOD1 and LOD2 (with the normal map reused), plus a simplified collider mesh (D14). Border-touching pieces regenerate each LOD from the same border ring at a coarser interior and are never decimated, so the seam ring stays exact. Other pieces use Blender Decimate, and trees keep a scaled subset of their leaf cards and decimate only the bark.
5. **Export:** a GLB with named nodes, sockets (`socket-lane-<n>`, `anchor-prop-<n>`), a pivot at the hex center or edge-0 midpoint, and Y-up.
6. **Compress:** gltf-transform 4.3.0 meshopt (`EXT_meshopt_compression`, FLOAT attributes, no `KHR_mesh_quantization`), glTF validation, per-channel UASTC → BC7/ASTC KTX2 + zstd with sRGB (base color, emissive) or linear (normal, ORM, masks) formats, PNG fallback, and hard GLB and texture byte budgets.
7. **Gate:** reimport check (geometry, sockets, materials, LOD ratios, budget), a 6-view render, a seam test against neighbor pieces, an in-engine screenshot, and a provenance JSON. Then owner approval.

### 11.3 Directory layout

```
assets-source/
  kit/<piece-id>/               # generator output reports, renders, provenance (no .blend, see §11.6)
  blend/<piece-id>/             # .blend sources and bake intermediates (local-only, gitignored)
  audio/{sfx,ambience,music}/<cue-id>/
scripts/blender/kit/<piece-id>.py
scripts/blender/lib/            # shared bake/LOD/export helpers
audio/music/prompts/<cue-id>.md
data/{tiles,sockets.ron,recipes}
assets/kit/                     # runtime GLB/KTX2/PNG + manifest.json
```

### 11.4 Provenance

Every asset gets `provenance.json` recording: the generator script hash and the hashes of the helper modules it imports (interactive pieces also record their `.blend` SHA-256 reference), the authoring agent and model, Blender version, bake settings, tool versions (gltf-transform, KTX), source and runtime hashes, the license, and for audio the provider and subscription (D21).

### 11.5 Make targets

`make kit-build PIECE=<id>`, `make kit-check`, `make kit-compress`, `make audio-master`, `make audio-check`, `make assets-manifest`. `make checkall` runs the asset gates for any staged assets.

As built: `make kit-compress` also takes `PIECE=<id>`. There is no `make assets-manifest`: a piece's entry in `assets/kit/manifest.json` is added by hand before `scripts/assets/kit-provenance.mjs` stamps it validated, and `make audio-master` writes the audio manifest. The kit gate (`kit_gate`) and the data validator run inside `cargo test` over every committed asset, so `make checkall` and CI check all of them, not only staged ones. `make audio-check` runs on its own, outside `checkall` and CI.

### 11.6 Large-file storage

Runtime derivatives are committed directly (each under the budget). Interactive `.blend` sources and bake intermediates go to external storage (an R2 bucket or similar, decided in P1) and are referenced by hash in the provenance. Git LFS is not used for them because of quota limits.

P1 (owner, 2026-10-13): external storage deferred. `.blend` sources and bake intermediates stay local in the gitignored `assets-source/blend/`, referenced by SHA-256 in provenance.

## 12. Technical architecture

### 12.1 Workspace

```
crates/
  hex-core/      # grid, sockets, tile defs, placement rules, undo, save model (no GPU deps; native-testable)
  hex-sim/       # networks, pathing, agents, recipes (deterministic, fixed tick)
  hex-render/    # Bevy renderer primitives: materials, water, post effects, lighting
  hex-play-state/ # Shared presentation data, no renderer or play dependency
  hex-play/      # Play presentation, boids, cars, walking, tier coordination; optional gate tooling
  hex-assets/    # meshopt GLB loader, KTX2 family selection, kit manifest, streaming
  hex-app/       # Bevy app + plugins, wasm entry, input actions, JS bridge (wasm-bindgen)
web/             # Trunk-served web shell; web/ui: the TypeScript HUD overlay (esbuild, no Vite), audio layer and IndexedDB saves; unsupported-browser page, frame-latency limit
data/            # tile/socket/recipe RON, validated at build
scripts/         # blender, audio, manifest, deploy
```

- `hex-core` has no Bevy dependency. `hex-sim` depends only on `bevy_ecs`, so it survives a fallback to the custom wgpu route (D2). As built, `hex-sim` depends only on `hex-core`, with no Bevy at all, and recipes are not in it yet (P3). Both compile natively and are covered by `cargo test`. Rendering primitives are isolated in `hex-render`; play presentation and its runtime consumers live in `hex-play`, with shared presentation data in `hex-play-state`.
- A native desktop build of `hex-app` (Bevy on Metal) is kept working for fast iteration, profiling and screenshot tests.
- Reusable local prior art: `hexx` hex math and the WFC generator from `~/Repos/nurbles`; the Trunk build, WebGPU gate, Pages deploy and browser smoke test from `~/Repos/suck-it-up`; the headless Blender build/validate/promote scripts from `~/Repos/the-cube`.
- The JS bridge is a typed command/event API (`placeTile`, `rotateGhost`, `undo`, events such as `tilePlaced`, `recipeUnlocked`, `audioEmitterUpdate`), with TypeScript types generated from Rust.
  - As built: `web/ui/src/bridge.ts` is written by hand and checked against `bridge-schema.json`, which a `hex-app` test regenerates from the Rust enums (`make ui-schema`), so a Rust change fails `cargo test` and then `make ui-test` until both match. `recipeUnlocked` arrives with recipes (P3). There is no `audioEmitterUpdate`: the audio layer computes its emitter mix in TypeScript from the play state each frame. Commands and events: [`docs/agent-operability.md`](docs/agent-operability.md#commands).

### 12.2 Save format

- A versioned, compact binary save (postcard or bincode) containing the tile map, rotations, seeds, unlocks, the undo stack, the camera, and settings. Agents are re-derived on load, not saved.
- Autosave to IndexedDB runs after every placement (debounced). There are multiple named slots.
- A `.hexworld` export is the save blob plus a magic header and a version. Import validates the file and migrates older versions.
- A schema contract document lists migrations, and tests load every historical version.
- As built ([`docs/save-format.md`](docs/save-format.md), `crates/hex-core/src/save.rs`): postcard behind a `HEXWORLD` magic, a u16 format version (3 since replace, `d27a627`), the payload length and a CRC-32, and files over 16 MiB are refused. A save holds the world seed, each tile's cell, level, id, rotation and per-cell seed, the undo and redo stacks, unlocks (empty until P3), the orbit camera, and the quality, tilt-shift and audio-volume settings. Fixtures for v1 to v3 load in tests. Autosave runs 600 ms after each edit and flushes on page hide, and the Worlds sheet offers new, rename, duplicate, delete, export and import (picker or drag-drop). The load confirm, import preview and refusal panel of the mockups are on the backlog (owner A16).

### 12.3 Agent operability

Following `~/.claude/guides/agent-operability.md`, both the native and web builds support:

- `?shot=<scenario>` and `--shot`: load a scripted scenario, render, and save a PNG.
- `?diag=1`: an overlay with frame timings, draw calls, agent counts and GPU memory.
- A deterministic scenario runner (seeded world plus a placement script) for e2e and perf tests.
- As built: `?shot=` and `--shot` take one of the fixed scenarios and capture at frame 120; on the web the PNG comes back as a data URL through `hexWorld.takeShot`. `?diag=1` shows the `diag()` JSON: frame-time percentiles, visible mesh and material pairs and triangles, car and boid counts, the wasm heap, GPU pass timings and the stall histogram. It has no GPU memory figure and no draw-call count. The runner is `--script <file.json>` natively with `--seed` and `--clock fixed`; web e2e sends the same command lists through `hexWorld.cmd`. Every flag, command and state field: [`docs/agent-operability.md`](docs/agent-operability.md).

## 13. Deployment

- GitHub Actions on push to `main`: build the WASM (release, `wasm-opt`), build the web bundle, run the gates, and deploy to Pages with `actions/deploy-pages`.
  - As built (`.github/workflows/ci.yml`): a check job runs `make fmt-check`, `ui-check`, `lint`, `typecheck` and `test`; a web job runs `make web-size` (the Trunk release build with `wasm-opt` `z` and the 12 MiB gzip wasm gate) and deploys with `actions/deploy-pages`. CI does not run the initial-download gate; `make checkall-ci` runs it locally before a push.
- Local play and review servers: `scripts/dev/serve.py <port> <dir>` serves `dist/` on 8094 after `make build-web` and the repo on 8095 for review pages, with a deep listen backlog and `Cache-Control: no-cache`, so a rebuilt kit GLB is never served from the browser's cache (`012d88e`, `1cc6d26`).
- A `CNAME` file containing `hex-world.pardev.net`, plus a Cloudflare DNS CNAME record pointing to `paulrobello.github.io` (DNS-only, so GitHub can issue the TLS certificate). HTTPS is enforced.
- Fingerprinted asset filenames with long cache lifetimes. The manifest has a short cache lifetime.
- An unsupported-browser page (served when WebGPU feature detection fails, TC4) that names the supported browsers.
- `pardev.net` is a Cloudflare-managed domain (`~/.claude/guides/infrastructure.md`).

## 14. Quality gates and testing

- `make checkall` (fast): `cargo fmt --check`, `clippy -D warnings`, `cargo test` (core and sim), TypeScript typecheck, lint and unit tests, data validation (tiles, sockets, recipes), and asset gates for staged assets.
- `make checkall-ci`: everything in `checkall` plus the WASM release build, the bundle-size budget, the initial-download gate (`scripts/size/initial-download.mjs`, ≤30 MiB for the larger of BC7 and ASTC), and the deploy-artifact check.
- As built: `make checkall` is `fmt-check`, `lint`, `typecheck` (native and wasm32), `test` (the whole workspace, so the kit gate and data validator included) and `ui-check` (TypeScript typecheck, lint, unit tests and build). `make checkall-ci` adds `web-size` (release build and the 12 MiB gzip wasm gate) and `initial-download`. Recipe validation (no recipes yet) and the deploy-artifact check are not built.
- Browser e2e and perf tests run locally with Playwright on GPU Chromium (`channel: 'chromium'`, Metal ANGLE flags) and are never run in CI. They cover a placement scenario, an undo/bulldoze scenario, a save/load round trip, a photo-mode export, and a 2,000-tile perf scenario.
- Visual regression: `?shot=` scenarios compared against approved baselines with a perceptual tolerance. Not built yet: the `make shots-*` targets capture checkpoint evidence under `docs/checkpoints/` but compare nothing.
- Unit-tested core rules: socket compatibility, rotation, legal-slot computation, bulldoze connectivity, undo/redo, save migration, and network stitching.

## 15. Phases

Each phase ends with every acceptance criterion verified, with evidence, and is not done until then.

### P0 — Foundations spike

Goal: de-risk the stack before content work.

- [x] Rust workspace and Trunk web shell build. `make checkall` and `make checkall-ci` exist and pass.
- [x] Bevy 0.20 runs on the WebGPU backend in Chrome and Safari 26 (macOS), rendering a lit PBR hex mesh. The native build renders the same scene. An iOS 26 Safari smoke test loads. (Verified on Safari 27.0.1, the installed version, and on the owner's iPhone on 2026-10-13.)
- [x] Engine gate (D2): 2,000 instanced hex tiles with 4 shadow cascades, bloom, TAA, SSAO and DOF hold the frame budget in Chrome and Safari on the reference machine. Logs confirm the SSAO plugin is active and report the GPU-preprocessing mode. The Bevy-vs-custom-wgpu decision is recorded with these measurements. (Provisional: Bevy stays; Chrome uncapped p95 3.7–4.3 ms on the tuned instanced layout, but full shadows fail and paced p95 is 17.3–17.5 ms against 17.0. Carry-overs for P2: GPU timestamp queries and the >25 ms stall tail. See [`docs/decisions/0001-engine.md`](docs/decisions/0001-engine.md).)
- [x] One custom compute pass (boids) and one custom material render inside Bevy's render graph on WebGPU.
- [x] One meshopt-compressed GLB (float attributes, not quantized) and one BC7 KTX2 + zstd texture load and render on wasm32. The asset route (§9.3) is recorded with measured size and load time. (The P0 GLB is meshopt-compressed but not quantized; bevy_gltf 0.20 rejects `KHR_mesh_quantization`; see D7 in `docs/decisions/0002-asset-pipeline.md`.)
- [x] Wire size of the wasm and time to first interactive frame are measured on GitHub Pages (`docs/perf/p0-deploy.json`: 8,571,055 B gzip wasm, READY 9.6 s on one cold load).
- [x] The unsupported-browser page is shown when WebGPU is missing.
- [x] GitHub Actions deploys to Pages, and `https://hex-world.pardev.net` serves the build over HTTPS.
- [x] The `?shot=` and `?diag=1` hooks work.

### P1 — Asset pipeline proof

Goal: one kit family through the full pipeline at final quality.

- [x] Shared Blender helper library: bake (all maps plus hook masks), LOD, collider, export, compress. Evidence: `scripts/blender/lib/`, its tests in `scripts/blender/tests/` and `make blender-test` (45 tests, exit 0, 2026-10-13).
- [x] Scripted pieces: meadow base and road straight edge/center, each passing every gate in §11.2. Interactive route: one hero tree through the Blender MCP, also passing. Evidence: `make kit-check` and `assets-source/kit/{meadow_base,road_straight_edge,road_straight_center,tree_hero_oak}/provenance.json`.
- [x] Look-dev: tone mapping, IBL and the texel density standard are chosen. The hex scale constants are finalized. Evidence: `assets-source/lookdev/lookdev.json` (`approvals`, `gate: "look-dev"`).
- [x] Seam test: two road tiles placed side by side show no visible seam in an in-engine close-up screenshot (owner-approved). Evidence: `assets-source/kit/road_straight_edge/approvals.json` (`gate: "seam"`) and `assets-source/kit/road_straight_edge/seam-checkpoint.md`.
- [x] Provenance JSON complete for every piece. Evidence: the four `assets-source/kit/<piece>/provenance.json` files and the four `assets-source/kit/materials/<set>/provenance.json` files, checked by `kit_gate::provenance_complete_and_hashes_match` and `kit_gate::material_provenance_complete_and_hashes_match`.
- [ ] The external storage for `.blend` files is chosen and working. Deferred by owner (R1), local SHA-256-referenced storage in use (§11.6).

### P2 — MVP vertical slice

Goal: the polished, playable core loop with D12 content.

- [ ] All MVP tiles (§5.4) are built through the pipeline and approved. (All 21 tiles draw built pieces (the windmill, farm and wheat field tiles, ahead of P3, included; the windmill's and the wheat field's piece approvals are still pending, the farm pieces are owner-approved 2026-10-19), and `data/pending-pieces.ron` is empty. In `assets-source/kit/*/approvals.json` every piece of the meadow, pine forest, oak, the three beach tiles and ocean is owner-approved. Still open, 2026-10-14: `river_edge`, `river_straight_base` and `river_bend_base` carry no piece approval record (the owner approved the river family's shape, seams and LODs as A2); the straight, bend and bridge centres were rebuilt with the wind bend weight in `9ddb35f` and are pending; the spring's and both river mouths' bases and centres have never been approved as pieces (the owner approved the mouth fan blend, R1, and the mouth grass line, round 15); and the road pieces and `house_pad` still read pending although the owner approved their verge fix in round 14 (R6).)
- [x] Placement: ghost preview, rotation, edge glow, legal-slot inventory filter, auto-rotate. (Ghost, frontier slots, edge glow and socket key marks in play (`72133c8`); the Legend sheet lists fitting tiles first and dims the rest (`5950073`); R and auto-rotate step only through distinct legal orientations and keep the chosen one per tile (`93542a6`). Driven end to end by `make ui-e2e`. Owner follow-ups 2026-10-14: the ghost is a see-through preview of the real tile with its own socket colours on the outline, and slot pinning (`4464dfb`); the ghost draws its seeded scatter and its coast layout (`55d22d7`); Shift-click quick place and Ctrl/Cmd-click replace (`d27a627`, save format v3).)
- [x] Staged rise animation (§4.4), including below-surface features, skippable. (`72133c8`: column, cap, edges with water filling channels, props, then agents over 2.0 s; placing again fast-forwards, undo and bulldoze mid-rise are safe, reduced motion cuts to the end. `docs/checkpoints/rise/`.)
- [x] Road network stitching. Cars route between houses across tile boundaries, pulling out of and parking in per-house spots (§7.1). (hex-sim stitches lane endpoints into one road graph and routes cars with A* at a 20 Hz tick; each house owns a car that reverses out of its `anchor-park-<n>` spot and noses into a free one at its destination (`a62c1cc`, `docs/checkpoints/parking/`). Kit cars in three body types with runtime paint (`3dfc734`).)
- [x] GPU boids: birds roosting in forests, and fish schooling in ocean and river water, sharing one boid budget. (`ebf3f53`: 1,000 at High, fish scaled to water tiles up to 300; birds roost on oaks, on the oak tile since the pine and oak split (owner U3, `4464dfb`); fish stay inside each connected water region. `docs/checkpoints/boids/`. Lower-tier caps follow the quality-tier work; the birds and fish draw the approved `bird_starling` and `fish_trout` kit pieces, x4 and x2 real size, each fish in the palette of the water it spawns in.)
- [ ] Water: river flow and ocean waves with shore foam, meeting the §9.1 hero-water quality bar. (Every area of the bar has code and shot evidence (§9.1 "Built so far"). The owner approved the water look-dev on 2026-10-13, and water tiles in play (A5) and the live wet band on kit beaches (A22, `c9a4d03`) on 2026-10-14. Still open: river foam from flow speed reads weak (the A20 audit in the 2026-10-14 owner queue) and needs tuning with the owner.)
- [x] A gate sums the initial download (wasm + IBL + core pieces + sets, one texture family) and fails above 30 MB; hero assets stream after first interaction. (`make checkall-ci` counts the core pack only: 19.47 MiB on ASTC with every MVP family merged, 2026-10-14. Kit families stream by biome pack after the first interaction (`0ca5adb`). A placed tile stays hidden, with a soft outline on its slot, until its pack lands and then rises in the kit look (owner decision A10, 2026-10-14, `docs/checkpoints/streaming/`). The PNG fallback is report only.)
- [x] The owner approves the water look in a dedicated `water` scenario. Shots cover the ocean, a river, a beach and a tile seam, captured at rest and in motion. (Approved 2026-10-13, including underwater cut faces on exposed edges: `--scenario water`, `docs/checkpoints/water/`.)
- [ ] Spatial ambience with the focus mix. Placement and UI SFX. At least 3 Suno music cues mastered and crossfading. (Music 2026-10-14: `meadow-day`, `coast-breeze` and `hearth-town` are mastered and stream after the first gesture, crossfading by the biome mix; `photo-mode` plays in photo mode; `make audio-e2e` checks a stream rises and plays. The owner's audition of the mastered loops is pending. The SFX and ambience are approved in play (owner U5), with forest and meadow one-shots in place of loop beds (`e6896d0`), car sounds (`e047873`, owner R9) and the ambience bus at 10%. Not built: the town murmur and the per-tile placement stingers (§8.1).)
- [x] The owner approves UI mockups (every screen and key interaction state, with motion) before UI implementation starts (§4.7). (Approved 2026-10-13: `design/ui/legend/direction.md`, the full pass in `review.html`.)
- [x] The shipped UI meets the §4.7 quality bar, and the owner signs it off in the running build. (Signed off by the owner in the running build on 2026-10-14, round 14 R7 in `docs/review/2026-10-14-owner-answers.md`, after the A8 follow-ups landed in `4464dfb`: the see-through ghost of the real tile with its socket colours (U1), separate pine and oak tiles (U3), slot pinning (U4) and tile-list socket hexes that turn with the rotation (U6). The photo mode (A14), save slot (A16) and Settings sheet (A21) deviations were accepted the same day.)
- [x] Undo/redo and bulldoze with the connectivity rule. (hex-core world history of at least 100 steps and the split-refusing bulldoze, with unit tests; the HUD's undo, redo and bulldoze, including a refused split, are driven by `make ui-e2e`.)
- [x] Water tiles and kit pieces render in one view within 16 sampled textures per stage. (Fixed 2026-10-13 by dropping Bevy's unused `bluenoise_texture` view binding: kit forward is exactly 16 next to refracting water. Gated by `make web-smoke` case `water_kit`, which clamps the device to 16 and fails at 17. ADR 0002.)
- [ ] GPU timestamp queries attribute frame cost, and the >25 ms stall tail is measured and fixed or accepted with evidence (ADR 0001 carry-over). (Instrumented 2026-10-14: `gpu_timing=on` puts pass-boundary timestamps between Bevy's passes and reports per-pass GPU times; `diag().stall` keeps the window's frame-time histogram and joins each frame over 25 ms to the GPU sample and pipeline compiles around it. Each marker waits on the texture the passes before it wrote, so the segments sum to the frame total; timing-on frames run slower than real ones, so budgets are measured with timing off. Measured 2026-10-14 under the load guard (`docs/perf/p2-passes.json`): the web tail is 6–19 frames over 25 ms per 600, each 30–67 ms, evenly spaced (one every 24–50 frames) and longer with tilt-shift on (39–93 ms). Native has none, and no native GPU frame exceeded 25 ms. Attributed 2026-10-14 on `bf64195` (`docs/perf/stall-tail/`: a Chrome trace and suspect toggles under the load guard): uncapped, the page's frames outrun Chrome's GPU process, about 180 ms of WebGPU work queues up, and every 49–50 frames the compositor's swap waits 48–67 ms for it (`WaitForCommandsToBeScheduled::Dawn`) while the page blocks behind it. GC, wasm memory growth, pipeline compiles, the HUD, state polling and audio are ruled out. At 60 Hz the queue holds one frame and no frame exceeds 25 ms. The `?frame_latency=3` limit bounds frames in flight as wgpu does natively and removes the uncapped tail (0 frames over 25 ms against 8 and 6, +0.3 ms mean). Owner decision Q7 (2026-10-14): the limit is on by default on the web (`web/frame-latency.js`, limit 3; `?frame_latency=N` changes it, `?frame_latency=off` turns it off). The native tail seen on `bf64195` (`docs/perf/p2-passes-2026-10-14.json`: 2–5 frames over 25 ms in 5 of 12 runs) is investigated (`000eb9a`, `docs/perf/stall-tail/native.md`, 27 load-guarded runs): 50 of its 54 frames over 25 ms wait in `get_current_texture` for the window's next drawable, on a roughly 1 s wall-clock grid, at the same rate on an older build, with `--present fifo` and with post off, so the app's own work does not cause it. The best-supported cause, not proven, is other processes' GPU work. Still open: a clean-host native re-run to confirm it, and the owner accepting the native tail.)
- [x] IndexedDB slots, autosave, `.hexworld` export and import. (`796b282`: named slots in IndexedDB, autosave 600 ms after each edit and on page hide, resume on start, `.hexworld` export and import with typed refusals; `make ui-e2e` runs `e2e/persist.mjs`.)
- [x] Photo mode with DOF and 4× export. (`561e368`: free camera, DOF off / tilt-shift preset / manual lens, exposure, FOV, roll, agent pause, and 1×/2×/4× PNG export through an offscreen target; `make shots-photo` writes `docs/checkpoints/photo/`. The panel's look is part of the owner's shipped-UI sign-off. Owner decision A14, 2026-10-14: the web caps export at 2× until 4× is tested for GPU memory, with 4× disabled and explained in the panel; native keeps 4×. Loading a world exits photo mode (A16).)
- [x] Tilt-shift rendering (§9.1) in the game camera: zoom-scaled focal-plane blur, player toggle and strength slider, off by default (owner, 2026-10-14; it was on from the Medium tier). It holds the P2 60 fps 95th-percentile budget on High, measured under the load guard. (Look owner-approved 2026-10-13: the pass, tier scaling and `--tilt` keys are on main; the player toggle and strength slider are in a first Settings sheet, its Tilt-shift section only, and photo mode's tilt-shift preset drives the same pass, 2026-10-14; measured on the 2,000-tile world 2026-10-14 at 2560×1440 on a build from before that slider work, `docs/perf/p2-passes.json`: on the web +0.7 ms mean frame time and p95 3.8–5.1 ms with it on; natively a 0.54 ms GPU segment and p95 12.4–16.5 ms with it on (10.8–11.9 ms off), within the 16.6 ms budget by as little as 0.1 ms, because native frames are bound by presentation or the CPU (GPU total about 4.4 ms against a 7.0 ms mean frame). Re-run on the merged build `bf64195` 2026-10-14 under the load guard (`docs/perf/p2-passes-2026-10-14.json`, 3 rounds, load 6.0–14.5 before and after): web p95 3.9–4.2 ms with it on (3.6–3.8 off, +0.52 ms mean); native p95 12.1–13.3 ms with it on and 12.5–16.4 ms off, GPU segment 0.54 ms, so it holds the 16.6 ms budget.; re-run on the merged build `bf64195` under the load guard, `docs/perf/p2-passes-2026-10-14.json`: native tilt-on p95 12.1–13.3 ms, web p95 3.9–4.2 ms on High.)
- [ ] Grass blades and wind sway (§9.1; owner requests 2026-10-14, added to P2). (Built: grass on High and Ultra at 28 blades per m² with the 0–150% density slider, crossed chunky blades with clump height and colour jitter, grass among wildflowers (`a1c35ef`, `5560267`); the oak, spruces, wildflowers, reeds and cattails sway in the shared `hex_render::wind` field, shadows included (`9ddb35f`). Evidence: `docs/checkpoints/grass/`, `docs/checkpoints/foliage-wind/`. Still open: the owner's look approval of the current grass and the sway, and a load-guarded frame-cost measurement of both in play.)
- [x] Quality tiers with auto-detection. 60 fps at the 95th percentile on the reference machine with a scripted 2,000-tile world on High. (Auto-detection, the `set_tier` command and per-tier boid caps landed 2026-10-14, rules in `docs/agent-operability.md` "Quality tier". The 2,000-tile world on High at 1440p holds 60 fps: web p95 3.4–3.8 ms over 3 load-guarded runs (`docs/perf/p2-passes.json`; gate2000, instanced, mid600 shadows). The Settings sheet's Quality list (Auto, Low, Medium, High, and Ultra where detection lands on High) sends `set_tier` and round-trips through the save's quality, 2026-10-14. A quality change shows a loading panel until the rebuilt tier draws (`a6b3a1f`, owner report 2026-10-14). Still open: the §9.2 resolution scale and per-tier shadow cascades, Medium's half-resolution AO and Ultra's extras (§9.2 "As built"). The 2,000-tile measurement is the `gate2000` scenario, which draws no grass; grass and foliage sway in play are not yet measured under the load guard.)
- [x] Every §9.4 hook is present in the MVP assets and shaders (verified by the asset gate). (`6cd0e3e` plumbed every hook inert at the MVP values; `4af4173` authored the last 21 prop slots' masks, so `kit_gate`'s hooks module lists no known gaps. It checks every manifest slot and shader, driven from the manifests and shader directories. `docs/kit/hooks-contract.md`, `docs/checkpoints/hooks/`.)
- [x] Every audio asset's provenance records its provider and subscription (D21). (`make audio-check` fails any mastered cue whose provenance lacks them; all 20 mastered SFX and ambience cues in `assets/audio/manifest.json` (the four car cues of `e047873` included) record ElevenLabs and the subscription tier, 2026-10-14; the retired `forest-birdsong` and `meadow-bed` loops keep their records for provenance. The six Suno music cues record Suno v6 and the owner's Suno Pro subscription with each song id, mastered 2026-10-14; their WAV sources stay in the git-ignored local store and provenance holds their SHA-256 and size.)

### P3 — Progression and farm/rail content

- [ ] Recipe system, recipe book with hints, discovery moment, creative-mode toggle.
- [ ] Farm (animals), windmill, rail straight/bend/junction, station, bakery, and at least 6 recipes. (The windmill piece is built, `e5ef3c9`, and the windmill tile is in the inventory ahead of the recipes, owner request 2026-10-14: sails turning in play, `docs/checkpoints/windmill/windmill-play.jpg`. The farm tile is in the inventory ahead of P3 with static animals, owner request 2026-10-19: the approved barn, hay bales, cow and sheep on a meadow base, `docs/checkpoints/farm/farm-play.jpg`. The paddock fence is in: `fence_paddock`, a post-and-rail paddock round the animals with a 3 m gate toward the yard, blocking the walker run by run (`docs/checkpoints/farm/farm-walk-fence.jpg`), piece approval pending (`docs/checkpoints/fence_paddock/sheet.jpg`). The wheat field tile is in the inventory too, owner request 2026-10-19: `docs/checkpoints/wheat_field/sheet.jpg`, piece approval pending. The farm gained a grazing cow, a lamb and a grazing sheep and hay bale variants, 2026-10-20, approvals pending. Rail is modelled but not yet in play: track straight and 60° bend, the level crossing, the junction, the station platform and shelter, the steam tank engine and carriage, and the river rail bridge (2026-10-19 and 20, approvals pending; runtime on cards `01a1579e911c` rail network and `01a157994d7a` station tile).)
- [ ] Trains route between stations across tile boundaries. Farm animals wander within their farm. Boats on ocean and rivers.
- [ ] Bridges and level crossings where rail meets road or river.

P3 references are owner-approved (A17, 2026-10-14, `assets-source/refs/<id>/approval.json`). The windmill, the farm pieces (barn, hay bales, cow and sheep, the paddock fence and their variants), the wheat field, the bakery and the rail pieces are modelled. The decisions: rail slightly wider than standard gauge, single track, bending across adjacent edges (60°), with a 60° level crossing and field gates; a side-platform station about 20 m long with the ref-1 shelter; a steam tank engine with runtime body colour and two carriages; a pitched-roof red board barn with wheat; hornless brown-and-white cows and black-faced sheep in ref-2's smooth form; a white windmill on a flat tile with animated sails and a still fantail; the bakery as its own tile with a road through it; and the O4 skirt on every P3 tile.

### P4 — Elevation

- [ ] Height levels 0–3, with `Cliff` and `Ramp` edge profiles and height-aware socket rules.
- [ ] Hills and mountain tiles. Rivers that flow downhill (river lanes carry a flow direction). Waterfalls at cliff edges.
- [ ] Roads and rails need ramps between levels.

### P5 — Atmosphere

- [ ] Day/night cycle: sun and moon, sky model, emissive night lighting, vehicle headlights.
- [ ] Weather: rain, snow, fog and storms with GPU particles, wetness and snow accumulation using the §9.4 masks.
- [ ] Seasons: foliage tint, snow cover, frozen rivers. Adaptive music stems by mood and time of day.
- [ ] Wind that follows the season and the weather (owner, 2026-10-14: "vary the strength of the wind depending on season and or weather"; not needed now). Wind strength, gusts and direction change with the season and the weather instead of today's fixed constants. The shared `hex_render::wind` field (`crates/hex-render/src/wind.wesl`) already drives the grass and the foliage sway (trees, wildflowers, reeds, cattails), so its prevailing direction and layer amplitudes become inputs the season and weather set. Water ripples could later read the same field.

### P6 — Content breadth

- [ ] Factories, volcano (with an eruption signature moment), craters, mountains, desert, tundra, swamp, towns with pedestrians, harbors, and more.
- [ ] Lighthouse: a coastal tile with a lighthouse whose light beam sweeps around, animated as a rotating lamp and beam that reads at dusk and night (owner, 2026-10-14).
- [ ] A goal of 100+ tile types from the kit, with recipes covering every non-starter type.

### P7 — Mobile and tablet

- [ ] Touch gestures mapped onto the input-action layer, a responsive UI, and a mobile quality tier.
- [ ] 30+ fps on iPad Air (M1) and a recent iPhone with Safari WebGPU.

### P8 — First-person stroll and drive

- [x] A walk controller using the per-tile colliders (owner, 2026-10-14: "add a first person mode so i can walk around the map"). F or the bar's Walk button enters at eye level (1.7 m) on the ground nearest the camera focus; WASD walks at 2.8 m/s, Shift sprints at twice the walk speed (5.6 m/s), Space hops, and the mouse looks (the pointer lock on the web). The walker follows the kit ground (the topmost LOD0 surface of each tile's ground pieces), is stopped by the house, oak, spruce and rock colliders, by water deeper than 0.6 m and by the map edge, and leaving restores the orbit camera. The walk speed is a per-viewer setting from 2.8 to 8.0 m/s, the Walk speed slider in the Settings sheet's World tab, and sprint is always twice it (owner, 2026-10-14: "double walk and sprint speed, always keep sprint double walk and add a walk speed setting with minimum at the new double speed"). Evidence: `crates/hex-play/src/walk.rs` tests (ground on meadow, road, beach and river banks; a house wall on a turned tile; tree trunks; the shoreline and the map edge), the `make ui-e2e` walk steps, and the eye-level stills in [`docs/checkpoints/first-person/`](docs/checkpoints/first-person/) (`make shots-first-person`). Controls and commands: [`docs/agent-operability.md`](docs/agent-operability.md#walk-mode).
- [ ] A drive mode on the road network.
- [ ] Close-range LOD0 and texture quality pass verified in first person.

## 16. Risks

| Code | Risk | Mitigation |
|---|---|---|
| R1 | KTX2/meshopt loading in Bevy on wasm is unproven (TC3). | P0 spike with the routes in §9.3. |
| R8 | Bevy's web post stack (TAA, SSAO, DOF) fails or underperforms on browser WebGPU, or its API churn breaks custom nodes (TC5). | P0 engine gate with the custom wgpu fallback (D2). Exact version pin; upgrades as deliberate tasks. |
| R2 | AAA quality at 2,000 tiles in a browser exceeds the frame budget. | Instancing, GPU culling, chunked LOD, quality tiers, and perf gates from P2 onward. |
| R3 | Kit seams are visible between modular pieces. | A strict edge contract, normal blending at the hex border, and a seam gate in P1. |
| R4 | Asset throughput: high-poly + bake per piece is slow. | Scripted generators, a shared helper library, and family-level reviews. |
| R5 | Single-threaded WASM limits simulation scale (TC1). | Simulation LOD, GPU compute for flocks, and a worker fallback (§7.3). |
| R6 | Safari or Firefox WebGPU differences cause rendering bugs. | Local browser e2e tests in each supported browser before releases. |
| R7 | Bundle size grows past Pages limits. | Size budgets enforced in `checkall-ci`, plus streaming biome packs. |

## 17. Open questions

| Code | Question | Decide by |
|---|---|---|
| Q1 | Resolved: R = 16 m and a 4 m level step, finalized in P1 (§5.1). | P1 look-dev |
| Q2 | Deferred by the owner in P1: `.blend` sources stay in the local, gitignored blend store, referenced by SHA-256 (§11.6). The external store (R2 bucket vs. other) is still open. | P1 |
| Q3 | Resolved: pure-Rust `meshoptimizer-rs` GLB rewrite plus BC7/ASTC KTX2 + zstd with a PNG fallback (§9.3, [`docs/decisions/0001-engine.md`](docs/decisions/0001-engine.md)). | P0 |
| Q6 | Provisional: Bevy retained (D2, [`docs/decisions/0001-engine.md`](docs/decisions/0001-engine.md)). The Safari 27 startup failure is diagnosed and worked around (`web/wgsl-compat.js`), and Safari renders. Paced p95 still misses 17.0 ms in Chrome and in Safari close-up. | P0 |
| Q4 | Resolved: the `rich-3` grade is the default (owner, 2026-10-14, after the grade comparison): ACES fitted at EV100 12.5 with a +0.013 linear shadow lift before the curve. ACES fitted, chosen at the 2026-10-13 look-dev, clipped walls facing away from the sun to black; TonyMcMapface at EV100 12.9 (owner, 2026-10-14) kept the walls readable but read washed out, and stays selectable as `tony` (D4, [`docs/decisions/0002-asset-pipeline.md`](docs/decisions/0002-asset-pipeline.md)). | P1 |
| Q5 | Resolved: the PRD is not registered in par-prd. `PRD.md` in this repo is canonical. | Owner |

## 18. Engine research summary (2026-10-12)

- Bevy 0.20.0 (released 2026-10-08, wgpu 30) is current. Bevy's WebGPU showcase demonstrates bloom, cascaded shadows, contact shadows, deferred rendering, volumetric fog, atmosphere, transmission and decals in the browser (https://bevy.org/examples-webgpu/).
- TAA, DOF and SSR compile with web-specific workarounds but are not in the showcase. SSAO is disabled at runtime when the device allows fewer than 5 storage textures per stage (desktop adapters normally allow more). Irradiance volumes, meshlets and mesh shaders are off on wasm32.
- On wasm32 Bevy is single-threaded, has no pipelined rendering, and compiles pipelines synchronously (TC5).
- Bevy's measured wasm floor is about 24 MB raw, 7.6 MB gzip, 5.2 MB brotli. `~/Repos/suck-it-up` ships Bevy 0.19.1 to GitHub Pages at about 9.8 MB gzip, with custom WGSL materials and compute passes proven on WebGPU.
- A custom wgpu 30 renderer is viable (mature WebGPU backend; `gltf`, `ktx2`, `ruzstd`, `glam` crates) but has no maintained renderer to build on (rend3 archived, three-d is WebGL2 only). Estimated 3–5 person-months for the PBR + post stack at production quality.
- Fyrox and Blade have no WebGPU web path.
- Decision: Bevy first, because it already supplies PBR, shadows, post effects, assets and the ECS. The P0 engine gate decides whether that holds.
- P0 engine gate measured (2026-10-12, [`docs/decisions/0001-engine.md`](docs/decisions/0001-engine.md)): instanced layout, Chrome uncapped p95 strategic / close-up of 3.7 / 3.7 ms with tuned shadows, 3.8 / 4.0 ms with mid600, 4.3 / 4.3 ms with mid1200 and 22.0 / 38.6 ms (fail) with full shadows. Paced 60 Hz p95 is 17.3–17.5 ms, which fails the pre-set 17.0 ms. Safari 27.0.1 quit at frame 3 on a shader its compiler rejects (Bevy's `let` handle aliases need `texture_and_sampler_let`). With `web/wgsl-compat.js` it renders, and its paced p95 is 17.0 / 18.0 ms (strategic / close-up) at 1 ms timer resolution, with means of 16.67 / 16.69 ms. wasm is 8.46 MB gzip and READY takes 1.2–1.4 s (localhost; live on Pages 2026-10-13: 9.2 MB total over the wire, READY 9.6 s on one cold load, `docs/perf/p0-deploy.json`).

