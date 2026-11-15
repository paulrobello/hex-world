//! The tile catalog loaded from `data/` (embedded, so the wasm build needs no filesystem) and the
//! pending kit-piece list (plan F4).

use std::collections::{BTreeSet, HashMap};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::rail_end::{STOPPED_END_GROUND, carries_rail, stopped_edges};
use crate::socket::{EdgeSocket, HeightMeet, SocketTable, SocketTableError, heights_meet};
use crate::tile::{KitRef, TileDef, local_edge, opposite_edge};

/// Every file in `data/tiles/`, as `(id, RON text)`, in inventory order (plan F1, with the owner O1
/// coast corners after the straight beach, as PRD §5.4 lists them, the windmill, the farm and the
/// wheat field after the oak, the massif after the hillsides, its own picker entry, A52 Q4 O1, and the
/// single-tile hill and peak after the massif, each its own entry, spec 2026-11-13 D1).
pub const BUILTIN_TILES: &[(&str, &str)] = &[
    ("meadow", include_str!("../../../data/tiles/meadow.ron")),
    ("forest", include_str!("../../../data/tiles/forest.ron")),
    ("oak", include_str!("../../../data/tiles/oak.ron")),
    ("windmill", include_str!("../../../data/tiles/windmill.ron")),
    ("farm", include_str!("../../../data/tiles/farm.ron")),
    (
        "wheat_field",
        include_str!("../../../data/tiles/wheat_field.ron"),
    ),
    (
        "house_cluster",
        include_str!("../../../data/tiles/house_cluster.ron"),
    ),
    ("bakery", include_str!("../../../data/tiles/bakery.ron")),
    (
        "road_straight",
        include_str!("../../../data/tiles/road_straight.ron"),
    ),
    (
        "road_bend_120",
        include_str!("../../../data/tiles/road_bend_120.ron"),
    ),
    (
        "road_bend_60",
        include_str!("../../../data/tiles/road_bend_60.ron"),
    ),
    (
        "road_junction",
        include_str!("../../../data/tiles/road_junction.ron"),
    ),
    (
        "hillside_000001",
        include_str!("../../../data/tiles/hillside_000001.ron"),
    ),
    (
        "hillside_000011",
        include_str!("../../../data/tiles/hillside_000011.ron"),
    ),
    (
        "hillside_000101",
        include_str!("../../../data/tiles/hillside_000101.ron"),
    ),
    (
        "hillside_000111",
        include_str!("../../../data/tiles/hillside_000111.ron"),
    ),
    (
        "hillside_001001",
        include_str!("../../../data/tiles/hillside_001001.ron"),
    ),
    (
        "hillside_001011",
        include_str!("../../../data/tiles/hillside_001011.ron"),
    ),
    (
        "hillside_001101",
        include_str!("../../../data/tiles/hillside_001101.ron"),
    ),
    (
        "hillside_001111",
        include_str!("../../../data/tiles/hillside_001111.ron"),
    ),
    (
        "hillside_010101",
        include_str!("../../../data/tiles/hillside_010101.ron"),
    ),
    (
        "hillside_010111",
        include_str!("../../../data/tiles/hillside_010111.ron"),
    ),
    (
        "hillside_011011",
        include_str!("../../../data/tiles/hillside_011011.ron"),
    ),
    (
        "hillside_011111",
        include_str!("../../../data/tiles/hillside_011111.ron"),
    ),
    (
        "massif_granite",
        include_str!("../../../data/tiles/massif_granite.ron"),
    ),
    (
        "hill_meadow",
        include_str!("../../../data/tiles/hill_meadow.ron"),
    ),
    (
        "peak_granite",
        include_str!("../../../data/tiles/peak_granite.ron"),
    ),
    (
        "road_ramp",
        include_str!("../../../data/tiles/road_ramp.ron"),
    ),
    (
        "road_tunnel_portal",
        include_str!("../../../data/tiles/road_tunnel_portal.ron"),
    ),
    (
        "rail_straight",
        include_str!("../../../data/tiles/rail_straight.ron"),
    ),
    (
        "rail_bend_60",
        include_str!("../../../data/tiles/rail_bend_60.ron"),
    ),
    (
        "rail_bend_120",
        include_str!("../../../data/tiles/rail_bend_120.ron"),
    ),
    (
        "rail_junction",
        include_str!("../../../data/tiles/rail_junction.ron"),
    ),
    (
        "rail_road_crossing",
        include_str!("../../../data/tiles/rail_road_crossing.ron"),
    ),
    (
        "rail_station",
        include_str!("../../../data/tiles/rail_station.ron"),
    ),
    (
        "rail_tunnel_portal",
        include_str!("../../../data/tiles/rail_tunnel_portal.ron"),
    ),
    (
        "rail_viaduct_l1",
        include_str!("../../../data/tiles/rail_viaduct_l1.ron"),
    ),
    (
        "rail_viaduct_l2",
        include_str!("../../../data/tiles/rail_viaduct_l2.ron"),
    ),
    (
        "rail_viaduct_l3",
        include_str!("../../../data/tiles/rail_viaduct_l3.ron"),
    ),
    (
        "river_straight",
        include_str!("../../../data/tiles/river_straight.ron"),
    ),
    (
        "river_bend",
        include_str!("../../../data/tiles/river_bend.ron"),
    ),
    (
        "river_bend_60",
        include_str!("../../../data/tiles/river_bend_60.ron"),
    ),
    (
        "river_road_bridge",
        include_str!("../../../data/tiles/river_road_bridge.ron"),
    ),
    (
        "river_rail_bridge",
        include_str!("../../../data/tiles/river_rail_bridge.ron"),
    ),
    (
        "river_spring",
        include_str!("../../../data/tiles/river_spring.ron"),
    ),
    (
        "river_fork",
        include_str!("../../../data/tiles/river_fork.ron"),
    ),
    ("beach", include_str!("../../../data/tiles/beach.ron")),
    (
        "beach_outer",
        include_str!("../../../data/tiles/beach_outer.ron"),
    ),
    (
        "beach_inner",
        include_str!("../../../data/tiles/beach_inner.ron"),
    ),
    (
        "river_mouth",
        include_str!("../../../data/tiles/river_mouth.ron"),
    ),
    (
        "river_mouth_l",
        include_str!("../../../data/tiles/river_mouth_l.ron"),
    ),
    ("ocean", include_str!("../../../data/tiles/ocean.ron")),
];

