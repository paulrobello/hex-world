# Hex World — Product Requirements Document

| Field | Value |
|---|---|
| Product | Hex World |
| Repo | `paulrobello/hex-world` (public, MIT) |
| Hosting | GitHub Pages, custom domain `hex-world.pardev.net` |
| Owner | Paul Robello |
| Status | Draft v1 — brainstorm complete, awaiting review |
| Date | 2026-10-12 |

## 1. Summary

Hex World is a browser-based world-building game. The world starts as a single hex tile. The player picks tiles from an inventory, rotates them, and places them next to existing tiles where the edges are compatible. Each placed tile rises out of the ground in a staged assembly animation, and the world comes alive: cars drive the roads, trains cross tile boundaries, rivers flow, windmills turn, animals roam farms, and birds flock overhead.

The target is a "stylized-real diorama" at AAA visual quality: real PBR materials on slightly exaggerated miniature proportions. It is rendered by a custom Rust + wgpu engine compiled to WebAssembly on WebGPU. Every 3D asset is authored high-poly in Blender, baked to PBR textures, and reduced to runtime LODs. Sound effects and ambience come from ElevenLabs. Music comes from Suno, using a prompt pack kept in the repo.

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
| D2 | Engine: Rust + wgpu → WASM, WebGPU required. No WebGL2 fallback, so compute shaders may be used freely. |
| D3 | Adjacency: typed edge sockets on all 6 edges, with a compatibility table. Rotation rotates the sockets. |
| D4 | Inventory: every unlocked tile type is unlimited. The panel filters or highlights tiles that are legal at the hovered slot. |
| D5 | Elevation: discrete height levels (target 0–3) with cliff and ramp sockets. The MVP ships at a single level, and the schema reserves the height fields from day one. |
| D6 | Agents: vehicles route on stitched road, rail and river graphs between real destinations. Animals wander within their biome, and birds flock across the map freely. |
| D7 | Atmosphere: photo mode ships in the MVP. Day/night, weather and seasons come later, but MVP assets carry their hooks (§9.4). |
| D8 | Platforms: desktop first. Mobile and tablet come in a later phase, so input and UI are designed touch-ready. |
| D9 | Persistence: IndexedDB autosave with multiple slots, plus `.hexworld` file export and import. No backend. |
| D10 | Art direction: stylized-real diorama. |
| D11 | Tile composition: a hex base, 6 edge pieces, a center piece and scattered props, assembled from data-defined kit pieces. |
| D12 | MVP content: about 10 tiles covering grass/meadow, forest, house, road, river, beach and ocean. Live agents are cars and birds. |
| D13 | Audio: Suno music via a repo prompt pack. The owner generates the tracks, and repo scripts master and validate them. ElevenLabs makes the SFX and ambience, which are spatial and mixed by camera focus. |
| D14 | Camera: orbit, pan and zoom. First-person stroll and drive come later, so assets need close-range LOD0 quality and per-tile colliders from day one. |
| D15 | Placement animation: staged assembly over about 1.5–2.5 s, skippable by placing the next tile. |
| D16 | Scale: about 2,000 tiles at 60 fps on desktop. |
| D17 | Asset authoring: Opus 5.5 subagents. Structural pieces come from scripted headless Blender generators, and organic hero pieces are made interactively through the Blender MCP with `.blend` sources kept. |
| D18 | UI: an HTML/CSS overlay (TypeScript) over the wgpu canvas, talking to Rust through wasm-bindgen. |
| D19 | Name "Hex World". Public repo under MIT. Deployed to GitHub Pages at `hex-world.pardev.net`. |
| D20 | Players get an undo stack and a bulldoze tool. Bulldozing is blocked if it would split the world into disconnected islands. |

### Assumptions (confirm or correct)

