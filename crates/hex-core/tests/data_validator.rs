//! Build-time validator for `data/` (PRD §5.3, plan F1–F4).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use hex_core::SocketKind::{self, *};
use hex_core::{BUILTIN_TILES, EdgeProfile, PendingPieces, SOCKETS_RON, TileCatalog, TileDef};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn catalog() -> TileCatalog {
    TileCatalog::builtin().unwrap_or_else(|e| panic!("{e}"))
}

fn pending() -> PendingPieces {
    PendingPieces::builtin().unwrap_or_else(|e| panic!("{e}"))
}

/// Piece ids in `assets/kit/manifest.json`. JSON objects and arrays are valid RON maps and lists.
fn manifest_piece_ids() -> BTreeSet<String> {
    let path = repo_root().join("assets/kit/manifest.json");
    let text = std::fs::read_to_string(&path).unwrap();
    let value: ron::Value = ron::from_str(&text).expect("manifest.json parses as a RON value");
    let ron::Value::Map(root) = value else {
        panic!("manifest root is not an object")
    };
    let Some(ron::Value::Seq(pieces)) = root.get(&ron::Value::String("pieces".into())) else {
        panic!("manifest has no pieces list")
    };
    pieces
        .iter()
        .map(|p| match p {
            ron::Value::Map(m) => match m.get(&ron::Value::String("id".into())) {
                Some(ron::Value::String(id)) => id.clone(),
                other => panic!("piece without string id: {other:?}"),
            },
            other => panic!("piece is not an object: {other:?}"),
        })
        .collect()
}

/// Plan F1: edges 0..5 for each MVP tile.
const F1: [(&str, [SocketKind; 6]); 12] = [
    ("meadow", [Grass, Grass, Grass, Grass, Grass, Grass]),
    ("forest", [Forest, Forest, Forest, Forest, Forest, Forest]),
    ("house_cluster", [Road, Grass, Grass, Grass, Grass, Grass]),
    ("road_straight", [Road, Grass, Grass, Road, Grass, Grass]),
    ("road_bend_120", [Road, Grass, Road, Grass, Grass, Grass]),
    ("road_bend_60", [Road, Road, Grass, Grass, Grass, Grass]),
    ("road_junction", [Road, Grass, Road, Grass, Road, Grass]),
    ("river_straight", [River, Grass, Grass, River, Grass, Grass]),
    ("river_bend", [River, Grass, River, Grass, Grass, Grass]),
    (
        "river_road_bridge",
        [River, Road, Grass, River, Road, Grass],
    ),
    ("beach", [Ocean, Ocean, Beach, Grass, Grass, Beach]),
    ("ocean", [Ocean, Ocean, Ocean, Ocean, Ocean, Ocean]),
];

#[test]
fn every_tile_file_parses_and_is_embedded() {
    let dir = repo_root().join("data/tiles");
    let mut on_disk: BTreeSet<String> = BTreeSet::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        assert_eq!(
            path.extension().and_then(|e| e.to_str()),
            Some("ron"),
            "{path:?}"
        );
        let text = std::fs::read_to_string(&path).unwrap();
        let def: TileDef =
            ron::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let stem = path.file_stem().unwrap().to_str().unwrap();
        assert_eq!(def.id, stem, "{path:?}: id must equal the file name");
        on_disk.insert(stem.to_owned());
    }
    let embedded: BTreeSet<String> = BUILTIN_TILES
        .iter()
        .map(|(id, _)| (*id).to_owned())
        .collect();
    assert_eq!(
        on_disk, embedded,
        "data/tiles and catalog::BUILTIN_TILES must list the same files"
    );
}

#[test]
fn tile_ids_are_unique_and_rules_hold() {
    let c = TileCatalog::from_ron(SOCKETS_RON, &BUILTIN_TILES).unwrap_or_else(|e| panic!("{e}"));
    let ids: BTreeSet<&str> = c.tiles().iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids.len(), c.tiles().len());
}

#[test]
fn the_twelve_f1_tiles_have_exactly_the_f1_edge_kinds() {
    let c = catalog();
    assert_eq!(c.tiles().len(), F1.len());
    for (id, kinds) in F1 {
        let t = c
            .get(id)
            .unwrap_or_else(|| panic!("F1 tile {id} is missing"));
        let got: Vec<SocketKind> = t.edges.iter().map(|e| e.kind).collect();
        assert_eq!(got, kinds, "{id}");
    }
}

#[test]
fn every_mvp_edge_is_flat_at_height_zero() {
    for t in catalog().tiles() {
        for (e, edge) in t.edges.iter().enumerate() {
            assert_eq!(edge.height, 0, "{} edge {e}", t.id);
            assert_eq!(edge.profile, EdgeProfile::Flat, "{} edge {e}", t.id);
        }
    }
}

