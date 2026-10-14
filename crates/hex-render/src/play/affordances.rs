//! The Legend in-world affordances (`design/ui/legend/in-world.md`): the free-slot ring, the
//! hovered slot, the ghost with its edge glow and socket key marks, the bulldoze outlines and the
//! commit flash. `hex-app` writes [`PlayView`]; this module draws it.
//!
//! Screen-space widths: every line is a quad per segment drawn with [`AffordanceMaterial`], whose
//! vertex shader (`assets/shaders/affordance.wesl`) projects both endpoints and pushes the quad's
//! corners out along the screen-space normal by the half-width in pixels, so a 2 px outline is
//! 2 px at every zoom. Dashes, the inward offset of the key marks, the cross ticks and the bulldoze
//! hatch are measured in pixels too, and every pixel size is multiplied by the window's scale
//! factor (the spec is at a device pixel ratio of 1). Halos are Gaussian falloffs of the distance
//! to the core, matching the prototype's CSS drop shadows, and the core is drawn emissive for bloom.
//! The material binds no texture, draws without depth testing, and stays out of the prepass and
//! the shadow passes.
use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::NoFrustumCulling,
    light::NotShadowCaster,
    mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology},
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{
        AsBindGroup, CompareFunction, RenderPipelineDescriptor, ShaderType,
        SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
    window::PrimaryWindow,
};
use hex_core::{Axial, EdgeStatus, HEX_RADIUS, SocketKind, TileCatalog, opposite_edge, world_edge};

use super::{
    PlayCatalog, PlayView,
    rise::{ReducedMotion, RiseCue, RiseMessage},
};
use crate::{ActiveScenario, Scenario, SceneEntity};
use hex_core::RiseStage;

pub const AFFORDANCE_SHADER: &str = "shaders/affordance.wesl";

const APOTHEM: f32 = HEX_RADIUS * 0.866_025_4;
/// Slot outlines sit at ground height, the ghost and the bulldoze outlines at the cap rim.
const GROUND_Y: f32 = 0.15;
const CAP_Y: f32 = 0.35;
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
}

struct Part {
    entity: Entity,
    material: Handle<AffordanceMaterial>,
    base: AffordanceParams,
    anim: Anim,
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
    Ghost(Axial),
    Bulldoze(Axial),
    Flash(Axial, u32),
}

struct Element {
    key: Key,
    sig: u64,
    root: Entity,
    parts: Vec<Part>,
    alpha: Fade,
    glow: Fade,
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

fn ring_parts(frontier: &[Axial]) -> Vec<PartSpec> {
    let segs: Vec<_> = frontier
        .iter()
        .flat_map(|&c| hex_segs(centre(c, GROUND_Y), SLOT_R))
        .collect();
    vec![PartSpec {
        mesh: line_mesh(&segs),
        params: AffordanceParams::line(NEUTRAL, NEUTRAL_A, 2.0).dashed(6.0, 7.0),
        layer: 0.0,
        anim: Anim::Plain,
    }]
}

fn hover_parts(cell: Axial) -> Vec<PartSpec> {
    let c = centre(cell, GROUND_Y);
    vec![
        PartSpec {
            mesh: fill_mesh(&[hex_poly(c, SLOT_R)]),
            params: AffordanceParams::fill(INK, 0.04),
            layer: 0.5,
            anim: Anim::Plain,
        },
        PartSpec {
            mesh: line_mesh(&hex_segs(c, SLOT_R)),
            params: AffordanceParams::line(INK, 0.9, 2.0).dashed(6.0, 7.0),
            layer: 1.0,
            anim: Anim::Plain,
        },
    ]
}

fn ghost_parts(view: &PlayView, catalog: &TileCatalog) -> Vec<PartSpec> {
    let Some(g) = &view.ghost else {
        return vec![];
    };
    let c = centre(g.cell, CAP_Y);
    let mut out = vec![
        PartSpec {
            mesh: fill_mesh(&[hex_poly(c, RIM_R)]),
            params: AffordanceParams::fill(GHOST_FILL, 0.12),
            layer: 2.0,
            anim: Anim::Breath,
        },
        PartSpec {
            mesh: line_mesh(&hex_segs(c, RIM_R)),
            params: AffordanceParams::line(GHOST_LINE, 0.75, 2.0).dashed(10.0, 8.0),
            layer: 4.0,
            anim: Anim::Plain,
        },
    ];
    // Lane strokes stand in for the tile's meshes (the prototype's ghost): river 22 px, road 18 px.
    if let Some(def) = catalog.get(&g.tile) {
        for (kind, width) in [(SocketKind::River, 22.0), (SocketKind::Road, 18.0)] {
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
                });
            }
        }
    }
    // Socket key marks: the neighbour's class along the middle 40% of each touched edge, 11 px in.
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
                params: AffordanceParams::line(socket_rgb(kind), 0.95, 4.0).inset(11.0),
                layer: 5.0,
                anim: Anim::Glow,
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
            });
        }
    }
    out
}

