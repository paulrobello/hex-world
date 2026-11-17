# ENH-003: Safe start and single-decode import

| Field | Value |
|---|---|
| **Source** | Audit 2026-11-15, follow-up to security finding SEC-002 |
| **Impact** | Medium: a save that hangs or aborts on load no longer locks the player out of every saved world |
| **Effort** | S to M |
| **Depends on** | SEC-002 (edits the same decode and import paths) |
| **Owner decision needed** | The parameter name and the notice wording |

## Goal

1. A start-up option skips the automatic resume of the most recent world, so a world that freezes or aborts the game can be deleted or exported from the Worlds screen instead of clearing site data.
2. Importing a file decodes it once, not twice.

## Current state (at `edbc7ad`)

- `web/ui/src/persist.ts:223` `Persistence.start()`: after `refresh()` it takes `this.slots[0]` and calls `this.open(last, false)`. Nothing skips it. `open` reads the slot bytes and calls `this.host.load(bytes)` (about line 513).
- The workspace builds wasm with `panic = "abort"` (`Cargo.toml:26`). A world that panics downstream of a valid decode aborts on every start, because it stays the most recent slot. Decode accepts cells out to ±2^24 (`crates/hex-core/src/save.rs:36`) and downstream tolerance of extreme coordinates is unverified (ENH-004 tests it).
- The HUD already reads its own query parameters from `location.search` (`web/ui/src/main.ts:94-103`, `version-check-model.ts:12`), separate from the engine's allowlist in `crates/hex-app/src/args.rs:541-576`.
- Import decodes twice: `hexWorld.inspect(bytes)` for the preview (`crates/hex-app/src/persist.rs:263` `inspect_json`), then `hexWorld.load(bytes)` on confirm (`persist.rs:224` `request_load`). Each is a full `decode` including history validation.

## Design

1. **`?safe=1`, HUD-side only.** `start()` behaves as if no slot were resumable: the start world shows, `firstVisit()` is not called (it is not a first visit: the title theme and first-visit hints stay off), a notice says the last world was not opened, and the Worlds screen opens with every slot listed so the player can delete, export or open one deliberately. The engine needs no change, so the parameter is not added to `web_pairs`.
2. **Autosave must not overwrite anything.** `start()` already sets `this.current` to a fresh unsaved slot id before resuming; in safe mode that fresh slot stays current and unsaved until the player places a tile, exactly like a new world.
3. **Automatic offer after a failed start.** Set a `sessionStorage` flag before `host.load` in the start-up resume and clear it when the `loaded` event arrives. If the page starts and finds the flag still set, the previous start did not finish: skip the resume as if `?safe=1` were present and say so. `sessionStorage` survives a reload of the same tab, which is what a player does after a freeze or an abort.
4. **Single decode.** In `persist.rs`, `inspect_json` keeps its decoded `(SaveData, version)` in a one-slot `Mutex<Option<Inspected>>` keyed by the byte length and `crc32fast::hash(bytes)`. `request_load` takes it when the key matches and skips `decode`. The slot is cleared on any other load, on `inspect` of different bytes, and when the import preview is dismissed (the HUD already calls a cancel path; add a bridge call or clear on the next command).

## Steps

1. `web/ui/src/persist-model.ts` (new, or beside the existing start-up helpers in `settings-model.ts`): `export function startupResume(search: string, crashed: boolean): "resume" | "safe"`. Unit-test it.
2. `persist.ts` `start()`: compute the mode; in `"safe"` skip `firstVisit()` and `open(last, false)`, set `this.ready = true`, call `this.ux.notify({ text: "Opened without your last world", sub: "Open, export or delete it from Worlds.", icon: "alert" })` and `this.opts.openWorlds?.()` (add the option; `main.ts:122` passes a callback that opens the Worlds screen).
3. Crash flag: `sessionStorage.setItem("hexworld.resuming", id)` immediately before the start-up `host.load`; remove it in the `loaded` case of `onEvents` and in the refusal branch of `open`. Wrap both in guarded storage helpers — written fresh in `web/ui/src/persist-model.ts` when this shipped (2026-11-16); QA-010's shared helpers never landed.
4. Rust: add `struct Inspected { len: usize, crc: u32, save: SaveData, version: u16 }` and `static INSPECTED: Mutex<Option<Inspected>>` in `persist.rs`. `inspect_json` stores on success. `request_load` computes `(len, crc)`, takes a matching entry, else decodes. Add tests beside `inspect_reports_tiles_or_refusal_and_queues_nothing` (`persist.rs:778`): load after inspect of the same bytes does not call decode (count calls with a `#[cfg(test)]` counter), load of different bytes does, and a second inspect replaces the slot.
5. Docs: `docs/agent-operability.md` web parameters (a HUD-side row for `safe`), `docs/save-format.md` (recovery paragraph), `PRD.md` saves section (dated decision line).
6. e2e: extend `e2e/persist.mjs` with two cases: a start with `?safe=1` shows the start world with slots intact, and a start with the crash flag pre-set in `sessionStorage` does the same.

## Files to touch

`web/ui/src/persist.ts`, `web/ui/src/persist-model.ts` (new), `web/ui/src/main.ts`, `web/ui/test/persist-model.test.ts` (new), `crates/hex-app/src/persist.rs`, `e2e/persist.mjs`, `docs/agent-operability.md`, `docs/save-format.md`, `PRD.md`.

## Verify

- `node --test web/ui/test/persist-model.test.ts`: `?safe=1` and the crash flag both give `"safe"`; neither gives `"resume"`.
- `cargo test -p hex-app persist`: the inspect-then-load test proves one decode.
- Booked window: `node e2e/persist.mjs dist` passes with the two new cases, and no slot is modified or deleted by a safe start (compare the IndexedDB slot list before and after).
- `make ui-check` and `make checkall-ci` pass.

## Rollback

Two independent commits (safe start; single decode), each revertible alone. The `sessionStorage` key is ignored by older builds.

## Out of scope

Moving decode to a worker, and any change to what the decoder accepts (SEC-002).
