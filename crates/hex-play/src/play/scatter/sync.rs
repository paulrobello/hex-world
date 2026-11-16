//! The play systems: spawn each tile's scatter as kit children, report what was drawn, and follow
//! the world as it changes.
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

use bevy::{
    gltf::{Gltf, GltfMesh},
    prelude::*,
};
use hex_core::{Axial, TileCatalog, TileDef, cell_seed};

use super::super::{
    KitPart, PlayCatalog, PlayKit, PlaySpawned, PlayView,
    packs::PlayPacks,
    rise::{PropKind, RiseRole},
    spawn_kit_parts,
};
use hex_render::{ActiveScenario, Scenario, SceneEntity};

use super::layout::{Instance, Layout, Prepared, layout, prepare};
use super::surface::{Tri, ground_pieces, ground_tris};
use super::tables::{footprint, group};

// ---------------------------------------------------------------------------------------------
// Play: drawing, streaming, the shared layouts and the report

/// The entity holding one tile's scatter, a child of the tile root.
#[derive(Component)]
pub struct ScatterGroup;

/// What a tile's group was drawn from: its tile, seed and which rules' pieces the kit had.
#[derive(Clone, Debug, PartialEq)]
struct Key {
    tile: String,
    seed: u64,
    drawn: Vec<bool>,
    /// The field boundaries on the tile's edges ([`super::super::boundaries::fingerprint`]).
    cut: u64,
}

/// Prepared grounds by tile id, layouts by cell, and the group drawn under each tile root.
#[derive(Resource, Default)]
pub struct PlayScatter {
    prepared: HashMap<String, Arc<Prepared>>,
    layouts: HashMap<Axial, (String, u64, u64, Arc<Layout>)>,
    roots: HashMap<Entity, (Key, Option<Entity>)>,
    /// The field boundaries' footprints per tile (tile frame), with the fingerprint of the world
    /// seed and placed tiles they were derived from: rebuilt only when those change, not on every
    /// view change (a hovered cell).
    fences: (u64, Arc<HashMap<Axial, Vec<hex_core::boundary::Seg>>>),
    /// How many times `fences` was derived (tests).
    pub(crate) fence_builds: u64,
}

impl PlayScatter {
    /// The definition's prepared ground, built once its ground pieces are in the kit and shared by
    /// every placed tile of that definition and the ghost. A piece outside the kit holds it back
    /// only while `tris_of` has nothing for it: a piece lost for the session ([`ground_tris`])
    /// adds no ground, and the rest still grows grass and scatter.
    pub(crate) fn ground(
        &mut self,
        def: &TileDef,
        catalog: &TileCatalog,
        kit: &HashMap<String, Vec<KitPart>>,
        tris_of: &mut dyn FnMut(&str) -> Option<Vec<(Tri, String)>>,
    ) -> Option<Arc<Prepared>> {
        if !ground_pieces(def, catalog)
            .iter()
            .all(|(id, _)| kit.contains_key(*id) || tris_of(id).is_some())
        {
            return None;
        }
        if let Some(p) = self.prepared.get(&def.id) {
            return Some(p.clone());
        }
        let p = Arc::new(prepare(def, catalog, tris_of)?);
        self.prepared.insert(def.id.clone(), p.clone());
        Some(p)
    }
}

/// One drawn tile's scatter.
#[derive(Clone, Debug)]
pub struct TileScatter {
    pub tile: String,
    pub seed: u64,
    pub layout: Arc<Layout>,
    /// The field boundaries on the tile's edges in its frame (`hex_core::boundary`, B2): the
    /// layout has none of its instances over them, and grass keeps off them.
    pub fences: Arc<Vec<hex_core::boundary::Seg>>,
    /// Their fingerprint ([`super::super::boundaries::fingerprint`], 0 for none).
    pub cut: u64,
    /// Per rule: its piece is in the kit and its instances are drawn.
    pub drawn: Vec<bool>,
    /// Per rule: its piece.
    pub pieces: Vec<String>,
}

impl TileScatter {
    /// The piece instance `i` draws: its rule's piece or one of its variants.
    pub fn piece_of(&self, i: &Instance) -> &str {
        i.piece(self.pieces[i.rule as usize].as_str())
    }
}

/// The scatter of every drawn tile, for the boids' roosts; `revision` changes with it.
#[derive(Resource, Default)]
pub struct ScatterLayouts {
    pub revision: u64,
    pub tiles: BTreeMap<Axial, TileScatter>,
}

