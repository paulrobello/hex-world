---
name: Hex World, Legend
description: The player HUD as the margin of a survey map sheet, where every tile is keyed by its six socket colours.
colors:
  sheet: "rgb(18 39 58)"
  sheet-2: "rgb(25 52 76)"
  sheet-3: "rgb(34 68 98)"
  void: "rgb(14 15 16)"
  ink: "rgb(238 243 246)"
  ink-2: "rgb(176 197 214)"
  ink-3: "rgb(132 156 176)"
  ink-3-lifted: "rgb(146 170 190)"
  ink-on-ink: "rgb(18 39 58)"
  rule: "rgb(176 197 214 / .2)"
  track: "rgb(255 255 255 / .1)"
  pick: "rgb(198 46 104)"
  ok: "rgb(123 224 143)"
  bad: "rgb(255 93 82)"
  caution: "rgb(244 196 92)"
  neutral: "rgb(214 228 240 / .55)"
  ghost-fill: "rgb(74 163 223 / .12)"
  ghost-line: "rgb(214 236 255 / .75)"
  socket-grass: "rgb(156 196 100)"
  socket-forest: "rgb(63 138 79)"
  socket-road: "rgb(226 104 60)"
  socket-river: "rgb(74 163 223)"
  socket-beach: "rgb(227 194 122)"
  socket-ocean: "rgb(45 111 176)"
  lod-0: "rgb(253 231 37)"
  lod-1: "rgb(33 145 140)"
  lod-2: "rgb(68 1 84)"
typography:
  headline:
    fontFamily: "Manrope, system-ui, sans-serif"
    fontSize: "1.25rem"
    fontWeight: 800
    letterSpacing: "-.015em"
    fontFeature: "tnum"
  title:
    fontFamily: "Manrope, system-ui, sans-serif"
    fontSize: "1rem"
    fontWeight: 800
    lineHeight: 1.1
    letterSpacing: "-.01em"
    fontFeature: "tnum"
  name:
    fontFamily: "Manrope, system-ui, sans-serif"
    fontSize: "1rem"
    fontWeight: 700
    fontFeature: "tnum"
  body:
    fontFamily: "Manrope, system-ui, sans-serif"
    fontSize: ".875rem"
    fontWeight: 600
    lineHeight: 1.4
    fontFeature: "tnum"
  meta:
    fontFamily: "Manrope, system-ui, sans-serif"
    fontSize: ".8125rem"
    fontWeight: 600
    fontFeature: "tnum"
  label:
    fontFamily: "Manrope, system-ui, sans-serif"
    fontSize: ".75rem"
    fontWeight: 700
    lineHeight: 1
    fontFeature: "tnum"
rounded:
  code: "1px"
  swatch: "2px"
  control: "4px"
  panel: "6px"
  round: "999px"
spacing:
  s-1: "4px"
  s-2: "8px"
  s-3: "12px"
  s-4: "16px"
  s-5: "20px"
  s-6: "28px"
