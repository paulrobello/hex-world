//! P4 runtime ground (PRD §15, elevation Task 7). A sloped tile (a hillside, the road ramp, the
//! tunnel portal) draws no baked base: its cap and skirt are built from `hex_core::slope`'s field
//! ([`ground_mesh`]) and drawn with `meadow_base`'s own cap and skirt slot materials, so it shades
//! like the baked Meadow beside it. A raised tile stands on a cliff column of the skirt's soil
//! down to the level-0 floor, and a ramp's road pieces are draped over the field. Scatter, grass
//! and walkers read the same ground through [`tile_ground_tris`].
//!
//! The meshes carry no `Lod`: `chunks::settle` instances only meshes that `KitFamilies` knows
//! (manifest pieces), so these stay on the per-entity path and draw at every distance.

use std::collections::HashMap;

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
};
use hex_core::{
    Ground, HEX_RADIUS, LEVEL_STEP, PrismMesh, SocketKind, TileCatalog, TileDef, cap_tris,
    field_at, ground_mesh,
};

use super::{
    KitMaterial, KitPart,
    scatter::{GROUND_PROPS, Tri, ground_pieces, turn},
};

/// Cap rings of the runtime ground (1.33 m between rings), shared by the drawn cap and the
/// triangles scatter, grass and walkers stand on.
pub const RINGS: u32 = 12;
/// The piece whose slot materials the runtime ground and the cliff columns draw with.
pub const GROUND_PIECE: &str = "meadow_base";
pub const CAP_SLOT: &str = "meadow_base_cap";
pub const SKIRT_SLOT: &str = "meadow_base_skirt";
/// The baked skirt's constant UV (`slope::SKIRT_UV`): the soil set is sampled in world space.
const WALL_UV: [f32; 2] = [0.005, 0.005];
/// How far the column reaches up into the baked skirt (m): the skirt's bottom edge has more
/// vertices than the column's top, so a shared edge could open hairline cracks; the overlap is
/// coplanar soil in the same material.
pub const COLUMN_OVERLAP: f32 = 0.05;
/// Finite-difference step of the field gradient a drape tilts normals by (m).
const GRAD_EPS: f32 = 0.01;

/// Whether `def`'s centre and lane-edge pieces follow its field: the road ramp. A cutting's
/// corridor is flat at the tile's level, so draping it would change nothing.
pub fn drapes(def: &TileDef) -> bool {
    def.ground == Ground::RoadRamp
}

/// The ids `def` drapes: its centre and its road and river lane-edge pieces, none on a def that
/// does not drape ([`drapes`]). The tunnel portal is a `Cutting`, so none of its pieces (its hero
/// included) is draped; a [`GROUND_PROPS`] hero is kept out of the list on any def.
pub fn draped_pieces<'a>(def: &'a TileDef, catalog: &'a TileCatalog) -> Vec<&'a str> {
    if !drapes(def) {
        return vec![];
    }
    let mut out: Vec<&str> = def.center.iter().map(|c| c.as_str()).collect();
    let pieces = def.edge_pieces(catalog.sockets());
    for (s, piece) in def.edges.iter().zip(pieces) {
        if s.lane.is_some()
            && matches!(s.kind, SocketKind::Road | SocketKind::River)
            && let Some(p) = piece
        {
            out.push(p.as_str());
        }
    }
    out.retain(|p| !GROUND_PROPS.contains(p));
    out
}

fn mesh_of(p: PrismMesh, tangents: Vec<[f32; 4]>) -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, p.positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, p.normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, p.uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, tangents)
    .with_inserted_indices(Indices::U32(p.indices))
}

/// The tangent of the cap's planar mapping (u along +x, v along −z, `w` 1): +x projected onto the
/// surface, which is what mikktspace gives the baked cap.
fn cap_tangent(n: [f32; 3]) -> [f32; 4] {
    let n = Vec3::from(n);
    let t = (Vec3::X - n * n.x).normalize_or(Vec3::X);
    [t.x, t.y, t.z, 1.0]
}

