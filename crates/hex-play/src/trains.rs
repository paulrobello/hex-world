//! Trains in `scenario=play` (PRD §7.2, P3). `hex-app` runs the rail simulation and writes every
//! vehicle's interpolated pose into [`TrainsView`]; this module keeps one entity per vehicle in step
//! with it.
//!
//! A train is the steam tank engine (`train_engine`), a carriage (`train_carriage`) and the brake
//! third last (`train_carriage_b`, the same length and axles as the carriage), each standing on the
//! rail top ([`RAIL_TOP_Y`], the same over the level crossing's
//! deck and the river bridge) and painted at runtime (PRD §6) like the cars: one model and one
//! material per piece, the livery picked from [`LIVERIES`] by the train id and carried as a
//! per-entity [`MeshTag`], so every vehicle of a piece and LOD draws in one instanced batch.
//! Until the pieces load (they stream with the village pack) no train is drawn.
use std::collections::{HashMap, HashSet};

use bevy::{
    gltf::{Gltf, GltfMesh},
    mesh::MeshTag,
    prelude::*,
    render::{renderer::RenderDevice, settings::WgpuFeatures},
};
use hex_assets::{KitManifest, TextureFamily, parse_manifest, select_family};

use crate::{
    ActiveScenario, Scenario, SceneEntity,
    car_material::{CarMaterial, build_material},
    cars::{CAR_PACK, CarPart, paint_tag},
    kit_material::slot_requests,
    kit_scene::MANIFEST,
    play::packs::{PackState, PlayPacks},
    scene::{Lod, lod_range},
    workload::GateConfig,
};

/// Rail top over the ground plane on every rail piece, the level crossing's deck and the river
/// bridge's arch (the pieces' bake reports: `railTopM`, `deckTopM`). The train pieces stand on it.
pub const RAIL_TOP_Y: f32 = 0.46;

/// Rolling stock, one kit piece each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StockKind {
    Engine,
    Carriage,
    /// The brake third, last in every train (`hex_sim::rail::CONSIST`): its veranda and roof
    /// lookout at the piece's -x end, the train's tail.
    BrakeCarriage,
}

impl StockKind {
    pub const ALL: [StockKind; 3] = [
        StockKind::Engine,
        StockKind::Carriage,
        StockKind::BrakeCarriage,
    ];

    pub fn piece(self) -> &'static str {
        match self {
            StockKind::Engine => "train_engine",
            StockKind::Carriage => "train_carriage",
            StockKind::BrakeCarriage => "train_carriage_b",
        }
    }
}

/// Railway liveries (sRGB) for the engine and its carriages: lined green, crimson lake, brunswick
/// green, Caledonian blue, black and chocolate (owner Q11 is open; these are the default).
pub const LIVERIES: [[u8; 3]; 6] = [
    [0x2f, 0x5d, 0x3a],
    [0x7a, 0x1f, 0x22],
    [0x1d, 0x3b, 0x2a],
    [0x2c, 0x4f, 0x8a],
    [0x1a, 0x1b, 0x1d],
    [0x5a, 0x34, 0x22],
];

/// The livery for a train id: the whole train wears one colour.
pub fn livery_for(train: u64) -> [u8; 3] {
    LIVERIES[(train % LIVERIES.len() as u64) as usize]
}

/// One vehicle's pose between the last two sim ticks, in world XZ metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StockPose {
    /// Unique per vehicle: the train id times 4 plus the vehicle's place in the consist.
    pub id: u64,
    pub train: u64,
    pub kind: StockKind,
    pub pos: [f32; 2],
    /// Heading in radians from +x toward +z, toward the engine end of the train.
    pub yaw: f32,
}

/// The vehicles to draw, written by `hex-app` every frame while any train exists.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct TrainsView {
    pub vehicles: Vec<StockPose>,
}

/// A drawn vehicle, tagged with its view id.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayStock(pub u64);

/// One drawn part of a vehicle (a slot of a piece at one LOD).
#[derive(Component, Clone, Copy, Debug)]
pub struct StockPart;

