//! Cars in `scenario=play` (PRD §7.2). `hex-app` runs the simulation and writes the interpolated
//! poses into [`CarsView`]; this module keeps one entity per car id in step with it.
//!
//! Each car is one of three kit pieces (`car_sedan`, `car_van`, `car_pickup`), chosen by the sim
//! from the car's seed ([`CarKind::from_seed`]), and painted at runtime (PRD §6): one model and one
//! material per type, the paint colour picked from [`PALETTE`] by the car's seed and carried as a
//! per-entity [`MeshTag`], which keeps every car of a type and LOD in one instanced batch
//! (`car_material.rs`). Until the kit pieces load (or if they fail) every car draws as the
//! placeholder mesh.
use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
};

use bevy::{
    asset::UntypedAssetId,
    core_pipeline::core_3d::Opaque3d,
    gltf::{Gltf, GltfMesh},
    mesh::MeshTag,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems, camera::ExtractedCamera,
        render_phase::ViewBinnedRenderPhases, renderer::RenderDevice, settings::WgpuFeatures,
        view::ExtractedView,
    },
};
use hex_assets::{KitManifest, TextureFamily, parse_manifest, select_family};

use crate::{
    ActiveScenario, Scenario, SceneEntity,
    car_material::{CarMaterial, build_material},
    kit_material::slot_requests,
    kit_scene::MANIFEST,
    play::packs::{PackState, PlayPacks},
    scene::lod_range,
    workload::GateConfig,
};

/// Placeholder car size in metres: length along +x (the driving direction), height, width.
pub const CAR_SIZE: Vec3 = Vec3::new(4.2, 1.5, 1.8);

/// Road crown height: `BED_TOP` 0.08 + `CROWN` 0.03 in `scripts/blender/kit/_road.py`.
pub const ROAD_SURFACE_Y: f32 = 0.11;

/// The three car body types (owner, 2026-10-13), one kit piece each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CarKind {
    #[default]
    Sedan,
    Van,
    Pickup,
}

impl CarKind {
    pub const ALL: [CarKind; 3] = [CarKind::Sedan, CarKind::Van, CarKind::Pickup];

    pub fn piece(self) -> &'static str {
        match self {
            CarKind::Sedan => "car_sedan",
            CarKind::Van => "car_van",
            CarKind::Pickup => "car_pickup",
        }
    }

    /// The body type for a car seed: half sedans, a quarter each vans and pickups.
    pub fn from_seed(seed: u64) -> Self {
        match mix(seed ^ 0x6b69_6e64) % 4 {
            0 | 1 => CarKind::Sedan,
            2 => CarKind::Van,
            _ => CarKind::Pickup,
        }
    }
}

/// Curated paint colours (sRGB): stylized-real car paints, muted enough for the diorama.
pub const PALETTE: [[u8; 3]; 12] = [
    [0xe9, 0xe8, 0xe3], // white
    [0xa9, 0xad, 0xb0], // silver
    [0x4a, 0x4e, 0x54], // graphite
    [0x1b, 0x1c, 0x1f], // black
    [0xa3, 0x20, 0x1c], // red
    [0x23, 0x46, 0x7a], // blue
    [0x3f, 0x7f, 0x8c], // teal
    [0x2e, 0x51, 0x37], // green
    [0xd9, 0xa7, 0x2a], // yellow
    [0xc8, 0x62, 0x2a], // orange
    [0xcd, 0xbf, 0x9c], // beige
    [0x6b, 0x24, 0x30], // maroon
];

/// splitmix64: a well-mixed hash of a seed.
fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// The paint colour for a car seed.
pub fn paint_for(seed: u64) -> [u8; 3] {
    PALETTE[(mix(seed ^ 0x0070_6169_6e74) % PALETTE.len() as u64) as usize]
}

/// The [`MeshTag`] value for a paint colour: sRGB8 packed 0xBBGGRR (`unpack4x8unorm` in the shader).
pub fn paint_tag([r, g, b]: [u8; 3]) -> u32 {
    u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
}

