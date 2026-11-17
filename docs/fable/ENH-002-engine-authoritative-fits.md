# ENH-002: Engine-authoritative placement fits

| Field | Value |
|---|---|
| **Source** | Audit 2026-11-15, architecture finding ARC-002 (Medium) |
| **Impact** | Medium: placement rules are written once; the HUD can no longer show a fit the engine refuses |
| **Effort** | M to L |
| **Depends on** | ARC-003 (typed shim) is helpful but not required |
| **Owner decision needed** | Yes: `PRD.md` §5 (line 200) records the TypeScript mirror as intentional |

## Goal

`hex-core` is the only implementation of placement legality. The Tiles sheet reads which tiles and orientations fit the selected slot from an engine-published state field, as it already does for footprint (mega) tiles. `web/ui/src/rules.ts` keeps wording and presentation only.

## Current state (at `edbc7ad`)

- `web/ui/src/rules.ts` (942 lines) re-implements socket compatibility, directional beach edges, stopped rail ends, lanes, deck levels, corner heights and the placement ranking: `laneMeets:28`, `heightsMeet:219`, `meets:232`, `placementMeets:267`, `facingAt:322`, `needsAt:375`, `refusedWhy:410`, `legalOrientations:504`, `legalRotations:541`, `fit:618`, `megaFit:676`, `fitEntry:728`, `tunnelCover:849`, `cutOff:880`. It is imported by `debug.ts`, `hud.ts`, `glyph.ts`, `import-model.ts`, `sheet-controller.ts`, `slots.ts`, `sheet.ts`, `words.ts`, `worlds.ts`.
- A parity fixture guards the mirror: `crates/hex-app/src/bridge.rs` `mod schema` (about lines 937-1024) computes the legal orientations of every tile at every frontier slot of eight worlds from `hex-core` and writes `web/ui/test/rules-parity.json`; `web/ui/test/rules.test.ts` checks `rules.ts` against it. Parity holds only for the worlds the fixture enumerates.
- The design is already split: mega tiles do not use the mirror. `GameState` caches `World::mega_slots` per footprint tile on `World::stamp` (`crates/hex-app/src/game.rs:237-280`) and publishes `play.mega_slots`. `PRD.md:700` records that choice ("The HUD learns where it fits from an engine-published `mega_slots` state field rather than mirroring the rules").
- The last five commits each changed a placement rule (elevation, raised land edges, rail at levels) and had to change it in both languages.

## Design

1. **New state field `play.fits`**, published for the slot the sheet is showing (the pinned slot, else the hovered free slot):
   ```json
   { "slot": {"q": 3, "r": -1},
     "needs": ["Grass", null, "Road", ...],
     "stopped": [false, ...],
     "entries": [ { "id": "road_straight", "orientations": [[0,0],[3,0]], "refusal": null },
                  { "id": "ocean", "orientations": [], "refusal": "beach_needed" } ] }
   ```
   `orientations` is in `hex-core` ranking order (the same `[rotation, level]` pairs the parity fixture emits). `needs` and `stopped` feed the wording functions, so `needsAt` and `facingAt` can go.
2. **Computed in `hex-app`, cached like megas.** Move the per-slot computation out of `mod schema` into a `GameState::fits_of(slot) -> Arc<Fits>` beside `mega_slots_of`, cached on `(World::stamp, slot)`. `mod schema` and `sync_view` both call it, so the test fixture and the game share one code path.
3. **Published only when it changes.** `StatePublication` already sends per-key dirty patches; `fits` is dirty when the slot or the world stamp changes. Size: about 40 picker entries with at most 24 orientation pairs each, a few kilobytes per slot change, nothing per frame at rest.
4. **Refusal codes, not wording, cross the bridge.** `refusedWhy` and `swapRefusal` currently derive a reason in TypeScript. The engine publishes a code from a closed enum (contract-tested through `bridge-schema.json`) and `words.ts` maps code to text.

## Risk to settle first: one frame of latency

