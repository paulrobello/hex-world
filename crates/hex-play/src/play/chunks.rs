//! Settled kit parts draw through GPU instance records. Tile roots and scatter layout records
//! remain authoritative.
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};

use bevy::camera::primitives::Aabb;
use bevy::mesh::Indices;
use bevy::prelude::*;
use hex_core::{Axial, HEX_RADIUS};
use hex_render::{
    chunks::chunk_center,
    kit_instances::{
        GpuKitInstance, KitChunk, KitGpuMeta, KitInstanceSet, KitInstanceSetData, KitSlot,
        KitSlotDraw,
    },
    kit_scene::KitRotor,
    scene::{Lod, lod_range},
};

use super::{KitMaterial, KitPart, PlaySpawned, PlayView, RisePart, SceneEntity, rise::PlayRises};

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
}

struct Tile {
    cell: Axial,
    parts: Vec<Part>,
}

#[derive(Resource, Default)]
pub(super) struct PlayChunks {
    tiles: HashMap<Entity, Tile>,
    dirty: HashSet<Axial>,
    pending: bool,
    /// The (slot, LOD) draw entities of the current instance set.
    slot_entities: Vec<Entity>,
}

/// Maps every kit part mesh to its (family, LOD): a family is one piece body primitive across
/// the three LODs, identified by (piece, material, ordinal within the LOD). Parts without an
/// entry stay on the per-entity path.
#[derive(Resource, Default)]
pub(super) struct KitFamilies {
    family_of: HashMap<AssetId<Mesh>, (u32, u8)>,
    meshes_of: HashMap<u32, [Option<Handle<Mesh>>; 3]>,
}

impl KitFamilies {
    #[allow(clippy::type_complexity)]
    pub(super) fn build(pieces: &HashMap<String, Vec<KitPart>>) -> Self {
        let mut out = Self::default();
        let mut intern: HashMap<(String, AssetId<KitMaterial>, u32), u32> = HashMap::new();
        let mut next: u32 = 0;
        let mut ids: Vec<_> = pieces.keys().cloned().collect();
        ids.sort();
        for id in ids {
            let mut per_lod_seen = [0u32; 3];
            for part in &pieces[&id] {
                if part.rotor.is_some() {
                    continue;
                }
                let ord = per_lod_seen[part.lod];
                per_lod_seen[part.lod] += 1;
                let fid = *intern
                    .entry((id.clone(), part.material.id(), ord))
                    .or_insert_with(|| {
                        let fid = next;
                        next += 1;
                        out.meshes_of.insert(fid, [None, None, None]);
                        fid
                    });
                out.family_of.insert(part.mesh.id(), (fid, part.lod as u8));
                out.meshes_of.get_mut(&fid).expect("inserted above")[part.lod] =
                    Some(part.mesh.clone());
            }
        }
        out
    }

    fn family_of(&self, mesh: AssetId<Mesh>) -> Option<(u32, u8)> {
        self.family_of.get(&mesh).copied()
    }

    fn meshes_of(&self, family: u32) -> [Option<Handle<Mesh>>; 3] {
        self.meshes_of
            .get(&family)
            .cloned()
            .unwrap_or([None, None, None])
    }
}
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
    meshes: Res<Assets<Mesh>>,
    families: Option<Res<KitFamilies>>,
    config: Res<hex_render::GateConfig>,
    mut report: ResMut<super::scatter::ScatterReport>,
    parts: Query<
        (
            Entity,
            &Mesh3d,
            &MeshMaterial3d<KitMaterial>,
            &Transform,
            Option<&RisePart>,
            &Lod,
        ),
        (Without<KitRotor>,),
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
        // Parts of a piece whose pack arrived without a full LOD family stay per-entity.
        let Some(families) = families.as_ref() else {
            continue;
        };
        if families.family_of(mesh.0.id()).is_none() {
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
            });
        chunks.dirty.insert(chunk_center(cell, 19));
        commands.entity(entity).try_despawn();
    }
    if !chunks.dirty.is_empty() {
        let families = families.as_ref().expect("families");
        rebuild(&mut commands, &mut chunks, &meshes, families);
    }
    update_report(&chunks, &mut report);
}

