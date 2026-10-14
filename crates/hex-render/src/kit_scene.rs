//! `scenario=kit`: the P1 family laid out for look-dev, the seam close-up and LOD checks.
//! Pieces come from `assets/kit/manifest.json` (compiled in); missing pieces warn and are skipped.
//! `kit_src=fixture` swaps in the kit gate fixture (served from `assets/kit/p1/fixture/`).
use bevy::{
    gltf::{Gltf, GltfMesh},
    prelude::*,
    render::{renderer::RenderDevice, settings::WgpuFeatures},
};
use hex_assets::{KitManifest, TextureFamily, parse_manifest, select_family};
use hex_core::{Axial, HEX_RADIUS};
use std::collections::HashMap;

use crate::{
    SceneEntity,
    kit_material::{KitMaterial, build_material, slot_requests},
    scene::lod_range,
    workload::{GateConfig, KitSource},
};

pub const MANIFEST: &str = include_str!("../../../assets/kit/manifest.json");
/// The gate fixture's manifest; its files are copied to `assets/kit/p1/fixture/` so `kit://`
/// serves them natively and on the web.
pub const FIXTURE_MANIFEST: &str = include_str!("../tests/fixtures/kit/manifest.json");
/// The fixture with its cap on a probe tileable set (`kit_src=fixture_tiled`, smoke only).
pub const FIXTURE_TILED_MANIFEST: &str =
    include_str!("../../../assets/kit/p1/fixture_tile/manifest.json");

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub piece: &'static str,
    pub cell: Axial,
    /// Rotation k: edge e maps to edge (e + k) mod 6 (PRD §5.1).
    pub rot: u8,
    /// Offset from the cell center in meters (x, z), applied after rotation.
    pub offset: [f32; 2],
    /// Extra turn about +Y after `rot`, in degrees, the way edges run (90 maps +x to +z): a hero
    /// prop's `yaw_deg` (`data/tiles/*.ron`).
    pub yaw_deg: f32,
}

const fn place(piece: &'static str, q: i32, r: i32, rot: u8) -> Placement {
    Placement {
        piece,
        cell: Axial { q, r },
        rot,
        offset: [0.0, 0.0],
        yaw_deg: 0.0,
    }
}

/// A hero prop at tile-local runtime `at` (x, z) with `yaw_deg`, on a tile placed at rot 0.
const fn hero(piece: &'static str, q: i32, r: i32, at: [f32; 2], yaw_deg: f32) -> Placement {
    Placement {
        offset: at,
        yaw_deg,
        ..place(piece, q, r, 0)
    }
}

/// Two road-straight tiles meeting at (0,0) edge 0 / (1,0) edge 3, ringed by meadow at every
/// rotation (Review Focus 1), and the hero tree on (0,1).
pub fn kit_layout() -> Vec<Placement> {
    let mut v = vec![];
    for (q, r) in [(0, 0), (1, 0)] {
        v.push(place("meadow_base", q, r, 0));
        v.push(place("road_straight_center", q, r, 0));
        v.push(place("road_straight_edge", q, r, 0));
        v.push(place("road_straight_edge", q, r, 3));
    }
    for (k, (q, r)) in [(0, 1), (1, -1), (-1, 0), (0, -1), (-1, 1), (2, -1)]
        .into_iter()
        .enumerate()
    {
        v.push(place("meadow_base", q, r, k as u8));
    }
    v.push(Placement {
        offset: [3.0, -2.0],
        ..place("tree_hero_oak", 0, 1, 0)
    });
    // house_cluster (data/tiles/house_cluster.ron) on the meadow at (-1, 0): its edge-0 road stub
    // meets road tile (0, 0)'s edge-3 stub, so the road runs on into the cluster's driveway.
    v.push(place("house_pad", -1, 0, 0));
    v.push(place("road_straight_edge", -1, 0, 0));
    v.push(hero("house_cottage", -1, 0, [-0.5, -7.0], 90.0));
    v.push(hero("house_cottage", -1, 0, [-0.5, 7.0], 270.0));
    v.push(hero("house_twostorey", -1, 0, [-7.5, 0.0], 0.0));
    // Road-family centre pieces on ring cells, each with its edge stubs. The wide bend on (0, -1)
    // turns from world edge 3 to world edge 5, both on the layout's outer rim.
    v.push(place("road_bend_120_center", 0, -1, 3));
    v.push(place("road_straight_edge", 0, -1, 3));
    v.push(place("road_straight_edge", 0, -1, 5));
    // The sharp bend on (2,-1), lanes on world edges 2 and 3.
    v.push(place("road_bend_60_center", 2, -1, 2));
    v.push(place("road_straight_edge", 2, -1, 2));
    v.push(place("road_straight_edge", 2, -1, 3));
    // The Y junction on (1,-1); its edge 0 meets the sharp bend's edge-3 lane.
    v.push(place("road_junction_center", 1, -1, 0));
    for k in [0, 2, 4] {
        v.push(place("road_straight_edge", 1, -1, k));
    }
    // Three ocean tiles meeting at one corner, each at another rotation (seams under rotation), with
    // open edges to the east. Cells stay inside the water look-dev patch, so `water_kit` keeps 12.
    for (q, r, k) in [(1, 1, 0), (2, 1, 3), (2, 0, 1)] {
        v.push(place("ocean_base", q, r, k));
        v.push(place("ocean_center", q, r, k));
    }
    v
}

