//! `scenario=water`: the hero-water look-dev (PRD §9.1, P2). One ocean, one river with a bend, a
//! beach and hex joints, on placeholder terrain, lit with the kit look-dev (ACES, EV100 12.5, noon
//! IBL, the HDRI sun).
//!
//! Water and terrain are separate hex meshes per cell, each spawned with its own rotation, so the
//! shaders must be continuous across joints and rotations: every wave, flow and foam term is a
//! function of world position, and per-vertex inputs (flow, depth, shore distance) are computed
//! from world position too.
//!
//! The mesh builders take a [`WaterField`], the world-space description they sample: [`Lookdev`]
//! is this patch's hand-laid scene, and [`tiles`] builds one per placed water tile in `play`.
//!
//! Both materials are custom `Material`s, not `ExtendedMaterial<StandardMaterial, _>`: the
//! StandardMaterial layout binds its texture slots even when unused, and they keep their own
//! material group to one texture. Refracting tiers put `ScreenSpaceTransmission` on the camera,
//! which adds the view transmission texture to every mesh pipeline in the view: the kit forward
//! pipeline then samples 16 textures (15 with the IBL alone), exactly WebGPU's default limit, so a
//! new view or kit texture binding needs headroom found first (`water_kit` smoke case).
use bevy::{
    asset::RenderAssetUsages,
    core_pipeline::prepass::DepthPrepass,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    light::NotShadowCaster,
    mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology},
    pbr::{MaterialPipeline, MaterialPipelineKey, ScreenSpaceTransmission},
    prelude::*,
    render::render_resource::{
        AsBindGroup, Extent3d, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
        TextureDimension, TextureFormat,
    },
    shader::ShaderRef,
};
use hex_core::{Axial, HEX_RADIUS};

use crate::{SceneEntity, post::QualityTier};

pub mod tiles;

pub const WATER_SHADER: &str = "shaders/water.wesl";
pub const TERRAIN_SHADER: &str = "shaders/water_terrain.wesl";

/// Water surface height (m).
pub const SEA_LEVEL: f32 = 0.0;
/// Grass plateau height above the water (m). The terrain shader's grass line (0.82-1.08 m above
/// the water) and wet band (0.32-0.40 m) are tuned to it.
pub const LAND: f32 = 1.4;
/// Hex grid subdivisions per sector edge: 0.5 m vertex spacing at the 16 m radius.
pub const GRID: u32 = 32;
/// Peak shoreline surge (m); shared with the terrain's wet band through the shader library.
pub const WASH_AMP: f32 = 0.22;

/// Cells of the look-dev patch, with the rotation each cell's meshes are spawned at.
pub fn layout() -> Vec<(Axial, u8)> {
    [
        (0, -1),
        (1, -1),
        (2, -1),
        (3, -1),
        (-1, 0),
        (0, 0),
        (1, 0),
        (2, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
        (2, 1),
    ]
    .into_iter()
    .map(|(q, r): (i32, i32)| {
        let rot = (q * 2 + r * 5).rem_euclid(6) as u8;
        (Axial::new(q, r), rot)
    })
    .collect()
}

/// Signed distance from the coast along +x (negative inland).
pub fn shore_distance(x: f32, z: f32) -> f32 {
    x - (30.0 + 3.5 * (z / 10.0).sin())
}

/// The river's centre line, inland to the mouth; the bend sits around (-10, 0).
const RIVER: [Vec2; 8] = [
    Vec2::new(-52.0, -14.0),
    Vec2::new(-34.0, -15.0),
    Vec2::new(-20.0, -10.0),
    Vec2::new(-10.0, 1.0),
    Vec2::new(0.0, 9.0),
    Vec2::new(12.0, 11.0),
    Vec2::new(24.0, 9.0),
    Vec2::new(40.0, 6.0),
];

/// River half-width (m), widening toward the mouth.
fn river_half_width(along: f32) -> f32 {
    3.0 + 2.0 * ((along - 70.0) / 30.0).clamp(0.0, 1.0)
}

/// Peak surface speed on the river's centre line (m/s).
pub const RIVER_SPEED: f32 = 1.1;

#[derive(Clone, Copy, Debug)]
pub struct RiverSample {
    /// Distance to the centre line (m).
    pub dist: f32,
    /// Arc length of the nearest point from the source (m).
    pub along: f32,
    /// Unit downstream tangent at the nearest point.
    pub tangent: Vec2,
}

/// The centre line as a Catmull-Rom curve through `RIVER`, sampled every ~1 m.
fn river_curve() -> &'static [Vec2] {
    static CURVE: std::sync::OnceLock<Vec<Vec2>> = std::sync::OnceLock::new();
    CURVE.get_or_init(|| {
        let n = RIVER.len();
        let at = |i: isize| RIVER[i.clamp(0, n as isize - 1) as usize];
        let mut out = vec![];
        for i in 0..n as isize - 1 {
            let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
            let steps = (p1.distance(p2).ceil() as usize).max(1);
            for s in 0..steps {
                let t = s as f32 / steps as f32;
                let (t2, t3) = (t * t, t * t * t);
                out.push(
                    0.5 * ((2.0 * p1)
                        + (p2 - p0) * t
                        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
                        + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3),
                );
            }
        }
        out.push(RIVER[n - 1]);
        out
    })
}