/// Rebuilds the whole instance set from the cached parts and respawns the draw-slot entities.
/// Settles are wave-batched, so this runs at most once per rise wave or tile change.
fn rebuild(
    commands: &mut Commands,
    chunks: &mut PlayChunks,
    meshes: &Assets<Mesh>,
    families: &KitFamilies,
) {
    chunks.dirty.clear();
    for e in chunks.slot_entities.drain(..) {
        commands.entity(e).try_despawn();
    }
    // Tiles grouped by chunk, ordered by chunk centre so the set is deterministic, and slot
    // instances ordered chunk-major so each chunk owns a contiguous per-slot range.
    let mut grouped: Vec<(Axial, Vec<&Tile>)> = vec![];
    let mut chunk_of: HashMap<Axial, usize> = HashMap::new();
    for tile in chunks.tiles.values() {
        let centre = chunk_center(tile.cell, 19);
        let ci = *chunk_of.entry(centre).or_insert_with(|| {
            grouped.push((centre, vec![]));
            grouped.len() - 1
        });
        grouped[ci].1.push(tile);
    }
    grouped.sort_by_key(|(centre, _)| (centre.q, centre.r));
    let mut slots: Vec<(u32, Handle<KitMaterial>, Vec<GpuKitInstance>)> = vec![];
    let mut slot_of: HashMap<(u32, AssetId<KitMaterial>), usize> = HashMap::new();
    let mut per_chunk: Vec<(Vec3, Aabb, Vec<Range<u32>>)> = vec![];
    let mut local_bounds: HashMap<AssetId<Mesh>, (Vec3, Vec3)> = HashMap::new();
    // One instance per placement: a family's three LOD parts share it, so the first LOD seen
    // represents it and its siblings are dropped (the LOD draw picks the mesh, not the record).
    let mut seen: HashSet<(u32, AssetId<KitMaterial>, Entity)> = HashSet::new();
    for (centre, tiles) in &grouped {
        let [x, z] = centre.to_world(HEX_RADIUS);
        let origin = Vec3::new(x, 0.0, z);
        let mut ranges = vec![0..0u32; slots.len()];
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for tile in tiles {
            for part in &tile.parts {
                let Some((fid, _lod)) = families.family_of(part.mesh.id()) else {
                    continue;
                };
                if !seen.insert((fid, part.material.id(), part.parent)) {
                    continue;
                }
                let (blo, bhi) = *local_bounds
                    .entry(part.mesh.id())
                    .or_insert_with(|| mesh_bounds(meshes.get(&part.mesh)));
                let (min, max) = bounds_of(&blo, &bhi, &part.world);
                lo = lo.min(min);
                hi = hi.max(max);
                let si = *slot_of.entry((fid, part.material.id())).or_insert_with(|| {
                    slots.push((fid, part.material.clone(), vec![]));
                    ranges.resize(slots.len(), 0..0);
                    slots.len() - 1
                });
                let start = slots[si].2.len() as u32;
                slots[si].2.push(GpuKitInstance {
                    rotation: part.world.rotation.to_array(),
                    translation: (part.world.translation - origin).to_array(),
                    tag: 0,
                    scale: part.world.scale.to_array(),
                    _pad: 0,
                });
                if ranges[si].is_empty() {
                    ranges[si] = start..start + 1;
                } else {
                    ranges[si].end += 1;
                }
            }
        }
        per_chunk.push((origin, Aabb::from_min_max(lo, hi), ranges));
    }
    // Chunks assembled before a slot exists have shorter range vectors; every chunk needs one
    // range per slot (empty when the chunk holds none of it).
    let slot_count = slots.len();
    for (_, _, ranges) in &mut per_chunk {
        ranges.resize(slot_count, 0..0);
    }
    // Concatenate slot instances and derive the per-chunk ranges into the flat record buffer.
    let mut pieces: Vec<GpuKitInstance> = vec![];
    let mut out_slots: Vec<KitSlot> = vec![];
    let mut meta: Vec<KitGpuMeta> = vec![];
    for (fid, material, instances) in &slots {
        let meshes3 = families.meshes_of(*fid);
        let tri = |h: Option<&Handle<Mesh>>| {
            h.and_then(|h| meshes.get(h))
                .map_or(0, |m| m.indices().map_or(0, Indices::len) as u32)
        };
        let tris = [
            tri(meshes3[0].as_ref()),
            tri(meshes3[1].as_ref()),
            tri(meshes3[2].as_ref()),
        ];
        let mesh_ids = std::array::from_fn(|i| meshes3[i].as_ref().map(|h| h.id()));
        for h in &meshes3 {
            meta.push(gpu_meta(h.as_ref().and_then(|h| meshes.get(h))));
        }
        pieces.extend(instances.iter().copied());
        out_slots.push(KitSlot {
            mesh: mesh_ids,
            tris,
            material: material.id(),
            key: 0,
        });
    }
    let mut out_chunks: Vec<KitChunk> = vec![];
    for (origin, aabb, ranges) in per_chunk {
        out_chunks.push(KitChunk {
            centre: origin,
            aabb,
            ranges,
        });
    }
    let _count = slots.iter().map(|(_, _, i)| i.len()).sum::<usize>();
    for (si, (fid, material, _)) in slots.iter().enumerate() {
        let meshes3 = families.meshes_of(*fid);
        for lod in 0..3u8 {
            if meshes3[lod as usize].is_none() {
                continue;
            }
            chunks.slot_entities.push(
                commands
                    .spawn((
                        KitSlotDraw {
                            slot: si as u32,
                            lod,
                        },
                        MeshMaterial3d(material.clone()),
                        Visibility::Inherited,
                        SceneEntity,
                    ))
                    .id(),
            );
        }
    }
    commands.insert_resource(KitInstanceSet(Arc::new(KitInstanceSetData {
        pieces,
        slots: out_slots,
        chunks: out_chunks,
        meta,
        version: 0,
    })));
}

