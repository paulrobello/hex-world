//! The placed world: legal slots, rotation choice, edge status, placement, bulldoze and
//! undo/redo (PRD §4.1–4.2).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::axial::Axial;
use crate::catalog::{Facing, START_TILE, TileCatalog};
use crate::footprint;
use crate::socket::{HeightMeet, SocketKind};
use crate::tile::{EdgePieces, KitRef, TileDef, local_edge, opposite_edge};
use crate::tunnel;

/// Undo depth (PRD §4.2 requires at least 100). Older actions are dropped first.
pub const HISTORY_DEPTH: usize = 128;

/// Highest level a tile may stand on (P4). A placed tile's `level + def.top()` never exceeds it.
pub const MAX_LEVEL: u8 = 3;

/// One placed tile: its definition id, rotation `k` (local edge `e` faces world edge `e + k`), its
/// level (`0..=MAX_LEVEL`, one `LEVEL_STEP` each) and the seed its scatter and agents derive from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacedTile {
    pub def: String,
    pub rotation: u8,
    #[serde(default)]
    pub level: u8,
    pub seed: u64,
}

/// How a tile is put down: its rotation `0..6` and its level.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct Orientation {
    pub rotation: u8,
    pub level: u8,
}

impl Orientation {
    pub const fn new(rotation: u8, level: u8) -> Self {
        Self { rotation, level }
    }
}

/// [`World::placement_key`]: (cliff faces, cliff drop, level rank, rotation).
type PlacementKey = (u8, u16, u8, u8);

/// Whether `def` may stand at `level`: level 0, or a raisable tile whose top stays within
/// [`MAX_LEVEL`].
pub fn level_allowed(def: &TileDef, level: u8) -> bool {
    level == 0 || (def.raisable && level.saturating_add(def.top()) <= MAX_LEVEL)
}

/// Ghost-preview glow for one edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeStatus {
    Compatible,
    Incompatible,
    /// No placed neighbor across this edge.
    Neutral,
}

/// One outward face of a ghosted tile: the ghost's own `cell`, its world edge `w`, the glow
/// against the occupant across it, and whether that seam is an accepted cliff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Face {
    pub cell: Axial,
    pub w: u8,
    pub status: EdgeStatus,
    pub cliff: bool,
}

/// Direction for cycling through distinct legal tile orientations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RotateDir {
    Forward,
    Back,
}

/// A placement refusal. Failed placements leave the world and history unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaceError {
    /// A cell the tile would cover already holds a tile.
    Occupied,
    /// The empty target cell touches no placed tile.
    NotAdjacent,
    /// The requested definition id is absent from the catalog.
    UnknownTile(String),
    /// The requested rotation is outside `0..6`.
    BadRotation(u8),
    /// The tile may not stand at this level ([`level_allowed`]).
    LevelOutOfRange(u8),
    /// World edges whose neighbor socket does not fit.
    Incompatible(Vec<u8>),
}

impl fmt::Display for PlaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlaceError::Occupied => write!(f, "cell is occupied"),
            PlaceError::NotAdjacent => write!(f, "cell is not next to the world"),
            PlaceError::UnknownTile(id) => write!(f, "unknown tile {id:?}"),
            PlaceError::BadRotation(k) => write!(f, "rotation {k} is not 0..6"),
            PlaceError::LevelOutOfRange(l) => write!(f, "this tile cannot stand at level {l}"),
            PlaceError::Incompatible(edges) => write!(f, "edges {edges:?} do not fit"),
        }
    }
}

impl std::error::Error for PlaceError {}

/// Why a tile has no legal orientation at a slot (ENH-002): a closed set that crosses the bridge
/// as a string code (`play.fits` entries' `refusal`, contract-tested through `enums.refusal` in
/// bridge-schema.json) and is worded in web/ui/src/words.ts. The wording stays out of the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Refusal {
    /// The cell is inside the world: only a footprint tile, standing on a covering legal centre,
    /// can be here.
    InnerSlot,
    /// No edge of the tile meets the slot's need `kind`.
    Missing { kind: SocketKind },
    /// A deck end faces an empty cell: the deck spans to the rail it meets.
    DeckEnd,
    /// The tile's `mine` kind meets the placed `theirs` kind across an edge.
    Clash {
        mine: SocketKind,
        theirs: SocketKind,
    },
    /// A footprint tile's outer edges clash at every free centre beside the slot.
    FootprintEdges,
    /// A footprint tile has no seven free cells beside the world.
    NoFootprintRoom,
}

/// The lower-case word for a socket kind in a [`Refusal`] code.
fn kind_word(kind: SocketKind) -> &'static str {
    match kind {
        SocketKind::Grass => "grass",
        SocketKind::Forest => "forest",
        SocketKind::Road => "road",
        SocketKind::River => "river",
        SocketKind::Beach => "beach",
        SocketKind::Ocean => "ocean",
        SocketKind::Rail => "rail",
    }
}

impl Refusal {
    /// The code the bridge publishes, kind pairs in kind-word order.
    pub fn code(&self) -> String {
        match self {
            Refusal::InnerSlot => "inner_slot".into(),
            Refusal::Missing { kind } => format!("missing_{}", kind_word(*kind)),
            Refusal::DeckEnd => "deck_end".into(),
            Refusal::Clash { mine, theirs } => {
                format!("clash_{}_{}", kind_word(*mine), kind_word(*theirs))
            }
            Refusal::FootprintEdges => "footprint_edges".into(),
            Refusal::NoFootprintRoom => "no_footprint_room".into(),
        }
    }

    /// Every code the placement check can return, in the order bridge-schema.json's
    /// `enums.refusal` lists them.
    pub fn codes() -> Vec<String> {
        let mut out: Vec<String> = [
            "inner_slot",
            "deck_end",
            "footprint_edges",
            "no_footprint_room",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        for kind in SocketKind::ALL {
            out.push(format!("missing_{}", kind_word(kind)));
        }
        for mine in SocketKind::ALL {
            for theirs in SocketKind::ALL {
                out.push(format!("clash_{}_{}", kind_word(mine), kind_word(theirs)));
            }
        }
        out
    }
}

impl serde::Serialize for Refusal {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.code())
    }
}

/// A removal refusal: the remaining world must stay nonempty and connected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BulldozeError {
    /// The target cell holds no tile.
    Empty,
    /// Removing the only tile would leave the world empty.
    LastTile,
    /// Removing this tile would split the remaining tiles into disconnected groups.
    WouldDisconnect,
    /// This meadow covers a tunnel; remove a portal first.
    TunnelCover,
}

impl fmt::Display for BulldozeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BulldozeError::Empty => write!(f, "no tile here"),
            BulldozeError::LastTile => write!(f, "the last tile cannot be removed"),
            BulldozeError::WouldDisconnect => write!(f, "removing this tile splits the world"),
            BulldozeError::TunnelCover => write!(f, "this tile covers a tunnel"),
        }
    }
}

impl std::error::Error for BulldozeError {}

/// A replacement refusal. Failed or unchanged replacements record no history entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplaceError {
    /// The target cell holds no tile.
    Empty,
    /// The requested definition id is absent from the catalog.
    UnknownTile(String),
    /// The requested rotation is outside `0..6`.
    BadRotation(u8),
    /// The tile may not stand at this level ([`level_allowed`]).
    LevelOutOfRange(u8),
    /// World edges whose neighbor socket does not fit.
    Incompatible(Vec<u8>),
    /// The same tile in the same orientation at the same level is already there, so nothing would
    /// change.
    Unchanged,
    /// This meadow covers a tunnel; remove a portal first.
    TunnelCover,
    /// A footprint tile cannot take the place of a single tile, or the reverse.
    FootprintMismatch,
}

impl fmt::Display for ReplaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReplaceError::Empty => write!(f, "no tile here"),
            ReplaceError::UnknownTile(id) => write!(f, "unknown tile {id:?}"),
            ReplaceError::BadRotation(k) => write!(f, "rotation {k} is not 0..6"),
            ReplaceError::LevelOutOfRange(l) => write!(f, "this tile cannot stand at level {l}"),
            ReplaceError::Incompatible(edges) => write!(f, "edges {edges:?} do not fit"),
            ReplaceError::Unchanged => write!(f, "that tile is already here"),
            ReplaceError::TunnelCover => write!(f, "this tile covers a tunnel"),
            ReplaceError::FootprintMismatch => {
                write!(f, "a mega tile and a single tile cannot replace each other")
            }
        }
    }
}

impl std::error::Error for ReplaceError {}

/// A reversible world change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Place {
        cell: Axial,
        tile: PlacedTile,
    },
    Bulldoze {
        cell: Axial,
        tile: PlacedTile,
    },
    /// The tile at `cell` swapped in one step: `from` was there, `to` took its place.
    Replace {
        cell: Axial,
        from: PlacedTile,
        to: PlacedTile,
    },
}

impl Action {
    /// The cell the action changes.
    pub fn cell(&self) -> Axial {
        match self {
            Action::Place { cell, .. }
            | Action::Bulldoze { cell, .. }
            | Action::Replace { cell, .. } => *cell,
        }
    }
}

/// LIFO undo and redo stacks. Part of the save (PRD §4.2).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct History {
    undo: VecDeque<Action>,
    redo: Vec<Action>,
}

/// The tile map. Cells live in an ordered map so iteration and serialized saves are deterministic.
/// A mega tile is held once, at its anchor (centre) in `tiles`; `members` maps each of its ring
/// cells to that anchor and is derived from `tiles` and the catalog.
///
/// `members` is not serialized, and deserializing a `World` directly leaves it empty: a world
/// holding a mega tile would then read its ring cells as free. Only [`crate::SaveData::decode`],
/// which rebuilds `members` against the catalog, yields a valid world from bytes. It is the only
/// deserialize path in the workspace; the RON round trip in this module's tests holds single
/// tiles only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    seed: u64,
    tiles: BTreeMap<Axial, PlacedTile>,
    history: History,
    #[serde(skip)]
    members: BTreeMap<Axial, Axial>,
    #[serde(skip)]
    stamp: Stamp,
}

static NEXT_STAMP: AtomicU64 = AtomicU64::new(1);

/// A world's [`World::stamp`]. It takes no part in equality or the save.
#[derive(Clone, Debug)]
struct Stamp(u64);

impl Stamp {
    fn fresh() -> Self {
        Self(NEXT_STAMP.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for Stamp {
    fn default() -> Self {
        Self::fresh()
    }
}

impl PartialEq for Stamp {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for Stamp {}

impl World {
    /// A world holding only the start tile at the origin (A1). The start tile is not undoable.
    pub fn new(seed: u64) -> Self {
        let mut tiles = BTreeMap::new();
        tiles.insert(
            Axial::ORIGIN,
            PlacedTile {
                def: START_TILE.to_owned(),
                rotation: 0,
                level: 0,
                seed: cell_seed(seed, Axial::ORIGIN),
            },
        );
        Self {
            seed,
            tiles,
            history: History::default(),
            members: BTreeMap::new(),
            stamp: Stamp::fresh(),
        }
    }

    /// Rebuilds a world from saved parts. No rule checks run here: [`crate::save`] validates the
    /// result against the catalog before handing it out. Exposed (hidden from docs) for the
    /// integration tests that rebuild translated worlds the decoder accepts
    /// (`tests/common/worlds.rs`, `hex-sim`'s near-limit test); play never calls it.
    #[doc(hidden)]
    pub fn from_parts(
        seed: u64,
        tiles: BTreeMap<Axial, PlacedTile>,
        undo: VecDeque<Action>,
        redo: Vec<Action>,
    ) -> Self {
        Self {
            seed,
            tiles,
            history: History { undo, redo },
            members: BTreeMap::new(),
            stamp: Stamp::fresh(),
        }
    }

    /// The undo stack (oldest first) and the redo stack (the next redo last).
    pub(crate) fn history_stacks(&self) -> (&VecDeque<Action>, &[Action]) {
        (&self.history.undo, &self.history.redo)
    }

    /// Identifies this state of the placed tiles across every world in the process: each world
    /// takes a new stamp when it is built and whenever its tiles change, and a clone keeps its
    /// original's until either changes. So two equal stamps mean equal tiles, and a cache of
    /// anything derived from the tiles may key on it.
    pub fn stamp(&self) -> u64 {
        self.stamp.0
    }

    /// The world seed from which placed tiles derive deterministic seeds.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Placed tiles ordered by axial coordinate.
    pub fn tiles(&self) -> &BTreeMap<Axial, PlacedTile> {
        &self.tiles
    }

    /// Ring cells of placed mega tiles, each mapped to its anchor.
    pub fn members(&self) -> &BTreeMap<Axial, Axial> {
        &self.members
    }

    /// Recomputes `members` from the placed tiles' definitions.
    pub(crate) fn rebuild_members(&mut self, catalog: &TileCatalog) {
        self.members = self
            .tiles
            .iter()
            .filter(|(_, t)| catalog.get(&t.def).is_some_and(TileDef::is_mega))
            .flat_map(|(&a, _)| {
                footprint::members(a)
                    .into_iter()
                    .skip(1)
                    .map(move |m| (m, a))
            })
            .collect();
        self.stamp = Stamp::fresh();
    }

    /// The anchor of the tile covering `cell`: the cell itself for an anchor, its anchor for a
    /// member, otherwise `None`.
    pub fn anchor_of(&self, cell: Axial) -> Option<Axial> {
        anchor_in(&self.tiles, &self.members, cell)
    }

    /// Whether a tile covers `cell`.
    pub fn occupied(&self, cell: Axial) -> bool {
        self.anchor_of(cell).is_some()
    }

    /// The tile covering `cell` (a member answers with its anchor's tile), or `None` when the cell
    /// is empty.
    pub fn tile(&self, cell: Axial) -> Option<&PlacedTile> {
        self.tiles.get(&self.anchor_of(cell)?)
    }

    /// Free cells adjacent to at least one occupied cell, sorted.
    pub fn frontier(&self) -> Vec<Axial> {
        let free: BTreeSet<Axial> = self
            .tiles
            .keys()
            .chain(self.members.keys())
            .flat_map(|c| (0..6).map(move |e| c.neighbor(e)))
            .filter(|&n| !self.occupied(n))
            .collect();
        free.into_iter().collect()
    }

    /// Empty and adjacent to the world.
    pub fn is_slot(&self, cell: Axial) -> bool {
        !self.occupied(cell) && (0..6).any(|e| self.occupied(cell.neighbor(e)))
    }

    /// Whether `def` could stand with its anchor at `cell`: a slot for a single tile; for a mega
    /// tile, all seven cells empty and at least one cell around the footprint occupied.
    pub fn is_slot_for(&self, cell: Axial, def: &TileDef) -> bool {
        if !def.is_mega() {
            return self.is_slot(cell);
        }
        footprint::members(cell).iter().all(|&m| !self.occupied(m))
            && footprint::outer_neighbours(cell)
                .iter()
                .any(|&n| self.occupied(n))
    }

    /// The occupant of `cell` seen across its world edge `w`: the tile, the socket it shows there
    /// and its level. `None` when the cell is empty, its definition is not in the catalog, or the
    /// edge is interior to a footprint.
    pub fn facing<'c>(&self, catalog: &'c TileCatalog, cell: Axial, w: u8) -> Option<Facing<'c>> {
        facing_in(&self.tiles, &self.members, catalog, cell, w)
    }

    /// Every outward face of `def` ghosted with its anchor at `cell` in orientation `o` (6 for a
    /// single tile, in world edge order; 18 for a mega tile, in local outer-index order), against
    /// the occupant across it. An occupant whose definition is missing from the catalog counts as
    /// incompatible.
    pub fn faces(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
        o: Orientation,
    ) -> Vec<Face> {
        let mut out = vec![];
        each_face(cell, def, o, |m, w, mine| {
            out.push(self.face(catalog, m, w, mine, true));
            true
        });
        out
    }

    /// The face `mine` shows across world edge `w` of `m` against the occupant there; its `cliff`
    /// is worked out only when `cliff` asks for it (else `false`).
    fn face(&self, catalog: &TileCatalog, m: Axial, w: u8, mine: Facing, cliff: bool) -> Face {
        let across = m.neighbor(w);
        let (status, cliff) = if !self.occupied(across) {
            (EdgeStatus::Neutral, false)
        } else {
            match self.facing(catalog, across, opposite_edge(w)) {
                None => (EdgeStatus::Incompatible, false),
                Some(theirs) => {
                    let status = if catalog.facing_meets(mine, theirs) {
                        EdgeStatus::Compatible
                    } else {
                        EdgeStatus::Incompatible
                    };
                    let cliff = cliff
                        && match mine.meet(theirs) {
                            HeightMeet::Seamless => false,
                            HeightMeet::Cliff | HeightMeet::Refused => {
                                catalog.facings_meet(mine, theirs)
                            }
                        };
                    (status, cliff)
                }
            }
        };
        Face {
            cell: m,
            w,
            status,
            cliff,
        }
    }

    /// The cliffs `def` at `cell` in `o` makes: how many faces meet their occupant as a cliff, and
    /// the summed corner drop across them.
    fn cliffs(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
        o: Orientation,
    ) -> (u8, u16) {
        let (mut n, mut drop) = (0, 0);
        each_face(cell, def, o, |m, w, mine| {
            if self.face(catalog, m, w, mine, true).cliff {
                n += 1;
                if let Some(theirs) = self.facing(catalog, m.neighbor(w), opposite_edge(w)) {
                    let abs = |f: Facing, c: u8| i32::from(c) + i32::from(f.level);
                    let ((a0, a1), (b0, b1)) = (mine.socket.corners(), theirs.socket.corners());
                    let d = (abs(mine, a0) - abs(theirs, b1)).abs()
                        + (abs(mine, a1) - abs(theirs, b0)).abs();
                    drop += u16::try_from(d).unwrap_or(u16::MAX);
                }
            }
            true
        });
        (n, drop)
    }

    /// Placement preference for `o` (smaller first): fewest cliff faces, then the smallest drop
    /// across them; among cliff-free orientations the lowest level, among cliffed ones the highest
    /// (owner O1, 2026-11-13: a tile in a mixed-level slot rises to the side it shares the most
    /// edges with); then rotation.
    fn placement_key(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
        o: Orientation,
    ) -> PlacementKey {
        let (cliffs, drop) = self.cliffs(catalog, cell, def, o);
        let level = if cliffs == 0 {
            o.level
        } else {
            MAX_LEVEL - o.level
        };
        (cliffs, drop, level, o.rotation)
    }

    /// Glow for each world edge of `def` ghosted at `cell` in orientation `o` ([`World::faces`]
    /// at `cell`). A neighbor whose definition is missing from the catalog counts as incompatible.
    /// A mega def has no face at its anchor, so every entry is `Neutral`: read [`World::faces`].
    pub fn edge_status(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
        o: Orientation,
    ) -> [EdgeStatus; 6] {
        let mut out = [EdgeStatus::Neutral; 6];
        for f in self.faces(catalog, cell, def, o) {
            if f.cell == cell {
                out[usize::from(f.w)] = f.status;
            }
        }
        out
    }

    /// World edges of `def` at `cell` in orientation `o` that meet a placed neighbour as a cliff:
    /// the pair is accepted ([`TileCatalog::facings_meet`]) but its corners differ in absolute
    /// height. A stopped rail end beside a raised tile counts: the stop meets it as lane-free
    /// ground, so the step is a cliff face. A mega def has no face at its anchor, so every entry is
    /// `false`: read [`World::faces`].
    pub fn cliff_edges(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
        o: Orientation,
    ) -> [bool; 6] {
        let mut out = [false; 6];
        for f in self.faces(catalog, cell, def, o) {
            if f.cell == cell {
                out[usize::from(f.w)] = f.cliff;
            }
        }
        out
    }

    /// Whether `def` may be placed at `cell` in orientation `o`.
    pub fn fits(&self, catalog: &TileCatalog, cell: Axial, def: &TileDef, o: Orientation) -> bool {
        o.rotation < 6
            && level_allowed(def, o.level)
            && self.is_slot_for(cell, def)
            && each_face(cell, def, o, |m, w, mine| {
                let status = self.face(catalog, m, w, mine, false).status;
                // A viaduct's deck ends span between two tiles, so a deck edge needs its rail.
                status != EdgeStatus::Incompatible
                    && (mine.socket.deck() == 0 || status == EdgeStatus::Compatible)
            })
    }

    /// The orientations `test` accepts in [`World::placement_key`] order, so placement never
    /// defaults to a cliff it can avoid. A tile that is not raisable stands at level 0 only.
    fn ordered_orientations(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
        test: impl Fn(Orientation) -> bool,
    ) -> Vec<Orientation> {
        self.ordered_with_cliff(catalog, cell, def, test)
            .into_iter()
            .map(|(_, o)| o)
            .collect()
    }

    /// [`World::ordered_orientations`] with each orientation's [`World::placement_key`],
    /// computed once per orientation.
    fn ordered_with_cliff(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
        test: impl Fn(Orientation) -> bool,
    ) -> Vec<(PlacementKey, Orientation)> {
        let mut out: Vec<(PlacementKey, Orientation)> = (0..=MAX_LEVEL)
            .filter(|&l| level_allowed(def, l))
            .flat_map(|l| (0..6).map(move |k| Orientation::new(k, l)))
            .filter(|&o| test(o))
            .map(|o| (self.placement_key(catalog, cell, def, o), o))
            .collect();
        out.sort_by_key(|&(key, _)| key);
        out
    }

    /// Orientations in which `def` fits with its anchor at `cell`, in `World::placement_key`
    /// order. Empty when `cell` is not a slot for `def` ([`World::is_slot_for`]).
    pub fn legal_orientations(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
    ) -> Vec<Orientation> {
        if !self.is_slot_for(cell, def) {
            return vec![];
        }
        self.ordered_orientations(catalog, cell, def, |o| self.fits(catalog, cell, def, o))
    }

    /// Tile types with at least one legal orientation at `cell`, in catalog order.
    pub fn inventory<'a>(&self, catalog: &'a TileCatalog, cell: Axial) -> Vec<&'a TileDef> {
        catalog
            .tiles()
            .iter()
            .filter(|def| {
                (0..=MAX_LEVEL)
                    .any(|l| (0..6).any(|k| self.fits(catalog, cell, def, Orientation::new(k, l))))
            })
            .collect()
    }

