# UI screen and state inventory

The full mockup pass (PRD §4.7) covers every screen and key state below, in the chosen direction, with motion shown as an interactive prototype or a clip. Motion notes are defaults for every direction: short eased feedback (120–320 ms) that explains cause and effect and never blocks input. Under `prefers-reduced-motion`, transitions become instant cuts, looping motion stops, and the state change is still shown by colour, icon and text.

Sources: PRD §4.1–§4.7, §8.1, §9.1, §9.2, §12.2, §13; TC4, TC5; D4, D8, D9, D15, D18, D20.

## S1. Loading

| State | What it shows | Motion |
|---|---|---|
| S1.1 First load | Name, a progress bar for wasm, core kit and IBL bytes (the ≤ 30 MB initial download), and the current stage name. | Bar fills linearly with real bytes. No looping spinner. |
| S1.2 Warming pipelines | Render pipelines compile behind the loading screen (TC5). The stage reads "Preparing the renderer". Progress counts pipelines when it can, otherwise an indeterminate bar. | Indeterminate bar sweeps at 1.2 s per pass. Stops under reduced motion and shows a static "working" label. |
| S1.3 Ready | The loading layer fades away and the first meadow tile is revealed (A1). | 300 ms fade out, input enabled before the fade ends. |
| S1.4 Load failure | Network or decode failure: what failed, a Retry button, and a link to report. | Shake-free. The error panel fades in over 150 ms. |
| S1.5 Streaming after first interaction | A small, quiet indicator while biome packs stream (§10). | 150 ms fade in and out. Never covers the inventory. |

## S2. Play HUD

| State | What it shows | Motion |
|---|---|---|
| S2.1 Idle, world of one tile | Tool rail, inventory closed or open, a first-run hint pointing at the open ring of slots. | Hint fades in after 1.5 s idle, out on first action. |
| S2.2 Idle, busy world | Same chrome, autosave state, tile count. | None at rest. |
| S2.3 HUD hidden | Every panel tucks away (also the photo-mode hide). | Panels slide off their nearest edge, 200 ms. |
| S2.4 Tool hover and focus | Tooltip with name and shortcut, visible focus ring on keyboard focus. | Tooltip 120–140 ms fade and 4 px slide. |
| S2.5 Tool press | Pressed and active states for Place, Rotate, Undo, Redo, Bulldoze, Photo, Settings. | 120 ms press scale, then a 200 ms fire ring or flash. |
| S2.6 Touch layout (P7 readiness) | Same rail at ≥ 44 px targets, no hover-only information. | Same as desktop. |

## S3. Inventory and legal-slot filter (D4, §4.1, §4.2)

| State | What it shows | Motion |
|---|---|---|
| S3.1 Open, no slot hovered | All unlocked tiles, unlimited counts, none dimmed. | Panel opens in 200–260 ms from its edge (Tab toggles). |
| S3.2 Slot hovered, filter active | Tiles that fit (with legal rotation count) and tiles that do not (dimmed, with the socket-table reason). | Fitting tiles brighten or re-rank, 200–320 ms. Dimming is 160 ms. |
| S3.3 Slot changes | Filter recomputes for the new slot. | Same as S3.2, items keep identity (no blink). |
| S3.4 Nothing fits this slot | Every tile dimmed. A plain explanation of what the slot needs and a pointer to the nearest slot where something fits. | 160 ms. |
| S3.5 Tile hover | Glyph or thumbnail lifts, name emphasised. | 120–140 ms. |
| S3.6 Tile selected (fits) | Selected styling, ghost appears in the hovered slot at the first legal rotation (auto-rotate). | 200 ms select, ghost fades in over 150 ms. |
| S3.7 Tile selected (does not fit) | Selected with the incompatible colour, ghost shows red edges, reason repeated near the ghost. | 200 ms. No shake. |
| S3.8 Keyboard navigation | Arrow keys move through tiles, number keys pick, Enter selects. Focus ring visible. | 120 ms focus move. |
| S3.9 Closed | Only the open handle remains. | 200–260 ms slide out. |
| S3.10 Thumbnails | Rendered per-rotation thumbnails replace the stand-in glyphs once they exist (F4). | Thumbnails rotate in step with the ghost (R). |

