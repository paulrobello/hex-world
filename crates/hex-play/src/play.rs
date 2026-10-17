//! `scenario=play`: draws the game world that `hex-app` owns (PRD §4.1). This module holds no game
//! rules: `hex-app` writes [`PlayView`] and this module renders it.
//!
//! Placed tiles use kit pieces from `assets/kit/manifest.json` where the tile references them and
//! plain coloured placeholders otherwise (a biome prism, lane strips, blocks). A newly placed tile
//! plays its staged rise ([`rise`], PRD §4.4) and a removed one sinks. The slot ring, the ghost, its
//! edge glow and socket key marks, and the bulldoze outlines are the Legend in-world affordances
//! ([`affordances`], `design/ui/legend/in-world.md`).
use std::collections::{BTreeSet, HashMap, HashSet};

use bevy::{
    asset::RenderAssetUsages,
    gltf::{Gltf, GltfMesh, GltfNode},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::{renderer::RenderDevice, settings::WgpuFeatures},
};
use hex_assets::{parse_manifest, rotor_mesh_name, select_family};
use hex_core::{
    Axial, Biome, HEX_RADIUS, LEVEL_STEP, PairPieces, SocketKind, TileCatalog, TileDef, hex_prism,
};

pub mod affordances;
mod chunks;
pub mod ghost;
pub mod grass;
pub mod grass_gpu;
pub mod packs;
mod plugins;
pub use plugins::PlayPlugin;
pub mod rise;
pub mod scatter;

use packs::{PackRequests, PlayPacks};

use rise::{PlayRises, PropKind, ReducedMotion, RiseMessage, RisePart, RiseRole};

use crate::{
    ActiveScenario, Scenario, SceneEntity,
    kit_material::{KitMaterial, build_material, slot_requests},
    kit_scene::{KitRotor, MANIFEST, piece_rotation, rotor_phase},
    scene::{Lod, lod_range},
    water,
    workload::GateConfig,
};

const APOTHEM: f32 = HEX_RADIUS * 0.866_025_4;

