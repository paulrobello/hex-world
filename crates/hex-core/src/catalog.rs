//! The tile catalog loaded from `data/` (embedded, so the wasm build needs no filesystem) and the
//! pending kit-piece list (plan F4).

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::socket::{SocketTable, SocketTableError};
use crate::tile::{KitRef, TileDef};

/// Every file in `data/tiles/`, as `(id, RON text)`, in inventory order (plan F1, with the owner O1
/// coast corners after the straight beach, as PRD §5.4 lists them, and the windmill after the oak).
pub const BUILTIN_TILES: [(&str, &str); 19] = [
    ("meadow", include_str!("../../../data/tiles/meadow.ron")),
    ("forest", include_str!("../../../data/tiles/forest.ron")),
    ("oak", include_str!("../../../data/tiles/oak.ron")),
    ("windmill", include_str!("../../../data/tiles/windmill.ron")),
    (
        "house_cluster",
        include_str!("../../../data/tiles/house_cluster.ron"),
    ),
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
        "river_straight",
        include_str!("../../../data/tiles/river_straight.ron"),
    ),
    (
        "river_bend",
        include_str!("../../../data/tiles/river_bend.ron"),
    ),
    (
        "river_road_bridge",
        include_str!("../../../data/tiles/river_road_bridge.ron"),
    ),
    (
        "river_spring",
        include_str!("../../../data/tiles/river_spring.ron"),
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
        Ok(Self { sockets, tiles })
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
        Self::from_ron(crate::socket::SOCKETS_RON, &BUILTIN_TILES)
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

    /// Every kit piece referenced by any tile, sorted.
    pub fn kit_refs(&self) -> BTreeSet<&KitRef> {
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
    fn builtin_pending_list_parses() {
        // Empty once every tile's pieces are in the kit (the beach family closed the list).
        let p = PendingPieces::builtin().unwrap();
        assert!(p.pieces.iter().all(|e| !e.delivered_by.is_empty()));
    }
}
