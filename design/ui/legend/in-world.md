# Legend: in-world affordances

What the engine draws in the 3D scene to match the Legend HUD: the free-slot ring, the ghost, the edge glow, the socket key marks, and the bulldoze outlines. The HUD side lives in the prototypes beside this file; `play.html` and `first-run.html` draw these affordances as an SVG stand-in over in-engine captures, so their screenshots are the visual reference.

Colours are sRGB, 8-bit. The render target is HDR (PRD §9.1), so every glowing line is drawn emissive with the multiplier given, after tone mapping is accounted for, and bloom carries the halo. Widths are screen-space pixels at a device pixel ratio of 1, constant at every zoom, so the affordance never becomes a hairline when zoomed out or a stripe when zoomed in. For reference, at the default strategic zoom a 16 m hex edge (R = 16 m, §5.1) spans about 150 px, so 1 px is about 0.11 m.

Motion uses `cubic-bezier(.22, .9, .3, 1)` unless stated. Under `prefers-reduced-motion` (or the in-game override in Settings, Controls), every transition below is an instant cut, the breathing stops, and the state is still carried by colour, dash pattern and the HUD text.

## Colours

| Token | sRGB | Use in the world |
|---|---|---|
| Compatible | `rgb(123 224 143)` | Touching edge that matches its neighbour. Emissive ×2.5. |
| Incompatible | `rgb(255 93 82)` | Touching edge that clashes; bulldoze target. Emissive ×2.5. |
| Neutral | `rgb(214 228 240)` at 55% | Edge with no neighbour; free-slot outline. Not emissive. |
| Ink | `rgb(238 243 246)` | Hovered slot outline, hovered tile rim, commit flash, cut-off hatch. |
| Ghost line | `rgb(214 236 255)` at 75% | Ghost outline. |
| Ghost fill | `rgb(74 163 223)` at 12% | Ghost cap tint. |
| Socket: Grass | `rgb(156 196 100)` | Socket key mark. |
| Socket: Forest | `rgb(63 138 79)` | Socket key mark. |
| Socket: Road | `rgb(226 104 60)` | Socket key mark; ghost road lane. |
| Socket: River | `rgb(74 163 223)` | Socket key mark; ghost river lane. |
| Socket: Beach | `rgb(227 194 122)` | Socket key mark. |
| Socket: Ocean | `rgb(45 111 176)` | Socket key mark. |

Legality is never colour alone. Compatible edges are solid, incompatible edges are dashed and carry a cross tick, and neutral edges are thin. The HUD tag beside the ghost repeats the state in words.

## Free-slot ring

- Every free slot adjacent to the world draws its hex outline at ground height: neutral, 2 px, dashed 6 on and 7 off.
- Hovered slot: outline goes to ink at 90% and the cap gets an ink fill at 4%. Crossfade 120 ms.
- The slot ring hides in bulldoze and photo mode (160 ms fade).
- The hovered slot stays the sheet's slot while the pointer moves onto the HUD. Another slot, Esc, or a placement replaces it.
- Pinning (owner choice U4, 2026-10-14): a click on a free slot pins it. The ghost, its rotation and the sheet stay on the pinned slot while the pointer crosses other tiles on its way to the sheet, and a second click on it places. A click on another free slot pins that one; Esc or a click elsewhere unpins. The pinned ghost's outline is solid, and the slot under the pointer keeps the hovered outline so the next click's target is visible. Shift-click skips the pin and places at once (see Place).

## Hovered tile

Owner request, 2026-10-14: a placed tile under the pointer gets a faint outline, so the player sees which tile the pointer is on.