components:
  bar:
    backgroundColor: "{colors.sheet}"
    rounded: "{rounded.panel}"
    padding: "6px"
  tool:
    textColor: "{colors.ink-2}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "0 10px"
    height: "44px"
  tool-hover:
    backgroundColor: "{colors.sheet-2}"
    textColor: "{colors.ink}"
  tool-active:
    backgroundColor: "{colors.pick}"
    textColor: "{colors.ink}"
  tool-bulldoze-active:
    backgroundColor: "{colors.bad}"
    textColor: "{colors.ink}"
  sheet:
    backgroundColor: "{colors.sheet}"
    rounded: "{rounded.panel}"
    padding: "16px 16px 12px"
    width: "372px"
  sheet-wide:
    width: "420px"
  key:
    backgroundColor: "{colors.sheet-2}"
    rounded: "{rounded.control}"
    padding: "12px"
  row:
    textColor: "{colors.ink}"
    typography: "{typography.name}"
    rounded: "{rounded.control}"
    padding: "6px 10px 6px 6px"
  row-hover:
    backgroundColor: "{colors.sheet-2}"
  row-off:
    textColor: "{colors.ink-2}"
    padding: "3px 10px 3px 6px"
  socket-hex:
    width: "32px"
    height: "32px"
  btn:
    backgroundColor: "{colors.sheet-2}"
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "0 16px"
    height: "44px"
  btn-hover:
    backgroundColor: "{colors.sheet-3}"
  btn-ink:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.ink-on-ink}"
  btn-bad:
    backgroundColor: "{colors.bad}"
  btn-quiet:
    textColor: "{colors.ink-2}"
  btn-sm:
    padding: "0 12px"
    height: "36px"
  toast:
    backgroundColor: "{colors.sheet}"
    rounded: "{rounded.panel}"
    padding: "8px 8px 8px 14px"
  toast-action:
    backgroundColor: "{colors.sheet-2}"
    textColor: "{colors.ink}"
    rounded: "{rounded.control}"
    padding: "0 14px"
    height: "36px"
  ghost-tag:
    backgroundColor: "{colors.sheet}"
    typography: "{typography.body}"
    padding: "6px 6px 6px 12px"
  callout:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.ink-on-ink}"
    rounded: "{rounded.control}"
    padding: "8px 12px"
  tip:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.ink-on-ink}"
    rounded: "{rounded.control}"
    padding: "6px 8px 6px 10px"
  note:
    backgroundColor: "{colors.sheet-2}"
    textColor: "{colors.ink-2}"
    rounded: "{rounded.control}"
    padding: "10px 12px"
  switch:
    backgroundColor: "{colors.track}"
    rounded: "13px"
    width: "44px"
    height: "26px"
  switch-on:
    backgroundColor: "{colors.ink}"
  seg:
    rounded: "{rounded.panel}"
    padding: "2px"
  seg-option-checked:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.ink-on-ink}"
    rounded: "{rounded.control}"
    height: "40px"
  slider-track:
    backgroundColor: "{colors.track}"
    height: "4px"
  slider-thumb:
    backgroundColor: "{colors.ink}"
    rounded: "{rounded.round}"
    size: "18px"
  kbd:
    textColor: "{colors.ink-2}"
    typography: "{typography.label}"
    rounded: "{rounded.control}"
    padding: "3px 5px"
---

# Design System: Hex World, Legend

## Overview

**Creative North Star: "The Survey Sheet Margin"**

The HUD is the margin of a survey map sheet laid beside the diorama. Socket kinds are map classes (grass, forest, road, river, beach, ocean), so every tile is keyed by its six edge colours the way a map legend keys its symbols, and the slot the player points at prints its own key: what it needs. The diorama is the hero. The margin frames it in deep survey-blue panels and never competes with it.

Density is that of a legend: small, exact, tabular. One grotesk family (Manrope) carries every role, with tabular figures throughout. Colour splits into three strict jobs. Survey-blue and ink are the paper. One magenta marks the current tool and the current selection. The six socket colours and the ok, bad and caution states appear only as data. Panels sit in the margins (tool bar top left, sheet full height on the right, notes bottom left) so the centre stays clear for the ghost.

The system was written after the build, from `design/ui/legend/style.css` (the owner-approved comp) and `design/ui/legend/screens.css` (the full pass), and moved to the repo root when the overlay shipped (owner Q7). Its sidecar is `.impeccable/design.json`, from which `web/ui/build.mjs` emits the colour, shadow, motion and focus tokens in front of `web/ui/src/hud.css`. The Tiles sheet is open by default, stays open while the player hovers slots, and closes only by Tab or its close control. Tiles that do not fit stay in the list, dimmed with their reason, below every tile that fits. These are owner decisions and bind every later surface.

**Key Characteristics:**
- Deep survey-blue panels on a near-black void, with no glass, no blur and no gradients on chrome.
- Six socket colours as the only chromatic vocabulary, always shown as data: edges, codes, keys, mix strips.
- One magenta for the current tool and the current selection.
- Legality is never colour alone. It is always colour plus pattern (solid, dashed, tick) plus words.
- One family, tabular figures, weights from 600 to 800, no uppercase labels.
- Every control is at least 44 px tall, ready for touch.

## Colors

A dark survey-blue paper with ink in three strengths, one magenta for selection, and the map's six class colours reserved for data.

### Primary
- **Survey Magenta** (`pick`): the active tool fill, the selected row's 1.5 px outline and its filled number key, and text selection. Nothing else on screen is magenta.

### Secondary
- **Fit Green** (`ok`): compatible edges, the green-glow edge state, the "Fits here" count, the saved dot, and ok toasts and notes.
- **Clash Red** (`bad`): incompatible edges, the bulldoze tool when active, removal outlines, refusal reasons, the "Nothing fits" key outline, and destructive buttons.
- **Survey Amber** (`caution`): "Not saved" and caution notes only.