/// How the trains draw (`hexWorld.state().trains_draw`), as JSON: `vehicles`, `parts` (part
/// entities), `visible_parts` (in any view this frame) and `batches`, the distinct mesh and
/// material pairs among the visible parts, per LOD in `batches_by_lod`. Every vehicle of a piece
/// shares one mesh per LOD and one material per slot and carries its livery as a [`MeshTag`], so
/// the batches stay one per piece, LOD and slot however many trains run.
#[derive(Resource, Clone, Debug, Default)]
pub struct TrainReport(pub String);

/// The train kit pieces, loaded with the village pack in the play scenario.
#[derive(Resource, Default)]
pub enum TrainKit {
    #[default]
    Idle,
    Loading {
        manifest: KitManifest,
        items: Vec<(StockKind, Handle<Gltf>)>,
        family: TextureFamily,
    },
    Ready(HashMap<StockKind, Vec<CarPart>>),
}

pub struct TrainsPlugin;

impl Plugin for TrainsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TrainsView>()
            .init_resource::<TrainKit>()
            .init_resource::<TrainReport>()
            .add_systems(Update, (start_kit, finish_kit).chain())
            .add_systems(
                PostUpdate,
                (
                    sync.before(bevy::transform::TransformSystems::Propagate),
                    report.after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
                ),
            );
    }
}

/// Starts loading the train pieces once the play scenario has requested the village pack.
pub fn start_kit(
    mut kit: ResMut<TrainKit>,
    active: Res<ActiveScenario>,
    device: Option<Res<RenderDevice>>,
    server: Res<AssetServer>,
    packs: Option<Res<PlayPacks>>,
) {
    let (Scenario::Play, TrainKit::Idle, Some(device)) = (active.0, &*kit, device) else {
        return;
    };
    if packs
        .and_then(|p| p.state(CAR_PACK))
        .is_none_or(|s| s == PackState::Idle)
    {
        return;
    }
    let f = device.features();
    let family = select_family(
        f.contains(WgpuFeatures::TEXTURE_COMPRESSION_BC),
        f.contains(WgpuFeatures::TEXTURE_COMPRESSION_ASTC),
    );
    let manifest = match parse_manifest(MANIFEST) {
        Ok(m) => m,
        Err(e) => {
            warn!("trains: {e}, drawing none");
            *kit = TrainKit::Ready(HashMap::new());
            return;
        }
    };
    let items = StockKind::ALL
        .iter()
        .filter_map(|k| {
            let e = manifest.pieces.iter().find(|e| e.id == k.piece())?;
            Some((*k, server.load(format!("kit://{}", e.glb))))
        })
        .collect();
    *kit = TrainKit::Loading {
        manifest,
        items,
        family,
    };
}

/// Builds each loaded piece's parts: every primitive of `<id>_lod{0,1,2}` with its slot's
/// [`CarMaterial`] (one material per slot, shared by every vehicle of the piece).
pub fn finish_kit(
    mut kit: ResMut<TrainKit>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    mut materials: ResMut<Assets<CarMaterial>>,
    config: Res<GateConfig>,
) {
    let TrainKit::Loading {
        manifest,
        items,
        family,
    } = &*kit
    else {
        return;
    };
    let failed = |h: &Handle<Gltf>| {
        server.load_state(h).is_failed() || server.recursive_dependency_load_state(h).is_failed()
    };
    if items
        .iter()
        .any(|(_, h)| !server.is_loaded_with_dependencies(h) && !failed(h))
    {
        return;
    }
    let mut ready = HashMap::new();
    for (kind, h) in items {
        let id = kind.piece();
        let (Some(g), Some(entry)) = (
            gltfs.get(h).filter(|_| !failed(h)),
            manifest.pieces.iter().find(|e| e.id == id),
        ) else {
            warn!("trains: {id} failed to load, not drawn");
            continue;
        };
        let mut parts = vec![];
        let mut by_slot: HashMap<String, Option<Handle<CarMaterial>>> = HashMap::new();
        for lod in 0..3usize {
            let Some(gm) = g
                .named_meshes
                .get(format!("{id}_lod{lod}").as_str())
                .and_then(|mh| gltf_meshes.get(mh))
            else {
                warn!("trains: {id} has no {id}_lod{lod}");
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
                    warn!("trains: {id} material {slot_name} has no manifest slot");
                    continue;
                };
                let mat = by_slot
                    .entry(slot_name.clone())
                    .or_insert_with(|| match slot_requests(manifest, entry, slot, *family) {
                        Ok(r) => {
                            Some(materials.add(build_material(&server, slot, &r, config.kit_hooks)))
                        }
                        Err(e) => {
                            warn!("trains: {e}, slot {slot_name} skipped");
                            None
                        }
                    })
                    .clone();
                if let Some(material) = mat {
                    parts.push(CarPart {
                        lod,
                        mesh: prim.mesh.clone(),
                        material,
                    });
                }
            }
        }
        if !parts.is_empty() {
            ready.insert(*kind, parts);
        }
    }
    info!(
        "trains: kit pieces ready: {:?}",
        ready.keys().collect::<Vec<_>>()
    );
    *kit = TrainKit::Ready(ready);
}

