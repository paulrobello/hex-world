//! The Legend in-world affordances (`design/ui/legend/in-world.md`): the free-slot ring, the
//! hovered slot, the faint rim on a hovered placed tile, the ghost with its edge glow and socket key
//! marks, the bulldoze outlines and the commit flash. `hex-app` writes [`PlayView`]; this module draws it.
//!
//! Screen-space widths: every line is a quad per segment drawn with [`AffordanceMaterial`], whose
//! vertex shader (`assets/shaders/affordance.wesl`) projects both endpoints and pushes the quad's
//! corners out along the screen-space normal by the half-width in pixels, so a 2 px outline is
//! 2 px at every zoom. Dashes, the inward offset of the key marks, the cross ticks and the bulldoze
//! hatch are measured in pixels too, and every pixel size is multiplied by the window's scale
//! factor (the spec is at a device pixel ratio of 1). Halos are Gaussian falloffs of the distance
//! to the core, matching the prototype's CSS drop shadows, and the core is drawn emissive for bloom.
//! The material binds no texture, draws without depth testing, and stays out of the prepass and
//! the shadow passes. Only raised ground hides an affordance (P4): with any raised or sloped tile
//! placed, [`occlude`] cuts each part's lines and fills where [`Terrain`] stands between the eye and
//! them, so props and grass never do and a flat world draws exactly as before. On a sloped tile the
//! lines and fills follow its ground ([`drape`]). A tunnel preview is never cut (`on_top`).
use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::NoFrustumCulling,
    light::NotShadowCaster,
    mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology, VertexAttributeValues},
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{
        AsBindGroup, CompareFunction, RenderPipelineDescriptor, ShaderType,
        SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
    window::PrimaryWindow,
};
use hex_core::{
    Axial, EdgeStatus, Ground, HEX_RADIUS, LEVEL_STEP, SocketKind, TileCatalog, opposite_edge,
    slope::field_at, world_edge,
};

use super::{
    PlayCatalog, PlayView, PlayWaiting,
    ghost::{GhostDrawn, GhostMeshes},
    rise::{ReducedMotion, RiseCue, RiseMessage},
    scatter::turn,
};
use crate::{ActiveScenario, Scenario, SceneEntity, boids::plan::cell_at};
use hex_core::RiseStage;

pub const AFFORDANCE_SHADER: &str = "shaders/affordance.wesl";

const APOTHEM: f32 = HEX_RADIUS * 0.866_025_4;
/// Slot outlines sit at ground height, the ghost and the bulldoze outlines at the cap rim.
const GROUND_Y: f32 = 0.15;
const CAP_Y: f32 = 0.35;
/// A tunnel preview's line: the road's top over its tile's level.
const TUNNEL_Y: f32 = 0.11;
/// Outline radii: slots just inside their hex so neighbouring slots do not overdraw.
const SLOT_R: f32 = HEX_RADIUS * 0.965;
const RIM_R: f32 = HEX_RADIUS * 0.985;

// Tokens (sRGB) of `in-world.md`.
const COMPATIBLE: [u8; 3] = [123, 224, 143];
const INCOMPATIBLE: [u8; 3] = [255, 93, 82];
const NEUTRAL: [u8; 3] = [214, 228, 240];
const NEUTRAL_A: f32 = 0.55;
const INK: [u8; 3] = [238, 243, 246];
const GHOST_LINE: [u8; 3] = [214, 236, 255];
const GHOST_FILL: [u8; 3] = [74, 163, 223];
const SHEET: [u8; 3] = [18, 39, 58];
/// Emissive multiplier of the compatible and incompatible cores.
const GLOW: f32 = 2.5;

/// The socket key colour of a map class.
pub fn socket_rgb(k: SocketKind) -> [u8; 3] {
    match k {
        SocketKind::Grass => [156, 196, 100],
        SocketKind::Forest => [63, 138, 79],
        SocketKind::Road => [226, 104, 60],
        SocketKind::River => [74, 163, 223],
        SocketKind::Beach => [227, 194, 122],
        SocketKind::Ocean => [45, 111, 176],
        // `socket-rail` (`--k-T`, .impeccable/design.json): a violet clear of the pick magenta.
        SocketKind::Rail => [152, 128, 214],
    }
}

fn lin(rgb: [u8; 3], a: f32) -> Vec4 {
    let c = Color::srgb_u8(rgb[0], rgb[1], rgb[2]).to_linear();
    Vec4::new(c.red, c.green, c.blue, a)
}

const MODE_LINE: f32 = 0.0;
const MODE_FILL: f32 = 1.0;
const MODE_TICK: f32 = 2.0;

/// The shader's uniform; see `affordance.wesl` for each lane.
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType, Reflect)]
pub struct AffordanceParams {
    pub color: Vec4,
    pub shape: Vec4,
    pub halo: Vec4,
    pub misc: Vec4,
    pub hatch: Vec4,
}

impl AffordanceParams {
    fn line(rgb: [u8; 3], alpha: f32, width: f32) -> Self {
        Self {
            color: lin(rgb, alpha),
            shape: Vec4::new(width, 0.0, 0.0, 0.0),
            misc: Vec4::new(1.0, MODE_LINE, 1.0, 0.0),
            ..default()
        }
    }

    fn dashed(mut self, on: f32, off: f32) -> Self {
        self.shape.y = on;
        self.shape.z = off;
        self
    }

    /// Moves the centre line `px` toward the inside of a clockwise hex edge.
    fn inset(mut self, px: f32) -> Self {
        self.shape.w = px;
        self
    }

    fn glow(mut self, emissive: f32, halo: [f32; 4]) -> Self {
        self.misc.x = emissive;
        self.halo = Vec4::from_array(halo);
        self
    }

    fn fill(rgb: [u8; 3], alpha: f32) -> Self {
        Self {
            color: lin(rgb, alpha),
            misc: Vec4::new(1.0, MODE_FILL, 1.0, 0.0),
            ..default()
        }
    }

    fn hatched(mut self, rgb: [u8; 3], alpha: f32, spacing: f32, width: f32) -> Self {
        self.hatch = lin(rgb, alpha);
        self.misc.w = spacing;
        self.shape.x = width;
        self
    }

    fn tick(rgb: [u8; 3], width: f32, len: f32, angle: f32) -> Self {
        Self {
            color: lin(rgb, 1.0),
            shape: Vec4::new(width, len, 0.0, 0.0),
            misc: Vec4::new(GLOW, MODE_TICK, 1.0, angle),
            ..default()
        }
    }
}

/// Lines, ticks and fills of the in-world affordances. No textures.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct AffordanceMaterial {
    #[uniform(100)]
    pub params: AffordanceParams,
    /// Draw order among affordances, higher on top; every layer sorts after the transparent world.
    pub layer: f32,
}

impl Material for AffordanceMaterial {
    fn vertex_shader() -> ShaderRef {
        AFFORDANCE_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        AFFORDANCE_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn depth_bias(&self) -> f32 {
        10_000.0 + 100.0 * self.layer
    }

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
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // The quads are built in screen space, so either winding may face the camera, and an
        // affordance is never hidden behind the world it marks.
        descriptor.primitive.cull_mode = None;
        if let Some(ds) = descriptor.depth_stencil.as_mut() {
            ds.depth_compare = Some(CompareFunction::Always);
            ds.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

/// One quad per segment: POSITION a, COLOR.xyz b, UV_0 (t, side).
pub fn line_mesh(segs: &[(Vec3, Vec3)]) -> Mesh {
    let (mut pos, mut col, mut uv, mut idx) = (vec![], vec![], vec![], vec![]);
    for (i, (a, b)) in segs.iter().enumerate() {
        let base = (i * 4) as u32;
        for (t, side) in [(0.0, -1.0), (0.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
            pos.push(a.to_array());
            col.push([b.x, b.y, b.z, 0.0]);
            uv.push([t, side]);
        }
        idx.extend([base, base + 2, base + 1, base + 1, base + 2, base + 3]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_indices(Indices::U32(idx))
}

/// Triangle fans over convex polygons.
pub fn fill_mesh(polys: &[Vec<Vec3>]) -> Mesh {
    let (mut pos, mut idx) = (vec![], vec![]);
    for poly in polys {
        let base = pos.len() as u32;
        let c = poly.iter().copied().sum::<Vec3>() / poly.len().max(1) as f32;
        pos.push(c.to_array());
        pos.extend(poly.iter().map(|p| p.to_array()));
        let n = poly.len() as u32;
        for i in 0..n {
            idx.extend([base, base + 1 + i, base + 1 + (i + 1) % n]);
        }
    }
    let len = pos.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; len])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; len])
    .with_inserted_indices(Indices::U32(idx))
}

/// A fill of separate triangles, laid out as [`fill_mesh`]'s.
fn tri_mesh(tris: &[[Vec3; 3]]) -> Mesh {
    let pos: Vec<[f32; 3]> = tris.iter().flatten().map(|p| p.to_array()).collect();
    let len = pos.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; len])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; len])
    .with_inserted_indices(Indices::U32((0..len as u32).collect()))
}

/// The height a cell's affordances lift by (P4): its placed tile's level, else the ghost's on the
/// ghost's cell, else the free slot's level ([`PlayView::slot_levels`]), × `LEVEL_STEP`.
fn lift(view: &PlayView, cell: Axial) -> f32 {
    let level = view
        .tile_at(cell)
        .map(|t| t.level)
        .or_else(|| {
            view.ghost
                .as_ref()
                .filter(|g| g.cell == cell)
                .map(|g| g.level)
        })
        .or_else(|| view.slot_levels.get(&cell).copied())
        .unwrap_or(0);
    f32::from(level) * LEVEL_STEP
}

/// A sloped tile's ground under its affordances (P4): ground field, local corners, rotation.
type Slope = (Ground, [u8; 6], u8);

/// The sloped def `tile` turned by `rotation`, if it is sloped.
fn slope_of(catalog: &TileCatalog, tile: &str, rotation: u8) -> Option<Slope> {
    let d = catalog.get(tile)?;
    d.is_sloped().then(|| (d.ground, d.corners(0), rotation))
}

/// The placed tile at `cell` as [`Slope`], if it is sloped: part of a tile affordance's signature.
fn tile_slope(view: &PlayView, catalog: &TileCatalog, cell: Axial) -> Option<Slope> {
    let t = view.tile_at(cell)?;
    slope_of(catalog, &t.tile, t.rotation)
}

/// The sloped placed tiles by cell, and with `ghost` the ghost's cell when its tile is sloped.
fn slopes(view: &PlayView, catalog: &TileCatalog, ghost: bool) -> HashMap<Axial, Slope> {
    let ghost = view
        .ghost
        .as_ref()
        .filter(|_| ghost)
        .map(|g| (g.cell, g.tile.as_str(), g.rotation));
    view.tiles
        .iter()
        .map(|t| (t.cell, t.tile.as_str(), t.rotation))
        .chain(ghost)
        .filter_map(|(cell, tile, rotation)| Some((cell, slope_of(catalog, tile, rotation)?)))
        .collect()
}

/// The ground's height over its level at world `p` in `cell`, whose tile is `slope`.
fn field_in(cell: Axial, (ground, corners, rotation): Slope, p: Vec2) -> f32 {
    let d = p - Vec2::from(cell.to_world(HEX_RADIUS));
    let l = turn((6 - rotation % 6) % 6, [d.x, 0.0, d.y]);
    field_at(ground, corners, l[0], l[2])
}

