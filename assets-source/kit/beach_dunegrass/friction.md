# beach_dunegrass (and _b, _c) friction

2026-10-20, beach dressing (owner D3), scripted route, numpy-drawn cards (`_dunegrass.py`).

- **The mound sets the LOD2 floor** (2 min, one build). A 5-sided mound with its apron ring is 15
  tris at LOD2, so with two cards LOD2 is 21 and LOD0 must be at least 140 (15%). The small clumps
  (9 and 10 cards at three segments) came out at 124 and 130. Fix: four segments per LOD0 card (142,
  150, 166 tris), which also bows the cards more smoothly. Proposed text for `gates.md` (LOD floor):
  "A prop with a fixed part kept in every LOD (a sand mound) has the same floor: LOD0 >= that part's
  LOD2 tris / 0.15."
- **A world-space tileable slot on a scatter prop.** The mound takes the `plaster` set at
  `beach_sand`'s tint, so it is the beach's own sand wherever it lands; it needs no UV0 density
  (world-space), but its `masks` sample UV0, so its UVs sit in a corner of the atlas cleared on
  tile 0 where the masks carry sand values. No skill change.
- **The mound has to outreach the slope.** The scatter stands a piece at the lowest ground under
  its centre and a 0.3 m ring, and the dry band slopes up to about 0.18 where grass grows, so the
  dome's rim sits 0.04 m below the pivot and a buried apron runs on 0.3 m down to -0.3 m.
- **Textured still versus engine.** EEVEE shades the cards' back faces with the flipped normal, so
  the still reads darker at the fringe than the engine, which shades alpha-cutoff back faces with the
  front normal (`project.md`, Foliage normals). Judge the clump in play.
- **The mound reads as a ledge on the downhill side** (open, about 10 min, three shot runs). An
  upright, round mound on the 0.1-0.18 slope of the dry band shows its lower rim as a pale polygon
  at eye level. A steeper buried apron (0.55 m to 0.3 m), 12 sides instead of 10 and rim normals
  turned toward +Z changed the mound's pixels (4,701 pixels by more than 8 levels in the foreground
  box) but not the read. Real fixes are the owner's or the orchestrator's: tilt beach clumps to the
  sand's normal at runtime, or lower and smaller mounds.
- **Integration touch-list for a centre that loses slots and tiles that gain scatter** (one failed
  checkall). Dropping a manifest slot also needs the `PLANTS` list in `crates/hex-play/tests/hooks/
  mod.rs` (a stale entry fails), the slot names in `ghost.rs` `water_tile_slots_tint_by_their_ground`;
  giving a tile scatter rules needs `make ui-schema` (`web/ui/src/bridge-schema.json` lists each
  tile's scatter pieces, `bridge::schema::schema_matches_web_ui`); a new kit-scene cell needs the
  counts in `crates/hex-render/src/lib.rs` `tile_counts`. Proposed `dispatch.md` text (part 11):
  "A centre losing slots or a tile gaining scatter rules: PLANTS, ghost slot roles, `make ui-schema`."
- **Before shots need the base binary and the base assets** (about 8 min). The bootstrap's cloned
  release binary was older than main's shaders and quit on its first frame (pbr pipeline layout),
  and a `git archive` of the base tree reads as fresh to cargo (dep-info paths are workspace-relative
  and the archive's mtimes are old) until its sources are touched. Recipe used: `git archive
  --output=… HEAD Cargo.toml Cargo.lock rust-toolchain.toml crates data assets assets-source web`,
  untar into scratch, `cp -cR target`, `touch` every file under crates/ and data/, then
  `cargo build --release -p hex-app` there (2.5 min) and run it with that directory as cwd.
  Proposed `stages.md` text: "Before/after play shots: build the base binary from an archived base
  tree (touch its sources) and run it in that tree, so it loads the base assets."
- **A scratch script named `numbers.py` shadowed the stdlib** (2 min): PIL's PNG plugin imports
  `fractions`, which imports `numbers`, and Python put the scratch directory first. Name scratch
  scripts distinctively (`bd_numbers.py`).