/// A wall's tangent: horizontal along the wall, `w` 1 (its UV is constant).
fn wall_tangent(n: [f32; 3]) -> [f32; 4] {
    [-n[2], 0.0, n[0], 1.0]
}

/// The runtime cap and skirt of a sloped `def` ([`ground_mesh`] at [`RINGS`]), tile-local.
pub fn cap_and_skirt(def: &TileDef) -> (Mesh, Mesh) {
    let g = ground_mesh(def.ground, def.corners(0), RINGS);
    let cap_t = g.cap.normals.iter().map(|n| cap_tangent(*n)).collect();
    let skirt_t = g.skirt.normals.iter().map(|n| wall_tangent(*n)).collect();
    (mesh_of(g.cap, cap_t), mesh_of(g.skirt, skirt_t))
}

/// A raised tile's cliff column: the hex wall below the baked skirt, from [`COLUMN_OVERLAP`] above
/// y = −`LEVEL_STEP` down to `depth` metres below it, tile-local, outward normals and the skirt's
/// constant UV, so with the skirt it reaches the level-0 floor.
pub fn column_mesh(depth: f32) -> Mesh {
    let (top, bottom) = (-LEVEL_STEP + COLUMN_OVERLAP, -LEVEL_STEP - depth);
    let corner = |k: u32| {
        let a = (30.0 + 60.0 * k as f32).to_radians();
        [HEX_RADIUS * a.cos(), HEX_RADIUS * a.sin()]
    };
    let mut p = PrismMesh {
        positions: vec![],
        normals: vec![],
        uvs: vec![],
        indices: vec![],
    };
    for k in 0..6u32 {
        // Side k runs from corner k to corner k + 1; its outward normal is at 60° · (k + 1).
        let a = (60.0 * (k + 1) as f32).to_radians();
        let n = [a.cos(), 0.0, a.sin()];
        let base = p.positions.len() as u32;
        for c in [corner(k), corner(k + 1)] {
            p.positions.push([c[0], top, c[1]]);
            p.positions.push([c[0], bottom, c[1]]);
            p.normals.extend_from_slice(&[n, n]);
            p.uvs.extend_from_slice(&[WALL_UV, WALL_UV]);
        }
        let (t0, b0, t1, b1) = (base, base + 1, base + 2, base + 3);
        // The skirt's winding (`slope::ground_mesh`): front faces look outward.
        p.indices.extend_from_slice(&[b0, t1, b1, b0, t0, t1]);
    }
    let tangents = p.normals.iter().map(|n| wall_tangent(*n)).collect();
    mesh_of(p, tangents)
}

/// The field at (x, z) and its gradient (d/dx, d/dz), tile frame.
fn field_grad(ground: Ground, corners: [u8; 6], x: f32, z: f32) -> (f32, f32, f32) {
    let f = |x: f32, z: f32| field_at(ground, corners, x, z);
    let gx = (f(x + GRAD_EPS, z) - f(x - GRAD_EPS, z)) / (2.0 * GRAD_EPS);
    let gz = (f(x, z + GRAD_EPS) - f(x, z - GRAD_EPS)) / (2.0 * GRAD_EPS);
    (f(x, z), gx, gz)
}

/// `mesh` with [`field_at`] added to every vertex y and its normals and tangents carried through
/// the shear: each normal tilts by the field's gradient at its vertex, so a kerb's hard edges
/// stay hard.
pub fn drape(mesh: &Mesh, ground: Ground, corners: [u8; 6]) -> Mesh {
    drape_turned(mesh, ground, corners, 0)
}

