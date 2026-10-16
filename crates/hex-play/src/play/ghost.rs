//! The ghost's see-through meshes (`design/ui/legend/in-world.md`, Ghost): the selected tile's kit
//! pieces at the ghost's cell and rotation, tinted by role and drawn at 45% with [`GhostMaterial`].
//!
//! Scatter: the ghost draws the tile's seeded scatter ([`scatter`]) exactly as the placed tile will
//! get it: the same prepared ground ([`PlayScatter::ground`]), the seed `cell_seed(world seed, cell)`
//! and each instance's own transform under a root that carries the tile's rotation. The layout
//! depends on the tile and the seed only, never on the neighbours, so it is known before placement.
//! The ghost draws every part at LOD0, as the placed tile does within the kit's near LOD range.
//!
//! Water tiles (coast, ocean, river): each ground part takes the tint of its manifest material slot
//! (grass, dry and damp sand, seabed or river bed, stones) rather than one tint per piece, and a
//! flat water surface at the water level covers the part of the hex where the tile's own water
//! field ([`water::tiles::layout`] of the tile alone) has its bed below the level. Both are
//! tile-local, so they turn with the root; the surface mesh is built once per tile.
//!
//! Sampled-texture budget: [`GhostMaterial`] binds one uniform and no texture, so its pipeline
//! samples only the view's textures, fewer than the kit forward pipeline, which is exactly at
//! WebGPU's 16 beside refracting water. It stays out of the prepass and the shadow passes.
//!
//! While a piece the tile draws is in a pack still streaming there is no mesh to show: the
//! affordances keep the outline, the edge colours and the lane strokes ([`GhostDrawn`]).
use std::collections::{HashMap, HashSet};

use bevy::{
    asset::RenderAssetUsages,
    gltf::{Gltf, GltfMesh},
    light::NotShadowCaster,
    mesh::PrimitiveTopology,
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};
use hex_core::{Axial, Biome, HEX_RADIUS, SocketKind, TileCatalog, TileDef, cell_seed};

use super::{
    KitPart, PlayCatalog, PlayKit, PlayTile, PlayView,
    affordances::socket_rgb,
    packs::{PackState, PlayPacks},
    rise::ReducedMotion,
    scatter::{self, Layout, PlayScatter},
};
use crate::{
    ActiveScenario, Scenario, SceneEntity,
    kit_scene::piece_rotation,
    water::{self, WaterField, tiles::WaterKind},
};

pub const GHOST_SHADER: &str = "shaders/ghost.wesl";

/// Opacity of the ghost meshes.
const GHOST_ALPHA: f32 = 0.45;
/// Fade in after the slot is hovered, as the ghost outline does (s).
const FADE_DELAY: f32 = 0.15;
const FADE_IN: f32 = 0.12;
const INK: [u8; 3] = [238, 243, 246];
const BUILDING: [u8; 3] = [222, 210, 190];
/// Ground props: the scatter's rocks and wildflowers, and a water tile's stones.
const STONE: [u8; 3] = [176, 172, 160];
/// A water tile's damp sand band and its seabed or river bed.
const WET_SAND: [u8; 3] = [186, 158, 106];
const SEABED: [u8; 3] = [62, 146, 156];
/// Sort bias (m of view depth) that draws the water surface after the bed under it: both sit near
/// the tile centre, and the transparent phase sorts by mesh centre.
const WATER_BIAS: f32 = 4.0;
/// Steps along each side of the hex's six sector triangles in the water surface grid.
const WATER_GRID: u32 = 16;

/// One colour, no texture: lit by a fixed key light in `ghost.wesl`.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct GhostMaterial {
    /// Linear rgb and opacity.
    #[uniform(100)]
    pub color: Vec4,
    /// Transparent-sort bias toward the camera (m).
    pub bias: f32,
}