/// The ground's height over its level under world `p`: the field of the sloped tile there, else 0.
fn rise(slopes: &HashMap<Axial, Slope>, p: Vec2) -> f32 {
    let cell = cell_at(p);
    slopes
        .get(&cell)
        .map_or(0.0, |&slope| field_in(cell, slope, p))
}

/// A line part's segments (`line_mesh`: POSITION is a quad's start, COLOR.xyz its end).
fn segments(mesh: &Mesh) -> Option<Vec<(Vec3, Vec3)>> {
    let Some(VertexAttributeValues::Float32x3(a)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return None;
    };
    let Some(VertexAttributeValues::Float32x4(b)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR) else {
        return None;
    };
    Some(
        a.iter()
            .zip(b)
            .step_by(4)
            .map(|(a, b)| (Vec3::from(*a), Vec3::new(b[0], b[1], b[2])))
            .collect(),
    )
}

/// A fill part's triangles.
fn triangles(mesh: &Mesh) -> Option<Vec<[Vec3; 3]>> {
    let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return None;
    };
    let idx: Vec<usize> = mesh.indices()?.iter().collect();
    Some(
        idx.as_chunks::<3>()
            .0
            .iter()
            .map(|t| t.map(|i| Vec3::from(pos[i])))
            .collect(),
    )
}

/// `t` cut into equal triangles no side of which is longer than about `step` metres.
fn subdivide([a, b, c]: [Vec3; 3], step: f32) -> Vec<[Vec3; 3]> {
    let longest = a.distance(b).max(b.distance(c)).max(c.distance(a));
    let n = (longest / step).ceil().max(1.0) as usize;
    let at =
        |i: usize, j: usize| a + (b - a) * (i as f32 / n as f32) + (c - a) * (j as f32 / n as f32);
    let mut out = Vec::with_capacity(n * n);
    for i in 0..n {
        for j in 0..n - i {
            out.push([at(i, j), at(i + 1, j), at(i, j + 1)]);
            if j + 1 < n - i {
                out.push([at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)]);
            }
        }
    }
    out
}

/// Spacing (m) of the points a draped line or fill follows the ground through.
const DRAPE_STEP: f32 = 2.0;

/// Lays a part over the sloped tiles in `slopes` (P4): lines are cut every [`DRAPE_STEP`] and
/// fills subdivided as finely, and every point lifted by the ground's [`rise`] under it, so the
/// part follows a hillside instead of a flat plane its high side would bury. Ticks keep their two
/// points. A part over no sloped tile, and an `on_top` part, are unchanged.
fn drape(spec: PartSpec, slopes: &HashMap<Axial, Slope>) -> PartSpec {
    if spec.on_top || slopes.is_empty() {
        return spec;
    }
    let Some(VertexAttributeValues::Float32x3(pos)) = spec.mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return spec;
    };
    if !pos
        .iter()
        .any(|p| slopes.contains_key(&cell_at(Vec3::from(*p).xz())))
    {
        return spec;
    }
    let up = |p: Vec3| p + Vec3::Y * rise(slopes, p.xz());
    let mode = spec.params.misc.y;
    let mesh = if mode == MODE_FILL {
        let Some(tris) = triangles(&spec.mesh) else {
            return spec;
        };
        let tris: Vec<[Vec3; 3]> = tris
            .into_iter()
            .flat_map(|t| subdivide(t, DRAPE_STEP))
            .map(|t| t.map(up))
            .collect();
        tri_mesh(&tris)
    } else {
        let Some(segs) = segments(&spec.mesh) else {
            return spec;
        };
        let segs: Vec<(Vec3, Vec3)> = segs
            .into_iter()
            .flat_map(|(a, b)| {
                let n = if mode == MODE_TICK {
                    1
                } else {
                    (a.distance(b) / DRAPE_STEP).ceil().max(1.0) as usize
                };
                (0..n).map(move |k| {
                    (
                        a.lerp(b, k as f32 / n as f32),
                        a.lerp(b, (k + 1) as f32 / n as f32),
                    )
                })
            })
            .map(|(a, b)| (up(a), up(b)))
            .collect();
        line_mesh(&segs)
    };
    PartSpec { mesh, ..spec }
}

/// The raised ground that may hide an affordance from the eye (P4): every tile above level 0 or
/// sloped, by cell, as its level height and slope. Empty cells and flat level-0 tiles are y = 0,
/// under every affordance, so they never hide one; props and grass are not ground at all.
#[derive(Clone, Debug, PartialEq)]
pub struct Terrain {
    cells: HashMap<Axial, (f32, Option<Slope>)>,
    /// The highest ground (m): a ray at or above it is never hidden.
    max_top: f32,
}

/// The view's [`Terrain`], or `None` in a world with no raised or sloped tile, where nothing is
/// ever tested.
pub fn terrain(view: &PlayView, catalog: &TileCatalog) -> Option<Terrain> {
    let mut max_top = 0.0f32;
    let cells: HashMap<Axial, (f32, Option<Slope>)> = view
        .tiles
        .iter()
        .filter_map(|t| {
            let def = catalog.get(&t.tile)?;
            let y0 = f32::from(t.level) * LEVEL_STEP;
            let slope = slope_of(catalog, &t.tile, t.rotation);
            if y0 == 0.0 && slope.is_none() {
                return None;
            }
            max_top = max_top.max(y0 + f32::from(def.top()) * LEVEL_STEP);
            Some((t.cell, (y0, slope)))
        })
        .collect();
    (!cells.is_empty()).then_some(Terrain { cells, max_top })
}

/// Steps (m) the eye ray is sampled at, and the start skipped by the ray's own ground.
const MARCH_STEP: f32 = 0.5;
const MARCH_SKIP: f32 = 0.5;
/// How far (m) under the ground a ray must pass to count as hidden: the drawn 12-ring cap departs
/// from the analytic field by about this much.
const MARCH_MARGIN: f32 = 0.05;
/// The longest stretch (m) a nearly level eye ray is followed.
const MARCH_MAX: f32 = 600.0;

impl Terrain {
    /// The ground's height (m) at world `p`.
    pub fn height(&self, p: Vec2) -> f32 {
        let cell = cell_at(p);
        self.cells.get(&cell).map_or(0.0, |&(y0, slope)| {
            y0 + slope.map_or(0.0, |s| field_in(cell, s, p))
        })
    }

    /// Whether raised ground stands between `eye` and world point `p`: the ray from `p` toward the
    /// eye, sampled every [`MARCH_STEP`] while it is under the highest ground, dips more than
    /// [`MARCH_MARGIN`] below it somewhere past the first [`MARCH_SKIP`].
    pub fn occluded(&self, eye: Vec3, p: Vec3) -> bool {
        let to_eye = eye - p;
        let len = to_eye.length();
        if len <= MARCH_SKIP {
            return false;
        }
        let dir = to_eye / len;
        let mut end = len.min(MARCH_MAX);
        if dir.y > 1e-6 {
            end = end.min((self.max_top - p.y) / dir.y);
        } else if p.y >= self.max_top {
            return false;
        }
        let mut s = MARCH_SKIP;
        while s < end {
            let q = p + dir * s;
            if q.y < self.height(q.xz()) - MARCH_MARGIN {
                return true;
            }
            s += MARCH_STEP;
        }
        false
    }
}

/// Spacing (m) of the points a line or fill is tested at for [`occlude`].
const OCCLUDE_STEP: f32 = 1.0;

/// `mesh` (a part's mesh in `mode`) with what `terrain` hides from `eye` cut away. A line keeps
/// each run of visible samples as one segment from its first to its last (half a step past each
/// into the hidden side), and a wholly visible segment its exact ends, so a dash restarts only
/// after a hidden gap. A tick stands or goes with its centre. A fill keeps the visible ones of
/// its triangles cut to [`OCCLUDE_STEP`]. `None` when nothing is hidden: the part draws whole.
fn occlude_mesh(mesh: &Mesh, mode: f32, terrain: &Terrain, eye: Vec3) -> Option<Mesh> {
    let seen = |p: Vec3| !terrain.occluded(eye, p);
    if mode == MODE_FILL {
        let all: Vec<[Vec3; 3]> = triangles(mesh)?
            .into_iter()
            .flat_map(|t| subdivide(t, OCCLUDE_STEP))
            .collect();
        let tris: Vec<[Vec3; 3]> = all
            .iter()
            .copied()
            .filter(|[a, b, c]| seen((*a + *b + *c) / 3.0))
            .collect();
        return (tris.len() < all.len()).then(|| tri_mesh(&tris));
    }
    let mut out = vec![];
    let mut cut = false;
    for (a, b) in segments(mesh)? {
        if mode == MODE_TICK {
            if seen(a) {
                out.push((a, b));
            } else {
                cut = true;
            }
            continue;
        }
        let n = (a.distance(b) / OCCLUDE_STEP).ceil().max(1.0) as usize;
        let vis: Vec<bool> = (0..=n)
            .map(|k| seen(a.lerp(b, k as f32 / n as f32)))
            .collect();
        if vis.iter().all(|&v| v) {
            out.push((a, b));
            continue;
        }
        cut = true;
        let half = 0.5 / n as f32;
        let mut k = 0;
        while k <= n {
            if !vis[k] {
                k += 1;
                continue;
            }
            let first = k;
            while k < n && vis[k + 1] {
                k += 1;
            }
            let t0 = if first == 0 {
                0.0
            } else {
                first as f32 / n as f32 - half
            };
            let t1 = if k == n {
                1.0
            } else {
                k as f32 / n as f32 + half
            };
            out.push((a.lerp(b, t0), a.lerp(b, t1)));
            k += 1;
        }
    }
    cut.then(|| line_mesh(&out))
}

fn dir(deg: f32) -> Vec3 {
    let a = deg.to_radians();
    Vec3::new(a.cos(), 0.0, a.sin())
}

fn centre(cell: Axial, y: f32) -> Vec3 {
    let [x, z] = cell.to_world(HEX_RADIUS);
    Vec3::new(x, y, z)
}

/// World edge `e` of the hex at `c`, from corner to corner in increasing angle: clockwise on
/// screen for a camera above the ground, so the inside is to the right of a -> b.
pub fn edge_seg(c: Vec3, e: u8, r: f32) -> (Vec3, Vec3) {
    let m = 60.0 * e as f32;
    (c + dir(m - 30.0) * r, c + dir(m + 30.0) * r)
}

fn hex_segs(c: Vec3, r: f32) -> Vec<(Vec3, Vec3)> {
    (0..6).map(|e| edge_seg(c, e, r)).collect()
}

fn hex_poly(c: Vec3, r: f32) -> Vec<Vec3> {
    (0..6).map(|e| edge_seg(c, e, r).0).collect()
}

