# HUD state-change feedback inventory

UI card criterion 3: every state change has short eased feedback that never delays input, and reduced motion is respected.

The spec is `DESIGN.md` (Motion) and the tokens in `.impeccable/design.json`: one curve, `--ease` = `cubic-bezier(.22, .9, .3, 1)`, and the durations `--t-fast` 120 ms, `--t` 180 ms, `--t-slow` 240 ms and `--t-toast` 180 ms. The mockups (`design/ui/legend/screens.css`, `play.js`, `saves.html`, `photo.html`) use some values off that scale (130, 150, 160, 200, 220, 260 and 300 ms). The owner's Q7 answer (direction.md, 2026-10-13) asked for those to be tokenised, so a shipped value on the token scale counts as matching. "Changed" marks the five rows this pass touched.

Audit date 2026-10-20, on c8d311a.

| # | State change | Before | After | Mockup reference |
|---|---|---|---|---|
| 1 | Place tool (click, or a placement) | Fills `pick` (`--t-fast`). The icon nudges -14° and rings 6 px (`--t`) | Unchanged | screens.css `.tool.is-fired` 200 ms. DESIGN.md Tool: 180 ms |
| 2 | Bulldoze on and off (B, Esc) | Fills `bad` (`--t-fast`). The tool nudges and rings (`--t`). The sheet body dims to 40% (`--t`) and the slot key becomes the bulldoze key | Unchanged | in-world.md Bulldoze: mode change 150 ms |
| 3 | Walk in and out (F, Esc, Leave) | The bar and sheet slide off (`--t-slow`). The walk hint fades and rises 8 px over `--t-slow` (240 ms) | **Changed**: the hint now fades over `--t-toast` (180 ms) | direction.md Walk mode: "the toast's … 180 ms fade" |
| 4 | Photo mode in and out (P, Esc) | The HUD steps aside (`--t-slow`). The photo bar drops in and the photo sheet slides in (`--t-slow`). On leaving, the photo UI cuts away and the HUD slides back (`--t-slow`) | Unchanged | photo.html `pbar` and `slide-in`, 200 ms |
| 5 | Tile selection (row click, 1 to 9) | The row fills `sheet-2` and gains a 1.5 px `pick` outline (`--t-fast`) | Unchanged | direction.md: select 120 ms |
| 6 | Rotation (R, Shift+R, Ctrl+wheel) | The Rotate tool nudges and rings (`--t`). Each fitting row's socket code turns 60° (`--t`). Each row's baked thumbnail swaps to that rotation, with no transition (they are per-rotation renders). The tag readout updates. The engine turns the ghost in 200 ms | Unchanged | direction.md: icon 180 ms, glyphs 220 ms. in-world.md Rotate |
| 7 | Sheet re-rank (pointer on a new slot) | Rows slide to their new places with a Web Animation over a literal 260 ms | **Changed**: the duration is read from `--t-slow` (240 ms), so the token is the only source | play.js re-rank: 260 ms |
| 8 | Slot key change | The key background crossfades (`--t-fast`). The content swaps | Unchanged | screens.css `.key`: 160 ms |
| 9 | Tiles sheet open and close (Tab, close control) | Slides off the right edge with a fade (`--t-slow`) | Unchanged | direction.md: 240 ms |
| 10 | Hide and show UI (H) | The bar and sheet slide off their edges. The Show UI chip rises and fades in (`--t-slow`) | Unchanged | screens.css `.bar` and `.show-ui`: 200 ms |
| 11 | Undo and redo (⌘Z, ⇧⌘Z, the buttons, the toast action) | The tool nudges and rings (`--t`). The margin note rises 8 px and fades in (`--t-toast`) and dismisses at 4 s. A repeat updates its count in place. The disabled state fades (`--t-fast`) | Unchanged | direction.md: the undo note rises 8 px and fades in over 180 ms, dismisses at 4 s |
| 12 | Placement | The tool rings. The tag reads "Placed {tile}" with a check (opacity `--t-fast`) and fades at 1.1 s. The autosave words crossfade (`--t-fast`) | Unchanged | in-world.md Place table |
| 13 | Refused placement or bulldoze | The tag pulses its bad ring once (`--t-slow`) and states the reason. A refused bulldoze also rings the tool | Unchanged | in-world.md: 240 ms pulse |
| 14 | Bulldoze removes a tile | "Removed {tile}" note with Undo (`--t-toast`) | Unchanged | play.js bulldoze toast |
| 15 | Settings open, close and tab switch | The sheet slides (`--t-slow`). A tab's panel rises 4 px and fades in (`--t-fast`) | Unchanged | screens.css `.sheet .panel`: 150 ms |
| 16 | Settings controls (switch, segmented, quality tier, grade select) | Knob slide, fills and radio ring (`--t-fast`). The select list drops 4 px and its chevron turns (`--t-fast`) | Unchanged | screens.css `.switch` and `.seg` 150 and 120 ms; settings.html `.tier` |
| 17 | Worlds sheet: open, rows, confirms, menu, drop target, import panels | The sheet slides (`--t-slow`). Rows, the delete and load confirms, and the import preview and refusal checks panels rise and fade in (`--t`, `panel-in`), and cut in at once under reduced motion. The menu does the same over `--t-fast`. The drop target fades (`--t-fast`) | Unchanged (the load confirm and import panels were added 2026-10-21 on the same tokens) | saves.html: 150, 200 and 120 ms |
| 18 | Save state (Saved, Saving…, Autosave off) | The words crossfade (`--t-fast`). The Saving dot blinks on a 900 ms loop while a write is pending | Unchanged | screens.css `.save-state`: 150 ms |
| 19 | Notes and errors (storage off, a load, an import or replace; a refused import is the sheet's checks panel, row 17) | Margin note (`--t-toast`), with its tone carried by the icon colour | Unchanged | screens.css `.toast` (`--t-toast`) |
| 20 | New-version note | The same toast (`--t-toast`), staying until Reload | Unchanged | (toast) |
| 21 | Loading progress and ready | Hex edges draw and the bar grows (`--t-slow`, linear). Stage rows change colour (`--t-fast`). On ready, the hex fills with Grass and the panel fades (`--t-slow`) | Unchanged | loading.html: 240 and 300 ms |
| 22 | Loading failure | The failure panel and Retry appeared with no transition | **Changed**: it rises 4 px and fades in (`--t-fast`) | loading.html `.fail`: panel-in 150 ms |
| 23 | Quality rebuild | The overlay appears with no fade, on purpose: the frame before the rebuild may stay on screen for seconds | Unchanged (by design) | hud.css comment |
| 24 | Tooltips | Appear after a 350 ms hover delay, or at once on keyboard focus. They slide 4 px and fade (`--t-fast`) | Unchanged | screens.css `.tip`: 130 ms |
| 25 | First-run callouts | Appear after 1.5 s of calm, then fade and rise (`--t-slow`). Under reduced motion they appear at once | Unchanged | screens.css `.callout`: 240 ms |
| 26 | Photo focus click | The ring settles on the point and fades over 1.2 s. Under reduced motion the ring never showed: it is visible only through its animation, and the blanket `animation: none !important` beat the opacity-only override | **Changed** (bug fix): under reduced motion it is a still ring that holds, then fades by opacity | DESIGN.md: under reduced motion, state still shows |
| 27 | Reduced motion, as a whole | Only the OS `prefers-reduced-motion` reached the HUD's CSS and its JS guards. The engine flag (`set_reduced_motion`, scripted or from a future Settings override) cut the engine's motion but not the HUD's | **Changed**: `dom.ts syncReducedMotion` mirrors `play.reduced_motion` as `is-reduced-motion` on the root every frame, and `reduceMotion()` reads either source. The CSS cut is repeated under `:root.is-reduced-motion`, and it now covers the `.hud` roots themselves (walk, photo) as well as their contents | DESIGN.md Motion; in-world.md (instant cuts) |

## Summary

The audit covered 27 state changes. Before this pass, 2 had no feedback in some condition: the loading failure panel (row 22), and the photo focus ring under reduced motion (row 26). 2 more did not match the written spec: the walk hint fade (row 3) and the re-rank's literal duration (row 7). Reduced motion followed only the OS setting (row 27). The other 22 already matched DESIGN.md and the token scale.

## Input never waits

Every handler applies its command in the same task as the key or click. No path awaits an animation: `retrigger` restarts a class, transitions run on the compositor, and the re-rank's Web Animation is fire-and-forget. One throttle is specified behaviour, not a delay: a held R (`e.repeat`) repeats every 250 ms (in-world.md Rotate), and separate presses are never throttled. `e2e/ui-motion.mjs` checks this under full motion, OS reduced motion and the engine flag:

- Five R presses in one task send five `rotate` commands at once and land as five `ghostRotated` events.
- Tab, Tab during the 240 ms slide flips the sheet on each press.
- B, B sends two toggles at once.

## Evidence (frame strips, GPU Chromium, animations paused and stepped through 0, 10, 25, 50 and 100% of their duration)

- `sheet-close.jpg`, `sheet-open.jpg`: the Tiles sheet slide (row 9).
- `rotate-tool.jpg`: the Rotate tool's nudge and ring (row 6).
- `rotate-row.jpg`: the selected row's socket code turning (row 6).
- `undo-note.jpg`: the undo note rising and fading in (row 11).
- `bulldoze-on.jpg`: the bulldoze fill, nudge and ring (row 2).
- `walk-hint.jpg`: the walk hint fading in over the toast's 180 ms (row 3, changed).
- `settings-tab.jpg`: a Settings tab's panel rising in (row 15).
- `sheet-close-reduced.jpg`: the same Tab under reduced motion, an instant cut with no animations (row 27).

Regenerate with `MOTION_OUT=docs/checkpoints/ui-feedback node e2e/ui-motion.mjs dist` after `make build-web`.
