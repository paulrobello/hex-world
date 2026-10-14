# Atmosphere hook contract (PRD §9.4, D7)

**Status: in force (P2, 2026-10-14).** Every MVP kit piece and every shader the kit and water draw with carries the six §9.4 hooks, and every hook is inert at the MVP values: noon, no snow, dry, summer tint, open water. The asset gate holds the manifest and the shaders to this contract (`crates/hex-render/tests/hooks/mod.rs`, a module of `kit_gate`, so `make kit-check` and `make test` run it).

Sources: `crates/hex-render/src/hooks.rs` and `hooks.wesl`, `assets/shaders/kit_material.wesl`, `car_material.wesl` and `water.wesl`, `crates/hex-render/src/kit_material.rs`, `car_material.rs` and `water.rs`.

## Channels

No hook adds a sampled texture. Kit forward is already at WebGPU's 16 next to refracting water, so hooks use the masks texture (binding 101), uniforms, or the mesh uniform's tag.

| Hook | Asset side | Engine side | MVP value |
|---|---|---|---|
| Emissive, night-only intensity | masks **B**: 1 on emitters (house window glass, car head and tail lights), 0 elsewhere | `night` uniform × B × 4 added to emissive | `night` 0 |
| Snow accumulation and up-facing coverage | masks **R**: snow capacity, 0 where snow never lies (glass, seabed) | `snow` × R × smoothstep(0.55, 0.85, N.y) | `snow` 0 |
| Wetness and porosity | masks **G**: porosity, 0 under water | `wetness` × G darkens and glosses. On a `wetBand` slot (coast sand, water contract §3) it is scaled by 1 minus the live wet band's wetness, so rain wets only what the swash left dry | `wetness` 0 |
| Foliage seasonal tint index | nothing to author for alpha-tested slots and non-bark tree slots; an opaque plant elsewhere sets `"foliage": true` on its manifest slot (`hooks::is_foliage`) | `FLAG_FOLIAGE` slot × the instance's `MeshTag` through `season_tint`: 0 summer, 1 early autumn, 2 late autumn | tag 0 (untagged) |
| Time-of-day block | nothing to author | `hooks::TimeOfDay` drives the sun (direction, lux, colour), the sky (IBL intensity), fog (`DistanceFog`) and exposure (EV100 offset) through `hooks::atmosphere`, and the `night` uniform | hour 12, whose row is the look-dev rig exactly |
| Water freeze | nothing to author | `WaterParams::freeze`, read only by the `WATER_FREEZE` pipeline variant (ice holds the surface at rest, a pale rough sheet hides the water below) | 0, variant off |

`MeshTag` on a `KitMaterial` entity is reserved for the season index: any other use tints a foliage slot. Cars use their own material (`CarMaterial`), where the tag is the paint colour.

masks **A** stays baked AO. A slot that binds no masks map takes the shader's uniform default while a weather hook is on: full snow on up-facing faces, full porosity, no emitters, no AO (`hooks.wesl` `NO_MASKS`).

## What the gate checks

- **Every slot** carries an authored masks map (`"masks": true` and a `masks` map on the piece), or is listed: `MASKS_EXEMPT` (the hook is meaningless, with the reason) or `KNOWN_MISSING` (a real gap awaiting re-authoring). A new piece is covered without editing the gate, and a listed entry that no longer applies fails, so the list only shrinks.
- **Every authored slot's channels**, sampled at its triangles in every LOD that draws it (each LOD has its own atlas islands, so a coarse LOD can lose a channel): R > 0 somewhere, except glass and the always-submerged slots (`SUBMERGED`: the seabeds `coast_seabed` and `beach_seabed`, and `ocean_center`'s `coast_pebble` and `coast_rock`, which lie on the ocean floor under at least 2.5 m of water); G > 0 somewhere, except the submerged slots; and a piece with a `*_glass` slot has B > 0 (its lights).
- **Foliage:** every alpha-tested slot, every non-bark tree slot and every slot in the gate's `PLANTS` list (`house_pad_hedge`, `beach_dunegrass`, `river_reed`, `river_cattail`) gets `FLAG_FOLIAGE` through the runtime `slot_requests` and `params_for`, and every tree has one. The manifest's `"foliage": true` is checked against `PLANTS`, never against itself: a listed plant without the flag fails, a flagged slot not listed fails, and a list entry that names no slot fails.
- **MVP values:** `params_for(KitHooks::None)` is neutral for every slot, for kit and car materials alike. The default hour is noon. Open water does not compile `WATER_FREEZE`.
- **Shaders:** every `.wesl`/`.wgsl` under `assets/shaders/` and `crates/hex-render/src/` is in `HOOKED`, with the hooks it must carry, or in `SHADER_EXEMPT`, with the reason. Comments are stripped before matching. The kit and car shaders keep the owner-approved inline masks math, verbatim, and `hooks.wesl` `surface` carries the same math for unmasked slots.
- **Uniform blocks:** `KitParams`, `CarParams` and `WaterParams` match their WGSL structs field for field, and carry their hook fields.