/// The tiles removing `cell` would cut off: every component but the largest.
pub fn cut_off(tiles: &[Axial], cell: Axial) -> Vec<Axial> {
    let rest: BTreeSet<Axial> = tiles.iter().copied().filter(|&c| c != cell).collect();
    let mut seen: BTreeSet<Axial> = BTreeSet::new();
    let mut parts: Vec<Vec<Axial>> = vec![];
    for &start in &rest {
        if seen.contains(&start) {
            continue;
        }
        let mut part = vec![start];
        seen.insert(start);
        let mut i = 0;
        while i < part.len() {
            for e in 0..6 {
                let n = part[i].neighbor(e);
                if rest.contains(&n) && seen.insert(n) {
                    part.push(n);
                }
            }
            i += 1;
        }
        parts.push(part);
    }
    // Stable on ties: the first part found (lowest cell) stays.
    let keep = parts
        .iter()
        .enumerate()
        .max_by_key(|(i, p)| (p.len(), std::cmp::Reverse(*i)))
        .map(|(i, _)| i);
    let mut out: Vec<Axial> = parts
        .into_iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != keep)
        .flat_map(|(_, p)| p)
        .collect();
    out.sort();
    out
}

/// How a part animates beyond its element's fade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Anim {
    Plain,
    /// The ghost cap fill: 100% to 55% and back over 2.4 s.
    Breath,
    /// Edge glow, key marks and ticks: fade in after the ghost changes.
    Glow,
    /// Incompatible glow: also widens on a refused placement.
    GlowPulse,
    /// The blocked bulldoze outline: widens on a refused bulldoze.
    Pulse,
}

struct PartSpec {
    mesh: Mesh,
    params: AffordanceParams,
    layer: f32,
    anim: Anim,
    /// Never cut by raised ground ([`occlude`]): a tunnel preview shows the road under its cover.
    on_top: bool,
}

struct Part {
    entity: Entity,
    material: Handle<AffordanceMaterial>,
    base: AffordanceParams,
    anim: Anim,
    /// The drawn mesh, and the part's whole mesh [`occlude`] cuts it from.
    mesh: Handle<Mesh>,
    source: Mesh,
    on_top: bool,
    /// What the drawn mesh holds: 0 the whole `source`, else [`mesh_sig`] of its cut.
    shown: u64,
}

/// A value easing from `from` to `to` over `dur` seconds after `delay`.
#[derive(Clone, Copy, Debug)]
struct Fade {
    from: f32,
    to: f32,
    t: f32,
    dur: f32,
    delay: f32,
}

impl Fade {
    fn at(v: f32) -> Self {
        Self {
            from: v,
            to: v,
            t: 0.0,
            dur: 0.0,
            delay: 0.0,
        }
    }

    fn value(&self) -> f32 {
        let u = if self.dur <= 0.0 {
            if self.t >= self.delay { 1.0 } else { 0.0 }
        } else {
            ((self.t - self.delay) / self.dur).clamp(0.0, 1.0)
        };
        self.from + (self.to - self.from) * ease(u)
    }

    fn to(&mut self, to: f32, dur: f32, delay: f32) {
        if (self.to - to).abs() < 1e-6 {
            return;
        }
        *self = Self {
            from: self.value(),
            to,
            t: 0.0,
            dur,
            delay,
        };
    }

    fn done(&self) -> bool {
        self.t >= self.delay + self.dur
    }
}

/// The ghost's edge glow, socket colours and key marks (`in-world.md`, Rotate): after R they wait
/// at 0 while the ghost turns and fade in over `T_FAST` from the frame the turn lands.
#[derive(Clone, Copy, Debug)]
struct Glow {
    fade: Fade,
    /// Waiting for the running turn to land.
    held: bool,
}

impl Glow {
    fn at(v: f32) -> Self {
        Self {
            fade: Fade::at(v),
            held: false,
        }
    }

    /// The element was rebuilt: the glow restarts from 0, held while the ghost `turning`.
    fn restart(&mut self, turning: bool, instant: bool) {
        self.fade = Fade::at(0.0);
        self.held = turning && !instant;
        if !self.held {
            self.fade.to(1.0, if instant { 0.0 } else { T_FAST }, 0.0);
        }
    }

    /// One frame: a held glow starts its fade on the frame the turn lands.
    fn step(&mut self, turning: bool, instant: bool, dt: f32) {
        if self.held && (!turning || instant) {
            self.held = false;
            self.fade.to(1.0, if instant { 0.0 } else { T_FAST }, 0.0);
        }
        self.fade.t += dt;
    }

    fn value(&self) -> f32 {
        if self.held { 0.0 } else { self.fade.value() }
    }
}

