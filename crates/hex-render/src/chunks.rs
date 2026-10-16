//! Static chunk merging (PRD §6, §9): tiles grouped into compact 7- or 19-hex clusters, and per
//! cluster and LOD every static piece that shares a material merged into one mesh.
use std::collections::{BTreeMap, HashMap};

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
};
use hex_core::Axial;

/// Radius of the super-hex that holds `size` cells (`3k² + 3k + 1`).
fn chunk_radius(size: u32) -> i32 {
    match size {
        7 => 1,
        19 => 2,
        _ => panic!("chunk size {size} is not 7 or 19"),
    }
}

/// Center of the radius-`k` super-hex containing `cell`. The centers form the lattice spanned
/// by `(k+1, k)` and `(-k, 2k+1)`, whose super-hexes tile the plane exactly.
pub fn chunk_center(cell: Axial, size: u32) -> Axial {
    let k = chunk_radius(size);
    let d = (3 * k * k + 3 * k + 1) as f32;
    let (q, r) = (cell.q as f32, cell.r as f32);
    let a = (((2 * k + 1) as f32 * q + k as f32 * r) / d).floor() as i32;
    let b = ((-k as f32 * q + (k + 1) as f32 * r) / d).floor() as i32;
    for da in -1..=2 {
        for db in -1..=2 {
            let (a, b) = (a + da, b + db);
            let c = Axial::new(a * (k + 1) - b * k, a * k + b * (2 * k + 1));
            if c.distance(cell) <= k {
                return c;
            }
        }
    }
    unreachable!("super-hex lattice covers every cell")
}

/// Groups tile indices by chunk, in order of each chunk's first tile.
pub fn chunk_tiles(cells: &[Axial], size: u32) -> Vec<(Axial, Vec<usize>)> {
    let mut order: HashMap<Axial, usize> = HashMap::new();
    let mut out: Vec<(Axial, Vec<usize>)> = vec![];
    for (ti, &cell) in cells.iter().enumerate() {
        let c = chunk_center(cell, size);
        let slot = *order.entry(c).or_insert_with(|| {
            out.push((c, vec![]));
            out.len() - 1
        });
        out[slot].1.push(ti);
    }
    out
}

fn attr3(m: &Mesh, id: impl Into<bevy::mesh::MeshVertexAttributeId>) -> &[[f32; 3]] {
    match m.attribute(id) {
        Some(VertexAttributeValues::Float32x3(v)) => v,
        _ => panic!("piece mesh lacks a Float32x3 attribute"),
    }
}

/// Merges `(material, mesh, world transform)` parts into one mesh per material, with vertices
/// relative to `origin` (where the chunk entity sits). Vertex format matches the source pieces.
pub fn merge_by_material(
    parts: &[(usize, &Mesh, Transform)],
    origin: Vec3,
) -> BTreeMap<usize, Mesh> {
    let mut groups: BTreeMap<usize, Vec<(&Mesh, Transform)>> = BTreeMap::new();
    for (mat, mesh, t) in parts {
        groups.entry(*mat).or_default().push((mesh, *t));
    }
    groups
        .into_iter()
        .map(|(mat, items)| {
            let nv: usize = items.iter().map(|(m, _)| m.count_vertices()).sum();
            let ni: usize = items
                .iter()
                .map(|(m, _)| m.indices().map_or(0, Indices::len))
                .sum();
            let mut pos = Vec::with_capacity(nv);
            let mut nrm = Vec::with_capacity(nv);
            let mut uvs = Vec::with_capacity(nv);
            let mut idx: Vec<u32> = Vec::with_capacity(ni);
            for (m, t) in items {
                let base = pos.len() as u32;
                let inv_scale = 1.0 / t.scale;
                pos.extend(
                    attr3(m, Mesh::ATTRIBUTE_POSITION)
                        .iter()
                        .map(|p| (t.transform_point(Vec3::from(*p)) - origin).to_array()),
                );
                nrm.extend(attr3(m, Mesh::ATTRIBUTE_NORMAL).iter().map(|n| {
                    (t.rotation * (Vec3::from(*n) * inv_scale))
                        .normalize_or_zero()
                        .to_array()
                }));
                match m.attribute(Mesh::ATTRIBUTE_UV_0) {
                    Some(VertexAttributeValues::Float32x2(v)) => uvs.extend_from_slice(v),
                    _ => panic!("piece mesh lacks UV_0"),
                }
                idx.extend(
                    m.indices()
                        .expect("indexed piece")
                        .iter()
                        .map(|i| base + i as u32),
                );
            }
            let mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::RENDER_WORLD,
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
            .with_inserted_indices(Indices::U32(idx));
            (mat, mesh)
        })
        .collect()
}

