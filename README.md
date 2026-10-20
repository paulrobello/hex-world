# Hex World

[![CI](https://github.com/paulrobello/hex-world/actions/workflows/ci.yml/badge.svg)](https://github.com/paulrobello/hex-world/actions/workflows/ci.yml)

A browser world-building game on hexagon tiles, built with Rust, Bevy and WebGPU.
Play: https://hex-world.pardev.net (requires a WebGPU browser).

See `PRD.md` for the product plan. Build with `make run` (native) or `make build-web`.

Agents and scripts can drive, screenshot and inspect the game natively and on the web: see [docs/agent-operability.md](docs/agent-operability.md).

## Status

- **P0 foundations:** done. Engine decision (Bevy stays, provisional) in [docs/decisions/0001-engine.md](docs/decisions/0001-engine.md).
- **P1 asset pipeline proof:** done. Blender pipeline, kit material, look-dev, meadow/road pieces, hero oak and provenance gates. Decisions in [docs/decisions/0002-asset-pipeline.md](docs/decisions/0002-asset-pipeline.md).
- **P2 prototypes already on main:** the hero-water look-dev scenario (`--scenario water`) and the tilt-shift post pass (`--tilt`), both owner-approved.
- **P2 MVP vertical slice:** most boxes done, live at hex-world.pardev.net; the open ones are owner approvals and measurements (`PRD.md` §15 P2).
- **P3 content ahead of recipes:** the windmill, farm (barn, bales, paddock fence, cows and sheep), wheat field and bakery tiles are in the inventory; rail pieces (track, crossing, junction, station, steam train, river rail bridge) are modelled but not yet in play. Kit variety grew on 2026-10-20: house, oak and hay hero variants, forest undergrowth (ferns, logs, shrubs, saplings) and road verge bushes. Recipes, rail routing and animal wander are next.
- **Current state for a new session:** [handoff.md](handoff.md).

## Change history

There is no separate release changelog. Use `git log --oneline` for merged changes and `PRD.md` §15 for phase completion and remaining scope. Keep the phase checkboxes current when a phase lands, and describe each change in its commit subject. The deployed content-hash build id identifies a build, not a semantic release version.

## Prerequisites

For running and checking the game:

- Rust **stable**, with `rustfmt`, `clippy` and the `wasm32-unknown-unknown` target.
- [Trunk](https://trunkrs.dev/) **0.21.14** (the version used by CI).
- Node.js **22.18+** (native TypeScript stripping) and npm. CI uses **Node 24**.
- `make`, Git and a WebGPU-capable browser. On Linux, the native build also needs the system libraries installed in `.github/workflows/ci.yml`: `libasound2-dev`, `libudev-dev`, `libwayland-dev`, `libxkbcommon-dev`.

Asset authoring is optional for playing or running the normal gates. Install its tools separately:

| Work | Tools |
|------|-------|
| Kit generation | Blender (the pipeline uses 5.2); `BLENDER=/path/to/blender` overrides the Makefile's macOS app path. |
| Kit texture compression | KTX-Software **4.4.2**. `make fetch-tools` downloads and extracts **only this tool**, locally under `tools/`, on macOS ARM64, and writes `tools/ktx-env.sh`. Other platforms need their own KTX installation. |
| Audio mastering and checkpoint images/video | Python 3, ffmpeg/ffprobe (including `libopus` for audio and `libx264` for video), and ImageMagick (`magick`). |
| Tile thumbnails | [uv](https://docs.astral.sh/uv/) and Python **3.11+**. `make thumbs` uses uv to install the script's pinned **Pillow 12.2.0** dependency. |
| Brand rebuilds | **resvg 0.48.1** (`brew install resvg` on macOS, then verify the version). |

## Getting started

From a fresh checkout:

```sh
git clone https://github.com/paulrobello/hex-world.git
cd hex-world
rustup toolchain install stable --component rustfmt --component clippy
rustup target add wasm32-unknown-unknown
cargo install trunk --version 0.21.14 --locked
trunk serve
```

Open http://127.0.0.1:8094. Trunk watches and rebuilds the game; its pre-build hook runs `make ui-build`, which installs the pinned UI npm dependencies when needed. The committed runtime assets are enough to play without Blender or KTX tools.

Run `make checkall` for the fast gate (Rust formatting, Clippy, native/WASM checks, workspace tests and UI checks). `make checkall-ci` adds the release web build and size budgets. For native iteration use `make run ARGS="--scenario play"`; for a static release web build use `make build-web`.

## Where to start

- Product plan and phase checklists: `PRD.md` (§15 is the phase list).
- 3D asset work: the project skill `.claude/skills/blender-asset/` (`make kit-build`, `kit-compress`, `kit-provenance`, `kit-check`). `.blend` sources live only in the main checkout's gitignored `assets-source/blend/`.
- Gates: `make checkall` (fast), `make checkall-ci` (CI parity, run before any push), `make web-smoke` (local GPU browser, including the clamped 16-texture `water_kit` case), `make initial-download` (30 MiB first-load budget, also in `checkall-ci`), `make kit-check` (assets).
- Past plans: `docs/superpowers/plans/`.
- Brand: `make brand` rebuilds the favicon, app and manifest icons, share image and icon concept sheet from `design/brand/` (needs resvg 0.48.1).

## License

[MIT](LICENSE), copyright 2026 Paul Robello.