### Tertiary: the six socket colours
- **Grass** (`socket-grass`), **Forest** (`socket-forest`), **Road** (`socket-road`), **River** (`socket-river`), **Beach** (`socket-beach`) and **Ocean** (`socket-ocean`), plus **Rail** (`socket-rail`, P3, a violet kept clear of the pick magenta). In CSS these are `--k-G`, `--k-F`, `--k-R`, `--k-V`, `--k-B`, `--k-O` and `--k-T`. They drive the tile glyph edges and fill, the row socket hex, the slot key's edge marks, the Worlds mix strip, the loading hex key, the ghost's road and river lanes, and the in-world socket key marks.

### Debug data: the LOD levels
- **LOD 0**, **LOD 1** and **LOD 2** (`lod-0`, `lod-1`, `lod-2`; `--lod-0` to `--lod-2`): three viridis stops ordered by lightness, so the levels separate without hue. They are the engine's LOD tint and appear only in the Settings Debug tab's LOD key, each beside its level name, distance band and drawn count. Developer overlays only; never chrome.

### Neutral
- **Survey Blue** (`sheet`): every panel. That covers the bar, sheet, toast, ghost tag and stream chip.
- **Survey Blue, Raised** (`sheet-2`): rows on hover and selection, the key, notes, buttons, and hovered tools.
- **Survey Blue, High** (`sheet-3`): button hover.
- **Void** (`void`): the page behind the island, and the fill of a tile being removed.
- **Ink** (`ink`): primary text, the focus ring, slider fill, switch-on, the checked segment, and the tooltip and callout background.
- **Ink 2** (`ink-2`): secondary text, idle tool labels, and key list text.
- **Ink 3** (`ink-3`): tertiary text such as hints, the sheet foot and off-list meta. On `sheet-2` surfaces it lifts to **Ink 3, Lifted** (`ink-3-lifted`) so it keeps 4.5:1 contrast.
- **Ink on Ink** (`ink-on-ink`): text on ink-filled surfaces (tooltips, callouts, `btn--ink`, the checked segment).
- **Rule** (`rule`): bar dividers, field separators, and the tab baseline.
- **Track** (`track`): slider and progress tracks, and the off switch.
- **Neutral Edge** (`neutral`): free-slot rings, unneighboured edges, and the slot key's dashed body.
- **Ghost Fill and Ghost Line** (`ghost-fill`, `ghost-line`): the ghost cap tint and its dashed outline. `ghost-fill` is derived from `socket-river` at 12% (`color-mix(in srgb, var(--k-V) 12%, transparent)`), so it cannot drift from the River socket.

### Named Rules
**The Colour-as-Data Rule.** Socket colours appear only where they encode a socket: an edge, a code cell, a key mark, a mix-strip segment, a lane, or a tile fill. They never decorate chrome. The one standing exception is `ghost-fill`, which is the River hue at 12% (see Do's and Don'ts).

**The One Magenta Rule.** `pick` marks the current tool and the current selection only. Bulldoze takes `bad` instead of magenta, because the active tool there is destructive.

**The Never-Colour-Alone Rule.** Every state carries a second channel:

| State | Colour | Non-colour channel |
|---|---|---|
| ok | Fit Green | Solid 7 px edge with a glow, the word "Fits", a check icon |
| bad | Clash Red | Dashed 14/9 edge with a cross tick, an x in the key, a reason line, an alert icon |
| neutral | Neutral Edge | Thin 3 px edge, or a dashed 6/7 ring |
| caution | Survey Amber | An alert icon and the words "Not saved" |

## Typography

**Display Font:** none. The system has no display role.
**Body Font:** Manrope (variable, weights 200 to 800, OFL, self-hosted), falling back to system-ui and sans-serif.
**Label/Mono Font:** Manrope at 700. Key caps use the same family.

**Character:** A crisp, slightly technical grotesk, set small and heavy, like the type in a map legend. Tabular figures are on at the body level (`font-variant-numeric: tabular-nums`), so counts, sizes and percentages line up.