/// A (slot, LOD) mesh's decompress metadata, mirroring the mesh uniform's `MeshMetadata`
/// (`Mesh::final_aabb` and `final_uv_ranges` are what the allocator uploads).
fn gpu_meta(mesh: Option<&Mesh>) -> KitGpuMeta {
    const ZERO: KitGpuMeta = KitGpuMeta {
        center: [0.0; 3],
        _pad0: 0,
        half: [0.0; 3],
        _pad1: 0,
        uv_a: [0.0; 4],
        uv_b: [0.0; 4],
    };
    let Some(m) = mesh else {
        return ZERO;
    };
    let (center, half, _uv) = match (m.final_aabb, m.final_uv_ranges) {
        (Some(a), uv) => (
            [a.center().x, a.center().y, a.center().z],
            [a.half_size().x, a.half_size().y, a.half_size().z],
            uv,
        ),
        (None, uv) => ([0.0; 3], [0.0; 3], uv),
    };
    let uv = m.final_uv_ranges;
    let uv_a = match uv[0] {
        Some(b) => [b.min.x, b.min.y, b.max.x - b.min.x, b.max.y - b.min.y],
        None => [0.0, 0.0, 1.0, 1.0],
    };
    let uv_b = match uv[1] {
        Some(b) => [b.min.x, b.min.y, b.max.x - b.min.x, b.max.y - b.min.y],
        None => [0.0, 0.0, 1.0, 1.0],
    };
    KitGpuMeta {
        center,
        _pad0: 0,
        half,
        _pad1: 0,
        uv_a,
        uv_b,
    }
}

/// A mesh's local position bounds (`None` mesh: origin).
fn mesh_bounds(mesh: Option<&Mesh>) -> (Vec3, Vec3) {
    let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) =
        mesh.and_then(|m| m.attribute(Mesh::ATTRIBUTE_POSITION))
    else {
        return (Vec3::ZERO, Vec3::ZERO);
    };
    let (lo, hi) = p.iter().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(lo, hi), v| (lo.min(Vec3::from(*v)), hi.max(Vec3::from(*v))),
    );
    (lo, hi)
}