| Code | Assumption |
|---|---|
| A1 | The world's first tile is grass/meadow. |
| A2 | Farm (with animals), windmill and rail/trains are not in the MVP. They ship in P3 alongside recipes. |
| A3 | No WASM threads (TC1). Heavy simulation runs in a dedicated Web Worker running a second WASM instance, or on the GPU. |
| A4 | High-poly `.blend` files and bake intermediates are kept out of git. Scripted pieces are rebuilt from their generators, and interactive hero `.blend` files live in external storage (§11.6). Only runtime derivatives are committed. |
| A5 | The owner's Suno and ElevenLabs plan terms allow redistributing generated audio in a public repo and on a public site. This is verified before the first audio commit and recorded in each asset's provenance. |

### Technical constraints

| Code | Constraint |
|---|---|
| TC1 | GitHub Pages cannot set COOP/COEP headers, so `SharedArrayBuffer` and WASM threads are unavailable. The `coi-serviceworker` workaround exists, but it is not used by default (it breaks on first load and complicates caching). |
| TC2 | GitHub Pages limits: the published site should stay under 1 GB, individual files under 100 MB, and soft bandwidth is about 100 GB/month. The runtime asset payload has a hard budget (§10). |
| TC3 | par-factory's runtime decoders (meshopt and KTX2 through three.js JS decoders) do not carry over. Rust/WASM decoding of KTX2/Basis and meshopt must be proven in P0. |
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
- The inventory offers auto-rotate: when a slot is hovered, the ghost snaps to the first compatible rotation, and the player can cycle through the other legal rotations.
- Undo (Ctrl/Cmd+Z) reverts placements and bulldozes in LIFO order. Redo is supported. The stack depth is at least 100 actions and persists with the save.
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
| Place | Left-click |
| Rotate ghost | R / Shift+R, or Ctrl+wheel |
| Undo / redo | Ctrl/Cmd+Z / Ctrl/Cmd+Shift+Z |
| Bulldoze tool | B |
| Inventory | Tab |
| Photo mode | P |

Every action routes through an input-action layer, so touch gestures (P7) and a first-person controller (P8) map onto the same actions.

### 4.6 Photo mode (MVP)

- A free camera that is detached from the orbit constraints.
- Depth of field (focus distance and aperture, with a tilt-shift preset), exposure, FOV and roll.
- Hide UI, pause agents, and set the time of day once day/night exists (P5).
- Export PNG at 1×, 2× or 4× resolution through an offscreen render target.

## 5. Tile system

### 5.1 Grid

- Pointy-top hexes with axial coordinates `(q, r)`. Storage is a hash map from coordinate to placed tile.
- World scale is real meters, so first person (P8) works without rescaling. Hex circumradius R = 16 m (about 27.7 m flat-to-flat), and the height level step is 4 m. Both values are constants, finalized during the P1 scale review.
- Edge indices 0–5 run clockwise from east. Rotation `k` maps edge `e` to `(e + k) mod 6`.

### 5.2 Sockets

Each edge carries a socket descriptor:

```
EdgeSocket {
  kind: SocketKind,        // terrain/feature type at this edge
  lane: Option<LaneSpec>,  // road/rail/river crossing: offset along edge, width, direction
  height: u8,              // edge height level (0 in MVP; reserved for D5)
  profile: EdgeProfile,    // Flat | Cliff | Ramp (MVP: Flat only)
}
```

- `SocketKind` (MVP): `Grass`, `Forest`, `Road`, `River`, `Beach`, `Ocean`. Planned later: `Rail`, `Farm`, `Cliff`, `Mountain`, `Lava`, `Town` and others.
- A data table (`data/sockets.ron`) defines which socket kinds may meet, symmetrically. For example, `Road`↔`Road` is legal, `Road`↔`Grass` is not (roads cannot dead-end into an edge), `Beach`↔`Ocean` and `Beach`↔`Grass` are legal, and `Ocean`↔`Grass` is not (coasts need a beach).
- Lane sockets must also match in lane offset and width, so roads and rivers line up exactly across the seam.
- Height rules (P4): edges must have equal height, unless the edge profile is `Cliff` (any difference) or `Ramp` (a difference of exactly 1).

