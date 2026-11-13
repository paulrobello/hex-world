# Single-tile hills and peaks (P4): design

Status: **pending owner approval; built ahead under the no-idle rule.** Written 2026-11-13 by an asset agent while the owner was away. Every choice below is a ruling made to keep work moving: it names what was decided, why, and what it costs if the owner rules otherwise. Nothing here is an owner approval, and the reference picks are pending choices only (no `approval.json` is written).

## Intent

A40 O1 (owner, 2026-10-27): "single-tile hills and peaks that rise inside the hex and meet neighbours at their edge level; bigger massifs come with the 7-hex mega-tile card". The massif (`docs/superpowers/specs/2026-10-29-mega-tiles-design.md`) shipped the big feature. This round adds the small ones: a landform that fits one hex, sits in a Meadow field without a seam, and stands on any level a Meadow can.

## Goals

1. Two single-cell tiles, **`hill_meadow`** (a rounded grassy hill, crown about 8.5 m above the tile's level, a few low granite outcrops near the crown) and **`peak_granite`** (a craggy granite peak, summit about 18 m, cliffs and buttresses, scree in the gullies, grassy lower slopes).
2. All six edges are plain `Grass`, flat at the tile's own level, exactly where a Meadow's are. The landform rises inside the hex, so neighbours meet it with today's seam, cliff and hillside rules (the massif's A52 Q1 O1 carried to one cell).
3. The ground is visually continuous with the baked Meadow beside it: the same tileable grass set and cap shading, runtime grass blades on the slopes, and Meadow's scatter density per square metre of grass (memory `visual-standard.md`).
4. Rock and scree are a unique bake under a scoped texel floor (the massif's A54 ruling). Pines and loose boulders are runtime scatter, never baked (memory `scatter-not-baked.md`).

## Non-goals (this round)

- Roads, rails and rivers on or across a hill: every edge is plain Grass (as the massif, A52 Q2 O1).
- Tunnels under a hill: refused. `hex-core::tunnel::COVER_DEF` stays `meadow`, so a hill is never cover; a test pins it (as the massif, A52 Q3 O1).
- Hill variants by rotation beyond the six turns `look_period: Some(6)` gives, and hill edges at more than one level.
- Boid perches, sim nodes, cars and farm use on the slopes.

## Rulings

Each ruling: what, why, and the cost if the owner rules otherwise.

- **D1. Two lone tiles, after the massif in the inventory.** Picker entries `hill_meadow` ("Grassy hill") and `peak_granite` ("Granite peak"), each its own Tiles sheet entry, in `BUILTIN_TILES` right after `massif_granite`. Why: a picker group would make R cycle hill shapes and turns together, which no other landform does. Cost if wrong: a `PICKER_GROUPS` row and a parity test.
- **D2. Sizes.** hill_meadow crown 8.5 m (brief: 8-10 m; ref-1 reads about 8 m against the 16 m sides). peak_granite summit 18 m (brief: 16-20 m), kept under 19 m so the peak stays 3 m under the boid sky band at every level (D11). Cost if wrong: one landform constant per generator and a rebuild.
- **D3. Levels.** Both tiles are `raisable: true` with `top() = 0`, so they stand on levels 0-3 like a Meadow (`hex-core::world::level_allowed`). A raised hill sits on the same runtime column a raised Meadow uses, and its skirt is Meadow's (4 m, soil). A hill on level 3 tops out at 12 + 18 = 30 m. Why: the brief's "a hill sits on a raised tile" is the raised-Meadow case with a landform on top. Cost if wrong: `raisable: false` in two RON files.
- **D4. Neighbours.** Whatever meets a Meadow edge meets a hill edge (socket kind `Grass`, no new socket). A hillside, road, river bank, forest or another hill may sit beside it. Cost if wrong: a new socket kind and its table rows.
- **D5. Turns.** `look_period: Some(6)`: the all-Grass edges would otherwise fold six turns into one, and a peak's buttresses face a way. R steps through six turns per level. Six thumbnails per tile, 12 in all.
- **D6. Kit kind, budget class and pack.** Kind `base` (a ground piece: the base gate's ring, seam, normal, skirt and LOD-identity checks apply to one hex for free), LOD0 at most 4000 triangles, KTX2 class `piece` (1.5 MB per texture family), pack **`hero`**. Why the hero pack: it already streams the massif, it is not in the initial download, and `core` keeps its size and purity. Cost if wrong: a pack change is one manifest field; a separate `hills` pack would add a pack row and a HUD loading entry.
- **D7. Texture budget.** A 1024 albedo atlas, normal, ORM and masks at 512 (the `outcrop_granite` split, which measures 1.17 MB), so each family fits 1.5 MB. The atlas follows the massif's layout: the ground's masks and the lower-LOD rock blocks in the left quarter, LOD0 rock and scree on the right. Cost if wrong: a hero class needs a hero kind, which loses the base gate's seam checks.
- **D8. Texel floor (as A54).** A scoped floor per piece in `kit_gate::texel_min`, set to the measured LOD0 rock and scree density rounded down to a multiple of 5 px/m, and never below the massif's approved 20 px/m. The hill's tiny outcrops will measure far higher. Cost if wrong: one row in `texel_min`.
- **D9. Scatter.** Each piece's grass cap (`<id>_cap`) joins `floor_slots` wherever `meadow_base_cap` and `massif_granite_cap` are listed. Its rock and scree join `excluded_slot`'s exact list. Both tiles carry Meadow's five rules, with densities scaled by the measured share of grass cap against a Meadow's, so instances per square metre of grass match a Meadow (a test checks it). peak_granite adds spruce and sapling drifts on its lower slopes within a single-tile band (`scatter::ring_band` gains a row: centre distance about 8-12.5 m). hill_meadow gets no pines, because its references show none. Cost if wrong: data-only density and band edits.
- **D10. Grass.** The cap slot is `{"tileable": "grass", "masks": true}`, so runtime blades grow on it like any grass slot (`grass::grass_slots`). The bake shades the cap with the same set and parameters as the massif's approved cap. The grass-slot test lists the two new caps.
- **D11. Boids.** Nothing changes. The flock band starts at `SKY_LOW` 22 m above the highest placed level (`boids::plan::sky_lift`), so an 18 m summit at any level stays at least 4 m under it. A test pins `LANDFORM_TOPS < SKY_LOW - 3`. No perches are baked.
- **D12. Picking.** `game.rs::top_of` adds a single landform's top (a `LANDFORM_TOPS` table in `hex_play::play`, each top tested against the piece's LOD0 GLB within 5 cm, like `MASSIF_PEAK`). A ray into a peak's upper half then picks the peak, not the cell behind it. `camera::PICK_PLANES` already reaches 46 m. Cost if wrong: drop the table, and a click on a peak's top picks past it.
- **D13. Walker.** The walker stands on the piece's LOD0 ground (`slope_ground::tile_ground_tris`, unchanged for a flat-edged base). Grass and scree slopes are walkable (the limit is gradient 4). The rock is blocked by a `walk::FOOTPRINT_OBSTACLES` row per piece: the rock foot measured top-down on LOD0 and split into convex parts, using the massif's `walk_parts.py` method. Cost if wrong: re-measure the rows after a rebuild.
- **D14. Thumbnails.** The shared single-tile pose holds an 18 m summit (NDC y about 0.46, under the far corner's 0.55), so `thumbs.rs` keeps one pose and `single_tile_camera_is_unchanged` holds. A test pins the summit inside the frame. The orchestrator captures the 12 thumbnails with `make thumbs`. Until then, `every_tile_rotation_has_a_thumbnail` skips only these two ids (`AWAITING_THUMBNAILS` in `data_validator.rs`), so the other tiles stay gated, and it fails once `make thumbs` lists them until they leave that list.
- **D15. Saves.** No format change. The new definition ids load through the existing wire `Tile`, and old saves are untouched. No fixture is needed, because the format version does not move.
- **D16. Sim, cars and farms.** No lanes, so the road graph gains nothing. A road facing a hill ends as it would at any laneless edge. Car height does not apply.
- **D17. Sound.** Biome `Grass`, so the hills use Meadow's ambience and placement stinger. No new cue.
- **D18. Kit showcase.** `kit_scene` places both pieces beside the massif, and `tile_counts` counts them.
- **D19. Performance.** Each piece draws four material slots (cap, rock, scree, skirt), like the massif, at LOD0 at most 4000 triangles, LOD1 at most 50% and LOD2 at most 15% of LOD0. A field of hills costs about what a field of Meadow does plus the rock slots. The orchestrator's flat-world perf baseline is unaffected, because no existing tile changes.

## Design

### hex-core

Two RON files (`data/tiles/hill_meadow.ron`, `data/tiles/peak_granite.ron`): biome `Grass`, base the piece of the same id, six `Grass` edges, Meadow's scatter rules at scaled densities (D9), plus peak_granite's pines, `look_period: Some(6)`, `raisable: true`. `BUILTIN_TILES` lists them after the massif. The data validator's inventory (`F1`) gains both, with six Grass edges each. No world, footprint or socket change.

### hex-play and hex-render

- `scatter.rs`: the caps join `floor_slots`, the rock and scree join `excluded_slot`, and `ring_band` gains a single-tile pine band for peak_granite.
- `walk.rs`: `FOOTPRINT_OBSTACLES` rows for each piece's rock foot. hill_meadow's outcrops are low, but they are rock the walker should not climb through, so they get a row too.
- `play.rs`: `LANDFORM_TOPS`, with a GLB test.
- `grass.rs`: the grass-slot test lists both caps.
- `kit_scene.rs`: places both pieces, and `tile_counts` follows.
- `kit_gate.rs`: the scoped texel floors (D8) and the hooks `MASKS_EXEMPT` skirt rows.

### hex-app

`top_of` uses `LANDFORM_TOPS` (D12). `thumbs.rs` gains a test that the hills' summits land inside the shared pose.

### HUD

`words.ts` `TILE_NAMES` gains both names. `make ui-schema` regenerates the inventory schema and the parity file. The audio test's tile lists classify both as land with Meadow's biome.

### Kit pieces

Each piece is built by a thin generator (`scripts/blender/kit/<id>.py`) over a family file, `scripts/blender/kit/_hills.py`. The family file drives the massif's helper `_massif.py` through its module globals and function overrides, and never edits it. It covers:

- the single-hex outline, border ring and span;
- the landform (crown, flanks, rock, fans);
- the atlas sizes;
- the LOD triangle targets;
- the voxel extent;
- the budget class `piece`.

The pipeline is the standard one: high poly, Cycles bake, LODs, collider, sockets, GLB, KTX2, reimport gates and provenance. The build references are pending picks:

- **hill_meadow ref-1:** the rounded hill with low outcrop slabs at the crown. ref-4 is the same brief from the low camera, used for the silhouette.
- **peak_granite ref-1:** the craggy peak with buttresses, scree gullies and grassy lower slopes. ref-4 is its low-camera silhouette.

The refs sheets are `docs/checkpoints/<id>/reference/refs-sheet.jpg`.

## Delivery while references are unapproved

`kit-provenance` refuses a piece without the owner's `approval.json` (R2), and the orphan gate refuses a tracked generator or kit folder without a manifest entry. So the branch commits only these:

- the family helper (`_*.py` is exempt);
- the references;
- this spec;
- `docs/checkpoints/<id>/`.

The thin generators, manifest entries, tile data, runtime tables and tests travel as a verified, text-only `docs/checkpoints/hill_meadow/integration.patch`. That one patch covers both pieces, because they share every table. The text under `assets-source/kit/<id>/` (the pending `approvals.json`, `friction.md` and the three build reports) travels in the same patch. The binary artifacts (each piece's GLB, KTX2 and PNG maps under `assets/kit/p1/<id>/`, and its six-view renders) stay uncommitted in the agent worktree, listed with their SHA-256 in `docs/checkpoints/hill_meadow/integration.md`, so the orchestrator can land them as built, as the massif's were. If the worktree is gone, `make kit-build`, `kit-compress` and `kit-provenance` rebuild them once the owner approves the references; `kit-determinism` passing twice proves the GLB rebuild matches. Ruling (what: binaries in the worktree, text in the patch; why: nothing under those two folders can be committed before R2 without turning a gate red; cost if wrong: one rebuild of both pieces, a few minutes of bakes each).

## Verification

- Kit gates per piece: `kit-build`, `kit-compress`, `kit-determinism` (twice, both passing) and `kit-renders`. Then `kit-check` with the patch applied, where the only expected failures are the approval and provenance checks, until R2.
- Runtime tests with the patch applied:
  - the tunnel-cover refusal;
  - scatter density per square metre of grass against Meadow;
  - the peak pine band;
  - the rock and scree exclusion;
  - the walker blocked at the rock foot and climbing the grass;
  - `LANDFORM_TOPS` against each GLB;
  - the boid clearance;
  - the thumbnail frame;
  - the grass slots;
  - the picker order.
- `make checkall` with the patch applied, and `make ui-e2e` because the walker is affected. Until R2 the kit gate's provenance test fails by design for the two pieces, and a failing test binary stops `cargo test --workspace`, so the cargo leg runs with `-- --skip provenance` and the remaining `checkall` legs run on their own. The report says "red only on provenance (R2)" with the provenance failure's text.
- Owner review: in-engine shots of each hill beside Meadow, forest and a road, at level 0 and raised, from the play camera and the low camera, before merge (memory `visual-standard.md`). The orchestrator captures them.

## Owner questions (queued, each with a recommendation)

- **Q1. Reference picks.** O1: hill ref-1 and peak ref-1 (recommended). O2: hill ref-2 (single tor) and peak ref-3 (one cliff face). O3: re-roll.
- **Q2. Pines on the hill.** O1: none (recommended, matching the references). O2: a few spruces low on the flanks, as the peak has.
- **Q3. Pack.** O1: `hero` (recommended): a hill selected before the hero pack has streamed fetches that whole pack, about 23 MB a texture family today (the three hero oaks and the massif) plus the two hills' share, and after the first interaction every pack streams anyway. O2: a new `hills` pack: only the hills' own bytes on that first selection, at the cost of a new `hex_assets::PACKS` entry and its place in the streaming order.
- **Q4. The peak's rock read.** In the review stills (`docs/checkpoints/peak_granite/review-sheet.jpg`) the pinnacles read as rounded blocks with blotchy lichen and moss tint, where ref-1 shows a sharper, jointed crag. O1: judge it in the in-engine shots first, at the play and low cameras (recommended: the stills are EEVEE evidence, and the pick in Q1 is still open). O2: a jointing pass now (deeper slab facets and joints, less tint on the lower rock), at the cost of a rebuild, re-measuring `PEAK_ROCK_FOOT` and `LANDFORM_TOPS`, re-running the gates and regenerating the patch.