impl ScatterLayouts {
    /// Every drawn instance with its cell and piece.
    pub fn drawn(&self) -> impl Iterator<Item = (Axial, &str, &Instance)> {
        self.tiles.iter().flat_map(|(c, t)| {
            t.layout
                .instances
                .iter()
                .filter(|i| t.drawn[i.rule as usize])
                .map(move |i| (*c, t.piece_of(i), i))
        })
    }
}

/// `hexWorld.state().scatter` as JSON: per tile, the planned, placed and drawn count of each piece.
#[derive(Resource, Clone, Debug, Default)]
pub struct ScatterReport(pub String);

/// Spawns the drawn instances of `layout` under `root`, growing in the props stage of `cell`'s rise.
fn spawn_group(
    commands: &mut Commands,
    (root, cell): (Entity, Axial),
    kit: &HashMap<String, Vec<KitPart>>,
    def: &TileDef,
    layout: &Layout,
    drawn: &[bool],
) -> Option<Entity> {
    if !layout.instances.iter().any(|i| drawn[i.rule as usize]) {
        return None;
    }
    let count = layout.instances.len().min(u8::MAX as usize) as u8;
    let group = commands
        .spawn((
            ScatterGroup,
            Transform::IDENTITY,
            Visibility::default(),
            SceneEntity,
        ))
        .id();
    commands.entity(root).add_child(group);
    commands.entity(group).with_children(|p| {
        for inst in layout.instances.iter().filter(|i| drawn[i.rule as usize]) {
            let role = RiseRole::Prop {
                order: inst.order,
                count,
                kind: PropKind::Tree,
            };
            let piece = inst.piece(def.props.scatter[inst.rule as usize].piece.as_str());
            spawn_kit_parts(p, kit, piece, inst.transform(), Some((cell, role)));
        }
    });
    Some(group)
}

fn json_counts(pieces: &[String], n: impl Fn(usize) -> usize) -> String {
    let fields: Vec<String> = pieces
        .iter()
        .enumerate()
        .map(|(i, p)| format!("\"{p}\":{}", n(i)))
        .collect();
    format!("{{{}}}", fields.join(","))
}

fn report(
    world_seed: u64,
    tiles: &BTreeMap<Axial, TileScatter>,
    waiting: &[(Axial, String)],
    kit: &HashMap<String, Vec<KitPart>>,
) -> String {
    let mut rows = vec![];
    let mut drawn_by_piece: BTreeMap<&str, usize> = BTreeMap::new();
    let mut placed_total = 0;
    for (c, t) in tiles {
        let l = &t.layout;
        let drawn = |i: usize| if t.drawn[i] { l.placed(i) } else { 0 };
        for p in &t.pieces {
            drawn_by_piece.entry(p.as_str()).or_default();
        }
        for i in l.instances.iter().filter(|i| t.drawn[i.rule as usize]) {
            *drawn_by_piece.entry(t.piece_of(i)).or_default() += 1;
        }
        placed_total += l.instances.len();
        rows.push(format!(
            "{{\"q\":{},\"r\":{},\"tile\":\"{}\",\"layout\":\"{:016x}\",\"planned\":{},\"placed\":{},\"drawn\":{},\"area_m2\":{}}}",
            c.q,
            c.r,
            t.tile,
            l.fingerprint(),
            json_counts(&t.pieces, |i| l.planned[i] as usize),
            json_counts(&t.pieces, |i| l.placed(i)),
            json_counts(&t.pieces, drawn),
            json_counts(&t.pieces, |i| l.area[i].round() as usize),
        ));
    }
    let entities: usize = drawn_by_piece
        .iter()
        .map(|(p, n)| n * kit.get(*p).map_or(0, Vec::len))
        .sum();
    let pairs: HashSet<_> = drawn_by_piece
        .iter()
        .filter(|(_, n)| **n > 0)
        .flat_map(|(p, _)| kit.get(*p).into_iter().flatten())
        .map(|part| (part.lod.min(2), part.mesh.id(), part.material.id()))
        .collect();
    // Per LOD, tracked, not gated (owner A24 Q5, 2026-10-26, replacing A2 Q2's 60 per LOD).
    let mut by_lod = [0usize; 3];
    for (lod, _, _) in &pairs {
        by_lod[*lod] += 1;
    }
    let drawn_total: usize = drawn_by_piece.values().sum();
    let by_piece: Vec<String> = drawn_by_piece
        .iter()
        .map(|(p, n)| format!("\"{p}\":{n}"))
        .collect();
    let waiting: Vec<String> = waiting
        .iter()
        .map(|(c, t)| format!("{{\"q\":{},\"r\":{},\"tile\":\"{t}\"}}", c.q, c.r))
        .collect();
    format!(
        "{{\"world_seed\":{world_seed},\"tiles\":[{}],\"waiting\":[{}],\"placed\":{placed_total},\"drawn\":{drawn_total},\"drawn_by_piece\":{{{}}},\"entities\":{entities},\"mesh_material_pairs\":{},\"mesh_material_pairs_by_lod\":[{},{},{}]}}",
        rows.join(","),
        waiting.join(","),
        by_piece.join(","),
        pairs.len(),
        by_lod[0],
        by_lod[1],
        by_lod[2]
    )
}