### Hierarchy
The scale has five steps: 12, 13, 14, 16 and 20 px (`--fs-1` to `--fs-5`).
- **Headline** (800, 1.25rem, letter-spacing -.015em): sheet titles (Tiles, Settings, Worlds, Photo).
- **Title** (800, 1rem, -.01em): the bar's world name, the import preview title, and group and section heads. Group and section heads use 800 at 13 px.
- **Name** (700, 1rem): tile row names.
- **Body** (500 to 600, 0.875rem, line-height 1.4): base text, tools, buttons and fields. Buttons and field labels use 700.
- **Meta** (600, 0.8125rem): row meta, key list text, hints and tooltips.
- **Label** (700, 0.75rem, line-height 1): key caps, the sheet foot, and the off-list meta.

### Named Rules
**The One Family Rule.** Manrope carries every role. Hierarchy comes from weight (600, 700 or 800) and the five-step scale, never from a second face, uppercase or tracking.

## Layout

The layout is a set of margins around a clear centre. The **tool bar** is fixed top left, 20 px in (`s-5`). The **sheet** is fixed right, full height, inset 20 px top, right and bottom, and 372 px wide (420 px for the Settings, Worlds and Photo sheets). **Toasts** sit bottom left, 20 px in. The **stream chip** sits above the toast at 84 px from the bottom. The **ghost tag** sits beside the ghost. The centre belongs to the ghost and the diorama.

Spacing uses a six-step scale: 4, 8, 12, 16, 20 and 28 px. Tight gaps of 2 and 3 px separate rows and key lines. The sheet's inner padding is 16 / 16 / 12 px. The key sits 16 px below the head, with 12 px of inner padding.

Every tool, button and row is at least 44 px tall, except the small button, which is 36 px. Touch arrives in P7 and keeps these targets.

**The Narrow-Desktop Rule (1400 px).** At 1400 px wide or less, key caps leave the bar and live only in tooltips, tools tighten to 9 px of side padding, the bar's rules to 4 px of margin and the world name to 128 px, the Tiles sheet narrows to 340 px, and the row socket hex narrows to 28 px. The bar and sheet never meet, and the ghost area stays clear (shot `s12-narrow-1280x720`).

**Performance corner** (developer overlay, Settings Debug tab): a `sheet` panel with `shadow-1` below the bar, top left at the bar's left edge, 12 px under it. It ignores the pointer and hides with H. The grid overlay's axial labels sit under every HUD panel.

**The F6 Escape Rule.** Tab belongs to the sheet: it opens and closes Tiles, except while focus is inside the Settings or Worlds sheet, where Tab walks that sheet's controls (a confirm's buttons, the import panels). So F6, and Shift+F6 to go back, moves focus between `[data-region]` landmarks (bar, sheet, toast), skipping closed ones. The sheet foot and the Settings key table both state it.

## Elevation & Depth

Panels are lifted off the diorama by soft, dark, ambient shadows. There is no glass, no blur and no border. Within a panel, depth is tonal: `sheet` is the paper, and `sheet-2` and `sheet-3` are raised surfaces. In the world layer, depth is emissive. Edge states glow by drop-shadow, which stands in for bloom.

### Shadow Vocabulary
- **Panel** (`box-shadow: 0 8px 24px rgb(0 0 0 / .45)`, `--shadow-1`): the bar, toast, tooltip, callout and stream chip.
- **Sheet** (`box-shadow: 0 16px 40px rgb(0 0 0 / .5)`, `--shadow-2`): the full-height sheet.
- **Tag** (`box-shadow: 0 6px 16px rgb(0 0 0 / .45)`): the ghost tag. A refusal adds a 1.5 px inset `bad` ring.
- **Edge glow** (`drop-shadow(0 0 6px <state> / .9) drop-shadow(0 0 14px <state> / .6)`): ok and bad edges and removal outlines.

### Named Rules
**The Paper-and-Ink Rule.** Raised means a lighter survey blue, never a border or a lighter shadow. Rings appear only as state: the selected row's outline, a refusal tag, or the "Nothing fits" key.

## Shapes

Corners are small and square-shouldered. Panels use 6 px (`--r`). Every control, row, key, key cap, note, tooltip and callout uses 4 px (`--r-control`). Swatches use 2 px (`--r-swatch`). Full rounds are reserved for the slider thumb, the save dot, the switch (13 px), the toast count, and the ghost tag's leading edge (`999px 4px 4px 999px` on `--r-control`, a tag pointing at its ghost). When the ghost tag stacks a headline over a reason, it squares off to 6 px.

The hexagon is the system's only silhouette. It appears in the slot key, tile glyphs, the loading key, the place and bulldoze icons, and the free-slot ring. Dashed strokes mean "open or unresolved": the slot ring (6/7), the slot key body (5/5), the ghost outline (10/8), refusal edges (14/9), and the blocked bulldoze outline (10/8).

