//! `scenario=play`: draws the game world that `hex-app` owns (PRD §4.1). This module holds no game
//! rules: `hex-app` writes [`PlayView`] and this module renders it.
//!
//! Placed tiles use kit pieces from `assets/kit/manifest.json` where the tile references them and
//! plain coloured placeholders otherwise (a biome prism, lane strips, blocks). A newly placed tile
//! plays its staged rise ([`rise`], PRD §4.4) and a removed one sinks. The slot ring, the ghost, its
//! edge glow and socket key marks, and the bulldoze outlines are the Legend in-world affordances
//! ([`affordances`], `design/ui/legend/in-world.md`).
use std::collections::{HashMap, HashSet};

use bevy::{
    asset::RenderAssetUsages,
    gltf::{Gltf, GltfMesh},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::{renderer::RenderDevice, settings::WgpuFeatures},
};
use hex_assets::{parse_manifest, select_family};
use hex_core::{
    Axial, Biome, EdgeStatus, HEX_RADIUS, LEVEL_STEP, SocketKind, TileCatalog, TileDef, hex_prism,
};

pub mod affordances;
pub mod packs;
pub mod rise;

use packs::{PackRequests, PlayPacks};

use rise::{PlayRises, PropKind, ReducedMotion, RiseMessage, RisePart, RiseRole};

use crate::{
    ActiveScenario, Scenario, SceneEntity,
    kit_material::{KitMaterial, build_material, slot_requests},
    kit_scene::{MANIFEST, piece_rotation},
    scene::lod_range,
    water,
    workload::GateConfig,
};

const APOTHEM: f32 = HEX_RADIUS * 0.866_025_4;

/// The play systems (kit loading, the view sync, the rise and the affordances), in order. `hex-app`
/// orders its view writer before this set and the car mirror after it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlaySystems;

#[derive(Clone, Debug, PartialEq)]
pub struct PlayTile {
    pub cell: Axial,
    pub tile: String,
    pub rotation: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayGhost {
    pub cell: Axial,
    pub tile: String,
    pub rotation: u8,
    /// Status per world edge 0..6.
    pub edges: [EdgeStatus; 6],
    /// False when the tile does not fit here at this rotation.
    pub valid: bool,
}

/// What the play scenario draws, written by `hex-app` when the game changes.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct PlayView {
    pub tiles: Vec<PlayTile>,
    pub frontier: Vec<Axial>,
    pub ghost: Option<PlayGhost>,
    /// Bulldoze tool: the hovered tile and whether removing it is allowed.
    pub bulldoze: Option<(Axial, bool)>,
    /// The bulldoze tool is active (the slot ring hides).
    pub bulldoze_mode: bool,
    /// Refused placements and bulldozes so far; each new one pulses the refused highlight.
    pub refusals: u32,
}

/// Tile definitions the view's ids resolve against (the embedded catalog `hex-app` also uses).
#[derive(Resource)]
pub struct PlayCatalog(pub TileCatalog);

impl Default for PlayCatalog {
    fn default() -> Self {
        Self(TileCatalog::builtin().expect("the embedded catalog is validated by hex-core"))
    }
}

/// Root of one placed tile's entities.
#[derive(Component)]
pub struct PlayTileRoot;

/// Entities spawned for the current view. `setup` replaces it on every switch into the scenario,
/// since the switch despawned the previous scene.
#[derive(Resource, Default)]
pub struct PlaySpawned {
    tiles: HashMap<Axial, (PlayTile, Entity)>,
    rebuild: bool,
    /// Pieces built since the last sync: tiles drawing them swap to the kit look in place.
    refresh: HashSet<String>,
}

#[derive(Clone)]
pub struct KitPart {
    lod: usize,
    mesh: Handle<Mesh>,
    material: Handle<KitMaterial>,
}

/// The built kit pieces. `Loading` holds pieces of packs that arrived before core; `Ready` is
/// core and every pack since ([`packs`]). A piece missing from the manifest (pending, plan F4),
/// failing to load, or in a pack still streaming has no entry and draws as a placeholder.
#[derive(Resource)]
pub enum PlayKit {
    Loading(HashMap<String, Vec<KitPart>>),
    Ready(HashMap<String, Vec<KitPart>>),
}

#[derive(Resource)]
pub struct PlainAssets {
    prism: Handle<Mesh>,
    strip: [Handle<Mesh>; 2],
    hub: Handle<Mesh>,
    pad: Handle<Mesh>,
    block: Handle<Mesh>,
    biome: [Handle<StandardMaterial>; 4],
    socket: [Handle<StandardMaterial>; 6],
    block_mat: Handle<StandardMaterial>,
}

