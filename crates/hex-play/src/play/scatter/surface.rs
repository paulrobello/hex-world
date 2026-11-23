//! The topmost LOD0 ground of a tile as a bucketed triangle grid, and the ground pieces that make it.

use bevy::{
    gltf::{Gltf, GltfMesh},
    prelude::*,
};
use hex_core::{TileCatalog, TileDef};

use super::super::packs::PlayPacks;

use super::layout::EXTENT;
use super::tables::{is_ground_prop, is_not_ground};

/// Bucket size (m) of the surface's triangle grid.
const BUCKET: f32 = 1.0;

const BUCKETS: usize = 33;

/// A triangle's corners in the tile frame (runtime axes: Y up).
pub type Tri = [[f32; 3]; 3];

/// A tile's ground: the topmost upward triangles over each (x, z), with their slot names,
/// bucketed on a 1 m grid.
pub struct Surface {
    pub(super) slots: Vec<String>,
    /// Each triangle with its doubled signed (x, z) area, which `top` divides by.
    pub(super) tris: Vec<(Tri, f32, u16)>,
    buckets: Vec<Vec<u32>>,
}

pub(super) fn bucket(v: f32) -> usize {
    (((v + EXTENT) / BUCKET).floor().max(0.0) as usize).min(BUCKETS - 1)
}

