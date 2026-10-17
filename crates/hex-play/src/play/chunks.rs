//! Settled kit parts share chunk meshes. Tile roots and scatter layout records remain authoritative.
use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use hex_core::{Axial, HEX_RADIUS};
use hex_render::{
    chunks::chunk_center,
    kit_scene::KitRotor,
    scene::{Lod, lod_range},
};

use super::{KitMaterial, PlaySpawned, PlayView, RisePart, SceneEntity, rise::PlayRises};

#[derive(Clone)]
struct Part {
    entity: Entity,
    parent: Entity,
    mesh: Handle<Mesh>,
    material: Handle<KitMaterial>,
    local: Transform,
    world: Transform,
    rise: Option<RisePart>,
    lod: u8,
    scatter: bool,
}

struct Tile {
    cell: Axial,
    parts: Vec<Part>,
}

#[derive(Resource, Default)]
pub(super) struct PlayChunks {
    tiles: HashMap<Entity, Tile>,
    draws: HashMap<Axial, Vec<Entity>>,
    dirty: HashSet<Axial>,
    scatter_draws: HashMap<Axial, usize>,
    pending: bool,
}

#[derive(Component)]
pub(super) struct ChunkDraw;

/// Restore the original hierarchy before the normal tile sync starts a sink or replaces children.
pub(super) fn restore(
    mut commands: Commands,
    view: Res<PlayView>,
    spawned: Option<ResMut<PlaySpawned>>,
    mut chunks: ResMut<PlayChunks>,
    parents: Query<(), With<Transform>>,
) {
    let Some(mut spawned) = spawned else { return };
    let refresh = !spawned.refresh.is_empty() || spawned.rebuild || spawned.epoch != view.epoch;
    if !refresh && !view.is_changed() {
        return;
    }
    let want: HashMap<_, _> = view.tiles.iter().map(|t| (t.cell, t)).collect();
    let old: HashMap<_, _> = spawned.tiles.iter().map(|(c, (t, _))| (*c, t)).collect();
    let changed: Vec<_> = old
        .keys()
        .chain(want.keys())
        .copied()
        .filter(|c| old.get(c) != want.get(c))
        .collect();
    let roots: Vec<_> = chunks
        .tiles
        .iter()
        .filter(|(root, tile)| {
            refresh
                || !parents.contains(**root)
                || changed.iter().any(|c| c.distance(tile.cell) <= 1)
        })
        .map(|(root, _)| *root)
        .collect();
    for root in roots {
        let tile = chunks.tiles.remove(&root).expect("cached tile");
        chunks.dirty.insert(chunk_center(tile.cell, 19));
        for part in tile.parts {
            if !parents.contains(part.parent) {
                continue;
            }
            let mut e = commands.spawn((
                Mesh3d(part.mesh),
                MeshMaterial3d(part.material),
                part.local,
                Lod(part.lod),
                lod_range(part.lod as usize),
                SceneEntity,
                ChildOf(part.parent),
            ));
            if let Some(rise) = part.rise {
                e.insert(rise);
            }
            let restored = e.id();
            if let Some(ids) = spawned
                .bypass_change_detection()
                .pair_entities
                .get_mut(&tile.cell)
            {
                for id in ids.iter_mut().filter(|id| **id == part.entity) {
                    *id = restored;
                }
            }
        }
    }
}