fn biome_index(b: Biome) -> usize {
    match b {
        Biome::Grass => 0,
        Biome::Forest => 1,
        Biome::Beach => 2,
        Biome::Ocean => 3,
    }
}

fn socket_index(k: SocketKind) -> usize {
    SocketKind::ALL.iter().position(|&s| s == k).unwrap_or(0)
}

fn prism_mesh(radius: f32, height: f32) -> Mesh {
    let p = hex_prism(radius, height);
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, p.positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, p.normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, p.uvs)
    .with_inserted_indices(Indices::U32(p.indices))
}

fn lit(c: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: c,
        perceptual_roughness: 0.9,
        ..default()
    }
}

fn flat(c: Color) -> StandardMaterial {
    let alpha_mode = if c.alpha() < 1.0 {
        AlphaMode::Blend
    } else {
        AlphaMode::Opaque
    };
    StandardMaterial {
        base_color: c,
        unlit: true,
        alpha_mode,
        ..default()
    }
}

impl PlainAssets {
    fn new(meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>) -> Self {
        let mut m = |c: StandardMaterial| mats.add(c);
        Self {
            prism: meshes.add(prism_mesh(HEX_RADIUS * 0.985, LEVEL_STEP)),
            strip: [6.0, 8.0].map(|w| meshes.add(Cuboid::new(APOTHEM, 0.9, w))),
            hub: meshes.add(Cylinder::new(4.0, 0.9)),
            pad: meshes.add(Cuboid::new(11.0, 0.5, 11.0)),
            block: meshes.add(Cuboid::new(6.0, 5.0, 6.0)),
            biome: [
                Color::srgb(0.33, 0.52, 0.22),
                Color::srgb(0.17, 0.36, 0.15),
                Color::srgb(0.86, 0.79, 0.58),
                Color::srgb(0.10, 0.33, 0.58),
            ]
            .map(|c| m(lit(c))),
            socket: [
                Color::srgb(0.45, 0.80, 0.32),
                Color::srgb(0.08, 0.38, 0.14),
                Color::srgb(0.16, 0.16, 0.18),
                Color::srgb(0.20, 0.55, 1.00),
                Color::srgb(0.96, 0.86, 0.56),
                Color::srgb(0.04, 0.18, 0.55),
            ]
            .map(|c| m(flat(c))),
            block_mat: m(lit(Color::srgb(0.80, 0.72, 0.60))),
        }
    }
}