/// One car's pose between the last two sim ticks, in world XZ metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CarPose {
    /// The sim's car id; ids are never reused. Also the car's seed for its paint colour.
    pub id: u64,
    /// The body type the sim assigned this car.
    pub kind: CarKind,
    pub pos: [f32; 2],
    /// Heading in radians from +x toward +z (`hex_sim::Pose::yaw`).
    pub yaw: f32,
}

/// The cars to draw, written by `hex-app` every frame while any car exists.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct CarsView {
    pub cars: Vec<CarPose>,
}

/// A drawn car, tagged with its sim id.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayCar(pub u64);

/// A car drawn with the placeholder mesh; respawned once the kit pieces are ready.
#[derive(Component)]
pub struct CarPlaceholder;

/// The placeholder mesh and material, drawn until the car kit pieces are ready.
#[derive(Resource, Clone)]
pub struct CarAssets {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
}

impl FromWorld for CarAssets {
    fn from_world(world: &mut World) -> Self {
        let mesh = world.resource_mut::<Assets<Mesh>>().add(car_mesh());
        // Vertex colours carry the body, roof and tyre colours.
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 0.55,
                ..default()
            });
        Self { mesh, material }
    }
}

/// One drawable part of a car kit piece: a primitive of one LOD with its slot's material.
#[derive(Clone)]
pub struct CarPart {
    pub lod: usize,
    pub mesh: Handle<Mesh>,
    pub material: Handle<CarMaterial>,
}

/// The car kit pieces, loaded with the village pack in the play scenario.
#[derive(Resource, Default)]
pub enum CarKit {
    #[default]
    Idle,
    Loading {
        manifest: KitManifest,
        items: Vec<(CarKind, Handle<Gltf>)>,
        family: TextureFamily,
    },
    Ready(HashMap<CarKind, Vec<CarPart>>),
}

/// Car mesh ids and labels, for the render-world batch census.
static CAR_MESHES: Mutex<Vec<(UntypedAssetId, String)>> = Mutex::new(Vec::new());

pub struct CarsPlugin;

impl Plugin for CarsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<CarMaterial>::default())
            .init_resource::<CarsView>()
            .init_resource::<CarAssets>()
            .init_resource::<CarKit>()
            .add_systems(Update, (start_kit, finish_kit, census).chain())
            .add_systems(
                PostUpdate,
                sync.before(bevy::transform::TransformSystems::Propagate),
            );
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(
                Render,
                census_bins
                    .after(RenderSystems::PrepareBindGroups)
                    .before(RenderSystems::Render),
            );
        }
    }
}

/// The pack the car pieces stream in: cars only drive between houses, so they load with them.
pub const CAR_PACK: &str = "village";

/// Starts loading the three car pieces once the play scenario has requested the village pack, so
/// they never count toward the initial download.
pub fn start_kit(
    mut kit: ResMut<CarKit>,
    active: Res<ActiveScenario>,
    device: Option<Res<RenderDevice>>,
    server: Res<AssetServer>,
    packs: Option<Res<PlayPacks>>,
) {
    let (Scenario::Play, CarKit::Idle, Some(device)) = (active.0, &*kit, device) else {
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
            warn!("cars: {e}, drawing placeholders");
            *kit = CarKit::Ready(HashMap::new());
            return;
        }
    };
    let items = CarKind::ALL
        .iter()
        .filter_map(|k| {
            let e = manifest.pieces.iter().find(|e| e.id == k.piece())?;
            Some((*k, server.load(format!("kit://{}", e.glb))))
        })
        .collect();
    *kit = CarKit::Loading {
        manifest,
        items,
        family,
    };
}