/// The gate fixture on the same seam as the road tiles, plus one rotated neighbour.
pub fn fixture_layout() -> Vec<Placement> {
    vec![
        place("fixture", 0, 0, 0),
        place("fixture", 1, 0, 3),
        place("fixture", 0, 1, 1),
    ]
}

/// The manifest and layout a `kit_src` selects.
pub fn source(src: KitSource) -> (&'static str, Vec<Placement>) {
    match src {
        KitSource::Runtime => (MANIFEST, kit_layout()),
        KitSource::Fixture => (FIXTURE_MANIFEST, fixture_layout()),
        KitSource::FixtureTiled => (FIXTURE_TILED_MANIFEST, fixture_layout()),
    }
}

pub fn piece_rotation(rot: u8) -> Quat {
    // Edges run clockwise viewed from +Y: +x -> +z is a negative rotation about +Y.
    Quat::from_rotation_y(-(60.0 * (rot % 6) as f32).to_radians())
}

/// The piece's full rotation: its edge rotation `rot`, then its hero `yaw_deg` (same sense).
pub fn placement_rotation(p: &Placement) -> Quat {
    piece_rotation(p.rot) * Quat::from_rotation_y(-p.yaw_deg.to_radians())
}

pub fn resolve_placements(m: &KitManifest, layout: &[Placement]) -> (Vec<Placement>, Vec<String>) {
    let mut warn = vec![];
    let ok = layout
        .iter()
        .filter(|p| {
            let found = m.pieces.iter().any(|e| e.id == p.piece);
            if !found && !warn.iter().any(|w: &String| w.contains(p.piece)) {
                warn.push(format!("kit: piece {} not in manifest, skipped", p.piece));
            }
            found
        })
        .copied()
        .collect();
    (ok, warn)
}

#[derive(Resource)]
pub struct KitPending;

#[derive(Resource)]
pub struct KitLoading {
    manifest: KitManifest,
    items: Vec<(Placement, Handle<Gltf>)>,
    family: TextureFamily,
}

pub fn start_kit(
    mut commands: Commands,
    pending: Option<Res<KitPending>>,
    device: Option<Res<RenderDevice>>,
    server: Res<AssetServer>,
    config: Res<GateConfig>,
) {
    let (Some(_), Some(device)) = (pending, device) else {
        return;
    };
    commands.remove_resource::<KitPending>();
    let f = device.features();
    let family = select_family(
        f.contains(WgpuFeatures::TEXTURE_COMPRESSION_BC),
        f.contains(WgpuFeatures::TEXTURE_COMPRESSION_ASTC),
    );
    let (json, layout) = source(config.kit_src);
    let manifest = match parse_manifest(json) {
        Ok(m) => m,
        Err(e) => {
            warn!("kit: {e}");
            return;
        }
    };
    let (placements, warnings) = resolve_placements(&manifest, &layout);
    for w in warnings {
        warn!("{w}");
    }
    let items = placements
        .into_iter()
        .filter_map(|p| {
            let e = manifest.pieces.iter().find(|e| e.id == p.piece)?;
            Some((p, server.load(format!("kit://{}", e.glb))))
        })
        .collect();
    info!("kit: texture family {family:?}");
    commands.insert_resource(KitLoading {
        manifest,
        items,
        family,
    });
}