/// `setup`'s play arm: plain assets, a fresh spawn record, and the start camera.
pub fn setup(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Transform {
    commands.insert_resource(PlainAssets::new(meshes, materials));
    commands.insert_resource(PlaySpawned {
        rebuild: true,
        ..default()
    });
    commands.insert_resource(water::tiles::PlayWater::default());
    commands.insert_resource(affordances::Affordances::default());
    commands.insert_resource(rise::PlayDust::default());
    Transform::from_xyz(14.0, 95.0, 125.0).looking_at(Vec3::new(14.0, 0.0, 16.0), Vec3::Y)
}

/// Requests the core pack the first time the play scenario runs, then each frame the packs the
/// view and the selected tile need and, after the first interaction, the rest ([`packs`]).
#[allow(clippy::too_many_arguments)]
pub fn start_kit(
    mut commands: Commands,
    active: Res<ActiveScenario>,
    kit: Option<Res<PlayKit>>,
    packs: Option<ResMut<PlayPacks>>,
    requests: Option<Res<PackRequests>>,
    view: Res<PlayView>,
    catalog: Res<PlayCatalog>,
    (device, server): (Option<Res<RenderDevice>>, Res<AssetServer>),
) {
    if active.0 != Scenario::Play {
        return;
    }
    if let (Some(_), Some(mut packs)) = (&kit, packs) {
        let changed = view.is_changed();
        packs.update(&view, changed, &catalog.0, requests.as_deref(), &server);
        if changed {
            packs.refresh_waiting(&view, &catalog.0);
        }
        return;
    }
    let (None, Some(device)) = (kit, device) else {
        return;
    };
    let f = device.features();
    let family = select_family(
        f.contains(WgpuFeatures::TEXTURE_COMPRESSION_BC),
        f.contains(WgpuFeatures::TEXTURE_COMPRESSION_ASTC),
    );
    let manifest = match parse_manifest(MANIFEST) {
        Ok(m) => m,
        Err(e) => {
            warn!("play: {e}, drawing placeholders only");
            commands.insert_resource(PlayKit::Ready(HashMap::new()));
            return;
        }
    };
    let mut packs = PlayPacks::new(manifest, family);
    packs.request(hex_assets::CORE_PACK, &server);
    commands.insert_resource(packs);
    commands.insert_resource(PlayKit::Loading(HashMap::new()));
}

/// Builds the meshes and materials of every piece in the packs that finished loading (the slot
/// walk of `kit_scene::finish_kit`). Core's arrival draws the world; a later pack's pieces swap in
/// place on the tiles that draw them (`sync`).
#[allow(clippy::too_many_arguments)]
pub fn finish_kit(
    mut kit: Option<ResMut<PlayKit>>,
    mut packs: Option<ResMut<PlayPacks>>,
    spawned: Option<ResMut<PlaySpawned>>,
    view: Res<PlayView>,
    catalog: Res<PlayCatalog>,
    server: Res<AssetServer>,
    (gltfs, gltf_meshes, mut materials): (
        Res<Assets<Gltf>>,
        Res<Assets<GltfMesh>>,
        ResMut<Assets<KitMaterial>>,
    ),
    config: Res<GateConfig>,
) {
    let (Some(kit), Some(packs)) = (kit.as_deref_mut(), packs.as_deref_mut()) else {
        return;
    };
    let arrived = packs.poll(&server);
    if arrived.is_empty() {
        return;
    }
    packs.refresh_waiting(&view, &catalog.0);
    let (PlayKit::Loading(pieces) | PlayKit::Ready(pieces)) = &mut *kit;
    let mut built = HashSet::new();
    for pack in arrived {
        let ids: Vec<String> = packs
            .manifest
            .pack_pieces(pack)
            .map(|e| e.id.clone())
            .collect();
        for id in ids {
            let parts = build_piece(
                &id,
                packs,
                &server,
                (&gltfs, &gltf_meshes, &mut materials),
                config.kit_hooks,
            );
            if parts.is_empty() {
                warn!("play: kit piece {id} failed to load, drawn as a placeholder");
                continue;
            }
            pieces.insert(id.clone(), parts);
            built.insert(id);
        }
    }
    info!("play: kit pieces ready: {:?}", {
        let mut ids: Vec<_> = pieces.keys().collect();
        ids.sort();
        ids
    });
    let mut spawned = spawned;
    if packs.core_ready()
        && let PlayKit::Loading(pieces) = &mut *kit
    {
        let pieces = std::mem::take(pieces);
        *kit = PlayKit::Ready(pieces);
        if let Some(s) = spawned.as_deref_mut() {
            s.rebuild = true;
        }
    } else if matches!(kit, PlayKit::Ready(_))
        && let Some(s) = spawned.as_deref_mut()
    {
        s.refresh.extend(built);
    }
}

/// One loaded piece's parts by LOD, with a material per manifest slot.
fn build_piece(
    id: &str,
    packs: &PlayPacks,
    server: &AssetServer,
    (gltfs, gltf_meshes, materials): (&Assets<Gltf>, &Assets<GltfMesh>, &mut Assets<KitMaterial>),
    hooks: crate::workload::KitHooks,
) -> Vec<KitPart> {
    let (manifest, family) = (&packs.manifest, packs.family);
    let failed = |h: &Handle<Gltf>| {
        server.load_state(h).is_failed() || server.recursive_dependency_load_state(h).is_failed()
    };
    let (Some(g), Some(entry)) = (
        packs
            .glb(id)
            .filter(|h| !failed(h))
            .and_then(|h| gltfs.get(h)),
        manifest.pieces.iter().find(|e| e.id == id),
    ) else {
        return vec![];
    };
    let mut parts = vec![];
    let mut by_slot: HashMap<String, Option<Handle<KitMaterial>>> = HashMap::new();
    for lod in 0..3usize {
        let Some(gm) = g
            .named_meshes
            .get(format!("{id}_lod{lod}").as_str())
            .and_then(|mh| gltf_meshes.get(mh))
        else {
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
                continue;
            };
            let mat = by_slot
                .entry(slot_name.clone())
                .or_insert_with(|| match slot_requests(manifest, entry, slot, family) {
                    Ok(r) => Some(materials.add(build_material(server, slot, &r, hooks))),
                    Err(e) => {
                        warn!("play: {e}, slot {slot_name} skipped");
                        None
                    }
                })
                .clone();
            if let Some(material) = mat {
                parts.push(KitPart {
                    lod,
                    mesh: prim.mesh.clone(),
                    material,
                });
            }
        }
    }
    parts
}