/// After `play::sync`: draws each placed tile's scatter under its root once its ground is in the
/// kit, redraws it when a rule's piece arrives or a pack refresh rebuilt the tile's children, and
/// publishes the layouts and the report.
#[allow(clippy::too_many_arguments)]
pub fn sync(
    mut commands: Commands,
    active: Res<ActiveScenario>,
    view: Res<PlayView>,
    (spawned, kit, packs, catalog): (
        Option<Res<PlaySpawned>>,
        Option<Res<PlayKit>>,
        Option<Res<PlayPacks>>,
        Res<PlayCatalog>,
    ),
    (gltfs, gltf_meshes, meshes): (Res<Assets<Gltf>>, Res<Assets<GltfMesh>>, Res<Assets<Mesh>>),
    (mut state, mut layouts, mut out): (
        ResMut<PlayScatter>,
        ResMut<ScatterLayouts>,
        ResMut<ScatterReport>,
    ),
    groups: Query<(), With<ScatterGroup>>,
    mut removed: RemovedComponents<ScatterGroup>,
) {
    let gone = removed.read().count() > 0;
    let (Scenario::Play, Some(spawned), Some(kit_res), Some(packs)) =
        (active.0, spawned, kit, packs)
    else {
        return;
    };
    let PlayKit::Ready(kit) = &*kit_res else {
        return;
    };
    if !(view.is_changed() || spawned.is_changed() || kit_res.is_changed() || gone) {
        return;
    }
    let state = &mut *state;
    let assets = (&*gltfs, &*gltf_meshes, &*meshes);
    let mut roots = HashMap::new();
    let mut next_layouts = HashMap::new();
    let mut tiles = BTreeMap::new();
    let mut waiting = vec![];
    let mut cells: Vec<_> = spawned.tiles.iter().collect();
    cells.sort_by_key(|(c, _)| (c.q, c.r));
    let tiles_sig = {
        let mut h: u64 = 0xCBF2_9CE4_8422_2325 ^ view.world_seed;
        for (c, (t, _)) in &cells {
            for b in t
                .tile
                .bytes()
                .chain(c.q.to_le_bytes())
                .chain(c.r.to_le_bytes())
                .chain([t.rotation])
            {
                h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01B3);
            }
        }
        h
    };
    if state.fences.0 != tiles_sig || state.fence_builds == 0 {
        let map = fence_map(view.world_seed, &spawned.tiles, &catalog.0);
        state.fences = (tiles_sig, Arc::new(map));
        state.fence_builds += 1;
    }
    let fence_map = state.fences.1.clone();
    let ridges = super::super::tunnels::ridges(&view);
    for (&cell, (t, root)) in cells {
        let Some(def) = catalog
            .0
            .get(&t.tile)
            .filter(|d| !d.props.scatter.is_empty())
        else {
            continue;
        };
        let Some(prepared) = state.ground(def, &catalog.0, kit, &mut |id| {
            ground_tris(id, &packs, assets)
        }) else {
            waiting.push((cell, def.id.clone()));
            continue;
        };
        let seed = cell_seed(view.world_seed, cell);
        // The field boundaries on its edges (B2): instances over them are left out, after the darts,
        // so the rest of the layout stays where it was.
        let fences = fence_map.get(&cell).cloned().unwrap_or_default();
        let ridge = ridges.get(&cell).copied();
        let cut = super::super::boundaries::fingerprint(&fences) ^ ridge.map_or(0, |r| r.sig());
        let layout = match state.layouts.get(&cell) {
            Some((id, s, k, l)) if *id == def.id && *s == seed && *k == cut => l.clone(),
            _ => {
                let mut l = layout(def, seed, &prepared);
                // Nothing stands on a rail tunnel's turf ridge (owner Q4).
                if let Some(r) = ridge {
                    let pieces: Vec<&str> =
                        def.props.scatter.iter().map(|r| r.piece.as_str()).collect();
                    l.instances.retain(|i| {
                        let piece = i.piece(pieces[i.rule as usize]);
                        !r.holds(i.pos.xz(), footprint(piece).ground * i.scale)
                    });
                }
                if !fences.is_empty() {
                    let pieces: Vec<&str> =
                        def.props.scatter.iter().map(|r| r.piece.as_str()).collect();
                    l.instances.retain(|i| {
                        let piece = i.piece(pieces[i.rule as usize]);
                        super::super::boundaries::clear_of(
                            &fences,
                            i.pos.xz(),
                            footprint(piece).ground * i.scale,
                        )
                    });
                }
                Arc::new(l)
            }
        };
        let drawn: Vec<bool> = def
            .props
            .scatter
            .iter()
            .map(|r| group(r.piece.as_str()).all(|p| kit.contains_key(p)))
            .collect();
        let key = Key {
            tile: def.id.clone(),
            seed,
            drawn: drawn.clone(),
            cut,
        };
        let group = match state.roots.get(root) {
            Some((k, g)) if *k == key && g.is_none_or(|g| groups.contains(g)) => *g,
            old => {
                if let Some((_, Some(g))) = old {
                    commands.entity(*g).try_despawn();
                }
                spawn_group(&mut commands, (*root, cell), kit, def, &layout, &drawn)
            }
        };
        roots.insert(*root, (key, group));
        next_layouts.insert(cell, (def.id.clone(), seed, cut, layout.clone()));
        tiles.insert(
            cell,
            TileScatter {
                tile: def.id.clone(),
                seed,
                layout,
                fences: Arc::new(fences),
                cut,
                drawn,
                pieces: def
                    .props
                    .scatter
                    .iter()
                    .map(|r| r.piece.as_str().to_owned())
                    .collect(),
            },
        );
    }
    state.roots = roots;
    state.layouts = next_layouts;
    let sig = |m: &BTreeMap<Axial, TileScatter>| -> Vec<(Axial, String, u64, Vec<bool>, u64)> {
        m.iter()
            .map(|(c, t)| (*c, t.tile.clone(), t.seed, t.drawn.clone(), t.cut))
            .collect()
    };
    if sig(&layouts.tiles) != sig(&tiles) {
        layouts.revision += 1;
        layouts.tiles = tiles;
    }
    let json = report(view.world_seed, &layouts.tiles, &waiting, kit);
    if out.0 != json {
        out.0 = json;
    }
}