/// `cubic-bezier(.22, .9, .3, 1)`, the Legend motion curve.
pub fn ease(u: f32) -> f32 {
    if u <= 0.0 {
        return 0.0;
    }
    if u >= 1.0 {
        return 1.0;
    }
    let (x1, y1, x2, y2) = (0.22f32, 0.9f32, 0.3f32, 1.0f32);
    let bez = |a: f32, b: f32, s: f32| {
        3.0 * a * s * (1.0 - s).powi(2) + 3.0 * b * s * s * (1.0 - s) + s.powi(3)
    };
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        if bez(x1, x2, mid) < u {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bez(y1, y2, 0.5 * (lo + hi))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Key {
    Ring,
    Hover(Axial),
    /// A placed tile under the pointer in place mode.
    TileHover(Axial),
    Ghost(Axial),
    Bulldoze(Axial),
    Flash(Axial, u32),
    /// A placed tile hidden until its pack arrives ([`PlayWaiting`]).
    Waiting(Axial),
}

struct Element {
    key: Key,
    sig: u64,
    root: Entity,
    parts: Vec<Part>,
    alpha: Fade,
    glow: Glow,
    /// The glow parts of the rotation the ghost turned from, fading out over `glow_out`.
    fading: Vec<Part>,
    glow_out: Fade,
    /// Fading out, despawned when invisible.
    leaving: bool,
}

/// The live affordance elements and the refusal pulse.
#[derive(Resource, Default)]
pub struct Affordances {
    elements: Vec<Element>,
    refusals: u32,
    pulse: Option<f32>,
    flashes: u32,
    breath: f32,
    /// The raised ground of the view ([`terrain`]), the eye the parts were last cut for, and
    /// whether a part was spawned or the ground changed since.
    terrain: Option<Terrain>,
    cut_eye: Option<Vec3>,
    cut_dirty: bool,
}

fn sig_of(v: &impl Hash) -> u64 {
    let mut h = DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

fn status_hash(s: EdgeStatus) -> u8 {
    match s {
        EdgeStatus::Compatible => 0,
        EdgeStatus::Incompatible => 1,
        EdgeStatus::Neutral => 2,
    }
}

/// The socket kind each placed neighbour shows the slot at `cell`, by world edge.
fn neighbour_sockets(
    view: &PlayView,
    catalog: &TileCatalog,
    cell: Axial,
) -> [Option<SocketKind>; 6] {
    let placed: HashMap<Axial, (&str, u8)> = view
        .tiles
        .iter()
        .map(|t| (t.cell, (t.tile.as_str(), t.rotation)))
        .collect();
    std::array::from_fn(|w| {
        let (id, rot) = placed.get(&cell.neighbor(w as u8))?;
        let def = catalog.get(id)?;
        Some(def.edge(opposite_edge(w as u8), *rot).kind)
    })
}

/// The free slots' dashed outlines, each on its slot level ([`PlayView::slot_levels`]).
fn ring_parts(view: &PlayView) -> Vec<PartSpec> {
    let segs: Vec<_> = view
        .frontier
        .iter()
        .flat_map(|&c| {
            let level = view.slot_levels.get(&c).copied().unwrap_or(0);
            hex_segs(centre(c, f32::from(level) * LEVEL_STEP + GROUND_Y), SLOT_R)
        })
        .collect();
    vec![PartSpec {
        mesh: line_mesh(&segs),
        params: AffordanceParams::line(NEUTRAL, NEUTRAL_A, 2.0).dashed(6.0, 7.0),
        layer: 0.0,
        anim: Anim::Plain,
        on_top: false,
    }]
}

fn hover_parts(cell: Axial, y0: f32) -> Vec<PartSpec> {
    let c = centre(cell, y0 + GROUND_Y);
    vec![
        PartSpec {
            mesh: fill_mesh(&[hex_poly(c, SLOT_R)]),
            params: AffordanceParams::fill(INK, 0.04),
            layer: 0.5,
            anim: Anim::Plain,
            on_top: false,
        },
        PartSpec {
            mesh: line_mesh(&hex_segs(c, SLOT_R)),
            params: AffordanceParams::line(INK, 0.9, 2.0).dashed(6.0, 7.0),
            layer: 1.0,
            anim: Anim::Plain,
            on_top: false,
        },
    ]
}

/// The faint rim on a placed tile under the pointer: ink, solid, thinner and far fainter than the
/// hovered slot's outline, at the cap rim like the bulldoze target.
fn tile_hover_parts(cell: Axial, y0: f32) -> Vec<PartSpec> {
    vec![PartSpec {
        mesh: line_mesh(&hex_segs(centre(cell, y0 + CAP_Y), RIM_R)),
        params: AffordanceParams::line(INK, 0.35, 1.5),
        layer: 1.0,
        anim: Anim::Plain,
        on_top: false,
    }]
}

/// The ghost's stand-in lane strokes by socket kind, with their widths in px.
const LANE_STROKES: [(SocketKind, f32); 3] = [
    (SocketKind::River, 22.0),
    (SocketKind::Road, 18.0),
    (SocketKind::Rail, 11.0),
];

fn ghost_parts(view: &PlayView, catalog: &TileCatalog, meshes: bool) -> Vec<PartSpec> {
    let Some(g) = &view.ghost else {
        return vec![];
    };
    let y0 = f32::from(g.level) * LEVEL_STEP;
    let c = centre(g.cell, CAP_Y + y0);
    // A pinned ghost's outline is solid, so the slot reads as held while the pointer is away.
    let outline = AffordanceParams::line(GHOST_LINE, 0.75, 2.0);
    let outline = if g.pinned {
        AffordanceParams::line(GHOST_LINE, 0.95, 3.0)
    } else {
        outline.dashed(10.0, 8.0)
    };
    let mut out = vec![
        PartSpec {
            mesh: fill_mesh(&[hex_poly(c, RIM_R)]),
            params: AffordanceParams::fill(GHOST_FILL, 0.12),
            layer: 2.0,
            anim: Anim::Breath,
            on_top: false,
        },
        PartSpec {
            mesh: line_mesh(&hex_segs(c, RIM_R)),
            params: outline,
            layer: 4.0,
            anim: Anim::Plain,
            on_top: false,
        },
    ];
    // A portal ghost's tunnel (P4): a faint dotted line at the road's height from the mouth to the
    // far side of the last cover cell, drawn over the cover that will hide the road.
    if let (Some(&first), Some(&last)) = (g.tunnel.first(), g.tunnel.last()) {
        let road = |cell: Axial| centre(cell, y0 + TUNNEL_Y);
        let half = (road(first) - road(g.cell)) * 0.5;
        out.push(PartSpec {
            mesh: line_mesh(&[(road(g.cell) + half, road(last) + half)]),
            params: AffordanceParams::line(GHOST_LINE, 0.6, 3.0).dashed(2.0, 7.0),
            layer: 3.0,
            anim: Anim::Plain,
            on_top: true,
        });
    }
    let def = catalog.get(&g.tile);
    // Lane strokes stand in for the tile's meshes while its pack streams: river 22 px, road 18 px,
    // rail 11 px (its 3.5 m ballast lane beside the road's 6 m).
    if let Some(def) = def.filter(|_| !meshes) {
        for (kind, width) in LANE_STROKES {
            let segs: Vec<_> = (0..6u8)
                .filter(|&e| {
                    let s = &def.edges[e as usize];
                    s.kind == kind && s.lane.is_some()
                })
                .map(|e| {
                    (
                        c + dir(60.0 * world_edge(e, g.rotation) as f32) * APOTHEM,
                        c,
                    )
                })
                .collect();
            if !segs.is_empty() {
                out.push(PartSpec {
                    mesh: line_mesh(&segs),
                    params: AffordanceParams::line(socket_rgb(kind), 0.6, width),
                    layer: 3.0,
                    anim: Anim::Plain,
                    on_top: false,
                });
            }
        }
    }
    // The tile's own socket on each edge at this rotation, 11 px inside the rim, and the
    // neighbour's socket key mark facing it 11 px outside: the edge glow between them says fit or
    // clash.
    if let Some(def) = def {
        for kind in SocketKind::ALL {
            let segs: Vec<_> = (0..6u8)
                .filter(|&w| def.edge(w, g.rotation).kind == kind)
                .map(|w| {
                    let (a, b) = edge_seg(c, w, RIM_R);
                    (a.lerp(b, 0.14), a.lerp(b, 0.86))
                })
                .collect();
            if !segs.is_empty() {
                out.push(PartSpec {
                    mesh: line_mesh(&segs),
                    params: AffordanceParams::line(socket_rgb(kind), 1.0, 5.0).inset(11.0),
                    layer: 5.0,
                    anim: Anim::Glow,
                    on_top: false,
                });
            }
        }
    }
    let sockets = neighbour_sockets(view, catalog, g.cell);
    for kind in SocketKind::ALL {
        let segs: Vec<_> = (0..6u8)
            .filter(|&w| sockets[w as usize] == Some(kind))
            .map(|w| {
                let (a, b) = edge_seg(c, w, RIM_R);
                (a.lerp(b, 0.3), a.lerp(b, 0.7))
            })
            .collect();
        if !segs.is_empty() {
            out.push(PartSpec {
                mesh: line_mesh(&segs),
                params: AffordanceParams::line(socket_rgb(kind), 0.95, 4.0).inset(-11.0),
                layer: 5.0,
                anim: Anim::Glow,
                on_top: false,
            });
        }
    }
    // Edge glow by state, then the cross ticks on incompatible edges.
    let edges_in = |s: EdgeStatus| -> Vec<(Vec3, Vec3)> {
        (0..6u8)
            .filter(|&w| g.edges[w as usize] == s)
            .map(|w| edge_seg(c, w, RIM_R))
            .collect()
    };
    let glows = [
        (
            EdgeStatus::Neutral,
            AffordanceParams::line(NEUTRAL, NEUTRAL_A * 0.7, 3.0),
            Anim::Glow,
        ),
        (
            EdgeStatus::Compatible,
            AffordanceParams::line(COMPATIBLE, 1.0, 7.0).glow(GLOW, [3.0, 0.9, 7.0, 0.6]),
            Anim::Glow,
        ),
        (
            EdgeStatus::Incompatible,
            AffordanceParams::line(INCOMPATIBLE, 1.0, 7.0)
                .dashed(14.0, 9.0)
                .glow(GLOW, [3.0, 0.9, 7.0, 0.5]),
            Anim::GlowPulse,
        ),
    ];
    for (status, params, anim) in glows {
        let segs = edges_in(status);
        if !segs.is_empty() {
            out.push(PartSpec {
                mesh: line_mesh(&segs),
                params,
                layer: 6.0,
                anim,
                on_top: false,
            });
        }
    }
    let bad: Vec<(Vec3, Vec3)> = edges_in(EdgeStatus::Incompatible)
        .into_iter()
        .map(|(a, b)| (a.lerp(b, 0.5), b))
        .collect();
    if !bad.is_empty() {
        for angle in [std::f32::consts::FRAC_PI_4, -std::f32::consts::FRAC_PI_4] {
            out.push(PartSpec {
                mesh: line_mesh(&bad),
                params: AffordanceParams::tick(INCOMPATIBLE, 3.5, 14.0, angle),
                layer: 7.0,
                anim: Anim::Glow,
                on_top: false,
            });
        }
    }
    out
}

fn bulldoze_parts(view: &PlayView) -> Vec<PartSpec> {
    let Some((cell, allowed)) = view.bulldoze else {
        return vec![];
    };
    let c = centre(cell, lift(view, cell) + CAP_Y);
    if allowed {
        return vec![
            PartSpec {
                mesh: fill_mesh(&[hex_poly(c, RIM_R)]),
                params: AffordanceParams::fill(INCOMPATIBLE, 0.12),
                layer: 2.0,
                anim: Anim::Plain,
                on_top: false,
            },
            PartSpec {
                mesh: line_mesh(&hex_segs(c, RIM_R)),
                params: AffordanceParams::line(INCOMPATIBLE, 1.0, 4.0)
                    .glow(GLOW, [4.0, 0.6, 0.0, 0.0]),
                layer: 6.0,
                anim: Anim::Plain,
                on_top: false,
            },
        ];
    }
    let cut = if view.bulldoze_cover {
        vec![]
    } else {
        let cells: Vec<Axial> = view.tiles.iter().map(|t| t.cell).collect();
        cut_off(&cells, cell)
    };
    let mut out = vec![PartSpec {
        mesh: line_mesh(&hex_segs(c, RIM_R)),
        params: AffordanceParams::line(INCOMPATIBLE, 1.0, 4.0)
            .dashed(10.0, 8.0)
            .glow(GLOW, [0.0, 0.0, 0.0, 0.0]),
        layer: 6.0,
        anim: Anim::Pulse,
        on_top: false,
    }];
    if !cut.is_empty() {
        let polys: Vec<_> = cut
            .iter()
            .map(|&k| hex_poly(centre(k, lift(view, k) + CAP_Y), RIM_R))
            .collect();
        let segs: Vec<_> = cut
            .iter()
            .flat_map(|&k| hex_segs(centre(k, lift(view, k) + CAP_Y), RIM_R))
            .collect();
        out.push(PartSpec {
            mesh: fill_mesh(&polys),
            params: AffordanceParams::fill(SHEET, 0.35).hatched(INK, 0.75, 10.0, 2.5),
            layer: 2.0,
            anim: Anim::Plain,
            on_top: false,
        });
        out.push(PartSpec {
            mesh: line_mesh(&segs),
            params: AffordanceParams::line(INK, 1.0, 2.0).dashed(3.0, 5.0),
            layer: 4.0,
            anim: Anim::Plain,
            on_top: false,
        });
    }
    out
}

fn flash_parts(cell: Axial, y0: f32) -> Vec<PartSpec> {
    vec![PartSpec {
        mesh: line_mesh(&hex_segs(centre(cell, y0 + CAP_Y), RIM_R)),
        params: AffordanceParams::line(INK, 1.0, 7.0).glow(4.0, [3.0, 0.9, 7.0, 0.6]),
        layer: 8.0,
        anim: Anim::Plain,
        on_top: false,
    }]
}

/// The loading cue on a placed tile's empty slot while its pack streams (owner decision A10): a
/// soft outline that breathes like the ghost fill, and holds still under reduced motion.
fn waiting_parts(cell: Axial, y0: f32) -> Vec<PartSpec> {
    vec![PartSpec {
        mesh: line_mesh(&hex_segs(centre(cell, y0 + GROUND_Y), SLOT_R)),
        params: AffordanceParams::line(INK, 0.6, 2.5),
        layer: 1.0,
        anim: Anim::Breath,
        on_top: false,
    }]
}

/// What a placed tile's affordances at `cell` depend on: its level and, if sloped, its ground.
fn tile_sig(view: &PlayView, catalog: &TileCatalog, cell: Axial) -> u64 {
    sig_of(&(lift(view, cell).to_bits(), tile_slope(view, catalog, cell)))
}

/// What the view asks for: (key, signature, whether it shows).
fn wanted(
    view: &PlayView,
    catalog: &TileCatalog,
    waiting: &[Axial],
    meshes: bool,
) -> Vec<(Key, u64, bool)> {
    let mut out = vec![];
    for &cell in waiting {
        out.push((Key::Waiting(cell), tile_sig(view, catalog, cell), true));
    }
    let ring_on = !view.frontier.is_empty() && !view.bulldoze_mode;
    out.push((
        Key::Ring,
        sig_of(&(&view.frontier, &view.slot_levels)),
        ring_on,
    ));
    if let Some(cell) = view.pointer_slot {
        out.push((Key::Hover(cell), sig_of(&lift(view, cell).to_bits()), true));
    }
    // Bulldoze mode outlines the hovered tile as its target instead; a tile still waiting for its
    // pack shows the waiting outline.
    if let Some(cell) = view
        .pointer_tile
        .filter(|c| !view.bulldoze_mode && !waiting.contains(c))
    {
        out.push((Key::TileHover(cell), tile_sig(view, catalog, cell), true));
    }
    if let Some(g) = &view.ghost {
        out.push((
            Key::Hover(g.cell),
            sig_of(&lift(view, g.cell).to_bits()),
            true,
        ));
        let edges: Vec<u8> = g.edges.iter().map(|s| status_hash(*s)).collect();
        let sockets: Vec<Option<usize>> = neighbour_sockets(view, catalog, g.cell)
            .iter()
            .map(|k| k.and_then(|k| SocketKind::ALL.iter().position(|&s| s == k)))
            .collect();
        out.push((
            Key::Ghost(g.cell),
            sig_of(&(
                &g.tile, g.rotation, g.level, g.valid, edges, sockets, g.pinned, meshes, &g.tunnel,
            )),
            true,
        ));
    }
    if let Some((cell, allowed)) = view.bulldoze {
        let cells: Vec<Axial> = view.tiles.iter().map(|t| t.cell).collect();
        let cut = if allowed || view.bulldoze_cover {
            vec![]
        } else {
            cut_off(&cells, cell)
        };
        let lifts: Vec<u64> = std::iter::once(cell)
            .chain(cut.iter().copied())
            .map(|c| tile_sig(view, catalog, c))
            .collect();
        out.push((Key::Bulldoze(cell), sig_of(&(allowed, cut, lifts)), true));
    }
    out
}

/// The parts of `key`'s element. Those on placed tiles (and the ghost's, on a sloped ghost) are
/// draped over sloped ground; the free slots' outlines lie flat on their slot level.
fn parts_for(key: Key, view: &PlayView, catalog: &TileCatalog, meshes: bool) -> Vec<PartSpec> {
    let parts = match key {
        Key::Ring => return ring_parts(view),
        Key::Hover(c) => return hover_parts(c, lift(view, c)),
        Key::TileHover(c) => tile_hover_parts(c, lift(view, c)),
        Key::Ghost(_) => ghost_parts(view, catalog, meshes),
        Key::Bulldoze(_) => bulldoze_parts(view),
        Key::Flash(c, _) => flash_parts(c, lift(view, c)),
        Key::Waiting(c) => waiting_parts(c, lift(view, c)),
    };
    let slopes = slopes(view, catalog, matches!(key, Key::Ghost(_)));
    parts.into_iter().map(|p| drape(p, &slopes)).collect()
}

/// The HUD motion tokens (`.impeccable/design.json`: `--t-fast`, `--t`, `--t-slow`) in seconds.
/// Every in-world state-change fade, turn and pulse takes one of them, on [`ease`] (owner
/// 2026-10-21: the engine's feedback runs on the HUD's motion scale).
pub const T_FAST: f32 = 0.12;
pub const T: f32 = 0.18;
pub const T_SLOW: f32 = 0.24;
/// The ghost waits this long after a slot is hovered before it fades in (`in-world.md`, Ghost): a
/// hover delay, not a transition.
pub const GHOST_DELAY: f32 = 0.15;
/// The commit flash's ink edges fade over `--t-fast` (owner A14 Q4, 2026-10-21; `in-world.md`,
/// Place, specified 80 ms).
const FLASH: f32 = T_FAST;
/// The ghost leaves its cell over this: the outline here, and its meshes in [`super::ghost`].
pub const GHOST_OUT: f32 = T_FAST;

/// Fade timings (s), on the motion tokens: (in, out, delay in). Bulldoze mode's changes (the ring
/// leaving and the target arriving) share `T`, as the HUD's bulldoze dim does.
fn timing(key: Key) -> (f32, f32, f32) {
    match key {
        Key::Ring => (T, T, 0.0),
        Key::Hover(_) | Key::TileHover(_) => (T_FAST, T_FAST, 0.0),
        Key::Ghost(_) => (T_FAST, GHOST_OUT, GHOST_DELAY),
        Key::Bulldoze(_) => (T, T, 0.0),
        Key::Flash(..) => (0.0, FLASH, 0.0),
        Key::Waiting(_) => (T, T_FAST, 0.0),
    }
}

fn spawn_parts(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<AffordanceMaterial>,
    root: Entity,
    specs: Vec<PartSpec>,
) -> Vec<Part> {
    specs
        .into_iter()
        .map(|s| {
            let material = materials.add(AffordanceMaterial {
                params: s.params,
                layer: s.layer,
            });
            let mesh = meshes.add(s.mesh.clone());
            let entity = commands
                .spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::IDENTITY,
                    NoFrustumCulling,
                    NotShadowCaster,
                    SceneEntity,
                    ChildOf(root),
                ))
                .id();
            Part {
                entity,
                material,
                base: s.params,
                anim: s.anim,
                mesh,
                source: s.mesh,
                on_top: s.on_top,
                shown: 0,
            }
        })
        .collect()
}

/// Matches the elements to the view: new ones fade in, gone ones fade out, changed ones rebuild
/// in place with their glow fading in again. Commit flashes start on each placement's rise.
#[allow(clippy::too_many_arguments)]
pub fn sync(
    mut commands: Commands,
    active: Res<ActiveScenario>,
    view: Res<PlayView>,
    catalog: Res<PlayCatalog>,
    reduced: Res<ReducedMotion>,
    drawn: Res<GhostDrawn>,
    ghost: Option<Res<GhostMeshes>>,
    mut aff: ResMut<Affordances>,
    mut cues: MessageReader<RiseMessage>,
    (mut meshes, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<AffordanceMaterial>>),
    waiting: Option<Res<PlayWaiting>>,
) {
    if active.0 != Scenario::Play {
        cues.clear();
        return;
    }
    let instant = reduced.0;
    let ghost_meshes = drawn.0;
    let mut flashes = vec![];
    for m in cues.read() {
        if m.cue == RiseCue::Stage(RiseStage::Column) && !m.skipped {
            flashes.push(m.cell);
        }
    }
    let aff = aff.as_mut();
    for cell in flashes {
        aff.flashes += 1;
        aff.cut_dirty = true;
        let key = Key::Flash(cell, aff.flashes);
        let root = commands
            .spawn((Transform::IDENTITY, Visibility::default(), SceneEntity))
            .id();
        let parts = spawn_parts(
            &mut commands,
            &mut meshes,
            &mut materials,
            root,
            parts_for(key, &view, &catalog.0, ghost_meshes),
        );
        let mut alpha = Fade::at(1.0);
        alpha.to(0.0, FLASH, 0.0);
        aff.elements.push(Element {
            key,
            sig: 0,
            root,
            parts,
            alpha,
            glow: Glow::at(1.0),
            fading: vec![],
            glow_out: Fade::at(0.0),
            leaving: true,
        });
    }
    if view.refusals != aff.refusals {
        aff.refusals = view.refusals;
        aff.pulse = (!instant).then_some(0.0);
    }
    if !view.is_changed()
        && !drawn.is_changed()
        && !waiting.as_ref().is_some_and(|w| w.is_changed())
    {
        return;
    }
    let want = wanted(
        &view,
        &catalog.0,
        waiting.as_deref().map_or(&[], |w| w.0.as_slice()),
        ghost_meshes,
    );
    let keys: HashSet<Key> = want.iter().filter(|w| w.2).map(|w| w.0).collect();
    // A rebuilt or new element's parts are whole until `occlude` cuts them.
    aff.cut_dirty = true;
    let turning = ghost.as_ref().is_some_and(|g| g.turning());
    for el in aff.elements.iter_mut() {
        if !keys.contains(&el.key) && !matches!(el.key, Key::Flash(..)) {
            let (_, out, _) = timing(el.key);
            el.alpha.to(0.0, if instant { 0.0 } else { out }, 0.0);
            el.leaving = true;
        }
    }
    for (key, sig, shown) in want {
        if !shown {
            continue;
        }
        let (fade_in, _, delay) = timing(key);
        let (fade_in, delay) = if instant {
            (0.0, 0.0)
        } else {
            (fade_in, delay)
        };
        match aff.elements.iter_mut().find(|e| e.key == key) {
            Some(el) => {
                el.leaving = false;
                el.alpha.to(1.0, fade_in, 0.0);
                if el.sig != sig {
                    el.sig = sig;
                    // R: the old rotation's glow fades out while the ghost turns; the rest goes now.
                    let turning = turning && matches!(key, Key::Ghost(_)) && !instant;
                    let g = el.glow.value();
                    for p in el.fading.drain(..) {
                        commands.entity(p.entity).try_despawn();
                    }
                    for p in std::mem::take(&mut el.parts) {
                        if turning && g > 0.0 && matches!(p.anim, Anim::Glow | Anim::GlowPulse) {
                            el.fading.push(p);
                        } else {
                            commands.entity(p.entity).try_despawn();
                        }
                    }
                    el.glow_out = Fade::at(g);
                    el.glow_out.to(0.0, T_FAST, 0.0);
                    el.parts = spawn_parts(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        el.root,
                        parts_for(key, &view, &catalog.0, ghost_meshes),
                    );
                    el.glow.restart(turning, instant);
                }
            }
            None => {
                let root = commands
                    .spawn((Transform::IDENTITY, Visibility::default(), SceneEntity))
                    .id();
                let parts = spawn_parts(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    root,
                    parts_for(key, &view, &catalog.0, ghost_meshes),
                );
                let mut alpha = Fade::at(0.0);
                alpha.to(1.0, fade_in, delay);
                aff.elements.push(Element {
                    key,
                    sig,
                    root,
                    parts,
                    alpha,
                    glow: Glow::at(1.0),
                    fading: vec![],
                    glow_out: Fade::at(0.0),
                    leaving: false,
                });
            }
        }
    }
}

/// What a cut mesh holds, for [`Part::shown`]: a hash of its positions, never 0.
fn mesh_sig(mesh: &Mesh) -> u64 {
    let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return 1;
    };
    let bits: Vec<[u32; 3]> = pos.iter().map(|p| p.map(f32::to_bits)).collect();
    sig_of(&bits).max(1)
}

