//! `scenario=kit`: the P1 family laid out for look-dev, the seam close-up and LOD checks.
//! Pieces come from `assets/kit/manifest.json` (compiled in); missing pieces warn and are skipped.
//! `kit_src=fixture` swaps in the kit gate fixture (served from `assets/kit/p1/fixture/`).
use bevy::{
    gltf::{Gltf, GltfMesh, GltfNode},
    prelude::*,
    render::{renderer::RenderDevice, settings::WgpuFeatures},
};
use hex_assets::{
    KitManifest, TextureFamily, lod_mesh_name, parse_manifest, rotor_mesh_name, select_family,
};
use hex_core::{Axial, HEX_RADIUS};
use std::collections::HashMap;

use crate::{
    SceneEntity,
    kit_material::{KitMaterial, build_material, slot_requests},
    scene::{Lod, lod_range},
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
    /// Height in meters: props sit on the base's relief at their spot (0 for bases and networks).
    pub y: f32,
}

const fn place(piece: &'static str, q: i32, r: i32, rot: u8) -> Placement {
    Placement {
        piece,
        cell: Axial { q, r },
        rot,
        offset: [0.0, 0.0],
        yaw_deg: 0.0,
        y: 0.0,
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
    // Farm hay bales (P3) on the oak's meadow, lifted onto the meadow relief at their spots.
    for (piece, offset, yaw_deg, y) in HAY_BALES {
        v.push(Placement {
            offset,
            yaw_deg,
            y,
            ..place(piece, 0, 1, 0)
        });
    }
    // The farm barn (P3, refs A17) on the oak's meadow: its long side along edge 2, its back 0.5 m
    // inside the border and its nearest corner 9.6 m from the oak (crown 8.55 m), on the meadow
    // relief at its centre (`relief()` in scripts/blender/kit/meadow_base.py).
    v.push(Placement {
        offset: [-4.75, 8.227],
        yaw_deg: 30.0,
        y: -0.154,
        ..place("barn_red", 0, 1, 0)
    });
    // The bakery (P3, refs A17) on road tile (1, 0), north of the road and facing it (yaw 90 turns
    // its +x shop front to +z), its LOD0 front (the bread basket, 3.73 m) 0.27 m clear of the road
    // profile (|z| <= 4.6 m), on the meadow relief at its centre (`relief()` in
    // scripts/blender/kit/meadow_base.py).
    v.push(Placement {
        offset: [1.5, -8.6],
        yaw_deg: 90.0,
        y: -0.062,
        ..place("bakery_shop", 1, 0, 0)
    });
    // Farm animals (P3) on the oak's meadow, in open sun on the oak's +x/+z side.
    for (piece, offset, yaw_deg, y) in FARM_ANIMALS {
        v.push(Placement {
            offset,
            yaw_deg,
            y,
            ..place(piece, 0, 1, 0)
        });
    }
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
    // Meadow props on the meadow at (-1, 1) (base rotation 4), each lifted onto the meadow relief at
    // its spot (`relief()` in scripts/blender/kit/meadow_base.py, evaluated in the base's frame).
    for (piece, offset, rot, y) in MEADOW_PROPS {
        v.push(Placement {
            offset,
            y,
            ..place(piece, -1, 1, rot)
        });
    }
    // The windmill (P3, refs approved under A17) alone on a meadow at (3, -1), the one water-patch
    // cell the layout left free (so `water_kit` keeps its 12 cells), its sails turned toward the
    // overview camera. Its rotor turns (`spin_rotors`).
    v.push(place("meadow_base", 3, -1, 0));
    v.push(Placement {
        yaw_deg: 120.0,
        y: WINDMILL_Y,
        ..place("windmill", 3, -1, 0)
    });
    // Rail (P3, refs A17) on two meadows south of the oak: the straight on (0, 2) and the sharp
    // bend on (1, 2) at rot 2 (lanes on world edges 2 and 3), so the bend's edge-3 lane meets the
    // straight's edge-0 lane and its other end runs out to the open rim rather than the ocean.
    v.push(place("meadow_base", 0, 2, 0));
    v.push(place("rail_straight_center", 0, 2, 0));
    v.push(place("meadow_base", 1, 2, 0));
    v.push(place("rail_bend_60_center", 1, 2, 2));
    // The train (P3, refs A17) on the rail: the engine and two carriages on the 8 m bend at (1, 2),
    // each with its outer axles on the track centreline and the inner buffers touching, wheels on the
    // rail top. Poses from docs/checkpoints/train/consist.py (the bend's frame, rot 2).
    for (piece, offset, yaw_deg) in TRAIN_ON_BEND {
        v.push(Placement {
            offset,
            yaw_deg,
            y: TRAIN_RAIL_TOP,
            ..place(piece, 1, 2, 2)
        });
    }
    // The level crossing (P3, refs A17) west of the straight on (-1, 2): its rail lane on edge 0
    // meets the straight's edge 3, and its road lanes on edges 1 and 4 carry road stubs.
    v.push(place("meadow_base", -1, 2, 0));
    v.push(place("rail_road_crossing_center", -1, 2, 0));
    v.push(place("road_straight_edge", -1, 2, 1));
    v.push(place("road_straight_edge", -1, 2, 4));
    v
}

/// The rail top above the cap (`RAIL_TOP` in scripts/blender/kit/_rail.py), where a train's wheels stand.
const TRAIN_RAIL_TOP: f32 = 0.46;

/// (piece, offset, yaw) of the train consist on the rail bend at (1, 2), rot 2.
const TRAIN_ON_BEND: [(&str, [f32; 2], f32); 3] = [
    ("train_engine", [9.490, 1.602], -34.3),
    ("train_carriage", [6.041, 8.814], -95.9),
    ("train_carriage", [9.072, 15.713], -120.0),
];

/// The meadow relief at the cap's centre (`relief(0, 0)` in scripts/blender/kit/meadow_base.py), where
/// the windmill stands.
const WINDMILL_Y: f32 = -0.233;

/// (piece, offset, yaw, relief height) of the hay bales on the oak's meadow (0, 1), base rotation 0.
const HAY_BALES: [(&str, [f32; 2], f32, f32); 3] = [
    ("hay_bale_round", [-2.0, 2.5], 20.0, -0.198),
    ("hay_bale_stack", [-9.5, 1.0], -15.0, -0.095),
    ("hay_bale_round", [-8.0, -4.5], 75.0, -0.129),
];

/// (piece, offset, yaw, relief height) of the farm animals on the oak's meadow (0, 1), base rotation 0.
const FARM_ANIMALS: [(&str, [f32; 2], f32, f32); 3] = [
    ("animal_cow", [7.0, 2.5], 70.0, -0.075),
    ("animal_sheep", [3.5, 7.0], -30.0, -0.089),
    ("animal_sheep", [2.0, 9.0], 140.0, -0.059),
];

/// (piece, offset, rotation, relief height) of the meadow props in the kit layout.
const MEADOW_PROPS: [(&str, [f32; 2], u8, f32); 15] = [
    ("rock_small", [-6.0, 5.0], 0, -0.124),
    ("rock_small", [5.0, 6.0], 2, -0.083),
    ("rock_small", [-2.5, -7.0], 4, -0.151),
    ("wildflower_clump", [-4.8, 4.2], 1, -0.172),
    ("wildflower_clump", [-6.9, 6.4], 3, -0.066),
    ("wildflower_clump", [-5.0, 6.6], 5, -0.104),
    ("wildflower_clump", [4.0, 7.2], 0, -0.091),
    ("wildflower_clump", [6.2, 4.6], 2, -0.075),
    ("wildflower_clump", [-3.6, -6.0], 1, -0.080),
    ("wildflower_clump", [-1.4, -8.0], 3, -0.129),
    ("wildflower_clump", [-7.4, 3.4], 4, -0.065),
    ("wildflower_clump", [0.0, 5.5], 2, -0.163),
    ("wildflower_clump", [-2.0, 3.0], 0, -0.138),
    ("wildflower_clump", [-8.5, 0.5], 5, -0.176),
    ("wildflower_clump", [2.0, 8.5], 1, -0.141),
];

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

/// Rotor speed (PRD §7.2: windmill sails turn as a rigid transform): a steady, slow 4 rpm.
pub const ROTOR_RPM: f64 = 4.0;

/// A spinning part of a kit instance: the rotor node's rest transform in its parent's frame (the
/// world here: placement, then the node's own transform; in play the hero's body part, so the node's
/// own transform) and its phase. It turns about its local +X axis.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct KitRotor {
    pub base: Transform,
    pub phase: f32,
}