Today the sheet recomputes fits synchronously when the hovered slot changes. With an engine-published field the update arrives with the next state patch, one frame later. `PRD.md` §5 and `e2e/ui-motion.mjs` hold the rule that input never waits on feedback. Step 1 measures this before anything is deleted; if the lag is visible, keep the hovered-slot path on the mirror and stop (see Rollback).

## Steps

1. **Spike, no deletion.** Add `fits_of` and publish `play.fits` behind the existing state path. In `sheet-controller.ts`, log (debug HUD only) whenever `play.fits` disagrees with `fitEntry(...)` for the same slot, and record the frame delta between a slot change and the matching `fits` patch. Run `make ui-e2e` and a manual hover sweep in a booked window. Decision point: zero disagreements on the eight parity worlds plus a hand-built elevation and rail world, and the owner accepts the one-frame lag by eye.
2. **Add refusal codes.** Enumerate the reasons `refusedWhy` returns (read `rules.ts:410-492`), add a `Refusal` enum in `hex-core` returned by the placement check, derive `Serialize`, emit its variants in the schema, map them in `words.ts`. Unit-test each code in `hex-core`.
3. **Switch the sheet.** `sheet-controller.ts`, `sheet.ts`, `hud.ts` and `slots.ts` read `play.fits` for legality, orientation lists, the rotation readout and the R cycle. `fitEntry`, `megaFit` and `entryReadout` become thin adapters over the published entries, then are inlined.
4. **Delete the mirror.** Remove `legalOrientations`, `legalRotations`, `placementMeets`, `meets`, `heightsMeet`, `laneMeets`, `facingAt`, `needsAt`, `fit`, `refusedWhy`, `swapRefusal` and their tests. Check each of `tunnelCover`, `isCover`, `cutOff`, `diffTiles` separately: they serve the bulldoze preview and import summary, not placement, and stay unless the engine already publishes the same fact.
5. **Retire the parity fixture.** Delete `web/ui/test/rules-parity.json` and its generator once nothing reads it; `make ui-schema` then regenerates only `bridge-schema.json`.
6. **Docs.** `PRD.md` §5: replace "the web overlay mirrors it" (line 200) and "mirrored in the HUD rules" (line 680) with the engine-published design and a dated decision line. `docs/agent-operability.md` State table: add `fits`. `CLAUDE.md` "Rust ↔ UI changes are two-sided": drop the `rules-parity.json` mention.

## Files to touch

`crates/hex-app/src/game.rs`, `crates/hex-app/src/bridge.rs`, `crates/hex-core/src/world.rs` and `catalog.rs` (refusal codes), `web/ui/src/bridge.ts`, `web/ui/src/bridge-schema.json`, `web/ui/src/rules.ts`, `web/ui/src/sheet-controller.ts`, `sheet.ts`, `hud.ts`, `slots.ts`, `words.ts`, `debug.ts`, `web/ui/test/rules.test.ts`, `web/ui/test/rules-parity.json`, `PRD.md`, `docs/agent-operability.md`, `CLAUDE.md`.

## Verify

- `cargo test -p hex-core` covers every refusal code and the orientation ranking.
- `cargo test -p hex-app bridge::schema` passes after `make ui-schema`; `make ui-check` passes.
- `wc -l web/ui/src/rules.ts` is under 400 and `grep -nE 'legalOrientations|placementMeets|heightsMeet|facingAt' web/ui/src` prints nothing.
- Booked load-sensitive window: `make ui-e2e` (including `e2e/ui-motion.mjs`, which asserts input never waits on feedback) and `make web-smoke` pass.
- A rule change made only in `hex-core` (flip one pair in a scratch `data/sockets.ron`) changes what the sheet offers with no TypeScript edit.
- `make checkall-ci` passes.

## Rollback

Steps 1 and 2 are additive and can ship alone: the published field and refusal codes exist while the mirror stays authoritative. Steps 3 to 5 are one squash commit; `git revert` restores `rules.ts`, its tests and the parity fixture. If the latency spike fails, stop after step 2 and record the measurement and the decision in `PRD.md` §5.

## Out of scope

Bulldoze preview rules (`cutOff`, `tunnelCover`), the import summary, and any change to the rules themselves.