### 5.3 Tile definitions (data)

Tile types live in `data/tiles/*.ron`:

```
TileDef {
  id: "road_bend_60",
  biome: Grass,
  base: "base_grass_h0",
  edges: [EdgeSocket; 6],
  center: Option<KitRef>,            // junction, building pad, lake bowl, …
  edge_pieces: [Option<KitRef>; 6],  // usually derived from sockets
  props: PropScatter,                // seeded scatter rules + fixed hero props
  networks: [NetworkSegment],        // road/rail/river graph edges inside the tile
  emitters: [AudioEmitter],
  agents: [AgentSpawnRule],
  rise: RiseTimeline,
  unlock: Unlock,                    // Default | Recipe(id)
}
```

A build-time validator checks every tile definition: socket and lane consistency with its network segments, existence of every referenced kit piece, and the presence of every rotation's thumbnail.

### 5.4 MVP tile list (D12)

About 10 types. The exact variants are frozen at the start of P2.

| # | Tile | Notes |
|---|---|---|
| 1 | Meadow | The starting tile. Grass on all edges, with seeded wildflower and rock scatter. |
| 2 | Forest | Grass edges and dense tree scatter. Bird roost spawn. |
| 3 | House cluster | Grass edges plus 1 road edge, with 2–3 houses. A car destination. |
| 4 | Road straight | Road on opposite edges. |
| 5 | Road bend | Road on edges 60° or 120° apart. |
| 6 | Road junction | A 3-way junction. |
| 7 | River straight | River lanes on opposite edges. The bed is cut below the surface. |
| 8 | River bend | Lanes 120° apart. |
| 9 | River + road bridge | A road crossing a river: shows bridges and cross-network seams. |
| 10 | Beach | A mix of grass, beach and ocean edges. Surf foam. |
| 11 | Ocean | Ocean on all edges. Deep water with a visible shelf below the surface. |

## 6. Modular asset kit (D11)

- **Bases** are the hex column (side skirt with visible soil strata) plus a terrain cap per biome and height. One base mesh per biome is shared through instancing.
- **Edge pieces** are 6 per tile, chosen by socket: road stub, river mouth, coast transition, grass blend, and later cliff and ramp pieces. They are authored for edge 0 and rotated at runtime.
- **Center pieces** are junctions, bends, building pads, lake bowls and river beds. Below-surface geometry (river beds, crater bowls, coast shelves) belongs to the center or edge piece, and the hex column is cut to fit it.
- **Props** are trees, rocks, fences, houses, windmills, lamps and so on. They are placed by seeded scatter rules (density, exclusion masks around lanes) plus fixed hero placements.
- **Assembly.** At placement the runtime resolves the tile definition into a list of kit-piece instances. Static pieces are batched into per-chunk GPU instance buffers.
- **Seam continuity.** Kit pieces snap to a shared edge contract (lane offsets, edge heights, a vertex ring at the hex border), and the terrain cap's normals are blended at the hex border so seams don't show.
- **Variety.** Each kit piece has 2–4 variants, and props get a per-instance seed for hue, scale and rotation jitter.

## 7. Living world

### 7.1 Networks

- Road, rail (P3) and river segments inside each tile are spline graphs with endpoints at the edge lanes. On placement, endpoints are stitched to the neighbors' matching endpoints to form a global graph per network type.
- Destinations are tagged nodes: houses, later stations, farms and factories.

### 7.2 Agents

| Agent | Behavior | Phase |
|---|---|---|
| Car | Routes between destination nodes on the road graph (A* over a cached graph) and follows lane splines. Spawn rate scales with connected destinations. | P2 |
| Bird | Boids flocking over the whole map, roosting in forest tiles. Runs on GPU compute. | P2 |
| Boat | Drifts on ocean tiles and travels connected rivers. | P3 |
| Train | Routes between stations on the rail graph. Multi-car consists that cross tile boundaries. | P3 |
| Farm animals | Wander within their farm tile's bounds, using idle, graze and run states. | P3 |
| Pedestrians | Walk sidewalk lanes in towns. | P6 |