/// Picker groups (owner, 2026-10-20): tiles the Tiles sheet lists as one entry, `(group id, member
/// tile ids in rotation order)`. The members stay separate tiles (saves, thumbnails, kit pieces and
/// the sim see only them); R steps through every member's distinct legal orientations in turn, so
/// the player switches between them by rotating. A group id never equals a tile id.
pub const PICKER_GROUPS: &[(&str, &[&str])] = &[
    ("beach_curve", &["beach_outer", "beach_inner"]),
    ("river_mouths", &["river_mouth", "river_mouth_l"]),
    (
        "rail_viaduct",
        &["rail_viaduct_l1", "rail_viaduct_l2", "rail_viaduct_l3"],
    ),
    (
        "hillside",
        &[
            "hillside_000001",
            "hillside_000011",
            "hillside_000101",
            "hillside_000111",
            "hillside_001001",
            "hillside_001011",
            "hillside_001101",
            "hillside_001111",
            "hillside_010101",
            "hillside_010111",
            "hillside_011011",
            "hillside_011111",
        ],
    ),
];

/// One Tiles sheet entry: a lone tile (`id` is its tile id) or a picker group.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PickerEntry {
    pub id: String,
    /// Member tile ids in rotation order; one for a lone tile.
    pub tiles: Vec<String>,
}

/// `data/pending-pieces.ron`.
pub const PENDING_PIECES_RON: &str = include_str!("../../../data/pending-pieces.ron");

/// The tile every new world starts with at the origin (A1).
pub const START_TILE: &str = "meadow";

