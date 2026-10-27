//! Diagnostic used for docs/perf/large-world-bounds/ (not compiled into the game): what Bevy's
//! `calculate_bounds` sees each frame. To use it, copy it to crates/hex-app/src/bounds_diag.rs, add
//! `mod bounds_diag;` and `.add_plugins(bounds_diag::BoundsDiagPlugin)` to crates/hex-app/src/main.rs,
//! and run natively with HEXW_BOUNDS_DIAG=1. Every 120 frames it logs the Mesh3d census, how many
//! entities had `Mesh3d` changed and `Aabb` rewritten since the last frame, the mesh and material
//! asset events, and (bounds_diag2) the changed entities' materials and vertex counts with the time
//! one `MeshAabb::get_aabb` pass over them takes.
use bevy::asset::AssetEvent;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::{NoAutoAabb, NoFrustumCulling, VisibilitySystems};
use bevy::ecs::archetype::Archetypes;
use bevy::ecs::component::Components;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;

pub struct BoundsDiagPlugin;

impl Plugin for BoundsDiagPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var_os("HEXW_BOUNDS_DIAG").is_some() {
            app.add_systems(
                PostUpdate,
                bounds_diag.after(VisibilitySystems::CalculateBounds),
            );
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn bounds_diag(
    mut events: MessageReader<AssetEvent<Mesh>>,
    all: Query<(Entity, &Mesh3d, Has<Aabb>, Has<NoFrustumCulling>, Has<NoAutoAabb>)>,
    changed: Query<
        (
            Entity,
            &Mesh3d,
            Has<MeshMaterial3d<hex_render::water::WaterMaterial>>,
            Has<MeshMaterial3d<hex_render::water::TerrainMaterial>>,
        ),
        Changed<Mesh3d>,
    >,
    kinds: Query<(
        Has<MeshMaterial3d<hex_render::water::WaterMaterial>>,
        Has<MeshMaterial3d<hex_render::water::TerrainMaterial>>,
        Has<MeshMaterial3d<StandardMaterial>>,
    ), With<Mesh3d>>,
    mut wev: MessageReader<AssetEvent<hex_render::water::WaterMaterial>>,
    mut tev: MessageReader<AssetEvent<hex_render::water::TerrainMaterial>>,
    added: Query<Entity, Added<Mesh3d>>,
    aabb_changed: Query<Entity, (Changed<Aabb>, With<Mesh3d>)>,
    names: Query<&Name>,
    meshes: Res<Assets<Mesh>>,
    archetypes: &Archetypes,
    components: &Components,
    mut frame: Local<u64>,
) {
    *frame += 1;
    let mut ev_added: HashSet<AssetId<Mesh>> = HashSet::default();
    let mut ev_modified: HashSet<AssetId<Mesh>> = HashSet::default();
    let mut ev_removed = 0;
    let mut ev_loaded = 0;
    for e in events.read() {
        match e {
            AssetEvent::Added { id } => {
                ev_added.insert(*id);
            }
            AssetEvent::Modified { id } => {
                ev_modified.insert(*id);
            }
            AssetEvent::Removed { .. } | AssetEvent::Unused { .. } => ev_removed += 1,
            AssetEvent::LoadedWithDependencies { .. } => ev_loaded += 1,
        }
    }
    if *frame % 120 != 0 {
        return;
    }
    let mut total = 0;
    let mut with_aabb = 0;
    let mut nfc = 0;
    let mut no_auto = 0;
    let mut pending = 0;
    let mut pending_unloaded = 0;
    let mut pending_no_aabb = 0;
    let mut users_of_changed_assets = 0;
    let mut per_mesh: HashMap<AssetId<Mesh>, u32> = HashMap::default();
    for (_, m, has_aabb, has_nfc, has_no_auto) in &all {
        total += 1;
        with_aabb += has_aabb as u32;
        nfc += has_nfc as u32;
        no_auto += has_no_auto as u32;
        *per_mesh.entry(m.id()).or_default() += 1;
        if ev_added.contains(&m.id()) || ev_modified.contains(&m.id()) {
            users_of_changed_assets += 1;
        }
        if !has_aabb && !has_nfc && !has_no_auto {
            pending += 1;
            match meshes.get(m) {
                None => pending_unloaded += 1,
                Some(mesh) if <Mesh as bevy::camera::primitives::MeshAabb>::get_aabb(mesh).is_none() => pending_no_aabb += 1,
                _ => {}
            }
        }
    }
    let changed_n = changed.iter().count();
    let (mut cw, mut ct, mut verts, mut maxv) = (0, 0, 0usize, 0usize);
    let t0 = std::time::Instant::now();
    for (_, m, w, t) in &changed {
        cw += w as u32;
        ct += t as u32;
        if let Some(mesh) = meshes.get(m) {
            let v = mesh.count_vertices();
            verts += v;
            maxv = maxv.max(v);
            std::hint::black_box(<Mesh as bevy::camera::primitives::MeshAabb>::get_aabb(mesh));
        }
    }
    let recompute_us = t0.elapsed().as_secs_f64() * 1e6;
    let (mut kw, mut kt, mut ks) = (0, 0, 0);
    for (w, t, st) in &kinds {
        kw += w as u32;
        kt += t as u32;
        ks += st as u32;
    }
    let wm = wev.read().filter(|e| matches!(e, AssetEvent::Modified { .. })).count();
    let tm = tev.read().filter(|e| matches!(e, AssetEvent::Modified { .. })).count();
    info!(
        "bounds_diag2 changed: water_mat={cw} terrain_mat={ct} verts={verts} max_verts={maxv} get_aabb_us={recompute_us:.0} | all mesh3d: water_mat={kw} terrain_mat={kt} standard={ks} | material Modified events this frame: water={wm} terrain={tm}"
    );
    let added_n = added.iter().count();
    // Archetypes holding Mesh3d (the query iterates each).
    let mesh3d_id = components.component_id::<Mesh3d>();
    let mut arch: Vec<(usize, Vec<String>)> = archetypes
        .iter()
        .filter(|a| mesh3d_id.is_some_and(|id| a.contains(id)) && !a.is_empty())
        .map(|a| {
            (
                a.len() as usize,
                a.components()
                    .iter().filter_map(|c| components.get_info(*c).map(|i| short(&i.name().to_string())))
                    .collect(),
            )
        })
        .collect();
    arch.sort_by_key(|a| std::cmp::Reverse(a.0));
    let mut changed_names: HashMap<String, u32> = HashMap::default();
    for (e, ..) in &changed {
        let n = names.get(e).map(|n| n.as_str().to_string()).unwrap_or_else(|_| "?".into());
        *changed_names.entry(n).or_default() += 1;
    }
    let mut cn: Vec<_> = changed_names.into_iter().collect();
    cn.sort_by_key(|c| std::cmp::Reverse(c.1));
    cn.truncate(8);
    info!(
        "bounds_diag frame={} mesh3d={} with_aabb={} nofrustum={} noauto={} pending_no_aabb_comp={} (unloaded={} empty={}) changed_mesh3d={} added_mesh3d={} aabb_changed={} ev_added={} ev_modified={} ev_removed={} ev_loaded={} users_of_changed_assets={} distinct_meshes={} archetypes={} mesh_assets={} changed_by_name={:?}",
        *frame,
        total,
        with_aabb,
        nfc,
        no_auto,
        pending,
        pending_unloaded,
        pending_no_aabb,
        changed_n,
        added_n,
        aabb_changed.iter().count(),
        ev_added.len(),
        ev_modified.len(),
        ev_removed,
        ev_loaded,
        users_of_changed_assets,
        per_mesh.len(),
        arch.len(),
        meshes.len(),
        cn
    );
    if *frame % 600 == 0 {
        for (n, comps) in arch.iter().take(12) {
            info!("bounds_diag archetype n={} [{}]", n, comps.join(", "));
        }
    }
}

fn short(name: &str) -> String {
    let base = name.split('<').next().unwrap_or(name);
    let last = base.rsplit("::").next().unwrap_or(base);
    if name.contains('<') {
        let inner = name[name.find('<').unwrap() + 1..].trim_end_matches('>');
        format!("{last}<{}>", inner.rsplit("::").next().unwrap_or(inner))
    } else {
        last.to_string()
    }
}
