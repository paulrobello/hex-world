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
//! Fades: the meshes fade in and out through the uniform's opacity alone (blended, as the outline
//! is), so a fade adds no binding and no pipeline and sorts in the transparent phase as the steady
//! ghost does. A ghost that leaves its cell (the pointer moves off the slot or to another one) keeps
//! its root and its own materials while it fades out over [`GHOST_OUT`], the outline's fade; the
//! new ghost gets fresh materials. A placement removes the ghost at once, as the rise takes over.
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
use hex_core::{
    Axial, Biome, EdgeStatus, HEX_RADIUS, LEVEL_STEP, SocketKind, TileCatalog, TileDef, cell_seed,
    rail_end::{self, BUFFER_STOP},
};

use super::{
    KitPart, PlayCatalog, PlayKit, PlayTile, PlayView,
    affordances::socket_rgb,
    packs::{PackState, PlayPacks},
    rise::ReducedMotion,
    scatter::{self, Layout, PlayScatter},
    slope_ground::{self, SlopeMeshes},
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
const FADE_DELAY: f32 = super::affordances::GHOST_DELAY;
const FADE_IN: f32 = super::affordances::T_FAST;
/// The meshes fade out when the ghost leaves its cell on the outline's fade (`T_FAST`).
const GHOST_OUT: f32 = super::affordances::GHOST_OUT;
/// R turns the ghost into its new rotation over the HUD's `--t` (`in-world.md`, Rotate: 180 to
/// 220 ms), on the Legend curve, in step with the sheet's socket codes and thumbnails.
pub const GHOST_TURN: f32 = super::affordances::T;
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
    /// The level it stands on (P4).
    level: u8,
    world_seed: u64,
    /// Per scatter rule: its piece is in the kit (scatter pieces do not gate [`tile_ready`]).
    scatter: Vec<bool>,
    /// The local rail edges whose end the ghost would leave open, one bit each ([`ghost_stops`]).
    stops: u8,
}

/// The spawned ghost: its root, what it shows and its fade clock.
#[derive(Resource, Default)]
pub struct GhostMeshes {
    root: Option<Entity>,
    sig: Option<Sig>,
    t: f32,
    /// The yaw the root shows, in edge steps (60° each), and the turn running toward a new rotation.
    shown: f32,
    turn: Option<Turn>,
    /// The parts were respawned: write the materials' opacity even when the fade holds still.
    dirty: bool,
    materials: Vec<(Role, Handle<GhostMaterial>, Vec3)>,
    /// Water surface meshes by tile id (`None`: the tile carries no water).
    water: HashMap<String, Option<Handle<Mesh>>>,
    /// Ghosts that left their cell, fading out with their own materials.
    leaving: Vec<Leaving>,
}

impl GhostMeshes {
    /// Whether the ghost is turning into a new rotation: the edge glow waits for the turn to land.
    pub fn turning(&self) -> bool {
        self.turn.is_some()
    }
}

/// A ghost root fading out after its cell was left, from opacity `from` over [`GHOST_OUT`].
struct Leaving {
    root: Entity,
    materials: Vec<Handle<GhostMaterial>>,
    from: f32,
    t: f32,
}

/// The opacity of a leaving ghost `t` seconds after it left at opacity `from`, on the Legend curve.
fn leave_alpha(from: f32, t: f32) -> f32 {
    from * (1.0 - super::affordances::ease(t / GHOST_OUT))
}

/// How a ghost root goes when the ghost changes: kept (`None`), faded out or cut at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Exit {
    Fade,
    Cut,
}

/// Whether the old ghost fades out: only when it leaves its cell (to no ghost or another cell),
/// is showing, and motion is on. A placement on its cell cuts it (the rise takes over in the same
/// frame), and a change on the same cell (a turn, another tile, a pin) rebuilds it in place.
fn exit(old: Axial, new: Option<Axial>, placed: bool, shown: f32, reduced: bool) -> Exit {
    if new == Some(old) || placed || reduced || shown <= 0.0 {
        Exit::Cut
    } else {
        Exit::Fade
    }
}

