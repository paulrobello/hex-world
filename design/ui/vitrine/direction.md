# Vitrine

The HUD is the case around a natural-history habitat diorama: a dark gallery wall, a brass-trimmed caption rail under the glass, and captions that are lit when the tile fits and left in the dark when it does not.

## Direction contract

- **THESIS:** The diorama is the exhibit and the UI is its case. It refuses the category default of floating cozy-game cards on a wood or parchment ground.
- **OWN-WORLD:** Bottle-green-black wall panels, one brass trim and action colour, warm lamp light on captions that fit, unlit captions for tiles that do not. Captions in tracked capitals over plain sentence-case notes.
- **STORY:** The player hovers a slot. The slot gets a pinned caption ("Touches River and Grass"), the tiles that fit light up on the caption rail, and the rest stay dark with a one-line reason.
- **FIRST VIEWPORT:** A vertical tool rail on the left. A full-width caption rail along the bottom holds the selected tile (large glyph, name, rotation n of N), then "Fits here" (lit), then "Not here" (unlit, scrolling). The diorama fills the upper two thirds. The primary action is a click on the ghost, and Place is the active tool.
- **FORM:** Museum habitat-diorama hall. Position 6 of 7 on the grounded list. Seed key `dc8cbb29`.
- **SIGNATURE MOVE:** Lamp-lit legality. A caption that fits carries a warm light from above, brightening on hover and fully lit when selected. A caption that does not fit is unlit and desaturated. The filter is the lighting.
- **FINISH:** unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance.

Raises from declined challengers:

- **Raise from the variable-font specimen:** one control drives the whole grid. R rotates the ghost and every lit caption's glyph in step.
- **Raise from the character catalog:** every tile owns the same fixed furniture, a name, a fit line and a glyph, in the same place on every caption.
- **Raise from the warm consumer app:** brass marks only something you can press or something that is selected, never decoration.

**Honest risk:** The museum world is the most atmospheric of the three. At small sizes the brass on dark green can read as "luxury app" rather than "museum", and a bottom rail takes about a quarter of the screen height when it is open.

## Tokens

| Group | Values |
|---|---|
| Type | Source Sans 3 (OFL), one family. 12 / 13 / 15 / 18 / 24 px (`--fs-1..5`). Captions 12–13 px semibold, tracked +0.12–0.16em, uppercase. Names 13–18 px semibold. Notes 12–13 px regular. |
| Spacing | 4 / 8 / 12 / 16 / 24 / 32 px. Radius 3 px. |
| Colour | Wall `rgb(17 27 23)`, wall-2 `rgb(24 37 31)`, brass `rgb(201 163 98)`, ink `rgb(240 234 222)`, ink-2 `rgb(184 196 186)`, ink-3 `rgb(140 155 145)`, lamp `rgb(255 214 150)` at 16–32 % alpha. States: compatible `rgb(112 214 162)`, incompatible `rgb(236 106 79)`, neutral edge `rgb(220 226 220 / .55)`. |
| Socket colours | Grass `rgb(122 170 84)`, Forest `rgb(52 110 66)`, Road `rgb(206 198 182)`, River `rgb(82 168 218)`, Beach `rgb(220 192 128)`, Ocean `rgb(44 106 156)`. |
| Iconography | One authored 24 px SVG set, 1.7 px round stroke, built on the hex silhouette (place, bulldoze, inventory share the hex). No icon font, no emoji. |
| Motion | `cubic-bezier(.2,.8,.2,1)`. Hover 140 ms (lamp brightens, glyph lifts 2 px). Select 200 ms (lamp to full, brass ring). Panel 260 ms slide under the floor line. Undo toast 200 ms drop from the top edge, auto-dismiss at 4 s. Tool press fires a 200 ms brass ring. Ghost outline breathes over 2.4 s. Under `prefers-reduced-motion` every transition is instant and the ghost stops breathing. Input is never blocked by motion. |