- With the place tool, the placed tile under the pointer draws a thin hex rim at the cap rim (the bulldoze target's rim): ink at 35%, 1.5 px, solid, with no fill, glow or breathing. It is clearly fainter than the hovered slot's 90%, 2 px dashed outline.
- Fades in 120 ms and out 120 ms. Under reduced motion it cuts on and off.
- Hidden in bulldoze mode, where the bulldoze target outlines the hovered tile instead, in photo mode, and while the pointer is on the HUD (the engine then hovers nothing, or the sheet's slot).
- A tile still waiting for its pack shows only the waiting outline.
- With a pinned slot (U4), the pinned ghost stays and the rim follows the pointer over placed tiles.

## Ghost

- The tile's real meshes at 45% opacity, desaturated 30% and tinted 15% toward ink, sitting at cap height. The meshes include the tile's seeded scatter (spruces, rocks, wildflowers) exactly as it lands at that cell and rotation. The engine draws them untextured, one tint per part (base by biome, lanes by socket kind, trees, buildings, ground props such as rocks and wildflowers), since the kit pipeline has no sampled texture to spare beside refracting water. On coast, ocean and river tiles each ground part takes the tint of its material slot (grass green, dry and damp sand, seabed or river bed blue-teal, stones), and a flat translucent water surface at the water level covers where the water will be: the seaward side of a coast, the whole ocean, a river's channel. While the tile's pack is still streaming, and in the prototype, coloured lane strokes stand in for the meshes (river 22 px, road 18 px, both at 60%).
- The tile's own socket colours (owner choice U1): a 5 px line in the tile's socket colour on each edge at the current rotation, along the middle 72% of the edge, 11 px inside the rim.
- Outline: ghost line colour, 2 px, dashed 10 on and 8 off, at the cap rim.
- Breathing: cap fill opacity 100% to 55% and back over 2.4 s, ease-in-out, looping. Off under reduced motion.
- Appears 150 ms after a slot is hovered with a tile selected, at the first legal rotation (auto-rotate). Disappears in 100 ms when the pointer leaves for a non-slot.

## Edge glow

Drawn on the cap rim of the ghost, one segment per edge.

| Edge state | Core | Halo | Pattern |
|---|---|---|---|
| Compatible | 7 px, compatible | 6 px at 90% plus 14 px at 60% (bloom) | Solid |
| Incompatible | 7 px, incompatible | 6 px at 90% plus 14 px at 50% | Dashed 14 on, 9 off, plus a cross tick at the midpoint: two 14 px strokes, 3.5 px wide |
| No neighbour | 3 px, neutral at 70% | None | Solid |

State changes crossfade in 120 ms.

## Socket key marks

The signature of the Legend direction, carried into the world. For every touched edge of the hovered slot, a 4 px line in the neighbour's socket colour runs along the middle 40% of the edge, set 11 px outside the slot on the neighbour's side, under the edge glow. The tile's own colour sits 11 px inside, so each touched edge reads neighbour, glow (fit or clash), tile. It is the in-world twin of the slot key in the sheet, so the player can match "River on one edge" to the blue mark at the river side without reading.

## Rotate

- R, Shift+R, or Ctrl+wheel turns the ghost 60° about its centre in 200 ms (range 180–220 ms).
- The edge glow fades out over the first 60 ms of the turn, recomputes, and fades in over 120 ms when the turn lands.
- With auto-rotate on (default), R steps through legal rotations only and wraps. With it off, R steps all six, and clashing edges show as incompatible.
- Holding R repeats every 250 ms. A press during a turn retargets the running turn instead of queueing.
- Sheet glyphs of fitting tiles turn in step (220 ms).

## Place

| Time | World | HUD |
|---|---|---|
| 0 ms | Edges flash ink, emissive ×4, for 80 ms. The ghost is removed and the staged rise starts in the same frame (§4.4, 1.5–2.5 s, per tile data). | Place tool fires its 200 ms ring. |
| ≤ 200 ms | Rise in progress. | Tag beside the slot reads "Placed {tile}" with a check, then fades at 1.1 s. Autosave reads "Saving…". |
| Next placement | Any running rise jumps to its end state. | Nothing extra. |

Rejected placement (the selected tile does not fit): the incompatible edges pulse once over 240 ms, core 7 to 13 px and halo 10 and 24 px, then settle. No camera shake. The tag keeps its reason.

Shift-click (owner feedback, 2026-10-14) places the selected tile on the slot under the pointer in one click, at the ghost's rotation there, with no pin step, so a held Shift lays a run of the same tile. The ghost keeps following the pointer, a pin on another slot stays, and a slot where nothing fits refuses as above.

Undo of a placement (and redo of a bulldoze): the tile reverses its rise, sinking 400 ms ease-in with a short dust puff around the rim. The prototype fades it in 260 ms. A bulldoze gets the dust cloud instead (see Bulldoze).

## Replace

Ctrl-click, or Cmd-click on macOS (owner feedback, 2026-10-14), on a placed tile swaps it for the selected tile at the ghost's rotation when that fits the neighbours, else the first rotation that fits. It is one undoable step.

| Time | World | HUD |
|---|---|---|
| 0 ms | The old tile sinks 400 ms, ease-in, with no dust puff and no bulldoze cloud. | The tag beside the cell reads "Replaced {old} with {new}" with a check, then fades at 1.1 s. The placement cues play. |
| 400 ms | The new tile's staged rise starts (§4.4), with its usual dust. | Autosave reads "Saving…". |

When nothing fits, the tile stays. The tag carries the bad ring and the reason, and `ui-invalid` plays. The same tile in the same orientation changes nothing. Undo and redo of a replace swap the tiles back the same way. The canvas never opens the browser's context menu, so Ctrl-click works on macOS. The Tiles sheet's footer names both clicks: "Shift-click place · ⌘-click replace".

## Bulldoze

| Hover target | Outline | Fill | Extra |
|---|---|---|---|
| Removable | 4 px solid incompatible, halo 8 px at 60% | Incompatible at 12% | Tag "Remove {tile}" |
| Would split the world | 4 px incompatible, dashed 10 on and 8 off | None | The tiles it would cut off get a hatch: 45° ink stripes 2.5 px wide every 10 px over sheet blue `rgb(18 39 58)` at 35%, with a 2 px ink dotted outline (3 on, 5 off). Tag "Removing this would split your world" and the count. |
| Last tile | Same as split, without the hatch | None | Tag "The world needs at least one tile" |

- Mode change in and out: 150 ms. The cursor becomes the bulldoze cursor and the ghost and slot ring hide.
- Clicking a blocked target repeats the 240 ms pulse on its outline. Clicking a removable tile destroys it: it sinks 700 ms ease-in inside a dust cloud and the HUD toast offers Undo.
- The dust cloud (owner request 2026-10-14) covers the whole hex: a darker earthy swell across the hex area, a lighter billow thrown past the rim and up from the centre several metres, and a few dark debris bits tossed up that fall back. It lingers about 1.5 s after the tile is gone, the upper billow thinning first, and is clear by 2.24 s: the owner asked for 20% less size and duration than the first look (2026-10-14), so every distance, particle size and phase is 80% of it. 120 particles at most on two shared meshes, unlit, no shadows, the same layout every time for a given cell. Reduced motion: no cloud, the tile is gone at once. Undo and redo keep the short puff above.
- Highlights fade in 160 ms.

## Audio pairing (§8.1)

Select, rotate, invalid placement, undo and bulldoze each play their UI cue at −20 LUFS on the same frame as the visual change above, never after it.
