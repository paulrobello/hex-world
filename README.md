# Hex World

A browser world-building game on hexagon tiles, built with Rust, Bevy and WebGPU.
Play: https://hex-world.pardev.net (requires a WebGPU browser).

See `PRD.md` for the product plan. Build with `make run` (native) or `make build-web`.

Agents and scripts can drive, screenshot and inspect the game natively and on the web: see [docs/agent-operability.md](docs/agent-operability.md).

## Status

- **P0 foundations:** done. Engine decision (Bevy stays, provisional) in [docs/decisions/0001-engine.md](docs/decisions/0001-engine.md).
- **P1 asset pipeline proof:** done. Blender pipeline, kit material, look-dev, meadow/road pieces, hero oak and provenance gates. Decisions in [docs/decisions/0002-asset-pipeline.md](docs/decisions/0002-asset-pipeline.md).
- **P2 prototypes already on main:** the hero-water look-dev scenario (`--scenario water`) and the tilt-shift post pass (`--tilt`), both owner-approved.
- **Next: P2 MVP vertical slice.** Scope and open checkboxes in `PRD.md` §15 P2; the UI quality bar is §4.7 and needs owner-approved mockups before any UI is built.

## Where to start

- Product plan and phase checklists: `PRD.md` (§15 is the phase list).
- 3D asset work: the project skill `.claude/skills/blender-asset/` (`make kit-build`, `kit-compress`, `kit-provenance`, `kit-check`). `.blend` sources live only in the main checkout's gitignored `assets-source/blend/`.
- Gates: `make checkall` (fast), `make checkall-ci` (CI parity, run before any push), `make web-smoke` (local GPU browser), `make kit-check` (assets).
- Past plans: `docs/superpowers/plans/`.