fn spawn_kit(
    p: &mut ChildSpawnerCommands,
    kit: &HashMap<String, Vec<KitPart>>,
    piece: &str,
    t: Transform,
    rise: Option<(Axial, RiseRole)>,
) -> bool {
    let Some(parts) = kit.get(piece) else {
        return false;
    };
    for part in parts {
        let mut e = p.spawn((
            Mesh3d(part.mesh.clone()),
            MeshMaterial3d(part.material.clone()),
            t,
            lod_range(part.lod),
            SceneEntity,
        ));
        if let Some((cell, role)) = rise {
            e.insert(RisePart::new(cell, role, t));
        }
    }
    true
}

/// Child transform for something authored on edge 0, moved to local edge `e` at `along` metres
/// from the centre and height `y`.
fn on_edge(e: u8, along: f32, y: f32) -> Transform {
    let rot = piece_rotation(e);
    Transform::from_translation(rot * Vec3::new(along, y, 0.0)).with_rotation(rot)
}

fn spawn_tile(
    commands: &mut Commands,
    plain: &PlainAssets,
    kit: &HashMap<String, Vec<KitPart>>,
    catalog: &TileCatalog,
    t: &PlayTile,
) -> Entity {
    let [x, z] = t.cell.to_world(HEX_RADIUS);
    let def = catalog.get(&t.tile);
    let rest = Transform::from_xyz(x, 0.0, z).with_rotation(piece_rotation(t.rotation));
    commands
        .spawn((
            PlayTileRoot,
            rest,
            RisePart::new(t.cell, RiseRole::Root, rest),
            Visibility::default(),
            SceneEntity,
        ))
        .with_children(|p| match def {
            Some(def) => tile_children(p, plain, kit, catalog, def, t.cell),
            None => {
                p.spawn((
                    Mesh3d(plain.prism.clone()),
                    MeshMaterial3d(plain.block_mat.clone()),
                    Transform::from_xyz(0.0, -LEVEL_STEP, 0.0),
                    SceneEntity,
                ));
            }
        })
        .id()
}

/// A hero prop's growth: trees scale up, buildings rise a floor at a time (roof included).
fn prop_kind(piece: &str) -> PropKind {
    if piece.contains("tree") {
        PropKind::Tree
    } else if piece.contains("twostorey") {
        PropKind::Building { floors: 3 }
    } else {
        PropKind::Building { floors: 2 }
    }
}

