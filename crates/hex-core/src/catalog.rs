//! The tile catalog loaded from `data/` (embedded, so the wasm build needs no filesystem) and the
//! pending kit-piece list (plan F4).

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::rail_end::{STOPPED_END_GROUND, carries_rail, stopped_edges};
use crate::socket::{EdgeSocket, HeightMeet, SocketTable, SocketTableError, heights_meet};
use crate::tile::{KitRef, TileDef, local_edge, opposite_edge};

/// Every file in `data/tiles/`, as `(id, RON text)`, in inventory order (plan F1, with the owner O1
/// coast corners after the straight beach, as PRD §5.4 lists them, and the windmill, the farm and
/// the wheat field after the oak).
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
        Ok(Self {
            sockets,
            tiles,
            rail_ends,
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
        self.tiles.iter().find(|t| t.id == id)
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
        self.tiles.iter().position(|t| t.id == def.id).map_or_else(
            || (stopped_edges(def), carries_rail(def)),
            |i| self.rail_ends[i],
        )
    }

    /// Whether `def` turned by `rotation` at height `level` may be placed with its world edge `w`
    /// against the placed `neighbour` turned by `nrot` at `nlevel` (owner, 2026-10-25): the sockets meet
    /// ([`SocketTable::edges_compatible`]), or `def` carries no rail at all and the neighbour's
    /// facing edge is a rail end that gets a buffer stop, which closes the line, so `def` meets it
    /// as [`STOPPED_END_GROUND`]. A tile that carries rail gets the socket rule alone: it continues
    /// the line, and it never turns an end toward a placed tile without rail.
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
        let (mine, theirs) = (def.edge(w, rotation), neighbour.edge(back, nrot));
        self.sockets.edges_compatible(mine, level, theirs, nlevel)
            || self.stop_closes(
                (neighbour, local_edge(back, nrot), theirs, nlevel),
                (def, mine, level),
            )
    }

    /// Whether placed tiles `a` (turned by `ra`, at level `la`) and `b` (turned by `rb`, at level
    /// `lb`) may touch across `a`'s world edge `w`, in either placement order: [`TileCatalog::placement_meets`] one way or the
    /// other. Loading checks this, as a world holds the pair however it was built (a station placed
    /// before the meadow beyond its end looks the same as one placed after). It is symmetric and
    /// depends only on the two tiles, so no third tile's placement or removal can change it.
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
        self.placement_meets(a, ra, la, w, b, rb, lb)
            || self.placement_meets(b, rb, lb, opposite_edge(w), a, ra, la)
    }

    /// How the facing sockets of `a` and `b` meet in height across `a`'s world edge `w`
    /// ([`heights_meet`]): `Cliff` seams are the ones a cliff skirt is drawn on.
    #[allow(clippy::too_many_arguments)]
    pub fn meet_kind(
        &self,
        a: &TileDef,
        ra: u8,
        la: u8,
        w: u8,
        b: &TileDef,
        rb: u8,
        lb: u8,
    ) -> HeightMeet {
        heights_meet(a.edge(w, ra), la, b.edge(opposite_edge(w), rb), lb)
    }

    /// Whether local edge `edge` (socket `own`) of `def` is a stopped rail end that meets `other`'s
    /// socket `theirs` as [`STOPPED_END_GROUND`], `other` carrying no rail.
    fn stop_closes(
        &self,
        (def, edge, own, own_level): (&TileDef, u8, &EdgeSocket, u8),
        (other, theirs, their_level): (&TileDef, &EdgeSocket, u8),
    ) -> bool {
        let stopped = self.rail_ends(def).0 & (1 << edge) != 0;
        let ground = EdgeSocket {
            height: own.height,
            profile: own.profile,
            ..EdgeSocket::new(STOPPED_END_GROUND)
        };
        stopped
            && !self.rail_ends(other).1
            && self
                .sockets
                .edges_compatible(&ground, own_level, theirs, their_level)
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

    #[test]
    fn builtin_pending_list_parses() {
        // Empty once every tile's pieces are in the kit (the beach family closed the list).
        let p = PendingPieces::builtin().unwrap();
        assert!(p.pieces.iter().all(|e| !e.delivered_by.is_empty()));
    }
}