    /// The world edges where `def` ghosted at `cell` in `o` fails the placement check
    /// ([`World::fits`]): an edge whose neighbour socket does not fit, or a deck end with nothing
    /// placed across it (a viaduct's deck spans to the rail it meets). The sheet's `clashes`.
    fn clash_edges(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
        o: Orientation,
    ) -> Vec<u8> {
        let mut out = vec![];
        each_face(cell, def, o, |m, w, mine| {
            let status = self.face(catalog, m, w, mine, false).status;
            if status == EdgeStatus::Incompatible
                || (mine.socket.deck() > 0 && status != EdgeStatus::Compatible)
            {
                out.push(w);
            }
            true
        });
        out
    }

    /// How many world edges [`World::clash_edges`] finds (fewest wins, the sheet's `bestK`).
    pub fn clash_count(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
        o: Orientation,
    ) -> usize {
        self.clash_edges(catalog, cell, def, o).len()
    }

    /// Why `def` has no legal orientation with its anchor at `cell` ([`World::legal_orientations`]
    /// is empty), or `None` when it fits: the codes `play.fits` publishes for the sheet's refusal
    /// wording (ENH-002). Classification first found in the sheet's mirror, kept in step:
    /// a need no edge of the tile meets takes precedence over what the least-clashing rotation's
    /// first clashing edge meets.
    pub fn placement_refusal(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
    ) -> Option<Refusal> {
        if !self.legal_orientations(catalog, cell, def).is_empty() {
            return None;
        }
        if !self.is_slot_for(cell, def) {
            return Some(if def.is_mega() {
                Refusal::NoFootprintRoom
            } else {
                Refusal::InnerSlot
            });
        }
        if def.is_mega() {
            // Room at this centre, but every orientation clashes on an outer edge.
            return Some(Refusal::FootprintEdges);
        }
        self.socket_refusal(catalog, cell, def)
    }

    /// Why `def` cannot meet the sockets around `cell` with any level-0 rotation, at a cell that
    /// could host the tile — a need no edge of the tile meets takes precedence over what the
    /// least-clashing rotation's first clashing edge meets (ENH-002: the sheet mirror's
    /// classification, kept in step when the mirror retired). `None` when every edge meets, which
    /// [`World::placement_refusal`] calls only for a tile with no legal orientation.
    pub fn socket_refusal(&self, catalog: &TileCatalog, cell: Axial, def: &TileDef) -> Option<Refusal> {
        // The least-clashing rotation at level 0, the first on ties.
        let best = (0..6)
            .map(|k| (self.clash_count(catalog, cell, def, Orientation::new(k, 0)), k))
            .min()
            .unwrap_or((0, 0))
            .1;
        // The first need kind in edge order that no edge of `def` meets.
        let mut seen: Vec<SocketKind> = vec![];
        for w in 0..6 {
            let kind = self
                .facing(catalog, cell.neighbor(w), opposite_edge(w))
                .map(|f| f.socket.kind);
            if let Some(kind) = kind
                && !seen.contains(&kind)
            {
                seen.push(kind);
                if !def
                    .edges
                    .iter()
                    .any(|e| catalog.sockets().kinds_compatible(kind, e.kind))
                {
                    return Some(Refusal::Missing { kind });
                }
            }
        }
        let w = self
            .clash_edges(catalog, cell, def, Orientation::new(best, 0))
            .into_iter()
            .next()?;
        match self.facing(catalog, cell.neighbor(w), opposite_edge(w)) {
            // Only a deck end clashes with nothing placed across it.
            None => Some(Refusal::DeckEnd),
            Some(theirs) => {
                let mine = Facing::single(def, best, 0, w);
                Some(Refusal::Clash {
                    mine: mine.socket.kind,
                    theirs: theirs.socket.kind,
                })
            }
        }
    }

    /// Every legal centre for the mega tile `def`, sorted, with its legal orientations
    /// ([`World::legal_orientations`]); empty for a single tile. A centre lies two steps from the
    /// world.
    pub fn mega_slots(
        &self,
        catalog: &TileCatalog,
        def: &TileDef,
    ) -> Vec<(Axial, Vec<Orientation>)> {
        let candidates: BTreeSet<Axial> = if def.is_mega() {
            self.tiles
                .keys()
                .chain(self.members.keys())
                .flat_map(|&c| footprint::outer_neighbours(c))
                .filter(|&a| self.is_slot_for(a, def))
                .collect()
        } else {
            BTreeSet::new()
        };
        candidates
            .into_iter()
            .map(|a| (a, self.legal_orientations(catalog, a, def)))
            .filter(|(_, legal)| !legal.is_empty())
            .collect()
    }

    /// The centre a footprint `def` placed for slot `cell` takes: `cell` itself when it is a legal
    /// centre, else the first legal centre among `cell.neighbor(0..6)` in edge order.
    pub fn mega_centre_for(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
    ) -> Option<Axial> {
        std::iter::once(cell)
            .chain((0..6).map(|e| cell.neighbor(e)))
            .find(|&a| !self.legal_orientations(catalog, a, def).is_empty())
    }

    /// The orientation the ghost snaps to when `cell` is hovered: the first legal one.
    pub fn auto_orient(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
    ) -> Option<Orientation> {
        self.legal_orientations(catalog, cell, def).first().copied()
    }

    /// What R steps through at `cell` for a picker entry (`members`, a group in rotation order or one
    /// lone tile): each member's distinct legal orientations (orientations at one level whose
    /// rotations give every world edge an equal socket count once, PRD §4.7 Q6), across the whole
    /// entry in `World::placement_key` order with member after member before rotation (owner, 2026-10-20: the
    /// Beach Curve's headland orientations, then its bay ones), then by rotation.
    pub fn group_orientations<'a>(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        members: &[&'a TileDef],
    ) -> Vec<(&'a TileDef, Orientation)> {
        let mut seq: Vec<(&'a TileDef, Orientation)> = vec![];
        let mut keys: Vec<(u8, u16, u8, usize, u8)> = vec![];
        for (i, &d) in members.iter().enumerate() {
            let mut reps: Vec<(PlacementKey, Orientation)> = vec![];
            let legal = if self.is_slot_for(cell, d) {
                self.ordered_with_cliff(catalog, cell, d, |o| self.fits(catalog, cell, d, o))
            } else {
                vec![]
            };
            for (key, o) in legal {
                if !reps
                    .iter()
                    .any(|(_, r)| r.level == o.level && d.same_orientation(r.rotation, o.rotation))
                {
                    reps.push((key, o));
                }
            }
            for ((cliffs, drop, level, rotation), o) in reps {
                seq.push((d, o));
                keys.push((cliffs, drop, level, i, rotation));
            }
        }
        let mut order: Vec<usize> = (0..seq.len()).collect();
        order.sort_by_key(|&j| keys[j]);
        order.into_iter().map(|j| seq[j]).collect()
    }

    /// The next entry of [`World::group_orientations`] after `(current tile, current orientation)`
    /// in `dir`, wrapping, so every step changes what the ghost shows and a step may switch to
    /// another member. From an illegal `current` it is the current tile's next legal orientation in
    /// the same order (wrapping among that tile's own), else the first (or, backwards, the last)
    /// entry. Returns the current orientation's own entry when it is the only one, and `None` when
    /// no member fits.
    pub fn cycle_group<'a>(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        members: &[&'a TileDef],
        current: (&str, Orientation),
        dir: RotateDir,
    ) -> Option<(&'a TileDef, Orientation)> {
        let seq = self.group_orientations(catalog, cell, members);
        let n = seq.len();
        if n == 0 {
            return None;
        }
        let (id, o) = current;
        let o = Orientation::new(o.rotation % 6, o.level);
        if let Some(i) = seq.iter().position(|(d, r)| {
            d.id == id && r.level == o.level && d.same_orientation(r.rotation, o.rotation)
        }) {
            return Some(match dir {
                RotateDir::Forward => seq[(i + 1) % n],
                RotateDir::Back => seq[(i + n - 1) % n],
            });
        }
        let own = members.iter().find(|d| d.id == id).and_then(|&d| {
            let key = |o: Orientation| self.placement_key(catalog, cell, d, o);
            let at = key(o);
            let mine: Vec<_> = seq
                .iter()
                .copied()
                .filter(|(m, _)| m.id == id)
                .map(|e| (key(e.1), e))
                .collect();
            match dir {
                RotateDir::Forward => mine
                    .iter()
                    .find(|(k, _)| *k > at)
                    .or(mine.first())
                    .map(|&(_, e)| e),
                RotateDir::Back => mine
                    .iter()
                    .rev()
                    .find(|(k, _)| *k < at)
                    .or(mine.last())
                    .map(|&(_, e)| e),
            }
        });
        own.or(Some(match dir {
            RotateDir::Forward => seq[0],
            RotateDir::Back => seq[n - 1],
        }))
    }

    /// World edges of `def` at `cell` in orientation `o` whose neighbor does not fit, ascending
    /// and once each (a mega tile's 18 faces share six directions).
    fn clashes(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
        o: Orientation,
    ) -> Vec<u8> {
        let bad: BTreeSet<u8> = self
            .faces(catalog, cell, def, o)
            .iter()
            .filter(|f| f.status == EdgeStatus::Incompatible)
            .map(|f| f.w)
            .collect();
        bad.into_iter().collect()
    }

    /// Places `def_id` with its anchor at `cell` in orientation `o` if every cell it covers is
    /// empty and every touching edge is compatible (PRD §4.1 step 4). A mega tile records one
    /// action at its anchor.
    pub fn place(
        &mut self,
        catalog: &TileCatalog,
        cell: Axial,
        def_id: &str,
        o: Orientation,
    ) -> Result<(), PlaceError> {
        let def = catalog
            .get(def_id)
            .ok_or_else(|| PlaceError::UnknownTile(def_id.to_owned()))?;
        let covers: &[Axial] = if def.is_mega() {
            &footprint::members(cell)
        } else {
            &[cell]
        };
        if covers.iter().any(|&m| self.occupied(m)) {
            return Err(PlaceError::Occupied);
        }
        if !self.is_slot_for(cell, def) {
            return Err(PlaceError::NotAdjacent);
        }
        if o.rotation >= 6 {
            return Err(PlaceError::BadRotation(o.rotation));
        }
        if !level_allowed(def, o.level) {
            return Err(PlaceError::LevelOutOfRange(o.level));
        }
        let bad = self.clashes(catalog, cell, def, o);
        if !bad.is_empty() {
            return Err(PlaceError::Incompatible(bad));
        }
        let tile = PlacedTile {
            def: def_id.to_owned(),
            rotation: o.rotation,
            level: o.level,
            seed: cell_seed(self.seed, cell),
        };
        self.tiles.insert(cell, tile.clone());
        self.stamp = Stamp::fresh();
        if def.is_mega() {
            self.rebuild_members(catalog);
        }
        self.record(Action::Place { cell, tile });
        Ok(())
    }

    /// Whether the tile anchored at `anchor` is a mega tile by its catalog definition.
    fn holds_mega(&self, catalog: &TileCatalog, anchor: Axial) -> bool {
        self.tiles
            .get(&anchor)
            .and_then(|t| catalog.get(&t.def))
            .is_some_and(TileDef::is_mega)
    }

    /// Orientations in which `def` may take the place of the tile covering `cell` (resolved to its
    /// anchor), ordered as [`World::legal_orientations`]: every touching edge fits its neighbor.
    /// Empty when `cell` holds no tile or only one of the two is a mega tile. Swapping a tile keeps
    /// the world connected, so only the edges are checked.
    pub fn replace_orientations(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
    ) -> Vec<Orientation> {
        let Some(cell) = self.anchor_of(cell) else {
            return vec![];
        };
        if self.holds_mega(catalog, cell) != def.is_mega() {
            return vec![];
        }
        self.ordered_orientations(catalog, cell, def, |o| {
            self.clashes(catalog, cell, def, o).is_empty()
        })
    }

    /// Swaps the tile covering `cell` (resolved to its anchor) for `def_id` in orientation `o` as
    /// one undoable action at the anchor. The same tile in the same orientation (equal sockets on
    /// every edge) at the same level is [`ReplaceError::Unchanged`] and records nothing; a mega
    /// tile for a single one, or the reverse, is [`ReplaceError::FootprintMismatch`]. Returns the
    /// tile that was there.
    pub fn replace(
        &mut self,
        catalog: &TileCatalog,
        cell: Axial,
        def_id: &str,
        o: Orientation,
    ) -> Result<PlacedTile, ReplaceError> {
        let def = catalog
            .get(def_id)
            .ok_or_else(|| ReplaceError::UnknownTile(def_id.to_owned()))?;
        let Some(cell) = self.anchor_of(cell) else {
            return Err(ReplaceError::Empty);
        };
        let from = self.tiles[&cell].clone();
        if self.holds_mega(catalog, cell) != def.is_mega() {
            return Err(ReplaceError::FootprintMismatch);
        }
        if tunnel::is_cover(self, catalog, cell) {
            return Err(ReplaceError::TunnelCover);
        }
        if o.rotation >= 6 {
            return Err(ReplaceError::BadRotation(o.rotation));
        }
        if !level_allowed(def, o.level) {
            return Err(ReplaceError::LevelOutOfRange(o.level));
        }
        if from.def == def_id
            && from.level == o.level
            && def.same_orientation(from.rotation, o.rotation)
        {
            return Err(ReplaceError::Unchanged);
        }
        let bad = self.clashes(catalog, cell, def, o);
        if !bad.is_empty() {
            return Err(ReplaceError::Incompatible(bad));
        }
        let to = PlacedTile {
            def: def_id.to_owned(),
            rotation: o.rotation,
            level: o.level,
            seed: cell_seed(self.seed, cell),
        };
        self.tiles.insert(cell, to.clone());
        self.stamp = Stamp::fresh();
        self.record(Action::Replace {
            cell,
            from: from.clone(),
            to,
        });
        Ok(from)
    }

    /// Whether the tile covering `cell` (resolved to its anchor) may be removed: not the last
    /// tile, and the rest stays connected.
    pub fn can_bulldoze(&self, catalog: &TileCatalog, cell: Axial) -> Result<(), BulldozeError> {
        let Some(cell) = self.anchor_of(cell) else {
            return Err(BulldozeError::Empty);
        };
        if tunnel::is_cover(self, catalog, cell) {
            return Err(BulldozeError::TunnelCover);
        }
        if self.tiles.len() == 1 {
            return Err(BulldozeError::LastTile);
        }
        if !self.connected_without(cell) {
            return Err(BulldozeError::WouldDisconnect);
        }
        Ok(())
    }

    /// Removes and returns the tile covering `cell`, its whole footprint at once, as one undoable
    /// action at its anchor, preserving a nonempty connected world.
    pub fn bulldoze(
        &mut self,
        catalog: &TileCatalog,
        cell: Axial,
    ) -> Result<PlacedTile, BulldozeError> {
        self.can_bulldoze(catalog, cell)?;
        let cell = self.anchor_of(cell).expect("can_bulldoze checked the cell");
        let tile = self.lift(cell).expect("can_bulldoze checked the cell");
        self.record(Action::Bulldoze {
            cell,
            tile: tile.clone(),
        });
        Ok(tile)
    }

    /// Whether an action remains in the bounded undo history.
    pub fn can_undo(&self) -> bool {
        !self.history.undo.is_empty()
    }

    /// Whether an undone action can be reapplied; a new edit clears this stack.
    pub fn can_redo(&self) -> bool {
        !self.history.redo.is_empty()
    }

    /// Reverts the latest action. LIFO order always lands on an earlier valid world, so no rule
    /// checks run. A footprint comes and goes whole; `catalog` supplies it when a bulldozed tile
    /// returns (a definition missing from it returns the anchor alone). Returns false when there
    /// is nothing to undo.
    pub fn undo(&mut self, catalog: &TileCatalog) -> bool {
        let Some(action) = self.history.undo.pop_back() else {
            return false;
        };
        match &action {
            Action::Place { cell, .. } => {
                self.lift(*cell);
            }
            Action::Bulldoze { cell, tile } => {
                self.put(catalog, *cell, tile.clone());
            }
            Action::Replace { cell, from, .. } => {
                self.tiles.insert(*cell, from.clone());
                self.stamp = Stamp::fresh();
            }
        }
        self.history.redo.push(action);
        true
    }

    /// Re-applies the latest undone action, a footprint whole as in [`World::undo`]. Returns false
    /// when there is nothing to redo.
    pub fn redo(&mut self, catalog: &TileCatalog) -> bool {
        let Some(action) = self.history.redo.pop() else {
            return false;
        };
        match &action {
            Action::Place { cell, tile } => {
                self.put(catalog, *cell, tile.clone());
            }
            Action::Bulldoze { cell, .. } => {
                self.lift(*cell);
            }
            Action::Replace { cell, to, .. } => {
                self.tiles.insert(*cell, to.clone());
                self.stamp = Stamp::fresh();
            }
        }
        self.history.undo.push_back(action);
        true
    }

    /// Inserts `tile` at `anchor`, with its members when its definition is a mega tile.
    fn put(&mut self, catalog: &TileCatalog, anchor: Axial, tile: PlacedTile) {
        if catalog.get(&tile.def).is_some_and(TileDef::is_mega) {
            for m in footprint::members(anchor).into_iter().skip(1) {
                self.members.insert(m, anchor);
            }
        }
        self.tiles.insert(anchor, tile);
        self.stamp = Stamp::fresh();
    }

    /// Removes the tile anchored at `anchor` together with its members.
    fn lift(&mut self, anchor: Axial) -> Option<PlacedTile> {
        let tile = self.tiles.remove(&anchor)?;
        self.stamp = Stamp::fresh();
        for m in footprint::members(anchor).into_iter().skip(1) {
            if self.members.get(&m) == Some(&anchor) {
                self.members.remove(&m);
            }
        }
        Some(tile)
    }

    /// Pushes a new action: clears redo and drops the oldest action beyond [`HISTORY_DEPTH`].
    fn record(&mut self, action: Action) {
        self.history.redo.clear();
        self.history.undo.push_back(action);
        while self.history.undo.len() > HISTORY_DEPTH {
            self.history.undo.pop_front();
        }
    }

    /// Flood fill over the 6-neighborhood of every occupied cell outside the footprint of the tile
    /// anchored at `removed`. True when every other anchor is reached.
    fn connected_without(&self, removed: Axial) -> bool {
        let Some(&start) = self.tiles.keys().find(|&&c| c != removed) else {
            return true;
        };
        let mut seen: BTreeSet<Axial> = BTreeSet::from([start]);
        let mut stack = vec![start];
        while let Some(cell) = stack.pop() {
            for e in 0..6 {
                let n = cell.neighbor(e);
                if self.anchor_of(n).is_some_and(|a| a != removed) && seen.insert(n) {
                    stack.push(n);
                }
            }
        }
        seen.iter().filter(|c| self.tiles.contains_key(c)).count() == self.tiles.len() - 1
    }
}

