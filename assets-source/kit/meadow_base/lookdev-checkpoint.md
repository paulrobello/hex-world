# Look-dev checkpoint package (meadow kit, second revision): as sent to the owner

Prepared by the meadow authoring agent on 2026-10-13 for the owner's look-dev review. The owner's answers are recorded in `assets-source/lookdev/lookdev.json` and `assets-source/kit/meadow_base/approvals.json`.

Every shot below uses the fix-round-1 engine state:
- kit camera exposure `KIT_EV100` = 12.5;
- the kit sun aimed along the HDRI sun direction (0.553, 0.742, 0.379);
- IBL_INTENSITY unchanged at 1500.

Each shot run had host load ≤ 15 before and after.

None of the shots below were committed: they were local output, and the commands named in each section regenerate them.

## 1. The nine look-dev PNGs (`make shots-lookdev`, real meadow, load 13.45 before and 11.91 after)
`kit-overview`, `kit-seam-closeup` and `kit-tree` under each of AgX, TonyMcMapface and ACES.

Measured cap colour in kit-overview (sRGB, mean over a 300×200 region of the cap):

| Tonemap | Cap colour (R, G, B) | Look |
|---|---|---|
| AgX | (121, 147, 95) | soft, slightly desaturated green |
| TonyMcMapface | (102, 138, 69) | |
| ACES | (83, 126, 47) | most saturated and darkest |

For comparison, the grass in the approved references measures (114–142, 133–148, 46–60): ref-1 (127, 148, 46) and ref-2 (142, 142, 57).
- AgX matches the references' brightness (luma ≈ 138 against ≈ 136) but has about twice their blue, so it is less saturated.
- ACES matches the references' hue best but is about one stop darker.

The tree shots show meadow only, because the road and tree pieces are not authored yet (Tasks 9 and 10).

## 2. Exposure sweep (AgX, the default tonemap)
Rendered with `make shots-kit ARGS="--ev100 <v>"`.

| EV100 | Cap colour (R, G, B) in kit-overview |
|---|---|
| 11.5 | (152, 178, 125): still pale |
| **12.5 (chosen)** | (121, 147, 95): matches the references' brightness |
| 13.3 | (96, 123, 69): darker than both references |

For comparison, the previous default (Bevy's EV100 9.7, the old sun) rendered the cap at (206, 225, 184), near white.

Owner decision: confirm 12.5, or choose another value. Any value can be previewed with `--ev100 <v>` or `?ev100=<v>`.

## 3. Hook shots (`make shots-kit ARGS="--kit_hooks <h>"`, at EV100 12.5)
- Snow: the cap turns white (205, 206, 207) and the soil skirt keeps its strata. With the cap white, the meadow swell and the 2 m border falloff read as faint soft creases near the tile edges. This is geometry shading, not a crack.
- Wet: the cap darkens to (109, 130, 78), from (126, 146, 97) dry.
- Night: identical to the plain shot, with no emission, because masks B = 0 on the meadow.
- Plain baseline: `make shots-kit` with no hooks.

## 4. Texel density (from assets-source/kit/meadow_base/provenance.json)
- Unique masks: `texelDensityPxPerM` = min 31.85, median 31.99, max 32.00 px/m. D2 targets 32 px/m.
- Tileable grass and soil: 1024² over tileM 2.0 m = 512 px/m, as D2 targets. Both are sampled in world space.

## 5. HDRI facts
- Name: kloofendal_48d_partly_cloudy_puresky (Poly Haven, 2k).
- License: CC0 (https://polyhaven.com/license).
- Prefiltered specular cubemap `noon.specular.ktx2`: 4,194,680 B, 256² base with a full mip chain, RGBA16F. The diffuse cubemap is 32².

## 6. Question: masks.A = AO
The meadow's masks.A holds the baked AO. Over the hex footprint it measures min 0.969, 1st percentile 0.984 and mean 0.9994, so the gentle relief barely occludes. The kit material multiplies diffuse occlusion by masks.A, and the tile ORM's R channel is never used.

Owner decision: keep masks.A as AO, which costs no extra texture under the B5 freeze, or reserve A for another hook and drop baked AO on bases.

## 7. Carry-overs from the IBL bake
1. **Exposure: resolved in engine.** The kit camera now carries `Exposure { ev100: KIT_EV100 = 12.5 }`. The sweep and the reference comparison are in section 2. IBL_INTENSITY stays at 1500: the sweep showed no sign that the sun and IBL are out of balance, since the cap and the skirt both sit in a sensible range at the same exposure.
2. **Three clamped sun texels** in `noon.specular.ktx2` (float16 overflow, recorded in lookdev.json). The meadow is rough (grass ORM G 0.80–0.91, soil 0.85–0.95), so no sparkle shows on it. Smooth surfaces such as the wet hook (roughness 0.12) and later roads may still sparkle.
3. **HDRI sun vs. directional light: resolved.** The kit DirectionalLight now shines along −(0.553, 0.742, 0.379), the HDRI sun direction. P0 scenarios keep their light at (200, 400, 150).
   - Shadows still render: kit-state reports `shadows: true`, 4 cascades, and draws in every cascade's shadow pass.
   - Before (old light, EV 9.7) and after shots of kit-overview were compared; the exposure also changed between the two, so that pair showed the combined change. On the meadow the light change alone is subtle, because the cap is nearly flat and there are no props to cast shadows yet.

## 8. Other things for the owner to see
- The soil strata repeat twice down each 4 m skirt, because D2 fixes tileM at 2.0 m. Ref-1 shows one set of bands. To keep a single set, raise soil tileM to 4.0 (256 px/m on walls).
- The six-view Blender renders in assets-source/kit/meadow_base/renders/ are evidence only. They show flat factor colours, because the tileable sets bind only in engine.