impl Material for GhostMaterial {
    fn fragment_shader() -> ShaderRef {
        GHOST_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn depth_bias(&self) -> f32 {
        self.bias
    }
}

/// Whether the current ghost draws its tile's meshes; the affordances drop the lane strokes then.
#[derive(Resource, Default, Debug, PartialEq, Eq)]
pub struct GhostDrawn(pub bool);

/// `hexWorld.state().ghost` as JSON (empty while no ghost draws meshes): its cell, tile, rotation,
/// scatter seed and layout fingerprint, scatter counts per piece, and its draw cost.
#[derive(Resource, Clone, Debug, Default)]
pub struct GhostReport(pub String);

/// The tint roles of a ghost's parts.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Role {
    Base(Biome),
    Lane(SocketKind),
    Centre,
    Tree,
    Building,
    Prop,
    Grass,
    Sand,
    WetSand,
    Seabed,
    /// A water tile's surface: the ocean's colour on coasts and the open sea, the river's on rivers.
    Water(SocketKind),
}

/// What a spawned ghost shows: rebuilt only when one of these changes.
#[derive(Clone, Debug, PartialEq)]
struct Sig {
    cell: Axial,
    tile: String,
    rotation: u8,
    world_seed: u64,
    /// Per scatter rule: its piece is in the kit (scatter pieces do not gate [`tile_ready`]).
    scatter: Vec<bool>,
}

/// The spawned ghost: its root, what it shows and its fade clock.
#[derive(Resource, Default)]
pub struct GhostMeshes {
    root: Option<Entity>,
    sig: Option<Sig>,
    t: f32,
    /// The parts were respawned: write the materials' opacity even when the fade holds still.
    dirty: bool,
    materials: Vec<(Role, Handle<GhostMaterial>, Vec3)>,
    /// Water surface meshes by tile id (`None`: the tile carries no water).
    water: HashMap<String, Option<Handle<Mesh>>>,
}

/// sRGB desaturated 30% and tinted 15% toward ink, as linear rgb.
fn ghost_tint(rgb: [u8; 3]) -> Vec3 {
    let c = Vec3::new(rgb[0] as f32, rgb[1] as f32, rgb[2] as f32) / 255.0;
    let luma = c.dot(Vec3::new(0.2126, 0.7152, 0.0722));
    let ink = Vec3::new(INK[0] as f32, INK[1] as f32, INK[2] as f32) / 255.0;
    let c = c.lerp(Vec3::splat(luma), 0.3).lerp(ink, 0.15);
    let l = Color::srgb(c.x, c.y, c.z).to_linear();
    Vec3::new(l.red, l.green, l.blue)
}

fn role_rgb(role: Role) -> [u8; 3] {
    match role {
        Role::Base(Biome::Grass) => socket_rgb(SocketKind::Grass),
        Role::Base(Biome::Forest) => socket_rgb(SocketKind::Forest),
        Role::Base(Biome::Beach) => socket_rgb(SocketKind::Beach),
        Role::Base(Biome::Ocean) => socket_rgb(SocketKind::Ocean),
        Role::Lane(k) => socket_rgb(k),
        Role::Centre => BUILDING,
        Role::Tree => socket_rgb(SocketKind::Forest),
        Role::Building => BUILDING,
        Role::Prop => STONE,
        Role::Grass => socket_rgb(SocketKind::Grass),
        Role::Sand => socket_rgb(SocketKind::Beach),
        Role::WetSand => WET_SAND,
        Role::Seabed => SEABED,
        Role::Water(k) => socket_rgb(k),
    }
}

/// A water tile's ground part tint from its manifest slot name, or `None` to keep the piece's.
fn slot_role(slot: &str) -> Option<Role> {
    if slot.contains("seabed") || slot.ends_with("_bed") {
        Some(Role::Seabed)
    } else if slot.contains("wet") {
        Some(Role::WetSand)
    } else if slot.contains("sand") {
        Some(Role::Sand)
    } else if slot.contains("grass") {
        Some(Role::Grass)
    } else if ["pebble", "rock", "stone"].iter().any(|k| slot.contains(k)) {
        Some(Role::Prop)
    } else {
        None
    }
}

