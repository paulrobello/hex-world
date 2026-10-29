# Mega tiles, first round (P4 massifs): design

Status: **approved by the owner 2026-10-29 ("approved"), with A52 answered O1 for every question.** Written 2026-10-29 from the read-only scoping in card `01a13bcc6f8d`. Decisions marked *(A52 Qn O1)* are the owner's answers. The plan corrects "12 outer edges" to 18 outer edges facing 12 neighbour cells (Goal 2 already says so).

## Intent

A40 O1 (owner, 2026-10-27) gives single-tile hills and peaks, and says "bigger massifs come with the 7-hex mega-tile card". A mega tile is one piece covering a centre cell and its six-cell ring, carrying one feature too big for a hex: first a mountain massif. It should read as a landmark that the surrounding tiles meet naturally, not as seven tiles glued together.

## Goals

1. One tile type, the **massif**, occupies 7 cells: a centre and its ring. It places, rotates, undoes, saves and bulldozes as a single tile.
2. Its 18 outer edges (three on each ring cell, facing the 12 cells around the footprint) sit flat at the tile's own level, the way a raised Meadow's edges do. The peak rises inside the footprint, so neighbours meet it with today's seam, cliff and hillside rules *(A52 Q1 O1)*.
3. Its ground is visually continuous with the baked Meadow beside it: the same grass, shading, blade and scatter density at the outer ring (the owner's visual standard, memory `visual-standard.md`).
4. Placement needs all 7 cells empty and at least one placed neighbour outside the footprint. The ghost shows the whole footprint, and R turns it about the centre (six rotations) *(A52 Q4 O1)*.
5. Saves keep one entry per mega tile, at the centre cell, whose definition implies the footprint. No format bump: the wire `Tile` (`hex-core/src/save.rs`) already carries `q, r, level, def, rotation, seed`. Decode expands the footprint and validates it.
6. The HUD learns whether a mega tile fits by asking the engine through the bridge (a published `mega_slots` state field), instead of mirroring the 7-cell rules in `rules.ts` *(A52 Q6 O1)*.

## Non-goals (this round)

- Roads, rails and rivers on or across a massif: every outer edge is plain ground *(A52 Q2 O1)*.
- Tunnels under a massif: refused *(A52 Q3 O1)*.
- Replacing a massif with single tiles, or the reverse: refused. Bulldoze and undo remove the whole footprint *(A52 Q5 O1)*.
- Outer edges at more than one level (A52 Q1 O2).
- Other mega-tile features (lakes, castles, towns). The footprint machinery is general, but only the massif ships.
- Scatter or walkers on the slopes beyond what the ring cells' Meadow ground already carries.

## Anti-goals

- A massif whose foot shows a seam, a colour step or a density step against the Meadow beside it.
- A world that can hold a partly applied footprint after any undo, redo, bulldoze or load. The footprint is atomic everywhere.
- The HUD offering a placement the engine refuses, or the reverse.

## Design

### hex-core

- `TileDef` gains an optional `footprint: Ring1` (centre plus six neighbours, in local coordinates rotated with the tile). The centre keeps its 6 `edges`; the footprint lists the **18 outer edges** clockwise from the ring cell at local edge 0, and the data validator checks the count.
- `World` keeps `tiles: BTreeMap<Axial, PlacedTile>` for the anchor (centre). A second map `members: BTreeMap<Axial, Axial>` (member cell → anchor) makes every cell of the footprint occupied. `tile(cell)` answers for any member cell with the anchor's `PlacedTile`. A new `anchors()` is for callers that want each tile once.
- `fits`, `is_slot`, `frontier`, `legal_orientations`, `edge_status`, `cliff_edges` and `clashes` test the footprint's cells and its 18 outer edges. Each outer edge faces exactly one neighbour cell, and `placement_meets` / `meet_kind` run per (outer edge, neighbour) pair unchanged.
- `can_bulldoze`, `connected_without` and the split rule treat the footprint as one node. Undo/redo already store a whole `PlacedTile`, so they insert or remove all members together.
- Tunnels: `tunnel::walk` treats a member cell as an obstacle, so no tunnel forms under a massif.
- `cell_seed` seeds from the anchor.

### Saves

The wire format is unchanged (one entry per anchor). Decode expands and validates footprints: no overlap, all members in bounds, edges meeting. Fixtures add a world with a massif. Old saves load unchanged.

### hex-sim

The massif carries no lanes, so `insert_tile` adds nothing to the road graph. Neighbouring roads that face it end as they would at any laneless edge.

### hex-play and hex-render

- One spawn root at the centre with the massif kit piece. The rise animation and dust emitter scale to the footprint.
- The massif piece carries its own ground mesh, generated headless in Blender (A42 O1) and baked like every piece. Its outer ring is shaped to the Meadow cap at the tile's level, so the runtime ground does not drape it. Grass and scatter on the ring follow the Meadow rules for each ring cell.
- Affordances and the ghost draw the footprint outline. Chunking assigns the whole massif to the anchor's chunk.
- Walk colliders, cars, boids and farm code treat member cells as occupied: walkers walk around the massif.

### HUD

- The engine publishes `mega_slots` (centre cell, tile and legal orientations) in its state, next to `frontier` and `inventory`, since bridge commands return nothing. The inventory shows the massif when a legal footprint covers some frontier cell; the ghost then anchors at that footprint's centre (a centre cell itself can never be a frontier cell). The picker thumbnail uses a wider camera (`thumbs.rs` gains a footprint-aware frame).
- `rules.ts` keeps single-tile rules only. The parity tests gain a case asserting that the HUD never lists the massif where the engine query refuses it.

### Kit piece

`massif_granite`, a hero-class piece in a streamed pack: a granite peak with a Meadow-matched ring, its height, scree and planting set by the owner-approved reference. It is built the standard way (approved reference first, a high-res source, a Cycles bake, reduced LODs, provenance and gates), with owner reference and piece approvals.

## Verification

- hex-core unit tests: placement across every rotation, footprint atomicity under undo/redo/bulldoze, the split rule, tunnel refusal, and save round trip with fuzz cases for overlapping footprints.
- Bridge contract and parity tests for `mega_fits`.
- `make thumbs` dirties only the massif's thumbnails. Flat-world visual parity is pixel-identical, and flat-world perf on MiniMac is within noise.
- In-engine review shots of the massif beside Meadow, forest and a road for the owner before merge.

## Left to the builder

The `Ring1` encoding, the bridge query's exact name and shape, and the massif's exact height and silhouette (within the approved reference).