/// Cuts every affordance part where raised ground hides it from the camera (P4): after the view
/// changes the ground ([`terrain`]), after `sync` spawns parts, and whenever the eye moves while
/// there is raised ground. A part's mesh is rewritten in place (its fades and glows run on) and
/// only when its cut changes. With no raised or sloped tile nothing is tested and every part draws
/// whole, as before P4.
pub fn occlude(
    active: Res<ActiveScenario>,
    view: Res<PlayView>,
    catalog: Res<PlayCatalog>,
    mut aff: ResMut<Affordances>,
    cams: Query<&GlobalTransform, crate::post::GameCamera>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if active.0 != Scenario::Play {
        return;
    }
    let aff = aff.bypass_change_detection();
    if view.is_changed() {
        let ground = terrain(&view, &catalog.0);
        if ground != aff.terrain {
            aff.terrain = ground;
            aff.cut_dirty = true;
        }
    }
    let eye = cams.iter().next().map(GlobalTransform::translation);
    let moved = match (eye, aff.cut_eye) {
        (Some(e), Some(last)) => e.distance(last) > 0.02,
        (e, last) => e.is_some() != last.is_some(),
    };
    if !aff.cut_dirty && !(moved && aff.terrain.is_some()) {
        return;
    }
    aff.cut_dirty = false;
    aff.cut_eye = eye;
    let Affordances {
        elements, terrain, ..
    } = aff;
    for part in elements
        .iter_mut()
        .flat_map(|e| e.parts.iter_mut().chain(e.fading.iter_mut()))
    {
        let cut = match (terrain.as_ref(), eye) {
            (Some(t), Some(eye)) if !part.on_top => {
                occlude_mesh(&part.source, part.base.misc.y, t, eye)
            }
            _ => None,
        };
        let shown = cut.as_ref().map_or(0, mesh_sig);
        if shown == part.shown {
            continue;
        }
        part.shown = shown;
        if let Some(mut m) = meshes.get_mut(&part.mesh) {
            *m = cut.unwrap_or_else(|| part.source.clone());
        }
    }
}

/// The ghost fill's breathing: 1 to 0.55 and back over 2.4 s, ease-in-out.
pub fn breath(t: f32) -> f32 {
    1.0 - 0.45 * (0.5 - 0.5 * (std::f32::consts::TAU * t / 2.4).cos())
}

/// The refusal pulse over `T_SLOW` (240 ms, as the HUD tag's pulse), 0 -> 1 -> 0.
fn pulse(t: f32) -> f32 {
    let u = (t / T_SLOW).clamp(0.0, 1.0);
    (std::f32::consts::PI * u).sin()
}

fn widen(mut p: AffordanceParams, k: f32) -> AffordanceParams {
    // Core 7 -> 13 px, halos 6/14 -> 10/24 px blur at their peak alphas (1, 0.7).
    p.shape.x *= 1.0 + (13.0 / 7.0 - 1.0) * k;
    p.halo.x = p.halo.x * (1.0 + (5.0 / 3.0 - 1.0) * k) + 2.0 * k * f32::from(p.halo.x == 0.0);
    p.halo.y = p.halo.y + (1.0 - p.halo.y) * k;
    p.halo.z = p.halo.z * (1.0 + (12.0 / 7.0 - 1.0) * k) + 5.0 * k * f32::from(p.halo.z == 0.0);
    p.halo.w = p.halo.w + (0.7 - p.halo.w) * k;
    p
}