- Agent caps scale with the quality tier (reference target: 300 vehicles, 500 animals and 1,000 birds at High).
- Agents are simulated at a fixed 20 Hz tick and interpolated at render time. Off-screen agents drop to a coarse simulation LOD.
- Skinned creatures use baked animation (vertex animation textures or a bone-transform atlas) so they can be GPU-instanced. No per-agent CPU skinning.
- Animated props (windmill blades, water wheels, flags) use rigid object transforms or vertex animation textures.

### 7.3 Simulation architecture

The deterministic simulation core (grid, sockets, networks, pathing, agents) runs in Rust. Rendering-only motion (boids, foliage sway, water) runs on GPU compute. If profiling shows the main thread is over budget, the simulation moves into a Web Worker running its own WASM instance, with state messages over `postMessage` (TC1).

## 8. Audio (D13)

### 8.1 SFX and ambience (ElevenLabs)

- Generated through the `elevenlabs` and `create-game-audio-assets` skills.
- **Per-tile emitters:** river babble, surf, forest birdsong, town murmur, and later the windmill creak and farm animals. Emitters are positional and attenuate with distance.
- **Focus mix:** emitter gain is weighted by the camera's focus point and zoom, so zooming in on a tile brings up its sounds. A global bed comes from the biome mix currently in view.
- **Placement:** a shared rumble and thunk, plus a short stinger per tile type.
- **UI:** select, rotate, invalid placement, undo, bulldoze and recipe-discovery cues.
- Uses the Web Audio API (HRTF or equal-power panning) through a thin TypeScript audio layer driven by events from Rust.

### 8.2 Music (Suno)

- `audio/music/prompts/*.md` holds one file per cue, with style tags, BPM, key, mood, instrumentation, structure tags, an instrumental flag, a loop intent and a target length.
- The owner generates the tracks in Suno and drops the WAV or MP3 into `assets-source/audio/music/<cue-id>/`.
- `make audio-master` normalizes loudness (music −18 LUFS integrated), sets loop points, encodes the runtime format, and writes the provenance JSON (the Suno plan tier confirmed against A5).
- The playlist crossfades between cues by world mood (the biome mix in view). Adaptive stems are a P5 option.

### 8.3 Runtime formats

- Opus in WebM is the primary runtime format, with an AAC (m4a) fallback for Safari if it is needed. Ambience loops are sample-accurate and gapless.
- Loudness targets follow par-factory: music −18 LUFS, SFX and loops −16 LUFS, UI cues −20 LUFS, with a true-peak ceiling of −1 dBTP.

## 9. Rendering (D2, D10)

### 9.1 Pipeline

- Rust + wgpu on the WebGPU backend, compiled to `wasm32-unknown-unknown` with wasm-bindgen.
- A clustered-forward PBR pipeline (metallic-roughness, glTF-conformant) with an HDR target.
- Lighting: a directional sun with cascaded shadow maps (3–4 cascades, PCF/PCSS soft shadows), image-based lighting from a prefiltered environment map plus irradiance, and SSAO (GTAO). Screen-space GI is evaluated in P5.
- Water: shared shaders for rivers and ocean, with flow maps on rivers, Gerstner or FFT waves on the ocean, depth-based color absorption, shoreline foam, and screen-space refraction plus planar or SSR reflection.
- Foliage: GPU-instanced with wind sway in the vertex shader. Alpha-tested leaves with dithered LOD transitions.
- Post-processing: TAA, bloom, filmic tone mapping (AgX or ACES, chosen during the P1 look-dev), color grading by LUT, and DOF (used by photo mode).
- Culling: GPU frustum and Hi-Z occlusion culling with indirect draws, organized in chunks (for example 7- or 19-hex clusters).
- LOD: three mesh LODs per kit piece with distance and screen-size selection and dithered cross-fade. Far chunks use merged meshes.