/// A socket-table, tile parsing, identity or validation failure while loading a catalog.
#[derive(Clone, Debug, PartialEq)]
pub enum CatalogError {
    /// The socket table could not be parsed or validated.
    Sockets(SocketTableError),
    /// A tile file is not valid RON for a tile definition.
    Parse { file: String, error: String },
    /// A tile definition id differs from its file stem.
    IdMismatch { file: String, id: String },
    /// Two definitions declare the same tile id.
    DuplicateId(String),
    /// Collected tile validation issues.
    Invalid(Vec<String>),
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CatalogError::Sockets(e) => write!(f, "{e}"),
            CatalogError::Parse { file, error } => write!(f, "tiles/{file}.ron: {error}"),
            CatalogError::IdMismatch { file, id } => {
                write!(f, "tiles/{file}.ron declares id {id:?}")
            }
            CatalogError::DuplicateId(id) => write!(f, "tile id {id:?} is declared twice"),
            CatalogError::Invalid(issues) => write!(f, "invalid tiles:\n{}", issues.join("\n")),
        }
    }
}

impl std::error::Error for CatalogError {}

/// Validated tile definitions plus the socket table they were checked against.
#[derive(Clone, Debug, PartialEq)]
pub struct TileCatalog {
    sockets: SocketTable,
    tiles: Vec<TileDef>,
    /// Per tile, in `tiles` order: its [`stopped_edges`] mask and whether it [`carries_rail`].
    rail_ends: Vec<(u8, bool)>,
    /// Each tile id's index in `tiles`.
    index: HashMap<String, usize>,
}

/// One side of a seam: the tile, the socket it shows across the seam, and its level. `local` is
/// the tile's local edge for a single tile (the rail stop mask needs it) and `None` for a mega
/// tile's outer face.
#[derive(Clone, Copy, Debug)]
pub struct Facing<'a> {
    pub def: &'a TileDef,
    pub socket: &'a EdgeSocket,
    pub local: Option<u8>,
    pub level: u8,
}

impl<'a> Facing<'a> {
    /// Single tile `def` turned by `rotation` at `level`, across its world edge `w`.
    pub fn single(def: &'a TileDef, rotation: u8, level: u8, w: u8) -> Self {
        Self {
            def,
            socket: def.edge(w, rotation),
            local: Some(local_edge(w, rotation)),
            level,
        }
    }

    /// How this face and `other` meet in height ([`heights_meet`]).
    pub fn meet(self, other: Facing) -> HeightMeet {
        heights_meet(self.socket, self.level, other.socket, other.level)
    }
}

impl TileCatalog {
    /// Builds the catalog and rejects duplicate ids or any [`TileDef::validate`] issue.
    pub fn new(sockets: SocketTable, tiles: Vec<TileDef>) -> Result<Self, CatalogError> {
        let mut ids: BTreeSet<&str> = BTreeSet::new();
        for t in &tiles {
            if !ids.insert(t.id.as_str()) {
                return Err(CatalogError::DuplicateId(t.id.clone()));
            }
        }
        let issues: Vec<String> = tiles.iter().flat_map(|t| t.validate(&sockets)).collect();
        if !issues.is_empty() {
            return Err(CatalogError::Invalid(issues));
        }
        let rail_ends = tiles
            .iter()
            .map(|t| (stopped_edges(t), carries_rail(t)))
            .collect();
        let index = tiles
            .iter()
            .enumerate()
            .map(|(i, t)| (t.id.clone(), i))
            .collect();
        Ok(Self {
            sockets,
            tiles,
            rail_ends,
            index,
        })
    }

    /// Parses a socket table and `(file stem, RON)` tile files; each file's id must equal its stem.
    pub fn from_ron(sockets_ron: &str, files: &[(&str, &str)]) -> Result<Self, CatalogError> {
        let sockets = SocketTable::from_ron(sockets_ron).map_err(CatalogError::Sockets)?;
        let mut tiles = Vec::with_capacity(files.len());
        for (file, text) in files {
            let def: TileDef = ron::from_str(text).map_err(|e| CatalogError::Parse {
                file: (*file).to_owned(),
                error: e.to_string(),
            })?;
            if def.id != *file {
                return Err(CatalogError::IdMismatch {
                    file: (*file).to_owned(),
                    id: def.id,
                });
            }
            tiles.push(def);
        }
        Self::new(sockets, tiles)
    }

    /// The catalog embedded from `data/`.
    pub fn builtin() -> Result<Self, CatalogError> {
        Self::from_ron(crate::socket::SOCKETS_RON, BUILTIN_TILES)
    }