/// [`drape`] for a piece drawn turned `k` steps (`piece_rotation(k)`, a lane-edge piece): the
/// field is sampled with each vertex turned into the tile frame.
pub fn drape_turned(mesh: &Mesh, ground: Ground, corners: [u8; 6], k: u8) -> Mesh {
    let mut out = Mesh::new(mesh.primitive_topology(), RenderAssetUsages::default());
    for (attr, values) in mesh.attributes() {
        out.insert_attribute(*attr, values.clone());
    }
    if let Some(i) = mesh.indices() {
        out.insert_indices(i.clone());
    }
    let back = (6 - k % 6) % 6;
    let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return out;
    };
    // Per vertex: the field height and its gradient in the piece's own frame.
    let fields: Vec<(f32, f32, f32)> = pos
        .iter()
        .map(|p| {
            let q = turn(k, *p);
            let (h, gx, gz) = field_grad(ground, corners, q[0], q[2]);
            let g = turn(back, [gx, 0.0, gz]);
            (h, g[0], g[2])
        })
        .collect();
    let positions: Vec<[f32; 3]> = pos
        .iter()
        .zip(&fields)
        .map(|(p, (h, ..))| [p[0], p[1] + h, p[2]])
        .collect();
    out.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    if let Some(VertexAttributeValues::Float32x3(ns)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        let normals: Vec<[f32; 3]> = ns
            .iter()
            .zip(&fields)
            .map(|(n, (_, fx, fz))| {
                Vec3::new(n[0] - fx * n[1], n[1], n[2] - fz * n[1])
                    .normalize_or(Vec3::Y)
                    .to_array()
            })
            .collect();
        out.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    }
    if let Some(VertexAttributeValues::Float32x4(ts)) = mesh.attribute(Mesh::ATTRIBUTE_TANGENT) {
        let tangents: Vec<[f32; 4]> = ts
            .iter()
            .zip(&fields)
            .map(|(t, (_, fx, fz))| {
                let v = Vec3::new(t[0], t[1] + fx * t[0] + fz * t[2], t[2]).normalize_or(Vec3::X);
                [v.x, v.y, v.z, t[3]]
            })
            .collect();
        out.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
    }
    out
}

/// A tile definition's ground triangles in the tile frame with their slot names: each ground
/// piece's LOD0 triangles (`tris_of`, piece-local) turned into place, except that a sloped def's
/// base is the runtime cap ([`cap_tris`] at [`RINGS`], slot [`CAP_SLOT`]) and a ramp's centre and
/// lane-edge pieces are draped over its field. `None` while a piece other than a replaced base
/// has no triangles.
pub fn tile_ground_tris(
    def: &TileDef,
    catalog: &TileCatalog,
    tris_of: &mut dyn FnMut(&str) -> Option<Vec<(Tri, String)>>,
) -> Option<Vec<(Tri, String)>> {
    let sloped = def.is_sloped();
    let corners = def.corners(0);
    let draped = draped_pieces(def, catalog);
    let mut all = vec![];
    for (i, (id, k)) in ground_pieces(def, catalog).into_iter().enumerate() {
        if sloped && i == 0 {
            all.extend(
                cap_tris(def.ground, corners, RINGS)
                    .into_iter()
                    .map(|t| (t, CAP_SLOT.to_owned())),
            );
            continue;
        }
        let drape = draped.contains(&id);
        for (t, s) in tris_of(id)? {
            let t = t.map(|p| {
                let q = turn(k, p);
                if drape {
                    [q[0], q[1] + field_at(def.ground, corners, q[0], q[2]), q[2]]
                } else {
                    q
                }
            });
            all.push((t, s));
        }
    }
    Some(all)
}

/// The runtime ground's meshes, built once per shape and shared by every tile and the ghost.
#[derive(Resource, Default)]
pub struct SlopeMeshes {
    ground: HashMap<(Ground, [u8; 6]), (Handle<Mesh>, Handle<Mesh>)>,
    columns: HashMap<u8, Handle<Mesh>>,
    draped: HashMap<(AssetId<Mesh>, Ground, [u8; 6], u8), Handle<Mesh>>,
}

