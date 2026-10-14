# fish_trout friction log

Shared with `bird_starling` (builder `scripts/blender/kit/_fauna.py`); see its log for the shared items.

1. **Hooks gate and an under-water prop.** The §9.4 hooks gate requires masks R (snow) > 0 on every
   slot outside its `SUBMERGED` list. A fish only ever swims inside the water volume, so authoring snow
   capacity on it would be wrong; `fish_trout_body` is added to `SUBMERGED` in
   `crates/hex-render/tests/hooks/mod.rs` with that reason, and its masks carry R = 0, G = 1. No skill
   change: the list is the documented mechanism. Lost about 2 minutes.
2. **Two palettes without a second texture.** The manifest has no variant field (`deny_unknown_fields`),
   and a palette tint per slot cannot turn a silver fish olive with spots. Both palettes share one
   atlas: UV0 maps every LOD into the left half (ocean) and the river variant samples at UV0 + (0.5, 0),
   with the normal, ORM and masks maps duplicated into both halves. The builder asserts that no island
   plus its gutter spills past the half. No skill change yet: one piece uses it; promote it to
   `project.md` if a second variant piece appears. Lost about 0 minutes.
