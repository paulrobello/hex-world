# Kit slots with no LOD1 mesh (owner A2 Q4, 2026-10-21)

The owner wants every piece to have a middle LOD. Every kit GLB has all three LOD meshes (`<id>_lod0`, `_lod1`, `_lod2`), but in these pieces the LOD1 mesh leaves out one or more material slots. The LOD2 mesh leaves them out too. Before the interim fix, those slots disappeared beyond 150 m.

**Interim fix (no asset work).** A slot with no LOD1 mesh now draws its LOD0 mesh through the LOD1 range. It fades out over the 395–405 m band, where LOD2 also leaves it out. LOD0 is the fallback because it is the only mesh these slots have. Both draw paths do this:

- `scene::lod_cover` on the per-entity path.
- `kit_instances::slot_lod_map` on the instanced path.

**The count.** 17 material slots are affected, across 16 pieces and 23 (piece, slot) entries. `river_cattail` is shared by seven river pieces.

**The tris column.** It gives the LOD0 triangles of the slot in that piece, which is what now draws from 150 m to 400 m. It is read from the GLB index accessors (`assets/kit/manifest.json` → each piece's `glb`).

| Piece | Pack | Slot | Fallback LOD0 tris |
|---|---|---|---|
| `house_pad` | village | `house_pad_stone` | 30 |
| `well_stone` | village | `well_stone_mortar` | 40 |
| `well_stone` | village | `well_stone_stone_buff` | 350 |
| `well_stone_b` | village | `well_stone_b_mortar` | 40 |
| `well_stone_b` | village | `well_stone_b_stone_buff` | 350 |
| `windmill` | village | `windmill_iron` | 12 |
| `windmill_b` | village | `windmill_b_iron` | 58 |
| `bakery_shop` | village | `bakery_shop_bread` | 34 |
| `bakery_shop` | village | `bakery_shop_wicker` | 26 |
| `bakery_shop_b` | village | `bakery_shop_b_bread` | 34 |
| `bakery_shop_b` | village | `bakery_shop_b_wicker` | 26 |
| `rail_station_platform` | village | `rail_station_board` | 4 |
| `rail_station_platform` | village | `rail_station_flowers` | 44 |
| `rail_station_platform` | village | `rail_station_timber` | 32 |
| `rail_station_shelter` | village | `rail_station_coping` | 20 |
| `rail_station_shelter` | village | `rail_station_iron` | 16 |
| `river_straight_center` | water | `river_cattail` | 638 |
| `river_bend_center` | water | `river_cattail` | 726 |
| `river_bend_60_center` | water | `river_cattail` | 550 |
| `river_road_bridge_center` | water | `river_cattail` | 396 |
| `river_rail_bridge_center` | water | `river_cattail` | 396 |
| `river_spring_center` | water | `river_cattail` | 660 |
| `river_fork_center` | water | `river_cattail` | 616 |

## Why the earlier count looked larger

The instanced path grouped a piece's primitives into LOD families by their ordinal across the whole LOD mesh. A LOD that dropped a slot therefore shifted every later slot's ordinal and split that slot into unpaired families. By that count, 97 families had no LOD1, including whole slots of `well_stone` and the houses. The split did not change what was drawn, because each half covered its own range, but it meant a "no LOD1" count could not be trusted.

Families are now keyed on the ordinal among a material's own primitives (`chunks::KitFamilies::build`). Only the 23 entries above lack LOD1.

## For the asset pass

Each entry needs the slot kept in the LOD1 mesh, and in the LOD2 mesh where it still reads at 400 m. The meshes must stay inside the §10 budgets: LOD1 at most 50% of LOD0, and LOD2 at most 15% of LOD0. When a piece gains its LOD1 slot, the fallback stops applying to it with no code change.
