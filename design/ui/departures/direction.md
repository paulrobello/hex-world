# Departures

The inventory is a live board ranked by fit, the way a gate board ranks flights by departure. When the hovered slot changes, rows move to their new rank instead of vanishing and reappearing, and anything that just moved holds one amber mark until the player has seen it.

## Direction contract

- **THESIS:** Legality is a ranking you watch change. It refuses the static grid inventory where legal tiles are only highlighted in place.
- **OWN-WORLD:** Black panels, white type, big tabular fit counts in a wide display face, one amber reserved for change (re-ranked rows, the undo row), the selected row inverted to white.
- **STORY:** The player hovers a slot. The board titles itself "Tiles for this slot", re-ranks, and the three tiles that fit slide to the top with their rotation count. The nine that cannot fit sit below at zero with their reason.
- **FIRST VIEWPORT:** The board top left (tile, fits, status columns). A labelled tool dock bottom centre. The undo change-row top right. A chip beside the ghost with the selected tile and its rotation. The world name and save state bottom right.
- **FORM:** Boarding pass and gate board (catalog challenger, fused). Verdict: competitive. It wins on product clarity, because the ranking makes the filter's cause and effect visible, and loses on audience identification, because travel ephemera is not the builder's world. Seed key `dc8cbb29`.
- **SIGNATURE MOVE:** The live re-rank. Rows animate to their new position when the slot changes, preserving identity, and moved rows keep an amber edge until hovered.
- **FINISH:** unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance.

Raises from declined challengers:

- **Raise from the variable-font specimen:** R turns the ghost and every fitting glyph in step.
- **Raise from the warm consumer app:** generous dock targets (84 × 60 px), labels under every icon, plain-language status.

**Honest risk:** The darkest and most data-dense of the three. Over a bright, sunny world it could feel like an operations tool rather than a toy, and the amber "changed" grammar has to stay rare or it becomes noise.

## Tokens

| Group | Values |
|---|---|
| Type | Unbounded (OFL) for the board title, fit counts and world name only. Manrope (OFL) for everything else. 12 / 13 / 14 / 16 / 22 px (`--fs-1..5`). Column heads 11 px, 700, uppercase, tracked +0.08em. |
| Spacing | 4 / 8 / 12 / 16 / 24 px. Radius 2 px. Rows 48 px (fit) and 34 px (blocked). |
| Colour | Black `rgb(9 10 11)` at 95 %, ink `rgb(246 246 243)`, ink-2 `rgb(178 182 184)`, ink-3 `rgb(128 133 136)`, amber (change only) `rgb(255 178 36)`. States: compatible `rgb(92 218 145)`, incompatible `rgb(255 96 80)`, neutral edge `rgb(230 232 230 / .5)`. Selected row is ink on black, inverted. |
| Socket colours | Grass `rgb(128 176 88)`, Forest `rgb(54 116 70)`, Road `rgb(214 214 208)`, River `rgb(86 170 224)`, Beach `rgb(224 196 132)`, Ocean `rgb(48 110 164)`. |
| Iconography | The shared authored 24 px SVG set at 1.8 px stroke, always with a text label under it in the dock. |
| Motion | `cubic-bezier(.16,1,.3,1)`. Re-rank 320 ms per row, staggered 40 ms, with an amber wash that fades as the row lands. Hover 120 ms. Tool press flashes amber for 200 ms. Board slides off the left edge in 200 ms. Undo row drops 8 px over 200 ms and dismisses at 4 s. Reduced motion jumps rows to their rank with no travel and keeps the amber edge as the only change signal. |