fn bulldoze_parts(view: &PlayView) -> Vec<PartSpec> {
    let Some((cell, allowed)) = view.bulldoze else {
        return vec![];
    };
    let c = centre(cell, CAP_Y);
    if allowed {
        return vec![
            PartSpec {
                mesh: fill_mesh(&[hex_poly(c, RIM_R)]),
                params: AffordanceParams::fill(INCOMPATIBLE, 0.12),
                layer: 2.0,
                anim: Anim::Plain,
            },
            PartSpec {
                mesh: line_mesh(&hex_segs(c, RIM_R)),
                params: AffordanceParams::line(INCOMPATIBLE, 1.0, 4.0)
                    .glow(GLOW, [4.0, 0.6, 0.0, 0.0]),
                layer: 6.0,
                anim: Anim::Plain,
            },
        ];
    }
    let cells: Vec<Axial> = view.tiles.iter().map(|t| t.cell).collect();
    let cut = cut_off(&cells, cell);
    let mut out = vec![PartSpec {
        mesh: line_mesh(&hex_segs(c, RIM_R)),
        params: AffordanceParams::line(INCOMPATIBLE, 1.0, 4.0)
            .dashed(10.0, 8.0)
            .glow(GLOW, [0.0, 0.0, 0.0, 0.0]),
        layer: 6.0,
        anim: Anim::Pulse,
    }];
    if !cut.is_empty() {
        let polys: Vec<_> = cut
            .iter()
            .map(|&k| hex_poly(centre(k, CAP_Y), RIM_R))
            .collect();
        let segs: Vec<_> = cut
            .iter()
            .flat_map(|&k| hex_segs(centre(k, CAP_Y), RIM_R))
            .collect();
        out.push(PartSpec {
            mesh: fill_mesh(&polys),
            params: AffordanceParams::fill(SHEET, 0.35).hatched(INK, 0.75, 10.0, 2.5),
            layer: 2.0,
            anim: Anim::Plain,
        });
        out.push(PartSpec {
            mesh: line_mesh(&segs),
            params: AffordanceParams::line(INK, 1.0, 2.0).dashed(3.0, 5.0),
            layer: 4.0,
            anim: Anim::Plain,
        });
    }
    out
}

fn flash_parts(cell: Axial) -> Vec<PartSpec> {
    vec![PartSpec {
        mesh: line_mesh(&hex_segs(centre(cell, CAP_Y), RIM_R)),
        params: AffordanceParams::line(INK, 1.0, 7.0).glow(4.0, [3.0, 0.9, 7.0, 0.6]),
        layer: 8.0,
        anim: Anim::Plain,
    }]
}

/// What the view asks for: (key, signature, whether it shows).
fn wanted(view: &PlayView, catalog: &TileCatalog) -> Vec<(Key, u64, bool)> {
    let mut out = vec![];
    let ring_on = !view.frontier.is_empty() && !view.bulldoze_mode;
    out.push((Key::Ring, sig_of(&view.frontier), ring_on));
    if let Some(g) = &view.ghost {
        out.push((Key::Hover(g.cell), 0, true));
        let edges: Vec<u8> = g.edges.iter().map(|s| status_hash(*s)).collect();
        let sockets: Vec<Option<usize>> = neighbour_sockets(view, catalog, g.cell)
            .iter()
            .map(|k| k.and_then(|k| SocketKind::ALL.iter().position(|&s| s == k)))
            .collect();
        out.push((
            Key::Ghost(g.cell),
            sig_of(&(&g.tile, g.rotation, g.valid, edges, sockets)),
            true,
        ));
    }
    if let Some((cell, allowed)) = view.bulldoze {
        let cells: Vec<Axial> = view.tiles.iter().map(|t| t.cell).collect();
        let cut = if allowed {
            vec![]
        } else {
            cut_off(&cells, cell)
        };
        out.push((Key::Bulldoze(cell), sig_of(&(allowed, cut)), true));
    }
    out
}

fn parts_for(key: Key, view: &PlayView, catalog: &TileCatalog) -> Vec<PartSpec> {
    match key {
        Key::Ring => ring_parts(&view.frontier),
        Key::Hover(c) => hover_parts(c),
        Key::Ghost(_) => ghost_parts(view, catalog),
        Key::Bulldoze(_) => bulldoze_parts(view),
        Key::Flash(c, _) => flash_parts(c),
    }
}