/// Keeps the water-covered part of triangle `tri` (where `depth` > 0), cut where `depth` crosses 0
/// along each edge (bisected: the beach slope is far steeper than the shelf, so a linear guess
/// would pull the shoreline seaward), as upward-facing triangles at height `y`.
fn clip_wet(out: &mut Vec<[f32; 3]>, tri: [Vec2; 3], depth: &impl Fn(Vec2) -> f32, y: f32) {
    let wet = tri.map(|p| depth(p) > 0.0);
    let cut = |p: Vec2, q: Vec2, wet_p: bool| {
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..16 {
            let m = 0.5 * (lo + hi);
            if (depth(p.lerp(q, m)) > 0.0) == wet_p {
                lo = m;
            } else {
                hi = m;
            }
        }
        p.lerp(q, 0.5 * (lo + hi))
    };
    let mut poly: Vec<Vec2> = vec![];
    for i in 0..3 {
        let (p, q) = (tri[i], tri[(i + 1) % 3]);
        if wet[i] {
            poly.push(p);
        }
        if wet[i] != wet[(i + 1) % 3] {
            poly.push(cut(p, q, wet[i]));
        }
    }
    for k in 1..poly.len().saturating_sub(1) {
        let (a, mut b, mut c) = (poly[0], poly[k], poly[k + 1]);
        let turn = (b - a).perp_dot(c - a);
        if turn.abs() < 1e-6 {
            continue;
        }
        if turn > 0.0 {
            std::mem::swap(&mut b, &mut c);
        }
        out.extend([a, b, c].map(|p| [p.x, y, p.y]));
    }
}

/// The water surface of `def` placed alone, tile-local at the water level: the hex wherever the
/// tile's own water field has its bed below the level, cut along the shoreline. `None` for a tile
/// that carries no water.
fn water_surface(def: &TileDef, catalog: &TileCatalog) -> Option<Mesh> {
    let alone = PlayTile {
        cell: Axial::ORIGIN,
        tile: def.id.clone(),
        rotation: 0,
    };
    let cells = water::tiles::layout(std::slice::from_ref(&alone), catalog, |_| true);
    let field = cells.get(&Axial::ORIGIN)?.field();
    let level = field.level();
    let depth = |p: Vec2| level - field.height(p.x, p.y);
    let corner = |k: u32| {
        let a = (30.0 + 60.0 * k as f32).to_radians();
        Vec2::new(a.cos(), a.sin()) * HEX_RADIUS
    };
    let n = WATER_GRID;
    let mut pos = vec![];
    for k in 0..6 {
        let (a, b) = (corner(k), corner(k + 1));
        let at = |i: u32, j: u32| (a * i as f32 + b * j as f32) / n as f32;
        for i in 0..n {
            for j in 0..n - i {
                clip_wet(
                    &mut pos,
                    [at(i, j), at(i + 1, j), at(i, j + 1)],
                    &depth,
                    level,
                );
                if i + j + 1 < n {
                    let tri = [at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)];
                    clip_wet(&mut pos, tri, &depth, level);
                }
            }
        }
    }
    if pos.is_empty() {
        return None;
    }
    let normals = vec![[0.0, 1.0, 0.0]; pos.len()];
    // Kept in the main world too: every respawn reads its vertex count for the ghost report.
    Some(
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals),
    )
}

/// Whether every pack the tile draws from has loaded, so its meshes can stand in for it.
pub fn tile_ready(
    def: &TileDef,
    catalog: &TileCatalog,
    kit: Option<&PlayKit>,
    packs: Option<&PlayPacks>,
) -> bool {
    let (Some(PlayKit::Ready(_)), Some(packs)) = (kit, packs) else {
        return false;
    };
    packs
        .tile_packs(def, catalog)
        .iter()
        .all(|p| packs.state(p) == Some(PackState::Ready))
}

