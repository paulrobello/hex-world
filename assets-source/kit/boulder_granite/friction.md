# Friction: the B1 granite set (boulder_granite, _b, _c, _d, outcrop_granite)

One log for the five pieces; they share `scripts/blender/kit/_boulder.py`.

1. **The approved texel density is under the gate's floor.** Owner A25 set "a unique bake per piece
   at about 128 px/m", while `kit_gate` holds every prop to 256 px/m. 256 px/m on the 4.5 m boulder
   or the 8 m outcrop needs a 2048² albedo, about 4 MB of KTX2 per family against the 1.5 MB `piece`
   class, so the ruling is the only reachable density. Fix: a scoped floor in the gate
   (`BOULDER_TEXEL_MIN_PX_PER_M` = 100 for the five ids, with its test), handed back in
   `docs/checkpoints/boulders/integration.patch`. Measured medians: 129, 218, 172, 134 and 109 px/m.
   Lost: about 10 minutes. Proposed skill text below.
2. **A single-slot piece cannot leave LOD2 under the gate as it stood.** `gates.md` covers a slot
   leaving LOD2, but for a one-slot prop that means no `<id>_lod2` mesh, and both
   `lod_budget_errors` and the bevy reimport test required one (the runtime already handled it:
   `build_piece` skips a missing LOD, `lod_cover` ends the slot at 400 m). Fix: `LOD2_ABSENT` in
   the gate (owner A25's two small boulders), the hooks module's `slot_triangles` returning an empty
   LOD2 for them, and `kit_scene` warning only for a missing LOD0 or LOD1. The bake report's
   `lod2Drops` names the slot with the owner ruling, since a 2.2 m boulder is about 7 px at 400 m
   and the sub-pixel rule does not apply. Lost: about 15 minutes.
3. **`beach_band` is per family, so a band row would have removed every inland boulder.**
   `off_beach_band` treats a tile with no coastline as off the band. Fix: `scatter::coast_band`
   applies the band only on a coast tile to a piece that also has an ordinary floor slot; beach-only
   dressing keeps it everywhere. Lost: about 10 minutes.
4. **`_rock.build` cannot be reused for larger rocks.** It asserts 256 px/m, always builds three
   LODs and sizes its AO ground plane at ±4 m. `_boulder.py` reuses `_rock.Stone`, `build_high`,
   `granite_material` and `_downsample` (no copies, `_rock.py` untouched) and keeps its own pipeline.
   The moss of `granite_material` reads as green blotches on a boulder's broad top, so
   `_boulder.sparser_moss` raises its noise threshold on the boulders' own material instance.
5. **The outcrop is not a ground box.** The brief asked for a `GROUND_BOXES` row (±4.0 × ±2.5 m),
   but `ground_boxes_match_the_kit` requires the collider to fill 95% of its box and an ellipse fills
   79%; grass instead clears the outcrop's exact ground slice through `grass::ROCK_BASES`, as for
   every other rock. No change needed beyond the `ROCK_BASES` rows.

6. **Appended rules cannot place a 2-4 m rock.** Measured over the density test's 64 seeds
   (`every_dart`): appended last, the meadow outcrop placed 0 of 64 and the forest outcrop never fit
   between 20 placed spruces; leading with the rocks (outcrop, boulders, then the forest's log)
   leaves one shortfall in 81 forest outcrops (a tile that plans two). That re-lays every meadow
   and forest, which moved one spruce to the back of the forest thumbnail, where its tip touched the
   frame (`make thumbs` failed, 2 px); the thumbs look-at was raised by 2 m (`crates/hex-app/src/thumbs.rs`; the binary thumbs patch was dropped at landing, its blob hashes trip gitleaks).
   Coast: on the beach band the outcrop places 0 of 83 and boulders 0 of 171 on `beach_outer`, so
   those rules were left out. Lost: about 40 minutes.
7. **A fixed-world coverage check is luck-sensitive.** `shots-scatter`'s mixed world lost
   `beach_driftwood` (variant a) and `log_fallen_c`: the beach 0-6 cap now picks among boulders
   too, and the forest's log rule moved. Not fixed (only seed-chasing or a bigger world would).
8. **Native shots at host load 200 came back black twice** (`make shots-scatter`); its state dumps
   were complete, so `node docs/checkpoints/scatter/check.mjs target/shots` checked them.

## Proposed skill text

- `gates.md`, Texel density row: "The B1 granite boulders and outcrop take a scoped floor of
  100 px/m (`BOULDER_TEXEL_MIN_PX_PER_M`, owner A25: about 128 px/m), since 256 px/m on a piece
  over about 3 m across needs a 2048² albedo, over the `piece` KTX2 class."
- `gates.md`, LOD slots row: "A one-slot piece leaves LOD2 by shipping no `<id>_lod2` at all; the
  gate allows that only for ids in `LOD2_ABSENT`, and the bake report's `lod2Drops` still names the
  slot with its reason."
- `project.md`, Props: "A rock over about 2 m across is a `_boulder.py` piece, not `_rock.build`:
  that pipeline asserts 256 px/m and three LODs."
- `pitfalls.md`, scatter: "A scatter rock wider than about 2 m leads its tile's rules: appended
  after spruces or flowers it never fits (outcrop 0 of 87 in the forest, 0 of 64 on the meadow).
  Probe rule orders with `every_dart` over the density test's 64 seeds before choosing one."