/// Fade timings (s) of `in-world.md`: (in, out, delay in).
fn timing(key: Key) -> (f32, f32, f32) {
    match key {
        Key::Ring => (0.16, 0.16, 0.0),
        Key::Hover(_) => (0.12, 0.12, 0.0),
        Key::Ghost(_) => (0.12, 0.10, 0.15),
        Key::Bulldoze(_) => (0.16, 0.15, 0.0),
        Key::Flash(..) => (0.0, 0.08, 0.0),
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
            let entity = commands
                .spawn((
                    Mesh3d(meshes.add(s.mesh)),
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
    mut aff: ResMut<Affordances>,
    mut cues: MessageReader<RiseMessage>,
    (mut meshes, mut materials): (ResMut<Assets<Mesh>>, ResMut<Assets<AffordanceMaterial>>),
) {
    if active.0 != Scenario::Play {
        cues.clear();
        return;
    }
    let instant = reduced.0;
    let mut flashes = vec![];
    for m in cues.read() {
        if m.cue == RiseCue::Stage(RiseStage::Column) && !m.skipped {
            flashes.push(m.cell);
        }
    }
    let aff = aff.as_mut();
    for cell in flashes {
        aff.flashes += 1;
        let key = Key::Flash(cell, aff.flashes);
        let root = commands
            .spawn((Transform::IDENTITY, Visibility::default(), SceneEntity))
            .id();
        let parts = spawn_parts(
            &mut commands,
            &mut meshes,
            &mut materials,
            root,
            parts_for(key, &view, &catalog.0),
        );
        let mut alpha = Fade::at(1.0);
        alpha.to(0.0, 0.08, 0.0);
        aff.elements.push(Element {
            key,
            sig: 0,
            root,
            parts,
            alpha,
            glow: Fade::at(1.0),
            leaving: true,
        });
    }
    if view.refusals != aff.refusals {
        aff.refusals = view.refusals;
        aff.pulse = (!instant).then_some(0.0);
    }
    if !view.is_changed() {
        return;
    }
    let want = wanted(&view, &catalog.0);
    let keys: HashSet<Key> = want.iter().filter(|w| w.2).map(|w| w.0).collect();
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
                    for p in el.parts.drain(..) {
                        commands.entity(p.entity).try_despawn();
                    }
                    el.parts = spawn_parts(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        el.root,
                        parts_for(key, &view, &catalog.0),
                    );
                    el.glow = Fade::at(0.0);
                    el.glow.to(1.0, if instant { 0.0 } else { 0.12 }, 0.0);
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
                    parts_for(key, &view, &catalog.0),
                );
                let mut alpha = Fade::at(0.0);
                alpha.to(1.0, fade_in, delay);
                aff.elements.push(Element {
                    key,
                    sig,
                    root,
                    parts,
                    alpha,
                    glow: Fade::at(1.0),
                    leaving: false,
                });
            }
        }
    }
}

/// The ghost fill's breathing: 1 to 0.55 and back over 2.4 s, ease-in-out.
pub fn breath(t: f32) -> f32 {
    1.0 - 0.45 * (0.5 - 0.5 * (std::f32::consts::TAU * t / 2.4).cos())
}

/// The 240 ms refusal pulse, 0 -> 1 -> 0.
fn pulse(t: f32) -> f32 {
    let u = (t / 0.24).clamp(0.0, 1.0);
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
    mut aff: ResMut<Affordances>,
    mut materials: ResMut<Assets<AffordanceMaterial>>,
    window: Query<&Window, With<PrimaryWindow>>,
) {
    if active.0 != Scenario::Play || aff.bypass_change_detection().elements.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    let scale = window.iter().next().map_or(1.0, |w| w.scale_factor());
    let aff = aff.as_mut();
    aff.breath += dt;
    let breathing = if reduced.0 { 1.0 } else { breath(aff.breath) };
    let pulse_k = match aff.pulse.as_mut() {
        Some(t) => {
            *t += dt;
            let k = pulse(*t);
            if *t >= 0.24 {
                aff.pulse = None;
            }
            k
        }
        None => 0.0,
    };
    aff.elements.retain_mut(|el| {
        el.alpha.t += dt;
        el.glow.t += dt;
        let a = el.alpha.value();
        if el.leaving && el.alpha.done() && a <= 1e-3 {
            commands.entity(el.root).try_despawn();
            return false;
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
    use super::*;

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

    #[test]
    fn socket_colours_match_the_tokens() {
        assert_eq!(socket_rgb(SocketKind::River), GHOST_FILL);
        assert_eq!(socket_rgb(SocketKind::Road), [226, 104, 60]);
    }
}