pub use hex_play_state::{
    PlayCatalog, PlayGhost, PlaySpawned, PlaySystems, PlayTile, PlayTileRoot, PlayView, PlayWaiting,
};
pub type KitPart = hex_play_state::KitPart<KitMaterial>;
pub type PlayKit = hex_play_state::PlayKit<KitMaterial>;

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
    commands.insert_resource(chunks::PlayChunks::default());
    commands.insert_resource(PlaySpawned {
        rebuild: true,
        ..default()
    });
    commands.insert_resource(water::tiles::PlayWater::default());
    commands.insert_resource(affordances::Affordances::default());
    commands.insert_resource(PlayWaiting::default());
    commands.insert_resource(ghost::GhostMeshes::default());
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
    mut commands: Commands,
    mut kit: Option<ResMut<PlayKit>>,
    mut packs: Option<ResMut<PlayPacks>>,
    spawned: Option<ResMut<PlaySpawned>>,
    view: Res<PlayView>,
    catalog: Res<PlayCatalog>,
    server: Res<AssetServer>,
    (gltfs, gltf_meshes, gltf_nodes, mut materials): (
        Res<Assets<Gltf>>,
        Res<Assets<GltfMesh>>,
        Res<Assets<GltfNode>>,
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
                (&gltfs, &gltf_meshes, &gltf_nodes, &mut materials),
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
    commands.insert_resource(chunks::KitFamilies::build(pieces));
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

/// One loaded piece's parts by LOD, with a material per manifest slot, and each LOD's rotor (the
/// node lookup of `kit_scene::finish_kit`) after its body.
fn build_piece(
    id: &str,
    packs: &PlayPacks,
    server: &AssetServer,
    (gltfs, gltf_meshes, gltf_nodes, materials): (
        &Assets<Gltf>,
        &Assets<GltfMesh>,
        &Assets<GltfNode>,
        &mut Assets<KitMaterial>,
    ),
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
        let rotor = g
            .named_nodes
            .get(rotor_mesh_name(id, lod).as_str())
            .and_then(|h| gltf_nodes.get(h))
            .and_then(|node| {
                let rm = node.mesh.as_ref().and_then(|h| gltf_meshes.get(h))?;
                Some((rm, node.transform))
            });
        let bodies = gm.primitives.iter().map(|prim| (prim, None));
        let rotors = rotor
            .into_iter()
            .flat_map(|(rm, t)| rm.primitives.iter().map(move |prim| (prim, Some(t))));
        for (prim, rotor) in bodies.chain(rotors) {
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
                    slot: slot_name,
                    rotor,
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
    spawn_kit_parts(p, kit, piece, t, rise).is_some()
}

/// [`spawn_kit`], returning the spawned part entities (`None` when the piece is not in the kit).
///
/// A rotor part spawns as a child of its LOD's first body part, at its node transform, so it rides
/// the hero's placement and rise (scale and visibility) and [`crate::kit_scene::spin_rotors`] turns
/// it in its parent's frame. Its phase comes from the cell and where its hub stands on the tile.
fn spawn_kit_parts(
    p: &mut ChildSpawnerCommands,
    kit: &HashMap<String, Vec<KitPart>>,
    piece: &str,
    t: Transform,
    rise: Option<(Axial, RiseRole)>,
) -> Option<Vec<Entity>> {
    let parts = kit.get(piece)?;
    let mut out = vec![];
    let mut bodies: [Option<Entity>; 3] = [None; 3];
    for part in parts {
        if let Some(rest) = part.rotor {
            let Some(body) = bodies.get(part.lod).copied().flatten() else {
                continue;
            };
            let [x, z] = rise.map_or([0.0; 2], |(cell, _)| cell.to_world(HEX_RADIUS));
            let hub = Vec3::new(x, 0.0, z) + (t * rest).translation;
            let e = p.commands_mut().spawn((
                Mesh3d(part.mesh.clone()),
                MeshMaterial3d(part.material.clone()),
                rest,
                KitRotor {
                    base: rest,
                    phase: rotor_phase(hub),
                },
                lod_range(part.lod),
                Lod(part.lod as u8),
                SceneEntity,
                ChildOf(body),
            ));
            out.push(e.id());
            continue;
        }
        let mut e = p.spawn((
            Mesh3d(part.mesh.clone()),
            MeshMaterial3d(part.material.clone()),
            t,
            lod_range(part.lod),
            Lod(part.lod as u8),
            SceneEntity,
        ));
        if let Some((cell, role)) = rise {
            e.insert(RisePart::new(cell, role, t));
        }
        if let Some(b) = bodies.get_mut(part.lod) {
            b.get_or_insert(e.id());
        }
        out.push(e.id());
    }
    Some(out)
}

/// Re-draws the pair edge pieces of `cells` under their tile roots. Each piece is authored for
/// edge 0 and placed at its local edge; the root already carries the tile's rotation. A tile new
/// to the view rises its pieces with its edges; a neighbour's update draws them at rest.
fn respawn_pairs(
    commands: &mut Commands,
    spawned: &mut PlaySpawned,
    kit: &HashMap<String, Vec<KitPart>>,
    cells: &BTreeSet<Axial>,
    rising: &BTreeSet<Axial>,
) {
    for &cell in cells {
        for e in spawned.pair_entities.remove(&cell).unwrap_or_default() {
            commands.entity(e).try_despawn();
        }
        let (Some((_, root)), Some(pieces)) = (spawned.tiles.get(&cell), spawned.pairs.get(cell))
        else {
            continue;
        };
        let rise = rising.contains(&cell).then_some((cell, RiseRole::Edge));
        let mut ids = vec![];
        commands.entity(*root).with_children(|p| {
            for (e, piece) in pieces.iter().enumerate() {
                if let Some(piece) = piece {
                    let t = Transform::from_rotation(piece_rotation(e as u8));
                    ids.extend(
                        spawn_kit_parts(p, kit, piece.as_str(), t, rise).unwrap_or_default(),
                    );
                }
            }
        });
        spawned.pair_entities.insert(cell, ids);
    }
}

/// Child transform for something authored on edge 0, moved to local edge `e` at `along` metres
/// from the centre and height `y`.
pub(crate) fn on_edge(e: u8, along: f32, y: f32) -> Transform {
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
    // A water tile's water surface comes from `water::tiles`, and so does its placeholder bed when
    // its base (the bed) is missing. While its centre is missing no river strips or hub are drawn
    // over the channel; road strips stay as the bridge deck.
    let water_bed = water::tiles::stand_in_bed(def, |id| kit.contains_key(id));
    let pending_water = !center_kit && water::tiles::water_kind(def, 0).is_some();
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
                && (s.kind == SocketKind::Road || (s.kind == SocketKind::River && !pending_water)))
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
        if pending_water {
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

/// Spawns and despawns tile entities to match the view. Tiles wait for the core pack. A placed
/// tile that draws a piece from a pack still streaming is hidden, and so are its water entities,
/// until every such pack arrives (owner decision A10, 2026-10-14): the placement itself has
/// already registered in the game, and the empty slot shows the [`PlayWaiting`] outline. On
/// arrival its children are respawned with the kit look and it rises then (or appears at rest if
/// it was hidden by a rebuild). A piece outside the manifest, or one that failed to load, still
/// draws its placeholder, since its pack is ready. Water tiles also get their surfaces, cut faces
/// and placeholder beds (`water::tiles::sync`, which respawns a cell whose base piece arrived).
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
    (packs, waiting_out, children): (
        Option<Res<PlayPacks>>,
        Option<ResMut<PlayWaiting>>,
        Query<&Children>,
    ),
) {
    let (Scenario::Play, Some(mut spawned), Some(plain)) = (active.0, spawned, plain) else {
        return;
    };
    let waits = |t: &PlayTile| {
        packs.as_deref().is_some_and(|p| {
            catalog
                .0
                .get(&t.tile)
                .is_some_and(|def| p.tile_waiting(def, &catalog.0))
        })
    };
    // Hidden tiles whose packs have all arrived: drawn this frame even when no piece was built
    // (a piece that failed to load draws its placeholder).
    let arrived: Vec<Axial> = spawned
        .hidden
        .keys()
        .copied()
        .filter(|c| spawned.tiles.get(c).is_some_and(|(t, _)| !waits(t)))
        .collect();
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
    if !spawned.bypass_change_detection().rebuild_for(view.epoch)
        && !view.is_changed()
        && spawned.refresh.is_empty()
        && arrived.is_empty()
    {
        return;
    }
    let Some(PlayKit::Ready(kit)) = kit.as_deref() else {
        return;
    };
    let waiting_cells: HashSet<Axial> = view
        .tiles
        .iter()
        .filter(|t| waits(t))
        .map(|t| t.cell)
        .collect();
    let rebuild = spawned.rebuild;
    if rebuild {
        for (_, (_, e)) in spawned.tiles.drain() {
            commands.entity(e).try_despawn();
        }
        rises.reset(view.tiles.iter().map(|t| t.cell), &mut commands);
        for &c in &waiting_cells {
            rises.undrawn(c);
        }
        spawned.rebuild = false;
        spawned.pairs = PairPieces::default();
        spawned.pair_entities.clear();
        spawned.refresh.clear();
        spawned.hidden.clear();
    }
    // Water entities of hidden tiles wait with them.
    let water_tiles: Vec<PlayTile> = view
        .tiles
        .iter()
        .filter(|t| !waiting_cells.contains(&t.cell))
        .cloned()
        .collect();
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
            &water_tiles,
            &catalog.0,
            |id| kit.contains_key(id),
            rebuild,
        )
        .into_iter()
        .collect();
    }
    let mut out = vec![];
    let want: HashMap<Axial, &PlayTile> = view.tiles.iter().map(|t| (t.cell, t)).collect();
    let (mut changed, swapped_cells) = sync_spans::remove_tiles(
        &mut commands,
        &mut spawned,
        &view,
        &want,
        &mut released,
        &children,
        (&mut rises, reduced.0, &mut out),
    );
    let (refreshed, shown) = sync_spans::refresh_tiles(
        &mut commands,
        &mut spawned,
        &plain,
        kit,
        &catalog.0,
        arrived,
        &mut rises,
        reduced.0,
        &mut out,
    );
    let added: Vec<&PlayTile> = view
        .tiles
        .iter()
        .filter(|t| !spawned.tiles.contains_key(&t.cell))
        .collect();
    if !rebuild && !added.is_empty() {
        rises.fast_forward(&mut out);
    }
    let added_cells: BTreeSet<Axial> = added.iter().map(|t| t.cell).collect();
    for t in added {
        let swap = swapped_cells.contains(&t.cell);
        if !swap {
            rises.cancel_sink(t.cell, &mut commands);
        }
        let e = spawn_tile(&mut commands, &plain, kit, &catalog.0, t);
        spawned.tiles.insert(t.cell, (t.clone(), e));
        if waiting_cells.contains(&t.cell) {
            commands.entity(e).insert(Visibility::Hidden);
            spawned.hidden.insert(t.cell, !rebuild);
            rises.undrawn(t.cell);
            continue;
        }
        match catalog.0.get(&t.tile).filter(|_| !rebuild) {
            Some(def) => {
                rises.begin(t.cell, &t.tile, &def.rise, reduced.0, &mut out);
                if swap {
                    rises.delay(t.cell, rise::SinkKind::Replace.seconds());
                }
            }
            None => rises.rest(t.cell),
        }
    }
    changed.extend(added_cells.iter().copied());
    let at = |cell: Axial| {
        want.get(&cell)
            .and_then(|t| Some((catalog.0.get(&t.tile)?, t.rotation)))
    };
    let mut redraw = spawned.pairs.update(&catalog.0, changed, at);
    // A tile drawn afresh, or whose children a pack refresh rebuilt, has none of its pair pieces
    // yet, even where the resolution did not change; a pair piece whose pack just arrived swaps
    // from nothing to the kit in place.
    redraw.extend(added_cells.iter().copied());
    redraw.extend(refreshed);
    let mut rising = if rebuild {
        BTreeSet::new()
    } else {
        added_cells
    };
    rising.extend(shown);
    respawn_pairs(&mut commands, &mut spawned, kit, &redraw, &rising);
    cues.write_batch(out);
    let mut now: Vec<Axial> = spawned.hidden.keys().copied().collect();
    now.sort_by_key(|c| (c.q, c.r));
    if let Some(mut w) = waiting_out
        && w.0 != now
    {
        w.0 = now;
    }
}