#[test]
fn every_lane_edge_has_exactly_one_endpoint_and_none_sit_on_other_edges() {
    let c = catalog();
    for t in c.tiles() {
        assert_eq!(t.validate(c.sockets()), Vec::<String>::new());
    }
}

#[test]
fn every_kit_ref_is_built_or_pending() {
    let c = catalog();
    let built = manifest_piece_ids();
    let pending = pending();
    let missing: Vec<&str> = c
        .kit_refs()
        .into_iter()
        .map(|r| r.as_str())
        .filter(|id| !built.contains(*id) && !pending.contains(id))
        .collect();
    assert!(
        missing.is_empty(),
        "kit pieces neither in assets/kit/manifest.json nor data/pending-pieces.ron: {missing:?}"
    );
}

#[test]
fn pending_list_is_honest() {
    let c = catalog();
    let built = manifest_piece_ids();
    let referenced: BTreeSet<&str> = c.kit_refs().into_iter().map(|r| r.as_str()).collect();
    let p = pending();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for piece in &p.pieces {
        let id = piece.id.as_str();
        assert!(seen.insert(id), "{id} is pending twice");
        assert!(
            !built.contains(id),
            "{id} is in assets/kit/manifest.json: remove it from data/pending-pieces.ron"
        );
        assert!(
            referenced.contains(id),
            "{id} is pending but no tile references it"
        );
        assert!(
            !piece.delivered_by.trim().is_empty(),
            "{id} must name the stream or card that delivers it"
        );
    }
}

/// `assets/thumbs/manifest.json` `tiles`: each tile id mapped to its image per rotation 0..6.
fn thumbnail_manifest() -> BTreeMap<String, Vec<String>> {
    let path = repo_root().join("assets/thumbs/manifest.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (run `make thumbs`)", path.display()));
    let value: ron::Value = ron::from_str(&text).expect("thumbs manifest parses as a RON value");
    let ron::Value::Map(root) = value else {
        panic!("thumbs manifest root is not an object")
    };
    let Some(ron::Value::Map(tiles)) = root.get(&ron::Value::String("tiles".into())) else {
        panic!("thumbs manifest has no tiles object")
    };
    tiles
        .iter()
        .map(|(k, v)| {
            let ron::Value::String(id) = k else {
                panic!("thumbs manifest tile key is not a string: {k:?}")
            };
            let ron::Value::Seq(files) = v else {
                panic!("thumbs manifest {id} is not a list")
            };
            let files = files
                .iter()
                .map(|f| match f {
                    ron::Value::String(f) => f.clone(),
                    other => panic!("thumbs manifest {id} entry is not a string: {other:?}"),
                })
                .collect();
            (id.clone(), files)
        })
        .collect()
}

#[test]
fn thumbnails_are_no_longer_pending() {
    assert_eq!(
        pending().thumbnails,
        None,
        "thumbnails are enforced by every_tile_rotation_has_a_thumbnail: \
         drop the thumbnails entry from data/pending-pieces.ron"
    );
}

/// PRD §5.3: every rotation of every tile has a thumbnail. Rotations may share an image only when
/// their world-edge sockets are equal, and must share one when they are.
#[test]
fn every_tile_rotation_has_a_thumbnail() {
    let c = catalog();
    let dir = repo_root().join("assets/thumbs");
    let manifest = thumbnail_manifest();
    let listed: BTreeSet<&str> = manifest.keys().map(String::as_str).collect();
    let tiles: BTreeSet<&str> = c.tiles().iter().map(|t| t.id.as_str()).collect();
    assert_eq!(
        listed, tiles,
        "assets/thumbs/manifest.json must list every catalog tile (run `make thumbs`)"
    );
    let mut problems: Vec<String> = vec![];
    for t in c.tiles() {
        let files = &manifest[&t.id];
        if files.len() != 6 {
            problems.push(format!(
                "{}: {} rotations listed, needs 6",
                t.id,
                files.len()
            ));
            continue;
        }
        for (k, f) in files.iter().enumerate() {
            if !dir.join(f).is_file() {
                problems.push(format!(
                    "{} rotation {k}: assets/thumbs/{f} is missing",
                    t.id
                ));
            }
        }
        for a in 0..6u8 {
            for b in a + 1..6 {
                let same_image = files[a as usize] == files[b as usize];
                let same_sockets = (0..6).all(|w| t.edge(w, a) == t.edge(w, b));
                if same_image != same_sockets {
                    problems.push(format!(
                        "{}: rotations {a} and {b} {} one image but {} world-edge sockets",
                        t.id,
                        if same_image { "share" } else { "do not share" },
                        if same_sockets {
                            "have equal"
                        } else {
                            "differ in"
                        },
                    ));
                }
            }
        }
    }
    assert!(
        problems.is_empty(),
        "tile thumbnails (run `make thumbs`):\n{}",
        problems.join("\n")
    );
}

#[test]
fn start_tile_is_in_the_catalog() {
    assert!(catalog().get(hex_core::START_TILE).is_some());
}