## S4. Ghost, rotation and edge glow (§4.1, §4.2)

| State | What it shows | Motion |
|---|---|---|
| S4.1 Ghost in a legal rotation | Translucent preview, touching edges glow green, untouched edges neutral. | Ghost outline breathes over 2.4 s (static under reduced motion). |
| S4.2 Rotate (R, Shift+R, Ctrl+wheel) | Ghost turns 60°, edge glow recomputes, rotation readout updates ("Rotation 2 of 2"). | 180–220 ms eased turn. Holding R repeats without queueing. |
| S4.3 Cycle legal rotations | With auto-rotate, R steps through legal rotations only and wraps. | Same as S4.2. |
| S4.4 Illegal rotation (manual rotate mode) | The touching edges that clash glow red, the others stay green or neutral. | Glow colour crossfades 120 ms. |
| S4.5 No legal rotation | Ghost shows red on clashing edges and a short reason. | 120 ms. |
| S4.6 Ghost over a non-slot | No ghost. The cursor shows the current tool. | 100 ms fade out. |

## S5. Place and rise (§4.4, D15)

| State | What it shows | Motion |
|---|---|---|
| S5.1 Place accepted | The ghost commits, the staged rise plays, the HUD acknowledges once (placement cue). | HUD acknowledgement ≤ 200 ms. The rise itself is the world's (1.5–2.5 s) and never blocks input. |
| S5.2 Place rejected | Left-click on an illegal ghost: the incompatible edges pulse once, reason stays visible. | Single 240 ms pulse, no screen shake. |
| S5.3 Place during a rise | Placing the next tile fast-forwards any running rises to their end state. | The HUD shows nothing extra. |

## S6. Undo and redo (§4.2)

| State | What it shows | Motion |
|---|---|---|
| S6.1 Undo | Toast "Undid: placed River bend at (q, r)" with a Redo action. The undone tile reverses its rise or vanishes. | Toast 200 ms in, auto-dismiss at 4 s, pauses while hovered. |
| S6.2 Redo | Toast "Redid: ..." with an Undo action. | Same. |
| S6.3 Repeated undo | One toast that updates its text and count ("Undid 3 actions"), never a stack of toasts. | Text crossfade 120 ms. |
| S6.4 Undo stack empty | Undo button disabled with a tooltip "Nothing to undo". | Disabled state, no animation. |
| S6.5 Redo stack empty | Redo disabled, tooltip "Nothing to redo". | Same. |
| S6.6 Stack depth | At least 100 actions, persisted with the save. The UI never shows the number unless asked. | None. |

## S7. Bulldoze (D20, §4.2)

| State | What it shows | Motion |
|---|---|---|
| S7.1 Bulldoze mode on (B) | Tool active in the incompatible colour, cursor changes, ghost hidden, hovered tile outlined. | 150 ms mode change. |
| S7.2 Hover a removable tile | Tile outlined as removable. | 120 ms. |
| S7.3 Bulldoze done | Tile removed, toast "Removed Forest" with Undo. | Toast as S6.1. |
| S7.4 Blocked: would split the world | The tile outlined as blocked, plus the island the removal would cut off highlighted, and the message "Removing this would split your world". | 160 ms highlight of the cut-off region. |
| S7.5 Blocked: last tile | Message "The world needs at least one tile". | 160 ms. |
| S7.6 Exit bulldoze | B again, Esc, or picking another tool. | 150 ms. |

## S8. Photo mode (§4.6, §9.1)