impl SlopeMeshes {
    /// The (cap, skirt) of a sloped `def`.
    pub fn ground(
        &mut self,
        def: &TileDef,
        meshes: &mut Assets<Mesh>,
    ) -> (Handle<Mesh>, Handle<Mesh>) {
        self.ground
            .entry((def.ground, def.corners(0)))
            .or_insert_with(|| {
                let (cap, skirt) = cap_and_skirt(def);
                (meshes.add(cap), meshes.add(skirt))
            })
            .clone()
    }

    /// The cliff column under a tile at `level` (> 0).
    pub fn column(&mut self, level: u8, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        self.columns
            .entry(level)
            .or_insert_with(|| meshes.add(column_mesh(f32::from(level) * LEVEL_STEP)))
            .clone()
    }

    /// `src` draped over `def`'s field for a piece turned `k` steps; `src` itself while its mesh is
    /// not loaded.
    pub fn draped(
        &mut self,
        src: &Handle<Mesh>,
        def: &TileDef,
        k: u8,
        meshes: &mut Assets<Mesh>,
    ) -> Handle<Mesh> {
        let key = (src.id(), def.ground, def.corners(0), k % 6);
        if let Some(h) = self.draped.get(&key) {
            return h.clone();
        }
        let Some(mesh) = meshes.get(src) else {
            // Kit meshes are loaded before their piece is built, so this is not expected.
            warn!(
                "slope ground: mesh {:?} not loaded, drawn un-draped on {}",
                src.id(),
                def.id
            );
            return src.clone();
        };
        let h = meshes.add(drape_turned(mesh, def.ground, def.corners(0), k));
        self.draped.insert(key, h.clone());
        h
    }
}

/// The LOD0 material of `slot` on [`GROUND_PIECE`], once its pack is in the kit.
pub fn slot_material(
    kit: &HashMap<String, Vec<KitPart>>,
    slot: &str,
) -> Option<Handle<KitMaterial>> {
    kit.get(GROUND_PIECE)?
        .iter()
        .find(|p| p.lod == 0 && p.rotor.is_none() && p.slot == slot)
        .map(|p| p.material.clone())
}

