# Engine state-change feedback inventory

Owner request 2026-10-21 (item 3): inventory every in-world state-change feedback with its duration and easing, compare it with the HUD motion tokens and the mockup motion, align the ones that are off, respect reduced motion, and show frame strips for review. Looks were not changed beyond timing and easing; anything that would need more is a question at the end.

The HUD scale is `--t-fast` 120 ms, `--t` 180 ms, `--t-slow` 240 ms on one curve, `cubic-bezier(.22, .9, .3, 1)` (`.impeccable/design.json`, `docs/checkpoints/ui-feedback/inventory.md`). The engine now names the same values `T_FAST`, `T` and `T_SLOW` in `crates/hex-play/src/play/affordances.rs`, and the test `feedback_runs_on_the_hud_motion_tokens` reads them back from `design.json` and checks that every fade in the affordance timing table is one of them. The engine's `ease()` is that exact curve (24-step bisection), not an approximation. The written spec is `design/ui/legend/in-world.md` (iw); its off-scale values are tokenised as the owner's Q7 answer asked (direction.md, 2026-10-13), and iw now records the built value beside each.

Everything advances on Bevy's virtual `Time` (`delta_secs`), so `--clock fixed` gives exact frame times. Reduced motion (`ReducedMotion`, set by `set_reduced_motion`, which the overlay sends from the OS setting or Settings, Controls, Reduce motion On) cuts every row below to its end state.

Audit date 2026-10-21, on 6c9aab8 plus this branch. "Changed" marks the rows this pass touched.

## Micro-feedback: on the token scale

