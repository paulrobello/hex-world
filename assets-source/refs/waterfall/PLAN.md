# Waterfall fall faces: plan for the reference gate

**Status:** pending owner review (requested 2026-11-22). `approval.json` is a pending request, not an approval: its `approved` list is empty, and the controller records the owner's pick. Nothing is modelled. Phase 5 modelling (Task 17 onward in `docs/superpowers/plans/2026-11-22-rivers-at-levels.md`) starts only after the owner approves a reference.

Sources: the approved spec `docs/superpowers/specs/2026-11-22-rivers-at-levels-design.md` (section 4, "Waterfall geometry", and RQ2, RQ5, RQ9 as answered on 2026-11-22). Owner answers that bind this piece:
- **RQ2:** O1 for the 4 m drop (grey granite ledges in the boulder and massif palette, moss and dark wet streaks, a straight lip). O2 for the 8 m and 12 m drops (stepped ledges that break the sheet into two or three tiers). One generator builds all three, through reference approval.
- **RQ5:** the falling sheet is runtime water, a clear refracting sheet at the lip turning white below at High and Ultra, and a thick white sheet at Low and Medium. It is not part of this piece.
- **RQ9:** the lower bed is not deepened. The plunge pool reads through runtime foam and ripples.
- **Scatter is never baked** (owner 2026-10-26). Boulders, reeds and lilies round the pool are runtime scatter.
- **Visual standard** (owner 2026-10-29): the face must match the approved granite (`boulders` ref-1, `massif_granite` ref-2, `peak_granite` ref-1, the walls) and the kit's quality, and the owner sees in-engine shots before merge.

**The refs set the look, not the geometry.** No candidate draws the hexagons exactly, none reaches a true 12 m scale against the tiles (ref-7 has the three tiers but reads nearer 8 m), and the pool sits wherever the model put it. The frame and the rings below set the geometry.

Sheet: `docs/checkpoints/waterfall/refs-sheet.jpg` (built by `docs/checkpoints/waterfall/sheet.py`). Its top row is the approved family the face must match and the in-engine soil cliff column it covers. Then one row per drop. Prompts: `scripts/refs/higgsfield-refs.sh` case `waterfall` (`make refs PIECE=waterfall`), with the re-roll history in comments there. 11 generations (9 first rolls, ref-7 re-rolled, ref-10 added), under the cap of 12.

## 1. Candidates

4 m drop (`waterfall_l1`, RQ2 O1):
- **ref-1:** the pair of tiles from the front at about 45°. Rounded, weathered pale granite blocks flank a single smooth sheet over a straight lip, with cushions of moss and lichen. The dark earth column carries on beyond the rock on the right. This is the closest match to the boulders and massif granite.
- **ref-2:** a three-quarter close-up of the lip. It has the best wet detail (dark streaks behind the sheet, glossy wet stone, moss on the ledges). Its lip is a dressed coping of squared blocks, which reads as a built weir, and it sits on a wooden tray, not the O4 skirt. Take only the wet-stone detail from it.
- **ref-3:** asked for eye level and came back as a front view, like ref-1. Two massive monolithic blocks flank the sheet. This is a blockier alternative to ref-1.

8 m drop (`waterfall_l2`, RQ2 O2):
- **ref-4:** the pair from the front. The sheet falls onto one broad granite ledge and breaks into a second white sheet. It uses the same rounded granite family as ref-1. The drop reads as about 5 to 6 m.
- **ref-5:** a close-up of a built two-tier cascade of squared masonry courses with a dressed coping, over a granite plinth and a square basin. It reads as a garden fountain, not a natural fall, and it has some brown blocks. Not recommended.
- **ref-6:** asked for eye level and came back as a front view. It shows two tiers in rounded granite with moss and wet streaks, close to ref-4. A stray cut-face colour shows on the raised tile's right wall.
- **ref-10:** eye level from the lower bank, 1.7 m up and about 12 m downstream. Two tiers fall between big rounded lichen-spotted granite blocks with moss on the ledges. The raised ground's dark earth wall shows beyond the rock on both sides, under its meadow lip. This is the walker's view, and it shows the face set into the soil column, which is the layout this piece must have.

