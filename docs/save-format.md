# Save format (`.hexworld`)

This is the schema contract for Hex World saves (PRD §12.2, D9). The code is `crates/hex-core/src/save.rs`, and the historical fixtures live in `crates/hex-core/tests/fixtures/save/`.

## What a save holds

| Part | Contents |
|---|---|
| World | The world seed, and every placed tile with its cell, reserved elevation level, tile id, rotation and per-cell seed |
| History | The undo stack and the redo stack (PRD §4.2), so Ctrl+Z works after a load |
| Unlocks | Unlocked content ids (P3). Always empty today |
| Camera | Orbit focus point, yaw, pitch and distance |
| Settings | Quality tier (Auto, Low, Medium, High, Ultra), the tilt-shift toggle and strength, and audio volumes for the master, music, SFX, ambience and UI buses |

Agents are not saved. They are re-derived from the world on load.

## File layout

The envelope integers are fixed-width little-endian.

| Offset | Size | Field |
|---|---|---|
| 0 | 8 | Magic `HEXWORLD` (ASCII) |
| 8 | 2 | Format version, u16 |
| 10 | 4 | Payload length `N`, u32 |
| 14 | `N` | Payload: postcard encoding of the version's schema |
| 14 + `N` | 4 | CRC-32 (IEEE, as computed by `crc32fast`) over bytes `0 .. 14 + N`, u32 |

The 10-byte magic and version prefix is frozen for every version. Everything after it belongs to the version, and every version so far uses the layout above. Files larger than 16 MiB are rejected before any parsing.

## Payload encoding

The payload is postcard 1.x (pinned to `=1.1.3`). Postcard is not self-describing, so field order and enum variant order are the format.

- `u8` and `bool` are one byte. A `bool` must be 0 or 1.
- `u16`, `u32` and `u64` are LEB128 varints. `i32` is zigzag-encoded, then written as a varint.
- `f32` is 4 bytes, IEEE-754, little-endian.
- A `String` is a varint byte length followed by UTF-8. A `Vec` is a varint count followed by its elements. A fixed array such as `[f32; 3]` is its elements with no count.
- A struct is its fields in declaration order, with no names. An enum is a varint variant index followed by that variant's fields.

## Version 1 schema

```rust
Save {
    world_seed: u64,
    tiles: Vec<Tile>,          // sorted by (q, r); each cell at most once
    undo: Vec<HistoryAction>,  // oldest first
    redo: Vec<HistoryAction>,  // the next redo last
    unlocks: Vec<String>,      // sorted
    camera: Camera,
    settings: Settings,
}
Tile { q: i32, r: i32, level: u8, def: String, rotation: u8, seed: u64 }
HistoryAction { kind: ActionKind, tile: Tile }  // tile.q and tile.r are the action's cell
ActionKind = Place (0) | Bulldoze (1)
Camera { focus: [f32; 3], yaw: f32, pitch: f32, distance: f32 }
Settings {
    quality: Quality,
    tilt_shift: bool,
    tilt_shift_strength: f32,
    volume_master: f32,
    volume_music: f32,
    volume_sfx: f32,
    volume_ambience: f32,
    volume_ui: f32,
}
Quality = Auto (0) | Low (1) | Medium (2) | High (3) | Ultra (4)
```

`q` and `r` are pointy-top axial coordinates. `level` is the tile's elevation level, reserved from day one (PRD D5 plans levels 0..=3). The MVP has a single level, so encoders write 0 and decoders reject anything else. P4 widens the accepted range to 0..=3 without a version bump, because a v1 file that uses levels is still a valid v1 file. A pre-P4 build reports such a file as `LevelOutOfRange` rather than `UnsupportedVersion`. Rotation `k` means local edge `e` faces world edge `e + k`. A `Place` action carries the tile it placed, and a `Bulldoze` action carries the tile it removed. Distances are in meters, angles are in radians, and volumes are linear gains.

## Versions

| Version | Introduced | Change | Fixture |
|---|---|---|---|
| 1 | 2026-10-13 (P2 wave 2, W2a) | First version | `v1-small-world.hexworld` |

## Decode order and errors

`SaveData::decode(bytes, catalog)` runs these checks in order and returns the first failure as a `SaveError`. It never panics on untrusted bytes. Fuzz tests in `save.rs` cover random input, every truncation and single-bit flip of a valid file, and random or mutated payloads behind a valid checksum.