pub fn river_sample(p: Vec2) -> RiverSample {
    let mut best = RiverSample {
        dist: f32::MAX,
        along: 0.0,
        tangent: Vec2::X,
    };
    let mut start = 0.0;
    for w in river_curve().windows(2) {
        let (a, b) = (w[0], w[1]);
        let seg = b - a;
        let len = seg.length();
        let t = ((p - a).dot(seg) / (len * len)).clamp(0.0, 1.0);
        let d = p.distance(a + seg * t);
        if d < best.dist {
            best = RiverSample {
                dist: d,
                along: start + t * len,
                tangent: seg / len,
            };
        }
        start += len;
    }
    best
}

pub(crate) fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Cuts a river channel into ground height `h` (relative to the water surface) at `dist` from the
/// centre line: a parabolic bed 1.55 m below the water on the centre line (1.5 m bowl plus 0.05 m),
/// blended back to `h` by a bank running from `0.7 w` to `w + 2.5` m. Never raises the ground.
pub fn carve(h: f32, dist: f32, w: f32) -> f32 {
    let bed = -1.5 * (1.0 - (dist / w).powi(2)).max(0.0) - 0.05;
    let bank = smoothstep(w * 0.7, w + 2.5, dist);
    let carved = bed + (h - bed) * bank;
    if carved < h { carved } else { h }
}

/// River inputs at `dist` from the centre line of a channel cut with half-width `w`: the river
/// weight (1 in the channel, gone 3 m past `w`) and the surface speed (m/s).
pub fn river_flow(dist: f32, w: f32) -> (f32, f32) {
    let river = 1.0 - smoothstep(w, w + 3.0, dist);
    let speed = RIVER_SPEED * (1.0 - (dist / (w + 1.0)).powi(2)).max(0.0);
    (river, speed)
}

/// Terrain COLOR (land, river, shore distance, 1) from the river distance past the channel's
/// half-width and the shore distance.
pub fn terrain_color(past_channel: f32, shore: f32) -> [f32; 4] {
    let river = 1.0 - smoothstep(4.0, 9.0, past_channel);
    let beach = (1.0 - river) * (1.0 - smoothstep(-14.0, -9.0, shore));
    [1.0 - beach - river, river, shore, 1.0]
}

/// Terrain height (m) at world (x, z): grass plateau, beach slope, shelving sea bed, and the
/// river channel carved through them.
pub fn height(x: f32, z: f32) -> f32 {
    let d = shore_distance(x, z);
    let h = if d < -12.0 {
        LAND + 0.12 * ((x * 0.21).sin() * (z * 0.17).cos()) * smoothstep(-12.0, -22.0, d)
    } else if d < 0.0 {
        // Beach: LAND over 12 m, flattening into the plateau.
        let t = -d / 12.0;
        LAND * t * (2.0 - t)
    } else if d < 8.0 {
        -0.12 * d
    } else {
        (-0.96 - 0.26 * (d - 8.0)).max(-6.0)
    };
    let r = river_sample(Vec2::new(x, z));
    carve(h, r.dist, river_half_width(r.along))
}

/// Per-vertex water inputs, all functions of world position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterSample {
    /// Surface flow (m/s) on the XZ plane.
    pub flow: Vec2,
    /// 1 in the river, 0 on the open sea.
    pub river: f32,
    /// Water depth above the bed at rest (m), 0 under land.
    pub depth: f32,
    /// `shore_distance`.
    pub shore: f32,
}

pub fn water_sample(x: f32, z: f32) -> WaterSample {
    let p = Vec2::new(x, z);
    let r = river_sample(p);
    let w = river_half_width(r.along);
    let shore = shore_distance(x, z);
    let (in_channel, speed) = river_flow(r.dist, w);
    // The river becomes sea over the 14 m past the coast.
    let river = in_channel * (1.0 - smoothstep(-2.0, 12.0, shore));
    WaterSample {
        flow: r.tangent * speed * river,
        river,
        depth: (SEA_LEVEL - height(x, z)).max(0.0),
        shore,
    }
}

/// The world a set of water meshes samples, everything a function of world XZ so neighbouring
/// meshes agree wherever they share a point.
pub trait WaterField {
    /// Rest height of the water surface (m).
    fn level(&self) -> f32;
    /// Terrain (bed) height (m).
    fn height(&self, x: f32, z: f32) -> f32;
    /// Per-vertex water inputs.
    fn water(&self, x: f32, z: f32) -> WaterSample;
    /// Terrain vertex COLOR: (land, river, shore distance, 1).
    fn terrain(&self, x: f32, z: f32) -> [f32; 4];
}

/// The look-dev patch: [`height`], [`water_sample`] and the prototype's river and coast.
pub struct Lookdev;

impl WaterField for Lookdev {
    fn level(&self) -> f32 {
        SEA_LEVEL
    }

    fn height(&self, x: f32, z: f32) -> f32 {
        height(x, z)
    }

    fn water(&self, x: f32, z: f32) -> WaterSample {
        water_sample(x, z)
    }

    fn terrain(&self, x: f32, z: f32) -> [f32; 4] {
        let s = water_sample(x, z);
        let r = river_sample(Vec2::new(x, z));
        terrain_color(r.dist - river_half_width(r.along), s.shore)
    }
}