| # | State change | Before | After | Spec |
|---|---|---|---|---|
| E1 | Ghost turn on R, Shift+R, Ctrl+wheel | **Missing.** The ghost root was rebuilt at the new rotation: an instant snap. The HUD inventory claimed a 200 ms turn; that was wrong | **Changed**: the ghost turns from the yaw on screen to the new rotation over `T` (180 ms) on `ease()`, the short way (a half turn goes clockwise). A press mid-turn starts from where the turn is, so five fast presses land five rotations at once and the ghost turns to the last. Only a rotation change on the same cell and tile turns it (R, and also an auto-rotate there, as after an undo changes what fits); a new cell or tile shows the rotation at once. The engine does not know the press direction, so a step of four or more edges (a tile with few distinct fits) turns the short way, against the press, while the HUD's thumbnails nudge with the press. Reduced motion: a snap. `ghost.rs` `GHOST_TURN`, `nearest_steps`, `turn_steps`; test `the_ghost_turns_the_short_way_over_the_hud_t_on_the_legend_curve` | iw 72: 200 ms (180 to 220) |
| E2 | Edge glow and socket marks on a rotate, pin or tile change | Cut to 0, fade in over a literal 0.12 s | Unchanged timing (now `T_FAST`). See Q2 | iw 73: out over 60 ms, in over 120 ms when the turn lands |
| E3 | Ghost appears on a hovered slot (meshes and outline) | Waits 150 ms, fades in over 120 ms | Unchanged (the wait is `GHOST_DELAY`, a hover delay, not a transition; the fade is `T_FAST`) | iw 52: appears 150 ms after the hover |
| E4 | Ghost leaves (pointer off a slot) | The outline faded out over 100 ms; the meshes cut at once | **Changed**: the outline fades over `T_FAST` (120 ms). The meshes still cut (Q3) | iw 52: disappears in 100 ms |
| E5 | Hovered slot fill and ink dashes; hovered placed tile rim | 120 ms in and out | Unchanged (`T_FAST`) | iw 31, 41: 120 ms |
| E6 | Free-slot ring hides and returns (bulldoze, photo) | 160 ms in and out | **Changed**: `T` (180 ms) | iw 32: 160 ms |
| E7 | Bulldoze target highlight (removable, blocked, cut-off hatch) | 160 ms in, 150 ms out | **Changed**: `T` (180 ms) both ways, so the whole bulldoze mode change (the ring leaving, the target arriving) runs on one duration, as the HUD's bulldoze dim does (`--t`) | iw 111: mode change 150 ms; iw 114: highlights 160 ms (the spec disagreed with itself) |
| E8 | Waiting outline (a tile whose pack is still streaming) | 160 ms in, 120 ms out | **Changed**: `T` in, `T_FAST` out | not in iw |
| E9 | Refused placement, blocked bulldoze: the edges pulse once | `sin(πu)` over a literal 0.24 s (two copies) | Unchanged timing, one constant (`T_SLOW`, 240 ms, as the HUD tag's pulse). A there-and-back pulse keeps its sine shape: the one curve is for transitions | iw 86, 112: 240 ms |
| E10 | Selection, pinned slot | Pinning swaps the dashed outline for a solid one at once and reruns E2's glow fade. There is no in-world selection colour (magenta is HUD only) | Unchanged | iw 34 (no timing) |

## Choreography: its own timing, kept

These are not state-change transitions on the HUD scale. Each has its own spec, and several are pinned by tests (`crates/hex-play/src/play/rise.rs`, `crates/hex-core/src/tile.rs`). None changed.

| # | What | Timing and easing | Reduced motion | Spec |
|---|---|---|---|---|
| C1 | Commit flash on placement | ink edges at emissive ×4, fading over 80 ms (`FLASH`) | not drawn | iw 82: 80 ms (Q4) |
| C2 | Staged rise | 2.0 s timeline: column 0 to 0.6 s (ease-out cubic), cap overshoot 0.45 to 0.85 s, edges 0.8 to 1.2 s (ease-out back, water fill smoothstep), props from 1.1 s staggered (ease-out back per floor), agents at 1.6 s; camera shake over the column stage; dust puff about 1.2 s | instant cut, every cue sent as skipped | PRD §4.4: 1.5 to 2.5 s per tile |
| C3 | Undo and redo sink | 400 ms ease-in (`u²`), with a small puff | despawned at once | iw 90: 400 ms ease-in |
| C4 | Bulldoze sink and dust cloud | 700 ms ease-in sink inside a cloud that clears by 2.24 s | despawned at once, no cloud | iw 112, 113 |
| C5 | Replace | the old tile sinks 400 ms, then the new tile's rise starts | cut | iw 98, 99 |
| C6 | Ghost breathing | 100% to 55% and back over 2.4 s, cosine | held at 100% | iw 51: 2.4 s ease-in-out |

## Input never waits

The engine applies `rotate`, `place`, `bulldoze` and the tool toggles on the frame they arrive; no animation gates a command. The ghost turn is display only: a new rotation replaces the running turn in the same frame. `e2e/ui-motion.mjs` still sends five R presses in one task and sees five `ghostRotated` events, under full motion, OS reduced motion, the engine flag and Settings On.

## Evidence

Frame strips from the native build at 960x600, `--clock fixed` (16.7 ms a frame, so each label is exact), `--tilt off`. Each frame is labelled with its time after the command. The `rm-` strips repeat each one under reduced motion, where the ghost snaps (`rm-ghost-turn.jpg`: no frame differs from the one before), the pulse never shows and the tile is up and gone at once; the windmill's sails keep turning in every strip, which is ambient motion, not feedback.

- `ghost-turn.jpg` / `rm-ghost-turn.jpg`: E1, and E2's glow returning.
- `ghost-in.jpg` / `rm-ghost-in.jpg`: E3.
- `refused.jpg` / `rm-refused.jpg`: E9.
- `rise.jpg` / `rm-rise.jpg`: C1 and C2.
- `bulldoze-mode.jpg` / `rm-bulldoze-mode.jpg`: E6 and E7.
- `bulldoze.jpg` / `rm-bulldoze.jpg`: C4.

Regenerate: `python3 docs/checkpoints/engine-feedback/make_script.py`, then `make shots-piece SCRIPT=docs/checkpoints/engine-feedback/engine-feedback.json OUT=target/engine-feedback-frames ARGS="--scenario play --clock fixed --water_clock fixed --size 960x600"`, then `python3 docs/checkpoints/engine-feedback/strips.py target/engine-feedback-frames`.

## Questions for the owner

- **Q1.** The ghost turn (E1) is new motion where the engine had none, built from in-world.md's Rotate line at 180 ms. Keep it?
- **Q2.** The edge glow (E2) cuts to 0 on R and is back by 120 ms, 60 ms before the ghost's turn lands. The spec fades it out over 60 ms and back in when the turn lands. Holding the glow back until 180 ms would match the spec; 60 ms is off the token scale, so the fade-out would become a cut or 120 ms. Change it?
- **Q3.** When the pointer leaves the slots the ghost meshes vanish at once while the outline fades over 120 ms (E4). A 120 ms mesh fade-out means keeping the old ghost alive while it fades. Worth it?
- **Q4.** The 80 ms commit flash (C1) is off the token scale. Leave it as a flash, or move it to 120 ms?
- **Q5.** Settings, Controls: the mockup put Reduce motion under "Placing" beside Auto-rotate. Auto-rotate has no switch in the engine (it is always on), so it is not built, and Reduce motion sits under "Accessibility" with a one-line hint explaining that the system setting always wins. The Keys table is not built either. Keep the heading, and should the Keys table and an Auto-rotate switch follow?