/// The kit pieces the tile draws at rest, with their local transforms and tint roles: the base,
/// the centre and its lane edge pieces, and the hero props (`play::tile_children`'s kit path).
fn ghost_pieces<'a>(
    def: &'a TileDef,
    catalog: &'a TileCatalog,
    has: impl Fn(&str) -> bool,
) -> Vec<(&'a str, Transform, Role)> {
    let mut out = vec![];
    let center_kit = def.center.as_ref().is_some_and(|c| has(c.as_str()));
    if !water::tiles::stand_in_bed(def, &has) {
        out.push((
            def.base.as_str(),
            Transform::IDENTITY,
            Role::Base(def.biome),
        ));
    }
    if let Some(center) = def.center.as_ref().filter(|_| center_kit) {
        let lane_kind = def.edges.iter().find(|s| s.lane.is_some()).map(|s| s.kind);
        let role = lane_kind.map_or(Role::Centre, Role::Lane);
        out.push((center.as_str(), Transform::IDENTITY, role));
        let pieces = def.edge_pieces(catalog.sockets());
        for e in 0..6u8 {
            let s = &def.edges[e as usize];
            if s.lane.is_none() || !matches!(s.kind, SocketKind::Road | SocketKind::River) {
                continue;
            }
            if let Some(piece) = pieces[e as usize] {
                out.push((
                    piece.as_str(),
                    Transform::from_rotation(piece_rotation(e)),
                    Role::Lane(s.kind),
                ));
            }
        }
    }
    for h in &def.props.hero {
        let t = Transform::from_xyz(h.at[0], 0.0, h.at[1])
            .with_rotation(Quat::from_rotation_y(-h.yaw_deg.to_radians()));
        let role = if h.piece.as_str().contains("tree") {
            Role::Tree
        } else {
            Role::Building
        };
        out.push((h.piece.as_str(), t, role));
    }
    out.retain(|(id, ..)| has(id));
    out
}

/// The tile's scatter instances whose piece is in the kit, at their tile-local transforms: trees
/// tinted as trees, rocks and wildflowers as ground props.
fn ghost_scatter<'a>(
    def: &'a TileDef,
    layout: &Layout,
    has: impl Fn(&str) -> bool,
) -> Vec<(&'a str, Transform, Role)> {
    layout
        .instances
        .iter()
        .filter_map(|i| {
            let piece = i.piece(def.props.scatter.get(i.rule as usize)?.piece.as_str());
            let role = if piece.contains("tree") {
                Role::Tree
            } else {
                Role::Prop
            };
            has(piece).then(|| (piece, i.transform(), role))
        })
        .collect()
}

