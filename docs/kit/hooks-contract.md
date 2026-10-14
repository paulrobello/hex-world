# Atmosphere hook contract (PRD §9.4, D7)

**Status: in force (P2, 2026-10-14).** Every MVP kit piece and every shader the kit and water draw with carries the six §9.4 hooks, and every hook is inert at the MVP values: noon, no snow, dry, summer tint, open water. The asset gate holds the manifest and the shaders to this contract (`crates/hex-render/tests/hooks/mod.rs`, a module of `kit_gate`, so `make kit-check` and `make test` run it).

Sources: `crates/hex-render/src/hooks.rs` and `hooks.wesl`, `assets/shaders/kit_material.wesl`, `car_material.wesl` and `water.wesl`, `crates/hex-render/src/kit_material.rs`, `car_material.rs` and `water.rs`.

## Channels

No hook adds a sampled texture. Kit forward is already at WebGPU's 16 next to refracting water, so hooks use the masks texture (binding 101), uniforms, or the mesh uniform's tag.

| Hook | Asset side | Engine side | MVP value |
|---|---|---|---|
| Emissive, night-only intensity | masks **B**: 1 on emitters (house window glass, car head and tail lights), 0 elsewhere | `night` uniform × B × 4 added to emissive | `night` 0 |
| Snow accumulation and up-facing coverage | masks **R**: snow capacity, 0 where snow never lies (glass, seabed) | `snow` × R × smoothstep(0.55, 0.85, N.y) | `snow` 0 |
| Wetness and porosity | masks **G**: porosity, 0 under water | `wetness` × G darkens and glosses | `wetness` 0 |
| Foliage seasonal tint index | nothing to author: every alpha-tested slot and every non-bark tree slot is foliage (`hooks::is_foliage`) | `FLAG_FOLIAGE` slot × the instance's `MeshTag` through `season_tint`: 0 summer, 1 early autumn, 2 late autumn | tag 0 (untagged) |
| Time-of-day block | nothing to author | `hooks::TimeOfDay` drives the sun (direction, lux, colour), the sky (IBL intensity), fog (`DistanceFog`) and exposure (EV100 offset) through `hooks::atmosphere`, and the `night` uniform | hour 12, whose row is the look-dev rig exactly |
| Water freeze | nothing to author | `WaterParams::freeze`, read only by the `WATER_FREEZE` pipeline variant (ice holds the surface at rest, a pale rough sheet hides the water below) | 0, variant off |

`MeshTag` on a `KitMaterial` entity is reserved for the season index: any other use tints a foliage slot. Cars use their own material (`CarMaterial`), where the tag is the paint colour.

masks **A** stays baked AO. A slot that binds no masks map takes the shader's uniform default while a weather hook is on: full snow on up-facing faces, full porosity, no emitters, no AO (`hooks.wesl` `NO_MASKS`).

## What the gate checks

- **Every slot** carries an authored masks map (`"masks": true` and a `masks` map on the piece), or is listed: `MASKS_EXEMPT` (the hook is meaningless, with the reason) or `KNOWN_MISSING` (a real gap awaiting re-authoring). A new piece is covered without editing the gate, and a listed entry that no longer applies fails, so the list only shrinks.
- **Every authored slot's channels**, sampled at its triangles in every LOD that draws it (each LOD has its own atlas islands, so a coarse LOD can lose a channel): R > 0 somewhere, except glass and the always-submerged seabeds (`coast_seabed`, `beach_seabed`); G > 0 somewhere, except the seabeds; and a piece with a `*_glass` slot has B > 0 (its lights).
- **Foliage:** every alpha-tested slot and every non-bark tree slot gets `FLAG_FOLIAGE` through the runtime `slot_requests` and `params_for`, and every tree has one.
- **MVP values:** `params_for(KitHooks::None)` is neutral for every slot, for kit and car materials alike. The default hour is noon. Open water does not compile `WATER_FREEZE`.
- **Shaders:** every `.wesl`/`.wgsl` under `assets/shaders/` and `crates/hex-render/src/` is in `HOOKED`, with the hooks it must carry, or in `SHADER_EXEMPT`, with the reason. Comments are stripped before matching. The kit and car shaders keep the owner-approved inline masks math, verbatim, and `hooks.wesl` `surface` carries the same math for unmasked slots.
- **Uniform blocks:** `KitParams`, `CarParams` and `WaterParams` match their WGSL structs field for field, and carry their hook fields.

Each check has a `gate_catches_*` test that removes its hook and expects the failure.

## Known gaps (re-authoring)

These slots run on the uniform default until their piece ships an authored masks map bound to the slot. Re-authoring is a Blender rebuild of the piece and needs owner approval of the family again.

| Piece | Slots | Missing |
|---|---|---|
| `ocean_center` | `coast_pebble`, `coast_rock` | no masks map (R snow, G wetness, A AO) |
| `beach_center`, `beach_outer_center`, `beach_inner_center` | `beach_dunegrass`, `beach_driftwood`, `beach_pebble` | no masks map |
| `river_straight_center`, `river_bend_center` | `river_stone`, `river_reed`, `river_cattail` | no masks map |
| `river_road_bridge_center` | `bridge_stone`, `river_stone`, `river_reed`, `river_cattail` | the piece has a masks map but these slots do not bind it (`masks: false`); the atlas may not cover their UVs |

Plant slots outside the foliage rule do not take the seasonal tint: `house_pad_hedge`, `beach_dunegrass`, `river_reed` and `river_cattail`. They need a foliage marker in the manifest (a `Slot` field) or to become alpha-tested, which is a manifest and asset change.

The boid shader (`boid_material.wesl`) is unlit, so it follows neither the time-of-day block nor fog. It takes the hooks when the boids are lit.

## Verification shots

`docs/checkpoints/hooks/shots.sh` renders the kit and coast checkpoints under each `--kit_hooks` variant (`none`, `snow`, `wet`, `night`, `dusk`, `autumn`, `freeze`) into `docs/checkpoints/hooks/`. `none` is the MVP look.
