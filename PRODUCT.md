# Product

Product truth for the Hex World player UI, condensed from `PRD.md` for the impeccable design flow. Every field below is **inferred from the PRD and the P2 wave-1 brief**, not from an owner interview (no question channel was available to the design sub-agent on 2026-10-13). The owner confirms or corrects it at the direction checkpoint.

## Platform

web

## Stack

Inferred. An HTML/CSS overlay written in TypeScript over the Bevy/WebGPU game canvas, talking to Rust through wasm-bindgen (PRD D18). No component library. The shipped page runs under a strict CSP (`default-src 'self'`), so fonts and assets are self-hosted.

## Users

Inferred. Players of calm, creative builders (the audience of tile-placing diorama games). Desktop first, with mouse and keyboard. Touch arrives in P7, so every control is sized and spaced for a finger from day one (D8).

## Product Purpose

Inferred. A browser world-building game. The world starts as one meadow hex. The player picks a tile, rotates it until its edge sockets match the neighbours, and places it. The tile rises out of the ground, and the world comes alive with cars, birds, water and sound. There is no fail state and no economy (D1).

## Positioning

Inferred. A "stylized-real diorama" at AAA visual quality (D10). The diorama is the hero. The UI frames it and never competes with it (PRD §4.7).

## Operating Context

Inferred. Long, relaxed sessions at a desk. The camera orbits a floating island on a near-black void. The UI must stay legible over bright grass, water and the dark void alike.

## Capabilities and Constraints

- Placement: inventory with a legal-slot filter, ghost preview, 60° rotation, auto-rotate to the first legal rotation, edge glow (green compatible, red incompatible, neutral no neighbour) (§4.1, §4.2).
- Undo/redo of at least 100 actions; bulldoze, blocked when it would split the world or remove the last tile (§4.2, D20).
- Photo mode with free camera, DOF, tilt-shift preset, exposure, FOV, roll, PNG export at 1×/2×/4× (§4.6).
- Settings: quality tier (Low/Medium/High/Ultra, auto-detected, overridable), tilt-shift toggle and strength slider, audio (§9.1, §9.2, §8).
- Saves: IndexedDB autosave, named slots, `.hexworld` export and import (§12.2).
- Unsupported-browser page when WebGPU is missing (§13).
- Controls: Tab inventory, R / Shift+R rotate, Ctrl/Cmd+Z undo, Ctrl/Cmd+Shift+Z redo, B bulldoze, P photo (§4.5).
- Strict CSP: no CDN fonts or scripts in the shipped game.

## Brand Commitments

Inferred. Name "Hex World". No logo exists yet. No committed palette or typeface.

## Evidence on Hand

Real in-engine screenshots: `docs/checkpoints/water/*.jpg`, `docs/shots/tilt-shift/*.png`. Tile thumbnails do not exist yet (F4: pending), so comps use stand-in glyphs drawn from each tile's socket string.

## Product Principles

1. The diorama is the hero; the UI is a frame.
2. Valid actions are obvious without help text (§4.7).
3. Every state change gets short, eased feedback that explains cause and effect and never delays input; reduced motion is respected (§4.7).

## Accessibility & Inclusion

Inferred. WCAG AA contrast for text over a moving 3D scene, full keyboard operation, visible focus, 44 px minimum touch targets, `prefers-reduced-motion` honoured, colour never the only carrier of legal/illegal state.