/// A vehicle standing on the rail top at `p`, its +x along its heading.
pub fn stock_transform(p: &StockPose) -> Transform {
    // Bevy turns +x toward -z for a positive angle about +Y, so the yaw (+x toward +z) is negated.
    Transform::from_xyz(p.pos[0], RAIL_TOP_Y, p.pos[1]).with_rotation(Quat::from_rotation_y(-p.yaw))
}

/// Spawns, moves and despawns vehicle entities to match [`TrainsView`] by id. A vehicle is a parent
/// with the pose and one child per part (slot x LOD), every child tagged with the train's livery.
pub fn sync(
    mut commands: Commands,
    active: Res<ActiveScenario>,
    view: Res<TrainsView>,
    kit: Res<TrainKit>,
    mut stock: Query<(Entity, &PlayStock, &mut Transform)>,
) {
    let TrainKit::Ready(kit) = &*kit else {
        return;
    };
    let want: HashMap<u64, &StockPose> = if active.0 == Scenario::Play {
        view.vehicles.iter().map(|v| (v.id, v)).collect()
    } else {
        HashMap::new()
    };
    if want.is_empty() && stock.is_empty() {
        return;
    }
    let mut have = HashSet::with_capacity(want.len());
    for (e, s, mut t) in &mut stock {
        match want.get(&s.0) {
            Some(p) if have.insert(s.0) => *t = stock_transform(p),
            _ => commands.entity(e).try_despawn(),
        }
    }
    for (id, p) in want {
        if have.contains(&id) {
            continue;
        }
        let Some(parts) = kit.get(&p.kind) else {
            continue;
        };
        let tag = MeshTag::new(paint_tag(livery_for(p.train)));
        commands
            .spawn((
                PlayStock(id),
                stock_transform(p),
                Visibility::default(),
                SceneEntity,
            ))
            .with_children(|c| {
                for part in parts {
                    c.spawn((
                        StockPart,
                        Mesh3d(part.mesh.clone()),
                        MeshMaterial3d(part.material.clone()),
                        tag.clone(),
                        lod_range(part.lod),
                        Lod(part.lod as u8),
                    ));
                }
            });
    }
}

