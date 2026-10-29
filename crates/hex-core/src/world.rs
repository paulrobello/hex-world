//! The placed world: legal slots, rotation choice, edge status, placement, bulldoze and
//! undo/redo (PRD §4.1–4.2).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::axial::Axial;
use crate::catalog::{START_TILE, TileCatalog};
use crate::socket::HeightMeet;
use crate::tile::{EdgePieces, KitRef, TileDef, opposite_edge};
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

/// Direction for cycling through distinct legal tile orientations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RotateDir {
    Forward,
    Back,
}

/// A placement refusal. Failed placements leave the world and history unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaceError {
    /// The target cell already holds a tile.
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    seed: u64,
    tiles: BTreeMap<Axial, PlacedTile>,
    history: History,
}

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
        }
    }

    /// Rebuilds a world from saved parts. No rule checks run here: [`crate::save`] validates the
    /// result against the catalog before handing it out.
    pub(crate) fn from_parts(
        seed: u64,
        tiles: BTreeMap<Axial, PlacedTile>,
        undo: VecDeque<Action>,
        redo: Vec<Action>,
    ) -> Self {
        Self {
            seed,
            tiles,
            history: History { undo, redo },
        }
    }

    /// The undo stack (oldest first) and the redo stack (the next redo last).
    pub(crate) fn history_stacks(&self) -> (&VecDeque<Action>, &[Action]) {
        (&self.history.undo, &self.history.redo)
    }

    /// The world seed from which placed tiles derive deterministic seeds.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Placed tiles ordered by axial coordinate.
    pub fn tiles(&self) -> &BTreeMap<Axial, PlacedTile> {
        &self.tiles
    }

    /// The tile at `cell`, or `None` when the cell is empty.
    pub fn tile(&self, cell: Axial) -> Option<&PlacedTile> {
        self.tiles.get(&cell)
    }

    /// Free cells adjacent to at least one placed tile, sorted.
    pub fn frontier(&self) -> Vec<Axial> {
        let free: BTreeSet<Axial> = self
            .tiles
            .keys()
            .flat_map(|c| (0..6).map(move |e| c.neighbor(e)))
            .filter(|n| !self.tiles.contains_key(n))
            .collect();
        free.into_iter().collect()
    }

    /// Empty and adjacent to the world.
    pub fn is_slot(&self, cell: Axial) -> bool {
        !self.tiles.contains_key(&cell)
            && (0..6).any(|e| self.tiles.contains_key(&cell.neighbor(e)))
    }

    /// Glow for each world edge of `def` ghosted at `cell` in orientation `o`. A neighbor whose
    /// definition is missing from the catalog counts as incompatible.
    pub fn edge_status(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
        o: Orientation,
    ) -> [EdgeStatus; 6] {
        std::array::from_fn(|w| {
            let w = w as u8;
            let Some(placed) = self.tiles.get(&cell.neighbor(w)) else {
                return EdgeStatus::Neutral;
            };
            let Some(neighbor) = catalog.get(&placed.def) else {
                return EdgeStatus::Incompatible;
            };
            if catalog.placement_meets(
                def,
                o.rotation,
                o.level,
                w,
                neighbor,
                placed.rotation,
                placed.level,
            ) {
                EdgeStatus::Compatible
            } else {
                EdgeStatus::Incompatible
            }
        })
    }

    /// World edges of `def` at `cell` in orientation `o` that meet a placed neighbour as a cliff:
    /// the pair is accepted ([`TileCatalog::edges_meet`]) but its corners differ in absolute
    /// height. A stopped rail end beside a raised tile counts: the stop meets it as lane-free
    /// ground, so the step is a cliff face.
    pub fn cliff_edges(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
        o: Orientation,
    ) -> [bool; 6] {
        std::array::from_fn(|w| {
            let w = w as u8;
            let Some(placed) = self.tiles.get(&cell.neighbor(w)) else {
                return false;
            };
            let Some(n) = catalog.get(&placed.def) else {
                return false;
            };
            let (r, l, nr, nl) = (o.rotation, o.level, placed.rotation, placed.level);
            match catalog.meet_kind(def, r, l, w, n, nr, nl) {
                HeightMeet::Seamless => false,
                HeightMeet::Cliff | HeightMeet::Refused => {
                    catalog.edges_meet(def, r, l, w, n, nr, nl)
                }
            }
        })
    }

    /// Whether `def` may be placed at `cell` in orientation `o`.
    pub fn fits(&self, catalog: &TileCatalog, cell: Axial, def: &TileDef, o: Orientation) -> bool {
        o.rotation < 6
            && level_allowed(def, o.level)
            && self.is_slot(cell)
            && !self
                .edge_status(catalog, cell, def, o)
                .contains(&EdgeStatus::Incompatible)
    }

    /// The orientations `test` accepts, cliff-free first, then by level, then by rotation, so
    /// placement never defaults to a cliff. A tile that is not raisable stands at level 0 only.
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

    /// [`World::ordered_orientations`] with each orientation's "has a cliff edge" flag, computed
    /// once per orientation.
    fn ordered_with_cliff(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
        test: impl Fn(Orientation) -> bool,
    ) -> Vec<(bool, Orientation)> {
        let mut out: Vec<(bool, Orientation)> = (0..=MAX_LEVEL)
            .filter(|&l| level_allowed(def, l))
            .flat_map(|l| (0..6).map(move |k| Orientation::new(k, l)))
            .filter(|&o| test(o))
            .map(|o| (self.cliff_edges(catalog, cell, def, o).contains(&true), o))
            .collect();
        out.sort_by_key(|&(cliff, o)| (cliff, o.level, o.rotation));
        out
    }

    /// Orientations in which `def` fits at `cell`: cliff-free first, then by level, then by
    /// rotation. Empty when `cell` is not a slot.
    pub fn legal_orientations(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
    ) -> Vec<Orientation> {
        if !self.is_slot(cell) {
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
    /// entry cliff-free first, then by level, then member after member (owner, 2026-10-20: the
    /// Beach Curve's headland orientations, then its bay ones), then by rotation.
    pub fn group_orientations<'a>(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        members: &[&'a TileDef],
    ) -> Vec<(&'a TileDef, Orientation)> {
        let mut seq: Vec<(&'a TileDef, Orientation)> = vec![];
        let mut keys: Vec<(bool, u8, usize, u8)> = vec![];
        for (i, &d) in members.iter().enumerate() {
            let mut reps: Vec<(bool, Orientation)> = vec![];
            let legal = if self.is_slot(cell) {
                self.ordered_with_cliff(catalog, cell, d, |o| self.fits(catalog, cell, d, o))
            } else {
                vec![]
            };
            for (cliff, o) in legal {
                if !reps
                    .iter()
                    .any(|(_, r)| r.level == o.level && d.same_orientation(r.rotation, o.rotation))
                {
                    reps.push((cliff, o));
                }
            }
            for (cliff, o) in reps {
                seq.push((d, o));
                keys.push((cliff, o.level, i, o.rotation));
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
            let key = |o: Orientation| {
                let cliff = self.cliff_edges(catalog, cell, d, o).contains(&true);
                (cliff, o.level, o.rotation)
            };
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

    /// World edges of `def` at `cell` in orientation `o` whose neighbor does not fit.
    fn clashes(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
        o: Orientation,
    ) -> Vec<u8> {
        self.edge_status(catalog, cell, def, o)
            .iter()
            .enumerate()
            .filter(|(_, s)| **s == EdgeStatus::Incompatible)
            .map(|(w, _)| w as u8)
            .collect()
    }

    /// Places `def_id` at `cell` in orientation `o` if every touching edge is compatible (PRD §4.1
    /// step 4).
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
        if self.tiles.contains_key(&cell) {
            return Err(PlaceError::Occupied);
        }
        if !self.is_slot(cell) {
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
        self.record(Action::Place { cell, tile });
        Ok(())
    }

    /// Orientations in which `def` may take the place of the tile at `cell`, ordered as
    /// [`World::legal_orientations`]: every touching edge fits its neighbor. Empty when `cell`
    /// holds no tile. Swapping a tile keeps the world connected, so only the edges are checked.
    pub fn replace_orientations(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
    ) -> Vec<Orientation> {
        if !self.tiles.contains_key(&cell) {
            return vec![];
        }
        self.ordered_orientations(catalog, cell, def, |o| {
            self.clashes(catalog, cell, def, o).is_empty()
        })
    }

    /// Swaps the tile at `cell` for `def_id` in orientation `o` as one undoable action. The same
    /// tile in the same orientation (equal sockets on every edge) at the same level is
    /// [`ReplaceError::Unchanged`] and records nothing. Returns the tile that was there.
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
        let Some(from) = self.tiles.get(&cell).cloned() else {
            return Err(ReplaceError::Empty);
        };
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
        self.record(Action::Replace {
            cell,
            from: from.clone(),
            to,
        });
        Ok(from)
    }

    /// Whether the tile at `cell` may be removed: not the last tile, and the rest stays connected.
    pub fn can_bulldoze(&self, catalog: &TileCatalog, cell: Axial) -> Result<(), BulldozeError> {
        if !self.tiles.contains_key(&cell) {
            return Err(BulldozeError::Empty);
        }
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

    /// Removes and returns a tile as one undoable action, preserving a nonempty connected world.
    pub fn bulldoze(
        &mut self,
        catalog: &TileCatalog,
        cell: Axial,
    ) -> Result<PlacedTile, BulldozeError> {
        self.can_bulldoze(catalog, cell)?;
        let tile = self
            .tiles
            .remove(&cell)
            .expect("can_bulldoze checked the cell");
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
    /// checks run. Returns false when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        let Some(action) = self.history.undo.pop_back() else {
            return false;
        };
        match &action {
            Action::Place { cell, .. } => {
                self.tiles.remove(cell);
            }
            Action::Bulldoze { cell, tile } => {
                self.tiles.insert(*cell, tile.clone());
            }
            Action::Replace { cell, from, .. } => {
                self.tiles.insert(*cell, from.clone());
            }
        }
        self.history.redo.push(action);
        true
    }

    /// Re-applies the latest undone action. Returns false when there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        let Some(action) = self.history.redo.pop() else {
            return false;
        };
        match &action {
            Action::Place { cell, tile } => {
                self.tiles.insert(*cell, tile.clone());
            }
            Action::Bulldoze { cell, .. } => {
                self.tiles.remove(cell);
            }
            Action::Replace { cell, to, .. } => {
                self.tiles.insert(*cell, to.clone());
            }
        }
        self.history.undo.push_back(action);
        true
    }

    /// Pushes a new action: clears redo and drops the oldest action beyond [`HISTORY_DEPTH`].
    fn record(&mut self, action: Action) {
        self.history.redo.clear();
        self.history.undo.push_back(action);
        while self.history.undo.len() > HISTORY_DEPTH {
            self.history.undo.pop_front();
        }
    }

    /// Flood fill over the 6-neighborhood of every tile except `removed`.
    fn connected_without(&self, removed: Axial) -> bool {
        let Some(&start) = self.tiles.keys().find(|&&c| c != removed) else {
            return true;
        };
        let mut seen: BTreeSet<Axial> = BTreeSet::from([start]);
        let mut stack = vec![start];
        while let Some(cell) = stack.pop() {
            for e in 0..6 {
                let n = cell.neighbor(e);
                if n != removed && self.tiles.contains_key(&n) && seen.insert(n) {
                    stack.push(n);
                }
            }
        }
        seen.len() == self.tiles.len() - 1
    }
}