    /// Looks up a definition by id, or returns `None` when absent.
    pub fn get(&self, id: &str) -> Option<&TileDef> {
        self.index.get(id).map(|&i| &self.tiles[i])
    }

    /// Tiles in inventory order.
    pub fn tiles(&self) -> &[TileDef] {
        &self.tiles
    }

    /// The validated socket table used by this catalog.
    pub fn sockets(&self) -> &SocketTable {
        &self.sockets
    }

    /// `def`'s [`stopped_edges`] mask and whether it [`carries_rail`], cached for catalog tiles.
    pub fn rail_ends(&self, def: &TileDef) -> (u8, bool) {
        self.index.get(&def.id).map_or_else(
            || (stopped_edges(def), carries_rail(def)),
            |&i| self.rail_ends[i],
        )
    }

    /// Whether `mine` may be placed against the placed `theirs` (owner, 2026-10-25): the sockets
    /// meet ([`SocketTable::edges_compatible`]), or `mine` carries no rail at all and `theirs` is a
    /// rail end that gets a buffer stop, which closes the line, so `mine` meets it as
    /// [`STOPPED_END_GROUND`]. A tile that carries rail gets the socket rule alone: it continues
    /// the line, and it never turns an end toward a placed tile without rail.
    pub fn facing_meets(&self, mine: Facing, theirs: Facing) -> bool {
        self.sockets
            .edges_compatible(mine.socket, mine.level, theirs.socket, theirs.level)
            || self.stop_closes(theirs, mine)
    }

    /// [`TileCatalog::facing_meets`] in either placement order. Loading checks this, as a world
    /// holds the pair however it was built (a station placed before the meadow beyond its end looks
    /// the same as one placed after). It is symmetric and depends only on the two faces, so no
    /// third tile's placement or removal can change it.
    pub fn facings_meet(&self, a: Facing, b: Facing) -> bool {
        self.facing_meets(a, b) || self.facing_meets(b, a)
    }

    /// Whether `def` turned by `rotation` at height `level` may be placed with its world edge `w`
    /// against the placed single tile `neighbour` turned by `nrot` at `nlevel`
    /// ([`TileCatalog::facing_meets`]).
    #[allow(clippy::too_many_arguments)]
    pub fn placement_meets(
        &self,
        def: &TileDef,
        rotation: u8,
        level: u8,
        w: u8,
        neighbour: &TileDef,
        nrot: u8,
        nlevel: u8,
    ) -> bool {
        let back = opposite_edge(w);
        self.facing_meets(
            Facing::single(def, rotation, level, w),
            Facing::single(neighbour, nrot, nlevel, back),
        )
    }

    /// Whether placed single tiles `a` (turned by `ra`, at level `la`) and `b` (turned by `rb`, at
    /// level `lb`) may touch across `a`'s world edge `w`, in either placement order
    /// ([`TileCatalog::facings_meet`]).
    #[allow(clippy::too_many_arguments)]
    pub fn edges_meet(
        &self,
        a: &TileDef,
        ra: u8,
        la: u8,
        w: u8,
        b: &TileDef,
        rb: u8,
        lb: u8,
    ) -> bool {
        self.facings_meet(
            Facing::single(a, ra, la, w),
            Facing::single(b, rb, lb, opposite_edge(w)),
        )
    }

    /// Whether `stopped` is a stopped rail end that meets `other`'s socket as
    /// [`STOPPED_END_GROUND`], `other` carrying no rail. A mega outer face has no local edge and
    /// never carries a stop.
    fn stop_closes(&self, stopped: Facing, other: Facing) -> bool {
        let Some(edge) = stopped.local else {
            return false;
        };
        let ground = EdgeSocket {
            height: stopped.socket.height,
            profile: stopped.socket.profile,
            ..EdgeSocket::new(STOPPED_END_GROUND)
        };
        self.rail_ends(stopped.def).0 & (1 << edge) != 0
            && !self.rail_ends(other.def).1
            && self
                .sockets
                .edges_compatible(&ground, stopped.level, other.socket, other.level)
    }