### 9.2 Quality tiers

| Tier | Resolution scale | Shadows | AO | Water | Agent caps |
|---|---|---|---|---|---|
| Low | 0.67 | 2 cascades, 1k | off | no refraction | 25% |
| Medium | 0.85 | 3 cascades, 2k | half-res | SSR off | 50% |
| High | 1.0 | 4 cascades, 2k | full | full | 100% |
| Ultra | 1.0 + TAA upsampled detail | 4 cascades, 4k | full + bent normals | full + planar | 100% |

The tier is auto-detected on first run from adapter info and a short benchmark, and the player can override it.

### 9.3 Asset formats at runtime

- glTF 2.0 binary (GLB) with `EXT_meshopt_compression` and `KHR_texture_basisu` (KTX2: UASTC for normals, ETC1S or UASTC+RDO for color maps). The browser transcodes to BC7 or ASTC depending on what the adapter supports.
- The decoders are chosen in P0 (TC3). Options: (a) Rust-native decoders compiled into the main WASM; (b) the official Basis Universal transcoder WASM and the meshoptimizer decoder JS/WASM, called from TypeScript, with the decoded buffers handed to Rust; (c) a build-time conversion into a custom pre-transcoded binary format, per target compression family, served by feature detection.

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

## 10. Performance and size budgets

| Item | Budget |
|---|---|
| Frame time on the reference desktop (Apple M-series, High tier, 1440p, 2,000 tiles) | ≤16.6 ms at the 95th percentile |
| Main-thread simulation tick | ≤4 ms |
| Draw calls after instancing and indirect draws | ≤500 |
| Initial download before first interaction | ≤25 MB (WASM + core kit + UI) |
| WASM binary (after `wasm-opt -Oz`, Brotli) | ≤6 MB |
| Total deployed site at the full game | ≤600 MB (TC2) |
| Kit piece LOD0 | ≤4k tris typical, ≤12k for hero pieces (landmarks, volcano) |
| LOD1 / LOD2 | ≤50% / ≤15% of LOD0 |
| Prop LOD0 | ≤1.5k tris (trees ≤3k, with leaf cards) |
| Textures | ≤2k per kit-piece atlas (KTX2) |
| Per-GLB size | ≤2 MB |
| Audio decoded in memory | ≤96 MB |

Kit content streams in by biome pack after the first interaction.

## 11. Asset pipeline (D17)

This adapts par-factory's pipeline (`~/Repos/par-factory/docs/specs/09-asset-pipeline.md`). Where par-factory routes modeling to a `gpt-6-astra` subagent, Hex World uses Opus 5.5 subagents.

### 11.1 Authoring routes

- **Scripted (structural):** bases, edge pieces, road, rail, river and coast pieces, buildings. An Opus 5.5 subagent writes a reproducible `scripts/blender/kit/<piece-id>.py` generator and runs it headless (`blender -b -P`). Rebuilding from the script must be deterministic.
- **Interactive (organic heroes):** mountains, volcano, cliffs, animals, hero trees. An Opus 5.5 subagent sculpts through the Blender MCP in a live session. The `.blend` source is saved to external storage (A4), and the piece is exported through the same scripted bake and LOD stage.

One subagent works on one kit piece at a time. Pieces are built serially per family, and the owner reviews each family together.

### 11.2 Stages