/// Pair edge pieces of the tile at `cell`, by local edge (decision D3, `data/sockets.ron`
/// `pair_edge_pieces`). `at(cell)` is the definition and rotation of the tile placed there. An edge
/// gets a piece when the (own socket, neighbour socket) pair across it has an entry; its corner
/// flags say whether the local edges either side carry the same pair. A tile that lists its edge
/// pieces explicitly gets none.
pub fn pair_edge_pieces<'a>(
    catalog: &'a TileCatalog,
    cell: Axial,
    at: impl Fn(Axial) -> Option<(&'a TileDef, u8)>,
) -> [Option<KitRef>; 6] {
    let Some((def, rot)) = at(cell) else {
        return Default::default();
    };
    if def.edge_pieces != EdgePieces::Derived {
        return Default::default();
    }
    let sockets = catalog.sockets();
    let pair = |e: u8| {
        let world = (e + rot) % 6;
        let (n, nrot) = at(cell.neighbor(world))?;
        Some((
            def.edges[e as usize].kind,
            n.edge(opposite_edge(world), nrot).kind,
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
    /// tile counts as changed).
    pub fn update<'a>(
        &mut self,
        catalog: &'a TileCatalog,
        changed: impl IntoIterator<Item = Axial>,
        at: impl Fn(Axial) -> Option<(&'a TileDef, u8)>,
    ) -> BTreeSet<Axial> {
        let mut dirty = BTreeSet::new();
        for c in changed {
            dirty.insert(c);
            dirty.extend((0..6).map(|e| c.neighbor(e)));
        }
        let mut out = BTreeSet::new();
        for c in dirty {
            let new = at(c).map(|_| pair_edge_pieces(catalog, c, &at));
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
mod tests {
    use super::*;
    use crate::socket::{EdgeProfile, Side, SocketKind};

    fn catalog() -> TileCatalog {
        TileCatalog::builtin().unwrap_or_else(|e| panic!("{e}"))
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
        assert!(w.undo() && w.tile(BEYOND_END).is_some());
        assert!(w.undo() && w.tile(BEYOND_END).is_none());
        assert!(w.redo() && w.redo() && w.tile(BEYOND_END).is_none());
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
        assert!(v.undo());
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
        assert!(c.placement_meets(meadow, 0, 0, 3, straight, 0, 0));
        assert!(c.edges_meet(straight, 0, 0, 0, meadow, 0, 0));
        assert!(!c.placement_meets(meadow, 0, 0, 3, &stub, 0, 0));
        assert!(!c.edges_meet(&stub, 0, 0, 0, meadow, 0, 0));
        // A rail tile never turns its end toward the meadow.
        assert!(!c.placement_meets(straight, 0, 0, 0, meadow, 0, 0));
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
                    let meet = c.edges_meet(a, ra, 0, w, b, rb, 0);
                    assert_eq!(
                        meet,
                        c.edges_meet(b, rb, 0, opposite_edge(w), a, ra, 0),
                        "{seam}"
                    );
                    assert!(meet || !c.placement_meets(a, ra, 0, w, b, rb, 0), "{seam}");
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
            .map(|d| d.id.clone())
            .filter(|id| id != "ocean")
            .filter(|id| {
                c.get(id)
                    .is_some_and(|d| d.edges.iter().any(|e| e.profile == EdgeProfile::Flat))
            })
            .collect();
        // A tile with no Flat edge (hillside 010101) cannot face the level meadow, so it never fits beside it.
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
        // The straight railway: like road_straight, but not raisable, so it stays on level 0.
        let road = def(&c, "rail_straight");
        assert!(!road.raisable);
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
        // Forest: every edge alike and not raisable, so one orientation on level 0.
        let forest = def(&c, "forest");
        assert!(!forest.raisable);
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
        assert!(w.undo());
        assert_eq!(w.tiles(), World::new(1).tiles());
        assert!(!w.undo(), "the start tile is not undoable");
        assert!(w.redo());
        assert_eq!(w, placed);
        assert!(!w.redo());
    }

    #[test]
    fn undo_restores_a_bulldozed_tile_exactly() {
        let c = catalog();
        let mut w = World::new(9);
        w.place(&c, EAST, "road_straight", o(2, 0)).unwrap();
        let before = w.tiles().clone();
        w.bulldoze(&c, EAST).unwrap();
        assert!(w.tile(EAST).is_none());
        assert!(w.undo());
        assert_eq!(w.tiles(), &before);
        assert!(w.redo());
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
            assert!(w.undo());
            assert_eq!(w.tiles(), want);
        }
        assert!(!w.undo());
        for want in states.iter().skip(1) {
            assert!(w.redo());
            assert_eq!(w.tiles(), want);
        }
    }

    #[test]
    fn a_new_action_clears_redo() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, EAST, "meadow", o(0, 0)).unwrap();
        w.undo();
        assert!(w.can_redo());
        w.place(&c, Axial::new(-1, 0), "meadow", o(0, 0)).unwrap();
        assert!(!w.can_redo());
        assert!(!w.redo());
    }

    #[test]
    fn failed_actions_do_not_touch_history() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, EAST, "meadow", o(0, 0)).unwrap();
        w.undo();
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
        while w.undo() {
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
        assert!(w.undo());
        assert_eq!(w.tiles(), &placed, "one undo restores the meadow");
        assert!(w.can_undo() && w.can_redo());
        assert!(w.redo());
        assert_eq!(w, replaced, "redo puts the forest back at rotation 3");
        // Undo twice: the replace, then the placement.
        assert!(w.undo() && w.undo());
        assert_eq!(w.tiles(), World::new(1).tiles());
        // A new action after an undone replace clears its redo.
        assert!(w.redo() && w.redo());
        assert!(w.undo());
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
        w.undo();
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
        let at = |cell: Axial| {
            w.tile(cell)
                .and_then(|t| Some((c.get(&t.def)?, t.rotation)))
        };
        let out = cache.update(c, [changed], at);
        let mut full = PairPieces::default();
        full.update(c, w.tiles().keys().copied().collect::<Vec<_>>(), at);
        assert_eq!(
            *cache, full,
            "incremental pair pieces differ from a full evaluation"
        );
        out
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
        assert!(w.undo());
        assert_eq!(
            pairs_after(&w, &c, &mut cache, FAR),
            BTreeSet::from([EAST, FAR])
        );
        assert_eq!(piece(&cache, EAST, 0), None);
        assert!(w.redo());
        assert_eq!(
            pairs_after(&w, &c, &mut cache, FAR),
            BTreeSet::from([EAST, FAR])
        );
        assert_eq!(piece(&cache, EAST, 0), Some("forest_forest_edge_e0"));
        // Bulldoze, then undo the bulldoze (the tile returns) and redo it (it goes again).
        w.bulldoze(&c, FAR).unwrap();
        pairs_after(&w, &c, &mut cache, FAR);
        assert!(w.undo());
        pairs_after(&w, &c, &mut cache, FAR);
        assert_eq!(piece(&cache, FAR, 2), Some("forest_forest_edge_e2"));
        assert!(w.redo());
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
            w.replace(&c, EAST, "forest", o(0, 1)),
            Err(ReplaceError::LevelOutOfRange(1))
        );
        let old = w.replace(&c, EAST, "meadow", o(0, 2)).unwrap();
        assert_eq!(old.level, 0);
        assert_eq!(w.tile(EAST).unwrap().level, 2);
        assert!(w.undo());
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
}
