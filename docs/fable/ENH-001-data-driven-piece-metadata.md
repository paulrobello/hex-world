# ENH-001: Data-driven kit-piece runtime metadata

| Field | Value |
|---|---|
| **Source** | Audit 2026-11-15, architecture finding ARC-001 (High) |
| **Impact** | High: adding a kit piece becomes a data edit instead of Rust edits in three crates |
| **Effort** | L, four phases, each independently shippable |
| **Depends on** | QA-001 (completeness tests), QA-005 (typed labels), QA-003 (table modules) |
| **Owner decision needed** | Yes: this replaces the `integration.patch` convention for the per-piece tables |

## Goal

Everything the runtime knows about a kit piece beyond drawing it lives in one data file, `data/pieces.ron`, embedded at compile time like the tile definitions. A new piece needs one entry there. A piece with no entry, or an incomplete one, fails a test.

## Current state (at `edbc7ad`)

- `assets/kit/manifest.json` holds 162 pieces with draw data only (`id`, `pack`, `kind`, `glb`, `provenance`, `slots`, `maps`). It is hand-maintained, parsed with `deny_unknown_fields` (`crates/hex-assets/src/kit_manifest.rs:244`), fetched at run time and shipped to the browser.
- Runtime metadata is Rust constants keyed by piece-id string:
  - `crates/hex-play/src/walk.rs`: `OBSTACLES` (80), `CENTRE_OBSTACLES` (170), `FOOTPRINT_OBSTACLES` and the `*_ROCK_FOOT` polygons (221-540), `PIECE_POSTS`, `DECLARED_PARTS` (547-602), `CAR_GROUND` (720), `prop_kind` (1146).
  - `crates/hex-play/src/play/scatter.rs`: `VARIANTS` and weights (207-269), `FOOTPRINTS` (615-1209), `GROUND_BOXES` (1234), `FENCES` (1340), `NOT_GROUND` (1434), `GROUND_PROPS` (1546).
  - `crates/hex-play/src/play.rs:737`: the rise classification.
  - `crates/hex-core/src/variant.rs:10`: `HERO_VARIANTS`.
  - `crates/hex-render/src/kit_scene.rs:70`: the showcase layout.
- Distinct manifest ids as string literals in production code: 90 in `walk.rs`, 91 in `scatter.rs`, 111 in `kit_scene.rs`, 23 in `variant.rs`. About 1,950 lines of `walk.rs` and `scatter.rs` are data.
- After QA-001, QA-005 and QA-003 the tables sit in `walk/tables.rs` and `scatter/tables.rs` with completeness tests and a `PropLabel` enum. This plan starts from that state. Re-read those files first: the line numbers above are pre-split.
- Tile data already follows the target pattern: `crates/hex-core/src/catalog.rs:18` embeds `data/tiles/*.ron` with `include_str!` and `crates/hex-core/tests/data_validator.rs` validates it.

## Design

1. **Where the data lives: `data/pieces.ron`, not the manifest.** The manifest is fetched asynchronously and reaches play code only through `PlayKit` (`crates/hex-play/src/play/packs.rs:74`), while the tables are read by pure functions (`footprint(piece)`, `prop_kind(piece)`) that have no resource access and run in native tests. Embedding with `include_str!` keeps those functions pure, adds nothing to the browser download, and leaves the manifest's wire shape and its `deny_unknown_fields` parser untouched.
2. **Who parses it: `hex-core`.** New module `crates/hex-core/src/piece.rs` with `PieceTable::builtin() -> &'static PieceTable` behind a `LazyLock`, parsed once from the embedded RON (`ron` is already a `hex-core` dependency). `hex-core` owns it because `HERO_VARIANTS` is there and `hex-core` has no Bevy dependency, so `hex-play`, `hex-render` and tests can all read it.
3. **Entry shape** (every field explicit, no defaults except where noted):
   ```ron
   (
     id: "bakery_shop",
     label: Building,                    // PropLabel: the click label
     rise: Building(floors: 2),          // Tree | Building(floors)
     walk: Obstacle,                     // Obstacle | WalkThrough | Ground | Boundary
     footprint: Some((ground: 2.6, crown: 0.0)),
     ground_box: Some((half: (3.1, 2.4), ..)),
     not_ground: false,
     car_ground: [],                     // road-surface slot names cars ride on
     variants: [],                       // [(id, weight)] scatter variants of this piece
     hero_variants: ["bakery_shop_b"],
     showcase: Placed,                   // Placed | NotShowcased("reason")
   )
   ```
   Geometry that is a polygon or part list (`FOOTPRINT_OBSTACLES`, `CENTRE_OBSTACLES`, `PIECE_POSTS`, `DECLARED_PARTS`, `FENCES`) uses the same field names as the Rust structs so `serde` derives map one to one.
4. **Accessors keep their signatures.** `scatter::footprint(piece)`, `walk::prop_kind(piece)`, `hex_core::variant::hero_piece(...)` and the rest stay, and read the table. Call sites do not change.

## Steps

### Phase 0: owner decision
Confirm the convention change: asset agents return a `data/pieces.ron` entry (plus any non-table Rust change) instead of an `integration.patch` over the tables. No code before this.