Icons are a single authored set on a 24 px grid with a 1.8 px round-capped stroke, inlined as an SVG sprite and drawn at 20 to 22 px.

## Components

### Bar and Tool
- **Bar:** a `sheet` panel with 6 px padding and 2 px gaps. It holds the world link (name over tile count and save state), 1 px `rule` dividers, and the tools.
- **Tool:** 44 px tall, an icon plus a label plus a key cap, `ink-2` on transparent. Hover fills `sheet-2`. Active fills `pick` with white text. Bulldoze active fills `bad`. Firing a tool nudges its icon (rotate -14°, 180 ms) and rings it (6 px ink ring fading over 180 ms). Disabled drops to `ink-3` at 55%.

### Sheet
- A full-height right-margin panel: a headline plus a close chevron, an optional sub line, a scrolling body, and a foot of key hints at 12 px.
- It slides off its own edge (`translateX(100% + 24px)`, 240 ms) when closed. The Tiles sheet's "Doesn't fit here" header sticks to the sheet foot so the dimmed group never scrolls out of sight.
- Bulldoze makes the sheet body inert (40% opacity, no pointer events).

### Key (signature)
- **Slot key:** a `sheet-2` card, a 64 px hex with a 2.5 px dashed `neutral` body, and a 9 px round-capped edge in the socket colour of each touched edge. Beside it sit a title and a list. Each line leads with a 14 × 4 px socket swatch: "River on one edge", "Grass or forest beside it".
- **Nothing fits:** the same key with a 5 px `bad` cross in the hex, a 1.5 px inset `bad` ring, the reason, and a "Show the nearest slot" button.
- **Bulldoze key:** the hex filled with `bad` at 15% and crossed, with three swatches: remove (filled and outlined), would split (dashed), and cut off (ink hatch).

### Row and Socket Code (signature)
- **Row:** a 52 px glyph, then name over meta, then the socket code. Hover fills `sheet-2` and lifts the glyph 2 px at 1.04 scale. Selection fills `sheet-2`, draws a 1.5 px inset `pick` outline, and fills the row's number key (1 to 9) with `pick`.
- **Off row ("Doesn't fit here"):** compact, with a 28 px glyph at 45% opacity and 30% saturation, the name at 14 px in `ink-2`, the reason in 12 px meta, and no code. Selected, it draws a `bad` outline.
- **Socket code:** a 32 px hex in the slot key's geometry, every edge a line in its socket colour, turned to the rotation the row's picture shows, with the picture's turn transition. It is the tile's legend entry and lines up against the slot key.

### Toast
- A margin note at bottom left on `sheet`: an icon, text over an optional 13 px sub line, and an optional action button on `sheet-2`. It rises 8 px and fades in, dismisses after 4 s, and pauses while hovered. One toast shows at a time and updates in place, with a count pill for repeats ("2 Undone"). The icon carries tone: `bad` or `ok`.
- **Update note:** the toast's persistent variant for a newer deployed build: the `retry` icon, "New version available" over a sub line that says the world is saved first (or, in `caution`, that autosave is off and reloading loses it), and a Reload action. It sits above the toast at 84 px from the bottom, never times out, and hides with the HUD (H, photo and walk modes).

### Tag
- **Ghost tag:** sits beside the ghost or the bulldoze target. It names the tile and rotation and gives fit text in `ok` or `bad`. A refusal sets the headline over the reason in `bad` and adds an inset `bad` ring.
- **Settings tag:** 12 px, weight 700, on an 8% white chip.

### Callout and Tooltip
- **Callout (first run):** a small ink chip with a key cap and an icon, plus a dashed 1.5 px ink leader line ending in a dot, like a map callout. It fades and rises 4 px over 240 ms.
- **Tooltip:** an ink chip showing name, key cap and an optional note. It appears after a 350 ms hover delay or immediately on keyboard focus, slides 4 px over 120 ms, and sits 8 px below its target.