/// Pointy-top hex of `radius`, each of the six sectors split into `n²` triangles. Returns local
/// positions (y = 0) and counter-clockwise (from +Y) triangles. Sector borders duplicate their
/// vertices, so values are always recomputed from position, never shared by index.
pub fn hex_grid(n: u32, radius: f32) -> (Vec<Vec3>, Vec<u32>) {
    let corner = |i: u32| {
        let a = (30.0 + 60.0 * i as f32).to_radians();
        Vec3::new(radius * a.cos(), 0.0, radius * a.sin())
    };
    let mut pos = vec![];
    let mut idx = vec![];
    for s in 0..6 {
        let (ca, cb) = (corner(s), corner((s + 1) % 6));
        let base = pos.len() as u32;
        for i in 0..=n {
            for j in 0..=(n - i) {
                pos.push((ca * i as f32 + cb * j as f32) / n as f32);
            }
        }
        // Row i holds n + 1 - i points.
        let mut row_start = vec![base];
        for i in 0..n {
            let prev = row_start[i as usize];
            row_start.push(prev + (n + 1 - i));
        }
        let at = |i: u32, j: u32| row_start[i as usize] + j;
        for i in 0..n {
            for j in 0..(n - i) {
                idx.extend([at(i, j), at(i + 1, j), at(i, j + 1)]);
                if i + j + 1 < n {
                    idx.extend([at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)]);
                }
            }
        }
    }
    // Wind every triangle counter-clockwise seen from +Y (front face up).
    for t in idx.chunks_mut(3) {
        let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
        if (b - a).cross(c - a).y < 0.0 {
            t.swap(1, 2);
        }
    }
    (pos, idx)
}

pub(crate) fn cell_transform(cell: Axial, rot: u8) -> Transform {
    let [x, z] = cell.to_world(HEX_RADIUS);
    Transform::from_xyz(x, 0.0, z).with_rotation(crate::kit_scene::piece_rotation(rot))
}

/// One cell's water surface in cell-local space at the field's level: COLOR carries (flow.x,
/// flow.z, river, depth) and UV_0 carries (shore distance, 0 = surface), sampled at the vertex's
/// world position.
pub fn surface_mesh(field: &impl WaterField, cell: Axial, rot: u8) -> Mesh {
    let (local, idx) = hex_grid(GRID, HEX_RADIUS);
    let t = cell_transform(cell, rot);
    let level = field.level();
    let mut colors = Vec::with_capacity(local.len());
    let mut uvs = Vec::with_capacity(local.len());
    for p in &local {
        let w = t.transform_point(*p);
        let s = field.water(w.x, w.z);
        colors.push([s.flow.x, s.flow.y, s.river, s.depth]);
        uvs.push([s.shore, 0.0]);
    }
    let n = local.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        local
            .iter()
            .map(|p| p.with_y(level).to_array())
            .collect::<Vec<_>>(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(idx))
}

/// The look-dev patch's surface for one cell.
pub fn water_mesh(cell: Axial, rot: u8) -> Mesh {
    surface_mesh(&Lookdev, cell, rot)
}

/// A water strip at the field's level, positions relative to `origin` (world XZ, unrotated). Each
/// row is a centre point and a unit lateral axis in world XZ; the strip spans `half` metres either
/// side in `cols` steps. Attributes as in [`surface_mesh`]. River surfaces use it: a row on a tile
/// edge lies along that edge, so two strips meeting there share their end rows' points.
pub fn ribbon_mesh(
    field: &impl WaterField,
    origin: Vec2,
    rows: &[(Vec2, Vec2)],
    half: f32,
    cols: u32,
) -> Mesh {
    let level = field.level();
    let (mut pos, mut colors, mut uvs, mut idx) = (vec![], vec![], vec![], vec![]);
    for &(c, lat) in rows {
        for j in 0..=cols {
            let w = c + lat * (half * (2.0 * j as f32 / cols as f32 - 1.0));
            let s = field.water(w.x, w.y);
            pos.push([w.x - origin.x, level, w.y - origin.y]);
            colors.push([s.flow.x, s.flow.y, s.river, s.depth]);
            uvs.push([s.shore, 0.0]);
        }
    }
    let stride = cols + 1;
    for i in 0..rows.len().saturating_sub(1) as u32 {
        for j in 0..cols {
            let (a, b) = (i * stride + j, i * stride + j + 1);
            let (c, d) = (a + stride, b + stride);
            for mut t in [[a, c, b], [b, c, d]] {
                let [p0, p1, p2] = t.map(|k| Vec3::from(pos[k as usize]));
                if (p1 - p0).cross(p2 - p0).y < 0.0 {
                    t.swap(1, 2);
                }
                idx.extend(t);
            }
        }
    }
    let n = pos.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(idx))
}

/// Edges of `cell` (0..6, edge k faces 60k degrees) with no water cell beside them.
pub fn exposed_edges(cell: Axial) -> Vec<u8> {
    let cells: Vec<Axial> = layout().into_iter().map(|(c, _)| c).collect();
    (0..6)
        .filter(|&k| !cells.contains(&cell.neighbor(k)))
        .collect()
}

/// UV_0.y offset marking a skirt vertex; the terrain height there is `uv.y - SKIRT_TAG`. The
/// shader tells skirt from surface by `uv.y > 0.5`, so beds must stay above -9.5 m.
pub const SKIRT_TAG: f32 = 10.0;

