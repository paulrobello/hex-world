use std::{collections::HashMap, sync::Arc};

use bevy::{
    gltf::{Gltf, GltfMesh},
    prelude::*,
};
use hex_core::{Axial, TileDef};

use crate::play::{
    PlayCatalog, PlayKit, PlayTile, PlayView, PlayWaiting,
    packs::PlayPacks,
    scatter::{self, ScatterLayouts},
};
use hex_render::{ActiveScenario, Scenario};

use super::ground::{TileSurface, WalkGround, ride_max};
use super::shape::{PieceShape, piece_shape};

/// Surfaces (with their [`ride_max`]) by tile id and collider footprints by piece, kept across
/// rebuilds, and what the ground was last built from.
#[derive(Default)]
pub struct WalkCache {
    surfaces: HashMap<String, (Arc<TileSurface>, f32)>,
    shapes: HashMap<String, PieceShape>,
    built: Option<BuiltFrom>,
}

/// The inputs of the last rebuild: the drawn tiles, the hidden ones, the scatter revision and how
/// many kit pieces were built. Change detection cannot stand in for it: `PlayView` changes with
/// every hovered cell and `PlayKit` is written every frame.
#[derive(Clone, PartialEq)]
struct BuiltFrom {
    tiles: Vec<PlayTile>,
    tunnels: Vec<hex_core::Tunnel>,
    world_seed: u64,
    hidden: Vec<Axial>,
    scatter: u64,
    kit_pieces: usize,
}

/// Rebuilds [`WalkGround`] in the play scenario when the view, the hidden tiles, the scatter or
/// the kit change (after `play::PlaySystems`, so the scatter is current).
#[allow(clippy::too_many_arguments)]
pub fn sync(
    active: Res<ActiveScenario>,
    view: Res<PlayView>,
    (waiting, layouts, kit, packs, catalog): (
        Option<Res<PlayWaiting>>,
        Res<ScatterLayouts>,
        Option<Res<PlayKit>>,
        Option<Res<PlayPacks>>,
        Res<PlayCatalog>,
    ),
    (gltfs, gltf_meshes, meshes): (Res<Assets<Gltf>>, Res<Assets<GltfMesh>>, Res<Assets<Mesh>>),
    mut ground: ResMut<WalkGround>,
    mut cache: Local<WalkCache>,
) {
    let (Scenario::Play, Some(kit_res), Some(packs)) = (active.0, kit, packs) else {
        return;
    };
    let PlayKit::Ready(kit) = &*kit_res else {
        return;
    };
    let hidden: Vec<Axial> = waiting.map(|w| w.0.clone()).unwrap_or_default();
    let from = BuiltFrom {
        tiles: view.tiles.clone(),
        tunnels: view.tunnels.clone(),
        world_seed: view.world_seed,
        hidden,
        scatter: layouts.revision,
        kit_pieces: kit.len(),
    };
    if cache.built.as_ref() == Some(&from) {
        return;
    }
    if cache
        .built
        .as_ref()
        .is_some_and(|b| b.kit_pieces != from.kit_pieces)
    {
        // A pack arrival can turn a placeholder into kit ground or a fallback into a collider.
        cache.surfaces.clear();
        cache.shapes.clear();
    }
    let assets = (&*gltfs, &*gltf_meshes, &*meshes);
    let WalkCache {
        surfaces, shapes, ..
    } = &mut *cache;
    let mut surface_of = |def: &TileDef| -> Option<(Arc<TileSurface>, f32)> {
        if let Some(s) = surfaces.get(&def.id) {
            return Some(s.clone());
        }
        let pieces = scatter::ground_pieces(def, &catalog.0);
        if !pieces.iter().all(|(id, _)| kit.contains_key(*id)) {
            return None;
        }
        let s = Arc::new(TileSurface::new(def, &catalog.0, &mut |id| {
            scatter::kit_tris(id, &packs, assets)
        })?);
        let ride = ride_max(def, &s.centre);
        surfaces.insert(def.id.clone(), (s.clone(), ride));
        Some((s, ride))
    };
    let mut shape_of = |piece: &str| -> PieceShape {
        if let Some(h) = shapes.get(piece) {
            return h.clone();
        }
        match kit
            .contains_key(piece)
            .then(|| piece_shape(piece, &packs, assets))
            .flatten()
        {
            Some(h) => {
                shapes.insert(piece.to_owned(), h.clone());
                h
            }
            None => PieceShape::fallback(piece),
        }
    };
    let revision = ground.revision + 1;
    *ground = WalkGround::build(
        &from.tiles,
        &catalog.0,
        from.world_seed,
        &from.hidden,
        &mut surface_of,
        &mut shape_of,
        &layouts.tiles,
    );
    ground.set_tunnels(&from.tunnels);
    ground.revision = revision;
    cache.built = Some(from);
}