/// Writes [`TrainReport`] from the parts' visibility this frame.
pub fn report(
    mut out: ResMut<TrainReport>,
    stock: Query<(), With<PlayStock>>,
    parts: Query<(&Mesh3d, &MeshMaterial3d<CarMaterial>, &ViewVisibility, &Lod), With<StockPart>>,
) {
    let vehicles = stock.iter().count();
    if vehicles == 0 && out.0.is_empty() {
        return;
    }
    let mut pairs: [HashSet<(AssetId<Mesh>, AssetId<CarMaterial>)>; 3] = Default::default();
    let mut visible = 0;
    for (mesh, material, vis, lod) in &parts {
        if vis.get() {
            visible += 1;
            pairs[(lod.0 as usize).min(2)].insert((mesh.0.id(), material.0.id()));
        }
    }
    let by_lod = pairs.each_ref().map(HashSet::len);
    out.0 = format!(
        "{{\"vehicles\":{vehicles},\"parts\":{},\"visible_parts\":{visible},\"batches\":{},\"batches_by_lod\":[{},{},{}]}}",
        parts.iter().count(),
        by_lod.iter().sum::<usize>(),
        by_lod[0],
        by_lod[1],
        by_lod[2]
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rail top every train stands on is the bake reports' rail top on the five track pieces
    /// and the level crossing's deck top, so a train rides level over the deck and the bridge.
    #[test]
    fn trains_stand_on_every_rail_piece_s_rail_top() {
        let reports = [
            (
                include_str!("../../../assets-source/kit/rail_straight_center/bake-report.json"),
                "railTopM",
            ),
            (
                include_str!("../../../assets-source/kit/rail_bend_60_center/bake-report.json"),
                "railTopM",
            ),
            (
                include_str!("../../../assets-source/kit/rail_bend_120_center/bake-report.json"),
                "railTopM",
            ),
            (
                include_str!("../../../assets-source/kit/rail_junction_center/bake-report.json"),
                "railTopM",
            ),
            (
                include_str!(
                    "../../../assets-source/kit/river_rail_bridge_center/bake-report.json"
                ),
                "railTopM",
            ),
            (
                include_str!(
                    "../../../assets-source/kit/rail_road_crossing_center/bake-report.json"
                ),
                "deckTopM",
            ),
        ];
        for (text, key) in reports {
            let v: serde_json::Value = serde_json::from_str(text).unwrap();
            let mut found = vec![];
            fn walk(v: &serde_json::Value, key: &str, out: &mut Vec<f64>) {
                match v {
                    serde_json::Value::Object(m) => {
                        for (k, x) in m {
                            if k == key {
                                out.extend(x.as_f64());
                            }
                            walk(x, key, out);
                        }
                    }
                    serde_json::Value::Array(a) => a.iter().for_each(|x| walk(x, key, out)),
                    _ => {}
                }
            }
            walk(&v, key, &mut found);
            assert!(!found.is_empty(), "{key} missing");
            for y in found {
                assert!((y as f32 - RAIL_TOP_Y).abs() < 1e-3, "{key} {y}");
            }
        }
    }

    #[test]
    fn a_train_wears_one_livery_and_trains_differ() {
        assert_eq!(livery_for(0), livery_for(6));
        let first: HashSet<_> = (0..6u64).map(livery_for).collect();
        assert_eq!(first.len(), LIVERIES.len());
        let t = stock_transform(&StockPose {
            id: 0,
            train: 0,
            kind: StockKind::Engine,
            pos: [1.0, 2.0],
            yaw: std::f32::consts::FRAC_PI_2,
        });
        assert_eq!(t.translation, Vec3::new(1.0, RAIL_TOP_Y, 2.0));
        assert!((t.rotation * Vec3::X - Vec3::Z).length() < 1e-5);
    }

    /// Both carriage designs share one envelope, so `hex_sim::rail::CONSIST` spaces them alike: the
    /// same length over buffers, width, axles and wheels in their bake reports.
    #[test]
    fn the_carriage_designs_share_one_envelope() {
        let geometry = |text: &str| -> serde_json::Value {
            serde_json::from_str::<serde_json::Value>(text).unwrap()["geometry"].clone()
        };
        let a = geometry(include_str!(
            "../../../assets-source/kit/train_carriage/bake-report.json"
        ));
        let b = geometry(include_str!(
            "../../../assets-source/kit/train_carriage_b/bake-report.json"
        ));
        for key in ["lengthM", "widthM", "axlesX", "wheelRadiusM"] {
            assert!(!a[key].is_null(), "{key}");
            assert_eq!(a[key], b[key], "{key}");
        }
        assert_eq!(a["extent"]["x"], b["extent"]["x"], "buffer faces");
    }

    #[test]
    fn the_train_pieces_are_in_the_kit() {
        let manifest = parse_manifest(MANIFEST).unwrap();
        for k in StockKind::ALL {
            assert!(manifest.pieces.iter().any(|p| p.id == k.piece()), "{k:?}");
        }
    }
}