| State | What it shows | Motion |
|---|---|---|
| S8.1 Enter (P) | HUD hides, photo panel appears with free camera hints. | HUD out 200 ms, panel in 200 ms. |
| S8.2 Panel | Depth of field (focus distance, aperture, tilt-shift preset), exposure, FOV, roll, Hide UI, Pause agents, time of day (disabled until P5). | Slider values update live. |
| S8.3 Hidden UI | Only a small "Show UI" affordance on hover or a key reminder. | 200 ms. |
| S8.4 Export choice | PNG at 1×, 2× or 4×, with output dimensions shown. | None. |
| S8.5 Export in progress | Progress for the offscreen render, Cancel. | Determinate bar. |
| S8.6 Export done | "Saved hex-world-2026-10-13.png (5760 × 3600)" with Open or Show. | Toast 200 ms. |
| S8.7 Export failed | What failed (for example the 4× target is too large for this GPU) and a suggestion (try 2×). | 150 ms. |
| S8.8 Exit | Esc or P, the HUD returns in its previous state. | 200 ms. |

## S9. Settings (§9.1, §9.2, §8)

| State | What it shows | Motion |
|---|---|---|
| S9.1 Graphics | Quality tier Low, Medium, High, Ultra with the auto-detected tier marked ("Auto: High") and a manual override. Resolution scale shown per tier. | 150 ms panel transitions. |
| S9.2 Tier changed | The change applies, and a note explains a frame-rate impact when going up. | 150 ms. |
| S9.3 Auto-detect running | A short benchmark on first run, with the result. | Determinate bar. |
| S9.4 Tilt-shift | Toggle (on by default from Medium tier up) and strength slider, previewed live in the scene. | Slider live. |
| S9.5 Audio | Master, music, ambience, effects, UI volume and mute, applied live. | Sliders live. |
| S9.6 Controls | Read-only control table (§4.5), and the reduced-motion preference echoed from the OS with an in-game override. | None. |

## S10. Saves, load, export and import (D9, §12.2)

| State | What it shows | Motion |
|---|---|---|
| S10.1 Autosave indicator | "Saving" then "Saved" after each placement (debounced), quiet. | 150 ms text crossfade. |
| S10.2 Save slots | Named slots with a thumbnail, tile count and last-saved time. New slot, rename, duplicate. | List items 150 ms. |
| S10.3 Load slot | Confirm when the current world has unsaved changes. | 200 ms. |
| S10.4 Overwrite or delete slot | Confirmation naming the slot and what will be lost. | 200 ms. |
| S10.5 Export `.hexworld` | Downloads the file and confirms its name. | Toast 200 ms. |
| S10.6 Import `.hexworld` | File picker or drop target, then a preview of the world before replacing or adding as a slot. | Drop target highlights 120 ms. |
| S10.7 Import: older version | Notice that the file was migrated from an older format. | Toast 200 ms. |
| S10.8 Import: invalid file | What is wrong (not a Hex World file, corrupt, from a newer version) and what to do. | 150 ms. |
| S10.9 Storage unavailable | IndexedDB blocked (private mode): autosave off, export still works. | 150 ms. |

## S11. Unsupported browser (§13, TC4)

| State | What it shows | Motion |
|---|---|---|
| S11.1 No WebGPU | A static page in the chosen direction: what is missing, the supported browsers (Chrome and Edge 113+, Safari 26+, Firefox 141+ on Windows), and a still of the world. | None. |
| S11.2 WebGPU present but adapter failed | Same page with the specific reason and a retry. | None. |

## S12. Cross-cutting states

| State | Notes |
|---|---|
| Reduced motion | Every row above shows its reduced-motion behaviour. |
| Keyboard only | Every action reachable, visible focus everywhere, Tab reserved for the inventory with a documented focus escape. |
| Narrow desktop (1280 × 720) | Panels keep the ghost area clear. |
| Contrast over the scene | Every panel holds WCAG AA over bright grass, water and the dark void. |
| Audio cues (§8.1) | Select, rotate, invalid placement, undo, bulldoze each pair with a UI cue at −20 LUFS. |