/// The cut faces of the water volume on `edges` of `cell` (world edges), in cell-centred
/// (unrotated) space: one quad strip per edge from the rest surface (the field's level: displaced
/// in the vertex stage exactly like the surface rim it shares positions with) down to the bed (left
/// in place). Every water feature crossing the edge (sea, beach shallows, river channel) gets its
/// cross-section; the shader discards wherever the face is below the terrain, so it never pokes
/// out of a bank. UV_0 carries (shore distance, SKIRT_TAG + terrain height). `None` without edges.
pub fn skirt_mesh_for(field: &impl WaterField, cell: Axial, edges: &[u8]) -> Option<Mesh> {
    if edges.is_empty() {
        return None;
    }
    let level = field.level();
    let [cx, cz] = cell.to_world(HEX_RADIUS);
    let corner = |deg: f32| {
        let a = deg.to_radians();
        Vec3::new(HEX_RADIUS * a.cos(), 0.0, HEX_RADIUS * a.sin())
    };
    let (mut pos, mut nrm, mut uvs, mut col, mut idx) = (vec![], vec![], vec![], vec![], vec![]);
    for &k in edges {
        let mid = 60.0 * k as f32;
        let (ca, cb) = (corner(mid - 30.0), corner(mid + 30.0));
        let out = ((ca + cb) * 0.5).normalize();
        let base = pos.len() as u32;
        for i in 0..=GRID {
            let p = ca.lerp(cb, i as f32 / GRID as f32);
            let (wx, wz) = (p.x + cx, p.z + cz);
            let s = field.water(wx, wz);
            let ground = field.height(wx, wz);
            for y in [level, ground.min(level)] {
                pos.push([p.x, y, p.z]);
                nrm.push(out.to_array());
                uvs.push([s.shore, SKIRT_TAG + ground]);
                col.push([s.flow.x, s.flow.y, s.river, s.depth]);
            }
        }
        for i in 0..GRID {
            let (t0, b0, t1, b1) = (
                base + 2 * i,
                base + 2 * i + 1,
                base + 2 * i + 2,
                base + 2 * i + 3,
            );
            let (p0, p1, p2) = (
                Vec3::from(pos[t0 as usize]),
                Vec3::from(pos[b0 as usize]),
                Vec3::from(pos[t1 as usize]),
            );
            if (p1 - p0).cross(p2 - p0).dot(out) > 0.0 {
                idx.extend([t0, b0, t1, t1, b0, b1]);
            } else {
                idx.extend([t0, t1, b0, t1, b1, b0]);
            }
        }
    }
    Some(
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx)),
    )
}

/// The look-dev patch's cut faces for `cell`'s outer edges.
pub fn skirt_mesh(cell: Axial) -> Option<Mesh> {
    skirt_mesh_for(&Lookdev, cell, &exposed_edges(cell))
}

fn height_normal(field: &impl WaterField, x: f32, z: f32) -> Vec3 {
    let e = 0.25;
    let dx = field.height(x + e, z) - field.height(x - e, z);
    let dz = field.height(x, z + e) - field.height(x, z - e);
    Vec3::new(-dx, 2.0 * e, -dz).normalize()
}

/// Skirt depth below sea level for the patch's outer walls (m).
const SKIRT_FLOOR: f32 = -7.0;

/// One cell's terrain in cell-local space, heights and normals from the field, plus a vertical
/// skirt on every edge down to `floor` (hidden where a neighbour exists). COLOR carries
/// [`WaterField::terrain`] with w = 1 on the cap and 0 on the skirt.
pub fn terrain_mesh_for(field: &impl WaterField, cell: Axial, rot: u8, floor: f32) -> Mesh {
    let (mut local, mut idx) = hex_grid(GRID, HEX_RADIUS);
    let t = cell_transform(cell, rot);
    let inv = t.rotation.inverse();
    let mut normals = Vec::with_capacity(local.len());
    let mut colors = Vec::with_capacity(local.len());
    for p in local.iter_mut() {
        let w = t.transform_point(*p);
        p.y = field.height(w.x, w.z);
        normals.push((inv * height_normal(field, w.x, w.z)).to_array());
        colors.push(field.terrain(w.x, w.z));
    }
    // Skirt: the hex rim's top vertices down to `floor`, with outward normals.
    let corner = |i: u32| {
        let a = (30.0 + 60.0 * i as f32).to_radians();
        Vec3::new(HEX_RADIUS * a.cos(), 0.0, HEX_RADIUS * a.sin())
    };
    for s in 0..6 {
        let (ca, cb) = (corner(s), corner((s + 1) % 6));
        let out = ((ca + cb) * 0.5).normalize();
        for k in 0..GRID {
            let a = ca.lerp(cb, k as f32 / GRID as f32);
            let b = ca.lerp(cb, (k + 1) as f32 / GRID as f32);
            let (wa, wb) = (t.transform_point(a), t.transform_point(b));
            let ha = field.height(wa.x, wa.z);
            let hb = field.height(wb.x, wb.z);
            let base = local.len() as u32;
            for (p, w) in [
                (a.with_y(ha), wa),
                (b.with_y(hb), wb),
                (b.with_y(floor), wb),
                (a.with_y(floor), wa),
            ] {
                local.push(p);
                normals.push(out.to_array());
                let mut c = field.terrain(w.x, w.z);
                c[3] = 0.0;
                colors.push(c);
            }
            let (p0, p1, p2) = (
                local[base as usize],
                local[base as usize + 1],
                local[base as usize + 2],
            );
            let tri = if (p1 - p0).cross(p2 - p0).dot(out) > 0.0 {
                [0, 1, 2, 0, 2, 3]
            } else {
                [0, 2, 1, 0, 3, 2]
            };
            idx.extend(tri.map(|i| base + i));
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        local.iter().map(|p| p.to_array()).collect::<Vec<_>>(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(idx))
}

/// The look-dev patch's terrain for one cell, walls down to [`SKIRT_FLOOR`].
pub fn terrain_mesh(cell: Axial, rot: u8) -> Mesh {
    terrain_mesh_for(&Lookdev, cell, rot, SKIRT_FLOOR)
}

/// Gerstner wave count per tier (the shader loops `wave_count` times in both stages).
pub fn wave_count(tier: QualityTier) -> u32 {
    match tier {
        QualityTier::Low => 4,
        QualityTier::Medium => 6,
        QualityTier::High | QualityTier::Ultra => 8,
    }
}

#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct WaterParams {
    pub water_time: f32,
    pub wave_count: u32,
    pub sea_level: f32,
    pub wash_amp: f32,
    /// PRD §9.4 water freeze: 0 is open water (the MVP value), 1 a frozen sheet (`crate::hooks`).
    pub freeze: f32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}

/// Shader variants: Low drops refraction (and with it caustics), Medium drops the second caustic
/// layer and the screen-space reflection march. `freeze` compiles in the §9.4 ice paths, which read
/// `WaterParams::freeze`; it is off for open water, so the MVP runs the approved shader unchanged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WaterKey {
    pub refraction: bool,
    pub hq: bool,
    pub freeze: bool,
}