/// Spawns, moves and fades the ghost meshes to match the view's ghost.
#[allow(clippy::too_many_arguments)]
pub fn sync(
    mut commands: Commands,
    time: Res<Time>,
    active: Res<ActiveScenario>,
    view: Res<PlayView>,
    (kit, packs): (Option<Res<PlayKit>>, Option<Res<PlayPacks>>),
    catalog: Res<PlayCatalog>,
    reduced: Res<ReducedMotion>,
    (mut ghost, mut drawn, mut report): (
        ResMut<GhostMeshes>,
        ResMut<GhostDrawn>,
        ResMut<GhostReport>,
    ),
    (mut materials, mut scatter): (ResMut<Assets<GhostMaterial>>, ResMut<PlayScatter>),
    (gltfs, gltf_meshes, mut meshes): (
        Res<Assets<Gltf>>,
        Res<Assets<GltfMesh>>,
        ResMut<Assets<Mesh>>,
    ),
) {
    let in_kit = |id: &str| matches!(kit.as_deref(), Some(PlayKit::Ready(k)) if k.contains_key(id));
    let want = view
        .ghost
        .as_ref()
        .filter(|_| active.0 == Scenario::Play)
        .and_then(|g| {
            let def = catalog.0.get(&g.tile)?;
            tile_ready(def, &catalog.0, kit.as_deref(), packs.as_deref()).then(|| Sig {
                cell: g.cell,
                tile: g.tile.clone(),
                rotation: g.rotation,
                world_seed: view.world_seed,
                scatter: def
                    .props
                    .scatter
                    .iter()
                    .map(|r| scatter::group(r.piece.as_str()).all(&in_kit))
                    .collect(),
            })
        });
    drawn.set_if_neq(GhostDrawn(want.is_some()));
    let ghost = ghost.as_mut();
    if ghost.sig != want {
        if let Some(root) = ghost.root.take() {
            commands.entity(root).try_despawn();
        }
        let restart = ghost.sig.as_ref().map(|s| s.cell) != want.as_ref().map(|w| w.cell);
        ghost.sig = want.clone();
        if restart {
            ghost.t = 0.0;
        }
        report.0.clear();
        if let (Some(sig), Some(PlayKit::Ready(kit)), Some(packs)) =
            (want, kit.as_deref(), packs.as_deref())
        {
            let def = catalog.0.get(&sig.tile).expect("checked above");
            let has = |id: &str| kit.contains_key(id);
            let seed = cell_seed(sig.world_seed, sig.cell);
            let assets = (&*gltfs, &*gltf_meshes, &*meshes);
            let layout = if def.props.scatter.is_empty() {
                None
            } else {
                scatter
                    .ground(def, &catalog.0, kit, &mut |id| {
                        scatter::kit_tris(id, packs, assets)
                    })
                    .map(|g| scatter::layout(def, seed, &g))
            };
            let mut pieces = ghost_pieces(def, &catalog.0, has);
            if let Some(l) = &layout {
                pieces.extend(ghost_scatter(def, l, has));
            }
            let water_kind = water::tiles::water_kind(def, 0);
            let water = water_kind.as_ref().and_then(|kind| {
                let mesh = ghost
                    .water
                    .entry(def.id.clone())
                    .or_insert_with(|| water_surface(def, &catalog.0).map(|m| meshes.add(m)))
                    .clone()?;
                let k = match kind {
                    WaterKind::River => SocketKind::River,
                    _ => SocketKind::Ocean,
                };
                Some((mesh, Role::Water(k)))
            });
            let water_surface = water.is_some();
            ghost.dirty = true;
            let (root, cost) = spawn(
                &mut commands,
                &mut materials,
                &mut ghost.materials,
                (kit, &*meshes),
                (&pieces, water_kind.is_some(), water),
                sig.cell,
                sig.rotation,
            );
            ghost.root = Some(root);
            let counts: Vec<String> = def
                .props
                .scatter
                .iter()
                .enumerate()
                .map(|(ri, r)| {
                    let n = layout.as_ref().map_or(0, |l| l.placed(ri));
                    let n = if sig.scatter[ri] { n } else { 0 };
                    format!("\"{}\":{n}", r.piece.as_str())
                })
                .collect();
            report.0 = format!(
                "{{\"q\":{},\"r\":{},\"tile\":\"{}\",\"rotation\":{},\"seed\":\"{seed:016x}\",\"layout\":{},\"scatter\":{{{}}},\"water_surface\":{water_surface},\"entities\":{},\"mesh_material_pairs\":{},\"tris\":{}}}",
                sig.cell.q,
                sig.cell.r,
                sig.tile,
                sig.rotation,
                layout.as_ref().map_or("null".to_owned(), |l| format!(
                    "\"{:016x}\"",
                    l.fingerprint()
                )),
                counts.join(","),
                cost.entities,
                cost.pairs,
                cost.tris,
            );
        }
    }
    if ghost.root.is_none() {
        return;
    }
    let before = ghost.t;
    ghost.t += time.delta_secs();
    let fade = |t: f32| {
        if reduced.0 {
            1.0
        } else {
            ((t - FADE_DELAY) / FADE_IN).clamp(0.0, 1.0)
        }
    };
    let (a, b) = (fade(before), fade(ghost.t));
    if a == b && !ghost.dirty {
        return;
    }
    ghost.dirty = false;
    for (_, handle, rgb) in &ghost.materials {
        if let Some(mut m) = materials.get_mut(handle) {
            m.color = rgb.extend(GHOST_ALPHA * super::affordances::ease(b));
        }
    }
}

/// What a spawned ghost costs to draw: its mesh entities (at most one draw each in the sorted
/// transparent phase), their distinct (mesh, material) pairs, and their LOD0 triangles.
#[derive(Default)]
struct Cost {
    entities: usize,
    pairs: usize,
    tris: usize,
}