    /// The picker group `id` belongs to, when it is a member of one in this catalog.
    pub fn group_of(&self, id: &str) -> Option<&'static str> {
        PICKER_GROUPS
            .iter()
            .find(|(_, members)| members.contains(&id))
            .map(|(g, _)| *g)
    }

    /// The members of picker group `group` present in this catalog, in rotation order (empty for an
    /// unknown group).
    pub fn group_members(&self, group: &str) -> Vec<&TileDef> {
        PICKER_GROUPS
            .iter()
            .find(|(g, _)| *g == group)
            .map(|(_, members)| members.iter().filter_map(|m| self.get(m)).collect())
            .unwrap_or_default()
    }

    /// The tiles R cycles through with `id` selected: its group's members, or the tile alone.
    pub fn siblings(&self, id: &str) -> Vec<&TileDef> {
        match self.group_of(id) {
            Some(g) => self.group_members(g),
            None => self.get(id).into_iter().collect(),
        }
    }

    /// The Tiles sheet entries in inventory order: each lone tile, and each group at its first
    /// member's place.
    pub fn picker(&self) -> Vec<PickerEntry> {
        let mut out: Vec<PickerEntry> = vec![];
        for t in &self.tiles {
            match self.group_of(&t.id) {
                Some(g) if out.iter().any(|e| e.id == g) => {}
                Some(g) => out.push(PickerEntry {
                    id: g.to_owned(),
                    tiles: self.group_members(g).iter().map(|m| m.id.clone()).collect(),
                }),
                None => out.push(PickerEntry {
                    id: t.id.clone(),
                    tiles: vec![t.id.clone()],
                }),
            }
        }
        out
    }

    /// Every kit piece referenced by any tile, sorted.
    pub fn kit_refs(&self) -> BTreeSet<&str> {
        self.tiles
            .iter()
            .flat_map(|t| t.kit_refs(&self.sockets))
            .collect()
    }
}

/// A kit piece referenced by tile data before it is built, and who delivers it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PendingPiece {
    pub id: KitRef,
    pub delivered_by: String,
}

/// `data/pending-pieces.ron` (plan F4). Removing an entry makes the validator enforce that piece.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PendingPieces {
    pub pieces: Vec<PendingPiece>,
    /// Who delivers the per-rotation tile thumbnails, while they are pending as a whole.
    #[serde(default)]
    pub thumbnails: Option<String>,
}

impl PendingPieces {
    /// Parses the pending-piece delivery list from RON.
    pub fn from_ron(text: &str) -> Result<Self, String> {
        ron::from_str(text).map_err(|e| format!("pending-pieces.ron: {e}"))
    }

    /// Loads the embedded pending-piece delivery list.
    pub fn builtin() -> Result<Self, String> {
        Self::from_ron(PENDING_PIECES_RON)
    }

