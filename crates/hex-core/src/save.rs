//! Versioned binary save and the `.hexworld` file envelope (PRD §12.2, D9). The byte layout,
//! version history, migration rule and error list are the contract in `docs/save-format.md`.
//!
//! The payload is postcard, which is not self-describing: field order and enum variant order are
//! the format. Each version therefore has its own frozen wire structs (module `v1`), converted to
//! and from the runtime types, so a change to [`World`] can never silently change the format.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::axial::Axial;
use crate::catalog::TileCatalog;
use crate::tile::{TileDef, opposite_edge};
use crate::world::{Action, HISTORY_DEPTH, PlacedTile, World};

/// First bytes of every `.hexworld` file.
pub const SAVE_MAGIC: [u8; 8] = *b"HEXWORLD";

/// The format version [`SaveData::encode`] writes. [`SaveData::decode`] reads `1..=` this.
pub const SAVE_FORMAT_VERSION: u16 = 1;

/// Largest file [`SaveData::decode`] looks at.
pub const MAX_SAVE_BYTES: usize = 16 * 1024 * 1024;

/// Magic, version (u16 LE) and payload length (u32 LE). Magic and version are frozen for every
/// version; the length field and CRC trailer belong to the version's layout.
const PREFIX_LEN: usize = 14;

/// Largest `|q|` or `|r|` a saved cell may have. Far beyond any reachable world, and small enough
/// that neighbor and distance arithmetic on a loaded world cannot overflow `i32`.
const CELL_LIMIT: i32 = 1 << 24;

/// Highest elevation level a saved tile may carry. The v1 record reserves the field (PRD D5 plans
/// levels 0..=3); P4 raises this bound instead of bumping the format version.
const MAX_LEVEL: u8 = 0;

/// CRC-32 (IEEE) trailer over everything before it, u32 LE.
const CRC_LEN: usize = 4;

/// Orbit camera pose, engine-free. `focus` is the orbit pivot in world meters; angles are radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraState {
    pub focus: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

/// Quality tier choice (PRD §9.2). `Auto` lets the game detect a tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QualitySetting {
    Auto,
    Low,
    Medium,
    High,
    Ultra,
}

/// Linear gains in `0.0..=1.0` per mix bus (PRD §8).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioVolumes {
    pub master: f32,
    pub music: f32,
    pub sfx: f32,
    pub ambience: f32,
    pub ui: f32,
}

/// Player settings that travel with the save.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameSettings {
    pub quality: QualitySetting,
    pub tilt_shift: bool,
    pub tilt_shift_strength: f32,
    pub volumes: AudioVolumes,
}

/// Everything a save holds. Agents are not saved: they are re-derived on load (PRD §12.2).
#[derive(Clone, Debug, PartialEq)]
pub struct SaveData {
    /// Tile map, rotations, seeds and the undo/redo history.
    pub world: World,
    /// Unlocked content ids (P3). Not checked against the catalog.
    pub unlocks: BTreeSet<String>,
    pub camera: CameraState,
    pub settings: GameSettings,
}

/// Summary of a save slot for the slot list. hex-core reads no clock, so the caller supplies the
/// timestamp.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveSlotMeta {
    pub name: String,
    /// Unix time in milliseconds.
    pub saved_at_ms: u64,
    pub tile_count: u32,
    pub format_version: u16,
}

impl SaveSlotMeta {
    pub fn new(name: impl Into<String>, saved_at_ms: u64, save: &SaveData) -> Self {
        Self {
            name: name.into(),
            saved_at_ms,
            tile_count: u32::try_from(save.world.tiles().len()).unwrap_or(u32::MAX),
            format_version: SAVE_FORMAT_VERSION,
        }
    }
}

/// Which history stack an issue was found in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryStack {
    Undo,
    Redo,
}

/// Why a tile map fails the world rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldIssue {
    Empty,
    DuplicateCell(Axial),
    /// A coordinate is outside `-2^24..=2^24`.
    OutOfRange(Axial),
    /// The reserved elevation level is above what this build supports (only 0 until P4).
    LevelOutOfRange {
        cell: Axial,
        level: u8,
    },
    UnknownTile {
        cell: Axial,
        id: String,
    },
    BadRotation {
        cell: Axial,
        rotation: u8,
    },
    /// The tile at `cell` and its neighbor across world edge `edge` do not fit.
    Incompatible {
        cell: Axial,
        edge: u8,
    },
    Disconnected,
}

impl fmt::Display for WorldIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorldIssue::Empty => write!(f, "the world has no tiles"),
            WorldIssue::DuplicateCell(c) => write!(f, "cell ({}, {}) appears twice", c.q, c.r),
            WorldIssue::OutOfRange(c) => write!(f, "cell ({}, {}) is out of range", c.q, c.r),
            WorldIssue::LevelOutOfRange { cell, level } => write!(
                f,
                "level {level} at ({}, {}) is above {MAX_LEVEL}",
                cell.q, cell.r
            ),
            WorldIssue::UnknownTile { cell, id } => {
                write!(f, "unknown tile {id:?} at ({}, {})", cell.q, cell.r)
            }
            WorldIssue::BadRotation { cell, rotation } => {
                write!(
                    f,
                    "rotation {rotation} at ({}, {}) is not 0..6",
                    cell.q, cell.r
                )
            }
            WorldIssue::Incompatible { cell, edge } => {
                write!(f, "edge {edge} of ({}, {}) does not fit", cell.q, cell.r)
            }
            WorldIssue::Disconnected => write!(f, "the world is not connected"),
        }
    }
}