### Fields: Switch, Segmented Control, Slider, Note
- **Field:** a label and a value on one line, with a hint below in `ink-3`. Fields are divided by 1 px `rule`.
- **Switch:** 44 × 26 px on `track` with an `ink-2` knob. When on, it fills `ink` and the knob turns `sheet` and slides 18 px, over 120 ms.
- **Segmented control:** sits on a 22% black well with 2 px padding and a 6 px radius. Options are 40 px tall and 4 px radius. The checked option fills `ink` with `ink-on-ink` text. An option may carry a 12 px sub line.
- **Slider:** a 4 px track, filled in `ink` up to the value. The 18 px ink thumb has a 3 px `sheet` ring and scales to 1.12 on hover. On focus it shows a 5 px ink halo at 30%.
- **Note:** a legend entry inside a sheet. It is a `sheet-2` card with a 20 px icon column and a bold `ink` lead line over `ink-2` text. Tone lives only in the icon colour (caution, bad or ok), never in a stripe.

### Buttons
- **Shape:** 4 px radius, 44 px tall (36 px for the small size), weight 700.
- **Default:** `sheet-2`, hovering to `sheet-3`. **Ink:** the primary action, `ink` with `ink-on-ink` text, hovering to white. **Bad:** destructive, `bad` with dark text. **Quiet:** transparent `ink-2`, hovering to `sheet-2`. Pressing moves a button down 1 px.

### Signature recurrences
The slot key and socket code are the system's identity, and every later surface must reuse them:
1. **Nothing fits:** the slot key crossed in `bad`.
2. **Bulldoze key:** the slot key turned into a removal legend.
3. **Worlds mix strip:** a 6 px bar of socket-colour segments, flexed by each kind's tile count. It keys a whole world, both on save rows (120 px) and on the current world.
4. **Import preview:** the incoming file's mix strip and counts, shown before it replaces or joins a world.
5. **Loading hex key:** a 56 px hex whose six edges fill in the socket colours in order (G, F, R, V, B, O) as the kit arrives, then fills with Grass.
6. **In-world socket key marks:** a 4 px line in the neighbour's socket colour along the middle 40% of each touched edge, 11 px inside the slot. It is the world's twin of the slot key (`design/ui/legend/in-world.md`).

### Motion
- **Curve:** one curve, `cubic-bezier(.22, .9, .3, 1)`, for everything.
- **Durations:** 120 ms for hover and select, 180 ms for nudges and the base, and 240 ms for the sheet slide, callouts and the refusal pulse.
- **The rise:** belongs to the engine (1.5 to 2.5 s per tile).
- **Reduced motion:** `prefers-reduced-motion: reduce`, or the engine's own flag (`set_reduced_motion`, mirrored on the root as `is-reduced-motion`), turns every transition and keyframe into an instant cut. The in-game override in Settings, Controls is designed (mockup S9.6) but not built. Indeterminate progress becomes a full-width bar at 35%, and the ghost stops breathing. State still shows by colour, pattern, icon and text. The `?still` flag that pins motion for captures exists only in the mockups (`design/ui/legend/proto.js`).

## Do's and Don'ts

### Do:
- **Do** key every tile, slot and world with the six socket colours in edge order: the code, the key, the mix strip.
- **Do** pair every legality colour with a pattern and words: solid for ok; dashed with a cross tick for bad; thin for neutral.
- **Do** keep `pick` for the current tool and the current selection only.
- **Do** set every text in Manrope with tabular figures, at 12, 13, 14, 16 or 20 px, in weight 600, 700 or 800.
- **Do** use ink, at 2 px offset 2 px, for the focus ring on every control (`--focus-ring`, `--focus-width`, `--focus-offset`).
- **Do** lift `ink-3` to `ink-3-lifted` on any `sheet-2` surface.
- **Do** keep panels in the margins and the centre clear for the ghost, and apply the 1400 px narrow rule.
- **Do** give every control a 44 px target (36 px for the small button).
- **Do** use dashed strokes for open or unresolved shapes and drop-shadow glow for edge states. Both are native to this world.

### Don't:
- **Don't** use a socket colour as decoration on chrome. The only standing exception is `ghost-fill`, the River hue at 12% on the ghost cap, which `design/ui/legend/in-world.md` specifies.
- **Don't** mark selection or note tone with a coloured side stripe. Use the 1.5 px full outline for selection and the icon colour for note tone.
- **Don't** group the Tiles sheet by category. It groups by consequence: "Fits here" first, then "Doesn't fit here".
- **Don't** hide tiles that do not fit. Dim them and state the reason.
- **Don't** close the Tiles sheet on slot hover. Only Tab or its close control closes it.
- **Don't** add a second family, uppercase labels or tracked-out caps.
- **Don't** use glass, backdrop blur or bordered panels. Depth is tonal survey blue plus an ambient shadow.