/// A ghost turn on R: from the yaw on screen to the new rotation's nearest equivalent (edge steps).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Turn {
    from: f32,
    to: f32,
    t: f32,
}

/// The equivalent of rotation `rot` (edge steps) nearest `from`, so a turn takes the short way
/// (a half turn goes clockwise).
fn nearest_steps(from: f32, rot: u8) -> f32 {
    let d = (f32::from(rot % 6) - from).rem_euclid(6.0);
    from + if d > 3.0 { d - 6.0 } else { d }
}

/// The yaw (edge steps) a turn shows `t` seconds in: eased over [`GHOST_TURN`], then held.
fn turn_steps(turn: Turn, t: f32) -> f32 {
    turn.from + (turn.to - turn.from) * super::affordances::ease(t / GHOST_TURN)
}

/// The root's rotation at a yaw of `steps` edge steps ([`piece_rotation`] for whole steps).
fn yaw(steps: f32) -> Quat {
    Quat::from_rotation_y(-(60.0 * steps).to_radians())
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
        level: 0,
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
/// the centre and its lane edge pieces, and the hero props (`play::tile_children`'s kit path), each
/// hero the piece its slot draws at `cell`, so the preview shows the placed tile's variant.
fn ghost_pieces<'a>(
    def: &'a TileDef,
    catalog: &'a TileCatalog,
    cell: Axial,
    has: impl Fn(&str) -> bool,
) -> Vec<(&'a str, Transform, Role)> {
    let mut out = vec![];
    let center_kit = crate::play::centre_ready(def, &has);
    if !water::tiles::stand_in_bed(def, &has) {
        out.push((
            def.base.as_str(),
            Transform::IDENTITY,
            Role::Base(def.biome),
        ));
    }
    if center_kit {
        let lane_kind = def.edges.iter().find(|s| s.lane.is_some()).map(|s| s.kind);
        if let Some(center) = &def.center {
            let role = lane_kind.map_or(Role::Centre, Role::Lane);
            out.push((center.as_str(), Transform::IDENTITY, role));
        }
        let pieces = def.edge_pieces(catalog.sockets());
        for e in 0..6u8 {
            if !def.draws_edge_piece(e as usize) {
                continue;
            }
            // A piece listed on a lane-less edge carries the tile's lane on (the tunnel portal).
            let s = &def.edges[e as usize];
            let kind = match s.lane {
                Some(_) => s.kind,
                None => lane_kind.unwrap_or(s.kind),
            };
            if let Some(piece) = pieces[e as usize] {
                out.push((
                    piece.as_str(),
                    Transform::from_rotation(piece_rotation(e)),
                    Role::Lane(kind),
                ));
            }
        }
    }
    for (h, piece) in def.hero_pieces(cell) {
        let t = Transform::from_xyz(h.at[0], h.y, h.at[1])
            .with_rotation(Quat::from_rotation_y(-h.yaw_deg.to_radians()));
        let role = if piece.contains("tree") {
            Role::Tree
        } else {
            Role::Building
        };
        out.push((piece, t, role));
    }
    out.retain(|(id, ..)| has(id));
    out
}

/// A sloped tile's ghost (P4) draws its runtime ground for its base and its ramp's road pieces
/// draped, as `tile_children` does; a raised ghost stands on its cliff column. `pieces` loses the
/// entries `extra` takes over.
fn runtime_ground(
    def: &TileDef,
    catalog: &TileCatalog,
    (kit, slope, meshes): (
        &HashMap<String, Vec<KitPart>>,
        &mut SlopeMeshes,
        &mut Assets<Mesh>,
    ),
    level: u8,
    pieces: &mut Vec<(&str, Transform, Role)>,
    extra: &mut Vec<(Handle<Mesh>, Transform, Role)>,
) {
    let base = Role::Base(def.biome);
    if def.is_sloped() && kit.contains_key(slope_ground::GROUND_PIECE) {
        pieces.retain(|(id, _, role)| !(*id == def.base.as_str() && *role == base));
        let (cap, skirt) = slope.ground(def, meshes);
        extra.push((cap, Transform::IDENTITY, base));
        extra.push((skirt, Transform::IDENTITY, base));
    }
    if level > 0 {
        extra.push((slope.column(level, meshes), Transform::IDENTITY, base));
    }
    let draped = slope_ground::draped_pieces(def, catalog);
    for (id, t, role) in pieces.iter().filter(|(id, ..)| draped.contains(id)) {
        let k = (0..6u8)
            .find(|&k| piece_rotation(k).angle_between(t.rotation) < 1e-3)
            .unwrap_or(0);
        for part in kit[*id].iter().filter(|p| p.lod == 0 && p.rotor.is_none()) {
            extra.push((slope.draped(&part.mesh, def, k, meshes), *t, *role));
        }
    }
    pieces.retain(|(id, ..)| !draped.contains(id));
}