pub fn finish_kit(
    mut commands: Commands,
    loading: Option<Res<KitLoading>>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    mut materials: ResMut<Assets<KitMaterial>>,
    config: Res<GateConfig>,
) {
    let Some(loading) = loading else {
        return;
    };
    // A failed dependency leaves the root `Loaded` but never "loaded with dependencies", so it
    // counts as failed here; otherwise the wait would never end (review M4).
    let failed = |h: &Handle<Gltf>| {
        server.load_state(h).is_failed() || server.recursive_dependency_load_state(h).is_failed()
    };
    if loading
        .items
        .iter()
        .any(|(_, h)| !server.is_loaded_with_dependencies(h) && !failed(h))
    {
        return;
    }
    commands.remove_resource::<KitLoading>();
    let manifest = &loading.manifest;
    let mut cache: HashMap<(String, String), Option<Handle<KitMaterial>>> = HashMap::new();
    let mut spawned = 0;
    for (p, h) in &loading.items {
        if failed(h) {
            warn!(
                "kit: {} or one of its dependencies failed to load, skipped",
                p.piece
            );
            continue;
        }
        let Some(g) = gltfs.get(h) else {
            warn!("kit: {} failed to load, skipped", p.piece);
            continue;
        };
        let Some(entry) = manifest.pieces.iter().find(|e| e.id == p.piece) else {
            continue;
        };
        let [x, z] = p.cell.to_world(HEX_RADIUS);
        let rot = piece_rotation(p.rot);
        let pos = Vec3::new(x, 0.0, z) + rot * Vec3::new(p.offset[0], 0.0, p.offset[1]);
        let rot = placement_rotation(p);
        for lod in 0..3usize {
            let Some(mh) = g.named_meshes.get(format!("{}_lod{lod}", p.piece).as_str()) else {
                warn!("kit: {} has no {}_lod{lod}", p.piece, p.piece);
                continue;
            };
            let Some(gm) = gltf_meshes.get(mh) else {
                continue;
            };
            for prim in &gm.primitives {
                let slot_name = prim
                    .material
                    .as_ref()
                    .and_then(|mh| {
                        g.named_materials
                            .iter()
                            .find(|(_, v)| *v == mh)
                            .map(|(k, _)| k.to_string())
                    })
                    .unwrap_or_default();
                let Some(slot) = entry.slots.get(&slot_name) else {
                    warn!("kit: {} material {slot_name} has no manifest slot", p.piece);
                    continue;
                };
                let mat = cache
                    .entry((p.piece.to_string(), slot_name.clone()))
                    .or_insert_with(
                        || match slot_requests(manifest, entry, slot, loading.family) {
                            Ok(r) => Some(materials.add(build_material(
                                &server,
                                slot,
                                &r,
                                config.kit_hooks,
                            ))),
                            Err(e) => {
                                warn!("kit: {e}, slot {slot_name} skipped");
                                None
                            }
                        },
                    )
                    .clone();
                let Some(mat) = mat else {
                    continue;
                };
                commands.spawn((
                    Mesh3d(prim.mesh.clone()),
                    MeshMaterial3d(mat),
                    Transform::from_translation(pos).with_rotation(rot),
                    lod_range(lod),
                    SceneEntity,
                ));
                spawned += 1;
            }
        }
    }
    info!(
        "kit: spawned {spawned} meshes, {} materials",
        cache.values().flatten().count()
    );
}