Each check has a `gate_catches_*` test that removes its hook and expects the failure.

## Known gaps (re-authoring)

None: the gate's `KNOWN_MISSING` list is empty, and any new gap fails the gate until it is authored or listed with a reason.

The 21 small prop slots that were listed (2026-10-14) now bind authored masks without changing their GLBs. Each map is an analytic field of (x, y) on the piece's planar UV0, with A = 1, the AO those slots had before (none), so the MVP look is unchanged. Owner re-approval is pending in each piece's `approvals.json`.

| Piece | Slots | Masks |
|---|---|---|
| `ocean_center` | `coast_pebble`, `coast_rock` | uniform R 0, G 0, B 0, A 1 (always under the sea, so in `SUBMERGED`) |
| `beach_center`, `beach_outer_center`, `beach_inner_center` | `beach_dunegrass`, `beach_driftwood`, `beach_pebble` | R 1; G porosity 0.55 at the grass edge rising to 1.0 toward the damp band (`_beach_dress.masks_field`); B 0; A 1 |
| `river_straight_center`, `river_bend_center` | `river_stone`, `river_reed`, `river_cattail` | R 1; G porosity 1.0 at the waterline easing to 0.8 up the bank (`_river_dress.masks_field`); B 0; A 1 |
| `river_road_bridge_center` | `river_stone`, `river_reed`, `river_cattail`, `bridge_stone` | the existing map: the river centres' field over the dressing's own texels and a 32 px halo (so the unbaked A = 0 background stays out of its mips), and R 1, G 0.8, B 0, A 1 on the causeway's parked texel block (`_river_bridge.dress_masks`); the road's AO texels are unchanged, and so are its mips 0 to 4 |

`bridge_stone` is a world-space tileable slot whose UV0 is parked on one texel, so its masks are uniform. Wetness that varies with height on the causeway would need a real UV0 for it.

The boid shader (`boid_material.wesl`) is unlit, so it follows neither the time-of-day block nor fog. It takes the hooks when the boids are lit.

## Verification shots

`docs/checkpoints/hooks/shots.sh` renders the kit and coast checkpoints under each `--kit_hooks` variant (`none`, `snow`, `wet`, `night`, `dusk`, `autumn`, `freeze`) into `docs/checkpoints/hooks/`. `none` is the MVP look.

`docs/checkpoints/hooks/masks.sh` renders a coast, a river with the road bridge, the river bend, an ocean tile and a far overview (`masks.json`, play scenario, agents paused) under `none`, `snow`, `wet`, `night` and `autumn`, and the coast checkpoint's overview, headland and bay under `none` and `snow`, into `docs/checkpoints/hooks/masks-<variant>/`, beside the same views from the build before the prop masks (`*-before.jpg`). At `none`, outside each frame's run-to-run noise (two runs of each build), at most 54 isolated pixels of 1.6 M differ per frame, by at most 6 levels of 255 in the static frames. The straight-coast frame has zero run-to-run noise and shows 6 such pixels on the beach dressing's silhouettes. A bisect puts them on binding the masks map, not on the foliage flag, although the masked shader math is an identity at the MVP values.