mod sync_spans {
    use super::*;

    pub(super) fn remove_tiles(
        commands: &mut Commands,
        spawned: &mut PlaySpawned,
        view: &PlayView,
        want: &HashMap<Axial, &PlayTile>,
        released: &mut HashMap<Axial, Vec<Entity>>,
        children: &Query<&Children>,
        animation: (&mut PlayRises, bool, &mut Vec<RiseMessage>),
    ) -> (BTreeSet<Axial>, HashSet<Axial>) {
        let (rises, reduced, out) = animation;
        let mut gone = vec![];
        // A cell whose tile changed in place (a replace, or its undo or redo): the old tile sinks
        // without dust and the new one rises once it is down.
        let mut swapped = vec![];
        let mut changed: BTreeSet<Axial> = BTreeSet::new();
        spawned.tiles.retain(|cell, (t, e)| match want.get(cell) {
            Some(w) if *w == t => true,
            Some(_) => {
                swapped.push((*cell, t.tile.clone(), *e));
                changed.insert(*cell);
                false
            }
            None => {
                gone.push((*cell, t.tile.clone(), *e));
                false
            }
        });
        gone.sort_by_key(|(c, ..)| (c.q, c.r));
        changed.extend(gone.iter().map(|(c, ..)| *c));
        for c in &changed {
            spawned.hidden.remove(c);
        }
        for (cell, tile, root) in gone {
            let mut entities = vec![root];
            entities.extend(released.remove(&cell).unwrap_or_default());
            let kind = if view.bulldozed == Some(cell) {
                rise::SinkKind::Bulldoze
            } else {
                rise::SinkKind::Undo
            };
            rises.remove(cell, &tile, kind, entities, reduced, commands, out);
        }
        let swapped_cells: HashSet<Axial> = swapped.iter().map(|(c, ..)| *c).collect();
        for (cell, tile, root) in swapped {
            let mut entities = vec![root];
            entities.extend(released.remove(&cell).unwrap_or_default());
            // The new tile's rise poses every part of this cell; the sinking tile's parts leave it.
            for &e in &entities {
                commands.entity(e).try_remove::<RisePart>();
                for d in children.iter_descendants(e) {
                    commands.entity(d).try_remove::<RisePart>();
                }
            }
            rises.remove(
                cell,
                &tile,
                rise::SinkKind::Replace,
                entities,
                reduced,
                commands,
                out,
            );
        }
        for (_, es) in std::mem::take(released) {
            for e in es {
                commands.entity(e).try_despawn();
            }
        }
        (changed, swapped_cells)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn refresh_tiles(
        commands: &mut Commands,
        spawned: &mut PlaySpawned,
        plain: &PlainAssets,
        kit: &HashMap<String, Vec<KitPart>>,
        catalog: &TileCatalog,
        arrived: Vec<Axial>,
        rises: &mut PlayRises,
        reduced: bool,
        out: &mut Vec<RiseMessage>,
    ) -> (BTreeSet<Axial>, BTreeSet<Axial>) {
        let refresh = std::mem::take(&mut spawned.refresh);
        let mut refreshed: BTreeSet<Axial> = BTreeSet::new();
        let mut shown: BTreeSet<Axial> = BTreeSet::new();
        for cell in arrived {
            let (Some(rise), Some((t, root))) =
                (spawned.hidden.remove(&cell), spawned.tiles.get(&cell))
            else {
                continue;
            };
            let Some(def) = catalog.get(&t.tile) else {
                continue;
            };
            commands
                .entity(*root)
                .insert(Visibility::Inherited)
                .despawn_children()
                .with_children(|p| tile_children(p, plain, kit, catalog, def, cell));
            if rise {
                rises.begin(cell, &t.tile, &def.rise, reduced, out);
            } else {
                rises.rest(cell);
            }
            refreshed.insert(cell);
            shown.insert(cell);
        }
        for (cell, (t, root)) in spawned.tiles.iter().filter(|_| !refresh.is_empty()) {
            if shown.contains(cell) || spawned.hidden.contains_key(cell) {
                continue;
            }
            let Some(def) = catalog.get(&t.tile) else {
                continue;
            };
            if packs::drawn_pieces(def, catalog)
                .iter()
                .any(|p| refresh.contains(*p))
            {
                commands
                    .entity(*root)
                    .despawn_children()
                    .with_children(|p| tile_children(p, plain, kit, catalog, def, *cell));
                refreshed.insert(*cell);
            }
            let pairs_arrived = spawned
                .pairs
                .get(*cell)
                .is_some_and(|ps| ps.iter().flatten().any(|p| refresh.contains(p.as_str())));
            if pairs_arrived {
                refreshed.insert(*cell);
            }
        }
        (refreshed, shown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_view_epoch_redraws_every_tile_at_rest_once() {
        let mut s = PlaySpawned::default();
        assert!(!s.rebuild_for(0), "same epoch: tiles keep their rises");
        assert!(s.rebuild_for(1), "a replaced world redraws at rest");
        s.rebuild = false; // `sync` consumes the flag
        assert!(
            !s.rebuild_for(1),
            "once per epoch, so later placements rise"
        );
    }

    #[test]
    fn on_edge_points_at_the_edge_midpoint() {
        for e in 0..6u8 {
            let t = on_edge(e, APOTHEM, 0.0);
            let a = (60.0 * e as f32).to_radians();
            let want = Vec3::new(a.cos(), 0.0, a.sin()) * APOTHEM;
            assert!((t.translation - want).length() < 1e-3, "edge {e}");
        }
    }

    /// A hero's rotor (the windmill's sails) spawns under its LOD's body at its node transform with
    /// a [`KitRotor`] and no rise part of its own, so the rise moves it with the tower and
    /// `spin_rotors` alone writes its transform.
    #[test]
    fn a_hero_rotor_spawns_under_its_lod_body() {
        let rest = Transform::from_xyz(1.0, 9.0, 0.0);
        let part = |lod: usize, rotor: Option<Transform>| KitPart {
            lod,
            mesh: Handle::default(),
            material: Handle::default(),
            slot: "windmill_sail".into(),
            rotor,
        };
        let kit = HashMap::from([(
            "windmill".to_owned(),
            vec![
                part(0, None),
                part(0, Some(rest)),
                part(1, None),
                part(1, Some(rest)),
            ],
        )]);
        let cell = Axial::new(1, 0);
        let role = RiseRole::Prop {
            order: 0,
            count: 1,
            kind: PropKind::Building { floors: 2 },
        };
        let t = Transform::from_rotation(Quat::from_rotation_y(-120f32.to_radians()));
        let mut world = World::new();
        let root = world.spawn_empty().id();
        world.commands().entity(root).with_children(|p| {
            let ids = spawn_kit_parts(p, &kit, "windmill", t, Some((cell, role))).unwrap();
            assert_eq!(ids.len(), 4);
        });
        world.flush();
        let mut rotors = world.query::<(&KitRotor, &Transform, &ChildOf, Has<RisePart>)>();
        let found: Vec<_> = rotors
            .iter(&world)
            .map(|(r, tf, c, rises)| (*r, *tf, c.parent(), rises))
            .collect();
        assert_eq!(found.len(), 2, "one rotor per LOD");
        for (r, tf, parent, rises) in &found {
            assert_eq!((r.base, *tf), (rest, rest));
            assert!(!rises, "the rotor rides its body's rise");
            assert_ne!(*parent, root);
            let body = world.entity(*parent);
            assert!(body.contains::<RisePart>() && !body.contains::<KitRotor>());
        }
        assert_ne!(found[0].2, found[1].2, "each LOD's rotor has its own body");
        assert_eq!(found[0].0.phase, found[1].0.phase, "one hub, one phase");
    }

    #[test]
    fn every_mvp_tile_resolves_and_biomes_index() {
        let c = PlayCatalog::default();
        assert_eq!(c.0.tiles().len(), 19);
        let biomes: std::collections::BTreeSet<_> =
            c.0.tiles().iter().map(|t| biome_index(t.biome)).collect();
        assert_eq!(biomes.len(), 4);
    }
}