/// The world-space bounds of `mesh_bounds` transformed by `t` (all eight corners).
fn bounds_of(lo: &Vec3, hi: &Vec3, t: &Transform) -> (Vec3, Vec3) {
    let (mut out_lo, mut out_hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for i in 0..8 {
        let s = Vec3::new(
            if i & 1 == 0 { -1.0 } else { 1.0 },
            if i & 2 == 0 { -1.0 } else { 1.0 },
            if i & 4 == 0 { -1.0 } else { 1.0 },
        );
        let corner = *lo + (Vec3::ONE + s) * 0.5 * (*hi - *lo);
        let p = t.transform_point(corner);
        out_lo = out_lo.min(p);
        out_hi = out_hi.max(p);
    }
    (out_lo, out_hi)
}

fn update_report(chunks: &PlayChunks, report: &mut super::scatter::ScatterReport) {
    if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&report.0) {
        let unbatched = value["unbatched_entities"]
            .as_u64()
            .or_else(|| value["entities"].as_u64())
            .unwrap_or(0);
        let instanced = chunks.tiles.values().flat_map(|t| &t.parts).count() as u64;
        let draws = chunks.slot_entities.len() as u64;
        value["unbatched_entities"] = unbatched.into();
        value["entities"] = (unbatched.saturating_sub(instanced) + draws).into();
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
        // One family per part, as in production: a family is one piece primitive across LODs,
        // and one placement of it instances once. Distinct meshes, one family each.
        let mut app_families = KitFamilies::default();
        let mut handles: Vec<Handle<Mesh>> = vec![];
        {
            let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
            for _ in 0..50 {
                handles.push(meshes.add(Cuboid::new(1.0, 2.0, 3.0)));
            }
        }
        for (i, h) in handles.iter().enumerate() {
            app_families.family_of.insert(h.id(), (i as u32, 0));
            app_families
                .meshes_of
                .insert(i as u32, [Some(h.clone()), None, None]);
        }
        app.insert_resource(app_families);
        let root = app
            .world_mut()
            .spawn((Transform::from_xyz(12.0, 0.0, 3.0), Visibility::Inherited))
            .id();
        let mut first = Entity::PLACEHOLDER;
        for (i, h) in handles.iter().enumerate() {
            let local = Transform::from_xyz(i as f32, 0.0, 0.0);
            let id = app
                .world_mut()
                .spawn((
                    Mesh3d(h.clone()),
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
    fn settled_parts_instance_and_restore_before_removal_with_pair_ids_repaired() {
        let (mut app, root, old_pair) = app();
        app.add_systems(Update, settle);
        app.update();
        assert_eq!(
            app.world_mut().query::<&Mesh3d>().iter(app.world()).count(),
            0
        );
        let set = app.world().resource::<KitInstanceSet>().0.pieces.len();
        assert_eq!(set, 50);
        let chunks = app.world().resource::<PlayChunks>();
        assert_eq!(chunks.tiles[&root].parts.len(), 50);
        assert_eq!(chunks.slot_entities.len(), 50);
        app.world_mut().resource_mut::<PlayView>().tiles.clear();
        app.add_systems(First, restore);
        app.update();
        // Normal sync removes the root between restoration and settlement in production.
        let pair = app.world().resource::<PlaySpawned>().pair_entities[&Axial::ORIGIN][0];
        assert_ne!(pair, old_pair);
        assert!(app.world().entities().contains(root));
    }

    #[test]
    fn settled_tiles_wait_for_the_rise_wave_and_settle_once() {
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
            assert!(
                !app.world().is_resource_added::<KitInstanceSet>()
                    && app
                        .world()
                        .get_resource::<KitInstanceSet>()
                        .is_none_or(|s| s.0.pieces.is_empty())
            );
        }
        app.world_mut().resource_mut::<PlayRises>().rest(other);
        app.update();
        let set = app.world().resource::<KitInstanceSet>().0.pieces.len();
        assert_eq!(set, 50);
        assert_eq!(
            app.world().resource::<PlayChunks>().tiles[&root]
                .parts
                .len(),
            50
        );
        for _ in 0..4 {
            app.update();
            assert_eq!(app.world().resource::<KitInstanceSet>().0.pieces.len(), 50);
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
        assert_eq!(app.world().resource::<KitInstanceSet>().0.pieces.len(), 50);
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
        assert!(
            app.world()
                .resource::<PlayChunks>()
                .slot_entities
                .is_empty()
        );
    }
}