/// Runs after rise posing, using local hierarchy transforms rather than last frame's globals.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn settle(
    mut commands: Commands,
    spawned: Option<Res<PlaySpawned>>,
    rises: Res<PlayRises>,
    mut chunks: ResMut<PlayChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
    config: Res<hex_render::GateConfig>,
    mut report: ResMut<super::scatter::ScatterReport>,
    scatter_groups: Query<(), With<super::scatter::ScatterGroup>>,
    parts: Query<
        (
            Entity,
            &Mesh3d,
            &MeshMaterial3d<KitMaterial>,
            &Transform,
            Option<&RisePart>,
            &Lod,
        ),
        (Without<ChunkDraw>, Without<KitRotor>),
    >,
    hierarchy: Query<(&Transform, Option<&ChildOf>)>,
    children: Query<&Children>,
    rotors: Query<(), With<KitRotor>>,
) {
    let Some(spawned) = spawned else { return };
    if config.kit_hooks == hex_render::KitHooks::Autumn {
        return;
    }
    if spawned.is_changed() || rises.is_changed() {
        chunks.pending = true;
    }
    // Continuous placements fast-forward earlier rises. Keep their meshes individual until the
    // wave ends so each chunk merges once rather than once per placement.
    let collect = chunks.pending && rises.rising().next().is_none();
    if !collect && chunks.dirty.is_empty() {
        if report.is_changed() {
            update_report(&chunks, &mut report);
        }
        return;
    }
    let mut candidates = vec![];
    if collect {
        chunks.pending = false;
        for (cell, (_, root)) in &spawned.tiles {
            if spawned.hidden.contains_key(cell) || !rises.settled_cell(*cell) {
                continue;
            }
            let Ok((transform, _)) = hierarchy.get(*root) else {
                continue;
            };
            let mut stack = vec![(*root, *transform)];
            while let Some((parent, world)) = stack.pop() {
                let Ok(descendants) = children.get(parent) else {
                    continue;
                };
                for entity in descendants.iter() {
                    let Ok((local, _)) = hierarchy.get(entity) else {
                        continue;
                    };
                    let world = world * *local;
                    candidates.push((entity, parent, *root, *cell, world));
                    stack.push((entity, world));
                }
            }
        }
    }
    for (entity, parent, root, cell, world) in candidates {
        let Ok((_, mesh, material, local, rise, lod)) = parts.get(entity) else {
            continue;
        };
        if children
            .get(entity)
            .is_ok_and(|cs| cs.iter().any(|e| rotors.contains(e)))
        {
            continue;
        }
        let Some(source) = meshes.get(&mesh.0) else {
            continue;
        };
        // Kit source vertex colours must not be overwritten by the packed wind-origin channel.
        if source.attribute(Mesh::ATTRIBUTE_COLOR).is_some() {
            continue;
        }
        chunks
            .tiles
            .entry(root)
            .or_insert_with(|| Tile {
                cell,
                parts: vec![],
            })
            .parts
            .push(Part {
                entity,
                parent,
                mesh: mesh.0.clone(),
                material: material.0.clone(),
                local: *local,
                world,
                rise: rise.copied(),
                lod: lod.0,
                scatter: scatter_groups.contains(parent),
            });
        chunks.dirty.insert(chunk_center(cell, 19));
        commands.entity(entity).try_despawn();
    }
    let dirty = std::mem::take(&mut chunks.dirty);
    for center in dirty {
        for e in chunks.draws.remove(&center).unwrap_or_default() {
            commands.entity(e).try_despawn();
        }
        let [x, z] = center.to_world(HEX_RADIUS);
        let origin = Vec3::new(x, 0.0, z);
        let mut merged: HashMap<(AssetId<KitMaterial>, u8), (Handle<KitMaterial>, Mesh)> =
            HashMap::new();
        let mut scatter_keys = HashSet::new();
        for part in chunks
            .tiles
            .values()
            .filter(|t| chunk_center(t.cell, 19) == center)
            .flat_map(|t| &t.parts)
        {
            let Some(source) = meshes.get(&part.mesh) else {
                continue;
            };
            let mesh = hex_render::chunks::transformed_kit_mesh(source, part.world, origin);
            let key = (part.material.id(), part.lod);
            if part.scatter {
                scatter_keys.insert(key);
            }
            if let Some((_, target)) = merged.get_mut(&key) {
                target
                    .merge(&mesh)
                    .expect("kit parts of one material have compatible vertex layouts");
            } else {
                merged.insert(key, (part.material.clone(), mesh));
            }
        }
        let mut draws = vec![];
        for ((_, lod), (material, mut mesh)) in merged {
            mesh.asset_usage = bevy::asset::RenderAssetUsages::RENDER_WORLD;
            draws.push(
                commands
                    .spawn((
                        ChunkDraw,
                        Mesh3d(meshes.add(mesh)),
                        MeshMaterial3d(material),
                        Transform::from_translation(origin),
                        Lod(lod),
                        lod_range(lod as usize),
                        SceneEntity,
                    ))
                    .id(),
            );
        }
        chunks.draws.insert(center, draws);
        chunks.scatter_draws.insert(center, scatter_keys.len());
    }
    update_report(&chunks, &mut report);
}

