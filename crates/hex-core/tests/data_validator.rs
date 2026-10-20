//! Build-time validator for `data/` (PRD §5.3, plan F1–F4).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use hex_core::SocketKind::{self, *};
use hex_core::{
    BUILTIN_TILES, EdgeProfile, PendingPieces, SOCKETS_RON, Side, TileCatalog, TileDef,
};

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

/// Plan F1 with the owner O1 coast corners, the U3 oak split from the forest and the windmill, the
/// farm, the wheat field and the bakery ahead of P3 (PRD §5.4): edges 0..5 for each inventory tile.
const F1: [(&str, [SocketKind; 6]); 22] = [
    ("meadow", [Grass, Grass, Grass, Grass, Grass, Grass]),
    ("forest", [Forest, Forest, Forest, Forest, Forest, Forest]),
    ("oak", [Forest, Forest, Forest, Forest, Forest, Forest]),
    ("windmill", [Grass, Grass, Grass, Grass, Grass, Grass]),
    ("farm", [Grass, Grass, Grass, Grass, Grass, Grass]),
    ("wheat_field", [Grass, Grass, Grass, Grass, Grass, Grass]),
    ("house_cluster", [Road, Grass, Grass, Grass, Grass, Grass]),
    ("bakery", [Road, Grass, Grass, Road, Grass, Grass]),
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
    ("river_spring", [River, Grass, Grass, Grass, Grass, Grass]),
    ("beach", [Ocean, Ocean, Beach, Grass, Grass, Beach]),
    ("beach_outer", [Ocean, Ocean, Beach, Grass, Beach, Ocean]),
    ("beach_inner", [Ocean, Beach, Grass, Grass, Grass, Beach]),
    ("river_mouth", [Ocean, Ocean, Beach, River, Grass, Beach]),
    ("river_mouth_l", [Ocean, Ocean, Beach, Grass, River, Beach]),
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
    let c = TileCatalog::from_ron(SOCKETS_RON, BUILTIN_TILES).unwrap_or_else(|e| panic!("{e}"));
    let ids: BTreeSet<&str> = c.tiles().iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids.len(), c.tiles().len());
}