impl WaterKey {
    pub fn for_tier(tier: QualityTier) -> WaterKey {
        WaterKey {
            refraction: tier != QualityTier::Low,
            hq: matches!(tier, QualityTier::High | QualityTier::Ultra),
            freeze: false,
        }
    }

    pub fn defs(self) -> Vec<bevy::shader::ShaderDefVal> {
        let mut v = vec![];
        if self.refraction {
            v.push("WATER_REFRACTION".into());
        }
        if self.hq {
            v.push("WATER_HQ".into());
        }
        if self.freeze {
            v.push("WATER_FREEZE".into());
        }
        v
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
#[bind_group_data(WaterKey)]
pub struct WaterMaterial {
    #[uniform(100)]
    pub params: WaterParams,
    /// Tileable ripple normals (RG), foam breakup (B) and a second noise octave (A).
    #[texture(101)]
    #[sampler(102)]
    pub noise: Handle<Image>,
    #[reflect(ignore)]
    pub key: WaterKey,
}

impl From<&WaterMaterial> for WaterKey {
    fn from(m: &WaterMaterial) -> Self {
        m.key
    }
}

impl Material for WaterMaterial {
    fn vertex_shader() -> ShaderRef {
        WATER_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        WATER_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        if self.key.refraction {
            AlphaMode::Opaque
        } else {
            AlphaMode::Blend
        }
    }

    /// Refracting water reads the opaque pass's colour, so it draws in `Transmissive3d`.
    fn reads_view_transmission_texture(&self) -> bool {
        self.key.refraction
    }

    /// Water stays out of the depth prepass: the shader reads that depth as the bed beneath it.
    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let defs = key.bind_group_data.defs();
        descriptor.vertex.shader_defs.extend(defs.iter().cloned());
        if let Some(f) = descriptor.fragment.as_mut() {
            f.shader_defs.extend(defs);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct TerrainParams {
    pub terrain_time: f32,
    pub wash_amp: f32,
    pub sea_level: f32,
    pub _pad: f32,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct TerrainMaterial {
    #[uniform(100)]
    pub params: TerrainParams,
    #[texture(101)]
    #[sampler(102)]
    pub noise: Handle<Image>,
}

impl Material for TerrainMaterial {
    fn fragment_shader() -> ShaderRef {
        TERRAIN_SHADER.into()
    }
}

/// Size of the generated noise texture (texels per side; one tile repeats every `NOISE_M` m in
/// the shaders that scale it).
const NOISE_SIZE: u32 = 256;

fn hash2(ix: i32, iy: i32, seed: u32) -> f32 {
    let mut h = (ix as u32).wrapping_mul(0x8da6_b343)
        ^ (iy as u32).wrapping_mul(0xd816_3841)
        ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 16_777_216.0
}

/// Periodic gradient noise with its analytic derivative: (value, d/dx, d/dy), period `cells`.
fn gradient_noise(x: f32, y: f32, cells: i32, seed: u32) -> (f32, f32, f32) {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - ix as f32, y - iy as f32);
    let grad = |i: i32, j: i32| {
        let a = hash2(i.rem_euclid(cells), j.rem_euclid(cells), seed) * std::f32::consts::TAU;
        Vec2::new(a.cos(), a.sin())
    };
    let (ga, gb, gc, gd) = (
        grad(ix, iy),
        grad(ix + 1, iy),
        grad(ix, iy + 1),
        grad(ix + 1, iy + 1),
    );
    let (va, vb) = (ga.dot(Vec2::new(fx, fy)), gb.dot(Vec2::new(fx - 1.0, fy)));
    let (vc, vd) = (
        gc.dot(Vec2::new(fx, fy - 1.0)),
        gd.dot(Vec2::new(fx - 1.0, fy - 1.0)),
    );
    let q = |t: f32| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let dq = |t: f32| 30.0 * t * t * (t * (t - 2.0) + 1.0);
    let (ux, uy, dux, duy) = (q(fx), q(fy), dq(fx), dq(fy));
    let v = va + ux * (vb - va) + uy * (vc - va) + ux * uy * (va - vb - vc + vd);
    let d = ga
        + ux * (gb - ga)
        + uy * (gc - ga)
        + ux * uy * (ga - gb - gc + gd)
        + Vec2::new(dux, duy)
            * (Vec2::new(uy, ux) * (va - vb - vc + vd) + Vec2::new(vb - va, vc - va));
    (v, d.x, d.y)
}

/// Tileable noise, generated once: RG ripple normal (from summed gradient-noise slopes), B foam
/// breakup, A a second, finer octave. Mipmapped (box filter) so distant water does not sparkle.
pub fn noise_image() -> Image {
    let n = NOISE_SIZE;
    let mut level = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let (u, v) = (x as f32 / n as f32, y as f32 / n as f32);
            let (mut sx, mut sy, mut foam, mut fine) = (0.0, 0.0, 0.0, 0.0);
            for (o, (cells, amp)) in [(8, 1.0), (16, 0.5), (32, 0.25)].into_iter().enumerate() {
                let (val, dx, dy) =
                    gradient_noise(u * cells as f32, v * cells as f32, cells, o as u32 + 1);
                sx += dx * amp * 0.18;
                sy += dy * amp * 0.18;
                foam += val * amp;
            }
            let (f2, _, _) = gradient_noise(u * 64.0, v * 64.0, 64, 9);
            fine += f2;
            let nrm = Vec3::new(-sx, -sy, 1.0).normalize();
            level.extend([
                ((nrm.x * 0.5 + 0.5) * 255.0).round() as u8,
                ((nrm.y * 0.5 + 0.5) * 255.0).round() as u8,
                ((foam * 0.6 + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8,
                ((fine * 0.7 + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8,
            ]);
        }
    }
    let mut data = level.clone();
    let mut size = n;
    let mut mips = 1;
    while size > 1 {
        let half = size / 2;
        let mut next = Vec::with_capacity((half * half * 4) as usize);
        for y in 0..half {
            for x in 0..half {
                for c in 0..4 {
                    let at = |xx: u32, yy: u32| level[((yy * size + xx) * 4 + c) as usize] as u32;
                    let s = at(2 * x, 2 * y)
                        + at(2 * x + 1, 2 * y)
                        + at(2 * x, 2 * y + 1)
                        + at(2 * x + 1, 2 * y + 1);
                    next.push(((s + 2) / 4) as u8);
                }
            }
        }
        data.extend_from_slice(&next);
        level = next;
        size = half;
        mips += 1;
    }
    let top = data[..(n * n * 4) as usize].to_vec();
    let mut img = Image::new(
        Extent3d {
            width: n,
            height: n,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        top,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.data = Some(data);
    img.texture_descriptor.mip_level_count = mips;
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    img
}

/// Water animation time. `fixed` advances it by exactly 1/30 s per frame (motion captures),
/// otherwise it follows real time.
#[derive(Resource, Default)]
pub struct WaterClock {
    pub t: f32,
    pub fixed: bool,
}

pub const FIXED_STEP: f32 = 1.0 / 30.0;

#[derive(Component)]
pub struct WaterSurface;

/// The water and placeholder-terrain materials for a water surface at `level`, sharing one noise
/// texture.
pub fn materials(
    images: &mut Assets<Image>,
    water_mats: &mut Assets<WaterMaterial>,
    terrain_mats: &mut Assets<TerrainMaterial>,
    tier: QualityTier,
    level: f32,
) -> (Handle<WaterMaterial>, Handle<TerrainMaterial>) {
    let noise = images.add(noise_image());
    let water = water_mats.add(WaterMaterial {
        params: WaterParams {
            water_time: 0.0,
            wave_count: wave_count(tier),
            sea_level: level,
            wash_amp: WASH_AMP,
            ..default()
        },
        noise: noise.clone(),
        key: WaterKey::for_tier(tier),
    });
    let terrain = terrain_mats.add(TerrainMaterial {
        params: TerrainParams {
            terrain_time: 0.0,
            wash_amp: WASH_AMP,
            sea_level: level,
            _pad: 0.0,
        },
        noise,
    });
    (water, terrain)
}

/// Spawns the look-dev patch; `setup` calls it for `Scenario::Water` and `Scenario::WaterKit`.
pub fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    images: &mut Assets<Image>,
    water_mats: &mut Assets<WaterMaterial>,
    terrain_mats: &mut Assets<TerrainMaterial>,
    tier: QualityTier,
) {
    let (water, terrain) = materials(images, water_mats, terrain_mats, tier, SEA_LEVEL);
    for (cell, rot) in layout() {
        let t = cell_transform(cell, rot);
        commands.spawn((
            Mesh3d(meshes.add(terrain_mesh(cell, rot))),
            MeshMaterial3d(terrain.clone()),
            t,
            SceneEntity,
        ));
        commands.spawn((
            Mesh3d(meshes.add(water_mesh(cell, rot))),
            MeshMaterial3d(water.clone()),
            t,
            NotShadowCaster,
            WaterSurface,
            SceneEntity,
        ));
        if let Some(skirt) = skirt_mesh(cell) {
            let [x, z] = cell.to_world(HEX_RADIUS);
            commands.spawn((
                Mesh3d(meshes.add(skirt)),
                MeshMaterial3d(water.clone()),
                Transform::from_xyz(x, 0.0, z),
                NotShadowCaster,
                WaterSurface,
                SceneEntity,
            ));
        }
    }
}

/// Camera components the water scenario adds: the depth prepass the water shader reads as its
/// bed (present even with `post=none`), and screen-space transmission for refracting tiers.
pub fn insert_camera(cam: &mut EntityCommands, tier: QualityTier) {
    cam.insert(DepthPrepass);
    if WaterKey::for_tier(tier).refraction {
        cam.insert(ScreenSpaceTransmission::default());
    }
}

pub fn tick(
    time: Res<Time>,
    clock: Option<ResMut<WaterClock>>,
    mut water: ResMut<Assets<WaterMaterial>>,
    mut terrain: ResMut<Assets<TerrainMaterial>>,
) {
    let Some(mut clock) = clock else {
        return;
    };
    clock.t += if clock.fixed {
        FIXED_STEP
    } else {
        time.delta_secs()
    };
    let t = clock.t;
    for (_, m) in water.iter_mut() {
        m.params.water_time = t;
    }
    for (_, m) in terrain.iter_mut() {
        m.params.terrain_time = t;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_grid_counts_and_winding() {
        let (p, i) = hex_grid(4, HEX_RADIUS);
        assert_eq!(i.len() / 3, 6 * 16);
        assert_eq!(p.len(), 6 * 15);
        for t in i.chunks(3) {
            let (a, b, c) = (p[t[0] as usize], p[t[1] as usize], p[t[2] as usize]);
            assert!((b - a).cross(c - a).y > 0.0);
        }
    }

    /// Every rim vertex of a cell lands on a rim vertex of its neighbour, whatever the two
    /// rotations, so the shared edge has no T-junction and the displaced surfaces meet exactly.
    #[test]
    fn neighbouring_rims_share_vertices_across_rotations() {
        let (local, _) = hex_grid(GRID, HEX_RADIUS);
        let apothem = HEX_RADIUS * 3f32.sqrt() / 2.0;
        let rim: Vec<Vec3> = local
            .iter()
            .copied()
            .filter(|p| {
                (0..6)
                    .map(|k| {
                        let a = (60.0 * k as f32).to_radians();
                        p.x * a.cos() + p.z * a.sin()
                    })
                    .fold(f32::MIN, f32::max)
                    > apothem - 1e-3
            })
            .collect();
        assert!(rim.len() >= 6 * GRID as usize);
        let a = Axial::new(0, 0);
        for edge in 0..6u8 {
            let b = a.neighbor(edge);
            for (ra, rb) in [(0u8, 3u8), (1, 4), (5, 2)] {
                let (ta, tb) = (cell_transform(a, ra), cell_transform(b, rb));
                let wb: Vec<Vec3> = rim.iter().map(|p| tb.transform_point(*p)).collect();
                let mid = {
                    let [ax, az] = a.to_world(HEX_RADIUS);
                    let [bx, bz] = b.to_world(HEX_RADIUS);
                    Vec2::new((ax + bx) * 0.5, (az + bz) * 0.5)
                };
                let mut shared: Vec<Vec3> = vec![];
                for w in rim.iter().map(|p| ta.transform_point(*p)) {
                    // Sector borders duplicate the corners.
                    if Vec2::new(w.x, w.z).distance(mid) < HEX_RADIUS * 0.51
                        && !shared.iter().any(|s| s.distance(w) < 1e-3)
                    {
                        shared.push(w);
                    }
                }
                assert_eq!(shared.len(), GRID as usize + 1, "edge {edge}");
                for w in shared {
                    let near = wb.iter().map(|q| q.distance(w)).fold(f32::MAX, f32::min);
                    assert!(
                        near < 1e-3,
                        "edge {edge} rot {ra}/{rb}: {w} unmatched ({near})"
                    );
                }
            }
        }
    }

    /// Joint continuity: the same world point gets the same water inputs whichever cell (and
    /// rotation) its vertex belongs to, because they are computed from world position.
    #[test]
    fn water_inputs_continuous_across_a_joint() {
        let a = Axial::new(0, 0);
        let b = a.neighbor(0);
        let ma = water_mesh(a, 2);
        let mb = water_mesh(b, 5);
        let world = |m: &Mesh, c: Axial, r: u8| {
            let t = cell_transform(c, r);
            let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) =
                m.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!()
            };
            let Some(bevy::mesh::VertexAttributeValues::Float32x4(col)) =
                m.attribute(Mesh::ATTRIBUTE_COLOR)
            else {
                panic!()
            };
            p.iter()
                .zip(col)
                .map(|(p, c)| (t.transform_point(Vec3::from(*p)), *c))
                .collect::<Vec<_>>()
        };
        let (wa, wb) = (world(&ma, a, 2), world(&mb, b, 5));
        let mut matched = 0;
        for (p, c) in &wa {
            if let Some((_, d)) = wb.iter().find(|(q, _)| q.distance(*p) < 1e-3) {
                for k in 0..4 {
                    assert!((c[k] - d[k]).abs() < 1e-3, "{p}: {c:?} vs {d:?}");
                }
                matched += 1;
            }
        }
        assert!(matched > GRID as usize, "{matched}");
    }

    /// Skirt tops sit exactly on the surface rim of the same cell (so the shared displacement leaves
    /// no crack), and only outer edges get one.
    #[test]
    fn skirts_cover_exposed_edges_and_meet_the_rim() {
        assert!(exposed_edges(Axial::new(1, 0)).is_empty(), "interior cell");
        let cell = Axial::new(3, -1);
        let edges = exposed_edges(cell);
        assert!(edges.contains(&0) && !edges.contains(&3), "{edges:?}");
        let skirt = skirt_mesh(cell).unwrap();
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(sp)) =
            skirt.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!()
        };
        let [cx, cz] = cell.to_world(HEX_RADIUS);
        let rot = layout().into_iter().find(|(c, _)| *c == cell).unwrap().1;
        let t = cell_transform(cell, rot);
        let surface = water_mesh(cell, rot);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(wp)) =
            surface.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!()
        };
        let rim: Vec<Vec3> = wp
            .iter()
            .map(|p| t.transform_point(Vec3::from(*p)))
            .collect();
        let mut tops = 0;
        for p in sp.iter().filter(|p| p[1] == SEA_LEVEL) {
            let w = Vec3::new(p[0] + cx, p[1], p[2] + cz);
            let near = rim.iter().map(|q| q.distance(w)).fold(f32::MAX, f32::min);
            assert!(near < 1e-3, "{w} off the rim by {near}");
            tops += 1;
        }
        assert_eq!(tops, edges.len() * (GRID as usize + 1));
    }

    /// The river leaves the patch through an exposed edge of (-1, 0), so that edge's skirt carries the
    /// channel's cross-section: below sea level in the channel, dry on both banks.
    #[test]
    fn river_edge_skirt_spans_the_channel_only() {
        let cell = Axial::new(-1, 0);
        assert!(exposed_edges(cell).contains(&4));
        let skirt = skirt_mesh(cell).unwrap();
        let Some(bevy::mesh::VertexAttributeValues::Float32x2(uv)) =
            skirt.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!()
        };
        let grounds: Vec<f32> = uv.iter().map(|u| u[1] - SKIRT_TAG).collect();
        assert!(
            grounds.iter().any(|g| *g < -1.0),
            "channel bed below the surface"
        );
        assert!(grounds.iter().any(|g| *g > 1.0), "dry banks");
    }

    #[test]
    fn scenario_has_ocean_river_beach_and_land() {
        assert!(height(60.0, 0.0) < -3.0, "deep ocean");
        assert!(height(-30.0, 25.0) > 1.0, "land plateau");
        let beach = height(24.0, -20.0);
        assert!(beach > 0.0 && beach < 1.4, "beach slope {beach}");
        // River bed in the bend, below sea level, flowing downstream.
        let s = water_sample(-10.0, 1.0);
        assert!(s.depth > 1.0 && s.river > 0.9, "{s:?}");
        assert!(s.flow.length() > 0.8 && s.flow.y > 0.0, "{s:?}");
        // Open sea has no current.
        assert!(water_sample(60.0, -20.0).flow.length() < 1e-3);
    }

    #[test]
    fn river_flow_turns_through_the_bend() {
        let up = water_sample(-40.0, -15.0).flow.normalize();
        let down = water_sample(-6.0, 5.0).flow.normalize();
        assert!(up.angle_to(down).abs() > 0.3, "{up} {down}");
    }

    #[test]
    fn tiers_scale_waves_and_variants() {
        assert_eq!(wave_count(QualityTier::Low), 4);
        assert_eq!(wave_count(QualityTier::High), 8);
        assert!(!WaterKey::for_tier(QualityTier::Low).refraction);
        assert!(WaterKey::for_tier(QualityTier::Medium).refraction);
        assert!(!WaterKey::for_tier(QualityTier::Medium).hq);
        assert!(WaterKey::for_tier(QualityTier::High).hq);
    }

    #[test]
    fn noise_image_is_mipmapped_and_tileable() {
        let img = noise_image();
        assert_eq!(img.texture_descriptor.mip_level_count, 9);
        let (a, b) = (
            gradient_noise(0.3, 0.7, 8, 1),
            gradient_noise(8.3, 0.7, 8, 1),
        );
        assert!((a.0 - b.0).abs() < 1e-5 && (a.1 - b.1).abs() < 1e-4);
    }

    #[test]
    fn lookdev_meshes_hash() {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let mut eat = |m: &Mesh| {
            for (_, v) in m.attributes() {
                for b in v.get_bytes() {
                    b.hash(&mut h);
                }
            }
            if let Some(Indices::U32(i)) = m.indices() {
                i.hash(&mut h);
            }
        };
        for (cell, rot) in layout() {
            eat(&water_mesh(cell, rot));
            eat(&terrain_mesh(cell, rot));
            if let Some(s) = skirt_mesh(cell) {
                eat(&s);
            }
        }
        // Recorded before the field refactor: the look-dev meshes are bit-identical.
        assert_eq!(h.finish(), 0x8d2d_d2e1_d1c7_899f);
    }

    #[test]
    fn params_match_their_wgsl_structs() {
        assert_eq!(WaterParams::min_size().get(), 32);
        assert_eq!(TerrainParams::min_size().get(), 16);
    }
}