1. **High-poly:** sculpt or modifier-stack geometry at final detail (no tri budget). Bevels, damage and surface detail are modeled.
2. **Low-poly cage / LOD0:** a retopologized or decimated mesh within budget, with clean UVs (one atlas per piece) and a texel density standard decided in P1.
3. **Bake:** high-poly → LOD0 using Cycles bakes: base color, tangent-space normal, ORM (AO / roughness / metallic), emissive, plus the hook masks (§9.4).
4. **LODs:** LOD1 and LOD2 through Blender Decimate (with the normal map reused), plus a simplified collider mesh (D14).
5. **Export:** a GLB with named nodes, sockets (`socket-lane-<n>`, `anchor-prop-<n>`), a pivot at the hex center or edge-0 midpoint, and Y-up.
6. **Compress:** gltf-transform (pinned version), KTX2 (UASTC normals, ETC1S/UASTC color), meshopt quantization, validate, and a hard size-budget failure.
7. **Gate:** reimport check (geometry, sockets, materials, LOD ratios, budget), a 6-view render, a seam test against neighbor pieces, an in-engine screenshot, and a provenance JSON. Then owner approval.

### 11.3 Directory layout

```
assets-source/
  kit/<piece-id>/               # generator output reports, renders, provenance (no .blend, see A4)
  audio/{sfx,ambience,music}/<cue-id>/
scripts/blender/kit/<piece-id>.py
scripts/blender/lib/            # shared bake/LOD/export helpers
audio/music/prompts/<cue-id>.md
data/{tiles,sockets.ron,recipes}
web/public/assets/              # runtime GLB/KTX2/audio + manifest.json
```

### 11.4 Provenance

Every asset gets `provenance.json` recording: the generator script hash or `.blend` storage reference, the authoring agent and model, Blender version, bake settings, tool versions (gltf-transform, KTX), source and runtime hashes, the license, and for audio the provider, plan tier and terms-check date (A5).

### 11.5 Make targets

`make kit-build PIECE=<id>`, `make kit-check`, `make kit-compress`, `make audio-master`, `make audio-check`, `make assets-manifest`. `make checkall` runs the asset gates for any staged assets.

### 11.6 Large-file storage

Runtime derivatives are committed directly (each under the budget). Interactive `.blend` sources and bake intermediates go to external storage (an R2 bucket or similar, decided in P1) and are referenced by hash in the provenance. Git LFS is not used for them because of quota limits.

## 12. Technical architecture

### 12.1 Workspace

```
crates/
  hex-core/      # grid, sockets, tile defs, placement rules, undo, save model (no GPU deps; native-testable)
  hex-sim/       # networks, pathing, agents, recipes (deterministic, fixed tick)
  hex-render/    # wgpu renderer, materials, culling, LOD, post, photo mode
  hex-assets/    # GLB/KTX2 loading, kit manifest, streaming
  hex-app/       # wasm entry, frame loop, input actions, JS bridge (wasm-bindgen)
web/             # Vite + TypeScript UI overlay, audio layer, IndexedDB, unsupported-browser page
data/            # tile/socket/recipe RON, validated at build
scripts/         # blender, audio, manifest, deploy
```

- `hex-core` and `hex-sim` compile natively and are covered by `cargo test`. Rendering is isolated in `hex-render`.
- A native desktop build of `hex-app` (wgpu with Metal) is kept working for fast iteration, profiling and screenshot tests.
- The JS bridge is a typed command/event API (`placeTile`, `rotateGhost`, `undo`, events such as `tilePlaced`, `recipeUnlocked`, `audioEmitterUpdate`), with TypeScript types generated from Rust.

### 12.2 Save format

- A versioned, compact binary save (postcard or bincode) containing the tile map, rotations, seeds, unlocks, the undo stack, the camera, and settings. Agents are re-derived on load, not saved.
- Autosave to IndexedDB runs after every placement (debounced). There are multiple named slots.
- A `.hexworld` export is the save blob plus a magic header and a version. Import validates the file and migrates older versions.
- A schema contract document lists migrations, and tests load every historical version.

### 12.3 Agent operability

Following `~/.claude/guides/agent-operability.md`, both the native and web builds support:

- `?shot=<scenario>` and `--shot`: load a scripted scenario, render, and save a PNG.
- `?diag=1`: an overlay with frame timings, draw calls, agent counts and GPU memory.
- A deterministic scenario runner (seeded world plus a placement script) for e2e and perf tests.

