//! `scenario=play`: draws the game world that `hex-app` owns (PRD §4.1). This module holds no game
//! rules: `hex-app` writes [`PlayView`] and this module renders it.
//!
//! Placed tiles use kit pieces from `assets/kit/manifest.json` where the tile references them and
//! plain coloured placeholders otherwise (a biome prism, lane strips, blocks). The ghost, its edge
//! status markers, the frontier outlines and the bulldoze highlight are deliberately plain
//! in-world placeholders, to be restyled once the UI direction is approved (§4.7, D18).
use std::collections::HashMap;

use bevy::{
    asset::RenderAssetUsages,
    gltf::{Gltf, GltfMesh},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::{renderer::RenderDevice, settings::WgpuFeatures},
};
use hex_assets::{KitManifest, TextureFamily, parse_manifest, select_family};
use hex_core::{
    Axial, Biome, EdgeStatus, HEX_RADIUS, LEVEL_STEP, SocketKind, TileCatalog, TileDef, hex_prism,
};

use crate::{
    ActiveScenario, Scenario, SceneEntity,
    kit_material::{KitMaterial, build_material, slot_requests},
    kit_scene::{MANIFEST, piece_rotation},
    scene::lod_range,
    workload::GateConfig,
};

const APOTHEM: f32 = HEX_RADIUS * 0.866_025_4;

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

/// Ghost, edge markers, frontier outlines and the bulldoze highlight.
#[derive(Component)]
pub struct PlayOverlay;

/// Entities spawned for the current view. `setup` replaces it on every switch into the scenario,
/// since the switch despawned the previous scene.
#[derive(Resource, Default)]
pub struct PlaySpawned {
    tiles: HashMap<Axial, (PlayTile, Entity)>,
    overlay: Vec<Entity>,
    rebuild: bool,
}

#[derive(Clone)]
pub struct KitPart {
    lod: usize,
    mesh: Handle<Mesh>,
    material: Handle<KitMaterial>,
}

/// The kit pieces, loaded once on the first entry into the scenario. A piece missing from the
/// manifest (pending, plan F4) or failing to load has no entry and draws as a placeholder.
#[derive(Resource)]
pub enum PlayKit {
    Loading {
        manifest: KitManifest,
        items: Vec<(String, Handle<Gltf>)>,
        family: TextureFamily,
    },
    Ready(HashMap<String, Vec<KitPart>>),
}