/// Advances the fades, the breathing and the pulse, and writes each part's uniform when it moves.
pub fn animate(
    time: Res<Time>,
    mut commands: Commands,
    active: Res<ActiveScenario>,
    reduced: Res<ReducedMotion>,
    ghost: Option<Res<GhostMeshes>>,
    mut aff: ResMut<Affordances>,
    mut materials: ResMut<Assets<AffordanceMaterial>>,
    window: Query<&Window, With<PrimaryWindow>>,
) {
    if active.0 != Scenario::Play || aff.bypass_change_detection().elements.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    let turning = ghost.as_ref().is_some_and(|g| g.turning());
    let scale = window.iter().next().map_or(1.0, |w| w.scale_factor());
    let aff = aff.as_mut();
    aff.breath += dt;
    let breathing = if reduced.0 { 1.0 } else { breath(aff.breath) };
    let pulse_k = match aff.pulse.as_mut() {
        Some(t) => {
            *t += dt;
            let k = pulse(*t);
            if *t >= T_SLOW {
                aff.pulse = None;
            }
            k
        }
        None => 0.0,
    };
    aff.elements.retain_mut(|el| {
        el.alpha.t += dt;
        el.glow.step(turning, reduced.0, dt);
        el.glow_out.t += dt;
        let a = el.alpha.value();
        if el.leaving && el.alpha.done() && a <= 1e-3 {
            commands.entity(el.root).try_despawn();
            return false;
        }
        if !el.fading.is_empty() && (el.glow_out.done() || reduced.0) {
            for p in el.fading.drain(..) {
                commands.entity(p.entity).try_despawn();
            }
        }
        let out = el.glow_out.value();
        for p in &el.fading {
            let mut q = p.base;
            q.misc.z = scale;
            q.color.w *= a * out;
            q.hatch.w *= a;
            if let Some(m) = materials.get(&p.material)
                && m.params != q
                && let Some(mut m) = materials.get_mut(&p.material)
            {
                m.params = q;
            }
        }
        let g = el.glow.value();
        for p in &el.parts {
            let mut q = p.base;
            q.misc.z = scale;
            q.color.w *= a;
            q.hatch.w *= a;
            match p.anim {
                Anim::Plain => {}
                Anim::Breath => q.color.w *= breathing,
                Anim::Glow => q.color.w *= g,
                Anim::GlowPulse => {
                    q.color.w *= g;
                    q = widen(q, pulse_k);
                }
                Anim::Pulse => q = widen(q, pulse_k),
            }
            if let Some(m) = materials.get(&p.material)
                && m.params != q
                && let Some(mut m) = materials.get_mut(&p.material)
            {
                m.params = q;
            }
        }
        true
    });
}