fn update_report(chunks: &PlayChunks, report: &mut super::scatter::ScatterReport) {
    if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&report.0) {
        let unbatched = value["unbatched_entities"]
            .as_u64()
            .or_else(|| value["entities"].as_u64())
            .unwrap_or(0);
        let merged_parts = chunks
            .tiles
            .values()
            .flat_map(|t| &t.parts)
            .filter(|p| p.scatter)
            .count() as u64;
        let draws = chunks.scatter_draws.values().sum::<usize>() as u64;
        value["unbatched_entities"] = unbatched.into();
        value["entities"] = (unbatched.saturating_sub(merged_parts) + draws).into();
        let json = value.to_string();
        if report.0 != json {
            report.0 = json;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{PlayTile, RiseRole};
    use super::*;

    fn app() -> (App, Entity, Entity) {
        let mut app = App::new();
        app.init_resource::<PlayChunks>()
            .init_resource::<PlayView>()
            .init_resource::<PlaySpawned>()
            .init_resource::<PlayRises>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<hex_render::GateConfig>()
            .init_resource::<super::super::scatter::ScatterReport>();
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(1.0, 2.0, 3.0));
        let root = app
            .world_mut()
            .spawn((Transform::from_xyz(12.0, 0.0, 3.0), Visibility::Inherited))
            .id();
        let mut first = Entity::PLACEHOLDER;
        for i in 0..50 {
            let local = Transform::from_xyz(i as f32, 0.0, 0.0);
            let id = app
                .world_mut()
                .spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d::<KitMaterial>(Handle::default()),
                    local,
                    Lod(0),
                    RisePart::new(Axial::ORIGIN, RiseRole::Edge, local),
                    ChildOf(root),
                ))
                .id();
            if i == 0 {
                first = id;
            }
        }
        let tile = PlayTile {
            cell: Axial::ORIGIN,
            tile: "meadow".into(),
            rotation: 0,
        };
        app.world_mut()
            .resource_mut::<PlayView>()
            .tiles
            .push(tile.clone());
        app.world_mut()
            .resource_mut::<PlaySpawned>()
            .tiles
            .insert(Axial::ORIGIN, (tile, root));
        app.world_mut()
            .resource_mut::<PlaySpawned>()
            .pair_entities
            .insert(Axial::ORIGIN, vec![first]);
        app.world_mut()
            .resource_mut::<PlayRises>()
            .rest(Axial::ORIGIN);
        (app, root, first)
    }

    #[test]
    fn settled_parts_merge_and_restore_before_removal_with_pair_ids_repaired() {
        let (mut app, root, old_pair) = app();
        app.add_systems(Update, settle);
        app.update();
        assert_eq!(
            app.world_mut().query::<&Mesh3d>().iter(app.world()).count(),
            1
        );
        let chunks = app.world().resource::<PlayChunks>();
        assert_eq!(chunks.tiles[&root].parts.len(), 50);
        assert_eq!(chunks.draws.len(), 1);
        app.world_mut().resource_mut::<PlayView>().tiles.clear();
        app.add_systems(First, restore);
        app.update();
        // Normal sync removes the root between restoration and settlement in production.
        let pair = app.world().resource::<PlaySpawned>().pair_entities[&Axial::ORIGIN][0];
        assert_ne!(pair, old_pair);
        assert!(app.world().entities().contains(root));
    }

    #[test]
    fn settled_tiles_wait_for_the_rise_wave_and_merge_once() {
        let (mut app, root, _) = app();
        let other = Axial::new(1, 0);
        app.world_mut().resource_mut::<PlayRises>().begin(
            other,
            "meadow",
            &hex_core::RiseTimeline::default(),
            false,
            &mut vec![],
        );
        app.add_systems(Update, settle);
        for _ in 0..4 {
            app.update();
            assert!(app.world().resource::<PlayChunks>().draws.is_empty());
        }
        app.world_mut().resource_mut::<PlayRises>().rest(other);
        app.update();
        let draw = app.world().resource::<PlayChunks>().draws[&Axial::ORIGIN][0];
        assert_eq!(
            app.world().resource::<PlayChunks>().tiles[&root]
                .parts
                .len(),
            50
        );
        for _ in 0..4 {
            app.update();
            assert_eq!(
                app.world().resource::<PlayChunks>().draws[&Axial::ORIGIN][0],
                draw
            );
        }
    }

    #[test]
    fn unchanged_kit_does_not_restore_settled_parts() {
        let (mut app, root, _) = app();
        app.insert_resource(super::super::PlayKit::Ready(HashMap::new()));
        app.add_systems(First, restore);
        app.add_systems(Update, settle);
        app.update();
        app.world_mut()
            .resource_mut::<super::super::PlayKit>()
            .set_changed();
        app.update();
        assert_eq!(
            app.world().resource::<PlayChunks>().tiles[&root]
                .parts
                .len(),
            50
        );
        assert_eq!(
            app.world_mut().query::<&Mesh3d>().iter(app.world()).count(),
            1
        );
    }

    #[test]
    fn running_rise_keeps_original_mesh_entities() {
        let (mut app, _, _) = app();
        app.world_mut()
            .resource_mut::<PlayRises>()
            .undrawn(Axial::ORIGIN);
        app.add_systems(Update, settle);
        app.update();
        assert_eq!(
            app.world_mut().query::<&Mesh3d>().iter(app.world()).count(),
            50
        );
        assert!(app.world().resource::<PlayChunks>().draws.is_empty());
    }
}