#[derive(Resource)]
pub struct PlainAssets {
    prism: Handle<Mesh>,
    ghost: Handle<Mesh>,
    ring: Handle<Mesh>,
    strip: [Handle<Mesh>; 2],
    hub: Handle<Mesh>,
    pad: Handle<Mesh>,
    block: Handle<Mesh>,
    chip: Handle<Mesh>,
    bar: Handle<Mesh>,
    biome: [Handle<StandardMaterial>; 4],
    socket: [Handle<StandardMaterial>; 6],
    status: [Handle<StandardMaterial>; 3],
    block_mat: Handle<StandardMaterial>,
    ghost_mat: [Handle<StandardMaterial>; 2],
    bulldoze_mat: [Handle<StandardMaterial>; 2],
    frontier_mat: Handle<StandardMaterial>,
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

fn status_index(s: EdgeStatus) -> usize {
    match s {
        EdgeStatus::Compatible => 0,
        EdgeStatus::Incompatible => 1,
        EdgeStatus::Neutral => 2,
    }
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

/// A flat hexagonal outline between `inner` and `outer` circumradius, facing up.
pub fn ring_mesh(inner: f32, outer: f32) -> Mesh {
    let corner = |r: f32, i: usize| {
        let a = (30.0 + 60.0 * i as f32).to_radians();
        [r * a.cos(), 0.0, r * a.sin()]
    };
    let positions: Vec<[f32; 3]> = (0..6)
        .flat_map(|i| [corner(outer, i), corner(inner, i)])
        .collect();
    let mut indices = vec![];
    for i in 0..6u32 {
        let (o0, i0, o1, i1) = (2 * i, 2 * i + 1, 2 * ((i + 1) % 6), 2 * ((i + 1) % 6) + 1);
        // Corners run clockwise viewed from +Y, so (o0, i0, o1) is counter-clockwise from above.
        indices.extend_from_slice(&[o0, i0, o1, o1, i0, i1]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        vec![[0.0, 1.0, 0.0]; positions.len()],
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; positions.len()])
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_indices(Indices::U32(indices))
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
            ghost: meshes.add(prism_mesh(HEX_RADIUS * 0.97, 0.8)),
            ring: meshes.add(ring_mesh(HEX_RADIUS * 0.86, HEX_RADIUS * 0.94)),
            strip: [6.0, 8.0].map(|w| meshes.add(Cuboid::new(APOTHEM, 0.9, w))),
            hub: meshes.add(Cylinder::new(4.0, 0.9)),
            pad: meshes.add(Cuboid::new(11.0, 0.5, 11.0)),
            block: meshes.add(Cuboid::new(6.0, 5.0, 6.0)),
            chip: meshes.add(Cuboid::new(2.5, 0.5, 5.0)),
            bar: meshes.add(Cuboid::new(1.0, 0.6, HEX_RADIUS * 0.8)),
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
            status: [
                Color::srgb(0.15, 0.95, 0.30),
                Color::srgb(1.00, 0.12, 0.10),
                Color::srgb(0.38, 0.40, 0.46),
            ]
            .map(|c| m(flat(c))),
            block_mat: m(lit(Color::srgb(0.80, 0.72, 0.60))),
            ghost_mat: [
                Color::srgba(0.92, 0.96, 1.00, 0.35),
                Color::srgba(1.00, 0.18, 0.15, 0.45),
            ]
            .map(|c| m(flat(c))),
            bulldoze_mat: [
                Color::srgba(1.00, 0.60, 0.10, 0.45),
                Color::srgba(1.00, 0.10, 0.10, 0.55),
            ]
            .map(|c| m(flat(c))),
            frontier_mat: m(flat(Color::srgba(1.0, 1.0, 1.0, 0.28))),
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
    Transform::from_xyz(14.0, 95.0, 125.0).looking_at(Vec3::new(14.0, 0.0, 16.0), Vec3::Y)
}

/// Starts loading every manifest piece the first time the play scenario runs.
pub fn start_kit(
    mut commands: Commands,
    active: Res<ActiveScenario>,
    kit: Option<Res<PlayKit>>,
    device: Option<Res<RenderDevice>>,
    server: Res<AssetServer>,
) {
    let (Scenario::Play, None, Some(device)) = (active.0, kit, device) else {
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
    let items = manifest
        .pieces
        .iter()
        .map(|e| (e.id.clone(), server.load(format!("kit://{}", e.glb))))
        .collect();
    commands.insert_resource(PlayKit::Loading {
        manifest,
        items,
        family,
    });
}

/// Builds each loaded piece's meshes and materials (the slot walk of `kit_scene::finish_kit`).
pub fn finish_kit(
    mut kit: Option<ResMut<PlayKit>>,
    spawned: Option<ResMut<PlaySpawned>>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    mut materials: ResMut<Assets<KitMaterial>>,
    config: Res<GateConfig>,
) {
    let Some(PlayKit::Loading {
        manifest,
        items,
        family,
    }) = kit.as_deref()
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
    let mut pieces: HashMap<String, Vec<KitPart>> = HashMap::new();
    for (id, h) in items {
        let (Some(g), Some(entry)) = (
            gltfs.get(h).filter(|_| !failed(h)),
            manifest.pieces.iter().find(|e| &e.id == id),
        ) else {
            warn!("play: kit piece {id} failed to load, drawn as a placeholder");
            continue;
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
                    .or_insert_with(|| match slot_requests(manifest, entry, slot, *family) {
                        Ok(r) => {
                            Some(materials.add(build_material(&server, slot, &r, config.kit_hooks)))
                        }
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
        if !parts.is_empty() {
            pieces.insert(id.clone(), parts);
        }
    }
    info!("play: kit pieces ready: {:?}", {
        let mut ids: Vec<_> = pieces.keys().collect();
        ids.sort();
        ids
    });
    if let Some(kit) = kit.as_deref_mut() {
        *kit = PlayKit::Ready(pieces);
    }
    if let Some(mut s) = spawned {
        s.rebuild = true;
    }
}

fn spawn_kit(
    p: &mut ChildSpawnerCommands,
    kit: &HashMap<String, Vec<KitPart>>,
    piece: &str,
    t: Transform,
) -> bool {
    let Some(parts) = kit.get(piece) else {
        return false;
    };
    for part in parts {
        p.spawn((
            Mesh3d(part.mesh.clone()),
            MeshMaterial3d(part.material.clone()),
            t,
            lod_range(part.lod),
            SceneEntity,
        ));
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
    commands
        .spawn((
            PlayTileRoot,
            Transform::from_xyz(x, 0.0, z).with_rotation(piece_rotation(t.rotation)),
            Visibility::default(),
            SceneEntity,
        ))
        .with_children(|p| match def {
            Some(def) => tile_children(p, plain, kit, catalog, def),
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

fn tile_children(
    p: &mut ChildSpawnerCommands,
    plain: &PlainAssets,
    kit: &HashMap<String, Vec<KitPart>>,
    catalog: &TileCatalog,
    def: &TileDef,
) {
    let plain_mesh = |p: &mut ChildSpawnerCommands,
                      mesh: &Handle<Mesh>,
                      mat: &Handle<StandardMaterial>,
                      t: Transform| {
        p.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(mat.clone()),
            t,
            SceneEntity,
        ));
    };
    if !spawn_kit(p, kit, def.base.as_str(), Transform::IDENTITY) {
        plain_mesh(
            p,
            &plain.prism,
            &plain.biome[biome_index(def.biome)],
            Transform::from_xyz(0.0, -LEVEL_STEP, 0.0),
        );
    }
    let lanes: Vec<(u8, SocketKind)> = (0..6u8)
        .filter_map(|e| {
            let s = &def.edges[e as usize];
            (s.lane.is_some() && matches!(s.kind, SocketKind::Road | SocketKind::River))
                .then_some((e, s.kind))
        })
        .collect();
    let center_kit = def
        .center
        .as_ref()
        .is_some_and(|c| kit.contains_key(c.as_str()));
    if center_kit {
        let center = def.center.as_ref().expect("checked above");
        spawn_kit(p, kit, center.as_str(), Transform::IDENTITY);
        let pieces = def.edge_pieces(catalog.sockets());
        for &(e, kind) in &lanes {
            let piece = pieces[e as usize].map(|k| k.as_str()).unwrap_or_default();
            if !spawn_kit(p, kit, piece, Transform::from_rotation(piece_rotation(e))) {
                let i = (kind == SocketKind::River) as usize;
                plain_mesh(
                    p,
                    &plain.strip[i],
                    &plain.socket[socket_index(kind)],
                    on_edge(e, APOTHEM / 2.0, 0.05),
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
            );
        }
        if let Some(&(_, kind)) = lanes.first() {
            plain_mesh(
                p,
                &plain.hub,
                &plain.socket[socket_index(kind)],
                Transform::from_xyz(0.0, 0.05, 0.0),
            );
        } else if def.center.is_some() && def.biome == Biome::Grass {
            plain_mesh(
                p,
                &plain.pad,
                &plain.block_mat,
                Transform::from_xyz(0.0, 0.25, 0.0),
            );
        }
    }
    for h in &def.props.hero {
        let t = Transform::from_xyz(h.at[0], 0.0, h.at[1])
            .with_rotation(Quat::from_rotation_y(-h.yaw_deg.to_radians()));
        if !spawn_kit(p, kit, h.piece.as_str(), t) {
            plain_mesh(
                p,
                &plain.block,
                &plain.block_mat,
                t * Transform::from_xyz(0.0, 2.5, 0.0),
            );
        }
    }
}

fn spawn_overlay(
    commands: &mut Commands,
    plain: &PlainAssets,
    catalog: &TileCatalog,
    view: &PlayView,
) -> Vec<Entity> {
    let mut out = vec![];
    let at = |c: Axial, y: f32| {
        let [x, z] = c.to_world(HEX_RADIUS);
        Transform::from_xyz(x, y, z)
    };
    for &c in &view.frontier {
        out.push(
            commands
                .spawn((
                    Mesh3d(plain.ring.clone()),
                    MeshMaterial3d(plain.frontier_mat.clone()),
                    at(c, 0.1),
                    PlayOverlay,
                    SceneEntity,
                ))
                .id(),
        );
    }
    if let Some(g) = &view.ghost {
        let def = catalog.get(&g.tile);
        let root = commands
            .spawn((
                at(g.cell, 0.0),
                Visibility::default(),
                PlayOverlay,
                SceneEntity,
            ))
            .with_children(|p| {
                p.spawn((
                    Mesh3d(plain.ghost.clone()),
                    MeshMaterial3d(plain.ghost_mat[(!g.valid) as usize].clone()),
                    Transform::from_xyz(0.0, 0.6, 0.0),
                    SceneEntity,
                ));
                // The ghost's own sockets, so its rotation reads in a still frame.
                if let Some(def) = def {
                    for e in 0..6u8 {
                        let w = (e + g.rotation) % 6;
                        p.spawn((
                            Mesh3d(plain.chip.clone()),
                            MeshMaterial3d(
                                plain.socket[socket_index(def.edges[e as usize].kind)].clone(),
                            ),
                            on_edge(w, APOTHEM * 0.62, 1.55),
                            SceneEntity,
                        ));
                    }
                }
                for (w, s) in g.edges.iter().enumerate() {
                    p.spawn((
                        Mesh3d(plain.bar.clone()),
                        MeshMaterial3d(plain.status[status_index(*s)].clone()),
                        on_edge(w as u8, APOTHEM * 0.93, 1.5),
                        SceneEntity,
                    ));
                }
            })
            .id();
        out.push(root);
    }
    if let Some((c, allowed)) = view.bulldoze {
        out.push(
            commands
                .spawn((
                    Mesh3d(plain.ghost.clone()),
                    MeshMaterial3d(plain.bulldoze_mat[(!allowed) as usize].clone()),
                    at(c, 0.8).with_scale(Vec3::new(1.0, 3.0, 1.0)),
                    PlayOverlay,
                    SceneEntity,
                ))
                .id(),
        );
    }
    out
}

/// Spawns and despawns tile entities to match the view, and rebuilds the overlay when it changes.
/// Tiles wait for the kit so a tile never swaps from placeholder to kit piece mid-game.
pub fn sync(
    mut commands: Commands,
    active: Res<ActiveScenario>,
    view: Res<PlayView>,
    spawned: Option<ResMut<PlaySpawned>>,
    plain: Option<Res<PlainAssets>>,
    kit: Option<Res<PlayKit>>,
    catalog: Res<PlayCatalog>,
) {
    let (Scenario::Play, Some(mut spawned), Some(plain)) = (active.0, spawned, plain) else {
        return;
    };
    if !view.is_changed() && !spawned.rebuild {
        return;
    }
    for e in spawned.overlay.drain(..) {
        commands.entity(e).try_despawn();
    }
    spawned.overlay = spawn_overlay(&mut commands, &plain, &catalog.0, &view);

    let Some(PlayKit::Ready(kit)) = kit.as_deref() else {
        return;
    };
    if spawned.rebuild {
        for (_, (_, e)) in spawned.tiles.drain() {
            commands.entity(e).try_despawn();
        }
        spawned.rebuild = false;
    }
    let want: HashMap<Axial, &PlayTile> = view.tiles.iter().map(|t| (t.cell, t)).collect();
    spawned.tiles.retain(|cell, (t, e)| {
        let keep = want.get(cell).is_some_and(|w| *w == t);
        if !keep {
            commands.entity(*e).try_despawn();
        }
        keep
    });
    for t in &view.tiles {
        spawned.tiles.entry(t.cell).or_insert_with(|| {
            let e = spawn_tile(&mut commands, &plain, kit, &catalog.0, t);
            (t.clone(), e)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_faces_up() {
        let m = ring_mesh(10.0, 12.0);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) =
            m.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions missing");
        };
        let Some(Indices::U32(idx)) = m.indices() else {
            panic!("indices missing");
        };
        assert_eq!(idx.len(), 36);
        for t in idx.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(p[t[k] as usize]));
            assert!((b - a).cross(c - a).y > 0.0, "triangle {t:?} faces down");
        }
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

    #[test]
    fn every_mvp_tile_resolves_and_biomes_index() {
        let c = PlayCatalog::default();
        assert_eq!(c.0.tiles().len(), 12);
        let biomes: std::collections::BTreeSet<_> =
            c.0.tiles().iter().map(|t| biome_index(t.biome)).collect();
        assert_eq!(biomes.len(), 4);
    }
}
