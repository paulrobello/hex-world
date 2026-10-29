# ADR 0003: Separate renderer primitives from play presentation

Status: accepted for ARC-002, 2026-10-16.

## Context

The renderer previously owned play presentation and its consumers. Boids read the play view, kit, rise readiness and scatter layouts. Cars read streaming pack state. Walking and debug overlays read play geometry. Tier changes observed the play rebuild. Conversely, play used renderer materials, rotor helpers, LOD ranges, water primitives, scenario state and render configuration. Moving only `play/` would create a crate cycle.

## Decision

Use two new crates and keep the dependency graph directed downward:

- `hex-app` depends on `hex-play`, `hex-render`, `hex-sim`, and `hex-core`.
- `hex-play` depends on `hex-render`, `hex-play-state`, `hex-assets`, and `hex-core`.
- `hex-render` depends on `hex-play-state`, `hex-assets`, and `hex-core`.
- `hex-play-state` depends on `hex-core` and Bevy, never on either presentation crate.
- `hex-sim` depends on `hex-core`. `hex-assets` and `hex-core` have no internal crate dependencies.

`hex-play-state` owns `PlayView`, `PlayTile`, `PlayGhost`, `PlayCatalog`, `PlaySystems`, `PlaySpawned`, `PlayTileRoot`, `PlayWaiting`, `PlayKit`, `KitPart`, grass `Density`, `RisePart`, `RiseRole`, and `PropKind`. Kit state is generic over its material type. `hex-play::play` supplies the concrete `KitMaterial` aliases, so the shared crate does not depend on the renderer's material implementation. This preserves typed asset handles without dynamic dispatch or a duplicated readiness mirror.

`hex-play` owns the complete play module, including the ARC-003 plugins, plus its direct runtime consumers: boids, cars, walking, tier coordination, and the play-water tile adapter. Rise state and scatter layouts now have all their consumers in the same crate. The water adapter and boid planner move mechanically without algorithm changes. Their cross-module tests move with them.

`hex-app` explicitly installs `HexRenderPlugin` followed by `HexPlayPlugin`. The latter installs `PlaySceneSetup`, a function resource used by the existing scene setup system on startup and every scenario switch. This is the one lifecycle inversion: a renderer cannot call a higher-level crate directly. A function pointer is sufficient for this single synchronous setup operation. The shared data moves do not introduce service traits. Rise pose evaluation uses an extension trait because its component now belongs to the shared crate while animation math belongs to play.

The no-game-rules contract remains: play renders a view written by `hex-app`. Placement legality, world mutation, history and save rules remain in their existing crates.

## Tooling and runtime lighting

Both new presentation build surfaces default to no tooling. `hex-render/gate-tools` compiles benchmark geometry, the gate-only instance pipeline and single-tile kit fixture helper. Chunk partitioning and kit mesh baking are normal renderer primitives used by settled play presentation. `hex-play/gate-tools` enables that renderer feature plus diagnostics, the FPS display, developer overlays and per-pass GPU timing. `hex-app` explicitly opts in so existing CLI and browser developer controls remain available with unchanged behavior.

The old `lookdev` module is production lighting, not capture tooling: play uses its environment, exposure, backdrop and sunlight. It is named `lighting` now, with a compatibility `lookdev` re-export. It remains in default builds to preserve play appearance. Native capture orchestration already belongs to `hex-app` and stays there.

## Consequences

The compiler enforces an acyclic crate graph. Renderer-only clients no longer compile play, boids, walking, diagnostics or benchmark pipelines. Default play clients omit diagnostic tooling. The app retains its complete development interface. Shared spawn state exposes fields to its presentation owner across the crate boundary. This is data ownership, not a new gameplay API.

Bevy remains pinned through the workspace to `=0.20.1` (0.20.0 until 2026-10-29), with default features disabled. Both native and wasm32 compilation remain required. The new play and state crates retain the renderer's release optimization level for wasm builds.