/// Bakes one static kit instance without losing UV1 wind weights or tangent frames.
/// Negative colour alpha marks RGB as the original world-space wind pivot in kit shaders.
/// Callers keep meshes with authored colours on the individual-entity path.
pub fn transformed_kit_mesh(source: &Mesh, world: Transform, origin: Vec3) -> Mesh {
    let mut mesh = source.clone();
    mesh.transform_by(world);
    if let Some(indices) = mesh.indices() {
        let indices = indices.iter().map(|i| i as u32).collect();
        mesh.insert_indices(Indices::U32(indices));
    }
    let pivot = world.translation;
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        vec![[pivot.x, pivot.y, pivot.z, -1.0]; mesh.count_vertices()],
    );
    mesh.translate_by(-origin);
    mesh
}

#[cfg(all(test, feature = "gate-tools"))]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use hex_core::{Axial, HEX_RADIUS, spiral};

    use super::*;
    use crate::scene::{PieceMeshes, tile_pieces};
    use crate::workload::{GATE, piece_material};

    fn tris(m: &Mesh) -> usize {
        m.indices().map_or(0, |i| i.len() / 3)
    }

    #[test]
    fn every_tile_lands_in_exactly_one_compact_chunk() {
        for (size, radius) in [(7, 1), (19, 2)] {
            let cells = spiral(GATE.tiles);
            let chunks = chunk_tiles(&cells, size);
            let mut seen = vec![0; cells.len()];
            for (center, members) in &chunks {
                assert!(members.len() <= size as usize);
                for &ti in members {
                    seen[ti] += 1;
                    assert!(
                        cells[ti].distance(*center) <= radius,
                        "tile {ti} far from chunk"
                    );
                }
            }
            assert!(seen.iter().all(|&n| n == 1), "each tile assigned once");
            // The spiral fills complete rings up to radius 25 (1,951 cells): chunks whose
            // centers sit well inside it are full.
            for (center, members) in &chunks {
                if center.distance(Axial::ORIGIN) + radius <= 25 {
                    assert_eq!(members.len(), size as usize, "interior chunk at {center:?}");
                }
            }
        }
    }

    #[test]
    fn merge_preserves_triangles_and_materials_for_one_chunk_at_each_lod() {
        let mut assets = Assets::<Mesh>::default();
        let m = PieceMeshes::new(&mut assets);
        let cells = spiral(GATE.tiles);
        let (_, members) = &chunk_tiles(&cells, 7)[0];
        assert_eq!(members.len(), 7);

        for lod in 0..3 {
            // Expected: what the entities layout spawns for these tiles at this LOD.
            let mut want: BTreeMap<usize, usize> = BTreeMap::new();
            let mut parts = vec![];
            for &ti in members {
                let [x, z] = cells[ti].to_world(HEX_RADIUS);
                for (pi, (lods, t)) in tile_pieces(&m, ti).into_iter().enumerate() {
                    let mat = piece_material(ti, pi);
                    let mesh = assets.get(&lods[lod]).unwrap();
                    *want.entry(mat).or_default() += tris(mesh);
                    parts.push((mat, mesh, Transform::from_xyz(x, 0.0, z) * t));
                }
            }
            let merged = merge_by_material(&parts, Vec3::ZERO);
            let got: BTreeMap<usize, usize> = merged.iter().map(|(k, v)| (*k, tris(v))).collect();
            assert_eq!(got, want, "LOD{lod}: per-material triangles");
            let total: usize = got.values().sum();
            assert_eq!(total, want.values().sum::<usize>());
            assert!(total > 0);
        }
    }

    #[test]
    fn merge_places_vertices_where_the_entity_would() {
        let mut assets = Assets::<Mesh>::default();
        let m = PieceMeshes::new(&mut assets);
        let (lods, t) = &tile_pieces(&m, 3)[0];
        let mesh = assets.get(&lods[0]).unwrap();
        let world = Transform::from_xyz(100.0, 0.0, -40.0) * *t;
        let origin = Vec3::new(90.0, 0.0, -30.0);
        let merged = merge_by_material(&[(5, mesh, world)], origin);
        let out = &merged[&5];
        let pos = |m: &Mesh| match m.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) => p.clone(),
            _ => panic!("positions"),
        };
        let nrm = |m: &Mesh| match m.attribute(Mesh::ATTRIBUTE_NORMAL) {
            Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) => p.clone(),
            _ => panic!("normals"),
        };
        let mut seen: HashMap<usize, ()> = HashMap::new();
        for (i, (src, dst)) in pos(mesh).iter().zip(pos(out)).enumerate() {
            let want = world.transform_point(Vec3::from(*src)) - origin;
            assert!((Vec3::from(dst) - want).length() < 1e-3, "vertex {i}");
            seen.insert(i, ());
        }
        assert_eq!(seen.len(), mesh.count_vertices());
        for (src, dst) in nrm(mesh).iter().zip(nrm(out)) {
            let n = Vec3::from(dst);
            assert!((n.length() - 1.0).abs() < 1e-3);
            // A normal still points the way the transformed surface faces.
            let approx = (world.rotation * (Vec3::from(*src) / world.scale)).normalize();
            assert!(n.dot(approx) > 0.999);
        }
    }
}
