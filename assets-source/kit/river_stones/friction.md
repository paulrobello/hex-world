# Friction: riverbank scatter props (river_stones, _b, _c, river_reeds, _b), 2026-10-26

1. **A stone-only piece fails the wind gate with the river slots' manifest flags.** Copying
   `river_straight_center`'s `river_stone` slot entry (`"wind": 0.5`) onto a piece with no blades
   fails `kit_gate` with "wind slots never sway (U = 0 everywhere)". Fix: the three stone clusters'
   `river_stone` slot carries no `wind`; the reed pieces keep it (their spikes are in that slot and
   sway with the stalks). 2 minutes. No skill change: the gate message names the fix.
2. **A prop on a slope floats or sinks with the scatter's ring-minimum stand.** `scatter::stand`
   seats a piece at the lowest height under its centre and a 0.3 m ring, 0.25 m under the centre on
   the river bank's slope, so a stone cluster spread round its pivot floated on the downhill side.
   Fix: the clusters are strung along +x within 0.2 m, the scatter lays +x along the bank's contour
   (`contour`) and seats bank props at the centre's own height. 10 minutes. Proposed skill text
   (pitfalls.md, scatter props): "A scatter prop for sloping ground (river banks, coast shelves)
   is laid out along one axis and placed along the slope's contour at its centre height; a round
   cluster on a slope floats on one side."
3. **Banding by distance from a lane polyline is off on bends.** `river_bend`'s network has one
   interior point, so its polyline cuts the 24 m arc by up to 0.8 m, more than the 1 m band. Fix:
   the band is the river profile's height under the centre (`bank_band`), exact on every layout.
   5 minutes. No skill change (scatter code, documented in `scatter.rs`).