/// The boundary footprints of every drawn field tile, by the drawn tile they lie on, in its
/// frame: what scatter keeps clear of. A footprint tile's, at its anchor, hold those on all its
/// cells. The drawn `tiles` are read with footprint ring cells resolved ([`super::super::covering`]).
pub(super) fn fence_map(
    world_seed: u64,
    tiles: &HashMap<Axial, (super::super::PlayTile, Entity)>,
    catalog: &TileCatalog,
) -> HashMap<Axial, Vec<hex_core::boundary::Seg>> {
    let tile_at = super::super::covering(|c: Axial| {
        tiles
            .get(&c)
            .and_then(|(t, _)| Some((catalog.get(&t.tile)?, t.rotation)))
    });
    let level = |c: Axial| tiles.get(&c).map_or(0, |(t, _)| t.level);
    let all = super::super::boundaries::all_placements(
        world_seed,
        tiles.keys().copied(),
        &tile_at,
        &level,
    );
    // The drawn tiles beside a field tile, a footprint's ring cell standing for its anchor, whose
    // entry gathers every cell it covers.
    let near: std::collections::BTreeSet<Axial> = all
        .keys()
        .flat_map(|c| hex_core::boundary::within(*c, 1))
        .filter_map(|c| {
            if tiles.contains_key(&c) {
                return Some(c);
            }
            (0..6).map(|e| c.neighbor(e)).find(|a| {
                tiles
                    .get(a)
                    .and_then(|(t, _)| catalog.get(&t.tile))
                    .is_some_and(TileDef::is_mega)
            })
        })
        .collect();
    near.into_iter()
        .map(|c| {
            (
                c,
                super::super::boundaries::local_segments_from(&all, c, &tile_at),
            )
        })
        .filter(|(_, s)| !s.is_empty())
        .collect()
}
