# barn_red: owner review pending (2026-10-19)

Status: **pending**. Not an approval; the controller records the owner's answer.

- **Sheet:** `docs/checkpoints/barn_red/sheet.jpg` (refs, textured EEVEE still, in engine with tilt-shift off: play distance, sun-side and shade-side close-ups, eye level 1.7 m, scale beside the oak, and the EEVEE scale shot beside house_twostorey and house_cottage). Re-take: `bash docs/checkpoints/barn_red/run.sh`.
- **Built to A17 over the refs:** a pitched 45° slate roof (ref-2's gambrel not followed), red vertical board-and-batten walls, white trim, large braced double doors and a hayloft door on the +x gable. Building only (no field, fence, bales or yard). 10 m x 7 m, ridge 7.8 m: the largest footprint in the village family (house_twostorey 5 x 7.2 m, ridge 7.1 m; the windmill is taller).
- **Questions for the owner:**
  1. The red (manifest tint [0.45, 0.07, 0.045] on plaster, linear luminance about 0.10; doors darker [0.32, 0.055, 0.04]) reads as barn red on the sun side, but the shade side goes near-black under the default grade (sheet: "shade side"), as the houses' dark timber and slate do. Options: (a) keep it; (b) lift the value to about luminance 0.13-0.15, still clearly red, and re-shoot the shade side (a generator and manifest change, then rebuild and provenance).
  2. Is the barn's size right beside the houses, or should it be smaller (for example 8 x 6 m)?
  3. The refs show a lean-to side shed (ref-1) and open doors (ref-2). The barn ships closed doors and no lean-to. Add either?
  4. At eye level a 2.5 cm batten side seen edge-on can alias into a thin dashed line (sheet: "eye level", above the hayloft door). Accept, or widen the battens' proud depth to 4 cm?