12 m drop (`waterfall_l3`, RQ2 O2):
- **ref-7** (re-rolled): the pair from the front. Three clear tiers step down over broad granite ledges in the ref-1 family. The drop reads as about 8 m, not 12 m.
- **ref-8:** a close-up of three drops below a dressed coping of squared blocks, over rounded boulders, some of them dark brown. It is closer to a built weir than to the natural face.
- **ref-9:** asked for eye level and came back as a low front view. A narrow rock face with two or three ledges is set into tall dark-earth walls, with soil on both sides of the rock. This shows the layout, though its square pool and short drop are not to scale.

**Recommended pick: ref-1 as the design, with ref-4's single ledge for 8 m and ref-7's three tiers for 12 m, and ref-10 as the eye-level and layout guide** (a rock face set into the soil column, soil beyond it on both sides). These four form one coherent set: the same rounded pale granite with lichen and moss, a straight natural lip, and one more ledge per added level. Each height is the 4 m design with ledges added below the lip, so one generator scales it by drop. ref-2's wet streaks and glossy wet stone guide the masks. Its dressed lip does not. ref-3 is the alternative if the owner prefers fewer, larger blocks.

## 2. Pieces

| Piece | Drop | Form | Face size (w x h) |
|---|---|---|---|
| `waterfall_l1` | 4 m (1 level) | straight lip, one sheet (RQ2 O1) | 14 m x 4 m |
| `waterfall_l2` | 8 m (2 levels) | straight lip and one ledge, two tiers (RQ2 O2) | 14 m x 8 m |
| `waterfall_l3` | 12 m (3 levels) | straight lip and two ledges, three tiers (RQ2 O2) | 14 m x 12 m |