/// Distinct hex cells a kit source lays out (`state().tiles` for the kit scenario).
pub fn cell_count(src: KitSource) -> usize {
    let mut cells: Vec<_> = source(src).1.iter().map(|p| (p.cell.q, p.cell.r)).collect();
    cells.sort();
    cells.dedup();
    cells.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_assets::parse_manifest;

    #[test]
    fn rotation_maps_edge0_to_edge_k() {
        // Edge k's outward direction in XZ is at 60°·k (axial.rs: edge 1 = +z).
        for k in 0..6u8 {
            let v = piece_rotation(k) * Vec3::X;
            let a = (60.0 * k as f32).to_radians();
            assert!(
                (v - Vec3::new(a.cos(), 0.0, a.sin())).length() < 1e-5,
                "k={k} {v}"
            );
        }
    }

    #[test]
    fn hero_yaw_turns_the_way_edges_run() {
        // data/tiles yaw_deg: 90 maps +x to +z, as edge 1 does; it composes after the tile's rot.
        let p = |rot, yaw_deg| Placement {
            yaw_deg,
            ..place("house_cottage", 0, 0, rot)
        };
        let x = |p: Placement| placement_rotation(&p) * Vec3::X;
        assert!((x(p(0, 90.0)) - Vec3::Z).length() < 1e-5);
        assert!((x(p(0, 60.0)) - piece_rotation(1) * Vec3::X).length() < 1e-5);
        assert!((x(p(1, 60.0)) - piece_rotation(2) * Vec3::X).length() < 1e-5);
    }

    #[test]
    fn house_cluster_continues_the_road() {
        // The cluster's edge-0 road stub meets road tile (0, 0)'s edge-3 stub.
        let l = kit_layout();
        let pad = l.iter().find(|p| p.piece == "house_pad").unwrap();
        assert_eq!(pad.cell.neighbor(0), Axial { q: 0, r: 0 });
        assert!(
            l.iter()
                .any(|p| p.piece == "road_straight_edge" && p.cell == pad.cell && p.rot == 0)
        );
    }

    #[test]
    fn road_tiles_meet_at_edge0() {
        let l = kit_layout();
        let roads: Vec<_> = l
            .iter()
            .filter(|p| p.piece == "road_straight_center")
            .collect();
        assert_eq!(roads.len(), 2);
        assert_eq!(roads[0].cell.neighbor(0), roads[1].cell);
    }

    #[test]
    fn missing_piece_is_skipped_not_panic() {
        let m = parse_manifest(r#"{"version":1,"materials":{},"pieces":[]}"#).unwrap();
        let (ok, warn) = resolve_placements(&m, &kit_layout());
        assert!(ok.is_empty());
        assert!(warn.iter().any(|w| w.contains("meadow_base")));
    }

    #[test]
    fn fixture_source_resolves_every_placement() {
        let (json, layout) = source(KitSource::Fixture);
        let m = parse_manifest(json).unwrap();
        let (ok, warn) = resolve_placements(&m, &layout);
        assert!(warn.is_empty(), "{warn:?}");
        assert_eq!(ok.len(), layout.len());
        // Two fixtures share edge 0 of (0,0); the third sits on a rotated neighbour.
        assert_eq!(layout[0].cell.neighbor(0), layout[1].cell);
        assert!(layout.iter().any(|p| p.rot != 0));
    }

    #[test]
    fn fixture_tiled_source_binds_a_tileable_set() {
        let (json, layout) = source(KitSource::FixtureTiled);
        let m = parse_manifest(json).unwrap();
        let (ok, warn) = resolve_placements(&m, &layout);
        assert!(warn.is_empty() && ok.len() == layout.len(), "{warn:?}");
        let p = &m.pieces[0];
        let r = crate::kit_material::slot_requests(
            &m,
            p,
            &p.slots["fixture_cap"],
            hex_assets::TextureFamily::Png,
        )
        .unwrap();
        assert!(r.tile.is_some() && r.masks.is_some());
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/kit");
        use hex_assets::TextureFamily::{Astc, Bc7, Png};
        for f in [Png, Bc7, Astc] {
            let r = crate::kit_material::slot_requests(&m, p, &p.slots["fixture_cap"], f).unwrap();
            for mr in r.tile.unwrap().0 {
                assert!(repo.join(&mr.path).is_file(), "{} missing", mr.path);
            }
        }
    }

    #[test]
    fn runtime_source_is_the_kit_layout() {
        let (json, layout) = source(KitSource::Runtime);
        assert!(parse_manifest(json).is_ok());
        assert_eq!(layout, kit_layout());
    }

    /// The fixture files under `assets/kit/p1/fixture/` (served by `kit://`) are copies of the
    /// gate fixture and must not drift from the files `kit_gate` checks.
    #[test]
    fn served_fixture_matches_the_gate_fixture() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let gate = repo.join("crates/hex-render/tests/fixtures/kit/p1/fixture");
        let served = repo.join("assets/kit/p1/fixture");
        let mut n = 0;
        for e in std::fs::read_dir(&gate).unwrap() {
            let name = e.unwrap().file_name();
            let a = std::fs::read(gate.join(&name)).unwrap();
            let b = std::fs::read(served.join(&name))
                .unwrap_or_else(|e| panic!("assets/kit/p1/fixture/{name:?}: {e}"));
            assert!(a == b, "{name:?} differs from the gate fixture");
            n += 1;
        }
        assert_eq!(n, std::fs::read_dir(&served).unwrap().count());
    }
}
