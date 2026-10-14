# Legend

The HUD is the margin of a survey map sheet. The socket kinds are map classes (grass, forest, road, river, beach, ocean), so every tile is keyed by its six edge colours the way a map legend keys its symbols, and the hovered slot prints its own key: what it needs.

## Direction contract

- **THESIS:** The legal-slot filter is a legend you read at a glance. It refuses both the cozy-card inventory and the sci-fi glass HUD.
- **OWN-WORLD:** Deep survey-blue panels, map-class colours used only as data, one magenta for the current tool and selection, crisp grotesk type with tabular figures, six-block socket codes beside every tile.
- **STORY:** The player hovers a slot. The sheet prints "This slot needs: River on one edge, Grass or forest beside it". Tiles that fit sit under "Fits here" with their socket code. The rest drop to compact rows that state why.
- **FIRST VIEWPORT:** A labelled tool bar top left (Place, Rotate R, Undo ⌘Z, Redo, Bulldoze B, then Photo and Settings). A full-height legend sheet on the right with the slot key, the fitting tiles and the blocked tiles. A tag beside the ghost names the selected tile and its rotation. Undo appears as a note bottom left.
- **FORM:** Topographic map legend. Position 1 of 7 on the grounded list (the pick). Seed key `dc8cbb29`.
- **SIGNATURE MOVE:** The slot key. A small hex diagram draws exactly which edges of the hovered slot are touched and by what, and each tile's six-block socket code lines up against it.
- **FINISH:** unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance.

Raises from declined challengers:

- **Raise from the catalog sleeve:** state speaks in one code. The same six socket colours drive the glyph edges, the socket code strip, the slot key and the in-world edge glow.
- **Raise from the acetate tab manual:** the sheet is grouped by consequence (fits, does not fit), never by category, so the order itself carries the filter.
- **Raise from the variable-font specimen:** R rotates the ghost and every fitting glyph in step.

**Honest risk:** This is the most familiar shape of the three (tool bar plus right inspector), close to what most builders ship. Its distinctiveness rests on the slot key and socket codes, which must survive later screens or it collapses into a generic panel.

## Tokens

| Group | Values |
|---|---|
| Type | Manrope (OFL), one family, tabular figures. 12 / 13 / 14 / 16 / 20 px (`--fs-1..5`). Headings 800, names 700, body 500–600. |
| Spacing | 4 / 8 / 12 / 16 / 20 / 28 px. Radius 6 px on panels, 4 px on rows and buttons. |
| Colour | Sheet `rgb(18 39 58)`, sheet-2 `rgb(25 52 76)`, ink `rgb(238 243 246)`, ink-2 `rgb(176 197 214)`, ink-3 `rgb(132 156 176)`, pick (tool and selection) `rgb(198 46 104)`. States: compatible `rgb(123 224 143)`, incompatible `rgb(255 93 82)`, neutral edge `rgb(214 228 240 / .55)`. |
| Socket colours | Grass `rgb(156 196 100)`, Forest `rgb(63 138 79)`, Road `rgb(226 104 60)`, River `rgb(74 163 223)`, Beach `rgb(227 194 122)`, Ocean `rgb(45 111 176)`. |
| Iconography | The shared authored 24 px SVG set at 1.8 px stroke. Icons carry text labels in the bar except Redo, Photo and Settings, which carry aria labels now and get tooltips in the full pass. |
| Motion | `cubic-bezier(.22,.9,.3,1)`. Hover 120 ms. Select 120 ms (row fills, magenta bar). Rotate nudges the icon 180 ms and turns glyphs 220 ms. Sheet slides off the right edge in 240 ms. Undo note rises 8 px and fades in over 180 ms, dismisses at 4 s. Reduced motion makes all of it instant. |

## Owner decisions (2026-10-13)

- **Chosen direction:** Legend. Vitrine and Departures are retired; their comps stay as the record of the choice.
- **Inventory:** the sheet is open by default and stays open while the player hovers slots. Only Tab (or its close control) closes it, and Tab reopens it.
- **Tiles that do not fit:** stay in the list, dimmed with their reason, ordered below every tile that fits ("Fits here" first, then "Doesn't fit here").
- **PRODUCT.md:** approved as written and moved to the repo root.

### Full-pass answers (owner, 2026-10-13: "Yes to all")

- **Q1.** Rebase the mockups onto the real play captures (`docs/checkpoints/play/`) in place of the flat placed-tile stand-in and the masked one-tile world, and put the app icon on the loading and unsupported screens.
- **Q2.** The Legend unsupported page (S11) replaces `web/unsupported.html` when the overlay is built.
- **Q3.** The bar title shows the world name, tile count and autosave state ("Riverside · 14 tiles · Saved").
- **Q4.** New keys: H hides the UI, F6 moves focus between panels (Tab belongs to the sheet), and in photo mode WASD flies the camera and Q/E move it down and up (PRD §4.5).
- **Q5.** While the pointer is over the HUD, the sheet keeps the last hovered slot's filter.
- **Q6.** Rotation counts are distinct orientations. A tile with a single fit shows "Fits" with no R key.
- **Q7.** `DESIGN.md` and its sidecar move to the repo root when the overlay is implemented. The documenter's drift list is tokenised then: the dead side-stripe rule, focus-ring colour, motion durations off the 120/180/240 ms scale, the 4 px radius and the ghost fill colour.
- **Q8.** Shots ship as JPEG (quality 88, 4:4:4).
- **Q9.** Caution colour `rgb(244 196 92)` is a token, for non-blocking warnings (autosave off, a quality tier above the detected one).

- **Walk mode (PRD P8, 2026-10-14).** The bar gains an icon-only Walk button between Tiles and Photo (the walker glyph from the shared 24 px set at 1.8 px stroke, tooltip "Walk around F"). While walking the HUD steps aside as in photo mode, and one margin note at the bottom centre replaces it: the walker icon, "Click to look around" (once the pointer is locked, "Mouse looks around"), the WASD, Shift and Space keys on a quiet line, and a Leave button with Esc. A 4 px ink dot marks the centre of the view. It uses the toast's sheet, radius, shadow and 180 ms fade, so it adds no new visual language. Built straight from the owner's request without a mockup round; the stills are in `docs/checkpoints/first-person/` and `e2e/test-results/ui/walk-1-first-person.jpg`.
- **Mockups approved (owner, 2026-10-13):** "ui approved". The full pass in `review.html` (on real captures, f07069b) is the build target for the overlay.