    /// Whether a kit piece id is still listed as pending.
    pub fn contains(&self, id: &str) -> bool {
        self.pieces.iter().any(|p| p.id.as_str() == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_loads_in_f1_order() {
        let c = TileCatalog::builtin().unwrap_or_else(|e| panic!("{e}"));
        let ids: Vec<&str> = c.tiles().iter().map(|t| t.id.as_str()).collect();
        let want: Vec<&str> = BUILTIN_TILES.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, want);
        assert!(c.get(START_TILE).is_some());
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let (id, text) = BUILTIN_TILES[0];
        let err = TileCatalog::from_ron(crate::socket::SOCKETS_RON, &[(id, text), (id, text)]);
        assert_eq!(err, Err(CatalogError::DuplicateId(id.to_owned())));
    }

    #[test]
    fn file_stem_must_match_id() {
        let (_, text) = BUILTIN_TILES[0];
        let err = TileCatalog::from_ron(crate::socket::SOCKETS_RON, &[("other", text)]);
        assert!(matches!(err, Err(CatalogError::IdMismatch { .. })));
    }

    #[test]
    fn invalid_tile_is_rejected_with_its_issues() {
        let text = BUILTIN_TILES
            .iter()
            .find(|(id, _)| *id == "road_straight")
            .unwrap()
            .1
            .replace("to: Edge(3)", "to: Edge(2)");
        let err = TileCatalog::from_ron(crate::socket::SOCKETS_RON, &[("road_straight", &text)]);
        let Err(CatalogError::Invalid(issues)) = err else {
            panic!("expected Invalid, got {err:?}");
        };
        assert!(
            issues.iter().any(|i| i.contains("lane edge 3 has 0")),
            "{issues:?}"
        );
    }

    #[test]
    fn picker_groups_are_well_formed() {
        let c = TileCatalog::builtin().unwrap();
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for (g, members) in PICKER_GROUPS {
            assert!(c.get(g).is_none(), "group id {g} is also a tile id");
            assert!(members.len() >= 2, "group {g} has one member");
            for m in *members {
                assert!(c.get(m).is_some(), "group {g} member {m} is not a tile");
                assert!(seen.insert(m), "{m} sits in two groups");
            }
        }
    }

    #[test]
    fn picker_lists_beach_curve_once_in_the_headlands_place() {
        let c = TileCatalog::builtin().unwrap();
        let p = c.picker();
        let ids: Vec<&str> = p.iter().map(|e| e.id.as_str()).collect();
        assert!(!ids.contains(&"beach_outer") && !ids.contains(&"beach_inner"));
        let i = ids.iter().position(|&id| id == "beach_curve").unwrap();
        assert_eq!(ids[i - 1], "beach");
        assert_eq!(p[i].tiles, ["beach_outer", "beach_inner"]);
        let folded: usize = PICKER_GROUPS.iter().map(|(_, m)| m.len() - 1).sum();
        assert_eq!(p.len(), c.tiles().len() - folded);
        assert_eq!(c.siblings("beach_inner").len(), 2);
        assert_eq!(c.siblings("meadow").len(), 1);
    }

    #[test]
    fn picker_lists_the_river_mouths_once_with_the_mirrored_mouth_second() {
        let c = TileCatalog::builtin().unwrap();
        let p = c.picker();
        let ids: Vec<&str> = p.iter().map(|e| e.id.as_str()).collect();
        assert!(!ids.contains(&"river_mouth") && !ids.contains(&"river_mouth_l"));
        let i = ids.iter().position(|&id| id == "river_mouths").unwrap();
        assert_eq!(p[i].tiles, ["river_mouth", "river_mouth_l"]);
        assert_eq!(c.siblings("river_mouth_l").len(), 2);
    }

    /// A52 Q4 O1: the massif has its own Tiles sheet entry, right after the hillside group.
    #[test]
    fn picker_lists_the_massif_alone_after_the_hillsides() {
        let c = TileCatalog::builtin().unwrap();
        let p = c.picker();
        let i = p.iter().position(|e| e.id == "hillside").unwrap();
        assert_eq!(p[i + 1].id, "massif_granite");
        assert_eq!(p[i + 1].tiles, ["massif_granite"]);
        assert_eq!(c.siblings("massif_granite").len(), 1);
    }

    /// Spec 2026-11-13 D1 (single-tile hills): the hill and the peak are lone Tiles sheet entries,
    /// right after the massif.
    #[test]
    fn picker_lists_the_single_tile_hills_after_the_massif() {
        let c = TileCatalog::builtin().unwrap();
        let p = c.picker();
        let i = p.iter().position(|e| e.id == "massif_granite").unwrap();
        assert_eq!(p[i + 1].tiles, ["hill_meadow"]);
        assert_eq!(p[i + 2].tiles, ["peak_granite"]);
        for id in ["hill_meadow", "peak_granite"] {
            assert_eq!(c.siblings(id).len(), 1, "{id}");
            assert!(!c.get(id).unwrap().is_mega(), "{id}");
        }
    }

    #[test]
    fn portal_and_ramp_share_the_straight_bank() {
        let c = TileCatalog::builtin().unwrap();
        let ramp = c.get("road_ramp").unwrap();
        let portal = c.get("road_tunnel_portal").unwrap();
        assert_eq!(ramp.corners(0), [1, 1, 0, 0, 0, 1]);
        assert_eq!(portal.corners(0), [1, 1, 0, 0, 0, 1]);
        assert_eq!(ramp.ground, crate::Ground::RoadRamp);
        assert_eq!(portal.ground, crate::Ground::Cutting);
        assert_eq!(portal.tunnel.as_ref().unwrap().edge, 0);
        assert_eq!(c.siblings("road_ramp").len(), 1);
        assert_eq!(c.siblings("hillside_000111").len(), 12);
    }

    /// A stopped rail end closes against a no-rail tile at any pair of levels (the stop is a
    /// lane-free ground, a cliff where the levels differ), while rail continues only at equal
    /// absolute height.
    #[test]
    fn placement_meets_and_stop_closes_at_nonzero_levels() {
        let c = TileCatalog::builtin().unwrap();
        let straight = c.get("rail_straight").unwrap();
        let meadow = c.get("meadow").unwrap();
        for (rail_level, meadow_level) in [(0, 0), (2, 2), (0, 1), (2, 0), (1, 3)] {
            assert!(
                c.placement_meets(meadow, 0, meadow_level, 3, straight, 0, rail_level),
                "meadow {meadow_level} beside a stopped end at {rail_level}"
            );
            assert!(c.edges_meet(straight, 0, rail_level, 0, meadow, 0, meadow_level));
        }
        // A rail tile never turns its end toward a raised meadow.
        assert!(!c.placement_meets(straight, 0, 0, 0, meadow, 0, 1));
        assert!(c.placement_meets(straight, 0, 2, 0, straight, 0, 2));
        assert!(!c.placement_meets(straight, 0, 1, 0, straight, 0, 2));
        assert!(!c.placement_meets(straight, 0, 2, 0, straight, 0, 1));
    }

    /// `facing_meets` and `placement_meets` both match the single-tile rule written out here:
    /// the sockets meet, or the neighbour's facing edge is a stopped rail end and `def` carries
    /// no rail, so it meets the stop's lane-free ground. Single tiles only: a mega tile shows no
    /// socket at its anchor.
    #[test]
    fn facing_rules_match_placement_meets() {
        use crate::rail_end::{carries_rail, stopped_edges};
        let c = TileCatalog::builtin().unwrap();
        let singles: Vec<&TileDef> = c.tiles().iter().filter(|t| !t.is_mega()).collect();
        let rail: Vec<(u8, bool)> = singles
            .iter()
            .map(|t| (stopped_edges(t), carries_rail(t)))
            .collect();
        let s = c.sockets();
        for (&a, &(_, a_rail)) in singles.iter().zip(&rail) {
            for (&b, &(b_stops, _)) in singles.iter().zip(&rail) {
                for (ra, rb, w) in
                    (0..216u16).map(|i| ((i / 36) as u8, (i / 6 % 6) as u8, (i % 6) as u8))
                {
                    let back = opposite_edge(w);
                    let (mine, theirs) = (a.edge(w, ra), b.edge(back, rb));
                    for (la, lb) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                        let ground = EdgeSocket {
                            height: theirs.height,
                            profile: theirs.profile,
                            ..EdgeSocket::new(STOPPED_END_GROUND)
                        };
                        let want = s.edges_compatible(mine, la, theirs, lb)
                            || (b_stops & (1 << local_edge(back, rb)) != 0
                                && !a_rail
                                && s.edges_compatible(&ground, lb, mine, la));
                        let fa = Facing {
                            def: a,
                            socket: a.socket_toward(ra, None, w).unwrap(),
                            local: Some(local_edge(w, ra)),
                            level: la,
                        };
                        let fb = Facing {
                            def: b,
                            socket: b.socket_toward(rb, None, back).unwrap(),
                            local: Some(local_edge(back, rb)),
                            level: lb,
                        };
                        let seam =
                            format!("{} r{ra} l{la} | {} r{rb} l{lb} across {w}", a.id, b.id);
                        assert_eq!(c.facing_meets(fa, fb), want, "{seam}");
                        assert_eq!(c.placement_meets(a, ra, la, w, b, rb, lb), want, "{seam}");
                        assert_eq!(
                            c.facings_meet(fa, fb),
                            c.edges_meet(a, ra, la, w, b, rb, lb),
                            "{seam}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn builtin_pending_list_parses() {
        // Empty once every tile's pieces are in the kit (the beach family closed the list).
        let p = PendingPieces::builtin().unwrap();
        assert!(p.pieces.iter().all(|e| !e.delivered_by.is_empty()));
    }
}