/// Spawns the ghost root at `cell` and `rotation` with its pieces' LOD0 parts (tinted per slot on a
/// water tile) and its water surface.
#[allow(clippy::type_complexity)]
fn spawn(
    commands: &mut Commands,
    materials: &mut Assets<GhostMaterial>,
    cache: &mut Vec<(Role, Handle<GhostMaterial>, Vec3)>,
    (kit, meshes): (&HashMap<String, Vec<KitPart>>, &Assets<Mesh>),
    (pieces, slot_tint, water): (
        &[(&str, Transform, Role)],
        bool,
        Option<(Handle<Mesh>, Role)>,
    ),
    cell: Axial,
    rotation: u8,
) -> (Entity, Cost) {
    let [x, z] = cell.to_world(HEX_RADIUS);
    let root = commands
        .spawn((
            Transform::from_xyz(x, 0.0, z).with_rotation(piece_rotation(rotation)),
            Visibility::default(),
            SceneEntity,
        ))
        .id();
    let mut material = |role: Role| match cache.iter().find(|(r, ..)| *r == role) {
        Some((_, h, _)) => h.clone(),
        None => {
            let rgb = ghost_tint(role_rgb(role));
            let bias = if matches!(role, Role::Water(_)) {
                WATER_BIAS
            } else {
                0.0
            };
            let h = materials.add(GhostMaterial {
                color: rgb.extend(0.0),
                bias,
            });
            cache.push((role, h.clone(), rgb));
            h
        }
    };
    let mut cost = Cost::default();
    let mut pairs = HashSet::new();
    let parts = pieces.iter().flat_map(|&(piece, t, role)| {
        let ground = matches!(role, Role::Base(_) | Role::Lane(_) | Role::Centre);
        kit[piece].iter().filter(|p| p.lod == 0).map(move |p| {
            let role = slot_tint
                .then(|| slot_role(&p.slot))
                .flatten()
                .filter(|_| ground)
                .unwrap_or(role);
            (p.mesh.clone(), t * p.rotor.unwrap_or_default(), role)
        })
    });
    let water = water.map(|(mesh, role)| (mesh, Transform::IDENTITY, role));
    for (mesh, t, role) in parts.chain(water) {
        let material = material(role);
        cost.entities += 1;
        pairs.insert((mesh.id(), material.id()));
        cost.tris += meshes.get(&mesh).map_or(0, |m| {
            m.indices().map_or(m.count_vertices(), |i| i.len()) / 3
        });
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            t,
            NotShadowCaster,
            SceneEntity,
            ChildOf(root),
        ));
    }
    cost.pairs = pairs.len();
    (root, cost)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tile_ghosts_its_base_and_hero_props_from_the_catalog() {
        let c = PlayCatalog::default();
        for def in c.0.tiles() {
            let pieces = ghost_pieces(def, &c.0, |_| true);
            assert!(
                pieces.iter().any(|(id, ..)| *id == def.base.as_str()),
                "{} ghosts its base",
                def.id
            );
            for h in &def.props.hero {
                assert!(
                    pieces.iter().any(|(id, ..)| *id == h.piece.as_str()),
                    "{} ghosts hero {}",
                    def.id,
                    h.piece.as_str()
                );
            }
        }
    }

    #[test]
    fn pieces_missing_from_the_kit_are_left_out() {
        let c = PlayCatalog::default();
        let def = c.0.get("forest").expect("forest tile");
        let pieces = ghost_pieces(def, &c.0, |id| id == def.base.as_str());
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].0, def.base.as_str());
    }

    #[test]
    fn the_ghost_scatters_each_drawn_instance_at_its_own_transform() {
        let c = PlayCatalog::default();
        let def = c.0.get("forest").expect("forest tile");
        let inst = |rule: u8, x: f32| scatter::Instance {
            rule,
            pos: Vec3::new(x, 0.2, -x),
            yaw: x,
            scale: 1.1,
            order: 0,
            variant: 0,
        };
        let layout = Layout {
            instances: vec![inst(0, 1.0), inst(1, 2.0), inst(0, 3.0)],
            ..default()
        };
        let all = ghost_scatter(def, &layout, |_| true);
        assert_eq!(all.len(), 3);
        for ((piece, t, role), i) in all.iter().zip(&layout.instances) {
            let rule = &def.props.scatter[i.rule as usize];
            assert_eq!(*piece, rule.piece.as_str());
            assert_eq!(*t, i.transform());
            let tree = rule.piece.as_str().contains("tree");
            assert_eq!(*role, if tree { Role::Tree } else { Role::Prop });
        }
        let first = def.props.scatter[0].piece.as_str();
        let only_first = ghost_scatter(def, &layout, |id| id == first);
        assert_eq!(
            only_first.len(),
            2,
            "rules whose piece is missing are left out"
        );
    }

    #[test]
    fn water_tile_slots_tint_by_their_ground() {
        assert_eq!(slot_role("beach_grass"), Some(Role::Grass));
        assert_eq!(slot_role("beach_dunegrass"), Some(Role::Grass));
        assert_eq!(slot_role("beach_sand"), Some(Role::Sand));
        assert_eq!(slot_role("beach_wet"), Some(Role::WetSand));
        assert_eq!(slot_role("beach_seabed"), Some(Role::Seabed));
        assert_eq!(slot_role("coast_seabed"), Some(Role::Seabed));
        assert_eq!(slot_role("river_bed"), Some(Role::Seabed));
        assert_eq!(slot_role("river_stone"), Some(Role::Prop));
        assert_eq!(slot_role("beach_skirt"), None);
        assert_eq!(slot_role("beach_driftwood"), None);
    }

    /// The surface covers the hex where the tile's water is: all of the ocean, the seaward side of
    /// each coast, the channel of a river, and nothing on a dry tile.
    #[test]
    fn the_water_surface_covers_where_the_water_goes() {
        let c = PlayCatalog::default();
        let area = |id: &str| {
            let def = c.0.get(id).expect(id);
            water_surface(def, &c.0).map(|m| {
                let p = m
                    .attribute(Mesh::ATTRIBUTE_POSITION)
                    .unwrap()
                    .as_float3()
                    .unwrap();
                let up = p.as_chunks::<3>().0.iter().all(|[a, b, c]| {
                    let (a, b, c) = (Vec3::from(*a), Vec3::from(*b), Vec3::from(*c));
                    (b - a).cross(c - a).y > 0.0
                });
                assert!(up, "{id}: every triangle faces up");
                assert!(
                    p.iter().all(|v| v[1] == water::tiles::WATER_Y),
                    "{id}: at the level"
                );
                p.as_chunks::<3>()
                    .0
                    .iter()
                    .map(|[a, b, c]| {
                        let (a, b, c) = (
                            Vec2::new(a[0], a[2]),
                            Vec2::new(b[0], b[2]),
                            Vec2::new(c[0], c[2]),
                        );
                        0.5 * (b - a).perp_dot(c - a).abs()
                    })
                    .sum::<f32>()
            })
        };
        let hex = 1.5 * 3f32.sqrt() * HEX_RADIUS * HEX_RADIUS;
        let ocean = area("ocean").expect("ocean has water");
        assert!(
            (ocean - hex).abs() < 1.0,
            "ocean covers the hex: {ocean} of {hex}"
        );
        let beach = area("beach").expect("beach has water");
        assert!(
            (beach - hex / 2.0).abs() < 1.0,
            "beach: half the hex, {beach}"
        );
        let bay = area("beach_inner").expect("bay has water");
        let headland = area("beach_outer").expect("headland has water");
        assert!(
            headland > beach && beach > bay,
            "headland {headland} > beach > bay {bay}"
        );
        let river = area("river_straight").expect("river has water");
        let lane = 8.0 * 2.0 * HEX_RADIUS * 0.866_025_4;
        assert!(
            (river - lane).abs() < 0.1 * lane,
            "river: the 8 m lane, {river} of {lane}"
        );
        assert!(area("meadow").is_none() && area("forest").is_none());
    }

    #[test]
    fn the_tint_desaturates_toward_ink() {
        let grass = ghost_tint(socket_rgb(SocketKind::Grass));
        let raw = Color::srgb_u8(156, 196, 100).to_linear();
        let spread = |v: Vec3| v.max_element() - v.min_element();
        assert!(spread(grass) < raw.green - raw.blue);
    }
}