/// Why the saved undo/redo history cannot be replayed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryIssue {
    /// Undo plus redo exceeds [`HISTORY_DEPTH`], which play can never produce.
    TooDeep(usize),
    /// Action `index` (0 = the next one applied) does not match the world it applies to.
    Mismatch {
        stack: HistoryStack,
        index: usize,
        cell: Axial,
    },
    /// Applying action `index` leaves a world that breaks the rules.
    InvalidState {
        stack: HistoryStack,
        index: usize,
        issue: WorldIssue,
    },
}

impl fmt::Display for HistoryIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HistoryIssue::TooDeep(n) => {
                write!(f, "{n} history actions exceed the depth {HISTORY_DEPTH}")
            }
            HistoryIssue::Mismatch { stack, index, cell } => write!(
                f,
                "{stack:?} action {index} does not match cell ({}, {})",
                cell.q, cell.r
            ),
            HistoryIssue::InvalidState {
                stack,
                index,
                issue,
            } => write!(
                f,
                "{stack:?} action {index} leaves an invalid world: {issue}"
            ),
        }
    }
}

/// Why a `.hexworld` file was rejected. Decoding never panics on untrusted bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    /// Larger than [`MAX_SAVE_BYTES`].
    TooLarge(usize),
    BadMagic,
    /// Version 0, or newer than [`SAVE_FORMAT_VERSION`].
    UnsupportedVersion(u16),
    /// Shorter than its header says.
    Truncated {
        expected: u64,
        actual: u64,
    },
    /// Extra bytes after the checksum.
    TrailingBytes(u64),
    ChecksumMismatch {
        stored: u32,
        computed: u32,
    },
    /// The payload is not a valid postcard encoding of its version's schema.
    Decode(String),
    /// A camera or settings value is non-finite or out of range; names the field.
    InvalidValue(&'static str),
    InvalidWorld(WorldIssue),
    InvalidHistory(HistoryIssue),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::TooLarge(n) => write!(f, "{n} bytes is over the {MAX_SAVE_BYTES} limit"),
            SaveError::BadMagic => write!(f, "not a .hexworld file"),
            SaveError::UnsupportedVersion(v) => write!(
                f,
                "format version {v} is not supported (this build reads 1..={SAVE_FORMAT_VERSION})"
            ),
            SaveError::Truncated { expected, actual } => {
                write!(f, "file is truncated: {actual} of {expected} bytes")
            }
            SaveError::TrailingBytes(n) => write!(f, "{n} unexpected bytes after the checksum"),
            SaveError::ChecksumMismatch { stored, computed } => write!(
                f,
                "checksum mismatch: stored {stored:#010x}, computed {computed:#010x}"
            ),
            SaveError::Decode(e) => write!(f, "payload does not decode: {e}"),
            SaveError::InvalidValue(field) => write!(f, "{field} is out of range"),
            SaveError::InvalidWorld(issue) => write!(f, "invalid world: {issue}"),
            SaveError::InvalidHistory(issue) => write!(f, "invalid history: {issue}"),
        }
    }
}

impl std::error::Error for SaveError {}

impl SaveData {
    /// The `.hexworld` bytes at [`SAVE_FORMAT_VERSION`]. Autosave slots store the same bytes.
    pub fn encode(&self) -> Vec<u8> {
        let payload = postcard::to_allocvec(&v1::Save::from_runtime(self))
            .expect("postcard serializes the v1 wire structs");
        envelope(SAVE_FORMAT_VERSION, &payload)
    }

    /// Reads a `.hexworld` file of any supported version, migrates it to the current schema and
    /// validates it against `catalog`.
    pub fn decode(bytes: &[u8], catalog: &TileCatalog) -> Result<Self, SaveError> {
        let (version, payload) = open_envelope(bytes)?;
        let save = migrate(version, payload)?;
        save.validate(catalog)?;
        Ok(save)
    }

    fn validate(&self, catalog: &TileCatalog) -> Result<(), SaveError> {
        validate_values(&self.camera, &self.settings)?;
        validate_tiles(self.world.tiles(), catalog).map_err(SaveError::InvalidWorld)?;
        validate_history(&self.world, catalog).map_err(SaveError::InvalidHistory)
    }
}

/// Wraps a payload: magic, version, payload length, payload, CRC-32 of all of that.
fn envelope(version: u16, payload: &[u8]) -> Vec<u8> {
    let len = u32::try_from(payload.len()).expect("a save payload is far below 4 GiB");
    let mut out = Vec::with_capacity(PREFIX_LEN + payload.len() + CRC_LEN);
    out.extend_from_slice(&SAVE_MAGIC);
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(payload);
    let crc = crc32fast::hash(&out);
    out.extend_from_slice(&crc.to_le_bytes());
    out
}

/// Checks the envelope in contract order and returns the version and payload.
fn open_envelope(bytes: &[u8]) -> Result<(u16, &[u8]), SaveError> {
    if bytes.len() > MAX_SAVE_BYTES {
        return Err(SaveError::TooLarge(bytes.len()));
    }
    let seen = bytes.len().min(SAVE_MAGIC.len());
    if bytes[..seen] != SAVE_MAGIC[..seen] {
        return Err(SaveError::BadMagic);
    }
    let actual = bytes.len() as u64;
    if bytes.len() < PREFIX_LEN {
        return Err(SaveError::Truncated {
            expected: (PREFIX_LEN + CRC_LEN) as u64,
            actual,
        });
    }
    let version = u16::from_le_bytes([bytes[8], bytes[9]]);
    if version == 0 || version > SAVE_FORMAT_VERSION {
        return Err(SaveError::UnsupportedVersion(version));
    }
    let declared = u32::from_le_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]) as u64;
    let expected = (PREFIX_LEN + CRC_LEN) as u64 + declared;
    if actual < expected {
        return Err(SaveError::Truncated { expected, actual });
    }
    if actual > expected {
        return Err(SaveError::TrailingBytes(actual - expected));
    }
    let body_end = bytes.len() - CRC_LEN;
    let stored = u32::from_le_bytes([
        bytes[body_end],
        bytes[body_end + 1],
        bytes[body_end + 2],
        bytes[body_end + 3],
    ]);
    let computed = crc32fast::hash(&bytes[..body_end]);
    if stored != computed {
        return Err(SaveError::ChecksumMismatch { stored, computed });
    }
    Ok((version, &bytes[PREFIX_LEN..body_end]))
}