- **One generator:** `scripts/blender/kit/_waterfall.py` (spec A42 O1), with a thin `waterfall_l1.py`, `_l2.py` and `_l3.py` each setting the drop. Every height reuses the 4 m lip course and adds ledges below it. The same rock modules repeat down the face, so the three heights can share one bake (as `rail_viaduct_l1..l3` share one).
- **Pack:** `water`. **Kind:** a seam piece drawn under the upper tile's root, a new kind. Its `kit_gate` branch and failing test come first (Task 17: `check_waterfall_face`).
- **Budgets** (spec section 7): LOD0 at most 4,000 tris, LOD1 at most 50%, LOD2 at most 15%. KTX2 class `piece` (at most 1.5 MB). **No new sampled texture:** the face is a standard `KitMaterial` slot (albedo, normal, ORM, masks), so the 16-texture limit next to refracting water holds (ADR 0002).
- **Masks:** emissive stays 0, and wetness (masks G) is near 1 on the face and the ledge tops in the spray, so the runtime wetness hook darkens and glosses it (hooks contract). Moss and lichen are in the albedo.
- **Texture route:** a face of up to 168 m² plus the ledge tops may not fit a unique bake per height inside 1.5 MB. The options are one bake shared by the three heights over repeated rock modules (the viaduct's route), or the portal's shared granite islands. The builder picks after measuring texel density (`references/gates.md`).

## 3. Geometry

Frame: runtime metres in the upper tile's frame. The piece is built for edge 0 (facing +x) and placed by rotation, as edge pieces are. Hex sides are 16 m long, and the edge-0 border plane is x = 13.856 (the apothem). `LEVEL_STEP` is 4 m.

- **Where it sits:** on the upper tile's cliff column at the waterfall seam. The column plane is the border plane, and the column's soil is `meadow_base`'s skirt (`play.rs` `spawn_column`). The face covers the column across the channel and about 3 m each side: 14 m wide, |z| ≤ 7, centred on the lane. That covers the whole 13.66 m channel cut (bank tops at d = 6.83 m, water contract section 2) and leaves about 1 m of soil column at each corner. Nothing of the column shows behind the sheet (spec anti-goal: no cut face or soil wall behind the water).
- **Top ring:** on the river channel profile at the upper level, `y = 4·L_hi − 1.4 + h(d)`: the bed at −2.95 relative on the centre line, the waterline at d = 4, and meadow level from d = 6.83 m out. The lip is the channel bed at the seam, so the runtime sheet leaves the upper water surface at `4·L_hi − 1.4`. Gate tolerance (Task 17): ±0.03 m below 2.5 m of water, ±0.10 m elsewhere.
- **Bottom ring:** on the lower tile's bank profile at the seam, `y = 4·L_lo − 1.4 + h(d)`, so the rock runs down into the lower channel and its banks with no gap. The pool's water surface is `4·L_lo − 1.4`.
- **Side ends:** flush with the column's skirt plane at |z| = 7, so the face meets the soil column with no crack and no overhang.
- **Ledges** (8 m and 12 m): level shelves at about every 4 m of drop, stepping out no more than about 1.5 m past the border plane into the lower tile's channel. Further out they would crowd the pool's foam disc, which spans about 6 m from the seam.
- **LODs:** the face touches the border on three sides, so LOD1 and LOD2 must keep every top, bottom and side ring vertex. Only the interior rock decimates. Border-touching pieces are otherwise never decimated (blender-asset SKILL.md), so the Task 17 gate has to check the rings at every LOD.
- **Collider:** a box hull of the face, so the walker meets it as a wall, as at any cliff. The sheet has no collider (spec section 6).

## 4. Runtime integration notes (for the orchestrator)

- **The sheet must follow the ledges.** The spec draws the sheet as one curved ribbon from the lip down to the lower water surface. At 8 m and 12 m (RQ2 O2) the sheet breaks on each ledge, so a single ribbon would pass through the rock or hang in front of it. Recommendation: the piece exports each tier's lip edge as a glTF socket (`lip_0`, `lip_1`, `lip_2`: centre point, width and outward normal), and the runtime sheet emits one ribbon per tier from those sockets, with a foam churn where each tier lands. This touches the water mesh builder (Phase 4 in the plan), not this piece. It is listed as WF-Q3 on the review page.
- **Piece tables** (returned as `docs/checkpoints/waterfall/integration.patch` at build time): `OBSTACLES` (the face collider), `prop_kind` (a label), `NOT_GROUND` (the lip and face are neither walker nor scatter ground), `FOOTPRINTS` only if it is classed as a hero or scatter piece, and `kit_scene.rs` a `block_*` staircase of 1-, 2- and 3-level falls with `tile_counts`, or a `NOT_SHOWCASED` reason. `CAR_GROUND` and `HERO_VARIANTS` are untouched.
- **Scatter:** no bank props within about 6 m of the seam on either side (spec section 6). The optional `river_stones` ring round the pool is a runtime rule, never baked.
- **Thumbnails:** waterfalls exist only between placed tiles, so `make thumbs` must dirty nothing (`play::NoFalls`).

## 5. Open questions for the owner

1. **WF-Q1. Pick the references.** Recommended: ref-1 as the design, ref-4 for 8 m, ref-7 for 12 m and ref-10 as the eye-level and layout guide, with ref-2's wet detail.
2. **WF-Q2. Natural lip or dressed coping?** ref-1, 3, 4, 6, 7, 9 and 10 have a natural granite lip. ref-2, 5 and 8 have a course of squared dressed blocks, which reads as a built weir. Recommended: natural, to match the massif, the boulders and the natural river banks.
3. **WF-Q3. The sheet at 8 m and 12 m breaks on each ledge.** Recommended: the piece exports each tier's lip as a socket, and the runtime sheet draws one ribbon per tier (section 4). The other option is one ribbon over a face whose ledges sit behind it, which loses the tiers that RQ2 O2 chose.
4. **WF-Q4. Face width.** Recommended: 14 m as the spec says, covering the whole channel cut with about 1 m of soil column at each corner (ref-9 and ref-10 show rock framed by soil). The other option is the full 16 m side, corner to corner, which needs corner pieces where the face meets the column's other sides.