#[cfg(test)]
mod tests {
    /// The engine's motion constants are the HUD's tokens, read from the design sidecar the HUD
    /// is built from, and every fade in the timing table is one of them.
    #[test]
    fn feedback_runs_on_the_hud_motion_tokens() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../.impeccable/design.json");
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).expect("design.json")).unwrap();
        let token = |name: &str| -> f32 {
            let v = json["extensions"]["motion"]
                .as_array()
                .unwrap()
                .iter()
                .find(|m| m["name"] == name)
                .unwrap_or_else(|| panic!("token {name}"))["value"]
                .as_str()
                .unwrap()
                .to_owned();
            v.trim_end_matches("ms").parse::<f32>().unwrap() / 1000.0
        };
        assert_eq!(T_FAST, token("t-fast"));
        assert_eq!(T, token("t"));
        assert_eq!(T_SLOW, token("t-slow"));
        let on_scale = |v: f32| [0.0, T_FAST, T, T_SLOW].contains(&v);
        for key in [
            Key::Ring,
            Key::Hover(Axial::ORIGIN),
            Key::TileHover(Axial::ORIGIN),
            Key::Ghost(Axial::ORIGIN),
            Key::Bulldoze(Axial::ORIGIN),
            Key::Flash(Axial::ORIGIN, 0),
            Key::Waiting(Axial::ORIGIN),
        ] {
            let (fade_in, fade_out, _) = timing(key);
            assert!(
                on_scale(fade_in) && on_scale(fade_out),
                "{key:?}: {fade_in} / {fade_out}"
            );
        }
        assert_eq!(timing(Key::Ghost(Axial::ORIGIN)).1, GHOST_OUT);
        assert_eq!(
            GHOST_OUT, T_FAST,
            "the ghost's meshes and outline leave on --t-fast"
        );
        assert_eq!(pulse(T_SLOW / 2.0), 1.0);
        assert_eq!(pulse(T_SLOW), (std::f32::consts::PI).sin());
    }

    /// The commit flash's ink edges fade over `--t-fast` (owner A14 Q4), no longer 80 ms.
    #[test]
    fn the_commit_flash_fades_over_t_fast() {
        assert_eq!(FLASH, 0.12);
        assert_eq!(timing(Key::Flash(Axial::ORIGIN, 0)), (0.0, T_FAST, 0.0));
        let mut f = Fade::at(1.0);
        f.to(0.0, FLASH, 0.0);
        f.t = 0.08;
        assert!(f.value() > 0.0, "still showing at the old 80 ms");
        f.t = T_FAST;
        assert_eq!(f.value(), 0.0);
        assert!(f.done());
    }

    /// The ghost turn as `ghost::sync` runs it, one frame at a time: started on the press frame and
    /// advanced in it, landed on the first frame it has run `GHOST_TURN`.
    struct TurnSim(Option<f32>);
    impl TurnSim {
        fn step(&mut self, dt: f32) {
            if let Some(t) = self.0.as_mut() {
                *t += dt;
                if *t >= crate::play::ghost::GHOST_TURN {
                    self.0 = None;
                }
            }
        }
    }

    /// Owner A14 Q2: after R the edge glow comes back when the ghost's turn lands (`in-world.md`,
    /// Rotate), not 60 ms before. It waits at 0 through the turn, its first visible frame is the
    /// landing frame, and it is full `T_FAST` later; a press mid-turn holds it until the last turn
    /// lands; under reduced motion it is full at once.
    #[test]
    fn the_edge_glow_fades_in_when_the_ghost_turn_lands() {
        let dt = 1.0 / 60.0;
        // Frame 0 is the press: the ghost starts and advances its turn, the affordances rebuild.
        let run = |presses: &[usize], instant: bool| -> Vec<(bool, f32)> {
            let mut turn = TurnSim(None);
            let mut glow = Glow::at(1.0);
            (0..40)
                .map(|f| {
                    if presses.contains(&f) {
                        turn.0 = (!instant).then_some(0.0);
                    }
                    turn.step(dt);
                    let turning = turn.0.is_some();
                    if presses.contains(&f) {
                        glow.restart(turning, instant);
                    }
                    glow.step(turning, instant, dt);
                    (turning, glow.value())
                })
                .collect()
        };
        let frames = run(&[0], false);
        let landed = frames.iter().position(|(turning, _)| !turning).unwrap();
        assert_eq!(landed, 10, "the 180 ms turn lands on frame 10 (t = 183 ms)");
        assert!(
            frames[..landed].iter().all(|&(_, g)| g == 0.0),
            "{frames:?}"
        );
        assert!(frames[landed].1 > 0.0, "visible on the landing frame");
        let full = landed + (T_FAST / dt).ceil() as usize - 1;
        assert!(
            frames[full - 1].1 < 1.0 && frames[full].1 == 1.0,
            "{frames:?}"
        );
        // Two presses: the second (frame 5) restarts the turn and the hold.
        let frames = run(&[0, 5], false);
        let landed = frames.iter().position(|(turning, _)| !turning).unwrap();
        assert_eq!(landed, 15);
        assert!(frames[..landed].iter().all(|&(_, g)| g == 0.0));
        assert!(frames[landed].1 > 0.0);
        // Reduced motion: the ghost snaps and the glow is full on the press frame.
        assert_eq!(run(&[0], true)[0].1, 1.0);
        // A change with no turn (a tile change, a ghost whose pack streams) fades in at once.
        let mut glow = Glow::at(1.0);
        glow.restart(false, false);
        glow.step(false, false, dt);
        assert!(glow.value() > 0.0);
    }

    use super::*;
    use crate::play::{PlayGhost, PlayTile};

    #[test]
    fn line_mesh_has_a_quad_per_segment() {
        let m = line_mesh(&[(Vec3::ZERO, Vec3::X), (Vec3::X, Vec3::Z)]);
        assert_eq!(m.count_vertices(), 8);
        assert_eq!(m.indices().unwrap().len(), 12);
        assert!(m.attribute(Mesh::ATTRIBUTE_COLOR).is_some());
    }

    #[test]
    fn edges_run_clockwise_from_above_with_the_inside_on_the_right() {
        // Viewed from +Y with +X right and +Z down the screen, increasing angle turns clockwise;
        // the right of a -> b (x, z) is (-dz, dx)... toward the centre.
        for e in 0..6u8 {
            let (a, b) = edge_seg(Vec3::ZERO, e, 10.0);
            let d = b - a;
            let right = Vec3::new(-d.z, 0.0, d.x);
            let mid = (a + b) * 0.5;
            assert!(right.dot(-mid) > 0.0, "edge {e}");
            let m = dir(60.0 * e as f32) * 10.0 * 0.866_025_4;
            assert!(mid.distance(m) < 1e-3, "edge {e} midpoint faces edge_dir");
        }
    }

    /// A bulldoze refused as a tunnel cover cuts nothing off, so no cut hatch draws (the HUD's
    /// `cutOff` returns nothing for a cover), though the same line of tiles hatches its far end
    /// when the refusal is a disconnect.
    #[test]
    fn a_tunnel_cover_refusal_draws_no_cut_hatch() {
        let catalog = TileCatalog::builtin().unwrap();
        let tile = |q| PlayTile {
            cell: Axial::new(q, 0),
            tile: "meadow".into(),
            rotation: 0,
            level: 0,
        };
        let mut view = PlayView {
            tiles: vec![tile(0), tile(1), tile(2)],
            bulldoze: Some((Axial::new(1, 0), false)),
            ..default()
        };
        let hatched = parts_for(Key::Bulldoze(Axial::new(1, 0)), &view, &catalog, true).len();
        assert!(hatched > 1, "a disconnect hatches the cut part");
        let signature = |v: &PlayView| wanted(v, &catalog, &[], true);
        let before = signature(&view);
        view.bulldoze_cover = true;
        let plain = parts_for(Key::Bulldoze(Axial::new(1, 0)), &view, &catalog, true).len();
        assert_eq!(plain, 1, "the dashed outline alone");
        assert_ne!(before, signature(&view));
    }

    #[test]
    fn cut_off_is_every_part_but_the_largest() {
        let a = |q, r| Axial::new(q, r);
        // A line of three: removing the middle splits off one end.
        let line = [a(0, 0), a(1, 0), a(2, 0)];
        assert_eq!(cut_off(&line, a(1, 0)), vec![a(2, 0)]);
        assert!(cut_off(&line, a(2, 0)).is_empty());
        // Larger part stays.
        let tee = [a(0, 0), a(1, 0), a(2, 0), a(3, 0), a(-1, 0)];
        assert_eq!(cut_off(&tee, a(0, 0)), vec![a(-1, 0)]);
    }

    #[test]
    fn legend_curve_and_breathing() {
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
        assert!(ease(0.25) > 0.6, "fast out of the gate: {}", ease(0.25));
        let mut prev = 0.0;
        for i in 1..=20 {
            let v = ease(i as f32 / 20.0);
            assert!(v >= prev);
            prev = v;
        }
        assert!((breath(0.0) - 1.0).abs() < 1e-6);
        assert!((breath(1.2) - 0.55).abs() < 1e-5);
        assert!((breath(2.4) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn fade_waits_for_its_delay() {
        let mut f = Fade::at(0.0);
        f.to(1.0, 0.12, 0.15);
        f.t = 0.1;
        assert_eq!(f.value(), 0.0);
        f.t = 0.27;
        assert!((f.value() - 1.0).abs() < 1e-6);
        assert!(f.done());
    }

    fn shown(view: &PlayView, waiting: &[Axial]) -> Vec<Key> {
        let catalog = TileCatalog::builtin().unwrap();
        wanted(view, &catalog, waiting, true)
            .into_iter()
            .filter(|w| w.2)
            .map(|w| w.0)
            .collect()
    }

    /// A raised tile's affordances (P4) sit at its cap, not on the level-0 ground: the bulldoze
    /// target, the hover rim, the placement flash and the waiting outline all lift by its level.
    #[test]
    fn a_raised_tiles_affordances_lift_to_its_level() {
        let catalog = TileCatalog::builtin().unwrap();
        let cell = Axial::new(1, 0);
        let view = PlayView {
            tiles: vec![
                PlayTile {
                    cell: Axial::ORIGIN,
                    tile: "meadow".into(),
                    rotation: 0,
                    level: 0,
                },
                PlayTile {
                    cell,
                    tile: "meadow".into(),
                    rotation: 0,
                    level: 2,
                },
            ],
            bulldoze: Some((cell, true)),
            ..default()
        };
        let ys = |parts: Vec<PartSpec>| -> Vec<f32> {
            parts
                .iter()
                .flat_map(|p| {
                    let Some(VertexAttributeValues::Float32x3(a)) =
                        p.mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                    else {
                        panic!("positions")
                    };
                    a.iter().map(|q| q[1]).collect::<Vec<_>>()
                })
                .collect()
        };
        let top = 2.0 * LEVEL_STEP;
        let bulldoze = ys(parts_for(Key::Bulldoze(cell), &view, &catalog, true));
        assert!(!bulldoze.is_empty());
        for y in bulldoze {
            assert!((y - (top + CAP_Y)).abs() < 1e-4, "bulldoze at {y}");
        }
        for key in [Key::TileHover(cell), Key::Flash(cell, 1)] {
            for y in ys(parts_for(key, &view, &catalog, true)) {
                assert!((y - (top + CAP_Y)).abs() < 1e-4, "{key:?} at {y}");
            }
        }
        for y in ys(parts_for(Key::Waiting(cell), &view, &catalog, true)) {
            assert!((y - (top + GROUND_Y)).abs() < 1e-4, "waiting at {y}");
        }
        // The level-0 start tile's rim stays on the ground.
        for y in ys(parts_for(
            Key::TileHover(Axial::ORIGIN),
            &view,
            &catalog,
            true,
        )) {
            assert!((y - CAP_Y).abs() < 1e-4);
        }
    }

    /// A raised portal ghost's tunnel preview (P4): one faint dotted line at the road's height
    /// from the mouth to the far side of the last cover cell; no line without cover cells.
    #[test]
    fn a_portal_ghost_draws_its_tunnel_as_a_dotted_line_at_road_height() {
        let catalog = TileCatalog::builtin().unwrap();
        let ghost = |tunnel: Vec<Axial>| PlayView {
            ghost: Some(PlayGhost {
                cell: Axial::ORIGIN,
                tile: "road_tunnel_portal".into(),
                rotation: 0,
                level: 1,
                edges: [EdgeStatus::Neutral; 6],
                valid: true,
                pinned: false,
                tunnel,
            }),
            ..default()
        };
        let dotted = |v: &PlayView| -> Vec<Vec3> {
            ghost_parts(v, &catalog, true)
                .into_iter()
                .filter(|p| p.params.shape.y == 2.0 && p.params.shape.z == 7.0)
                .flat_map(|p| {
                    // `line_mesh`: POSITION is a segment's start, COLOR.xyz its end.
                    let (
                        Some(VertexAttributeValues::Float32x3(a)),
                        Some(VertexAttributeValues::Float32x4(b)),
                    ) = (
                        p.mesh.attribute(Mesh::ATTRIBUTE_POSITION),
                        p.mesh.attribute(Mesh::ATTRIBUTE_COLOR),
                    )
                    else {
                        panic!("line attributes")
                    };
                    a.iter()
                        .map(|q| Vec3::from(*q))
                        .chain(b.iter().map(|q| Vec3::new(q[0], q[1], q[2])))
                        .collect::<Vec<_>>()
                })
                .collect()
        };
        assert!(dotted(&ghost(vec![])).is_empty());
        let cells = vec![Axial::new(1, 0), Axial::new(2, 0)];
        let pts = dotted(&ghost(cells));
        assert_eq!(pts.len(), 8, "one segment");
        for q in &pts {
            assert!((q.y - (LEVEL_STEP + TUNNEL_Y)).abs() < 1e-5, "{q}");
        }
        let [x0, _] = Axial::ORIGIN.to_world(HEX_RADIUS);
        let [x2, _] = Axial::new(2, 0).to_world(HEX_RADIUS);
        let step = x2 / 2.0;
        let (lo, hi) = pts.iter().fold((f32::MAX, f32::MIN), |(lo, hi), q| {
            (lo.min(q.x), hi.max(q.x))
        });
        assert!(
            (lo - (x0 + step / 2.0)).abs() < 1e-3,
            "starts at the mouth: {lo}"
        );
        assert!(
            (hi - (x2 + step / 2.0)).abs() < 1e-3,
            "ends past the last cover: {hi}"
        );
    }

    /// Every vertex position of a part's mesh, line ends (`line_mesh`'s COLOR.xyz) included.
    fn points(p: &PartSpec) -> Vec<Vec3> {
        let Some(VertexAttributeValues::Float32x3(a)) = p.mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions")
        };
        let mut out: Vec<Vec3> = a.iter().map(|q| Vec3::from(*q)).collect();
        if p.params.misc.y != MODE_FILL
            && let Some(VertexAttributeValues::Float32x4(b)) =
                p.mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        {
            out.extend(b.iter().map(|q| Vec3::new(q[0], q[1], q[2])));
        }
        out
    }

    /// Elevation (P4): each free slot's dashed outline sits on its slot level (`PlayView::
    /// slot_levels`, the level the selected tile would take there), the hovered slot's with it;
    /// a slot not listed stays on the ground.
    #[test]
    fn a_free_slots_outline_lifts_to_its_slot_level() {
        let catalog = TileCatalog::builtin().unwrap();
        let (low, high) = (Axial::new(-1, 0), Axial::new(2, 0));
        let view = PlayView {
            frontier: vec![low, high],
            slot_levels: [(high, 1)].into_iter().collect(),
            pointer_slot: Some(high),
            ..default()
        };
        let ring = parts_for(Key::Ring, &view, &catalog, true);
        let mut seen = [0, 0];
        for q in ring.iter().flat_map(points) {
            let near = |c: Axial| q.xz().distance(Vec2::from(c.to_world(HEX_RADIUS))) < HEX_RADIUS;
            if near(high) {
                assert!((q.y - (LEVEL_STEP + GROUND_Y)).abs() < 1e-4, "{q}");
                seen[1] += 1;
            } else {
                assert!(near(low));
                assert!((q.y - GROUND_Y).abs() < 1e-4, "{q}");
                seen[0] += 1;
            }
        }
        assert!(seen[0] > 0 && seen[1] > 0, "{seen:?}");
        for q in parts_for(Key::Hover(high), &view, &catalog, true)
            .iter()
            .flat_map(points)
        {
            assert!((q.y - (LEVEL_STEP + GROUND_Y)).abs() < 1e-4, "hover {q}");
        }
    }

    /// The ground (m over its level) of `tile` placed at `cell` with `rotation`, under world `q`.
    fn field(catalog: &TileCatalog, tile: &str, cell: Axial, rotation: u8, q: Vec3) -> f32 {
        let def = catalog.get(tile).unwrap();
        let d = q.xz() - Vec2::from(cell.to_world(HEX_RADIUS));
        let l = crate::play::scatter::turn((6 - rotation % 6) % 6, [d.x, 0.0, d.y]);
        hex_core::slope::field_at(def.ground, def.corners(0), l[0], l[2])
    }

    /// Elevation (P4): affordances are depth-tested against the world, so on a sloped tile they
    /// follow its ground (level + field + their offset) instead of a flat plane its high side
    /// would bury: the hover rim, the bulldoze outline and fill, and a sloped ghost's outline.
    #[test]
    fn affordances_drape_over_a_sloped_tile() {
        let catalog = TileCatalog::builtin().unwrap();
        let (cell, tile, rotation) = (Axial::new(1, 0), "hillside_000011", 2);
        let view = PlayView {
            tiles: vec![PlayTile {
                cell,
                tile: tile.into(),
                rotation,
                level: 1,
            }],
            bulldoze: Some((cell, true)),
            ..default()
        };
        let check = |key: Key, view: &PlayView, offset: f32| {
            let parts = parts_for(key, view, &catalog, true);
            let mut rise = 0.0f32;
            for q in parts.iter().flat_map(points) {
                let want = LEVEL_STEP + field(&catalog, tile, cell, rotation, q) + offset;
                assert!((q.y - want).abs() < 2e-3, "{key:?} at {q}: want {want}");
                rise = rise.max(q.y - LEVEL_STEP - offset);
            }
            assert!(rise > 3.0, "{key:?} climbs the slope: {rise}");
        };
        check(Key::TileHover(cell), &view, CAP_Y);
        check(Key::Bulldoze(cell), &view, CAP_Y);
        let ghost = PlayView {
            ghost: Some(PlayGhost {
                cell,
                tile: tile.into(),
                rotation,
                level: 1,
                edges: [EdgeStatus::Neutral; 6],
                valid: true,
                pinned: false,
                tunnel: vec![],
            }),
            ..default()
        };
        check(Key::Ghost(cell), &ghost, CAP_Y);
        // A flat tile's affordances are the flat outlines as before.
        let flat = PlayView {
            tiles: vec![PlayTile {
                cell,
                tile: "meadow".into(),
                rotation: 0,
                level: 1,
            }],
            ..default()
        };
        let segs = parts_for(Key::TileHover(cell), &flat, &catalog, true);
        assert_eq!(
            points(&segs[0]).len(),
            6 * 4 * 2,
            "six edges, one quad each"
        );
    }

    /// Only a portal ghost's tunnel preview is never cut by raised ground: it shows the road under
    /// the cover. Every other part is tested ([`occlude`]).
    #[test]
    fn only_the_tunnel_preview_is_never_cut() {
        let catalog = TileCatalog::builtin().unwrap();
        let view = PlayView {
            ghost: Some(PlayGhost {
                cell: Axial::ORIGIN,
                tile: "road_tunnel_portal".into(),
                rotation: 0,
                level: 1,
                edges: [EdgeStatus::Incompatible; 6],
                valid: true,
                pinned: false,
                tunnel: vec![Axial::new(1, 0)],
            }),
            frontier: vec![Axial::new(0, 1)],
            ..default()
        };
        let parts = ghost_parts(&view, &catalog, false);
        let top: Vec<&PartSpec> = parts.iter().filter(|p| p.on_top).collect();
        assert_eq!(top.len(), 1);
        assert_eq!((top[0].params.shape.y, top[0].params.shape.z), (2.0, 7.0));
        for key in [Key::Ring, Key::Hover(Axial::new(0, 1))] {
            assert!(
                parts_for(key, &view, &catalog, true)
                    .iter()
                    .all(|p| !p.on_top)
            );
        }
    }

    /// A part as comparable data: params, layer, positions and colours.
    type Drawn = (AffordanceParams, u32, Vec<[f32; 3]>, Vec<[f32; 4]>);

    fn drawn(parts: &[PartSpec]) -> Vec<Drawn> {
        parts
            .iter()
            .map(|p| {
                let pos = match p.mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
                    Some(VertexAttributeValues::Float32x3(a)) => a.clone(),
                    _ => panic!("positions"),
                };
                let col = match p.mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
                    Some(VertexAttributeValues::Float32x4(a)) => a.clone(),
                    _ => panic!("colours"),
                };
                (p.params, p.layer.to_bits(), pos, col)
            })
            .collect()
    }

    /// Flat-world parity (P4 fix round 1): in a world with no raised or sloped tile there is no
    /// [`Terrain`], so `occlude` tests nothing, and every element's parts are exactly what the
    /// builders drew before P4: no drape, the ring on the ground, the bulldoze outline and its
    /// cut-off hatch, the ghost, the hover rims, the flash and the waiting outline unchanged.
    #[test]
    fn a_flat_world_draws_its_affordances_as_before() {
        let catalog = TileCatalog::builtin().unwrap();
        let cells = [Axial::new(-1, 0), Axial::ORIGIN, Axial::new(1, 0)];
        let tiles: Vec<PlayTile> = cells
            .iter()
            .zip(["meadow", "road_straight", "forest"])
            .map(|(&cell, tile)| PlayTile {
                cell,
                tile: tile.into(),
                rotation: 0,
                level: 0,
            })
            .collect();
        let slot = Axial::new(0, 1);
        let view = PlayView {
            tiles,
            frontier: vec![slot, Axial::new(0, -1)],
            ghost: Some(PlayGhost {
                cell: slot,
                tile: "meadow".into(),
                rotation: 0,
                level: 0,
                edges: [EdgeStatus::Compatible; 6],
                valid: true,
                pinned: false,
                tunnel: vec![],
            }),
            pointer_slot: Some(Axial::new(0, -1)),
            pointer_tile: Some(Axial::new(1, 0)),
            bulldoze: Some((Axial::ORIGIN, false)),
            ..default()
        };
        assert!(terrain(&view, &catalog).is_none(), "nothing to test");
        assert!(view.slot_levels.is_empty());
        let ring: Vec<_> = view
            .frontier
            .iter()
            .flat_map(|&c| hex_segs(centre(c, GROUND_Y), SLOT_R))
            .collect();
        let before = |key: Key| -> Vec<PartSpec> {
            match key {
                Key::Ring => vec![PartSpec {
                    mesh: line_mesh(&ring),
                    params: AffordanceParams::line(NEUTRAL, NEUTRAL_A, 2.0).dashed(6.0, 7.0),
                    layer: 0.0,
                    anim: Anim::Plain,
                    on_top: false,
                }],
                Key::Hover(c) => hover_parts(c, 0.0),
                Key::TileHover(c) => tile_hover_parts(c, 0.0),
                Key::Ghost(_) => ghost_parts(&view, &catalog, true),
                Key::Bulldoze(_) => bulldoze_parts(&view),
                Key::Flash(c, _) => flash_parts(c, 0.0),
                Key::Waiting(c) => waiting_parts(c, 0.0),
            }
        };
        let keys = [
            Key::Ring,
            Key::Hover(Axial::new(0, -1)),
            Key::Hover(slot),
            Key::TileHover(Axial::new(1, 0)),
            Key::Ghost(slot),
            Key::Bulldoze(Axial::ORIGIN),
            Key::Flash(Axial::new(-1, 0), 1),
            Key::Waiting(Axial::new(1, 0)),
        ];
        for key in keys {
            let parts = parts_for(key, &view, &catalog, true);
            assert!(!parts.is_empty(), "{key:?}");
            assert_eq!(drawn(&parts), drawn(&before(key)), "{key:?}");
        }
        assert!(
            bulldoze_parts(&view).len() > 1,
            "the cut-off hatch is in the check"
        );
    }

    /// Occlusion (P4 fix round 1): a point on a free slot's outline behind a level-1 plateau, seen
    /// low over it, is hidden; the same point is seen with the plateau off the line of sight (a
    /// raised tile beside it) and with no plateau there is nothing to test. A point on the
    /// plateau's own top is seen.
    #[test]
    fn raised_ground_hides_a_line_point_behind_it() {
        let catalog = TileCatalog::builtin().unwrap();
        let meadow = |cell, level| PlayTile {
            cell,
            tile: "meadow".into(),
            rotation: 0,
            level,
        };
        let at = |cell: Axial, y: f32| {
            let [x, z] = cell.to_world(HEX_RADIUS);
            Vec3::new(x, y, z)
        };
        let point = at(Axial::new(2, 0), GROUND_Y);
        let eye = at(Axial::ORIGIN, 6.0);
        let between = PlayView {
            tiles: vec![meadow(Axial::ORIGIN, 0), meadow(Axial::new(1, 0), 1)],
            ..default()
        };
        let t = terrain(&between, &catalog).unwrap();
        assert!(t.occluded(eye, point), "behind the plateau");
        assert!(!t.occluded(eye, at(Axial::new(1, 0), LEVEL_STEP + CAP_Y)));
        let beside = PlayView {
            tiles: vec![meadow(Axial::ORIGIN, 0), meadow(Axial::new(0, 2), 1)],
            ..default()
        };
        assert!(!terrain(&beside, &catalog).unwrap().occluded(eye, point));
        let flat = PlayView {
            tiles: vec![meadow(Axial::ORIGIN, 0), meadow(Axial::new(1, 0), 0)],
            ..default()
        };
        assert!(
            terrain(&flat, &catalog).is_none(),
            "no plateau: nothing hides it"
        );
        // The slot's outline is cut where the plateau hides it, and whole from high above.
        let ring = ring_parts(&PlayView {
            frontier: vec![Axial::new(2, 0)],
            ..default()
        });
        let m = &ring[0].mesh;
        // From 12 m up over the start tile the near half is behind the plateau, the far half seen.
        let high = at(Axial::ORIGIN, 12.0);
        let cut = occlude_mesh(m, MODE_LINE, &t, high).expect("partly hidden");
        let len = |m: &Mesh| -> f32 {
            segments(m)
                .unwrap()
                .iter()
                .map(|(a, b)| a.distance(*b))
                .sum()
        };
        assert!(
            len(&cut) > 0.0 && len(&cut) < len(m),
            "{} of {}",
            len(&cut),
            len(m)
        );
        assert!(occlude_mesh(m, MODE_LINE, &t, at(Axial::new(2, 0), 200.0)).is_none());
    }

    #[test]
    fn a_placed_tile_under_the_pointer_gets_the_faint_rim_in_place_mode_only() {
        let (origin, east, slot) = (Axial::ORIGIN, Axial::new(1, 0), Axial::new(0, 1));
        let tile = |cell| PlayTile {
            cell,
            tile: "meadow".into(),
            rotation: 0,
            level: 0,
        };
        // Place mode, as `hex-app` builds it with the pointer over the placed tile at (1, 0).
        let place = PlayView {
            tiles: vec![tile(origin), tile(east)],
            frontier: vec![slot],
            pointer_tile: Some(east),
            ..default()
        };
        let keys = shown(&place, &[]);
        assert!(keys.contains(&Key::TileHover(east)), "{keys:?}");
        assert!(!keys.contains(&Key::Hover(east)), "not the slot outline");
        let p = &tile_hover_parts(east, 0.0)[0].params;
        let slot_line = &hover_parts(slot, 0.0)[1].params;
        assert!(p.color.w < slot_line.color.w * 0.5 && p.shape.x < slot_line.shape.x);
        // A pinned slot elsewhere keeps its ghost beside the rim.
        let pinned = PlayView {
            ghost: Some(PlayGhost {
                cell: slot,
                tile: "meadow".into(),
                rotation: 0,
                level: 0,
                edges: [EdgeStatus::Neutral; 6],
                valid: true,
                pinned: true,
                tunnel: vec![],
            }),
            ..place.clone()
        };
        let keys = shown(&pinned, &[]);
        assert!(keys.contains(&Key::TileHover(east)) && keys.contains(&Key::Ghost(slot)));
        // Bulldoze mode: only the bulldoze target outlines the tile.
        let bulldoze = PlayView {
            frontier: vec![],
            bulldoze: Some((east, true)),
            bulldoze_mode: true,
            ..place.clone()
        };
        let keys = shown(&bulldoze, &[]);
        assert!(!keys.contains(&Key::TileHover(east)), "{keys:?}");
        assert!(keys.contains(&Key::Bulldoze(east)));
        // Photo mode: `hex-app` clears the frontier, the ghost and the pointer tile.
        let photo = PlayView {
            frontier: vec![],
            pointer_tile: None,
            ..place.clone()
        };
        assert!(
            !shown(&photo, &[])
                .iter()
                .any(|k| matches!(k, Key::TileHover(_)))
        );
        // A tile waiting for its pack keeps the waiting outline alone.
        let keys = shown(&place, &[east]);
        assert!(!keys.contains(&Key::TileHover(east)) && keys.contains(&Key::Waiting(east)));
    }

    /// While the village pack streams, a rail tile's ghost strokes its rail lanes (one per rail
    /// edge, edge midpoint to centre, in the rail socket colour, 11 px); with its meshes drawn the
    /// strokes go.
    #[test]
    fn the_ghost_strokes_rail_lanes_while_its_pack_streams() {
        let catalog = TileCatalog::builtin().unwrap();
        let ghost = |tile: &str| PlayView {
            ghost: Some(PlayGhost {
                cell: Axial::new(1, 0),
                tile: tile.into(),
                rotation: 0,
                level: 0,
                edges: [EdgeStatus::Neutral; 6],
                valid: true,
                pinned: false,
                tunnel: vec![],
            }),
            ..default()
        };
        let rail = AffordanceParams::line(socket_rgb(SocketKind::Rail), 0.6, 11.0);
        let strokes = |tile: &str, meshes: bool| -> Vec<usize> {
            ghost_parts(&ghost(tile), &catalog, meshes)
                .iter()
                .filter(|p| p.params == rail && p.layer == 3.0)
                .map(|p| p.mesh.count_vertices() / 4)
                .collect()
        };
        assert_eq!(strokes("rail_straight", false), vec![2]);
        assert_eq!(strokes("rail_junction", false), vec![3]);
        assert_eq!(strokes("rail_road_crossing", false), vec![2]);
        assert!(strokes("rail_straight", true).is_empty());
        assert!(strokes("road_straight", false).is_empty());
    }

    #[test]
    fn socket_colours_match_the_tokens() {
        assert_eq!(socket_rgb(SocketKind::River), GHOST_FILL);
        assert_eq!(socket_rgb(SocketKind::Road), [226, 104, 60]);
    }
}