/// The migration chain: decodes `payload` with its version's wire structs, upgrades them one
/// version at a time to the current one, and converts to runtime types. Rules are not checked.
/// A new version `N` adds an arm `N => …` and turns the `N - 1` arm into an upgrade step.
pub(crate) fn migrate(version: u16, payload: &[u8]) -> Result<SaveData, SaveError> {
    match version {
        1 => decode_exact::<v1::Save>(payload)?.into_runtime(),
        v => Err(SaveError::UnsupportedVersion(v)),
    }
}

/// Postcard decode that also rejects unread payload bytes (`from_bytes` would ignore them).
fn decode_exact<'a, T: Deserialize<'a>>(payload: &'a [u8]) -> Result<T, SaveError> {
    let (value, rest) =
        postcard::take_from_bytes(payload).map_err(|e| SaveError::Decode(e.to_string()))?;
    if !rest.is_empty() {
        return Err(SaveError::Decode(format!(
            "{} unread payload bytes",
            rest.len()
        )));
    }
    Ok(value)
}

fn validate_values(camera: &CameraState, settings: &GameSettings) -> Result<(), SaveError> {
    let finite: [(&'static str, f32); 7] = [
        ("camera.focus.x", camera.focus[0]),
        ("camera.focus.y", camera.focus[1]),
        ("camera.focus.z", camera.focus[2]),
        ("camera.yaw", camera.yaw),
        ("camera.pitch", camera.pitch),
        ("camera.distance", camera.distance),
        ("settings.tilt_shift_strength", settings.tilt_shift_strength),
    ];
    if let Some((field, _)) = finite.iter().find(|(_, v)| !v.is_finite()) {
        return Err(SaveError::InvalidValue(field));
    }
    let v = &settings.volumes;
    let gains: [(&'static str, f32); 5] = [
        ("settings.volumes.master", v.master),
        ("settings.volumes.music", v.music),
        ("settings.volumes.sfx", v.sfx),
        ("settings.volumes.ambience", v.ambience),
        ("settings.volumes.ui", v.ui),
    ];
    match gains.iter().find(|(_, g)| !(0.0..=1.0).contains(g)) {
        Some((field, _)) => Err(SaveError::InvalidValue(field)),
        None => Ok(()),
    }
}

/// The world rules every reachable world obeys: non-empty, coordinates in range, known tiles,
/// rotations `0..6`, every touching edge pair compatible, and one connected piece. The range check
/// runs before any neighbor arithmetic.
fn validate_tiles(
    tiles: &BTreeMap<Axial, PlacedTile>,
    catalog: &TileCatalog,
) -> Result<(), WorldIssue> {
    let Some(&start) = tiles.keys().next() else {
        return Err(WorldIssue::Empty);
    };
    let mut defs: BTreeMap<Axial, (&TileDef, u8)> = BTreeMap::new();
    for (&cell, t) in tiles {
        if [cell.q, cell.r]
            .iter()
            .any(|v| !(-CELL_LIMIT..=CELL_LIMIT).contains(v))
        {
            return Err(WorldIssue::OutOfRange(cell));
        }
        let Some(def) = catalog.get(&t.def) else {
            return Err(WorldIssue::UnknownTile {
                cell,
                id: t.def.clone(),
            });
        };
        if t.rotation >= 6 {
            return Err(WorldIssue::BadRotation {
                cell,
                rotation: t.rotation,
            });
        }
        defs.insert(cell, (def, t.rotation));
    }
    // The socket table is symmetric, so checking world edges 0..3 of every tile covers each
    // touching pair once.
    for (&cell, &(def, rotation)) in &defs {
        for w in 0..3 {
            let Some(&(other, other_rotation)) = defs.get(&cell.neighbor(w)) else {
                continue;
            };
            let theirs = other.edge(opposite_edge(w), other_rotation);
            if !catalog
                .sockets()
                .edges_compatible(def.edge(w, rotation), theirs)
            {
                return Err(WorldIssue::Incompatible { cell, edge: w });
            }
        }
    }
    let mut seen: BTreeSet<Axial> = BTreeSet::from([start]);
    let mut stack = vec![start];
    while let Some(cell) = stack.pop() {
        for e in 0..6 {
            let n = cell.neighbor(e);
            if tiles.contains_key(&n) && seen.insert(n) {
                stack.push(n);
            }
        }
    }
    if seen.len() == tiles.len() {
        Ok(())
    } else {
        Err(WorldIssue::Disconnected)
    }
}

/// Replays the whole undo stack backward and the whole redo stack forward from the current world,
/// as [`World::undo`] and [`World::redo`] would, checking each action against the cell it touches
/// and each resulting world against the rules. Those methods skip rule checks, so a tampered
/// history would otherwise reach play.
fn validate_history(world: &World, catalog: &TileCatalog) -> Result<(), HistoryIssue> {
    let (undo, redo) = world.history_stacks();
    let depth = undo.len() + redo.len();
    if depth > HISTORY_DEPTH {
        return Err(HistoryIssue::TooDeep(depth));
    }
    let mut tiles = world.tiles().clone();
    for (index, action) in undo.iter().rev().enumerate() {
        let stack = HistoryStack::Undo;
        let ok = match action {
            Action::Place { cell, tile } => tiles.remove(cell).as_ref() == Some(tile),
            Action::Bulldoze { cell, tile } => tiles.insert(*cell, tile.clone()).is_none(),
        };
        check_step(&tiles, catalog, stack, index, action, ok)?;
    }
    let mut tiles = world.tiles().clone();
    for (index, action) in redo.iter().rev().enumerate() {
        let stack = HistoryStack::Redo;
        let ok = match action {
            Action::Place { cell, tile } => tiles.insert(*cell, tile.clone()).is_none(),
            Action::Bulldoze { cell, tile } => tiles.remove(cell).as_ref() == Some(tile),
        };
        check_step(&tiles, catalog, stack, index, action, ok)?;
    }
    Ok(())
}

fn check_step(
    tiles: &BTreeMap<Axial, PlacedTile>,
    catalog: &TileCatalog,
    stack: HistoryStack,
    index: usize,
    action: &Action,
    matched: bool,
) -> Result<(), HistoryIssue> {
    if !matched {
        let (Action::Place { cell, .. } | Action::Bulldoze { cell, .. }) = action;
        return Err(HistoryIssue::Mismatch {
            stack,
            index,
            cell: *cell,
        });
    }
    validate_tiles(tiles, catalog).map_err(|issue| HistoryIssue::InvalidState {
        stack,
        index,
        issue,
    })
}

/// Format version 1 wire schema. Frozen: never edit these types once a v1 file exists; add a
/// `v2` module and an upgrade step instead (docs/save-format.md).
mod v1 {
    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    pub(super) struct Save {
        pub world_seed: u64,
        /// Sorted by `(q, r)`.
        pub tiles: Vec<Tile>,
        /// Oldest first.
        pub undo: Vec<HistoryAction>,
        /// The next redo last.
        pub redo: Vec<HistoryAction>,
        /// Sorted.
        pub unlocks: Vec<String>,
        pub camera: Camera,
        pub settings: Settings,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    pub(super) struct Tile {
        pub q: i32,
        pub r: i32,
        /// Reserved elevation level (PRD D5). Written as 0; decode rejects anything above
        /// [`MAX_LEVEL`].
        pub level: u8,
        pub def: String,
        pub rotation: u8,
        pub seed: u64,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    pub(super) enum ActionKind {
        Place,
        Bulldoze,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    pub(super) struct HistoryAction {
        pub kind: ActionKind,
        pub tile: Tile,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    pub(super) struct Camera {
        pub focus: [f32; 3],
        pub yaw: f32,
        pub pitch: f32,
        pub distance: f32,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    pub(super) enum Quality {
        Auto,
        Low,
        Medium,
        High,
        Ultra,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    pub(super) struct Settings {
        pub quality: Quality,
        pub tilt_shift: bool,
        pub tilt_shift_strength: f32,
        pub volume_master: f32,
        pub volume_music: f32,
        pub volume_sfx: f32,
        pub volume_ambience: f32,
        pub volume_ui: f32,
    }

    impl Tile {
        pub(super) fn new(cell: Axial, t: &PlacedTile) -> Self {
            Self {
                q: cell.q,
                r: cell.r,
                level: 0,
                def: t.def.clone(),
                rotation: t.rotation,
                seed: t.seed,
            }
        }

        fn into_runtime(self) -> Result<(Axial, PlacedTile), WorldIssue> {
            let cell = Axial::new(self.q, self.r);
            if self.level > MAX_LEVEL {
                return Err(WorldIssue::LevelOutOfRange {
                    cell,
                    level: self.level,
                });
            }
            Ok((
                cell,
                PlacedTile {
                    def: self.def,
                    rotation: self.rotation,
                    seed: self.seed,
                },
            ))
        }
    }

    impl HistoryAction {
        fn new(action: &Action) -> Self {
            match action {
                Action::Place { cell, tile } => Self {
                    kind: ActionKind::Place,
                    tile: Tile::new(*cell, tile),
                },
                Action::Bulldoze { cell, tile } => Self {
                    kind: ActionKind::Bulldoze,
                    tile: Tile::new(*cell, tile),
                },
            }
        }

        fn into_runtime(self) -> Result<Action, WorldIssue> {
            let (cell, tile) = self.tile.into_runtime()?;
            Ok(match self.kind {
                ActionKind::Place => Action::Place { cell, tile },
                ActionKind::Bulldoze => Action::Bulldoze { cell, tile },
            })
        }
    }

    /// Converts one history stack; issues use [`HistoryIssue`] indices (0 = the next applied).
    fn history_into_runtime(
        actions: Vec<HistoryAction>,
        stack: HistoryStack,
    ) -> Result<Vec<Action>, SaveError> {
        let len = actions.len();
        actions
            .into_iter()
            .enumerate()
            .map(|(i, a)| {
                a.into_runtime().map_err(|issue| {
                    SaveError::InvalidHistory(HistoryIssue::InvalidState {
                        stack,
                        index: len - 1 - i,
                        issue,
                    })
                })
            })
            .collect()
    }

    impl Save {
        pub(super) fn from_runtime(save: &SaveData) -> Self {
            let (undo, redo) = save.world.history_stacks();
            let c = &save.camera;
            let s = &save.settings;
            Self {
                world_seed: save.world.seed(),
                tiles: save
                    .world
                    .tiles()
                    .iter()
                    .map(|(&cell, t)| Tile::new(cell, t))
                    .collect(),
                undo: undo.iter().map(HistoryAction::new).collect(),
                redo: redo.iter().map(HistoryAction::new).collect(),
                unlocks: save.unlocks.iter().cloned().collect(),
                camera: Camera {
                    focus: c.focus,
                    yaw: c.yaw,
                    pitch: c.pitch,
                    distance: c.distance,
                },
                settings: Settings {
                    quality: match s.quality {
                        QualitySetting::Auto => Quality::Auto,
                        QualitySetting::Low => Quality::Low,
                        QualitySetting::Medium => Quality::Medium,
                        QualitySetting::High => Quality::High,
                        QualitySetting::Ultra => Quality::Ultra,
                    },
                    tilt_shift: s.tilt_shift,
                    tilt_shift_strength: s.tilt_shift_strength,
                    volume_master: s.volumes.master,
                    volume_music: s.volumes.music,
                    volume_sfx: s.volumes.sfx,
                    volume_ambience: s.volumes.ambience,
                    volume_ui: s.volumes.ui,
                },
            }
        }

        /// Structural conversion only. It catches the rules the runtime types cannot carry:
        /// duplicate cells (a map would silently keep one) and the reserved level.
        pub(super) fn into_runtime(self) -> Result<SaveData, SaveError> {
            let mut tiles: BTreeMap<Axial, PlacedTile> = BTreeMap::new();
            for t in self.tiles {
                let (cell, tile) = t.into_runtime().map_err(SaveError::InvalidWorld)?;
                if tiles.insert(cell, tile).is_some() {
                    return Err(SaveError::InvalidWorld(WorldIssue::DuplicateCell(cell)));
                }
            }
            let undo: VecDeque<Action> =
                history_into_runtime(self.undo, HistoryStack::Undo)?.into();
            let redo: Vec<Action> = history_into_runtime(self.redo, HistoryStack::Redo)?;
            let s = self.settings;
            Ok(SaveData {
                world: World::from_parts(self.world_seed, tiles, undo, redo),
                unlocks: self.unlocks.into_iter().collect(),
                camera: CameraState {
                    focus: self.camera.focus,
                    yaw: self.camera.yaw,
                    pitch: self.camera.pitch,
                    distance: self.camera.distance,
                },
                settings: GameSettings {
                    quality: match s.quality {
                        Quality::Auto => QualitySetting::Auto,
                        Quality::Low => QualitySetting::Low,
                        Quality::Medium => QualitySetting::Medium,
                        Quality::High => QualitySetting::High,
                        Quality::Ultra => QualitySetting::Ultra,
                    },
                    tilt_shift: s.tilt_shift,
                    tilt_shift_strength: s.tilt_shift_strength,
                    volumes: AudioVolumes {
                        master: s.volume_master,
                        music: s.volume_music,
                        sfx: s.volume_sfx,
                        ambience: s.volume_ambience,
                        ui: s.volume_ui,
                    },
                },
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> TileCatalog {
        TileCatalog::builtin().unwrap_or_else(|e| panic!("{e}"))
    }

    const EAST: Axial = Axial { q: 1, r: 0 };
    const WEST: Axial = Axial { q: -1, r: 0 };

    fn camera() -> CameraState {
        CameraState {
            focus: [12.5, 0.0, -3.25],
            yaw: 0.75,
            pitch: 0.9,
            distance: 180.0,
        }
    }

    fn settings() -> GameSettings {
        GameSettings {
            quality: QualitySetting::High,
            tilt_shift: true,
            tilt_shift_strength: 0.6,
            volumes: AudioVolumes {
                master: 0.8,
                music: 0.5,
                sfx: 1.0,
                ambience: 0.7,
                ui: 0.0,
            },
        }
    }

    /// Three places, a bulldoze and one undo: both history stacks are non-empty.
    fn sample(c: &TileCatalog) -> SaveData {
        let mut w = World::new(42);
        w.place(c, EAST, "road_straight", 1).unwrap();
        w.place(c, WEST, "forest", 2).unwrap();
        w.place(c, Axial::new(0, 1), "meadow", 0).unwrap();
        w.bulldoze(WEST).unwrap();
        w.undo();
        SaveData {
            world: w,
            unlocks: BTreeSet::from(["windmill".to_owned()]),
            camera: camera(),
            settings: settings(),
        }
    }

    fn wire(save: &SaveData) -> v1::Save {
        v1::Save::from_runtime(save)
    }

    fn file(wire: &v1::Save) -> Vec<u8> {
        envelope(1, &postcard::to_allocvec(wire).unwrap())
    }

    fn decode(bytes: &[u8]) -> Result<SaveData, SaveError> {
        SaveData::decode(bytes, &catalog())
    }

    #[test]
    fn round_trip_keeps_tiles_history_camera_and_settings() {
        let c = catalog();
        let save = sample(&c);
        let bytes = save.encode();
        assert_eq!(&bytes[..8], b"HEXWORLD");
        assert_eq!(&bytes[8..10], &1u16.to_le_bytes());
        let back = SaveData::decode(&bytes, &c).unwrap();
        assert_eq!(back, save);
        assert_eq!(back.encode(), bytes, "encoding is deterministic");

        // The restored history behaves exactly like the original.
        let (mut a, mut b) = (save.world.clone(), back.world.clone());
        assert!(b.can_undo() && b.can_redo());
        while a.undo() {
            assert!(b.undo());
            assert_eq!(a, b);
        }
        assert!(!b.undo());
        while a.redo() {
            assert!(b.redo());
            assert_eq!(a, b);
        }
        assert!(!b.redo());
        assert_eq!(b.tile(WEST), None, "the bulldoze was redone last");
    }

    #[test]
    fn a_fresh_world_round_trips() {
        let save = SaveData {
            world: World::new(7),
            unlocks: BTreeSet::new(),
            camera: camera(),
            settings: settings(),
        };
        assert_eq!(decode(&save.encode()), Ok(save));
    }

    /// Random legal play, including more recorded actions than [`HISTORY_DEPTH`]: every world it
    /// reaches must pass validation and round-trip.
    #[test]
    fn every_world_reached_by_play_round_trips() {
        let c = catalog();
        let mut checked_with_redo = 0;
        for seed in 0..20u64 {
            let mut rng = Rng(seed);
            let mut w = World::new(seed);
            for step in 1..=400 {
                match rng.below(10) {
                    0..=5 => {
                        let frontier = w.frontier();
                        let cell = frontier[rng.below(frontier.len())];
                        let options = w.inventory(&c, cell);
                        if !options.is_empty() {
                            let def = options[rng.below(options.len())];
                            let rotations = w.legal_rotations(&c, cell, def);
                            let k = rotations[rng.below(rotations.len())];
                            w.place(&c, cell, &def.id, k).unwrap();
                        }
                    }
                    6 => {
                        let cells: Vec<Axial> = w.tiles().keys().copied().collect();
                        let _ = w.bulldoze(cells[rng.below(cells.len())]);
                    }
                    7 | 8 => {
                        w.undo();
                    }
                    _ => {
                        w.redo();
                    }
                }
                if step % 50 == 0 {
                    checked_with_redo += usize::from(w.can_redo());
                    let save = SaveData {
                        world: w.clone(),
                        unlocks: BTreeSet::new(),
                        camera: camera(),
                        settings: settings(),
                    };
                    assert_eq!(decode(&save.encode()), Ok(save), "seed {seed} step {step}");
                }
            }
        }
        assert!(checked_with_redo > 0);
    }

    #[test]
    fn slot_meta_counts_tiles() {
        let save = sample(&catalog());
        let meta = SaveSlotMeta::new("Valley", 1_760_000_000_000, &save);
        assert_eq!(meta.tile_count, 4);
        assert_eq!(meta.format_version, SAVE_FORMAT_VERSION);
        assert_eq!(meta.name, "Valley");
    }

    #[test]
    fn envelope_errors_are_typed() {
        let good = sample(&catalog()).encode();
        assert_eq!(decode(b"PK\x03\x04 zip"), Err(SaveError::BadMagic));
        assert_eq!(decode(b"hexworld"), Err(SaveError::BadMagic));
        assert_eq!(
            decode(b""),
            Err(SaveError::Truncated {
                expected: 18,
                actual: 0
            })
        );
        assert_eq!(
            decode(b"HEXW"),
            Err(SaveError::Truncated {
                expected: 18,
                actual: 4
            })
        );
        assert_eq!(
            decode(&vec![0; MAX_SAVE_BYTES + 1]),
            Err(SaveError::TooLarge(MAX_SAVE_BYTES + 1))
        );

        let short = &good[..good.len() - 1];
        assert_eq!(
            decode(short),
            Err(SaveError::Truncated {
                expected: good.len() as u64,
                actual: short.len() as u64
            })
        );
        let mut long = good.clone();
        long.push(0);
        assert_eq!(decode(&long), Err(SaveError::TrailingBytes(1)));

        let mut flipped = good.clone();
        flipped[PREFIX_LEN + 3] ^= 0x10;
        assert!(matches!(
            decode(&flipped),
            Err(SaveError::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn versions_outside_the_supported_range_are_rejected_before_the_checksum() {
        let good = sample(&catalog()).encode();
        for v in [0u16, SAVE_FORMAT_VERSION + 1, u16::MAX] {
            let mut bytes = good.clone();
            bytes[8..10].copy_from_slice(&v.to_le_bytes());
            assert_eq!(decode(&bytes), Err(SaveError::UnsupportedVersion(v)));
        }
        assert_eq!(
            migrate(SAVE_FORMAT_VERSION + 1, &[]).unwrap_err(),
            SaveError::UnsupportedVersion(SAVE_FORMAT_VERSION + 1)
        );
    }

    #[test]
    fn bad_payloads_fail_to_decode() {
        assert!(matches!(
            decode(&envelope(1, &[0xff; 40])),
            Err(SaveError::Decode(_))
        ));
        assert!(matches!(
            decode(&envelope(1, &[])),
            Err(SaveError::Decode(_))
        ));
        let mut payload = postcard::to_allocvec(&wire(&sample(&catalog()))).unwrap();
        payload.push(0);
        assert_eq!(
            decode(&envelope(1, &payload)),
            Err(SaveError::Decode("1 unread payload bytes".into()))
        );
    }

    #[test]
    fn non_finite_and_out_of_range_values_are_rejected() {
        let c = catalog();
        let mut w = wire(&sample(&c));
        w.camera.yaw = f32::NAN;
        assert_eq!(
            decode(&file(&w)),
            Err(SaveError::InvalidValue("camera.yaw"))
        );
        let mut w = wire(&sample(&c));
        w.camera.focus[2] = f32::INFINITY;
        assert_eq!(
            decode(&file(&w)),
            Err(SaveError::InvalidValue("camera.focus.z"))
        );
        let mut w = wire(&sample(&c));
        w.settings.volume_music = 1.5;
        assert_eq!(
            decode(&file(&w)),
            Err(SaveError::InvalidValue("settings.volumes.music"))
        );
        let mut w = wire(&sample(&c));
        w.settings.volume_ui = f32::NAN;
        assert_eq!(
            decode(&file(&w)),
            Err(SaveError::InvalidValue("settings.volumes.ui"))
        );
    }

    fn tile_at(w: &mut v1::Save, q: i32, r: i32) -> &mut v1::Tile {
        w.tiles.iter_mut().find(|t| (t.q, t.r) == (q, r)).unwrap()
    }

    #[test]
    fn worlds_that_break_the_rules_are_rejected() {
        let c = catalog();
        let invalid = |w: &v1::Save| match decode(&file(w)) {
            Err(SaveError::InvalidWorld(issue)) => issue,
            other => panic!("expected InvalidWorld, got {other:?}"),
        };

        let mut w = wire(&sample(&c));
        tile_at(&mut w, 0, 1).def = "castle".into();
        assert_eq!(
            invalid(&w),
            WorldIssue::UnknownTile {
                cell: Axial::new(0, 1),
                id: "castle".into()
            }
        );

        let mut w = wire(&sample(&c));
        tile_at(&mut w, 0, 1).rotation = 6;
        assert_eq!(
            invalid(&w),
            WorldIssue::BadRotation {
                cell: Axial::new(0, 1),
                rotation: 6
            }
        );

        // Rotation 0 points the road end at the start meadow (world edge 3 of EAST).
        let mut w = wire(&sample(&c));
        tile_at(&mut w, 1, 0).rotation = 0;
        assert_eq!(
            invalid(&w),
            WorldIssue::Incompatible {
                cell: Axial::ORIGIN,
                edge: 0
            }
        );

        let mut w = wire(&sample(&c));
        let t = tile_at(&mut w, 0, 1);
        (t.q, t.r) = (5, 5);
        assert_eq!(invalid(&w), WorldIssue::Disconnected);

        let mut w = wire(&sample(&c));
        let dup = v1::Tile {
            q: 0,
            r: 1,
            level: 0,
            def: "forest".into(),
            rotation: 0,
            seed: 1,
        };
        w.tiles.push(dup);
        assert_eq!(invalid(&w), WorldIssue::DuplicateCell(Axial::new(0, 1)));

        let mut w = wire(&sample(&c));
        w.tiles.clear();
        w.undo.clear();
        w.redo.clear();
        assert_eq!(invalid(&w), WorldIssue::Empty);
    }

    #[test]
    fn reserved_levels_other_than_zero_are_rejected() {
        let c = catalog();
        assert!(wire(&sample(&c)).tiles.iter().all(|t| t.level == 0));

        let mut w = wire(&sample(&c));
        tile_at(&mut w, 0, 1).level = 1;
        assert_eq!(
            decode(&file(&w)),
            Err(SaveError::InvalidWorld(WorldIssue::LevelOutOfRange {
                cell: Axial::new(0, 1),
                level: 1
            }))
        );

        // History records reserve the same field. undo[0] is the oldest action, applied last.
        let mut w = wire(&sample(&c));
        w.undo[0].tile.level = 2;
        assert_eq!(
            decode(&file(&w)),
            Err(SaveError::InvalidHistory(HistoryIssue::InvalidState {
                stack: HistoryStack::Undo,
                index: 2,
                issue: WorldIssue::LevelOutOfRange {
                    cell: EAST,
                    level: 2
                }
            }))
        );
        let mut w = wire(&sample(&c));
        w.redo[0].tile.level = 3;
        assert_eq!(
            decode(&file(&w)),
            Err(SaveError::InvalidHistory(HistoryIssue::InvalidState {
                stack: HistoryStack::Redo,
                index: 0,
                issue: WorldIssue::LevelOutOfRange {
                    cell: WEST,
                    level: 3
                }
            }))
        );
    }

    #[test]
    fn extreme_coordinates_are_rejected_without_overflow() {
        let c = catalog();
        for q in [i32::MAX, i32::MIN, CELL_LIMIT + 1] {
            let mut w = wire(&sample(&c));
            tile_at(&mut w, 0, 1).q = q;
            assert_eq!(
                decode(&file(&w)),
                Err(SaveError::InvalidWorld(WorldIssue::OutOfRange(Axial::new(
                    q, 1
                ))))
            );
        }
        let mut w = wire(&sample(&c));
        w.redo.push(v1::HistoryAction {
            kind: v1::ActionKind::Place,
            tile: v1::Tile {
                q: 0,
                r: i32::MIN,
                level: 0,
                def: "meadow".into(),
                rotation: 0,
                seed: 0,
            },
        });
        assert_eq!(
            decode(&file(&w)),
            Err(SaveError::InvalidHistory(HistoryIssue::InvalidState {
                stack: HistoryStack::Redo,
                index: 0,
                issue: WorldIssue::OutOfRange(Axial::new(0, i32::MIN))
            }))
        );
    }

    #[test]
    fn histories_that_cannot_replay_are_rejected() {
        let c = catalog();
        let invalid = |w: &v1::Save| match decode(&file(w)) {
            Err(SaveError::InvalidHistory(issue)) => issue,
            other => panic!("expected InvalidHistory, got {other:?}"),
        };

        // The newest undo action placed (0,1); claim it placed a forest instead of a meadow.
        let mut w = wire(&sample(&c));
        w.undo.last_mut().unwrap().tile.def = "forest".into();
        assert_eq!(
            invalid(&w),
            HistoryIssue::Mismatch {
                stack: HistoryStack::Undo,
                index: 0,
                cell: Axial::new(0, 1)
            }
        );

        // Redo would bulldoze WEST, but the saved tile there differs.
        let mut w = wire(&sample(&c));
        w.redo[0].tile.rotation = 5;
        assert_eq!(
            invalid(&w),
            HistoryIssue::Mismatch {
                stack: HistoryStack::Redo,
                index: 0,
                cell: WEST
            }
        );

        // Undoing a bulldoze that never happened would overwrite the occupied origin.
        let mut w = wire(&sample(&c));
        w.undo.push(v1::HistoryAction {
            kind: v1::ActionKind::Bulldoze,
            tile: v1::Tile {
                q: 0,
                r: 0,
                level: 0,
                def: "meadow".into(),
                rotation: 0,
                seed: 0,
            },
        });
        assert_eq!(
            invalid(&w),
            HistoryIssue::Mismatch {
                stack: HistoryStack::Undo,
                index: 0,
                cell: Axial::ORIGIN
            }
        );

        // Undoing every placement of a one-tile world removes the last tile.
        let mut w = wire(&SaveData {
            world: World::new(1),
            unlocks: BTreeSet::new(),
            camera: camera(),
            settings: settings(),
        });
        w.undo.push(v1::HistoryAction {
            kind: v1::ActionKind::Place,
            tile: v1::Tile::new(Axial::ORIGIN, World::new(1).tile(Axial::ORIGIN).unwrap()),
        });
        assert_eq!(
            invalid(&w),
            HistoryIssue::InvalidState {
                stack: HistoryStack::Undo,
                index: 0,
                issue: WorldIssue::Empty
            }
        );

        // A redo that drops an unknown tile into the world.
        let mut w = wire(&sample(&c));
        w.redo.clear();
        w.redo.push(v1::HistoryAction {
            kind: v1::ActionKind::Place,
            tile: v1::Tile {
                q: 2,
                r: 0,
                level: 0,
                def: "castle".into(),
                rotation: 0,
                seed: 0,
            },
        });
        assert!(matches!(
            invalid(&w),
            HistoryIssue::InvalidState {
                stack: HistoryStack::Redo,
                index: 0,
                issue: WorldIssue::UnknownTile { .. }
            }
        ));

        let mut w = wire(&sample(&c));
        let filler = || v1::HistoryAction {
            kind: v1::ActionKind::Place,
            tile: v1::Tile {
                q: 9,
                r: 9,
                level: 0,
                def: "meadow".into(),
                rotation: 0,
                seed: 0,
            },
        };
        w.undo.splice(0..0, (0..HISTORY_DEPTH).map(|_| filler()));
        let depth = w.undo.len() + w.redo.len();
        assert!(depth > HISTORY_DEPTH);
        assert_eq!(invalid(&w), HistoryIssue::TooDeep(depth));
    }

    /// SplitMix64, so the fuzz corpus is reproducible without a `rand` dependency.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }

        fn bytes(&mut self, len: usize) -> Vec<u8> {
            (0..len).map(|_| self.next() as u8).collect()
        }
    }

    /// Outcome counts: accepted, payload decode errors, and rule (value/world/history) errors.
    #[derive(Default)]
    struct Tally {
        ok: usize,
        decode: usize,
        rules: usize,
    }

    impl Tally {
        /// Any input the decoder accepts must be a valid save that round-trips.
        fn record(&mut self, bytes: &[u8], c: &TileCatalog) {
            match SaveData::decode(bytes, c) {
                Ok(save) => {
                    assert_eq!(SaveData::decode(&save.encode(), c), Ok(save));
                    self.ok += 1;
                }
                Err(SaveError::Decode(_)) => self.decode += 1,
                Err(
                    SaveError::InvalidValue(_)
                    | SaveError::InvalidWorld(_)
                    | SaveError::InvalidHistory(_),
                ) => self.rules += 1,
                Err(e) => panic!("a valid envelope failed at the envelope: {e}"),
            }
        }
    }

    #[test]
    fn fuzz_random_bytes_never_panic() {
        let c = catalog();
        let mut rng = Rng(0x5EED_0001);
        for i in 0..3000 {
            let len = rng.below(256);
            let mut bytes = rng.bytes(len);
            if i % 2 == 0 && bytes.len() >= 8 {
                bytes[..8].copy_from_slice(&SAVE_MAGIC);
            }
            assert!(SaveData::decode(&bytes, &c).is_err());
        }
    }

    #[test]
    fn fuzz_every_truncation_and_bit_flip_fails_cleanly() {
        let c = catalog();
        let good = sample(&c).encode();
        for len in 0..good.len() {
            assert!(SaveData::decode(&good[..len], &c).is_err(), "prefix {len}");
        }
        for bit in 0..good.len() * 8 {
            let mut bytes = good.clone();
            bytes[bit / 8] ^= 1 << (bit % 8);
            assert!(SaveData::decode(&bytes, &c).is_err(), "bit {bit}");
        }
    }

    #[test]
    fn fuzz_payloads_behind_a_valid_checksum_never_panic() {
        let c = catalog();
        let payload = postcard::to_allocvec(&wire(&sample(&c))).unwrap();
        let mut rng = Rng(0x5EED_0002);
        let mut random = Tally::default();
        for _ in 0..3000 {
            let len = rng.below(payload.len() * 2);
            random.record(&envelope(1, &rng.bytes(len)), &c);
        }
        assert!(random.decode > 0);
        let mut mutated_tally = Tally::default();
        for _ in 0..3000 {
            let mut mutated = payload.clone();
            for _ in 0..1 + rng.below(4) {
                let i = rng.below(mutated.len());
                mutated[i] = rng.next() as u8;
            }
            if rng.below(4) == 0 {
                mutated.truncate(rng.below(mutated.len()));
            }
            mutated_tally.record(&envelope(1, &mutated), &c);
        }
        // The corpus reaches every stage: postcard, the rule checks, and acceptance.
        let t = &mutated_tally;
        assert!(
            t.ok > 0 && t.decode > 0 && t.rules > 0,
            "ok {} decode {} rules {}",
            t.ok,
            t.decode,
            t.rules
        );
    }
}
