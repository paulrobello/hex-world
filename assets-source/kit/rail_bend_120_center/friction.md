# rail_bend_120_center friction

1. **`_rail.build_piece` cannot build a bend at other edges.** It hard-codes the bend's lanes as
   (0, 1) in a local tuple, and `_rail.bend_point(theta, r=BEND_R)` binds r = 8 as a default
   argument at definition time, so setting `_rail.BEND_R = 24` from a new family file still builds
   an 8 m arc through `bend_frames` and `bend_sleepers`. `masks_field` and `sleeper_angles_or_x`
   read the module globals at call time, so they follow. Fix: `_rail_bend_wide.py` sets the four
   bend globals, builds its own frames and sleepers with r passed explicitly, asserts both ends land
   on their edge midpoints with the lateral along the border, and composes the piece from `_rail`'s
   parts on lanes (0, 2). `_rail.py` is untouched, so no rail piece goes stale. Lost: about 5 min
   (found reading the helper before the first build). Proposed skill text (pitfalls, "Blender and
   its API" or the Helpers rule): "A helper default argument such as `def f(x, r=RADIUS)` freezes
   the module global at import; setting the global from a new family file does not reach it. Pass
   the value explicitly." No `_rail.py` change: taking lanes and radius as parameters would stale
   the five pieces that import it, for no gain to this piece.
2. **A new tile moves four catalog-size pins.** `data_validator.rs` `F1` (array length),
   `scatter.rs` `no_instance_on_an_excluded_slot_for_every_mvp_tile_and_rotation` (28),
   `play.rs` `every_mvp_tile_resolves_and_biomes_index` (28), and `thumbs.rs`
   `distinct_rotations_follow_the_orientation` (`28 + 126`, tiles plus distinct rotations), plus
   the kit cell counts in `hex-render/src/lib.rs` `tile_counts` (Kit and WaterKit). `cargo test
   --workspace` stops at the first failing target, so one `make checkall` shows one pin at a time:
   run `cargo test --workspace --no-fail-fast` once to list them all. Lost: about 10 min (three
   checkall rounds). Proposed skill text (stages, after the manifest entry): "A new tile: run
   `cargo test --workspace --no-fail-fast` once and update every tile-count pin it names; the
   thumbnail test (`every_tile_rotation_has_a_thumbnail`) stays red until `make thumbs`."
3. **The circle holds one train, not two.** A stretch of at least 55 m gets one train, plus one per
   further 140 m (`hex_sim::rail` `MIN_TRACK`, `TRACK_PER_TRAIN`), so the 150.8 m circle of six wide bends
   holds one; the first test asserted two. Lost: 2 min. No skill change: the rule is in the module
   doc.
