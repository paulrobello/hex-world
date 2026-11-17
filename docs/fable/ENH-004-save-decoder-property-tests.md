# ENH-004: Property tests for the `.hexworld` decoder

| Field | Value |
|---|---|
| **Source** | Audit 2026-11-15: the security review did no fuzzing and left one question open |
| **Impact** | Medium: the only parser of untrusted input gets randomized coverage in the normal gate |
| **Effort** | S to M |
| **Depends on** | SEC-002 (its oracle test builds the same random worlds; share the generator) |
| **Owner decision needed** | No |

## Goal

`cargo test -p hex-core` proves, over randomized input, that `decode` never panics, that encode and decode round-trip, and that a world at the coordinate limit survives the operations play runs on a loaded world.

## Current state (at `edbc7ad`)

- `crates/hex-core/src/save.rs` decodes in a strict order: byte cap (`MAX_SAVE_BYTES`, line 28), magic, version, exact length, CRC-32, exact postcard consumption, value ranges, world rules (`validate_tiles`, 418), history replay (`validate_history`, 518). The security review found no slice or arithmetic panic by reading, and ran no fuzzer.
- Tests are example-based: inline tests in `save.rs` and fixtures for every historical version in `crates/hex-core/tests/save_fixtures.rs` (`tests/fixtures/save/`).
- Open question from the audit: decode accepts cells out to ±2^24 (`CELL_LIMIT`, `save.rs:36`), and whether `World` operations, the simulation and rendering tolerate such coordinates was not traced. The workspace profile is `panic = "abort"` (`Cargo.toml:26`), so a downstream panic on a resumed world repeats on every start.
- `hex-core` dev-dependencies: `serde_json` only. Nothing in the workspace uses `proptest` or `cargo-fuzz`.

## Design

Use `proptest` as a `hex-core` dev-dependency, pinned exactly. It runs inside `cargo test`, so `make checkall` and CI cover it on ubuntu and macos with no new tool, and its shrinker gives a minimal failing file. A separate `cargo-fuzz` target is not added: it needs nightly and would sit outside every gate.

A CRC-protected format defeats naive byte mutation (every mutant fails the checksum), so the mutation property re-stamps the CRC after mutating the payload.

## Steps

1. `crates/hex-core/Cargo.toml`: `[dev-dependencies] proptest = "=<current 1.x>"`. Run `make check-wgpu` and confirm `Cargo.lock` gains only proptest's tree.
2. `crates/hex-core/tests/save_props.rs` (new), with a shared module `tests/common/worlds.rs`:
   - `arb_world()`: a strategy that builds a `World` by applying a random sequence of `place`, `replace`, `bulldoze`, `undo` and `redo` through the public API from the start tile, choosing among the cells and orientations `World` reports as legal. Include mega tiles and levels 0 to 3. Cap sequences at 200 actions.
   - `restamp(bytes)`: recompute the envelope CRC over the payload so a mutated file passes the checksum. Read the envelope layout from `docs/save-format.md` and `save.rs:321-385`; expose a `#[doc(hidden)] pub fn` in `save.rs` only if the layout cannot be reproduced from public constants.
3. Properties (256 cases each by default; `PROPTEST_CASES` raises it locally):
   - **P1 no panic on arbitrary bytes**: `decode(&bytes)` returns for any `Vec<u8>` up to 4 KiB, and for any bytes prefixed with the valid magic and version.
   - **P2 no panic on mutated valid files**: take `encode(arb_world())`, flip 1 to 8 random payload bytes or truncate or extend the payload, `restamp`, decode. Any `Ok` or `Err` is a pass; a panic fails.
   - **P3 round trip**: `decode(encode(w)) == w` for `arb_world()` (compare tiles, history stacks, seed, settings).
   - **P4 accepted implies playable**: for every P2 mutant that decodes `Ok`, run `undo` until empty, `redo` until empty, and `can_bulldoze` on every tile. None may panic. This is the property that guards the `panic = "abort"` resume loop.
   - **P5 coordinate limit**: translate an `arb_world()` so its extent touches `±CELL_LIMIT` (and a mega anchor at the limit, whose ring is one past it, as `save.rs:1952` already does by example), encode, decode, then run P4's operations plus `fits` on every frontier slot.
4. Commit the `proptest-regressions/` directory proptest writes on a failure, so found cases replay in CI. Add it to nothing in `.gitignore`.
5. Downstream of `hex-core`: add one `hex-sim` test that builds the network and steps the simulation 600 ticks on a P5-style world near the limit (roads and a rail loop translated to `q = CELL_LIMIT - 8`). Rendering is not covered here: note on the card that `f32` world positions lose precision at that distance (2^24 cells times the cell size exceeds `f32` integer precision), and file a follow-up to lower `CELL_LIMIT` if the owner wants loaded worlds to render correctly at any accepted coordinate.
6. If any property fails, fix the decoder or the operation, not the test, and add the shrunk case as an example test beside the existing ones in `save.rs`.

## Files to touch

`crates/hex-core/Cargo.toml`, `Cargo.lock`, `crates/hex-core/tests/save_props.rs` (new), `crates/hex-core/tests/common/worlds.rs` (new), `crates/hex-sim/tests/` (one new test), possibly `crates/hex-core/src/save.rs` (a hidden CRC helper), `docs/save-format.md` (a "Tests" paragraph).

## Verify

- `cargo test -p hex-core --test save_props` passes with the default case count and with `PROPTEST_CASES=4096` locally (record the run time; keep the default-count run under 20 s so the fast gate stays fast).
- A deliberately planted bug (make `validate_tiles` skip the rotation range check in a scratch build) is caught by P2 or P4 within the default case count.
- `cargo test -p hex-sim` passes with the near-limit world.
- `make checkall-ci` passes; `make web-size` is unchanged (dev-dependencies do not ship).

## Rollback

Remove the dev-dependency and the two test files; nothing in production code depends on them.

## Out of scope

Fuzzing GLB and KTX2 parsing (same-origin assets), and changing `CELL_LIMIT`.