impl Surface {
    /// Vertical (skirt) and degenerate triangles carry no ground and are dropped, as are the
    /// [`is_not_ground`] slots.
    pub fn new(parts: impl IntoIterator<Item = (Tri, String)>) -> Self {
        let mut slots: Vec<String> = vec![];
        let mut tris = vec![];
        let mut buckets = vec![vec![]; BUCKETS * BUCKETS];
        for (t, name) in parts {
            if is_not_ground(&name) {
                continue;
            }
            let [a, b, c] = t;
            let d = (b[0] - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (b[2] - a[2]);
            if d.abs() < 1e-9 {
                continue;
            }
            let slot = match slots.iter().position(|s| *s == name) {
                Some(i) => i,
                None => {
                    slots.push(name);
                    slots.len() - 1
                }
            } as u16;
            let i = tris.len() as u32;
            tris.push((t, d, slot));
            let lo = |k: usize| bucket(t.iter().map(|p| p[k]).fold(f32::MAX, f32::min));
            let hi = |k: usize| bucket(t.iter().map(|p| p[k]).fold(f32::MIN, f32::max));
            for bz in lo(2)..=hi(2) {
                for bx in lo(0)..=hi(0) {
                    buckets[bz * BUCKETS + bx].push(i);
                }
            }
        }
        Self {
            slots,
            tris,
            buckets,
        }
    }

    /// Height and slot index ([`Surface::slots`]) of the topmost upward triangle over (x, z).
    pub(crate) fn top(&self, x: f32, z: f32) -> Option<(f32, u16)> {
        if x.abs() >= EXTENT || z.abs() >= EXTENT {
            return None;
        }
        let mut best: Option<(f32, u16)> = None;
        for &i in &self.buckets[bucket(z) * BUCKETS + bucket(x)] {
            let ([a, b, c], d, slot) = self.tris[i as usize];
            let u = ((x - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (z - a[2])) / d;
            let v = ((b[0] - a[0]) * (z - a[2]) - (x - a[0]) * (b[2] - a[2])) / d;
            if u < -1e-5 || v < -1e-5 || u + v > 1.0 + 1e-5 {
                continue;
            }
            let y = a[1] + u * (b[1] - a[1]) + v * (c[1] - a[1]);
            if best.is_none_or(|(h, _)| y > h) {
                best = Some((y, slot));
            }
        }
        best
    }

    /// Height and slot of the topmost upward triangle over (x, z), if any.
    pub fn at(&self, x: f32, z: f32) -> Option<(f32, &str)> {
        self.top(x, z)
            .map(|(h, s)| (h, self.slots[s as usize].as_str()))
    }

    /// The slot names, indexed by the slot index [`Surface::top`] returns.
    pub(crate) fn slots(&self) -> &[String] {
        &self.slots
    }

    /// Per bucket of the grid [`Surface::top`] searches, whether any of its triangles passes
    /// `keep` (given its slot index and corners).
    pub(crate) fn buckets_where(&self, keep: impl Fn(u16, &Tri) -> bool) -> Vec<bool> {
        self.buckets
            .iter()
            .map(|b| {
                b.iter().any(|&i| {
                    let (t, _, slot) = &self.tris[i as usize];
                    keep(*slot, t)
                })
            })
            .collect()
    }

    /// The bucket [`Surface::top`] searches for (x, z), `None` where it finds nothing.
    pub(crate) fn bucket_at(x: f32, z: f32) -> Option<usize> {
        (x.abs() < EXTENT && z.abs() < EXTENT).then(|| bucket(z) * BUCKETS + bucket(x))
    }
}

/// The ground pieces of a tile and their turn about +Y in the tile frame ([`turn`](super::layout::turn) steps, as
/// `tile_children` draws them with `piece_rotation`): the base, the centre piece if it has one, its
/// drawn edge pieces ([`TileDef::draws_edge_piece`]), and its [`is_ground_prop`] heroes (a yaw of
/// 60 k degrees is turn k).
pub fn ground_pieces<'a>(def: &'a TileDef, catalog: &'a TileCatalog) -> Vec<(&'a str, u8)> {
    let mut v = vec![(def.base.as_str(), 0)];
    for h in &def.props.hero {
        if is_ground_prop(h.piece.as_str()) {
            v.push((
                h.piece.as_str(),
                (h.yaw_deg / 60.0).round().rem_euclid(6.0) as u8,
            ));
        }
    }
    if let Some(c) = &def.center {
        v.push((c.as_str(), 0));
    }
    let pieces = def.edge_pieces(catalog.sockets());
    for e in 0..6u8 {
        if def.draws_edge_piece(e as usize)
            && let Some(p) = pieces[e as usize]
        {
            v.push((p.as_str(), e));
        }
    }
    v
}

/// The triangles of a footprint tile's ground (`all`, tile frame) that reach the cell whose centre
/// lies `at` from the tile's centre, in that cell's own frame: the ground a [`Surface`] of that
/// cell holds, since one spans a single hex.
pub fn member_tris(all: &[(Tri, String)], at: Vec2) -> Vec<(Tri, String)> {
    let (x, z) = (at.x, at.y);
    all.iter()
        .filter(|(t, _)| {
            let lo = |k: usize, o: f32| t.iter().map(|p| p[k] - o).fold(f32::MAX, f32::min);
            let hi = |k: usize, o: f32| t.iter().map(|p| p[k] - o).fold(f32::MIN, f32::max);
            lo(0, x) < EXTENT && hi(0, x) > -EXTENT && lo(2, z) < EXTENT && hi(2, z) > -EXTENT
        })
        .map(|(t, s)| (t.map(|p| [p[0] - x, p[1], p[2] - z]), s.clone()))
        .collect()
}

/// LOD0 triangles of a loaded kit piece with their slot names (`build_piece`'s slot walk).
pub(crate) fn kit_tris(
    id: &str,
    packs: &PlayPacks,
    (gltfs, gltf_meshes, meshes): (&Assets<Gltf>, &Assets<GltfMesh>, &Assets<Mesh>),
) -> Option<Vec<(Tri, String)>> {
    let g = gltfs.get(packs.glb(id)?)?;
    let gm = gltf_meshes.get(g.named_meshes.get(format!("{id}_lod0").as_str())?)?;
    let mut out = vec![];
    for prim in &gm.primitives {
        let name = prim
            .material
            .as_ref()
            .and_then(|mh| {
                g.named_materials
                    .iter()
                    .find(|(_, v)| *v == mh)
                    .map(|(k, _)| k.to_string())
            })
            .unwrap_or_default();
        let mesh = meshes.get(&prim.mesh)?;
        let pos = mesh.attribute(Mesh::ATTRIBUTE_POSITION)?.as_float3()?;
        let idx: Vec<usize> = mesh.indices()?.iter().collect();
        for t in idx.as_chunks::<3>().0 {
            out.push(([pos[t[0]], pos[t[1]], pos[t[2]]], name.clone()));
        }
    }
    Some(out)
}

/// [`kit_tris`] for a tile's ground: a piece whose GLB failed for good (`PlayPacks::lost`) has
/// none, so its tile's ground is built from the pieces that loaded.
pub(crate) fn ground_tris(
    id: &str,
    packs: &PlayPacks,
    assets: (&Assets<Gltf>, &Assets<GltfMesh>, &Assets<Mesh>),
) -> Option<Vec<(Tri, String)>> {
    kit_tris(id, packs, assets).or_else(|| packs.lost(id).then(Vec::new))
}