/// Calls `f` on each outward face of `def` with its anchor at `cell` in orientation `o` (the
/// cell, its world edge and the face it shows there), in [`World::faces`] order, until `f` returns
/// `false`. Returns whether every call returned `true`.
fn each_face<'c>(
    cell: Axial,
    def: &'c TileDef,
    o: Orientation,
    mut f: impl FnMut(Axial, u8, Facing<'c>) -> bool,
) -> bool {
    match &def.footprint {
        None => (0..6).all(|w| f(cell, w, Facing::single(def, o.rotation, o.level, w))),
        Some(fp) => footprint::outer_faces(cell, o.rotation)
            .into_iter()
            .all(|(m, w, j)| match fp.outer.get(usize::from(j)) {
                Some(socket) => f(
                    m,
                    w,
                    Facing {
                        def,
                        socket,
                        local: None,
                        level: o.level,
                    },
                ),
                None => true,
            }),
    }
}

/// The anchor of the tile covering `cell` in `tiles` with footprint members `members`.
fn anchor_in(
    tiles: &BTreeMap<Axial, PlacedTile>,
    members: &BTreeMap<Axial, Axial>,
    cell: Axial,
) -> Option<Axial> {
    if tiles.contains_key(&cell) {
        Some(cell)
    } else {
        members.get(&cell).copied()
    }
}

/// [`World::facing`] over a tile map and its footprint members ([`footprint_index`]), so play and
/// save validation read a seam by one rule.
pub(crate) fn facing_in<'c>(
    tiles: &BTreeMap<Axial, PlacedTile>,
    members: &BTreeMap<Axial, Axial>,
    catalog: &'c TileCatalog,
    cell: Axial,
    w: u8,
) -> Option<Facing<'c>> {
    let anchor = anchor_in(tiles, members, cell)?;
    let placed = tiles.get(&anchor)?;
    let def = catalog.get(&placed.def)?;
    let ring = if cell == anchor {
        None
    } else {
        Some((0..6).find(|&d| anchor.neighbor(d) == cell)?)
    };
    Some(Facing {
        def,
        socket: def.socket_toward(placed.rotation, ring, w)?,
        local: (!def.is_mega()).then(|| local_edge(w, placed.rotation)),
        level: placed.level,
    })
}

/// The ring cells of every mega tile in `tiles`, each mapped to its anchor, or the first cell (in
/// anchor order, then ring order) that two footprints, or a footprint and a tile, both claim. A
/// definition missing from `catalog` counts as a single tile.
pub(crate) fn footprint_index(
    tiles: &BTreeMap<Axial, PlacedTile>,
    catalog: &TileCatalog,
) -> Result<BTreeMap<Axial, Axial>, Axial> {
    let mut members: BTreeMap<Axial, Axial> = BTreeMap::new();
    for (&anchor, t) in tiles {
        if !catalog.get(&t.def).is_some_and(TileDef::is_mega) {
            continue;
        }
        for m in footprint::members(anchor).into_iter().skip(1) {
            if tiles.contains_key(&m) || members.insert(m, anchor).is_some() {
                return Err(m);
            }
        }
    }
    Ok(members)
}

/// Pair edge pieces of the tile at `cell`, by local edge (decision D3, `data/sockets.ron`
/// `pair_edge_pieces`). `at(cell)` is the definition and rotation of the tile covering `cell`, and
/// the cell's ring direction from that tile's anchor (`None` at an anchor), so a neighbour beside a
/// footprint's ring cell reads the footprint's outer socket ([`TileDef::socket_toward`]). An edge
/// gets a piece when the (own socket, neighbour socket) pair across it has an entry; its corner
/// flags say whether the local edges either side carry the same pair. A tile that lists its edge
/// pieces explicitly gets none, and so does a ring cell.
pub fn pair_edge_pieces<'a>(
    catalog: &'a TileCatalog,
    cell: Axial,
    at: impl Fn(Axial) -> Option<(&'a TileDef, u8, Option<u8>)>,
) -> [Option<KitRef>; 6] {
    let Some((def, rot, None)) = at(cell) else {
        return Default::default();
    };
    if def.edge_pieces != EdgePieces::Derived {
        return Default::default();
    }
    let sockets = catalog.sockets();
    let pair = |e: u8| {
        let world = (e + rot) % 6;
        let (n, nrot, ring) = at(cell.neighbor(world))?;
        Some((
            def.edges[e as usize].kind,
            n.socket_toward(nrot, ring, opposite_edge(world))?.kind,
        ))
    };
    std::array::from_fn(|e| {
        let e = e as u8;
        let (own, neighbour) = pair(e)?;
        sockets.pair_entry(own, neighbour)?;
        let same = |k: u8| pair(k % 6) == Some((own, neighbour));
        sockets.pair_edge_piece(own, neighbour, e, same(e + 5), same(e + 1))
    })
}

/// Resolved pair edge pieces of every placed cell. An edge's pair and its corner flags read only
/// its own tile and the cells across its edges, so a change at a cell (place, bulldoze, undo,
/// redo) can change the pieces of that cell and its six neighbours only: [`PairPieces::update`]
/// re-evaluates exactly those.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PairPieces {
    cells: BTreeMap<Axial, [Option<KitRef>; 6]>,
}

impl PairPieces {
    pub fn get(&self, cell: Axial) -> Option<&[Option<KitRef>; 6]> {
        self.cells.get(&cell)
    }

    /// Re-evaluates every cell in `changed` and its six neighbours against `at` (see
    /// [`pair_edge_pieces`]). Returns the cells whose pieces changed (a cell gaining or losing its
    /// tile counts as changed). Only anchors hold pieces. A footprint change reaches the cells
    /// around its ring only when `changed` lists its ring cells too.
    pub fn update<'a>(
        &mut self,
        catalog: &'a TileCatalog,
        changed: impl IntoIterator<Item = Axial>,
        at: impl Fn(Axial) -> Option<(&'a TileDef, u8, Option<u8>)>,
    ) -> BTreeSet<Axial> {
        let mut dirty = BTreeSet::new();
        for c in changed {
            dirty.insert(c);
            dirty.extend((0..6).map(|e| c.neighbor(e)));
        }
        let mut out = BTreeSet::new();
        for c in dirty {
            let new = at(c)
                .filter(|(.., ring)| ring.is_none())
                .map(|_| pair_edge_pieces(catalog, c, &at));
            if new.as_ref() != self.cells.get(&c) {
                out.insert(c);
                match new {
                    Some(p) => self.cells.insert(c, p),
                    None => self.cells.remove(&c),
                };
            }
        }
        out
    }
}