/// Builds each loaded car piece's parts: every primitive of `<id>_lod{0,1,2}` with its slot's
/// [`CarMaterial`] (one material per slot, shared by every car of the type).
pub fn finish_kit(
    mut kit: ResMut<CarKit>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    mut materials: ResMut<Assets<CarMaterial>>,
    config: Res<GateConfig>,
) {
    let CarKit::Loading {
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
    let mut labels = vec![];
    for (kind, h) in items {
        let id = kind.piece();
        let (Some(g), Some(entry)) = (
            gltfs.get(h).filter(|_| !failed(h)),
            manifest.pieces.iter().find(|e| e.id == id),
        ) else {
            warn!("cars: {id} failed to load, drawn as the placeholder");
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
                warn!("cars: {id} has no {id}_lod{lod}");
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
                    warn!("cars: {id} material {slot_name} has no manifest slot");
                    continue;
                };
                let mat = by_slot
                    .entry(slot_name.clone())
                    .or_insert_with(|| match slot_requests(manifest, entry, slot, *family) {
                        Ok(r) => {
                            Some(materials.add(build_material(&server, slot, &r, config.kit_hooks)))
                        }
                        Err(e) => {
                            warn!("cars: {e}, slot {slot_name} skipped");
                            None
                        }
                    })
                    .clone();
                if let Some(material) = mat {
                    labels.push((prim.mesh.id().untyped(), format!("{slot_name}@lod{lod}")));
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
    let mut kinds: Vec<_> = ready.keys().map(|k| k.piece()).collect();
    kinds.sort();
    info!("cars: kit car pieces ready: {kinds:?}");
    *CAR_MESHES.lock().unwrap() = labels;
    *kit = CarKit::Ready(ready);
}

fn part(size: Vec3, center: Vec3, color: Color) -> Mesh {
    let c = color.to_linear();
    let mesh =
        Mesh::from(Cuboid::from_size(size)).transformed_by(Transform::from_translation(center));
    let n = mesh.count_vertices();
    mesh.with_inserted_attribute(
        Mesh::ATTRIBUTE_COLOR,
        vec![[c.red, c.green, c.blue, 1.0]; n],
    )
}

/// A low-poly placeholder car facing +x with its wheels on y = 0: a neutral body, a darker cabin
/// and roof, and four dark wheels, [`CAR_SIZE`] overall.
pub fn car_mesh() -> Mesh {
    let [len, height, width] = CAR_SIZE.to_array();
    let body = Color::srgb(0.80, 0.78, 0.74);
    let roof = Color::srgb(0.20, 0.22, 0.26);
    let tyre = Color::srgb(0.06, 0.06, 0.07);
    let (wheel, sill, belt) = (0.62, 0.28, 0.90);
    let mut mesh = part(
        Vec3::new(len, belt - sill, width),
        Vec3::new(0.0, (sill + belt) / 2.0, 0.0),
        body,
    );
    let cabin = part(
        Vec3::new(len * 0.55, height - belt, width * 0.9),
        Vec3::new(-0.25, (belt + height) / 2.0, 0.0),
        roof,
    );
    mesh.merge(&cabin).expect("same attributes");
    for x in [-1.35, 1.35] {
        for z in [-1.0, 1.0] {
            let w = part(
                Vec3::new(wheel, wheel, 0.28),
                Vec3::new(x, wheel / 2.0, z * (width / 2.0 - 0.14)),
                tyre,
            );
            mesh.merge(&w).expect("same attributes");
        }
    }
    mesh
}

/// A car standing on the road crown at `p`, its +x along the heading.
pub fn car_transform(p: &CarPose) -> Transform {
    // Bevy turns +x toward -z for a positive angle about +Y, so the yaw (+x toward +z) is negated.
    Transform::from_xyz(p.pos[0], ROAD_SURFACE_Y, p.pos[1])
        .with_rotation(Quat::from_rotation_y(-p.yaw))
}

/// Spawns, moves and despawns car entities to match [`CarsView`] by car id. Outside the play
/// scenario every car entity goes. Cars are `SceneEntity`s, so a scenario switch also removes
/// them, and this respawns whatever the view still holds. A kit car is a parent with the pose and
/// one child per part (slot x LOD), every child tagged with the car's paint colour.
pub fn sync(
    mut commands: Commands,
    active: Res<ActiveScenario>,
    view: Res<CarsView>,
    assets: Res<CarAssets>,
    kit: Res<CarKit>,
    mut cars: Query<(Entity, &PlayCar, &mut Transform, Has<CarPlaceholder>)>,
) {
    let play = active.0 == Scenario::Play;
    let want: HashMap<u64, &CarPose> = if play {
        view.cars.iter().map(|c| (c.id, c)).collect()
    } else {
        HashMap::new()
    };
    if want.is_empty() && cars.is_empty() {
        return;
    }
    let kit = match &*kit {
        CarKit::Ready(k) => Some(k),
        _ => None,
    };
    let mut have = HashSet::with_capacity(want.len());
    for (e, car, mut t, placeholder) in &mut cars {
        let upgrade = placeholder
            && want
                .get(&car.0)
                .is_some_and(|p| kit.is_some_and(|k| k.contains_key(&p.kind)));
        match want.get(&car.0) {
            Some(p) if !upgrade && have.insert(car.0) => *t = car_transform(p),
            _ => commands.entity(e).try_despawn(),
        }
    }
    for (id, p) in want {
        if have.contains(&id) {
            continue;
        }
        match kit.and_then(|k| k.get(&p.kind)) {
            Some(parts) => {
                let tag = MeshTag::new(paint_tag(paint_for(id)));
                commands
                    .spawn((
                        PlayCar(id),
                        car_transform(p),
                        Visibility::default(),
                        SceneEntity,
                    ))
                    .with_children(|c| {
                        for part in parts {
                            c.spawn((
                                Mesh3d(part.mesh.clone()),
                                MeshMaterial3d(part.material.clone()),
                                tag.clone(),
                                lod_range(part.lod),
                            ));
                        }
                    });
            }
            None => {
                commands.spawn((
                    PlayCar(id),
                    CarPlaceholder,
                    Mesh3d(assets.mesh.clone()),
                    MeshMaterial3d(assets.material.clone()),
                    car_transform(p),
                    SceneEntity,
                ));
            }
        }
    }
}

/// Every 120 frames while cars exist, logs what the main world will draw for them: cars, types,
/// distinct paint colours and distinct (mesh, material) pairs among the visible car parts, the most
/// instanced draws Bevy can need for them. `census_bins` logs the render world's side.
fn census(
    mut frame: Local<u32>,
    cars: Query<&PlayCar, Without<CarPlaceholder>>,
    parts: Query<(
        &ViewVisibility,
        &Mesh3d,
        &MeshMaterial3d<CarMaterial>,
        &MeshTag,
    )>,
    view: Res<CarsView>,
) {
    *frame += 1;
    if !(*frame).is_multiple_of(120) || cars.is_empty() {
        return;
    }
    let (mut pairs, mut tags, mut visible) = (HashSet::new(), HashSet::new(), 0);
    for (vis, mesh, mat, tag) in &parts {
        if vis.get() {
            visible += 1;
            pairs.insert((mesh.id(), mat.id()));
            tags.insert(tag.value);
        }
    }
    let mut kinds: Vec<_> = view.cars.iter().map(|c| c.kind.piece()).collect();
    kinds.sort();
    kinds.dedup();
    info!(
        "cars census: {} kit cars, types {kinds:?}, {} distinct paint colours, {visible} visible parts, {} distinct (mesh, material) pairs",
        cars.iter().count(),
        tags.len(),
        pairs.len()
    );
    CENSUS_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);
}

static CENSUS_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Render-world side of the census: in the camera's opaque phase, the bins holding car meshes and
/// how many car entities each bin draws in its one instanced draw.
fn census_bins(
    cameras: Query<&ExtractedView, With<ExtractedCamera>>,
    opaque: Res<ViewBinnedRenderPhases<Opaque3d>>,
) {
    if !CENSUS_REQUESTED.swap(false, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let labels: HashMap<UntypedAssetId, String> =
        CAR_MESHES.lock().unwrap().iter().cloned().collect();
    for v in &cameras {
        let Some(p) = opaque.get(&v.retained_view_entity) else {
            continue;
        };
        let mut bins: Vec<(String, usize)> = p
            .batchable_meshes
            .iter()
            .filter_map(|((_, key), bin)| {
                Some((labels.get(&key.asset_id)?.clone(), bin.entities().len()))
            })
            .collect();
        bins.sort();
        let instances: usize = bins.iter().map(|b| b.1).sum();
        let unbatched = p
            .unbatchable_meshes
            .iter()
            .filter(|((_, key), _)| labels.contains_key(&key.asset_id))
            .map(|(_, b)| b.entities.len())
            .sum::<usize>();
        info!(
            "cars census (render): opaque phase: {} car bins drawing {instances} car instances, {unbatched} unbatched; multidraw batch sets (all meshes): {}; bins {bins:?}",
            bins.len(),
            p.multidrawable_meshes.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn car_mesh_has_the_placeholder_size_and_sits_on_the_ground() {
        let m = car_mesh();
        let Some(VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions missing");
        };
        let (lo, hi) = p.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), v| {
            (lo.min(Vec3::from(*v)), hi.max(Vec3::from(*v)))
        });
        assert!((hi - lo - CAR_SIZE).abs().max_element() < 1e-4, "{lo} {hi}");
        assert!(lo.y.abs() < 1e-6 && (lo.x + hi.x).abs() < 1e-6 && (lo.z + hi.z).abs() < 1e-6);
        assert!(m.attribute(Mesh::ATTRIBUTE_COLOR).is_some());
    }

    #[test]
    fn transform_faces_the_heading() {
        for (yaw, dir) in [
            (0.0, Vec3::X),
            (std::f32::consts::FRAC_PI_2, Vec3::Z),
            (std::f32::consts::PI, -Vec3::X),
        ] {
            let t = car_transform(&CarPose {
                id: 0,
                kind: CarKind::Sedan,
                pos: [3.0, -2.0],
                yaw,
            });
            assert!((t.rotation * Vec3::X - dir).length() < 1e-5, "yaw {yaw}");
            assert_eq!(t.translation, Vec3::new(3.0, ROAD_SURFACE_Y, -2.0));
        }
    }

    /// Seeds spread over every type and most of the palette, deterministically.
    #[test]
    fn seeds_pick_every_type_and_many_colours() {
        let mut kinds = HashMap::new();
        let mut colours = HashSet::new();
        for id in 0..200u64 {
            *kinds.entry(CarKind::from_seed(id)).or_insert(0) += 1;
            colours.insert(paint_for(id));
            assert_eq!(CarKind::from_seed(id), CarKind::from_seed(id));
            assert_eq!(paint_for(id), paint_for(id));
        }
        assert_eq!(kinds.len(), 3, "{kinds:?}");
        assert!(kinds[&CarKind::Sedan] > kinds[&CarKind::Van], "{kinds:?}");
        assert_eq!(colours.len(), PALETTE.len());
        // The first ten cars of a run already show at least six colours.
        let first: HashSet<_> = (0..10u64).map(paint_for).collect();
        assert!(first.len() >= 6, "{first:?}");
    }

    #[test]
    fn paint_tag_packs_srgb_bytes_for_unpack4x8unorm() {
        assert_eq!(paint_tag([0x11, 0x22, 0x33]), 0x0033_2211);
        let manifest = parse_manifest(MANIFEST).unwrap();
        for k in CarKind::ALL {
            assert!(
                manifest.pieces.iter().any(|p| p.id == k.piece()),
                "{} missing from the kit manifest",
                k.piece()
            );
        }
    }
}
