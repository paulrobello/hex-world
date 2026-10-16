//! The placed world: legal slots, rotation choice, edge status, placement, bulldoze and
//! undo/redo (PRD §4.1–4.2).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::axial::Axial;
use crate::catalog::{START_TILE, TileCatalog};
use crate::tile::{EdgePieces, KitRef, TileDef, opposite_edge};

/// Undo depth (PRD §4.2 requires at least 100). Older actions are dropped first.
pub const HISTORY_DEPTH: usize = 128;

/// One placed tile: its definition id, rotation `k` (local edge `e` faces world edge `e + k`) and
/// the seed its scatter and agents derive from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacedTile {
    pub def: String,
    pub rotation: u8,
    pub seed: u64,
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
}

impl fmt::Display for BulldozeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BulldozeError::Empty => write!(f, "no tile here"),
            BulldozeError::LastTile => write!(f, "the last tile cannot be removed"),
            BulldozeError::WouldDisconnect => write!(f, "removing this tile splits the world"),
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
    /// World edges whose neighbor socket does not fit.
    Incompatible(Vec<u8>),
    /// The same tile in the same orientation is already there, so nothing would change.
    Unchanged,
}

impl fmt::Display for ReplaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReplaceError::Empty => write!(f, "no tile here"),
            ReplaceError::UnknownTile(id) => write!(f, "unknown tile {id:?}"),
            ReplaceError::BadRotation(k) => write!(f, "rotation {k} is not 0..6"),
            ReplaceError::Incompatible(edges) => write!(f, "edges {edges:?} do not fit"),
            ReplaceError::Unchanged => write!(f, "that tile is already here"),
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

    /// Glow for each world edge of `def` ghosted at `cell` with `rotation`. A neighbor whose
    /// definition is missing from the catalog counts as incompatible.
    pub fn edge_status(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
        rotation: u8,
    ) -> [EdgeStatus; 6] {
        std::array::from_fn(|w| {
            let w = w as u8;
            let Some(placed) = self.tiles.get(&cell.neighbor(w)) else {
                return EdgeStatus::Neutral;
            };
            let Some(neighbor) = catalog.get(&placed.def) else {
                return EdgeStatus::Incompatible;
            };
            let theirs = neighbor.edge(opposite_edge(w), placed.rotation);
            if catalog
                .sockets()
                .edges_compatible(def.edge(w, rotation), theirs)
            {
                EdgeStatus::Compatible
            } else {
                EdgeStatus::Incompatible
            }
        })
    }

    /// Whether `def` may be placed at `cell` with `rotation`.
    pub fn fits(&self, catalog: &TileCatalog, cell: Axial, def: &TileDef, rotation: u8) -> bool {
        rotation < 6
            && self.is_slot(cell)
            && !self
                .edge_status(catalog, cell, def, rotation)
                .contains(&EdgeStatus::Incompatible)
    }

    /// Rotations 0..6 at which `def` fits at `cell`, ascending. Empty when `cell` is not a slot.
    pub fn legal_rotations(&self, catalog: &TileCatalog, cell: Axial, def: &TileDef) -> Vec<u8> {
        if !self.is_slot(cell) {
            return vec![];
        }
        (0..6)
            .filter(|&k| self.fits(catalog, cell, def, k))
            .collect()
    }

    /// Tile types with at least one legal rotation at `cell`, in catalog order.
    pub fn inventory<'a>(&self, catalog: &'a TileCatalog, cell: Axial) -> Vec<&'a TileDef> {
        catalog
            .tiles()
            .iter()
            .filter(|def| !self.legal_rotations(catalog, cell, def).is_empty())
            .collect()
    }

    /// The rotation the ghost snaps to when `cell` is hovered: the first legal one.
    pub fn auto_rotate(&self, catalog: &TileCatalog, cell: Axial, def: &TileDef) -> Option<u8> {
        self.legal_rotations(catalog, cell, def).first().copied()
    }

    /// The first rotation of each distinct legal orientation at `cell`, ascending. Rotations that
    /// give every world edge an equal socket (a symmetric tile turned by its symmetry) count once
    /// (PRD §4.7 Q6), so this is what R steps through and what "Rotation 2 of 3" counts.
    pub fn distinct_legal_rotations(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
    ) -> Vec<u8> {
        let mut out: Vec<u8> = vec![];
        for k in self.legal_rotations(catalog, cell, def) {
            if !out.iter().any(|&r| def.same_orientation(r, k)) {
                out.push(k);
            }
        }
        out
    }

    /// The next distinct legal orientation after `current` in `dir`, wrapping, as that
    /// orientation's first rotation, so every step changes what the ghost shows. From an illegal
    /// `current` it is the orientation of the next legal rotation in `dir`. Returns `current`'s own
    /// orientation when it is the only legal one, and `None` when none is legal.
    pub fn cycle_rotation(
        &self,
        catalog: &TileCatalog,
        cell: Axial,
        def: &TileDef,
        current: u8,
        dir: RotateDir,
    ) -> Option<u8> {
        let reps = self.distinct_legal_rotations(catalog, cell, def);
        let rep_of = |k: u8| reps.iter().position(|&r| def.same_orientation(r, k));
        let n = reps.len();
        if let Some(i) = rep_of(current % 6) {
            return Some(match dir {
                RotateDir::Forward => reps[(i + 1) % n],
                RotateDir::Back => reps[(i + n - 1) % n],
            });
        }
        (1..=6u8)
            .map(|step| match dir {
                RotateDir::Forward => (current % 6 + step) % 6,
                RotateDir::Back => (current % 6 + 6 - step) % 6,
            })
            .find_map(rep_of)
            .map(|i| reps[i])
    }

    /// Places `def_id` at `cell` if every touching edge is compatible (PRD §4.1 step 4).
    pub fn place(
        &mut self,
        catalog: &TileCatalog,
        cell: Axial,
        def_id: &str,
        rotation: u8,
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
        if rotation >= 6 {
            return Err(PlaceError::BadRotation(rotation));
        }
        let bad: Vec<u8> = self
            .edge_status(catalog, cell, def, rotation)
            .iter()
            .enumerate()
            .filter(|(_, s)| **s == EdgeStatus::Incompatible)
            .map(|(w, _)| w as u8)
            .collect();
        if !bad.is_empty() {
            return Err(PlaceError::Incompatible(bad));
        }
        let tile = PlacedTile {
            def: def_id.to_owned(),
            rotation,
            seed: cell_seed(self.seed, cell),
        };
        self.tiles.insert(cell, tile.clone());
        self.record(Action::Place { cell, tile });
        Ok(())
    }

    /// Rotations 0..6 at which `def` may take the place of the tile at `cell`, ascending: every
    /// touching edge fits its neighbor. Empty when `cell` holds no tile. Swapping a tile keeps the
    /// world connected, so only the edges are checked.
    pub fn replace_rotations(&self, catalog: &TileCatalog, cell: Axial, def: &TileDef) -> Vec<u8> {
        if !self.tiles.contains_key(&cell) {
            return vec![];
        }
        (0..6)
            .filter(|&k| {
                !self
                    .edge_status(catalog, cell, def, k)
                    .contains(&EdgeStatus::Incompatible)
            })
            .collect()
    }

    /// Swaps the tile at `cell` for `def_id` at `rotation` as one undoable action. The same tile
    /// in the same orientation (equal sockets on every edge) is [`ReplaceError::Unchanged`] and
    /// records nothing. Returns the tile that was there.
    pub fn replace(
        &mut self,
        catalog: &TileCatalog,
        cell: Axial,
        def_id: &str,
        rotation: u8,
    ) -> Result<PlacedTile, ReplaceError> {
        let def = catalog
            .get(def_id)
            .ok_or_else(|| ReplaceError::UnknownTile(def_id.to_owned()))?;
        let Some(from) = self.tiles.get(&cell).cloned() else {
            return Err(ReplaceError::Empty);
        };
        if rotation >= 6 {
            return Err(ReplaceError::BadRotation(rotation));
        }
        if from.def == def_id && def.same_orientation(from.rotation, rotation) {
            return Err(ReplaceError::Unchanged);
        }
        let bad: Vec<u8> = self
            .edge_status(catalog, cell, def, rotation)
            .iter()
            .enumerate()
            .filter(|(_, s)| **s == EdgeStatus::Incompatible)
            .map(|(w, _)| w as u8)
            .collect();
        if !bad.is_empty() {
            return Err(ReplaceError::Incompatible(bad));
        }
        let to = PlacedTile {
            def: def_id.to_owned(),
            rotation,
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
    pub fn can_bulldoze(&self, cell: Axial) -> Result<(), BulldozeError> {
        if !self.tiles.contains_key(&cell) {
            return Err(BulldozeError::Empty);
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
    pub fn bulldoze(&mut self, cell: Axial) -> Result<PlacedTile, BulldozeError> {
        self.can_bulldoze(cell)?;
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
    use crate::socket::{Side, SocketKind};

    fn catalog() -> TileCatalog {
        TileCatalog::builtin().unwrap_or_else(|e| panic!("{e}"))
    }

    fn def<'a>(c: &'a TileCatalog, id: &str) -> &'a TileDef {
        c.get(id).unwrap()
    }

    /// East neighbor of the origin; its world edge 3 faces the start meadow.
    const EAST: Axial = Axial { q: 1, r: 0 };

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
        w.place(&c, EAST, "meadow", 0).unwrap();
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
            w.legal_rotations(&c, EAST, def(&c, "road_straight")),
            vec![1, 2, 4, 5]
        );
        // A beach edge never faces the meadow (owner O1): only its grass edges 3 and 4 may.
        assert_eq!(w.legal_rotations(&c, EAST, def(&c, "beach")), vec![0, 5]);
        assert!(w.legal_rotations(&c, EAST, def(&c, "ocean")).is_empty());
        assert_eq!(w.legal_rotations(&c, EAST, def(&c, "meadow")).len(), 6);
    }

    #[test]
    fn non_slots_have_no_legal_rotations_or_inventory() {
        let c = catalog();
        let w = World::new(1);
        let far = Axial::new(5, 5);
        assert!(
            w.legal_rotations(&c, Axial::ORIGIN, def(&c, "meadow"))
                .is_empty()
        );
        assert!(w.legal_rotations(&c, far, def(&c, "meadow")).is_empty());
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
            .collect();
        assert_eq!(ids(&w, EAST), all_but_ocean);

        // A road end now points at EAST's world edge 4, next to the meadow on its edge 3: only
        // tiles with a road edge right after a grass-compatible edge fit. The bridge's road edges
        // follow river edges, so it does not.
        w.place(&c, Axial::new(1, -1), "road_straight", 1).unwrap();
        assert_eq!(
            ids(&w, EAST),
            [
                "house_cluster",
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
        assert_eq!(w.auto_rotate(&c, EAST, def(&c, "road_straight")), Some(1));
        assert_eq!(w.auto_rotate(&c, EAST, def(&c, "ocean")), None);
    }

    #[test]
    fn distinct_orientations_count_a_symmetric_turn_once() {
        let c = catalog();
        let w = World::new(1);
        let road = def(&c, "road_straight");
        // A straight road turned 180° shows the same sockets: 1 and 4, 2 and 5 are one orientation.
        assert_eq!(w.legal_rotations(&c, EAST, road), vec![1, 2, 4, 5]);
        assert!(road.same_orientation(1, 4) && road.same_orientation(2, 5));
        assert!(!road.same_orientation(1, 2));
        assert_eq!(w.distinct_legal_rotations(&c, EAST, road), vec![1, 2]);
        let meadow = def(&c, "meadow");
        assert_eq!(w.legal_rotations(&c, EAST, meadow).len(), 6);
        assert_eq!(w.distinct_legal_rotations(&c, EAST, meadow), vec![0]);
        assert!(
            w.distinct_legal_rotations(&c, EAST, def(&c, "ocean"))
                .is_empty()
        );
    }

    #[test]
    fn cycling_walks_distinct_orientations_and_wraps() {
        let c = catalog();
        let w = World::new(1);
        let road = def(&c, "road_straight");
        let next = |k, dir| w.cycle_rotation(&c, EAST, road, k, dir);
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
            w.cycle_rotation(&c, EAST, def(&c, "ocean"), 0, RotateDir::Forward),
            None
        );
    }

    #[test]
    fn cycling_a_fully_symmetric_tile_stays_on_its_one_orientation() {
        let c = catalog();
        let w = World::new(1);
        let meadow = def(&c, "meadow");
        for dir in [RotateDir::Forward, RotateDir::Back] {
            assert_eq!(w.cycle_rotation(&c, EAST, meadow, 0, dir), Some(0));
            assert_eq!(w.cycle_rotation(&c, EAST, meadow, 3, dir), Some(0));
        }
    }

    #[test]
    fn cycling_a_single_legal_rotation_stays_put() {
        let c = catalog();
        let mut w = World::new(1);
        // House road edge must face the road end on EAST's world edge 4: only rotation 4.
        w.place(&c, Axial::new(1, -1), "road_straight", 1).unwrap();
        let house = def(&c, "house_cluster");
        assert_eq!(w.legal_rotations(&c, EAST, house), vec![4]);
        for dir in [RotateDir::Forward, RotateDir::Back] {
            assert_eq!(w.cycle_rotation(&c, EAST, house, 4, dir), Some(4));
            assert_eq!(w.cycle_rotation(&c, EAST, house, 0, dir), Some(4));
        }
    }

    #[test]
    fn edge_status_glows_each_world_edge() {
        let c = catalog();
        let w = World::new(1);
        let road = def(&c, "road_straight");
        let s = w.edge_status(&c, EAST, road, 0);
        assert_eq!(s[3], EdgeStatus::Incompatible);
        assert!([0, 1, 2, 4, 5].iter().all(|&e| s[e] == EdgeStatus::Neutral));
        let s = w.edge_status(&c, EAST, road, 1);
        assert_eq!(s[3], EdgeStatus::Compatible);
    }

    #[test]
    fn roads_join_across_a_seam() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, Axial::new(1, -1), "road_straight", 1).unwrap();
        let road = def(&c, "road_straight");
        // EAST edge 4 faces (1,-1)'s road end on its edge 1.
        let s = w.edge_status(&c, EAST, road, 1);
        assert_eq!(s[4], EdgeStatus::Compatible);
        assert_eq!(s[3], EdgeStatus::Compatible);
        w.place(&c, EAST, "road_straight", 1).unwrap();
    }

    #[test]
    fn place_accepts_only_legal_placements() {
        let c = catalog();
        let mut w = World::new(1);
        assert_eq!(
            w.place(&c, Axial::ORIGIN, "meadow", 0),
            Err(PlaceError::Occupied)
        );
        assert_eq!(
            w.place(&c, Axial::new(3, 3), "meadow", 0),
            Err(PlaceError::NotAdjacent)
        );
        assert_eq!(
            w.place(&c, EAST, "castle", 0),
            Err(PlaceError::UnknownTile("castle".into()))
        );
        assert_eq!(
            w.place(&c, EAST, "meadow", 6),
            Err(PlaceError::BadRotation(6))
        );
        assert_eq!(
            w.place(&c, EAST, "road_straight", 0),
            Err(PlaceError::Incompatible(vec![3]))
        );
        assert_eq!(w.tiles().len(), 1);
        assert!(!w.can_undo());
        w.place(&c, EAST, "road_straight", 2).unwrap();
        let t = w.tile(EAST).unwrap();
        assert_eq!((t.def.as_str(), t.rotation), ("road_straight", 2));
        assert_eq!(t.seed, cell_seed(1, EAST));
    }

    #[test]
    fn placement_must_fit_every_neighbor() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, EAST, "beach", 0).unwrap();
        // (1,-1) touches the origin meadow (its edge 2) and the beach (its edge 1, beach edge 4 =
        // Grass). Ocean fits neither, forest fits both.
        let slot = Axial::new(1, -1);
        assert!(w.legal_rotations(&c, slot, def(&c, "ocean")).is_empty());
        assert_eq!(w.legal_rotations(&c, slot, def(&c, "forest")).len(), 6);
    }

    /// A beach east of the start meadow: coast along its world edges 2 (land on the right) and 5.
    fn coast_world(c: &TileCatalog) -> World {
        let mut w = World::new(1);
        w.place(c, EAST, "beach", 0).unwrap();
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
        assert_eq!(w.legal_rotations(&c, next, beach), vec![0]);
        assert_eq!(beach.edge(5, 3).kind, SocketKind::Beach);
        assert_eq!(
            w.edge_status(&c, next, beach, 3)[5],
            EdgeStatus::Incompatible
        );
        assert_eq!(
            w.place(&c, next, "beach", 3),
            Err(PlaceError::Incompatible(vec![4, 5]))
        );
        w.place(&c, next, "beach", 0).unwrap();
    }

    /// Owner O1: beach edges never meet grass or ocean edges, under any rotation.
    #[test]
    fn beach_never_meets_grass_or_ocean() {
        let c = catalog();
        let mut w = coast_world(&c);
        // A beach edge (local 5) against the meadow.
        assert_eq!(
            w.place(&c, Axial::new(-1, 0), "beach", 1),
            Err(PlaceError::Incompatible(vec![0]))
        );
        // Grass against the beach's edge 2, ocean against its edge 5.
        assert_eq!(
            w.place(&c, Axial::new(0, 1), "meadow", 0),
            Err(PlaceError::Incompatible(vec![5]))
        );
        assert_eq!(
            w.place(&c, Axial::new(2, -1), "ocean", 0),
            Err(PlaceError::Incompatible(vec![2]))
        );
        for id in ["meadow", "forest", "ocean"] {
            assert!(
                w.legal_rotations(&c, Axial::new(2, -1), def(&c, id))
                    .is_empty(),
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
                    lone.fits(&c, EAST, d, k),
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
            assert_eq!(w.legal_rotations(&c, next, d), vec![rotation], "{id}");
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
            w.place(&c, Axial::ORIGIN.neighbor(d), "beach_outer", d)
                .unwrap_or_else(|e| panic!("headland {d}: {e}"));
        }
        assert_eq!(w.tiles().len(), 7);

        let mut w = World::new(1);
        let lake = Axial::new(2, 0);
        for d in [3, 4, 5, 0, 1, 2] {
            // Ocean edge 0 faces the lake across world edge d + 3.
            w.place(&c, lake.neighbor(d), "beach_inner", (d + 3) % 6)
                .unwrap_or_else(|e| panic!("bay {d}: {e}"));
        }
        w.place(&c, lake, "ocean", 0).unwrap();
        assert_eq!(w.tiles().len(), 8);
        // A gap in the ring takes only the bay that closes it, at one rotation.
        let gap = lake.neighbor(1);
        w.bulldoze(gap).unwrap();
        let fitting: Vec<&str> = w.inventory(&c, gap).iter().map(|d| d.id.as_str()).collect();
        assert_eq!(fitting, ["beach_inner"]);
        assert_eq!(w.legal_rotations(&c, gap, def(&c, "beach_inner")), vec![4]);
    }

    fn line_of_three(c: &TileCatalog) -> World {
        let mut w = World::new(1);
        w.place(c, EAST, "meadow", 0).unwrap();
        w.place(c, Axial::new(2, 0), "meadow", 0).unwrap();
        w
    }

    #[test]
    fn last_tile_cannot_be_bulldozed() {
        let mut w = World::new(1);
        assert_eq!(w.bulldoze(Axial::ORIGIN), Err(BulldozeError::LastTile));
        assert_eq!(w.tiles().len(), 1);
    }

    #[test]
    fn empty_cell_cannot_be_bulldozed() {
        let mut w = World::new(1);
        assert_eq!(w.bulldoze(EAST), Err(BulldozeError::Empty));
    }

    #[test]
    fn bulldoze_refuses_to_split_the_world() {
        let c = catalog();
        let mut w = line_of_three(&c);
        assert_eq!(w.bulldoze(EAST), Err(BulldozeError::WouldDisconnect));
        assert_eq!(w.tiles().len(), 3);
        assert_eq!(w.bulldoze(Axial::new(2, 0)).unwrap().def, "meadow");
        assert_eq!(w.bulldoze(Axial::ORIGIN).unwrap().def, "meadow");
        assert_eq!(w.bulldoze(EAST), Err(BulldozeError::LastTile));
    }

    #[test]
    fn bulldoze_allows_removal_inside_a_ring() {
        let c = catalog();
        let mut w = World::new(1);
        for e in 0..6 {
            w.place(&c, Axial::ORIGIN.neighbor(e), "meadow", 0).unwrap();
        }
        // The ring stays connected without its center or any one ring cell.
        assert_eq!(w.can_bulldoze(EAST), Ok(()));
        w.bulldoze(Axial::ORIGIN).unwrap();
        assert_eq!(w.tiles().len(), 6);
        // Now every ring cell has two ring neighbors; removing one leaves a connected arc.
        w.bulldoze(EAST).unwrap();
        // The arc's middle cells are now cut points.
        assert_eq!(
            w.bulldoze(Axial::ORIGIN.neighbor(3)),
            Err(BulldozeError::WouldDisconnect)
        );
    }

    #[test]
    fn undo_and_redo_a_placement() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, EAST, "forest", 3).unwrap();
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
        w.place(&c, EAST, "road_straight", 2).unwrap();
        let before = w.tiles().clone();
        w.bulldoze(EAST).unwrap();
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
        w.place(&c, EAST, "meadow", 0).unwrap();
        states.push(w.tiles().clone());
        w.place(&c, Axial::new(2, 0), "forest", 0).unwrap();
        states.push(w.tiles().clone());
        w.bulldoze(Axial::new(2, 0)).unwrap();
        states.push(w.tiles().clone());
        w.bulldoze(Axial::ORIGIN).unwrap();
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
        w.place(&c, EAST, "meadow", 0).unwrap();
        w.undo();
        assert!(w.can_redo());
        w.place(&c, Axial::new(-1, 0), "meadow", 0).unwrap();
        assert!(!w.can_redo());
        assert!(!w.redo());
    }

    #[test]
    fn failed_actions_do_not_touch_history() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, EAST, "meadow", 0).unwrap();
        w.undo();
        assert!(w.place(&c, EAST, "ocean", 0).is_err());
        assert!(w.bulldoze(Axial::ORIGIN).is_err());
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
            w.place(&c, Axial::new(i, 0), "meadow", 0).unwrap();
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
        w.place(&c, EAST, "meadow", 0).unwrap();
        assert_eq!(
            w.replace_rotations(&c, EAST, def(&c, "forest")).len(),
            6,
            "forest fits beside the meadow at every rotation"
        );
        assert!(w.replace_rotations(&c, EAST, def(&c, "ocean")).is_empty());
        assert!(
            w.replace_rotations(&c, Axial::new(2, 0), def(&c, "forest"))
                .is_empty(),
            "an empty cell has nothing to replace"
        );
        let old = w.replace(&c, EAST, "forest", 2).unwrap();
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
        w.place(&c, EAST, "meadow", 0).unwrap();
        let before = w.clone();
        assert_eq!(
            w.replace(&c, EAST, "ocean", 0),
            Err(ReplaceError::Incompatible(vec![3]))
        );
        assert_eq!(
            w.replace(&c, Axial::new(2, 0), "forest", 0),
            Err(ReplaceError::Empty)
        );
        assert_eq!(
            w.replace(&c, EAST, "castle", 0),
            Err(ReplaceError::UnknownTile("castle".into()))
        );
        assert_eq!(
            w.replace(&c, EAST, "forest", 6),
            Err(ReplaceError::BadRotation(6))
        );
        assert_eq!(w, before, "refusals change neither tiles nor history");
    }

    #[test]
    fn replace_with_the_same_tile_and_orientation_is_a_no_op() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, EAST, "road_straight", 1).unwrap();
        let before = w.clone();
        assert_eq!(
            w.replace(&c, EAST, "road_straight", 1),
            Err(ReplaceError::Unchanged)
        );
        // Rotation 4 shows rotation 1's sockets on every edge: the same orientation.
        assert_eq!(
            w.replace(&c, EAST, "road_straight", 4),
            Err(ReplaceError::Unchanged)
        );
        assert_eq!(w, before, "a no-op records nothing");
        // Another orientation of the same tile turns it in place.
        w.replace(&c, EAST, "road_straight", 2).unwrap();
        assert_eq!(w.tile(EAST).unwrap().rotation, 2);
    }

    #[test]
    fn replace_is_one_undo_step_and_redo_reapplies_it() {
        let c = catalog();
        let mut w = World::new(1);
        w.place(&c, EAST, "meadow", 0).unwrap();
        let placed = w.tiles().clone();
        w.replace(&c, EAST, "forest", 3).unwrap();
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
        w.place(&c, Axial::new(-1, 0), "meadow", 0).unwrap();
        assert!(!w.can_redo());
        assert_eq!(w.tile(EAST).unwrap().def, "meadow");
    }

    #[test]
    fn world_with_history_round_trips_through_ron() {
        let c = catalog();
        let mut w = World::new(42);
        w.place(&c, EAST, "road_straight", 1).unwrap();
        w.place(&c, Axial::new(-1, 0), "forest", 0).unwrap();
        w.bulldoze(Axial::new(-1, 0)).unwrap();
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
        w.place(&c, EAST, "forest", 0).unwrap();
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
        w.place(&c, FAR, "forest", 1).unwrap();
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
        w.place(&c, FAR, "forest", 1).unwrap();
        pairs_after(&w, &c, &mut cache, FAR);
        w.bulldoze(FAR).unwrap();
        let changed = pairs_after(&w, &c, &mut cache, FAR);
        assert_eq!(changed, BTreeSet::from([EAST, FAR]));
        assert_eq!(piece(&cache, EAST, 0), None);
        assert!(cache.get(FAR).is_none());
    }

    #[test]
    fn undo_and_redo_re_evaluate_the_cell_and_its_neighbours() {
        let (mut w, c, mut cache) = two_forests();
        w.place(&c, FAR, "forest", 1).unwrap();
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
        w.bulldoze(FAR).unwrap();
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
        w.place(&c, FAR, "forest", 1).unwrap();
        pairs_after(&w, &c, &mut cache, FAR);
        // EAST edge-5 neighbour (2, -1) touches EAST (its world edge 2) and FAR (world edge 1).
        let third = EAST.neighbor(5);
        w.place(&c, third, "forest", 3).unwrap();
        let changed = pairs_after(&w, &c, &mut cache, third);
        assert_eq!(changed, BTreeSet::from([EAST, FAR, third]));
        assert_eq!(piece(&cache, EAST, 5), Some("forest_forest_edge_e5"));
        // third (rotation 3): world edge 2 faces EAST (local 5), world edge 1 faces FAR (local 4).
        assert_eq!(piece(&cache, third, 5), Some("forest_forest_edge_e5"));
        assert_eq!(piece(&cache, third, 4), Some("forest_forest_edge_e4"));
        assert_eq!(cache.get(third).unwrap().iter().flatten().count(), 2);
    }
}
