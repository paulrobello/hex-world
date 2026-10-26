# Friction: stone scatter props (beach_pebbles, _b, _c, seabed_rock, seabed_pebbles), 2026-10-26

One log for the five props `_stones.py` builds, and for the removal of the beach and ocean dressing
centres they replace.

1. **A cluster spread too tight for its pebbles.** `beach_pebbles_b` at a 0.26 m spread had no room
   for three pebbles of up to 0.17 m radius with the 3 cm gap (`AssertionError: no room for stone
   2`). Widened to 0.36 m. 1 minute. No skill change: the generator's own check caught it.
2. **A reused world script carried a window screenshot.** `bd-after-coast.json` (the coast island
   world) holds one window `screenshot`, which came back black at host load 308 and failed
   `make shots-piece` twice. The derived script keeps only offscreen `photo_export` frames
   (`docs/checkpoints/beach-ocean-scatter/scripts.py`). 2 minutes. The skill already sends black
   window shots to `photo_export`; no change.
3. **A test assumed every water tile has a centre.** `water::tiles` test
   `the_stand_in_bed_is_drawn_only_where_the_base_is_missing` expected a centre on every water
   tile and failed `make checkall` once beach and ocean took `center: None`; its centre check now
   runs only where a centre exists. 2 minutes. Proposed skill text (pitfalls.md, "Land a water
   centre with its base change", which is stale): "The engine draws a water tile's kit bed as soon
   as its base piece is in the kit (`water::tiles::stand_in_bed`); the centre only gates the river
   lane strips (`play.rs`, `pending_water`). A water tile may have no centre: the beach and ocean
   tiles' dressing is scatter since 2026-10-26."
4. **Tile scatter rules are in the bridge schema.** Adding scatter rules to `data/tiles` failed
   `bridge::schema::schema_matches_web_ui` until `make ui-schema` regenerated
   `web/ui/src/bridge-schema.json` (the inventory carries each tile's scatter list). CLAUDE.md
   names `make ui-schema` only for Rust enum changes. 2 minutes. Proposed CLAUDE.md text, "Rust ↔ UI
   changes are two-sided": "... and so is a `data/tiles` change the inventory carries (a tile's
   scatter rules): run `make ui-schema` after it."
5. **A scatter prop on a water bed counts against the bed's envelope.** The ocean floor may rise at
   most 0.10 m above the bed (water contract §4), but scatter stands a prop on the relief (up to
   0.06 m) and scales it by up to 1.15, so a stone sized like ocean_center's would poke over. The
   seabed props stand at the bed's contract level instead (`scatter::SEABED_Y`, in the
   integration patch) and are authored within `_stones.TOP_MAX_SEABED` (0.085 m), pinned by
   `seabed_stones_stay_within_the_ocean_floor_allowance`. Planned, no time lost. Proposed skill
   text (pitfalls.md, Water beds): "A scatter prop on a water bed is part of the bed the water
   reads: size its top against the bed's allowance at the scatter's largest scale, and stand it at
   the contract level rather than on the relief."