/// Deterministic per-cell seed (SplitMix64 of the world seed and the packed coordinate).
pub fn cell_seed(world_seed: u64, cell: Axial) -> u64 {
    let packed = ((cell.q as u32 as u64) << 32) | cell.r as u32 as u64;
    let mut z = world_seed ^ packed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::catalog::Placed;
    use crate::footprint;
    use crate::socket::{EdgeProfile, Side, SocketKind};

    fn catalog() -> TileCatalog {
        TileCatalog::builtin().unwrap_or_else(|e| panic!("{e}"))
    }

    /// A legal massif centre two steps east of the start meadow.
    const CENTRE: Axial = Axial { q: 2, r: 0 };

    /// A massif centred two steps east of the start meadow, turned by `k`.
    fn massif_world(c: &TileCatalog, k: u8) -> World {
        let mut w = World::new(1);
        w.place(c, CENTRE, "massif_granite", o(k, 0)).unwrap();
        w
    }

    /// The side cell between ring cells 0 and 1 of `CENTRE`: its world edges 4 and 3 face them.
    fn side_cell() -> Axial {
        CENTRE.neighbor(0).neighbor(1)
    }

    #[test]
    fn a_massif_occupies_all_seven_cells() {
        let c = catalog();
        let w = massif_world(&c, 0);
        assert_eq!(w.tiles().len(), 2);
        let anchor = w.tile(CENTRE).unwrap().clone();
        assert_eq!(anchor.def, "massif_granite");
        for m in footprint::members(CENTRE) {
            assert!(w.occupied(m), "{m:?}");
            assert_eq!(w.tile(m), Some(&anchor), "{m:?}");
            assert_eq!(w.anchor_of(m), Some(CENTRE), "{m:?}");
            let mut v = w.clone();
            assert_eq!(
                v.place(&c, m, "meadow", o(0, 0)),
                Err(PlaceError::Occupied),
                "{m:?}"
            );
            assert_eq!(v, w);
        }
        assert_eq!(w.members().len(), 6);
        assert_eq!(w.anchor_of(Axial::ORIGIN), Some(Axial::ORIGIN));
        assert_eq!(w.anchor_of(Axial::new(9, 9)), None);
        assert!(!w.occupied(Axial::new(9, 9)));
        // A massif whose ring overlaps the placed one is refused as occupied.
        let mut v = w.clone();
        assert_eq!(
            v.place(
                &c,
                CENTRE.neighbor(0).neighbor(0),
                "massif_granite",
                o(0, 0)
            ),
            Err(PlaceError::Occupied)
        );
        let mut rebuilt = w.clone();
        rebuilt.members.clear();
        rebuilt.rebuild_members(&c);
        assert_eq!(rebuilt, w);
    }

    #[test]
    fn a_massif_centre_is_two_steps_from_the_world() {
        let c = catalog();
        let massif = def(&c, "massif_granite");
        let w = World::new(1);
        for e in 0..6 {
            let frontier = Axial::ORIGIN.neighbor(e);
            assert!(w.legal_orientations(&c, frontier, massif).is_empty());
            assert!(!w.is_slot_for(frontier, massif));
            assert!(w.is_slot_for(frontier, def(&c, "meadow")));
        }
        assert!(
            w.inventory(&c, Axial::ORIGIN.neighbor(0))
                .iter()
                .all(|d| !d.is_mega())
        );
        let slots = w.mega_slots(&c, massif);
        let centres: Vec<Axial> = slots.iter().map(|(a, _)| *a).collect();
        let want: Vec<Axial> = footprint::outer_neighbours(Axial::ORIGIN)
            .into_iter()
            .filter(|&a| footprint::members(a).iter().all(|&m| !w.occupied(m)))
            .collect();
        assert_eq!(want.len(), 12);
        assert_eq!(centres, want);
        for (a, orientations) in &slots {
            assert_eq!(*orientations, w.legal_orientations(&c, *a, massif), "{a:?}");
            let level0: Vec<u8> = orientations
                .iter()
                .take_while(|o| o.level == 0)
                .map(|o| o.rotation)
                .collect();
            assert_eq!(level0, [0, 1, 2, 3, 4, 5], "{a:?}");
        }
        assert!(w.mega_slots(&c, def(&c, "meadow")).is_empty());
    }

    #[test]
    fn facing_a_member_returns_the_outer_socket() {
        let c = catalog();
        let w = massif_world(&c, 2);
        let members = footprint::members(CENTRE);
        for (cell, edge, _) in footprint::outer_faces(CENTRE, 2) {
            let f = w.facing(&c, cell, edge).expect("an outer face");
            assert_eq!(f.socket.kind, SocketKind::Grass);
            assert_eq!(
                (f.def.id.as_str(), f.local, f.level),
                ("massif_granite", None, 0)
            );
        }
        let mut interior = 0;
        for &m in &members {
            for e in 0..6 {
                if members.contains(&m.neighbor(e)) {
                    assert!(w.facing(&c, m, e).is_none(), "{m:?} edge {e}");
                    interior += 1;
                }
            }
        }
        assert_eq!(interior, 24);
        let f = w.facing(&c, Axial::ORIGIN, 0).unwrap();
        assert_eq!((f.def.id.as_str(), f.local), ("meadow", Some(0)));
        assert!(w.facing(&c, Axial::new(9, 9), 0).is_none());
    }

    #[test]
    fn a_neighbour_meeting_two_outer_edges_checks_both() {
        let c = catalog();
        let w = massif_world(&c, 0);
        let road = def(&c, "road_straight")
            .edges
            .iter()
            .find(|e| e.kind == SocketKind::Road)
            .unwrap()
            .clone();
        let mut grass_road = def(&c, "meadow").clone();
        grass_road.id = "grass_road".into();
        grass_road.edges[1] = road;
        let side = side_cell();
        // The Road edge (local 1) lands on world edge 3 at rotation 2 and 4 at rotation 3.
        assert_eq!(rots(&w, &c, side, &grass_road), vec![0, 1, 4, 5]);
        for k in [2, 3] {
            assert!(!w.fits(&c, side, &grass_road, o(k, 0)));
            let status = w.edge_status(&c, side, &grass_road, o(k, 0));
            let bad: Vec<u8> = (0..6)
                .filter(|&e| status[usize::from(e)] == EdgeStatus::Incompatible)
                .collect();
            assert_eq!(bad, vec![1 + k], "rotation {k}");
        }
    }

    #[test]
    fn frontier_rings_the_footprint() {
        let c = catalog();
        let w = massif_world(&c, 0);
        let f = w.frontier();
        assert!(f.is_sorted());
        for m in footprint::members(CENTRE) {
            assert!(!f.contains(&m), "{m:?}");
        }
        for n in footprint::outer_neighbours(CENTRE) {
            assert_eq!(f.contains(&n), !w.occupied(n), "{n:?}");
            assert_eq!(w.is_slot(n), !w.occupied(n), "{n:?}");
        }
        assert!(w.occupied(Axial::ORIGIN));
    }

    #[test]
    fn a_road_cannot_face_a_massif() {
        let c = catalog();
        let w = massif_world(&c, 0);
        let road = def(&c, "road_straight");
        let legal = rots(&w, &c, side_cell(), road);
        assert_eq!(legal, vec![2, 5]);
        for k in legal {
            assert_eq!(road.edge(3, k).kind, SocketKind::Grass);
            assert_eq!(road.edge(4, k).kind, SocketKind::Grass);
        }
    }

    #[test]
    fn a_raised_massif_meets_meadow_as_a_cliff() {
        let c = catalog();
        let massif = def(&c, "massif_granite");
        let w = World::new(1);
        let faces = w.faces(&c, CENTRE, massif, o(0, 1));
        assert_eq!(faces.len(), footprint::OUTER_EDGES);
        for f in &faces {
            let toward_meadow = f.cell.neighbor(f.w) == Axial::ORIGIN;
            assert_eq!(f.cliff, toward_meadow, "{f:?}");
            let want = if toward_meadow {
                EdgeStatus::Compatible
            } else {
                EdgeStatus::Neutral
            };
            assert_eq!(f.status, want, "{f:?}");
        }
        assert_eq!(faces.iter().filter(|f| f.cliff).count(), 1);
        assert!(w.fits(&c, CENTRE, massif, o(0, 1)));
        let legal = w.legal_orientations(&c, CENTRE, massif);
        assert_eq!(legal[0], o(0, 0));
        assert!(
            !w.faces(&c, CENTRE, massif, legal[0])
                .iter()
                .any(|f| f.cliff)
        );
        assert!(legal.contains(&o(0, 1)));
        // The single-tile view of a mega def has no face at its anchor.
        assert_eq!(
            w.edge_status(&c, CENTRE, massif, o(0, 1)),
            [EdgeStatus::Neutral; 6]
        );
        assert_eq!(w.cliff_edges(&c, CENTRE, massif, o(0, 1)), [false; 6]);
        // R walks the massif's six turns at level 0 before the cliffs.
        let seq = w.group_orientations(&c, CENTRE, &[massif]);
        let first: Vec<Orientation> = seq.iter().take(6).map(|(_, o)| *o).collect();
        assert_eq!(first, (0..6).map(|k| o(k, 0)).collect::<Vec<_>>());
        assert_eq!(
            w.cycle_group(
                &c,
                CENTRE,
                &[massif],
                (&massif.id, o(5, 0)),
                RotateDir::Forward
            ),
            Some((massif, o(0, 1)))
        );
    }

    #[test]
    fn massif_seed_comes_from_the_anchor() {
        let c = catalog();
        let w = massif_world(&c, 0);
        for m in footprint::members(CENTRE) {
            assert_eq!(
                w.tile(m).unwrap().seed,
                cell_seed(w.seed(), CENTRE),
                "{m:?}"
            );
        }
    }

    #[test]
    fn mega_centre_for_maps_a_frontier_slot_to_a_covering_centre() {
        let c = catalog();
        let massif = def(&c, "massif_granite");
        let w = World::new(1);
        let slot = Axial::ORIGIN.neighbor(0);
        assert!(w.is_slot(slot));
        assert_eq!(w.mega_centre_for(&c, slot, massif), Some(slot.neighbor(0)));
        assert_eq!(w.mega_centre_for(&c, CENTRE, massif), Some(CENTRE));
        assert_eq!(w.mega_centre_for(&c, Axial::new(9, 9), massif), None);
        // A pocket: an empty slot whose six neighbours are all placed.
        let mut v = World::new(1);
        for cell in [(1, 0), (1, 1), (2, 1), (3, 0), (3, -1), (2, -1)] {
            v.place(&c, Axial::new(cell.0, cell.1), "meadow", o(0, 0))
                .unwrap();
        }
        assert!(v.is_slot(CENTRE));
        assert_eq!(v.mega_centre_for(&c, CENTRE, massif), None);
    }

    /// The cell east of the massif at `CENTRE`, touching only its ring.
    const PAST_MASSIF: Axial = Axial { q: 4, r: 0 };

    /// `members` is exactly what the placed tiles derive, every member points at a placed mega
    /// anchor, and every mega anchor has all six members.
    fn assert_whole_footprints(w: &World, c: &TileCatalog) {
        let mut r = w.clone();
        r.rebuild_members(c);
        assert_eq!(r.members(), w.members());
        for (m, a) in w.members() {
            let t = w
                .tiles()
                .get(a)
                .unwrap_or_else(|| panic!("{m:?} points at a missing anchor {a:?}"));
            assert!(def(c, &t.def).is_mega(), "{m:?}");
        }
        for (&a, t) in w.tiles() {
            if def(c, &t.def).is_mega() {
                for m in footprint::members(a).into_iter().skip(1) {
                    assert_eq!(w.members().get(&m), Some(&a), "{m:?}");
                }
            }
        }
    }

    #[test]
    fn bulldozing_a_member_removes_the_whole_massif() {
        let c = catalog();
        let mut w = massif_world(&c, 0);
        let ring = CENTRE.neighbor(3);
        assert_eq!(w.can_bulldoze(&c, ring), Ok(()));
        let gone = w.bulldoze(&c, ring).unwrap();
        assert_eq!(gone.def, "massif_granite");
        for m in footprint::members(CENTRE) {
            assert!(!w.occupied(m), "{m:?}");
        }
        assert_eq!(w.tiles().len(), 1);
        assert!(w.members().is_empty());
        assert_eq!(w.history_stacks().0.back().map(Action::cell), Some(CENTRE));
    }

    #[test]
    fn undo_redo_never_leave_part_of_a_footprint() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, Axial::ORIGIN.neighbor(3), "meadow", o(0, 0))
            .unwrap();
        w.place(&c, CENTRE, "massif_granite", o(0, 0)).unwrap();
        w.place(&c, PAST_MASSIF, "meadow", o(0, 0)).unwrap();
        w.bulldoze(&c, PAST_MASSIF).unwrap();
        w.bulldoze(&c, CENTRE.neighbor(1)).unwrap();
        w.place(&c, CENTRE, "massif_granite", o(2, 0)).unwrap();
        w.place(&c, PAST_MASSIF, "meadow", o(0, 0)).unwrap();
        assert_whole_footprints(&w, &c);
        let done = w.clone();
        let mut steps = 0;
        while w.undo(&c) {
            assert_whole_footprints(&w, &c);
            steps += 1;
        }
        assert_eq!(steps, 7);
        assert_eq!(w.tiles().len(), 1);
        while w.redo(&c) {
            assert_whole_footprints(&w, &c);
        }
        assert_eq!(w, done);
    }

    #[test]
    fn bulldoze_refuses_a_massif_that_holds_the_world_together() {
        let c = catalog();
        let mut w = massif_world(&c, 0);
        w.place(&c, PAST_MASSIF, "meadow", o(0, 0)).unwrap();
        for m in footprint::members(CENTRE) {
            assert_eq!(
                w.can_bulldoze(&c, m),
                Err(BulldozeError::WouldDisconnect),
                "{m:?}"
            );
        }
        let before = w.clone();
        assert_eq!(
            w.bulldoze(&c, CENTRE.neighbor(3)),
            Err(BulldozeError::WouldDisconnect)
        );
        assert_eq!(w, before);
        assert_eq!(
            w.bulldoze(&c, PAST_MASSIF).map(|t| t.def),
            Ok("meadow".to_owned())
        );
    }

    #[test]
    fn a_tile_beside_a_member_counts_as_connected() {
        let c = catalog();
        let west = Axial::ORIGIN.neighbor(3);
        let mut w = World::new(1);
        w.place(&c, west, "meadow", o(0, 0)).unwrap();
        w.place(&c, CENTRE, "massif_granite", o(0, 0)).unwrap();
        w.place(&c, PAST_MASSIF, "meadow", o(0, 0)).unwrap();
        // Without the origin, `west` touches nothing.
        assert_eq!(
            w.can_bulldoze(&c, Axial::ORIGIN),
            Err(BulldozeError::WouldDisconnect)
        );
        // The origin and `PAST_MASSIF` touch only the massif's ring.
        assert_eq!(w.bulldoze(&c, west).map(|t| t.def), Ok("meadow".to_owned()));
        assert_eq!(w.can_bulldoze(&c, Axial::ORIGIN), Ok(()));
        assert_eq!(w.can_bulldoze(&c, PAST_MASSIF), Ok(()));
    }

    #[test]
    fn massif_replace_rules() {
        let c = catalog();
        let massif = def(&c, "massif_granite");
        let meadow = def(&c, "meadow");
        let mut w = massif_world(&c, 0);
        let before = w.clone();
        for m in footprint::members(CENTRE) {
            assert_eq!(
                w.replace(&c, m, "meadow", o(0, 0)),
                Err(ReplaceError::FootprintMismatch),
                "{m:?}"
            );
            assert!(w.replace_orientations(&c, m, meadow).is_empty(), "{m:?}");
        }
        assert_eq!(
            w.replace(&c, Axial::ORIGIN, "massif_granite", o(0, 0)),
            Err(ReplaceError::FootprintMismatch)
        );
        assert!(w.replace_orientations(&c, Axial::ORIGIN, massif).is_empty());
        assert_eq!(
            w.replace(&c, CENTRE.neighbor(2), "massif_granite", o(0, 0)),
            Err(ReplaceError::Unchanged)
        );
        assert_eq!(w, before);
        // Re-orienting from a ring cell turns the whole massif as one step at the anchor.
        let ring = CENTRE.neighbor(4);
        assert!(w.replace_orientations(&c, ring, massif).contains(&o(1, 0)));
        let depth = w.history_stacks().0.len();
        assert_eq!(
            w.replace(&c, ring, "massif_granite", o(1, 0))
                .map(|t| t.rotation),
            Ok(0)
        );
        assert_eq!(w.history_stacks().0.len(), depth + 1);
        assert_eq!(w.history_stacks().0.back().map(Action::cell), Some(CENTRE));
        assert_eq!(w.tiles().len(), 2);
        let turned = w.tile(CENTRE).unwrap();
        assert_eq!((turned.rotation, turned.seed), (1, cell_seed(1, CENTRE)));
        assert_eq!(w.members(), before.members());
        assert!(w.undo(&c));
        assert_eq!(w.tiles(), before.tiles());
        assert_eq!(w.members(), before.members());
    }

    fn def<'a>(c: &'a TileCatalog, id: &str) -> &'a TileDef {
        c.get(id).unwrap()
    }

    /// Rotations at which `d` fits at `cell` on level 0, ascending.
    fn rots(w: &World, c: &TileCatalog, cell: Axial, d: &TileDef) -> Vec<u8> {
        let mut out: Vec<u8> = w
            .legal_orientations(c, cell, d)
            .into_iter()
            .filter(|o| o.level == 0)
            .map(|o| o.rotation)
            .collect();
        out.sort();
        out
    }

    /// Rotations at which `d` may take the place of the tile at `cell` on level 0, ascending.
    fn replace_rots(w: &World, c: &TileCatalog, cell: Axial, d: &TileDef) -> Vec<u8> {
        let mut out: Vec<u8> = w
            .replace_orientations(c, cell, d)
            .into_iter()
            .filter(|o| o.level == 0)
            .map(|o| o.rotation)
            .collect();
        out.sort();
        out
    }

    /// The first rotation of each distinct level-0 orientation of `d` at `cell`.
    fn distinct0(w: &World, c: &TileCatalog, cell: Axial, d: &TileDef) -> Vec<u8> {
        w.group_orientations(c, cell, &[d])
            .into_iter()
            .filter(|(_, o)| o.level == 0)
            .map(|(_, o)| o.rotation)
            .collect()
    }

    /// R on a lone tile at level 0: the rotation [`World::cycle_group`] steps to.
    fn cycle(
        w: &World,
        c: &TileCatalog,
        cell: Axial,
        d: &TileDef,
        k: u8,
        dir: RotateDir,
    ) -> Option<u8> {
        w.cycle_group(c, cell, &[d], (&d.id, o(k, 0)), dir)
            .map(|(_, o)| o.rotation)
    }

    /// East neighbor of the origin; its world edge 3 faces the start meadow.
    const EAST: Axial = Axial { q: 1, r: 0 };

    /// A station east of the start meadow turned once: rail on world edges 1 and 4, grass toward
    /// the meadow. Its end on world edge 1 faces (1,1), a slot only the station touches (across its
    /// own edge 4).
    fn station_world(c: &TileCatalog) -> World {
        let mut w = World::new(1);
        w.place(c, EAST, "rail_station", o(1, 0)).unwrap();
        w
    }
    const BEYOND_END: Axial = Axial { q: 1, r: 1 };

    /// The world edges of the station's open ends, from the buffer stop rule.
    fn station_stops(c: &TileCatalog, w: &World) -> Vec<u8> {
        let at = |cell: Axial| w.tile(cell).map(|t| (def(c, &t.def), t.rotation));
        crate::rail_end::open_stops(def(c, "rail_station"), EAST, 1, at)
            .iter()
            .map(|s| (s.edge + 1) % 6)
            .collect()
    }

    /// Owner, 2026-10-25: a rail end closed by a buffer stop takes a neighbour with no rail (meadow,
    /// forest, houses) at any rotation, and the stop stays through placing, bulldozing, undo and
    /// redo. A road or river still may not dead-end into it, nor ocean meet it.
    #[test]
    fn stopped_rail_end_accepts_a_tile_without_rail() {
        let c = catalog();
        let mut w = station_world(&c);
        for id in ["meadow", "forest", "oak", "windmill", "wheat_field"] {
            assert_eq!(
                rots(&w, &c, BEYOND_END, def(&c, id)).len(),
                6,
                "{id} beyond the stop"
            );
        }
        // The houses (a road edge 0) fit every way but with their road to the stop.
        assert_eq!(
            rots(&w, &c, BEYOND_END, def(&c, "house_cluster")),
            vec![0, 1, 2, 3, 5]
        );
        // road_straight fits with a grass side to the stop, never its road.
        let road = def(&c, "road_straight");
        let legal = rots(&w, &c, BEYOND_END, road);
        assert_eq!(legal, vec![0, 2, 3, 5]);
        assert!(
            legal
                .iter()
                .all(|&k| road.edge(4, k).kind == SocketKind::Grass)
        );
        assert!(rots(&w, &c, BEYOND_END, def(&c, "ocean")).is_empty());
        assert_eq!(
            w.edge_status(&c, BEYOND_END, def(&c, "meadow"), o(0, 0))[4],
            EdgeStatus::Compatible
        );
        assert_eq!(station_stops(&c, &w), vec![1, 4]);
        w.place(&c, BEYOND_END, "meadow", o(0, 0)).unwrap();
        assert_eq!(station_stops(&c, &w), vec![1, 4], "the stop stays");
        w.bulldoze(&c, BEYOND_END).unwrap();
        assert_eq!(station_stops(&c, &w), vec![1, 4]);
        assert!(w.undo(&c) && w.tile(BEYOND_END).is_some());
        assert!(w.undo(&c) && w.tile(BEYOND_END).is_none());
        assert!(w.redo(&c) && w.redo(&c) && w.tile(BEYOND_END).is_none());
        assert_eq!(station_stops(&c, &w), vec![1, 4]);
        // A rail tile never turns an end toward a placed tile without rail.
        assert_eq!(
            rots(&World::new(1), &c, EAST, def(&c, "rail_station")),
            vec![1, 2, 4, 5]
        );
        // With a meadow beyond the end that also touches the start meadow, the station can be
        // bulldozed and restored by undo, its end against the meadow again.
        let mut v = station_world(&c);
        v.place(&c, Axial::new(1, -1), "meadow", o(0, 0)).unwrap();
        v.bulldoze(&c, EAST).unwrap();
        assert!(v.undo(&c));
        assert_eq!(v.tile(EAST).map(|t| t.rotation), Some(1));
        let at = |cell: Axial| v.tile(cell).map(|t| (def(&c, &t.def), t.rotation));
        assert_eq!(
            crate::rail_end::open_stops(def(&c, "rail_station"), EAST, 1, at).len(),
            2
        );
    }

    /// A rail tile beyond a stopped end still has to continue the line: its legal rotations are
    /// exactly those whose sockets meet, the ones with a rail edge facing the end, and placing one
    /// closes the end (its stop goes).
    #[test]
    fn rail_neighbour_still_connects_to_a_stopped_end() {
        let c = catalog();
        let mut w = station_world(&c);
        for d in c
            .tiles()
            .iter()
            .filter(|d| crate::rail_end::carries_rail(d))
        {
            let sockets_meet: Vec<u8> = (0..6)
                .filter(|&k| {
                    c.sockets().edges_compatible(
                        d.edge(4, k),
                        0,
                        def(&c, "rail_station").edge(1, 1),
                        0,
                    )
                })
                .collect();
            let legal = rots(&w, &c, BEYOND_END, d);
            assert_eq!(legal, sockets_meet, "{}", d.id);
            assert!(
                legal.iter().all(|&k| d.edge(4, k).kind == SocketKind::Rail),
                "{}",
                d.id
            );
        }
        assert_eq!(
            rots(&w, &c, BEYOND_END, def(&c, "rail_straight")),
            vec![1, 4]
        );
        assert_eq!(
            w.auto_orient(&c, BEYOND_END, def(&c, "rail_straight")),
            Some(o(1, 0))
        );
        w.place(&c, BEYOND_END, "rail_straight", o(1, 0)).unwrap();
        assert_eq!(station_stops(&c, &w), vec![4], "the line runs on");
    }

    /// Only a rail end that gets a buffer stop closes: a rail lane edge with no stop (here a stub
    /// whose lane edges carry no track to stand one on) still refuses a tile without rail, as every
    /// rail edge did before.
    #[test]
    fn rail_edge_without_a_stop_still_refuses_a_tile_without_rail() {
        let c = catalog();
        let straight = def(&c, "rail_straight");
        let meadow = def(&c, "meadow");
        let mut stub = straight.clone();
        stub.id = "rail_stub".into();
        stub.networks.clear();
        assert_eq!(crate::rail_end::stopped_edges(&stub), 0);
        assert_eq!(crate::rail_end::stopped_edges(straight), 0b1001);
        // A meadow placed west of a straight's stopped end 0, then the stub's unstopped one.
        assert!(c.placement_meets(
            Placed {
                def: meadow,
                rot: 0,
                level: 0
            },
            3,
            Placed {
                def: straight,
                rot: 0,
                level: 0
            }
        ));
        assert!(c.edges_meet(
            Placed {
                def: straight,
                rot: 0,
                level: 0
            },
            0,
            Placed {
                def: meadow,
                rot: 0,
                level: 0
            }
        ));
        assert!(!c.placement_meets(
            Placed {
                def: meadow,
                rot: 0,
                level: 0
            },
            3,
            Placed {
                def: &stub,
                rot: 0,
                level: 0
            }
        ));
        assert!(!c.edges_meet(
            Placed {
                def: &stub,
                rot: 0,
                level: 0
            },
            0,
            Placed {
                def: meadow,
                rot: 0,
                level: 0
            }
        ));
        // A rail tile never turns its end toward the meadow.
        assert!(!c.placement_meets(
            Placed {
                def: straight,
                rot: 0,
                level: 0
            },
            0,
            Placed {
                def: meadow,
                rot: 0,
                level: 0
            }
        ));
    }

    /// The load rule is symmetric for every pair of catalog tiles, rotations and edges, so load
    /// validation (which checks each touching pair once) accepts a pair placed in either order, and
    /// it accepts every pair placement accepts.
    #[test]
    fn edges_meet_is_symmetric() {
        let c = catalog();
        for a in c.tiles() {
            for b in c.tiles() {
                for (ra, rb, w) in
                    (0..216u16).map(|i| ((i / 36) as u8, (i / 6 % 6) as u8, (i % 6) as u8))
                {
                    let seam = format!("{} r{ra} | {} r{rb} across {w}", a.id, b.id);
                    let meet = c.edges_meet(
                        Placed {
                            def: a,
                            rot: ra,
                            level: 0,
                        },
                        w,
                        Placed {
                            def: b,
                            rot: rb,
                            level: 0,
                        },
                    );
                    assert_eq!(
                        meet,
                        c.edges_meet(
                            Placed {
                                def: b,
                                rot: rb,
                                level: 0
                            },
                            opposite_edge(w),
                            Placed {
                                def: a,
                                rot: ra,
                                level: 0
                            }
                        ),
                        "{seam}"
                    );
                    assert!(
                        meet || !c.placement_meets(
                            Placed {
                                def: a,
                                rot: ra,
                                level: 0
                            },
                            w,
                            Placed {
                                def: b,
                                rot: rb,
                                level: 0
                            }
                        ),
                        "{seam}"
                    );
                }
            }
        }
    }

    #[test]
    fn new_world_is_one_meadow_at_the_origin() {
        let w = World::new(7);
        assert_eq!(w.tiles().len(), 1);
        let t = w.tile(Axial::ORIGIN).unwrap();
        assert_eq!((t.def.as_str(), t.rotation), ("meadow", 0));
        assert!(!w.can_undo() && !w.can_redo());
    }

    #[test]
    fn frontier_is_the_sorted_ring_of_free_neighbors() {
        let c = catalog();
        let mut w = World::new(1);
        let mut want: Vec<Axial> = (0..6).map(|e| Axial::ORIGIN.neighbor(e)).collect();
        want.sort();
        assert_eq!(w.frontier(), want);
        w.place(&c, EAST, "meadow", o(0, 0)).unwrap();
        let f = w.frontier();
        assert_eq!(f.len(), 8);
        assert!(f.is_sorted());
        assert!(!f.contains(&EAST) && !f.contains(&Axial::ORIGIN));
    }

    #[test]
    fn legal_rotations_keep_road_ends_off_the_meadow() {
        let c = catalog();
        let w = World::new(1);
        assert_eq!(
            rots(&w, &c, EAST, def(&c, "road_straight")),
            vec![1, 2, 4, 5]
        );
        // A beach edge never faces the meadow (owner O1): only its grass edges 3 and 4 may.
        assert_eq!(rots(&w, &c, EAST, def(&c, "beach")), vec![0, 5]);
        assert!(rots(&w, &c, EAST, def(&c, "ocean")).is_empty());
        assert_eq!(rots(&w, &c, EAST, def(&c, "meadow")).len(), 6);
    }

    #[test]
    fn non_slots_have_no_legal_rotations_or_inventory() {
        let c = catalog();
        let w = World::new(1);
        let far = Axial::new(5, 5);
        assert!(rots(&w, &c, Axial::ORIGIN, def(&c, "meadow")).is_empty());
        assert!(rots(&w, &c, far, def(&c, "meadow")).is_empty());
        assert!(w.inventory(&c, Axial::ORIGIN).is_empty());
        assert!(w.inventory(&c, far).is_empty());
    }

    #[test]
    fn inventory_lists_only_tiles_that_fit_in_catalog_order() {
        let c = catalog();
        let mut w = World::new(1);
        let ids = |w: &World, cell| -> Vec<String> {
            w.inventory(&c, cell).iter().map(|d| d.id.clone()).collect()
        };
        let all_but_ocean: Vec<String> = c
            .tiles()
            .iter()
            .filter(|d| !d.is_mega())
            .map(|d| d.id.clone())
            .filter(|id| id != "ocean")
            .filter(|id| {
                c.get(id)
                    .is_some_and(|d| d.edges.iter().any(|e| e.profile == EdgeProfile::Flat))
            })
            // A viaduct needs rail on both deck ends, so it never fits at a lone slot.
            .filter(|id| {
                c.get(id)
                    .is_some_and(|d| d.edges.iter().all(|e| e.deck() == 0))
            })
            .collect();
        // A tile with no Flat edge (hillside 010101) cannot face the level meadow, so it never fits
        // beside it. A massif centre is never a frontier cell.
        assert_eq!(ids(&w, EAST), all_but_ocean);

        // A road end now points at EAST's world edge 4, next to the meadow on its edge 3: only
        // tiles with a road edge right after a grass-compatible edge fit. The bridge's road edges
        // follow river edges, so it does not.
        w.place(&c, Axial::new(1, -1), "road_straight", o(1, 0))
            .unwrap();
        assert_eq!(
            ids(&w, EAST),
            [
                "house_cluster",
                "bakery",
                "road_straight",
                "road_bend_120",
                "road_bend_60",
                "road_junction",
            ]
        );
    }

    #[test]
    fn auto_rotate_picks_the_first_legal_rotation() {
        let c = catalog();
        let w = World::new(1);
        assert_eq!(
            w.auto_orient(&c, EAST, def(&c, "road_straight")),
            Some(o(1, 0))
        );
        assert_eq!(w.auto_orient(&c, EAST, def(&c, "ocean")), None);
    }

    #[test]
    fn distinct_orientations_count_a_symmetric_turn_once() {
        let c = catalog();
        let w = World::new(1);
        let road = def(&c, "road_straight");
        // A straight road turned 180° shows the same sockets: 1 and 4, 2 and 5 are one orientation.
        assert_eq!(rots(&w, &c, EAST, road), vec![1, 2, 4, 5]);
        assert!(road.same_orientation(1, 4) && road.same_orientation(2, 5));
        assert!(!road.same_orientation(1, 2));
        assert_eq!(distinct0(&w, &c, EAST, road), vec![1, 2]);
        let meadow = def(&c, "meadow");
        assert_eq!(rots(&w, &c, EAST, meadow).len(), 6);
        assert_eq!(distinct0(&w, &c, EAST, meadow), vec![0]);
        assert!(distinct0(&w, &c, EAST, def(&c, "ocean")).is_empty());
    }

    /// A tile with hero props shows them turned at every rotation (the windmill's sails face a new
    /// way, the farm's barn stands on another side), so R reaches all six even where every edge's
    /// socket is equal (owner, 2026-10-20: the windmill ghost showed no rotation).
    #[test]
    fn hero_tiles_turn_through_every_rotation() {
        let c = catalog();
        let w = World::new(1);
        let heroes: Vec<&TileDef> = c
            .tiles()
            .iter()
            .filter(|d| !d.props.hero.is_empty())
            .collect();
        let ids: Vec<&str> = heroes.iter().map(|d| d.id.as_str()).collect();
        for id in ["windmill", "farm", "oak", "bakery", "house_cluster"] {
            assert!(ids.contains(&id), "{id} has hero props");
        }
        for d in heroes {
            for a in 0..6u8 {
                for b in a + 1..6 {
                    assert!(!d.same_orientation(a, b), "{} r{a} and r{b}", d.id);
                }
            }
            let legal = rots(&w, &c, EAST, d);
            // R visits cliff-free rotations first, so a sloped hero tile's order is not ascending.
            let order = distinct0(&w, &c, EAST, d);
            let mut sorted = order.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, legal, "{}", d.id);
            // R steps through the whole sequence (every level for a raisable tile), in order.
            let full = w.group_orientations(&c, EAST, &[d]);
            let mut cur = full[0].1;
            for (_, want) in full[1..].iter().chain(&full[..1]) {
                let (_, next) = w
                    .cycle_group(&c, EAST, &[d], (&d.id, cur), RotateDir::Forward)
                    .unwrap();
                assert_eq!(next, *want, "{} steps through each legal orientation", d.id);
                cur = next;
            }
        }
        let windmill = def(&c, "windmill");
        assert_eq!(distinct0(&w, &c, EAST, windmill), vec![0, 1, 2, 3, 4, 5]);
    }

    /// The wheat field's rows turn with it (owner D5, 2026-10-20: rows at 0, 60 and 120 degrees),
    /// though every edge is grass; a half turn lays the rows on themselves, so rotations 3-5 are
    /// 0-2 again and R steps 0, 1, 2 and back.
    #[test]
    fn wheat_field_turns_through_three_rotations() {
        let c = catalog();
        let w = World::new(1);
        let wheat = def(&c, "wheat_field");
        assert_eq!(wheat.orientation_period(), 3);
        for a in 0..6u8 {
            for b in 0..6u8 {
                assert_eq!(wheat.same_orientation(a, b), a % 3 == b % 3, "r{a} r{b}");
            }
        }
        assert_eq!(distinct0(&w, &c, EAST, wheat), vec![0, 1, 2]);
        let mut k = 0;
        for want in [1, 2, 0, 1] {
            k = cycle(&w, &c, EAST, wheat, k, RotateDir::Forward).unwrap();
            assert_eq!(k, want);
        }
        assert_eq!(
            cycle(&w, &c, EAST, wheat, 4, RotateDir::Forward),
            Some(2),
            "r4 is r1's orientation"
        );
        let mut bad = wheat.clone();
        bad.look_period = Some(4);
        assert!(
            bad.validate(c.sockets())
                .iter()
                .any(|m| m.contains("look_period 4 does not divide 6")),
            "{:?}",
            bad.validate(c.sockets())
        );
    }

    #[test]
    fn beach_curve_cycles_headland_then_bay_orientations() {
        let c = catalog();
        let w = World::new(1);
        let members = c.group_members("beach_curve");
        let ids: Vec<&str> = members.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, ["beach_outer", "beach_inner"]);
        // East of the start meadow only edge 3 touches grass: the headland fits one way, the bay
        // three (any of its grass edges against the meadow).
        let seq = w.group_orientations(&c, EAST, &members);
        let got: Vec<(&str, Orientation)> = seq.iter().map(|(d, k)| (d.id.as_str(), *k)).collect();
        assert_eq!(
            got,
            [
                ("beach_outer", o(0, 0)),
                ("beach_inner", o(0, 0)),
                ("beach_inner", o(1, 0)),
                ("beach_inner", o(5, 0))
            ]
        );
        for (d, k) in &seq {
            assert!(w.fits(&c, EAST, d, *k), "{} {k:?} is illegal", d.id);
        }
        for (i, (a, ka)) in seq.iter().enumerate() {
            for (b, kb) in &seq[i + 1..] {
                assert!(
                    a.id != b.id || !a.same_orientation(ka.rotation, kb.rotation),
                    "repeat"
                );
            }
        }
        // R walks the sequence and wraps; Shift+R walks it back.
        let mut cur = ("beach_outer".to_owned(), o(0, 0));
        let mut walked = vec![];
        for _ in 0..seq.len() {
            let (d, k) = w
                .cycle_group(&c, EAST, &members, (&cur.0, cur.1), RotateDir::Forward)
                .unwrap();
            cur = (d.id.clone(), k);
            walked.push(cur.clone());
        }
        assert_eq!(walked.last().unwrap(), &("beach_outer".to_owned(), o(0, 0)));
        assert_eq!(walked[0], ("beach_inner".to_owned(), o(0, 0)));
        let back = w
            .cycle_group(
                &c,
                EAST,
                &members,
                ("beach_outer", o(0, 0)),
                RotateDir::Back,
            )
            .unwrap();
        assert_eq!((back.0.id.as_str(), back.1), ("beach_inner", o(5, 0)));
        // From an illegal headland rotation: the headland's own next legal one.
        let from_bad = w
            .cycle_group(
                &c,
                EAST,
                &members,
                ("beach_outer", o(3, 0)),
                RotateDir::Forward,
            )
            .unwrap();
        assert_eq!(
            (from_bad.0.id.as_str(), from_bad.1),
            ("beach_outer", o(0, 0))
        );
    }

    #[test]
    fn beach_curve_switches_to_the_bay_where_only_the_bay_fits() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, Axial::new(0, 1), "meadow", o(0, 0)).unwrap();
        w.place(&c, Axial::new(1, 1), "meadow", o(0, 0)).unwrap();
        let slot = Axial::new(1, 0);
        let members = c.group_members("beach_curve");
        let seq = w.group_orientations(&c, slot, &members);
        assert!(!seq.is_empty());
        assert!(seq.iter().all(|(d, _)| d.id == "beach_inner"), "{seq:?}");
        let (d, _) = w
            .cycle_group(
                &c,
                slot,
                &members,
                ("beach_outer", o(0, 0)),
                RotateDir::Forward,
            )
            .unwrap();
        assert_eq!(d.id, "beach_inner");
    }

    #[test]
    fn cycling_walks_distinct_orientations_and_wraps() {
        let c = catalog();
        let w = World::new(1);
        // The straight railway: like road_straight, rotation cycling only.
        let road = def(&c, "rail_straight");
        let next = |k, dir| cycle(&w, &c, EAST, road, k, dir);
        assert_eq!(next(1, RotateDir::Forward), Some(2));
        assert_eq!(
            next(2, RotateDir::Forward),
            Some(1),
            "4 shows 1's sockets and is skipped"
        );
        assert_eq!(next(5, RotateDir::Forward), Some(1), "5 is 2's orientation");
        assert_eq!(next(1, RotateDir::Back), Some(2));
        assert_eq!(next(4, RotateDir::Back), Some(2), "4 is 1's orientation");
        assert_eq!(
            next(0, RotateDir::Forward),
            Some(1),
            "illegal: the next legal orientation"
        );
        assert_eq!(
            next(0, RotateDir::Back),
            Some(2),
            "back from 0 reaches 5, 2's orientation"
        );
        // Every press changes the orientation: R never lands on a duplicate.
        let mut k = 1;
        for _ in 0..6 {
            let n = next(k, RotateDir::Forward).unwrap();
            assert!(!road.same_orientation(k, n));
            k = n;
        }
        assert_eq!(
            cycle(&w, &c, EAST, def(&c, "ocean"), 0, RotateDir::Forward),
            None
        );
    }

    #[test]
    fn cycling_a_fully_symmetric_tile_stays_on_its_one_orientation() {
        let c = catalog();
        let w = World::new(1);
        // Forest: every edge alike, so every step keeps rotation 0 (R steps only its level).
        let forest = def(&c, "forest");
        for dir in [RotateDir::Forward, RotateDir::Back] {
            assert_eq!(cycle(&w, &c, EAST, forest, 0, dir), Some(0));
            assert_eq!(cycle(&w, &c, EAST, forest, 3, dir), Some(0));
        }
        // A raisable meadow has one orientation per level, ground first, and wraps.
        let meadow = def(&c, "meadow");
        let seq: Vec<Orientation> = w
            .group_orientations(&c, EAST, &[meadow])
            .into_iter()
            .map(|(_, o)| o)
            .collect();
        assert_eq!(seq, [o(0, 0), o(0, 1), o(0, 2), o(0, 3)]);
        let step = |k: Orientation| {
            w.cycle_group(&c, EAST, &[meadow], ("meadow", k), RotateDir::Forward)
                .unwrap()
                .1
        };
        assert_eq!(step(o(3, 0)), o(0, 1), "r3 is r0's orientation");
        assert_eq!(step(o(0, 3)), o(0, 0));
    }

    #[test]
    fn cycling_a_single_legal_rotation_stays_put() {
        let c = catalog();
        let mut w = World::new(1);
        // House road edge must face the road end on EAST's world edge 4: only rotation 4.
        w.place(&c, Axial::new(1, -1), "road_straight", o(1, 0))
            .unwrap();
        let house = def(&c, "house_cluster");
        assert_eq!(rots(&w, &c, EAST, house), vec![4]);
        for dir in [RotateDir::Forward, RotateDir::Back] {
            assert_eq!(cycle(&w, &c, EAST, house, 4, dir), Some(4));
            assert_eq!(cycle(&w, &c, EAST, house, 0, dir), Some(4));
        }
    }

    #[test]
    fn edge_status_glows_each_world_edge() {
        let c = catalog();
        let w = World::new(1);
        let road = def(&c, "road_straight");
        let s = w.edge_status(&c, EAST, road, o(0, 0));
        assert_eq!(s[3], EdgeStatus::Incompatible);
        assert!([0, 1, 2, 4, 5].iter().all(|&e| s[e] == EdgeStatus::Neutral));
        let s = w.edge_status(&c, EAST, road, o(1, 0));
        assert_eq!(s[3], EdgeStatus::Compatible);
    }

    #[test]
    fn roads_join_across_a_seam() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, Axial::new(1, -1), "road_straight", o(1, 0))
            .unwrap();
        let road = def(&c, "road_straight");
        // EAST edge 4 faces (1,-1)'s road end on its edge 1.
        let s = w.edge_status(&c, EAST, road, o(1, 0));
        assert_eq!(s[4], EdgeStatus::Compatible);
        assert_eq!(s[3], EdgeStatus::Compatible);
        w.place(&c, EAST, "road_straight", o(1, 0)).unwrap();
    }

    #[test]
    fn place_accepts_only_legal_placements() {
        let c = catalog();
        let mut w = World::new(1);
        assert_eq!(
            w.place(&c, Axial::ORIGIN, "meadow", o(0, 0)),
            Err(PlaceError::Occupied)
        );
        assert_eq!(
            w.place(&c, Axial::new(3, 3), "meadow", o(0, 0)),
            Err(PlaceError::NotAdjacent)
        );
        assert_eq!(
            w.place(&c, EAST, "castle", o(0, 0)),
            Err(PlaceError::UnknownTile("castle".into()))
        );
        assert_eq!(
            w.place(&c, EAST, "meadow", o(6, 0)),
            Err(PlaceError::BadRotation(6))
        );
        assert_eq!(
            w.place(&c, EAST, "road_straight", o(0, 0)),
            Err(PlaceError::Incompatible(vec![3]))
        );
        assert_eq!(w.tiles().len(), 1);
        assert!(!w.can_undo());
        w.place(&c, EAST, "road_straight", o(2, 0)).unwrap();
        let t = w.tile(EAST).unwrap();
        assert_eq!((t.def.as_str(), t.rotation), ("road_straight", 2));
        assert_eq!(t.seed, cell_seed(1, EAST));
    }

    #[test]
    fn placement_must_fit_every_neighbor() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, EAST, "beach", o(0, 0)).unwrap();
        // (1,-1) touches the origin meadow (its edge 2) and the beach (its edge 1, beach edge 4 =
        // Grass). Ocean fits neither, forest fits both.
        let slot = Axial::new(1, -1);
        assert!(rots(&w, &c, slot, def(&c, "ocean")).is_empty());
        assert_eq!(rots(&w, &c, slot, def(&c, "forest")).len(), 6);
    }

    /// A beach east of the start meadow: coast along its world edges 2 (land on the right) and 5.
    fn coast_world(c: &TileCatalog) -> World {
        let mut w = World::new(1);
        w.place(c, EAST, "beach", o(0, 0)).unwrap();
        w
    }

    /// Owner O1: across a beach edge the coast only continues the same way. (0,1) meets the
    /// beach's edge 2 (land on the right) with its own edge 5, which needs the land on its left;
    /// the mirrored beach puts its land end against the sea end and is refused.
    #[test]
    fn coast_continues_only_the_same_way() {
        let c = catalog();
        let mut w = coast_world(&c);
        let next = Axial::new(0, 1);
        let beach = def(&c, "beach");
        assert_eq!(rots(&w, &c, next, beach), vec![0]);
        assert_eq!(beach.edge(5, 3).kind, SocketKind::Beach);
        assert_eq!(
            w.edge_status(&c, next, beach, o(3, 0))[5],
            EdgeStatus::Incompatible
        );
        assert_eq!(
            w.place(&c, next, "beach", o(3, 0)),
            Err(PlaceError::Incompatible(vec![4, 5]))
        );
        w.place(&c, next, "beach", o(0, 0)).unwrap();
    }

    /// Owner O1: beach edges never meet grass or ocean edges, under any rotation.
    #[test]
    fn beach_never_meets_grass_or_ocean() {
        let c = catalog();
        let mut w = coast_world(&c);
        // A beach edge (local 5) against the meadow.
        assert_eq!(
            w.place(&c, Axial::new(-1, 0), "beach", o(1, 0)),
            Err(PlaceError::Incompatible(vec![0]))
        );
        // Grass against the beach's edge 2, ocean against its edge 5.
        assert_eq!(
            w.place(&c, Axial::new(0, 1), "meadow", o(0, 0)),
            Err(PlaceError::Incompatible(vec![5]))
        );
        assert_eq!(
            w.place(&c, Axial::new(2, -1), "ocean", o(0, 0)),
            Err(PlaceError::Incompatible(vec![2]))
        );
        for id in ["meadow", "forest", "ocean"] {
            assert!(
                rots(&w, &c, Axial::new(2, -1), def(&c, id)).is_empty(),
                "{id} beside a beach edge"
            );
        }
        // Every coast tile at every rotation fits beside the meadow only with a grass edge there.
        let lone = World::new(1);
        for id in ["beach", "beach_outer", "beach_inner"] {
            let d = def(&c, id);
            for k in 0..6 {
                let facing = d.edge(3, k).kind;
                assert_eq!(
                    lone.fits(&c, EAST, d, o(k, 0)),
                    facing == SocketKind::Grass,
                    "{id} r{k} shows {facing:?} to the meadow"
                );
            }
        }
    }

    /// Where the coast goes after (0,1), next to the beach east of the meadow: a straight beach
    /// carries it on along the same line, the headland turns it one edge toward the land (round
    /// the meadow) and the bay one edge away from it. Each has exactly one legal rotation.
    #[test]
    fn corners_turn_the_coast_toward_and_away_from_the_land() {
        let c = catalog();
        let w = coast_world(&c);
        let next = Axial::new(0, 1);
        for (id, rotation, leaves_by) in [
            ("beach", 0, 2),
            ("beach_outer", 1, 3),
            ("beach_inner", 0, 1),
        ] {
            let d = def(&c, id);
            assert_eq!(rots(&w, &c, next, d), vec![rotation], "{id}");
            // The coast enters by the edge with the land on its left (world 5, facing the beach)
            // and leaves by the one with the land on its right.
            let side = |s: Side| -> Vec<u8> {
                (0..6)
                    .filter(|&e| d.edge(e, rotation).land == Some(s))
                    .collect()
            };
            assert_eq!(side(Side::Left), vec![5], "{id}");
            assert_eq!(side(Side::Right), vec![leaves_by], "{id}");
        }
    }

    /// Six headlands close a coast round the start meadow (an island), and six bays close one
    /// round an ocean (a lake). Every seam in each ring is a beach-to-beach join.
    #[test]
    fn rings_of_corners_close_an_island_and_a_lake() {
        let c = catalog();
        let mut w = World::new(1);
        for d in 0..6u8 {
            // Grass edge 3 faces the meadow across world edge d + 3.
            w.place(&c, Axial::ORIGIN.neighbor(d), "beach_outer", o(d, 0))
                .unwrap_or_else(|e| panic!("headland {d}: {e}"));
        }
        assert_eq!(w.tiles().len(), 7);

        let mut w = World::new(1);
        let lake = Axial::new(2, 0);
        for d in [3, 4, 5, 0, 1, 2] {
            // Ocean edge 0 faces the lake across world edge d + 3.
            w.place(&c, lake.neighbor(d), "beach_inner", o((d + 3) % 6, 0))
                .unwrap_or_else(|e| panic!("bay {d}: {e}"));
        }
        w.place(&c, lake, "ocean", o(0, 0)).unwrap();
        assert_eq!(w.tiles().len(), 8);
        // A gap in the ring takes only the bay that closes it, at one rotation.
        let gap = lake.neighbor(1);
        w.bulldoze(&c, gap).unwrap();
        let fitting: Vec<&str> = w.inventory(&c, gap).iter().map(|d| d.id.as_str()).collect();
        assert_eq!(fitting, ["beach_inner"]);
        assert_eq!(rots(&w, &c, gap, def(&c, "beach_inner")), vec![4]);
    }

    fn line_of_three(c: &TileCatalog) -> World {
        let mut w = World::new(1);
        w.place(c, EAST, "meadow", o(0, 0)).unwrap();
        w.place(c, Axial::new(2, 0), "meadow", o(0, 0)).unwrap();
        w
    }

    #[test]
    fn last_tile_cannot_be_bulldozed() {
        let c = catalog();
        let mut w = World::new(1);
        assert_eq!(w.bulldoze(&c, Axial::ORIGIN), Err(BulldozeError::LastTile));
        assert_eq!(w.tiles().len(), 1);
    }

    #[test]
    fn empty_cell_cannot_be_bulldozed() {
        let c = catalog();
        let mut w = World::new(1);
        assert_eq!(w.bulldoze(&c, EAST), Err(BulldozeError::Empty));
    }

    #[test]
    fn bulldoze_refuses_to_split_the_world() {
        let c = catalog();
        let mut w = line_of_three(&c);
        assert_eq!(w.bulldoze(&c, EAST), Err(BulldozeError::WouldDisconnect));
        assert_eq!(w.tiles().len(), 3);
        assert_eq!(w.bulldoze(&c, Axial::new(2, 0)).unwrap().def, "meadow");
        assert_eq!(w.bulldoze(&c, Axial::ORIGIN).unwrap().def, "meadow");
        assert_eq!(w.bulldoze(&c, EAST), Err(BulldozeError::LastTile));
    }

    #[test]
    fn bulldoze_allows_removal_inside_a_ring() {
        let c = catalog();
        let mut w = World::new(1);
        for e in 0..6 {
            w.place(&c, Axial::ORIGIN.neighbor(e), "meadow", o(0, 0))
                .unwrap();
        }
        // The ring stays connected without its center or any one ring cell.
        assert_eq!(w.can_bulldoze(&c, EAST), Ok(()));
        w.bulldoze(&c, Axial::ORIGIN).unwrap();
        assert_eq!(w.tiles().len(), 6);
        // Now every ring cell has two ring neighbors; removing one leaves a connected arc.
        w.bulldoze(&c, EAST).unwrap();
        // The arc's middle cells are now cut points.
        assert_eq!(
            w.bulldoze(&c, Axial::ORIGIN.neighbor(3)),
            Err(BulldozeError::WouldDisconnect)
        );
    }

    #[test]
    fn undo_and_redo_a_placement() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, EAST, "forest", o(3, 0)).unwrap();
        let placed = w.clone();
        assert!(w.undo(&c));
        assert_eq!(w.tiles(), World::new(1).tiles());
        assert!(!w.undo(&c), "the start tile is not undoable");
        assert!(w.redo(&c));
        assert_eq!(w, placed);
        assert!(!w.redo(&c));
    }

    #[test]
    fn undo_restores_a_bulldozed_tile_exactly() {
        let c = catalog();
        let mut w = World::new(9);
        w.place(&c, EAST, "road_straight", o(2, 0)).unwrap();
        let before = w.tiles().clone();
        w.bulldoze(&c, EAST).unwrap();
        assert!(w.tile(EAST).is_none());
        assert!(w.undo(&c));
        assert_eq!(w.tiles(), &before);
        assert!(w.redo(&c));
        assert!(w.tile(EAST).is_none());
    }

    #[test]
    fn undo_is_lifo_across_places_and_bulldozes() {
        let c = catalog();
        let mut w = World::new(1);
        let mut states = vec![w.tiles().clone()];
        w.place(&c, EAST, "meadow", o(0, 0)).unwrap();
        states.push(w.tiles().clone());
        w.place(&c, Axial::new(2, 0), "forest", o(0, 0)).unwrap();
        states.push(w.tiles().clone());
        w.bulldoze(&c, Axial::new(2, 0)).unwrap();
        states.push(w.tiles().clone());
        w.bulldoze(&c, Axial::ORIGIN).unwrap();
        states.push(w.tiles().clone());
        for want in states.iter().rev().skip(1) {
            assert!(w.undo(&c));
            assert_eq!(w.tiles(), want);
        }
        assert!(!w.undo(&c));
        for want in states.iter().skip(1) {
            assert!(w.redo(&c));
            assert_eq!(w.tiles(), want);
        }
    }

    #[test]
    fn a_new_action_clears_redo() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, EAST, "meadow", o(0, 0)).unwrap();
        w.undo(&c);
        assert!(w.can_redo());
        w.place(&c, Axial::new(-1, 0), "meadow", o(0, 0)).unwrap();
        assert!(!w.can_redo());
        assert!(!w.redo(&c));
    }

    #[test]
    fn failed_actions_do_not_touch_history() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, EAST, "meadow", o(0, 0)).unwrap();
        w.undo(&c);
        assert!(w.place(&c, EAST, "ocean", o(0, 0)).is_err());
        assert!(w.bulldoze(&c, Axial::ORIGIN).is_err());
        assert!(w.can_redo());
        assert!(!w.can_undo());
    }

    #[test]
    fn history_keeps_the_newest_actions_up_to_the_depth() {
        let c = catalog();
        let mut w = World::new(1);
        let total = HISTORY_DEPTH + 50;
        // A straight line east keeps every slot adjacent.
        for i in 1..=total as i32 {
            w.place(&c, Axial::new(i, 0), "meadow", o(0, 0)).unwrap();
        }
        let mut undone = 0;
        while w.undo(&c) {
            undone += 1;
        }
        assert_eq!(undone, HISTORY_DEPTH);
        const { assert!(HISTORY_DEPTH >= 100) };
        // The oldest 50 placements were dropped from history, so they stay.
        assert_eq!(w.tiles().len(), 1 + total - HISTORY_DEPTH);
        assert!(w.tile(Axial::new(50, 0)).is_some());
        assert!(w.tile(Axial::new(51, 0)).is_none());
    }

    #[test]
    fn replace_swaps_a_tile_that_fits_its_neighbours() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, EAST, "meadow", o(0, 0)).unwrap();
        assert_eq!(
            replace_rots(&w, &c, EAST, def(&c, "forest")).len(),
            6,
            "forest fits beside the meadow at every rotation"
        );
        assert!(replace_rots(&w, &c, EAST, def(&c, "ocean")).is_empty());
        assert!(
            replace_rots(&w, &c, Axial::new(2, 0), def(&c, "forest")).is_empty(),
            "an empty cell has nothing to replace"
        );
        let old = w.replace(&c, EAST, "forest", o(2, 0)).unwrap();
        assert_eq!((old.def.as_str(), old.rotation), ("meadow", 0));
        let t = w.tile(EAST).unwrap();
        assert_eq!((t.def.as_str(), t.rotation), ("forest", 2));
        assert_eq!(t.seed, cell_seed(1, EAST));
        assert_eq!(w.tiles().len(), 2);
    }

    #[test]
    fn replace_refuses_what_does_not_fit_and_leaves_the_world() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, EAST, "meadow", o(0, 0)).unwrap();
        let before = w.clone();
        assert_eq!(
            w.replace(&c, EAST, "ocean", o(0, 0)),
            Err(ReplaceError::Incompatible(vec![3]))
        );
        assert_eq!(
            w.replace(&c, Axial::new(2, 0), "forest", o(0, 0)),
            Err(ReplaceError::Empty)
        );
        assert_eq!(
            w.replace(&c, EAST, "castle", o(0, 0)),
            Err(ReplaceError::UnknownTile("castle".into()))
        );
        assert_eq!(
            w.replace(&c, EAST, "forest", o(6, 0)),
            Err(ReplaceError::BadRotation(6))
        );
        assert_eq!(w, before, "refusals change neither tiles nor history");
    }

    #[test]
    fn replace_with_the_same_tile_and_orientation_is_a_no_op() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, EAST, "road_straight", o(1, 0)).unwrap();
        let before = w.clone();
        assert_eq!(
            w.replace(&c, EAST, "road_straight", o(1, 0)),
            Err(ReplaceError::Unchanged)
        );
        // Rotation 4 shows rotation 1's sockets on every edge: the same orientation.
        assert_eq!(
            w.replace(&c, EAST, "road_straight", o(4, 0)),
            Err(ReplaceError::Unchanged)
        );
        assert_eq!(w, before, "a no-op records nothing");
        // Another orientation of the same tile turns it in place.
        w.replace(&c, EAST, "road_straight", o(2, 0)).unwrap();
        assert_eq!(w.tile(EAST).unwrap().rotation, 2);
    }

    #[test]
    fn replace_is_one_undo_step_and_redo_reapplies_it() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, EAST, "meadow", o(0, 0)).unwrap();
        let placed = w.tiles().clone();
        w.replace(&c, EAST, "forest", o(3, 0)).unwrap();
        let replaced = w.clone();
        assert!(w.undo(&c));
        assert_eq!(w.tiles(), &placed, "one undo restores the meadow");
        assert!(w.can_undo() && w.can_redo());
        assert!(w.redo(&c));
        assert_eq!(w, replaced, "redo puts the forest back at rotation 3");
        // Undo twice: the replace, then the placement.
        assert!(w.undo(&c) && w.undo(&c));
        assert_eq!(w.tiles(), World::new(1).tiles());
        // A new action after an undone replace clears its redo.
        assert!(w.redo(&c) && w.redo(&c));
        assert!(w.undo(&c));
        w.place(&c, Axial::new(-1, 0), "meadow", o(0, 0)).unwrap();
        assert!(!w.can_redo());
        assert_eq!(w.tile(EAST).unwrap().def, "meadow");
    }

    #[test]
    fn world_with_history_round_trips_through_ron() {
        let c = catalog();
        let mut w = World::new(42);
        w.place(&c, EAST, "road_straight", o(1, 0)).unwrap();
        w.place(&c, Axial::new(-1, 0), "forest", o(0, 0)).unwrap();
        w.bulldoze(&c, Axial::new(-1, 0)).unwrap();
        w.undo(&c);
        let text = ron::ser::to_string(&w).unwrap();
        let back: World = ron::from_str(&text).unwrap();
        assert_eq!(back, w);
        assert!(back.can_undo() && back.can_redo());
        assert_eq!(
            ron::ser::to_string(&back).unwrap(),
            text,
            "serialization is deterministic"
        );
    }

    #[test]
    fn cell_seeds_are_deterministic_and_distinct() {
        assert_eq!(cell_seed(1, EAST), cell_seed(1, EAST));
        assert_ne!(cell_seed(1, EAST), cell_seed(2, EAST));
        assert_ne!(cell_seed(1, EAST), cell_seed(1, Axial::new(0, 1)));
    }

    /// Re-evaluates the pair pieces around `changed` and checks the incremental cache against a
    /// full evaluation of every placed cell. Returns the cells whose pieces changed.
    fn pairs_after(
        w: &World,
        c: &TileCatalog,
        cache: &mut PairPieces,
        changed: Axial,
    ) -> BTreeSet<Axial> {
        let at = |cell: Axial| occupant(w, c, cell);
        let out = cache.update(c, [changed], at);
        let mut full = PairPieces::default();
        full.update(c, w.tiles().keys().copied().collect::<Vec<_>>(), at);
        assert_eq!(
            *cache, full,
            "incremental pair pieces differ from a full evaluation"
        );
        out
    }

    /// The tile covering `cell` as [`pair_edge_pieces`] reads it: definition, rotation and the
    /// cell's ring direction from its anchor.
    fn occupant<'c>(
        w: &World,
        c: &'c TileCatalog,
        cell: Axial,
    ) -> Option<(&'c TileDef, u8, Option<u8>)> {
        let anchor = w.anchor_of(cell)?;
        let t = &w.tiles()[&anchor];
        let ring = (cell != anchor).then(|| (0..6).find(|&d| anchor.neighbor(d) == cell));
        Some((c.get(&t.def)?, t.rotation, ring.flatten()))
    }

    /// A forest beside a massif's ring cells reads the massif's outer socket there, as
    /// [`World::facing`] does, rather than the anchor's own edges. The builtin pair table has no
    /// entry with Grass, so the test adds a (Forest, Grass) pair for the seam to show.
    #[test]
    fn a_pair_piece_beside_a_massif_member_reads_its_outer_socket() {
        let base = catalog();
        let mut file = base.sockets().file().clone();
        file.pair_edge_pieces.push(crate::socket::PairEdgePiece {
            own: SocketKind::Forest,
            neighbour: SocketKind::Grass,
            piece: "forest_grass_edge".into(),
            per_base_edge: false,
            corners: crate::socket::PairCorners::Straight,
        });
        let c = TileCatalog::new(
            crate::socket::SocketTable::new(file).unwrap(),
            base.tiles().to_vec(),
        )
        .unwrap();
        let mut w = massif_world(&c, 2);
        let forest = side_cell();
        w.place(&c, forest, "forest", o(1, 0)).unwrap();
        let pieces = pair_edge_pieces(&c, forest, |cell| occupant(&w, &c, cell));
        let mut beside = 0;
        for e in 0..6u8 {
            let world = (e + 1) % 6;
            let want = w
                .facing(&c, forest.neighbor(world), opposite_edge(world))
                .and_then(|f| {
                    c.sockets()
                        .pair_edge_piece(SocketKind::Forest, f.socket.kind, e, false, false)
                });
            assert_eq!(pieces[e as usize], want, "local edge {e}");
            if w.members().contains_key(&forest.neighbor(world)) {
                assert_eq!(want.as_ref().map(KitRef::as_str), Some("forest_grass_edge"));
                beside += 1;
            }
        }
        assert_eq!(beside, 2, "a side cell meets two outer edges");
        // Ring cells hold no pair pieces of their own; the forest's come back from the cache.
        let mut cache = PairPieces::default();
        let mut cells: Vec<Axial> = w.tiles().keys().copied().collect();
        cells.extend(w.members().keys().copied());
        cache.update(&c, cells, |cell| occupant(&w, &c, cell));
        assert!(w.members().keys().all(|&m| cache.get(m).is_none()));
        assert_eq!(cache.get(forest), Some(&pieces));
    }

    fn piece(cache: &PairPieces, cell: Axial, local: usize) -> Option<&str> {
        cache.get(cell)?[local].as_ref().map(KitRef::as_str)
    }

    /// Two forests east of the start meadow: EAST (rotation 0) and FAR = EAST's edge-0 neighbour
    /// (rotation 1, so its world edge 3 is its local edge 2 and its base variant is e2).
    const FAR: Axial = Axial { q: 2, r: 0 };

    fn two_forests() -> (World, TileCatalog, PairPieces) {
        let c = catalog();
        let mut w = World::new(1);
        let mut cache = PairPieces::default();
        pairs_after(&w, &c, &mut cache, Axial::ORIGIN);
        w.place(&c, EAST, "forest", o(0, 0)).unwrap();
        pairs_after(&w, &c, &mut cache, EAST);
        assert!(
            cache.get(EAST).unwrap().iter().all(Option::is_none),
            "forest beside meadow"
        );
        (w, c, cache)
    }

    #[test]
    fn placing_a_forest_beside_a_forest_gives_both_their_edge_variants() {
        let (mut w, c, mut cache) = two_forests();
        w.place(&c, FAR, "forest", o(1, 0)).unwrap();
        let changed = pairs_after(&w, &c, &mut cache, FAR);
        assert_eq!(changed, BTreeSet::from([EAST, FAR]));
        assert_eq!(piece(&cache, EAST, 0), Some("forest_forest_edge_e0"));
        assert_eq!(piece(&cache, FAR, 2), Some("forest_forest_edge_e2"));
        let count = |cell| cache.get(cell).unwrap().iter().flatten().count();
        assert_eq!((count(EAST), count(FAR)), (1, 1));
    }

    #[test]
    fn bulldozing_a_forest_takes_the_neighbours_edge_piece_away() {
        let (mut w, c, mut cache) = two_forests();
        w.place(&c, FAR, "forest", o(1, 0)).unwrap();
        pairs_after(&w, &c, &mut cache, FAR);
        w.bulldoze(&c, FAR).unwrap();
        let changed = pairs_after(&w, &c, &mut cache, FAR);
        assert_eq!(changed, BTreeSet::from([EAST, FAR]));
        assert_eq!(piece(&cache, EAST, 0), None);
        assert!(cache.get(FAR).is_none());
    }

    #[test]
    fn undo_and_redo_re_evaluate_the_cell_and_its_neighbours() {
        let (mut w, c, mut cache) = two_forests();
        w.place(&c, FAR, "forest", o(1, 0)).unwrap();
        pairs_after(&w, &c, &mut cache, FAR);
        // Undo the place: the edge piece goes; redo it: the piece comes back.
        assert!(w.undo(&c));
        assert_eq!(
            pairs_after(&w, &c, &mut cache, FAR),
            BTreeSet::from([EAST, FAR])
        );
        assert_eq!(piece(&cache, EAST, 0), None);
        assert!(w.redo(&c));
        assert_eq!(
            pairs_after(&w, &c, &mut cache, FAR),
            BTreeSet::from([EAST, FAR])
        );
        assert_eq!(piece(&cache, EAST, 0), Some("forest_forest_edge_e0"));
        // Bulldoze, then undo the bulldoze (the tile returns) and redo it (it goes again).
        w.bulldoze(&c, FAR).unwrap();
        pairs_after(&w, &c, &mut cache, FAR);
        assert!(w.undo(&c));
        pairs_after(&w, &c, &mut cache, FAR);
        assert_eq!(piece(&cache, FAR, 2), Some("forest_forest_edge_e2"));
        assert!(w.redo(&c));
        pairs_after(&w, &c, &mut cache, FAR);
        assert_eq!(piece(&cache, EAST, 0), None);
    }

    #[test]
    fn a_third_forest_only_touches_its_own_seams() {
        let (mut w, c, mut cache) = two_forests();
        w.place(&c, FAR, "forest", o(1, 0)).unwrap();
        pairs_after(&w, &c, &mut cache, FAR);
        // EAST edge-5 neighbour (2, -1) touches EAST (its world edge 2) and FAR (world edge 1).
        let third = EAST.neighbor(5);
        w.place(&c, third, "forest", o(3, 0)).unwrap();
        let changed = pairs_after(&w, &c, &mut cache, third);
        assert_eq!(changed, BTreeSet::from([EAST, FAR, third]));
        assert_eq!(piece(&cache, EAST, 5), Some("forest_forest_edge_e5"));
        // third (rotation 3): world edge 2 faces EAST (local 5), world edge 1 faces FAR (local 4).
        assert_eq!(piece(&cache, third, 5), Some("forest_forest_edge_e5"));
        assert_eq!(piece(&cache, third, 4), Some("forest_forest_edge_e4"));
        assert_eq!(cache.get(third).unwrap().iter().flatten().count(), 2);
    }

    fn o(rotation: u8, level: u8) -> Orientation {
        Orientation::new(rotation, level)
    }

    /// Whether any edge of `def` at `cell` in `o` meets its neighbour as a cliff.
    fn cliffs(w: &World, c: &TileCatalog, cell: Axial, def: &TileDef, o: Orientation) -> bool {
        w.cliff_edges(c, cell, def, o).contains(&true)
    }

    /// A world whose cell `HOLLOW` (3, 0) is ringed by level-1 tiles: a raised meadow bridge at
    /// (1, 0) over the level-0 start meadow, then level-1 meadows round the hollow, except a
    /// level-1 straight road on each world edge in `roads` (its road running into the hollow).
    fn raised_ring(c: &TileCatalog, roads: &[u8]) -> World {
        let mut w = World::new(1);
        w.place(c, Axial::new(1, 0), "meadow", o(0, 1)).unwrap();
        let mut todo: Vec<u8> = (0..6).collect();
        while !todo.is_empty() {
            let before = todo.len();
            todo.retain(|&e| {
                let cell = HOLLOW.neighbor(e);
                let (id, k) = if roads.contains(&e) {
                    ("road_straight", e % 3)
                } else {
                    ("meadow", 0)
                };
                w.place(c, cell, id, o(k, 1)).is_err()
            });
            assert!(todo.len() < before, "ring cells {todo:?} never fit");
        }
        w
    }

    const HOLLOW: Axial = Axial { q: 3, r: 0 };

    /// Raised tiles (owner, 2026-11-13: "placing tiles like windmill should elevate them to
    /// matching edge heights"): every land tile the first elevation round deferred, dropped into a
    /// hollow ringed by level-1 ground, auto-orients to level 1 with no cliff (A38 O1), and R can
    /// still step a tile without roads down to the ground, where every edge is a cliff.
    #[test]
    fn deferred_land_tiles_take_the_raised_level_of_their_ring() {
        let c = catalog();
        for (id, roads) in [
            ("windmill", &[][..]),
            ("forest", &[]),
            ("oak", &[]),
            ("farm", &[]),
            ("wheat_field", &[]),
            ("house_cluster", &[1]),
            ("bakery", &[1, 4]),
        ] {
            let w = raised_ring(&c, roads);
            let d = def(&c, id);
            assert!(d.raisable, "{id}");
            let legal = w.legal_orientations(&c, HOLLOW, d);
            let first = *legal
                .first()
                .unwrap_or_else(|| panic!("{id}: no legal orientation"));
            assert_eq!(first.level, 1, "{id}: auto-orients to {first:?}");
            assert!(
                !cliffs(&w, &c, HOLLOW, d, first),
                "{id}: {first:?} has a cliff"
            );
            assert_eq!(w.auto_orient(&c, HOLLOW, d), Some(first), "{id}");
            // A road edge never meets a road on another level, so a road tile stays raised.
            let ground = legal.iter().find(|k| k.level == 0);
            if roads.is_empty() {
                let ground = ground.unwrap_or_else(|| panic!("{id}: no level-0 orientation"));
                assert!(
                    w.cliff_edges(&c, HOLLOW, d, *ground).iter().all(|&e| e),
                    "{id}: on the ground every edge is a cliff"
                );
            } else {
                assert_eq!(ground, None, "{id}: a road meets a road on its own level");
            }
            let mut placed = w.clone();
            placed.place(&c, HOLLOW, id, first).unwrap();
            assert_eq!(placed.tile(HOLLOW).unwrap().level, 1, "{id}");
        }
    }

    /// The owner's screenshot (2026-11-13): a windmill in a hollow with level-1 ground on three
    /// sides and open cells on the others takes level 1. Beside level-0 ground as well, every
    /// level has a cliff, and it still rises to the side with the most edges (owner O1).
    #[test]
    fn a_windmill_in_a_three_sided_hollow_takes_level_one() {
        let c = catalog();
        let mut w = World::new(1);
        for (q, r) in [(1, 0), (2, 0), (3, -1), (2, 1)] {
            w.place(&c, Axial::new(q, r), "meadow", o(0, 1)).unwrap();
        }
        let mill = def(&c, "windmill");
        let first = w.auto_orient(&c, HOLLOW, mill).unwrap();
        assert_eq!(first.level, 1);
        assert!(!cliffs(&w, &c, HOLLOW, mill, first));
        w.place(&c, Axial::new(3, 1), "meadow", o(0, 0)).unwrap();
        let mixed = w.auto_orient(&c, HOLLOW, mill).unwrap();
        assert_eq!(mixed.level, 1);
        assert_eq!(w.cliffs(&c, HOLLOW, mill, mixed).0, 1);
    }

    /// Owner O1 (2026-11-13): with a cliff on every level, the fewest cliff faces win, and a tie
    /// takes the higher level. Cliff-free orientations still prefer the ground.
    #[test]
    fn a_mixed_level_slot_takes_the_fewest_cliffs_then_the_higher_level() {
        let c = catalog();
        let mut w = World::new(1);
        let meadow = def(&c, "meadow");
        // EAST meets the level-0 start meadow; give it one level-1 neighbour too: one cliff either way.
        w.place(&c, Axial::new(1, -1), "meadow", o(0, 1)).unwrap();
        let tie = w.legal_orientations(&c, EAST, meadow);
        assert_eq!(w.cliffs(&c, EAST, meadow, tie[0]), (1, 2));
        assert_eq!(tie[0].level, 1);
        // A second level-0 neighbour makes level 0 the fewer-cliff side.
        w.place(&c, Axial::new(0, 1), "meadow", o(0, 0)).unwrap();
        let low = w.legal_orientations(&c, EAST, meadow);
        assert_eq!(low[0].level, 0);
        assert_eq!(w.cliffs(&c, EAST, meadow, low[0]).0, 1);
    }

    #[test]
    fn meadow_beside_meadow_defaults_to_level_zero() {
        let c = catalog();
        let w = World::new(1);
        let meadow = def(&c, "meadow");
        let legal = w.legal_orientations(&c, EAST, meadow);
        assert_eq!(legal[0].level, 0);
        assert!(!cliffs(&w, &c, EAST, meadow, legal[0]));
        assert!(legal.contains(&o(0, 1)));
        // EAST's world edge 3 faces the start meadow: one level up it is a cliff there.
        assert_eq!(
            w.cliff_edges(&c, EAST, meadow, o(0, 1)),
            [false, false, false, true, false, false]
        );
        // Every level-0 orientation comes before every raised one.
        let first_raised = legal.iter().position(|o| o.level > 0).unwrap();
        assert!(legal[..first_raised].iter().all(|o| o.level == 0));
        assert!(legal[first_raised..].iter().all(|o| o.level > 0));
    }

    #[test]
    fn level_three_is_the_cap() {
        let c = catalog();
        let mut w = World::new(1);
        let meadow = def(&c, "meadow");
        w.place(&c, EAST, "meadow", o(0, 3)).unwrap();
        let next = Axial::new(2, 0);
        let legal = w.legal_orientations(&c, next, meadow);
        // Its only neighbour stands at level 3, so level 3 is the cliff-free choice.
        assert_eq!(legal[0], o(0, 3));
        assert!(!cliffs(&w, &c, next, meadow, legal[0]));
        assert!(legal.iter().all(|o| o.level <= MAX_LEVEL));
        assert!(!w.fits(&c, next, meadow, o(0, 4)));
        assert_eq!(
            w.place(&c, next, "meadow", o(0, 4)),
            Err(PlaceError::LevelOutOfRange(4))
        );
        w.place(&c, next, "meadow", o(0, 3)).unwrap();
        assert_eq!(w.tile(next).unwrap().level, 3);
    }

    #[test]
    fn non_raisable_defs_only_offer_level_zero() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, EAST, "meadow", o(0, 1)).unwrap();
        let next = Axial::new(2, 0);
        let river = def(&c, "river_straight");
        assert!(!river.raisable);
        let legal = w.legal_orientations(&c, next, river);
        assert!(!legal.is_empty());
        assert!(legal.iter().all(|o| o.level == 0));
        // World edge 3 of `next` faces the raised meadow: never the river's edge, always a cliff.
        for &k in &legal {
            assert_ne!(river.edge(3, k.rotation).kind, SocketKind::River, "{k:?}");
            assert!(w.cliff_edges(&c, next, river, k)[3], "{k:?}");
        }
        assert_eq!(
            w.place(&c, next, "river_straight", o(legal[0].rotation, 1)),
            Err(PlaceError::LevelOutOfRange(1))
        );
    }

    #[test]
    fn road_cannot_cliff() {
        let c = catalog();
        let mut w = World::new(1);
        // Road on world edges 1 and 4, grass toward the start meadow: a cliff on the grass side.
        w.place(&c, EAST, "road_straight", o(1, 1)).unwrap();
        // BEYOND_END's world edge 4 faces the raised road end.
        let road = def(&c, "road_straight");
        let legal = w.legal_orientations(&c, BEYOND_END, road);
        assert!(!legal.is_empty());
        for o in &legal {
            assert_eq!(o.level, 1, "{o:?}");
            assert_eq!(road.edge(4, o.rotation).kind, SocketKind::Road, "{o:?}");
        }
        assert!(!w.fits(&c, BEYOND_END, road, Orientation::new(1, 0)));
    }

    /// Controller ruling: a stopped rail end at level 0 takes a raised tile without rail, and the
    /// step counts as a cliff, so the level-0 orientation comes first.
    #[test]
    fn stopped_rail_end_beside_a_raised_meadow_is_a_cliff() {
        let c = catalog();
        let w = station_world(&c);
        let meadow = def(&c, "meadow");
        assert!(w.fits(&c, BEYOND_END, meadow, o(0, 1)));
        assert!(w.cliff_edges(&c, BEYOND_END, meadow, o(0, 1))[4]);
        assert!(!w.cliff_edges(&c, BEYOND_END, meadow, o(0, 0))[4]);
        assert_eq!(w.auto_orient(&c, BEYOND_END, meadow), Some(o(0, 0)));
    }

    /// Rail at levels (owner, 2026-11-14): the straight, both bends and the junction are raisable,
    /// the station, level crossing and river rail bridge stay on the ground.
    #[test]
    fn rail_tiles_are_raisable_and_the_special_ones_stay_on_the_ground() {
        let c = catalog();
        for id in [
            "rail_straight",
            "rail_bend_60",
            "rail_bend_120",
            "rail_junction",
        ] {
            let d = def(&c, id);
            assert!(d.raisable, "{id}");
            assert!(level_allowed(d, 3), "{id}");
        }
        for id in ["rail_station", "rail_road_crossing", "river_rail_bridge"] {
            assert!(!def(&c, id).raisable, "{id}");
        }
    }

    /// A raised rail straight carries rail to a rail straight on its own level. A rail edge never
    /// meets rail on another level, so the ground piece is refused rather than drawn as a cliff.
    #[test]
    fn a_raised_rail_straight_meets_raised_rail_and_refuses_ground_rail() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, EAST, "rail_straight", o(1, 1)).unwrap();
        let next = EAST.neighbor(1);
        let straight = def(&c, "rail_straight");
        assert!(w.fits(&c, next, straight, o(1, 1)));
        assert!(!w.fits(&c, next, straight, o(1, 0)));
        w.place(&c, next, "rail_straight", o(1, 1)).unwrap();
        assert!(
            w.place(&c, next.neighbor(1), "rail_straight", o(1, 0))
                .is_err()
        );
    }

    /// The bends and the junction continue a raised straight on its own level and never on the
    /// ground, in at least one rotation.
    #[test]
    fn raised_bends_and_junction_continue_raised_rail() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, EAST, "rail_straight", o(1, 1)).unwrap();
        let next = EAST.neighbor(1);
        for id in ["rail_bend_60", "rail_bend_120", "rail_junction"] {
            let d = def(&c, id);
            let raised: Vec<u8> = (0..6).filter(|&k| w.fits(&c, next, d, o(k, 1))).collect();
            assert!(
                !raised.is_empty(),
                "{id}: no raised rotation continues the rail"
            );
            assert!(
                (0..6).all(|k| !w.fits(&c, next, d, o(k, 0))),
                "{id}: rail on the ground must not meet raised rail"
            );
        }
    }

    /// A raised rail's open end: raised grass beyond it has no cliff there, ground grass does, and
    /// the raised orientation is the one a placement picks.
    #[test]
    fn a_raised_rail_end_beside_raised_grass_is_not_a_cliff() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, EAST, "rail_straight", o(1, 1)).unwrap();
        let end = EAST.neighbor(1);
        let meadow = def(&c, "meadow");
        assert!(w.fits(&c, end, meadow, o(0, 1)));
        assert!(!cliffs(&w, &c, end, meadow, o(0, 1)));
        assert!(cliffs(&w, &c, end, meadow, o(0, 0)));
        assert_eq!(w.auto_orient(&c, end, meadow), Some(o(0, 1)));
    }

    /// A viaduct-like tile built from `rail_straight`: both rail ends stand `deck` levels up.
    fn viaduct(c: &TileCatalog, deck: u8) -> TileDef {
        let mut d = def(c, "rail_straight").clone();
        d.id = format!("test_viaduct_{deck}");
        d.raisable = false;
        for e in [0, 3] {
            d.edges[e].lane = d.edges[e]
                .lane
                .map(|l| crate::socket::LaneSpec { deck, ..l });
        }
        d
    }

    /// Rail at levels: a deck-2 viaduct fits only between rail standing at level 2 on both ends,
    /// needs both neighbours, and stands at level 0 only.
    #[test]
    fn a_viaduct_spans_between_two_raised_rail_tiles() {
        use std::collections::{BTreeMap, VecDeque};
        let c = catalog();
        let (north, mid, south) = (Axial::new(1, 0), Axial::new(1, 1), Axial::new(1, 2));
        let raised = |cell: Axial, level: u8| PlacedTile {
            def: "rail_straight".into(),
            rotation: 1,
            level,
            seed: cell_seed(1, cell),
        };
        let world = |tiles: &[(Axial, u8)]| {
            let map: BTreeMap<Axial, PlacedTile> = tiles
                .iter()
                .map(|&(cell, l)| (cell, raised(cell, l)))
                .collect();
            World::from_parts(1, map, VecDeque::new(), vec![])
        };
        let v = viaduct(&c, 2);
        let both = world(&[(north, 2), (south, 2)]);
        assert!(
            both.fits(&c, mid, &v, o(1, 0)),
            "both ends meet rail at level 2"
        );
        assert!(
            !both.fits(&c, mid, &v, o(1, 1)),
            "a viaduct stands at level 0 only"
        );
        assert!(
            !both.fits(&c, mid, &viaduct(&c, 1), o(1, 0)),
            "level 2 rail needs a deck of 2"
        );
        assert!(!both.fits(&c, mid, &viaduct(&c, 3), o(1, 0)));
        assert!(
            !world(&[(north, 2)]).fits(&c, mid, &v, o(1, 0)),
            "the far deck end has no rail to span to"
        );
        assert!(
            !world(&[(north, 2), (south, 1)]).fits(&c, mid, &v, o(1, 0)),
            "rail at level 1 is not at the deck's top"
        );
    }

    #[test]
    fn cycle_group_orders_cliff_free_first_and_wraps() {
        let c = catalog();
        let w = World::new(1);
        let walk = |members: &[&TileDef]| -> Vec<(String, Orientation)> {
            let seq: Vec<(String, Orientation)> = w
                .group_orientations(&c, EAST, members)
                .iter()
                .map(|(d, k)| (d.id.clone(), *k))
                .collect();
            let mut cur = seq[0].clone();
            let mut seen = vec![];
            for _ in 0..seq.len() {
                seen.push(cur.clone());
                let (d, k) = w
                    .cycle_group(&c, EAST, members, (&cur.0, cur.1), RotateDir::Forward)
                    .unwrap();
                cur = (d.id.clone(), k);
            }
            assert_eq!(cur, seen[0], "the walk wraps to its start");
            assert_eq!(seen, seq, "every entry once, in order");
            seq
        };
        // The Beach Curve (not raisable): every entry at level 0, headland then bay.
        let curve: Vec<&TileDef> = c.group_members("beach_curve");
        let seq = walk(&curve);
        assert!(seq.iter().all(|(_, k)| k.level == 0));
        // The real Hillside group: every legal (member, rotation, level) once, cliff-free entries
        // first, then the walk wraps (checked by `walk`).
        let hills: Vec<&TileDef> = c.group_members("hillside");
        assert_eq!(hills.len(), 12);
        let seq = walk(&hills);
        let mut distinct: Vec<&(String, Orientation)> = seq.iter().collect();
        distinct.sort_by_key(|(d, k)| (d.clone(), k.rotation, k.level));
        distinct.dedup();
        assert_eq!(distinct.len(), seq.len(), "no entry repeats");
        let expected: usize = hills
            .iter()
            .map(|d| w.group_orientations(&c, EAST, &[*d]).len())
            .sum();
        assert_eq!(
            seq.len(),
            expected,
            "every member's legal orientations appear"
        );
        assert!(
            seq.iter().any(|(_, k)| k.level > 0),
            "raised entries are in the cycle"
        );
        let cliff = |(d, k): &(String, Orientation)| cliffs(&w, &c, EAST, def(&c, d), *k);
        let first = seq.iter().position(cliff).unwrap();
        assert!(first > 0, "the cycle starts cliff-free");
        assert!(seq[..first].iter().all(|e| !cliff(e)));
        assert!(seq[first..].iter().all(cliff));
    }

    #[test]
    fn replace_to_another_level_is_a_change() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, EAST, "meadow", o(0, 0)).unwrap();
        assert_eq!(
            w.replace(&c, EAST, "meadow", o(3, 0)),
            Err(ReplaceError::Unchanged)
        );
        assert_eq!(
            w.replace(&c, EAST, "rail_station", o(0, 1)),
            Err(ReplaceError::LevelOutOfRange(1))
        );
        let old = w.replace(&c, EAST, "meadow", o(0, 2)).unwrap();
        assert_eq!(old.level, 0);
        assert_eq!(w.tile(EAST).unwrap().level, 2);
        assert!(w.undo(&c));
        assert_eq!(w.tile(EAST).unwrap().level, 0);
        let rots = w.replace_orientations(&c, EAST, def(&c, "meadow"));
        assert_eq!(rots[0].level, 0);
        assert_eq!(rots.len(), 24);
    }

    #[test]
    fn cover_cannot_be_bulldozed() {
        use crate::tunnel::tests::{catalog as cat, meadows, row};
        let c = cat();
        let mut w = row(&meadows(3), 0);
        let cover = Axial::new(2, 0);
        assert_eq!(w.can_bulldoze(&c, cover), Err(BulldozeError::TunnelCover));
        assert_eq!(w.bulldoze(&c, cover), Err(BulldozeError::TunnelCover));
        assert_eq!(w.tiles().len(), 5);
        // Take a portal away and the tunnel, and its protection, is gone.
        w.bulldoze(&c, Axial::new(4, 0)).unwrap();
        assert_eq!(w.bulldoze(&c, Axial::new(3, 0)).unwrap().def, "meadow");
    }

    #[test]
    fn cover_cannot_be_replaced() {
        use crate::tunnel::tests::{catalog as cat, meadows, row};
        let c = cat();
        let mut w = row(&meadows(3), 0);
        assert_eq!(
            w.replace(&c, Axial::new(2, 0), "meadow", o(0, 2)),
            Err(ReplaceError::TunnelCover)
        );
        assert!(!w.can_undo());
    }

    /// Codes are flat snake_case strings, one per closed-set member.
    #[test]
    fn refusal_codes_are_flat_strings() {
        assert_eq!(Refusal::InnerSlot.code(), "inner_slot",);
        assert_eq!(
            Refusal::Clash {
                mine: SocketKind::Grass,
                theirs: SocketKind::Beach
            }
            .code(),
            "clash_grass_beach",
        );
        assert_eq!(
            serde_json::to_value(Refusal::Missing {
                kind: SocketKind::Beach
            })
            .unwrap(),
            serde_json::json!("missing_beach"),
        );
        // The closed set: the four fixed codes, a missing_ code per kind, a clash_ code per pair.
        let codes = Refusal::codes();
        assert_eq!(
            codes.len(),
            4 + SocketKind::ALL.len() + SocketKind::ALL.len() * SocketKind::ALL.len()
        );
        assert_eq!(codes[0], "inner_slot");
        assert_eq!(codes[1], "deck_end");
        assert_eq!(codes[2], "footprint_edges");
        assert_eq!(codes[3], "no_footprint_room");
        assert_eq!(codes[4], "missing_grass");
        assert_eq!(codes.last().unwrap(), "clash_rail_rail");
    }

    /// Each refusal code fires where the mirror's wording for it does (ENH-002), on worlds built
    /// from legal placements alone.
    #[test]
    fn placement_refusal_classifies_each_reason() {
        let c = catalog();
        // Missing: an ocean tile has no edge that meets the start meadow's grass.
        let w = World::new(1);
        assert_eq!(
            w.placement_refusal(&c, Axial::new(1, 0), c.get("ocean").unwrap()),
            Some(Refusal::Missing {
                kind: SocketKind::Grass
            }),
        );
        // DeckEnd: rail meadows hold the slot's sides, so every viaduct rotation clashes, and the
        // least-clashing rotation's first clashing edge faces an empty cell.
        let mut w = World::new(1);
        for (q, r) in [(1, 0), (2, -1), (1, 1), (2, 1)] {
            w.place(&c, Axial::new(q, r), "meadow", o(0, 0)).unwrap();
        }
        assert_eq!(
            w.placement_refusal(&c, Axial::new(2, 0), c.get("rail_viaduct_l1").unwrap()),
            Some(Refusal::DeckEnd),
        );
        // InnerSlot: (2,1) touches no placed tile, so only a footprint tile can be there.
        let w = World::new(1);
        assert_eq!(
            w.placement_refusal(&c, Axial::new(2, 1), c.get("meadow").unwrap()),
            Some(Refusal::InnerSlot),
        );
        // NoFootprintRoom: far outside the world, a massif has no seven free cells beside it.
        let w = World::new(1);
        assert_eq!(
            w.placement_refusal(&c, Axial::new(7, 7), c.get("massif_granite").unwrap()),
            Some(Refusal::NoFootprintRoom),
        );
        // FootprintEdges: a beach past (1,0) holds room beside the world, but its coast runs the
        // wrong way for a massif member, which faces it as grass at every rotation.
        let mut w = World::new(1);
        w.place(&c, Axial::new(1, 0), "beach", o(5, 0)).unwrap();
        assert_eq!(
            w.placement_refusal(&c, Axial::new(2, 1), c.get("massif_granite").unwrap()),
            Some(Refusal::FootprintEdges),
        );
        // Clash: the road junction's road edges would dead-end into the two meadows, and its
        // grass edges keep the need kinds met, so the least-clashing rotation's first clashing
        // edge is a road edge facing grass.
        let mut w = World::new(1);
        w.place(&c, Axial::new(1, 0), "meadow", o(0, 0)).unwrap();
        assert_eq!(
            w.placement_refusal(&c, Axial::new(0, 1), c.get("road_junction").unwrap()),
            Some(Refusal::Clash {
                mine: SocketKind::Road,
                theirs: SocketKind::Grass,
            }),
        );
    }

    /// The classification agrees with the mirror's wording order: the same slot's needs name the
    /// missing kind, and a fitting tile has no refusal.
    #[test]
    fn placement_refusal_is_none_where_the_tile_fits() {
        let c = catalog();
        let w = World::new(1);
        assert_eq!(
            w.placement_refusal(&c, Axial::new(1, 0), c.get("meadow").unwrap()),
            None
        );
        // A forest also fits beside the meadow, so its refusal is None too.
        assert_eq!(
            w.placement_refusal(&c, Axial::new(1, 0), c.get("forest").unwrap()),
            None
        );
    }
}