fn tile_children(
    p: &mut ChildSpawnerCommands,
    plain: &PlainAssets,
    kit: &HashMap<String, Vec<KitPart>>,
    catalog: &TileCatalog,
    def: &TileDef,
    cell: Axial,
) {
    let plain_mesh = |p: &mut ChildSpawnerCommands,
                      mesh: &Handle<Mesh>,
                      mat: &Handle<StandardMaterial>,
                      t: Transform,
                      role: Option<RiseRole>| {
        let mut e = p.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(mat.clone()),
            t,
            SceneEntity,
        ));
        if let Some(role) = role {
            e.insert(RisePart::new(cell, role, t));
        }
    };
    let edge = Some((cell, RiseRole::Edge));
    let center_kit = def
        .center
        .as_ref()
        .is_some_and(|c| kit.contains_key(c.as_str()));
    // Water tiles without their centre piece get the placeholder bed and the water surface from
    // `water::tiles` instead of a base and river strips; road strips stay as the bridge deck.
    let water_bed = !center_kit && water::tiles::water_kind(def, 0).is_some();
    if !water_bed && !spawn_kit(p, kit, def.base.as_str(), Transform::IDENTITY, None) {
        plain_mesh(
            p,
            &plain.prism,
            &plain.biome[biome_index(def.biome)],
            Transform::from_xyz(0.0, -LEVEL_STEP, 0.0),
            None,
        );
    }
    let lanes: Vec<(u8, SocketKind)> = (0..6u8)
        .filter_map(|e| {
            let s = &def.edges[e as usize];
            (s.lane.is_some()
                && (s.kind == SocketKind::Road || (s.kind == SocketKind::River && !water_bed)))
                .then_some((e, s.kind))
        })
        .collect();
    if center_kit {
        let center = def.center.as_ref().expect("checked above");
        spawn_kit(p, kit, center.as_str(), Transform::IDENTITY, edge);
        let pieces = def.edge_pieces(catalog.sockets());
        for &(e, kind) in &lanes {
            let piece = pieces[e as usize].map(|k| k.as_str()).unwrap_or_default();
            if !spawn_kit(
                p,
                kit,
                piece,
                Transform::from_rotation(piece_rotation(e)),
                edge,
            ) {
                let i = (kind == SocketKind::River) as usize;
                plain_mesh(
                    p,
                    &plain.strip[i],
                    &plain.socket[socket_index(kind)],
                    on_edge(e, APOTHEM / 2.0, 0.05),
                    Some(RiseRole::Edge),
                );
            }
        }
    } else {
        // A pending centre: plain lane strips meeting at a hub, or a pad for a lane-less centre.
        for &(e, kind) in &lanes {
            let i = (kind == SocketKind::River) as usize;
            plain_mesh(
                p,
                &plain.strip[i],
                &plain.socket[socket_index(kind)],
                on_edge(e, APOTHEM / 2.0, 0.05),
                Some(RiseRole::Edge),
            );
        }
        if water_bed {
            // No hub in the channel: the road strips meet over the water.
        } else if let Some(&(_, kind)) = lanes.first() {
            plain_mesh(
                p,
                &plain.hub,
                &plain.socket[socket_index(kind)],
                Transform::from_xyz(0.0, 0.05, 0.0),
                Some(RiseRole::Edge),
            );
        } else if def.center.is_some() && def.biome == Biome::Grass {
            plain_mesh(
                p,
                &plain.pad,
                &plain.block_mat,
                Transform::from_xyz(0.0, 0.25, 0.0),
                Some(RiseRole::Edge),
            );
        }
    }
    let count = def.props.hero.len().min(u8::MAX as usize) as u8;
    for (i, h) in def.props.hero.iter().enumerate() {
        let t = Transform::from_xyz(h.at[0], 0.0, h.at[1])
            .with_rotation(Quat::from_rotation_y(-h.yaw_deg.to_radians()));
        let role = RiseRole::Prop {
            order: i.min(u8::MAX as usize) as u8,
            count,
            kind: prop_kind(h.piece.as_str()),
        };
        if !spawn_kit(p, kit, h.piece.as_str(), t, Some((cell, role))) {
            plain_mesh(
                p,
                &plain.block,
                &plain.block_mat,
                t * Transform::from_xyz(0.0, 2.5, 0.0),
                Some(role),
            );
        }
    }
}

/// Play's water hooks (`water::tiles`): the water entities, the water clock, and the camera.
type WaterHooks<'w, 's> = (
    Option<ResMut<'w, water::tiles::PlayWater>>,
    Option<Res<'w, water::WaterClock>>,
    Query<'w, 's, Entity, (With<Camera3d>, Without<water::tiles::PlayWaterCamera>)>,
    ResMut<'w, Assets<Mesh>>,
    ResMut<'w, Assets<Image>>,
    ResMut<'w, Assets<water::WaterMaterial>>,
    ResMut<'w, Assets<water::TerrainMaterial>>,
);

/// Play's rise hooks: the rise state, reduced motion and the rise cue writer.
type RiseHooks<'w> = (
    ResMut<'w, PlayRises>,
    Res<'w, ReducedMotion>,
    MessageWriter<'w, RiseMessage>,
);