### Phase 1: classification (label, rise, walk class)
1. Add `crates/hex-core/src/piece.rs` with `PieceDef`, `PieceTable`, `PropLabel` (moved from `hex-play`, re-exported there), `RiseKind`, `WalkClass`. Add `data/pieces.ron` with one entry per manifest piece of kind prop, tree or hero. Generate the first version with a throwaway script that calls the existing functions, so the data equals current behaviour by construction: for each id emit `prop_kind(id)`, `rise_kind(id)` and membership of `OBSTACLES`, `WALK_THROUGH`, `GROUND_PROPS`.
2. Transitional test `pieces_ron_matches_the_tables`: for every id, the parsed entry equals what the old functions and constants return. Run it green.
3. Switch `walk::prop_kind`, `play::rise_kind` and the obstacle, walk-through and ground-prop lookups to the table. Delete `OBSTACLES`, `WALK_THROUGH`, `GROUND_PROPS`, `NOT_GROUND` and the prefix matches. Delete the transitional test.
4. Validator: extend `crates/hex-core/tests/data_validator.rs` to assert `data/pieces.ron` ids are unique. In `crates/hex-play/tests/kit_gate/`, assert every manifest piece of kind prop, tree or hero has an entry and every entry id is in the manifest (the manifest is read from disk in that test, as today).

### Phase 2: variants
1. Add `variants` and `hero_variants` to the data from `VARIANTS`, `VARIANT_WEIGHTS`, `VERGE_VARIANT_WEIGHTS` and `HERO_VARIANTS`, generated the same way, with a transitional equality test.
2. `hex_core::variant` keeps only the deterministic pick and takes the variant list from the table. The pick must consume the list in the same order with the same weights: variant choice is seeded, so a reorder changes worlds and thumbnails.
3. Delete the constants. `variants_keep_their_family` (`variant.rs:499`) becomes a data test.

### Phase 3: geometry
1. Move `FOOTPRINTS`, `GROUND_BOXES`, `FENCES`, `FOOTPRINT_OBSTACLES` with its polygons, `CENTRE_OBSTACLES`, `PIECE_POSTS`, `DECLARED_PARTS` and `CAR_GROUND` into the data, generated from the constants with full float precision (`{:?}` round-trips `f32`), with a transitional equality test comparing bit patterns.
2. The measurement tests `footprints_match_the_kit`, `ground_boxes_match_the_kit` and `fences_match_the_kit` stay and now validate the data against the shipped GLBs.
3. Delete the constants and `DEFAULT_FOOTPRINT`: `footprint()` for an id with `footprint: None` is a hard error in tests and returns the zero footprint at run time for walk-through pieces only.

### Phase 4: showcase and docs
1. `showcase: Placed | NotShowcased(reason)` replaces `NOT_SHOWCASED` in `kit_scene.rs`. The layout positions stay in Rust (they are a hand-composed scene, not per-piece facts).
2. Update `CLAUDE.md` ("Piece tables that every new kit piece may need a row in" becomes one sentence naming `data/pieces.ron` and the validator), `.claude/skills/blender-asset/references/dispatch.md` part 11, `handoff.md` References, and `PRD.md` §12 with a dated decision line.
3. Optional follow-up, separate card: the Blender generator emits the footprint and ground-box numbers into its checkpoint so the entry is copied, not measured by hand.

## Files to touch

`data/pieces.ron` (new), `crates/hex-core/src/piece.rs` (new), `crates/hex-core/src/lib.rs`, `crates/hex-core/src/variant.rs`, `crates/hex-core/tests/data_validator.rs`, `crates/hex-play/src/walk/tables.rs`, `crates/hex-play/src/play/scatter/tables.rs`, `crates/hex-play/src/play.rs`, `crates/hex-play/tests/kit_gate/`, `crates/hex-render/src/kit_scene.rs`, `CLAUDE.md`, `.claude/skills/blender-asset/`, `handoff.md`, `PRD.md`.

Enumerate readers before each deletion with parsight `get_impact` and `get_symbol_context` on the constant (repository `hex-world`).

## Verify

Per phase:
- `cargo test -p hex-core` and `cargo test -p hex-play` pass, with the test count not lower than before the phase.
- The transitional equality test was green in the commit before the constants were deleted (keep that commit in the branch history).
- Booked load-sensitive window: `caffeinate -d -u make thumbs` leaves `git status --short assets/thumbs` empty. Variant picks and scatter are seeded, so this is the proof that behaviour is identical.
- `make kit-check` and `make checkall-ci` pass.

At the end:
- `git grep -nE '"(house|barn|tree|rock|hay|animal)_[a-z_]+"' crates/hex-play/src crates/hex-core/src -- ':!*tests*'` finds no piece id in a production table.
- Adding a fake piece to a scratch copy of the manifest without a `pieces.ron` entry fails the kit gate; adding an entry with a missing field fails to parse.

## Rollback

Each phase is one squash commit that deletes constants only after a green equality test, so `git revert` of that commit restores the constants and the old accessors. The data file is additive until its phase's deletion, so a partially finished phase can be left unmerged without affecting `main`.

## Out of scope

Changing `assets/kit/manifest.json`, changing any table value, re-baking any piece, and moving the showcase layout to data.