#[test]
fn the_f1_tiles_have_exactly_the_f1_edge_kinds() {
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

/// Owner O1: every beach edge names the side its land is on, looking out across it from its own
/// tile (`Side::Right` is the corner shared with edge `e + 1`). The land corner is shared with a
/// grass, forest or river edge (a river mouth, water contract §3a) and the sea corner with an ocean
/// edge, and every coast tile carries one coastline: one beach edge with the land on its right and
/// one with it on its left.
#[test]
fn every_beach_edge_names_the_side_its_land_is_on() {
    let mut coasts = 0;
    for t in catalog().tiles() {
        let beach: Vec<usize> = (0..6).filter(|&e| t.edges[e].kind == Beach).collect();
        if beach.is_empty() {
            continue;
        }
        coasts += 1;
        let mut rights = 0;
        for &e in &beach {
            let land = t.edges[e]
                .land
                .unwrap_or_else(|| panic!("{} edge {e} has no land side", t.id));
            let (land_edge, sea_edge) = match land {
                Side::Right => ((e + 1) % 6, (e + 5) % 6),
                Side::Left => ((e + 5) % 6, (e + 1) % 6),
            };
            let (l, o) = (t.edges[land_edge].kind, t.edges[sea_edge].kind);
            assert!(
                matches!(l, Grass | Forest | River),
                "{} edge {e}: land on the {land:?} faces edge {land_edge} {l:?}",
                t.id
            );
            assert_eq!(
                o, Ocean,
                "{} edge {e}: sea side faces edge {sea_edge}",
                t.id
            );
            rights += usize::from(land == Side::Right);
        }
        assert_eq!((beach.len(), rights), (2, 1), "{}: one coastline", t.id);
    }
    assert_eq!(
        coasts, 5,
        "beach, beach_outer, beach_inner, river_mouth and river_mouth_l"
    );
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
    // Pair edge pieces are generated names (`data/sockets.ron` `pair_edge_pieces`): every name an
    // entry can produce must be built or pending, not just the ones a current tile reaches.
    let pairs = c.sockets().pair_piece_names();
    let missing: Vec<&str> = c
        .kit_refs()
        .into_iter()
        .chain(pairs.iter().map(|r| r.as_str()))
        .filter(|id| !built.contains(*id) && !pending.contains(id))
        .collect();
    assert!(
        missing.is_empty(),
        "kit pieces neither in assets/kit/manifest.json nor data/pending-pieces.ron: {missing:?}"
    );
}

#[test]
fn pair_edge_piece_names_are_checked() {
    let names = catalog().sockets().pair_piece_names();
    let built = manifest_piece_ids();
    assert!(
        !names.is_empty(),
        "data/sockets.ron has no pair_edge_pieces"
    );
    for k in 0..6 {
        let id = format!("forest_forest_edge_e{k}");
        assert!(
            names.iter().any(|n| n.as_str() == id) && built.contains(&id),
            "{id}"
        );
    }
}

#[test]
fn pending_list_is_honest() {
    let c = catalog();
    let built = manifest_piece_ids();
    let pairs = c.sockets().pair_piece_names();
    let referenced: BTreeSet<&str> = c
        .kit_refs()
        .into_iter()
        .chain(pairs.iter().map(|r| r.as_str()))
        .collect();
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

/// PRD §7.1: a tile's parking spots are exactly its centre piece's `anchor-park-<n>` sockets, at
/// the same place and heading. Read from the piece's committed provenance (`sockets`, copied from
/// the export report); kit_gate checks the same against the GLB itself.
#[test]
fn parking_spots_match_the_centre_piece_anchors() {
    let mut problems = vec![];
    let mut checked = 0;
    let cat = catalog();
    for def in cat.tiles() {
        if def.parking.is_empty() {
            continue;
        }
        let Some(center) = &def.center else {
            problems.push(format!("{}: parking without a centre piece", def.id));
            continue;
        };
        let path = repo_root().join(format!(
            "assets-source/kit/{}/provenance.json",
            center.as_str()
        ));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
        let prov: serde_json::Value = serde_json::from_str(&text).unwrap();
        let anchors: BTreeMap<String, &serde_json::Value> = prov["sockets"]
            .as_array()
            .expect("provenance sockets")
            .iter()
            .filter_map(|s| {
                let name = s["name"].as_str()?;
                name.strip_prefix("anchor-").map(|id| (id.to_owned(), s))
            })
            .filter(|(id, _)| id.starts_with("park-"))
            .collect();
        let spots: BTreeSet<&str> = def.parking.iter().map(|s| s.id.as_str()).collect();
        let names: BTreeSet<&str> = anchors.keys().map(String::as_str).collect();
        if spots != names {
            problems.push(format!(
                "{}: spots {spots:?} differ from {} anchors {names:?}",
                def.id,
                center.as_str()
            ));
        }
        for spot in &def.parking {
            let Some(a) = anchors.get(&spot.id) else {
                continue;
            };
            let p: Vec<f64> = a["position"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect();
            // Runtime (x, y, z); tile-local (x, z).
            let (dx, dz) = (p[0] - f64::from(spot.at[0]), p[2] - f64::from(spot.at[1]));
            if dx.hypot(dz) > 1e-3 {
                problems.push(format!(
                    "{}: {} at {:?}, anchor at ({}, {})",
                    def.id, spot.id, spot.at, p[0], p[2]
                ));
            }
            // The export report's yaw is Blender's: a Blender yaw θ faces runtime (cos θ, -sin θ).
            let blender = a["yaw"].as_f64().unwrap().to_radians();
            let want = f64::from(spot.yaw_deg).to_radians();
            let off = (blender.cos() - want.cos()).hypot(-blender.sin() - want.sin());
            let extra = a["extras"]["heading_deg"].as_f64().unwrap_or(f64::NAN);
            if off > 1e-3 || (extra - f64::from(spot.yaw_deg)).abs() > 1e-3 {
                problems.push(format!(
                    "{}: {} faces {} deg, anchor yaw {} (Blender), heading_deg {extra}",
                    def.id,
                    spot.id,
                    spot.yaw_deg,
                    blender.to_degrees()
                ));
            }
            checked += 1;
        }
    }
    assert!(checked > 0, "no parking spot was checked");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