/// The rotor angle (radians, in [0, 2π)) at `elapsed` seconds of the app clock: `--clock fixed`
/// steps that clock by exactly 1/60 s a frame, so a shot at frame N always sees the same angle.
pub fn rotor_angle(elapsed: f64, phase: f32) -> f32 {
    let w = ROTOR_RPM * std::f64::consts::TAU / 60.0;
    (elapsed * w + f64::from(phase)).rem_euclid(std::f64::consts::TAU) as f32
}

/// A per-instance phase from where the rotor stands (its hub, to the centimetre), so neighbouring
/// windmills do not turn in step. Integer hashing keeps native and web identical.
pub fn rotor_phase(at: Vec3) -> f32 {
    let q = |v: f32| (v * 100.0).round() as i32 as u32;
    let mut h = q(at.x).wrapping_mul(0x9E37_79B1) ^ q(at.z).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h >> 8) as f32 / (1u32 << 24) as f32 * std::f32::consts::TAU
}

pub fn spin_rotors(time: Res<Time>, mut rotors: Query<(&KitRotor, &mut Transform)>) {
    let t = time.elapsed_secs_f64();
    for (r, mut tf) in &mut rotors {
        *tf = r.base * Transform::from_rotation(Quat::from_rotation_x(rotor_angle(t, r.phase)));
    }
}