/// Spawns and despawns tile entities to match the view. Tiles wait for the core pack; a tile that
/// draws a piece from a pack still streaming draws its placeholder, and swaps to the kit look in
/// place when the pack arrives (its children are respawned under the same root, so a running rise
/// keeps its pose and clock). Water tiles also get their surfaces, cut faces and placeholder beds
/// (`water::tiles::sync`, which respawns a cell whose centre piece arrived).
///
/// A tile new to the view (not on a rebuild) fast-forwards every running rise and starts its own;
/// a tile gone from it sinks with its released water entities. A rebuild (core arriving, a switch
/// into the scenario) draws every tile at rest.
#[allow(clippy::too_many_arguments)]
pub fn sync(
    mut commands: Commands,
    active: Res<ActiveScenario>,
    view: Res<PlayView>,
    spawned: Option<ResMut<PlaySpawned>>,
    plain: Option<Res<PlainAssets>>,
    kit: Option<Res<PlayKit>>,
    catalog: Res<PlayCatalog>,
    config: Res<GateConfig>,
    (water, clock, cameras, mut meshes, mut images, mut water_mats, mut terrain_mats): WaterHooks,
    (mut rises, reduced, mut cues): RiseHooks,
) {
    let (Scenario::Play, Some(mut spawned), Some(plain)) = (active.0, spawned, plain) else {
        return;
    };
    // The play camera and clock get what water needs as soon as the scenario is up (`setup` only
    // adds them for the look-dev scenarios), so the first water tile changes no pipeline.
    for cam in &cameras {
        water::tiles::prepare_camera(&mut commands.entity(cam), config.tier);
    }
    if clock.is_none() {
        commands.insert_resource(water::WaterClock {
            t: 0.0,
            fixed: config.water_fixed,
        });
    }
    if !view.is_changed() && !spawned.rebuild && spawned.refresh.is_empty() {
        return;
    }
    let Some(PlayKit::Ready(kit)) = kit.as_deref() else {
        return;
    };
    let rebuild = spawned.rebuild;
    if rebuild {
        for (_, (_, e)) in spawned.tiles.drain() {
            commands.entity(e).try_despawn();
        }
        rises.reset(view.tiles.iter().map(|t| t.cell), &mut commands);
        spawned.rebuild = false;
        spawned.refresh.clear();
    }
    let mut released: HashMap<Axial, Vec<Entity>> = HashMap::new();
    if let Some(mut water) = water {
        released = water::tiles::sync(
            &mut commands,
            &mut water,
            water::tiles::WaterAssets {
                meshes: &mut meshes,
                images: &mut images,
                water_mats: &mut water_mats,
                terrain_mats: &mut terrain_mats,
            },
            config.tier,
            &view.tiles,
            &catalog.0,
            |id| kit.contains_key(id),
            rebuild,
        )
        .into_iter()
        .collect();
    }
    let mut out = vec![];
    let want: HashMap<Axial, &PlayTile> = view.tiles.iter().map(|t| (t.cell, t)).collect();
    let mut gone = vec![];
    spawned.tiles.retain(|cell, (t, e)| match want.get(cell) {
        Some(w) if *w == t => true,
        Some(_) => {
            commands.entity(*e).try_despawn();
            false
        }
        None => {
            gone.push((*cell, t.tile.clone(), *e));
            false
        }
    });
    gone.sort_by_key(|(c, ..)| (c.q, c.r));
    for (cell, tile, root) in gone {
        let mut entities = vec![root];
        entities.extend(released.remove(&cell).unwrap_or_default());
        rises.remove(cell, &tile, entities, reduced.0, &mut commands, &mut out);
    }
    for (_, es) in released {
        for e in es {
            commands.entity(e).try_despawn();
        }
    }
    let refresh = std::mem::take(&mut spawned.refresh);
    for (cell, (t, root)) in spawned.tiles.iter().filter(|_| !refresh.is_empty()) {
        let Some(def) = catalog.0.get(&t.tile) else {
            continue;
        };
        if packs::drawn_pieces(def, &catalog.0)
            .iter()
            .any(|p| refresh.contains(*p))
        {
            commands
                .entity(*root)
                .despawn_children()
                .with_children(|p| tile_children(p, &plain, kit, &catalog.0, def, *cell));
        }
    }
    let added: Vec<&PlayTile> = view
        .tiles
        .iter()
        .filter(|t| !spawned.tiles.contains_key(&t.cell))
        .collect();
    if !rebuild && !added.is_empty() {
        rises.fast_forward(&mut out);
    }
    for t in added {
        rises.cancel_sink(t.cell, &mut commands);
        let e = spawn_tile(&mut commands, &plain, kit, &catalog.0, t);
        spawned.tiles.insert(t.cell, (t.clone(), e));
        match catalog.0.get(&t.tile).filter(|_| !rebuild) {
            Some(def) => rises.begin(t.cell, &t.tile, &def.rise, reduced.0, &mut out),
            None => rises.rest(t.cell),
        }
    }
    cues.write_batch(out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_edge_points_at_the_edge_midpoint() {
        for e in 0..6u8 {
            let t = on_edge(e, APOTHEM, 0.0);
            let a = (60.0 * e as f32).to_radians();
            let want = Vec3::new(a.cos(), 0.0, a.sin()) * APOTHEM;
            assert!((t.translation - want).length() < 1e-3, "edge {e}");
        }
    }

    #[test]
    fn every_mvp_tile_resolves_and_biomes_index() {
        let c = PlayCatalog::default();
        assert_eq!(c.0.tiles().len(), 14);
        let biomes: std::collections::BTreeSet<_> =
            c.0.tiles().iter().map(|t| biome_index(t.biome)).collect();
        assert_eq!(biomes.len(), 4);
    }
}