/// What tile spawning needs to build runtime ground: the mesh cache and the mesh assets.
pub struct GroundBuild<'a> {
    pub cache: &'a mut SlopeMeshes,
    pub meshes: &'a mut Assets<Mesh>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ys(m: &Mesh) -> (f32, f32) {
        let Some(VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions missing");
        };
        p.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| {
            (lo.min(v[1]), hi.max(v[1]))
        })
    }

    #[test]
    fn column_reaches_the_floor() {
        let m = column_mesh(8.0);
        let (lo, hi) = ys(&m);
        assert!((lo + 12.0).abs() < 1e-5, "min y {lo}");
        assert!((hi + 4.0 - COLUMN_OVERLAP).abs() < 1e-5, "max y {hi}");
        assert!(hi > -LEVEL_STEP, "the column overlaps the skirt");
        // Every face looks outward: its winding normal agrees with its vertex normal, which
        // points away from the tile centre.
        let Some(VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            unreachable!()
        };
        let Some(VertexAttributeValues::Float32x3(n)) = m.attribute(Mesh::ATTRIBUTE_NORMAL) else {
            panic!("normals missing")
        };
        let idx: Vec<usize> = m.indices().unwrap().iter().collect();
        assert_eq!(idx.len(), 36);
        for t in idx.as_chunks::<3>().0 {
            let [a, b, c] = t.map(|i| Vec3::from(p[i]));
            let face = (b - a).cross(c - a).normalize();
            let vn = Vec3::from(n[t[0]]);
            assert!(face.dot(vn) > 0.99, "{face} vs {vn}");
            let mid = (a + b + c) / 3.0;
            assert!(Vec2::new(mid.x, mid.z).dot(Vec2::new(vn.x, vn.z)) > 0.0);
        }
        assert!(m.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
    }

    /// A flat 3×3 grid of points over the ramp, its normals up and tangents along +x.
    fn grid() -> Mesh {
        let mut pos = vec![];
        for i in 0..3 {
            for j in 0..3 {
                pos.push([-8.0 + 8.0 * i as f32, 0.1, -6.0 + 6.0 * j as f32]);
            }
        }
        let n = pos.len();
        let mut idx = vec![];
        for i in 0..2u32 {
            for j in 0..2u32 {
                let a = i * 3 + j;
                idx.extend_from_slice(&[a, a + 1, a + 3, a + 1, a + 4, a + 3]);
            }
        }
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0, 0.0, 0.0, 1.0]; n])
        .with_inserted_indices(Indices::U32(idx))
    }

    #[test]
    fn drape_follows_the_field() {
        let corners = [1, 1, 0, 0, 0, 1];
        let src = grid();
        let m = drape(&src, Ground::RoadRamp, corners);
        let Some(VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions missing")
        };
        let Some(VertexAttributeValues::Float32x3(n)) = m.attribute(Mesh::ATTRIBUTE_NORMAL) else {
            panic!("normals missing")
        };
        let Some(VertexAttributeValues::Float32x4(t)) = m.attribute(Mesh::ATTRIBUTE_TANGENT) else {
            panic!("tangents missing")
        };
        assert_eq!(p.len(), 9);
        let mut rises = 0;
        for i in 0..9 {
            let [x, y, z] = p[i];
            let want = 0.1 + field_at(Ground::RoadRamp, corners, x, z);
            assert!((y - want).abs() < 1e-4, "({x}, {z}): {y} vs {want}");
            // The normal is the field's surface normal; the tangent lies in the surface.
            let (_, gx, gz) = field_grad(Ground::RoadRamp, corners, x, z);
            let want_n = Vec3::new(-gx, 1.0, -gz).normalize();
            assert!(
                Vec3::from(n[i]).dot(want_n) > 1.0 - 1e-5,
                "normal at ({x}, {z})"
            );
            assert!(Vec3::new(t[i][0], t[i][1], t[i][2]).dot(want_n).abs() < 1e-4);
            rises += usize::from(gx > 0.05);
        }
        assert!(rises > 0, "the grid sees the ramp's grade");
        assert_eq!(m.indices().unwrap().len(), src.indices().unwrap().len());
        // A piece turned half way round samples the field turned with it.
        let half = drape_turned(&src, Ground::RoadRamp, corners, 3);
        let Some(VertexAttributeValues::Float32x3(ph)) = half.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions missing")
        };
        for v in ph {
            let q = turn(3, *v);
            let want = 0.1 + field_at(Ground::RoadRamp, corners, q[0], q[2]);
            assert!((v[1] - want).abs() < 1e-4);
        }
    }

    /// The runtime cap's planar UVs and tangents match the baked cap's mapping, and the skirt
    /// hangs from the cap's rim to the column's top.
    #[test]
    fn cap_and_skirt_meet_the_column() {
        let c = TileCatalog::builtin().unwrap();
        let def = c.get("hillside_000111").unwrap();
        let (cap, skirt) = cap_and_skirt(def);
        for m in [&cap, &skirt] {
            for a in [
                Mesh::ATTRIBUTE_POSITION,
                Mesh::ATTRIBUTE_NORMAL,
                Mesh::ATTRIBUTE_UV_0,
                Mesh::ATTRIBUTE_TANGENT,
            ] {
                assert!(m.attribute(a).is_some(), "{a:?}");
            }
        }
        // The meadow relief carves hollows down to -0.25 m; the corners stay exact.
        let (lo, hi) = ys(&cap);
        assert!(
            lo > -0.26 && (hi - LEVEL_STEP).abs() < 1e-3,
            "cap {lo}..{hi}"
        );
        let (lo, _) = ys(&skirt);
        assert!((lo + LEVEL_STEP).abs() < 1e-5);
    }
}