/// The local rail edges of `def` at `rotation` whose world edge (status `edges`) has no rail tile
/// across it: the open ends the placed tile would have, one bit per local edge.
fn open_mask(def: &TileDef, rotation: u8, edges: &[EdgeStatus; 6]) -> u8 {
    (0..6u8)
        .filter(|&e| {
            rail_end::rail_edge(def, e)
                && edges[((e + rotation) % 6) as usize] != EdgeStatus::Compatible
        })
        .fold(0, |m, e| m | (1 << e))
}

/// The buffer stops at the ghost's open rail ends (owner D2-Q3), at their tile-local transforms.
fn ghost_stops(
    def: &TileDef,
    open: u8,
    has: impl Fn(&str) -> bool,
) -> Vec<(&'static str, Transform, Role)> {
    if !has(BUFFER_STOP) {
        return vec![];
    }
    rail_end::buffer_stops(def)
        .iter()
        .filter(|s| open & (1 << s.edge) != 0)
        .map(|s| {
            (
                BUFFER_STOP,
                crate::play::buffer_stop_local(s),
                Role::Building,
            )
        })
        .collect()
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
    (mut materials, mut scatter, mut slope): (
        ResMut<Assets<GhostMaterial>>,
        ResMut<PlayScatter>,
        Option<ResMut<SlopeMeshes>>,
    ),
    (gltfs, gltf_meshes, mut meshes): (
        Res<Assets<Gltf>>,
        Res<Assets<GltfMesh>>,
        ResMut<Assets<Mesh>>,
    ),
    mut roots: Query<&mut Transform>,
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
                level: g.level,
                world_seed: view.world_seed,
                scatter: def
                    .props
                    .scatter
                    .iter()
                    .map(|r| scatter::group(r.piece.as_str()).all(&in_kit))
                    .collect(),
                stops: open_mask(def, g.rotation, &g.edges),
            })
        });
    drawn.set_if_neq(GhostDrawn(want.is_some()));
    let ghost = ghost.as_mut();
    let fade = |t: f32| {
        if reduced.0 {
            1.0
        } else {
            ((t - FADE_DELAY) / FADE_IN).clamp(0.0, 1.0)
        }
    };
    if ghost.sig != want {
        if let (Some(root), Some(old)) = (ghost.root.take(), ghost.sig.as_ref()) {
            let placed = view.tiles.iter().any(|t| t.cell == old.cell);
            let shown = GHOST_ALPHA * super::affordances::ease(fade(ghost.t));
            let new = want.as_ref().map(|w| w.cell);
            if exit(old.cell, new, placed, shown, reduced.0) == Exit::Fade {
                ghost.leaving.push(Leaving {
                    root,
                    materials: std::mem::take(&mut ghost.materials)
                        .into_iter()
                        .map(|(_, h, _)| h)
                        .collect(),
                    from: shown,
                    t: 0.0,
                });
            } else {
                commands.entity(root).try_despawn();
            }
        }
        let restart = ghost.sig.as_ref().map(|s| s.cell) != want.as_ref().map(|w| w.cell);
        // Only R (the same tile turned on the same cell) turns; anything else shows the rotation at once.
        let turned = match (ghost.sig.as_ref(), want.as_ref()) {
            (Some(s), Some(w)) => !restart && s.tile == w.tile && s.rotation != w.rotation,
            _ => false,
        };
        let shown = match ghost.turn {
            Some(t) => turn_steps(t, t.t),
            None => ghost.shown,
        };
        ghost.turn = None;
        ghost.shown = want.as_ref().map_or(0.0, |w| f32::from(w.rotation));
        if let Some(w) = want.as_ref().filter(|_| turned && !reduced.0) {
            let to = nearest_steps(shown, w.rotation);
            ghost.turn = Some(Turn {
                from: shown,
                to,
                t: 0.0,
            });
            ghost.shown = to;
        }
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
                        scatter::ground_tris(id, packs, assets)
                    })
                    .map(|g| scatter::layout(def, seed, &g))
            };
            let mut pieces = ghost_pieces(def, &catalog.0, sig.cell, has);
            pieces.extend(ghost_stops(def, sig.stops, has));
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
            let mut extra: Vec<(Handle<Mesh>, Transform, Role)> = water
                .map(|(mesh, role)| (mesh, Transform::IDENTITY, role))
                .into_iter()
                .collect();
            if let Some(slope) = slope.as_deref_mut() {
                runtime_ground(
                    def,
                    &catalog.0,
                    (kit, slope, &mut meshes),
                    sig.level,
                    &mut pieces,
                    &mut extra,
                );
            }
            ghost.dirty = true;
            let (root, cost) = spawn(
                &mut commands,
                &mut materials,
                &mut ghost.materials,
                (kit, &*meshes),
                (&pieces, water_kind.is_some(), extra),
                sig.cell,
                (
                    ghost
                        .turn
                        .map_or_else(|| piece_rotation(sig.rotation), |t| yaw(t.from)),
                    sig.level,
                ),
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
    ghost.leaving.retain_mut(|l| {
        l.t += time.delta_secs();
        if reduced.0 || l.t >= GHOST_OUT {
            commands.entity(l.root).try_despawn();
            return false;
        }
        let a = leave_alpha(l.from, l.t);
        for h in &l.materials {
            if let Some(mut m) = materials.get_mut(h) {
                m.color.w = a;
            }
        }
        true
    });
    let Some(root) = ghost.root else {
        return;
    };
    if let Some(turn) = ghost.turn.as_mut() {
        turn.t += time.delta_secs();
        let (t, done) = (*turn, turn.t >= GHOST_TURN || reduced.0);
        if done {
            ghost.turn = None;
        }
        if let Ok(mut tf) = roots.get_mut(root) {
            tf.rotation = if done {
                yaw(t.to)
            } else {
                yaw(turn_steps(t, t.t))
            };
        }
    }
    let before = ghost.t;
    ghost.t += time.delta_secs();
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
    (pieces, slot_tint, extra): (
        &[(&str, Transform, Role)],
        bool,
        Vec<(Handle<Mesh>, Transform, Role)>,
    ),
    cell: Axial,
    (rotation, level): (Quat, u8),
) -> (Entity, Cost) {
    let [x, z] = cell.to_world(HEX_RADIUS);
    let root = commands
        .spawn((
            Transform::from_xyz(x, f32::from(level) * LEVEL_STEP, z).with_rotation(rotation),
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
    for (mesh, t, role) in parts.chain(extra) {
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
    fn the_ghost_turns_the_short_way_over_the_hud_t_on_the_legend_curve() {
        assert_eq!(GHOST_TURN, 0.18, "the HUD's --t");
        assert_eq!(nearest_steps(0.0, 1), 1.0);
        assert_eq!(nearest_steps(5.0, 0), 6.0, "5 to 0 turns one step on");
        assert_eq!(nearest_steps(0.0, 5), -1.0, "0 to 5 turns one step back");
        assert_eq!(nearest_steps(0.0, 3), 3.0, "a half turn goes clockwise");
        assert_eq!(
            nearest_steps(6.4, 2),
            8.0,
            "from mid-turn to the nearest equivalent"
        );
        let turn = Turn {
            from: 0.0,
            to: 1.0,
            t: 0.0,
        };
        assert_eq!(turn_steps(turn, 0.0), 0.0);
        let mid = turn_steps(turn, GHOST_TURN / 2.0);
        assert!(mid > 0.75 && mid < 1.0, "front-loaded like the HUD: {mid}");
        assert_eq!(turn_steps(turn, GHOST_TURN), 1.0);
        assert_eq!(turn_steps(turn, 1.0), 1.0, "and held");
        assert!(yaw(1.0).angle_between(piece_rotation(1)) < 1e-5);
        assert!(yaw(-1.0).angle_between(piece_rotation(5)) < 1e-5);
    }

    /// Owner A14 Q3: when the ghost leaves its cell its meshes fade out on the outline's timing
    /// (`T_FAST`, the Legend curve) instead of vanishing; a placement, a change on the same cell
    /// and reduced motion still cut.
    #[test]
    fn the_ghost_meshes_fade_out_with_the_outline() {
        assert_eq!(GHOST_OUT, super::super::affordances::T_FAST);
        let a = GHOST_ALPHA;
        assert_eq!(leave_alpha(a, 0.0), a);
        let mid = leave_alpha(a, GHOST_OUT / 2.0);
        assert!(
            mid > 0.0 && mid < a * 0.25,
            "front-loaded like the outline: {mid}"
        );
        assert_eq!(leave_alpha(a, GHOST_OUT), 0.0);
        let (c, d) = (Axial::ORIGIN, Axial::new(1, 0));
        assert_eq!(exit(c, None, false, a, false), Exit::Fade, "pointer off");
        assert_eq!(
            exit(c, Some(d), false, a, false),
            Exit::Fade,
            "to another slot"
        );
        assert_eq!(
            exit(c, None, true, a, false),
            Exit::Cut,
            "placed: the rise takes over"
        );
        assert_eq!(
            exit(c, Some(c), false, a, false),
            Exit::Cut,
            "same cell: rebuilt"
        );
        assert_eq!(exit(c, None, false, a, true), Exit::Cut, "reduced motion");
        assert_eq!(exit(c, None, false, 0.0, false), Exit::Cut, "never showed");
    }

    #[test]
    fn every_tile_ghosts_its_base_and_hero_props_from_the_catalog() {
        let c = PlayCatalog::default();
        for def in c.0.tiles() {
            let pieces = ghost_pieces(def, &c.0, Axial::ORIGIN, |_| true);
            assert!(
                pieces.iter().any(|(id, ..)| *id == def.base.as_str()),
                "{} ghosts its base",
                def.id
            );
            // Each hero slot ghosts the piece it draws at the cell: the authored one or a variant.
            for (i, h) in def.props.hero.iter().enumerate() {
                assert!(
                    pieces
                        .iter()
                        .any(|(id, ..)| def.hero_options(i).any(|o| o == *id)),
                    "{} ghosts hero {}",
                    def.id,
                    h.piece.as_str()
                );
            }
        }
    }

    /// The ghost previews the hero variant the placed tile draws at that cell (`hero_piece`):
    /// house 1 of a cluster at (1, 0) is house_cottage_b, at (-3, 0) the longhouse, at the origin
    /// the cottage.
    #[test]
    fn the_ghost_shows_the_variant_the_cell_draws() {
        let c = PlayCatalog::default();
        let def = c.0.get("house_cluster").unwrap();
        for cell in [
            Axial::ORIGIN,
            Axial::new(1, 0),
            Axial::new(2, -1),
            Axial::new(-3, 2),
        ] {
            let heroes: Vec<&str> = ghost_pieces(def, &c.0, cell, |_| true)
                .into_iter()
                .filter(|(_, _, r)| *r == Role::Building)
                .map(|(id, ..)| id)
                .collect();
            let drawn: Vec<&str> = def.hero_pieces(cell).map(|(_, p)| p).collect();
            assert_eq!(heroes, drawn, "{cell:?}");
        }
        let at = |q, r| ghost_pieces(def, &c.0, Axial::new(q, r), |_| true);
        assert!(at(1, 0).iter().any(|(id, ..)| *id == "house_cottage_b"));
        assert!(at(-3, 0).iter().any(|(id, ..)| *id == "house_longhouse"));
        for v in ["house_longhouse", "house_cottage_b"] {
            assert!(!at(0, 0).iter().any(|(id, ..)| *id == v), "{v}");
        }
    }

    /// A hero prop with a height stands at it in the ghost, as on the placed tile: the station
    /// shelter sits 1.38 m up on its platform, not sunk into it.
    #[test]
    fn the_ghost_stands_hero_props_at_their_height() {
        let c = PlayCatalog::default();
        let def = c.0.get("rail_station").expect("rail_station tile");
        let pieces = ghost_pieces(def, &c.0, Axial::ORIGIN, |_| true);
        let lifted: Vec<_> = def.props.hero.iter().filter(|h| h.y != 0.0).collect();
        assert!(!lifted.is_empty(), "the station has a raised hero");
        for h in lifted {
            let (_, t, _) = pieces
                .iter()
                .find(|(id, ..)| *id == h.piece.as_str())
                .unwrap_or_else(|| panic!("{} ghosted", h.piece.as_str()));
            assert_eq!(
                t.translation,
                Vec3::new(h.at[0], h.y, h.at[1]),
                "{}",
                h.piece.as_str()
            );
        }
    }

    #[test]
    fn pieces_missing_from_the_kit_are_left_out() {
        let c = PlayCatalog::default();
        let def = c.0.get("forest").expect("forest tile");
        let pieces = ghost_pieces(def, &c.0, Axial::ORIGIN, |id| id == def.base.as_str());
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
        assert_eq!(slot_role("beach_dunegrass_foliage"), Some(Role::Grass));
        assert_eq!(slot_role("beach_dunegrass_sand"), Some(Role::Sand));
        assert_eq!(slot_role("beach_sand"), Some(Role::Sand));
        assert_eq!(slot_role("beach_wet"), Some(Role::WetSand));
        assert_eq!(slot_role("beach_seabed"), Some(Role::Seabed));
        assert_eq!(slot_role("coast_seabed"), Some(Role::Seabed));
        assert_eq!(slot_role("river_bed"), Some(Role::Seabed));
        assert_eq!(slot_role("river_stone"), Some(Role::Prop));
        assert_eq!(slot_role("beach_skirt"), None);
        assert_eq!(slot_role("beach_driftwood_wood"), None);
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

    /// The ghost draws a stop at each rail end it would leave open (owner D2-Q3), by world edge
    /// at its rotation: a rotated bend facing a placed rail tile on one end gets one stop.
    #[test]
    fn the_ghost_draws_stops_at_the_ends_it_would_leave_open() {
        let c = TileCatalog::builtin().unwrap();
        let bend = c.get("rail_bend_60").unwrap();
        // Rotation 2: local edges 0 and 1 face world edges 2 and 3.
        let mut edges = [EdgeStatus::Neutral; 6];
        assert_eq!(open_mask(bend, 2, &edges), 0b11);
        edges[3] = EdgeStatus::Compatible;
        assert_eq!(open_mask(bend, 2, &edges), 0b01);
        edges[2] = EdgeStatus::Compatible;
        assert_eq!(open_mask(bend, 2, &edges), 0);
        let stops = ghost_stops(bend, 0b01, |_| true);
        assert_eq!(stops.len(), 1);
        let t = stops[0].1.translation;
        assert!(
            (t.x - 10.467).abs() < 0.02 && (t.z - 0.753).abs() < 0.02,
            "{t}"
        );
        assert!(ghost_stops(bend, 0b11, |id| id != BUFFER_STOP).is_empty());
        let meadow = c.tiles().iter().find(|t| t.networks.is_empty()).unwrap();
        assert_eq!(open_mask(meadow, 0, &[EdgeStatus::Neutral; 6]), 0);
    }
}