#[allow(clippy::too_many_arguments)]
pub fn finish_kit(
    mut commands: Commands,
    loading: Option<Res<KitLoading>>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    gltf_nodes: Res<Assets<GltfNode>>,
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
        let pos = Vec3::new(x, p.y, z) + rot * Vec3::new(p.offset[0], 0.0, p.offset[1]);
        let rot = placement_rotation(p);
        let place = Transform::from_translation(pos).with_rotation(rot);
        for lod in 0..3usize {
            let Some(mh) = g.named_meshes.get(lod_mesh_name(p.piece, lod).as_str()) else {
                warn!("kit: {} has no {}_lod{lod}", p.piece, p.piece);
                continue;
            };
            let Some(gm) = gltf_meshes.get(mh) else {
                continue;
            };
            let mut parts = vec![(gm, place, None)];
            // A rotor node draws at its own transform under the placement and turns about local X.
            if let Some(node) = g
                .named_nodes
                .get(rotor_mesh_name(p.piece, lod).as_str())
                .and_then(|h| gltf_nodes.get(h))
                && let Some(rm) = node.mesh.as_ref().and_then(|h| gltf_meshes.get(h))
            {
                let base = place * node.transform;
                let phase = rotor_phase(base.translation);
                parts.push((rm, base, Some(KitRotor { base, phase })));
            }
            for (prim, t, rotor) in parts
                .into_iter()
                .flat_map(|(gm, t, r)| gm.primitives.iter().map(move |prim| (prim, t, r)))
            {
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
                let mut e = commands.spawn((
                    Mesh3d(prim.mesh.clone()),
                    MeshMaterial3d(mat),
                    t,
                    lod_range(lod),
                    Lod(lod as u8),
                    SceneEntity,
                ));
                if let Some(r) = rotor {
                    e.insert(r);
                }
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
    fn rotor_turns_steadily_on_the_app_clock() {
        use std::f32::consts::{FRAC_PI_4, TAU};
        let period = 60.0 / ROTOR_RPM;
        assert!(rotor_angle(0.0, 0.0).abs() < 1e-6);
        assert!((rotor_angle(period / 8.0, 0.0) - FRAC_PI_4).abs() < 1e-5);
        let a = rotor_angle(period * 1000.0 + 1.0, 0.5);
        assert!(
            (a - rotor_angle(1.0, 0.5)).abs() < 1e-4,
            "no drift over long runs"
        );
        // 60 fixed frames (1 s) at 4 rpm turn the sails 24°.
        assert!((rotor_angle(1.0, 0.0) - 24f32.to_radians()).abs() < 1e-5);
        let p = rotor_phase(Vec3::new(69.3, 12.0, -24.0));
        assert!((0.0..TAU).contains(&p));
        assert_eq!(p, rotor_phase(Vec3::new(69.3, 12.0, -24.0)));
        assert_ne!(p, rotor_phase(Vec3::new(41.6, 12.0, -24.0)));
    }

    #[test]
    fn windmill_stands_on_its_own_meadow() {
        let l = kit_layout();
        let w = l.iter().find(|p| p.piece == "windmill").unwrap();
        assert!(
            l.iter()
                .any(|p| p.piece == "meadow_base" && p.cell == w.cell)
        );
        assert_eq!(l.iter().filter(|p| p.cell == w.cell).count(), 2);
        assert!(
            crate::water::layout().iter().any(|(c, _)| *c == w.cell),
            "inside the water look-dev patch"
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
    fn rail_straight_meets_the_bend() {
        let l = kit_layout();
        let at = |id: &str| *l.iter().find(|p| p.piece == id).unwrap();
        let (s, b) = (at("rail_straight_center"), at("rail_bend_60_center"));
        // The straight's lanes are local edges 0 and 3, the bend's 0 and 1, turned by rot.
        assert_eq!(s.cell.neighbor(s.rot), b.cell);
        assert_eq!((1 + b.rot) % 6, (s.rot + 3) % 6);
    }

    #[test]
    fn rail_crossing_continues_the_straight() {
        let l = kit_layout();
        let at = |id: &str| *l.iter().find(|p| p.piece == id).unwrap();
        let (s, x) = (at("rail_straight_center"), at("rail_road_crossing_center"));
        // The crossing's rail lane on its local edge 0 meets the straight's edge-3 lane.
        assert_eq!(x.cell.neighbor(x.rot), s.cell);
        assert_eq!(s.cell.neighbor((s.rot + 3) % 6), x.cell);
        // Road stubs on the crossing's road lanes (local edges 1 and 4).
        for k in [1, 4] {
            let e = (k + x.rot) % 6;
            assert!(
                l.iter()
                    .any(|p| p.piece == "road_straight_edge" && p.cell == x.cell && p.rot == e)
            );
        }
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
