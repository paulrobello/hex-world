//! The ghost's see-through meshes (`design/ui/legend/in-world.md`, Ghost): the selected tile's kit
//! pieces at the ghost's cell and rotation, tinted by role and drawn at 45% with [`GhostMaterial`].
//!
//! Sampled-texture budget: [`GhostMaterial`] binds one uniform and no texture, so its pipeline
//! samples only the view's textures, fewer than the kit forward pipeline, which is exactly at
//! WebGPU's 16 beside refracting water. It stays out of the prepass and the shadow passes.
//!
//! While a piece the tile draws is in a pack still streaming there is no mesh to show: the
//! affordances keep the outline, the edge colours and the lane strokes ([`GhostDrawn`]).
use bevy::{
    light::NotShadowCaster, prelude::*, render::render_resource::AsBindGroup, shader::ShaderRef,
};
use hex_core::{Axial, Biome, HEX_RADIUS, SocketKind, TileCatalog, TileDef};

use super::{
    KitPart, PlayCatalog, PlayKit, PlayView,
    affordances::socket_rgb,
    packs::{PackState, PlayPacks},
    rise::ReducedMotion,
};
use crate::{ActiveScenario, Scenario, SceneEntity, kit_scene::piece_rotation, water};

pub const GHOST_SHADER: &str = "shaders/ghost.wesl";

/// Opacity of the ghost meshes.
const GHOST_ALPHA: f32 = 0.45;
/// Fade in after the slot is hovered, as the ghost outline does (s).
const FADE_DELAY: f32 = 0.15;
const FADE_IN: f32 = 0.12;
const INK: [u8; 3] = [238, 243, 246];
const BUILDING: [u8; 3] = [222, 210, 190];

/// One colour, no texture: lit by a fixed key light in `ghost.wesl`.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct GhostMaterial {
    /// Linear rgb and opacity.
    #[uniform(100)]
    pub color: Vec4,
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
}

/// Whether the current ghost draws its tile's meshes; the affordances drop the lane strokes then.
#[derive(Resource, Default, Debug, PartialEq, Eq)]
pub struct GhostDrawn(pub bool);

/// The tint roles of a ghost's parts.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Role {
    Base(Biome),
    Lane(SocketKind),
    Centre,
    Tree,
    Building,
}

/// The spawned ghost: its root, what it shows and its fade clock.
#[derive(Resource, Default)]
pub struct GhostMeshes {
    root: Option<Entity>,
    sig: Option<(Axial, String, u8)>,
    t: f32,
    /// The parts were respawned: write the materials' opacity even when the fade holds still.
    dirty: bool,
    materials: Vec<(Role, Handle<GhostMaterial>, Vec3)>,
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
    }
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
    let water_bed = !center_kit && water::tiles::water_kind(def, 0).is_some();
    if !water_bed {
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
    mut ghost: ResMut<GhostMeshes>,
    mut drawn: ResMut<GhostDrawn>,
    mut materials: ResMut<Assets<GhostMaterial>>,
) {
    let want = view
        .ghost
        .as_ref()
        .filter(|_| active.0 == Scenario::Play)
        .and_then(|g| {
            let def = catalog.0.get(&g.tile)?;
            tile_ready(def, &catalog.0, kit.as_deref(), packs.as_deref())
                .then(|| (g.cell, g.tile.clone(), g.rotation))
        });
    drawn.set_if_neq(GhostDrawn(want.is_some()));
    let ghost = ghost.as_mut();
    if ghost.sig != want {
        if let Some(root) = ghost.root.take() {
            commands.entity(root).try_despawn();
        }
        let restart = ghost.sig.as_ref().map(|s| s.0) != want.as_ref().map(|w| w.0);
        ghost.sig = want.clone();
        if restart {
            ghost.t = 0.0;
        }
        if let (Some((cell, tile, rotation)), Some(PlayKit::Ready(kit))) = (want, kit.as_deref()) {
            let def = catalog.0.get(&tile).expect("checked above");
            ghost.dirty = true;
            ghost.root = Some(spawn(
                &mut commands,
                &mut materials,
                &mut ghost.materials,
                kit,
                &catalog.0,
                def,
                cell,
                rotation,
            ));
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

#[allow(clippy::too_many_arguments)]
fn spawn(
    commands: &mut Commands,
    materials: &mut Assets<GhostMaterial>,
    cache: &mut Vec<(Role, Handle<GhostMaterial>, Vec3)>,
    kit: &std::collections::HashMap<String, Vec<KitPart>>,
    catalog: &TileCatalog,
    def: &TileDef,
    cell: Axial,
    rotation: u8,
) -> Entity {
    let [x, z] = cell.to_world(HEX_RADIUS);
    let root = commands
        .spawn((
            Transform::from_xyz(x, 0.0, z).with_rotation(piece_rotation(rotation)),
            Visibility::default(),
            SceneEntity,
        ))
        .id();
    for (piece, t, role) in ghost_pieces(def, catalog, |id| kit.contains_key(id)) {
        let material = match cache.iter().find(|(r, ..)| *r == role) {
            Some((_, h, _)) => h.clone(),
            None => {
                let rgb = ghost_tint(role_rgb(role));
                let h = materials.add(GhostMaterial {
                    color: rgb.extend(0.0),
                });
                cache.push((role, h.clone(), rgb));
                h
            }
        };
        for part in kit[piece].iter().filter(|p| p.lod == 0) {
            commands.spawn((
                Mesh3d(part.mesh.clone()),
                MeshMaterial3d(material.clone()),
                t,
                NotShadowCaster,
                SceneEntity,
                ChildOf(root),
            ));
        }
    }
    root
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
    fn the_tint_desaturates_toward_ink() {
        let grass = ghost_tint(socket_rgb(SocketKind::Grass));
        let raw = Color::srgb_u8(156, 196, 100).to_linear();
        let spread = |v: Vec3| v.max_element() - v.min_element();
        assert!(spread(grass) < raw.green - raw.blue);
    }
}