1. `TooLarge`: the file is over 16 MiB.
2. `BadMagic`: the file does not start with `HEXWORLD`. A file that is a prefix of the magic is `Truncated` instead.
3. `Truncated`: the file is shorter than the 14-byte header.
4. `UnsupportedVersion`: the version is 0 or newer than this build. This runs before the length and checksum checks, so a file from a newer build always reports it, whatever that version's layout is.
5. `Truncated` or `TrailingBytes`: the file length differs from `18 + N`.
6. `ChecksumMismatch`: the CRC-32 trailer does not match.
7. `Decode`: postcard rejects the payload, or bytes remain after the schema ends.
8. Migration to the current schema (see below). Converting to runtime types rejects a duplicate cell (`InvalidWorld` with `DuplicateCell`) and a level above the supported maximum (`InvalidWorld` with `LevelOutOfRange` for a tile, or `InvalidHistory` with `InvalidState` for a history record).
9. `InvalidValue`: a camera or settings float is non-finite, or a volume is outside `0.0..=1.0`. The error names the field.
10. `InvalidWorld`, with a `WorldIssue`: the tile map is `Empty`, repeats a cell (`DuplicateCell`), carries a level above 0 (`LevelOutOfRange`, until P4 raises the maximum to 3), has a coordinate outside `-2^24..=2^24` (`OutOfRange`, checked before any neighbor arithmetic so extreme values cannot overflow), names a tile id the catalog lacks (`UnknownTile`), has a rotation of 6 or more (`BadRotation`), has a touching edge pair the socket table rejects (`Incompatible`), or is not one connected piece (`Disconnected`).
11. `InvalidHistory`, with a `HistoryIssue`: undo plus redo exceeds the history depth of 128 (`TooDeep`), an action does not match the cell it applies to (`Mismatch`), or applying an action leaves a world that fails step 10 (`InvalidState`).

The history check replays the whole undo stack backward and the whole redo stack forward from the saved world. `World::undo` and `World::redo` skip rule checks, so a tampered history would otherwise reach play. Every world that play can reach passes this check.

Some fields are deliberately not checked:

- Per-cell seeds are not compared with `cell_seed`, so a later feature can reroll a tile without a format change.
- Unlock ids are not checked against the catalog, because P3 defines what they name.

## Migrations

**The rule: every format change bumps the version and adds a fixture.** Never edit an existing `vN` wire module or an existing fixture.

`migrate(version, payload)` decodes a payload with its own version's wire structs, upgrades them one version at a time to the current version, and converts the result to the runtime types. To add version `N`:

1. Copy `mod v{N-1}` in `save.rs` to `mod vN` and make the change there. Move the runtime conversions (`from_runtime` and `into_runtime`) to `vN`, and give `v{N-1}::Save` an upgrade step that produces `vN::Save`.
2. Add an `N` arm to `migrate`. The `N-1` arm becomes "decode, then upgrade".
3. Bump `SAVE_FORMAT_VERSION`.
4. Add an `#[ignore]` writer for `v<N>-<name>.hexworld` in `tests/save_fixtures.rs`. Run it once, commit the file, and add a test that checks the new fixture's contents. Writers refuse to overwrite an existing file.
5. Add a row to the version table above.

`every_fixture_loads_and_every_version_has_one` loads every `v<N>-*.hexworld` file and checks four things:

- the header version matches the file name
- the file decodes and validates against the built-in catalog
- every version from 1 to the current one has a fixture
- current-version fixtures re-encode byte for byte, which catches accidental wire changes

Wire structs never use serde attributes that postcard cannot honor: `#[serde(default)]`, `skip_serializing_if`, `flatten` or `untagged`. A new field always means a new version.

**Catalog changes are migration events too.** Saves are validated against the built-in tile catalog. Renaming or removing a tile id, or changing sockets so that an existing adjacency becomes illegal, breaks older saves. The fixture test fails when that happens. Handle it with a version bump whose upgrade step maps or repairs the old data.

## Slots and autosave

The IndexedDB autosave slots (web layer, later) store the same `.hexworld` bytes as a file export. `SaveSlotMeta` holds the slot-list summary: name, save time, tile count and format version. The caller supplies the save time in Unix milliseconds, because hex-core reads no clock (`SystemTime::now` panics on `wasm32-unknown-unknown`).