## 13. Deployment

- GitHub Actions on push to `main`: build the WASM (release, `wasm-opt`), build the web bundle, run the gates, and deploy to Pages with `actions/deploy-pages`.
- A `CNAME` file containing `hex-world.pardev.net`, plus a Cloudflare DNS CNAME record pointing to `paulrobello.github.io` (DNS-only, so GitHub can issue the TLS certificate). HTTPS is enforced.
- Fingerprinted asset filenames with long cache lifetimes. The manifest has a short cache lifetime.
- An unsupported-browser page (served when WebGPU feature detection fails, TC4) that names the supported browsers.
- `pardev.net` is a Cloudflare-managed domain (`~/.claude/guides/infrastructure.md`).

## 14. Quality gates and testing

- `make checkall` (fast): `cargo fmt --check`, `clippy -D warnings`, `cargo test` (core and sim), TypeScript typecheck, lint and unit tests, data validation (tiles, sockets, recipes), and asset gates for staged assets.
- `make checkall-ci`: everything in `checkall` plus the WASM release build, the bundle-size budget, and the deploy-artifact check.
- Browser e2e and perf tests run locally with Playwright on GPU Chromium (`channel: 'chromium'`, Metal ANGLE flags) and are never run in CI. They cover a placement scenario, an undo/bulldoze scenario, a save/load round trip, a photo-mode export, and a 2,000-tile perf scenario.
- Visual regression: `?shot=` scenarios compared against approved baselines with a perceptual tolerance.
- Unit-tested core rules: socket compatibility, rotation, legal-slot computation, bulldoze connectivity, undo/redo, save migration, and network stitching.

## 15. Phases

Each phase ends with every acceptance criterion verified, with evidence, and is not done until then.

### P0 — Foundations spike

Goal: de-risk the stack before content work.

- [ ] Rust workspace and Vite web shell build. `make checkall` and `make checkall-ci` exist and pass.
- [ ] wgpu runs on the WebGPU backend in Chrome and Safari 26, rendering a lit PBR hex mesh. The native build renders the same scene.
- [ ] One KTX2 + meshopt GLB is loaded and rendered in the browser. The decoder route (§9.3 a/b/c) is chosen and recorded with measured size and load time.
- [ ] The unsupported-browser page is shown when WebGPU is missing.
- [ ] GitHub Actions deploys to Pages, and `https://hex-world.pardev.net` serves the build over HTTPS.
- [ ] The `?shot=` and `?diag=1` hooks work.

### P1 — Asset pipeline proof

Goal: one kit family through the full pipeline at final quality.

- [ ] Shared Blender helper library: bake (all maps plus hook masks), LOD, collider, export, compress.
- [ ] Scripted pieces: meadow base and road straight edge/center, each passing every gate in §11.2. Interactive route: one hero tree through the Blender MCP, also passing.
- [ ] Look-dev: tone mapping, IBL and the texel density standard are chosen. The hex scale constants are finalized.
- [ ] Seam test: two road tiles placed side by side show no visible seam in an in-engine close-up screenshot (owner-approved).
- [ ] Provenance JSON complete for every piece. The external storage for `.blend` files is chosen and working.

### P2 — MVP vertical slice

Goal: the polished, playable core loop with D12 content.

- [ ] All MVP tiles (§5.4) are built through the pipeline and approved.
- [ ] Placement: ghost preview, rotation, edge glow, legal-slot inventory filter, auto-rotate.
- [ ] Staged rise animation (§4.4), including below-surface features, skippable.
- [ ] Road network stitching. Cars route between houses across tile boundaries.
- [ ] GPU boids birds roosting in forests.
- [ ] Water: river flow and ocean waves with shore foam.
- [ ] Spatial ambience with the focus mix. Placement and UI SFX. At least 3 Suno music cues mastered and crossfading.
- [ ] Undo/redo and bulldoze with the connectivity rule.
- [ ] IndexedDB slots, autosave, `.hexworld` export and import.
- [ ] Photo mode with DOF and 4× export.
- [ ] Quality tiers with auto-detection. 60 fps at the 95th percentile on the reference machine with a scripted 2,000-tile world on High.
- [ ] Every §9.4 hook is present in the MVP assets and shaders (verified by the asset gate).
- [ ] A5 (audio license terms) is verified and recorded.

### P3 — Progression and farm/rail content

- [ ] Recipe system, recipe book with hints, discovery moment, creative-mode toggle.
- [ ] Farm (animals), windmill, rail straight/bend/junction, station, bakery, and at least 6 recipes.
- [ ] Trains route between stations across tile boundaries. Farm animals wander within their farm. Boats on ocean and rivers.
- [ ] Bridges and level crossings where rail meets road or river.

### P4 — Elevation

- [ ] Height levels 0–3, with `Cliff` and `Ramp` edge profiles and height-aware socket rules.
- [ ] Hills and mountain tiles. Rivers that flow downhill (river lanes carry a flow direction). Waterfalls at cliff edges.
- [ ] Roads and rails need ramps between levels.

### P5 — Atmosphere

- [ ] Day/night cycle: sun and moon, sky model, emissive night lighting, vehicle headlights.
- [ ] Weather: rain, snow, fog and storms with GPU particles, wetness and snow accumulation using the §9.4 masks.
- [ ] Seasons: foliage tint, snow cover, frozen rivers. Adaptive music stems by mood and time of day.

### P6 — Content breadth

- [ ] Factories, volcano (with an eruption signature moment), craters, mountains, desert, tundra, swamp, towns with pedestrians, harbors, and more.
- [ ] A goal of 100+ tile types from the kit, with recipes covering every non-starter type.

### P7 — Mobile and tablet

- [ ] Touch gestures mapped onto the input-action layer, a responsive UI, and a mobile quality tier.
- [ ] 30+ fps on iPad Air (M1) and a recent iPhone with Safari WebGPU.

### P8 — First-person stroll and drive

- [ ] A walk controller using the per-tile colliders, and a drive mode on the road network.
- [ ] Close-range LOD0 and texture quality pass verified in first person.

## 16. Risks

| Code | Risk | Mitigation |
|---|---|---|
| R1 | KTX2/meshopt decoding in Rust/WASM is immature (TC3). | P0 spike with three decoder routes (§9.3). |
| R2 | AAA quality at 2,000 tiles in a browser exceeds the frame budget. | Instancing, GPU culling, chunked LOD, quality tiers, and perf gates from P2 onward. |
| R3 | Kit seams are visible between modular pieces. | A strict edge contract, normal blending at the hex border, and a seam gate in P1. |
| R4 | Asset throughput: high-poly + bake per piece is slow. | Scripted generators, a shared helper library, and family-level reviews. |
| R5 | Single-threaded WASM limits simulation scale (TC1). | Simulation LOD, GPU compute for flocks, and a worker fallback (§7.3). |
| R6 | Audio license terms disallow public redistribution. | The A5 gate before the first audio commit. Swap the provider if needed. |
| R7 | Safari or Firefox WebGPU differences cause rendering bugs. | Local browser e2e tests in each supported browser before releases. |
| R8 | Bundle size grows past Pages limits. | Size budgets enforced in `checkall-ci`, plus streaming biome packs. |

## 17. Open questions

| Code | Question | Decide by |
|---|---|---|
| Q1 | Final hex scale (R = 16 m?) and level step (4 m?). | P1 look-dev |
| Q2 | External storage for `.blend` sources (R2 bucket vs. other). | P1 |
| Q3 | Decoder route for KTX2/meshopt (§9.3). | P0 |
| Q4 | Tone mapper (AgX vs. ACES). | P1 |
| Q5 | Whether to register this PRD in par-prd (`prd.pardev.net`) as the canonical copy. | Owner |
