//! The ground a first-person walker stands on (PRD P8, D14): per placed tile, the topmost LOD0
//! surface of its ground pieces (the same surface scatter stands on), whether it holds water, and
//! the obstacles its kit colliders put in the way. `hex-app`'s walk controller moves through it with
//! [`WalkGround::step`].
//!
//! - **Ground.** A tile's ground is [`scatter::Surface`] built from its base, centre and lane edge
//!   pieces (`scatter::ground_pieces`), sampled in the tile frame: meadow cap, road, beach slope,
//!   river banks and bed, the bridge deck. A tile whose ground pieces are outside the kit (a
//!   placeholder) stands at y = 0, the placeholder cap. Tiles hidden until their pack arrives
//!   ([`PlayWaiting`]) are not ground at all.
//! - **Water.** On a tile with water (`water::tiles::water_kind`) the surface is [`WATER_Y`]: the
//!   walker wades where the bed is at most [`MAX_WADE`] under it and stops at that contour.
//! - **Obstacles.** Houses, the windmill, the hero oak, spruces and rocks block the walker by their
//!   collider (`<piece>_collider` in the kit GLB) as a convex footprint on the ground, placed with
//!   the hero or scatter instance's transform under the tile's rest transform. Wildflowers do not
//!   block.
//! - **Props to look at.** Photo mode's click to focus ([`WalkGround::pick_prop`]) hits the same
//!   drawn obstacle instances as volumes: the collider footprint extruded from the collider's foot
//!   to the top of the piece's non-foliage LOD0 mesh, and for trees a crown of stacked cylinders
//!   fitted to the LOD0 foliage ([`PieceShape`]). Wildflowers are not pickable.
//! - **Edges.** A cell with no drawn tile is the map edge. The walker's body (a [`BODY_RADIUS`]
//!   circle) never overhangs an edge or deep water, and never climbs a step higher than
//!   [`MAX_STEP`] (a bridge railing, a house pad's lip if it were one).
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use bevy::{
    gltf::{Gltf, GltfMesh},
    math::Affine3A,
    prelude::*,
};
use hex_core::{Axial, HEX_RADIUS, TileCatalog, TileDef};

use crate::{
    ActiveScenario, Scenario,
    boids::plan::cell_at,
    kit_scene::piece_rotation,
    play::{
        PlayCatalog, PlayKit, PlayTile, PlayView, PlayWaiting,
        packs::PlayPacks,
        scatter::{self, ScatterLayouts, Surface, TileScatter, Tri, footprint, turn},
    },
    water::tiles::{WATER_Y, water_kind},
};

/// The walker's body radius (m): how close its centre comes to a wall, a trunk or an edge.
pub const BODY_RADIUS: f32 = 0.3;
/// Deepest water (m) the walker wades into.
pub const MAX_WADE: f32 = 0.6;
/// Highest step (m) the walker climbs in one stride; railings and walls are taller.
pub const MAX_STEP: f32 = 0.4;
/// Longest sub-step (m) a move is checked in.
const SUBSTEP: f32 = 0.1;
/// Pieces whose collider blocks the walker. Wildflowers (clumps and single stems) and the ground
/// pieces do not.
pub const OBSTACLES: [&str; 10] = [
    "house_cottage",
    "house_twostorey",
    "windmill",
    "tree_hero_oak",
    "tree_forest",
    "tree_forest_b",
    "tree_forest_c",
    "rock_small",
    "rock_small_b",
    "rock_small_c",
];

/// Why a move stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Blocked {
    /// No drawn tile there: the map edge or an empty slot.
    Edge,
    /// Water deeper than [`MAX_WADE`].
    Water,
    /// A step up higher than [`MAX_STEP`].
    Steep,
    /// A collider: the piece id.
    Obstacle(String),
}

impl Blocked {
    /// `building`, `tree`, `rock`, `water`, `edge` or `steep`.
    pub fn reason(&self) -> &'static str {
        match self {
            Blocked::Edge => "edge",
            Blocked::Water => "water",
            Blocked::Steep => "steep",
            Blocked::Obstacle(p) if p.starts_with("house") || p == "windmill" => "building",
            Blocked::Obstacle(p) if p.starts_with("tree") => "tree",
            Blocked::Obstacle(_) => "rock",
        }
    }

    pub fn piece(&self) -> Option<&str> {
        match self {
            Blocked::Obstacle(p) => Some(p),
            _ => None,
        }
    }
}

/// One placed tile as ground.
#[derive(Clone)]
struct CellGround {
    tile: String,
    centre: Vec2,
    rotation: u8,
    /// `None`: a placeholder tile, flat at y = 0.
    surface: Option<Arc<Surface>>,
    water: bool,
}

/// A collider footprint in world XZ: a convex polygon.
#[derive(Clone, Debug, PartialEq)]
pub struct Obstacle {
    pub piece: String,
    pub poly: Vec<Vec2>,
}

impl Obstacle {
    /// Whether a circle of `radius` at `p` overlaps the footprint.
    pub fn hits(&self, p: Vec2, radius: f32) -> bool {
        let n = self.poly.len();
        if n < 3 {
            return false;
        }
        let mut sign = 0.0f32;
        let mut inside = true;
        for i in 0..n {
            let (a, b) = (self.poly[i], self.poly[(i + 1) % n]);
            let c = (b - a).perp_dot(p - a);
            if c != 0.0 {
                if sign == 0.0 {
                    sign = c.signum();
                } else if c.signum() != sign {
                    inside = false;
                }
            }
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
            if p.distance(a + ab * t) < radius {
                return true;
            }
        }
        inside
    }
}

/// Horizontal bands and angular sectors a tree crown is fitted in.
pub const CROWN_BANDS: usize = 10;
pub const CROWN_SECTORS: usize = 8;

/// A tree crown in its piece's frame: [`CROWN_BANDS`] horizontal bands from `y0`, each `band`
/// metres tall, split into [`CROWN_SECTORS`] wedges about `axis` (XZ), each wedge as far out as the
/// foliage reaches in it. A lopsided canopy keeps its shape on every side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crown {
    pub axis: Vec2,
    pub y0: f32,
    pub band: f32,
    pub radii: [[f32; CROWN_SECTORS]; CROWN_BANDS],
}

fn sector_dir(j: usize) -> Vec2 {
    let a = j as f32 * std::f32::consts::TAU / CROWN_SECTORS as f32;
    Vec2::new(a.cos(), a.sin())
}

impl Crown {
    /// Fitted to foliage triangles, each sampled on a barycentric grid, so a long slanted needle
    /// face widens only the bands and wedges it reaches into. `None` with no foliage.
    pub fn fit<'a>(tris: impl IntoIterator<Item = &'a Tri>) -> Option<Self> {
        let tris: Vec<&Tri> = tris.into_iter().collect();
        let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for p in tris.iter().flat_map(|t| t.iter()) {
            lo = lo.min(Vec3::from(*p));
            hi = hi.max(Vec3::from(*p));
        }
        if tris.is_empty() || hi.y - lo.y < 1e-3 {
            return None;
        }
        let axis = Vec2::new(lo.x + hi.x, lo.z + hi.z) * 0.5;
        let band = (hi.y - lo.y) / CROWN_BANDS as f32;
        let mut radii = [[0.0f32; CROWN_SECTORS]; CROWN_BANDS];
        for t in &tris {
            let [a, b, c] = t.map(Vec3::from);
            // At least two samples per band height along the longest edge.
            let edge = a.distance(b).max(b.distance(c)).max(c.distance(a));
            let n = ((edge / (band * 0.5)).ceil() as u32).clamp(2, 64);
            for i in 0..=n {
                for j in 0..=n - i {
                    let (u, v) = (i as f32 / n as f32, j as f32 / n as f32);
                    let p = a + (b - a) * u + (c - a) * v;
                    let k = (((p.y - lo.y) / band) as usize).min(CROWN_BANDS - 1);
                    let q = p.xz() - axis;
                    let turn = q.y.atan2(q.x).rem_euclid(std::f32::consts::TAU);
                    let w = ((turn / std::f32::consts::TAU * CROWN_SECTORS as f32) as usize)
                        .min(CROWN_SECTORS - 1);
                    radii[k][w] = radii[k][w].max(q.length());
                }
            }
        }
        Some(Self {
            axis,
            y0: lo.y,
            band,
            radii,
        })
    }

    /// Where `ray` (in the crown's frame) enters the crown, if it does in front of its origin.
    pub fn hit(&self, ray: Ray3d) -> Option<f32> {
        let (o, d) = (ray.origin, *ray.direction);
        let mut best: Option<f32> = None;
        for (k, row) in self.radii.iter().enumerate() {
            let y = self.y0 + self.band * k as f32;
            for (w, &r) in row.iter().enumerate() {
                if r <= 0.0 {
                    continue;
                }
                let (mut t0, mut t1) = (f32::NEG_INFINITY, f32::INFINITY);
                let (q, dd) = (o.xz() - self.axis, d.xz());
                let (u0, u1) = (sector_dir(w), sector_dir(w + 1));
                let inside = clip(&mut t0, &mut t1, o.y - y, d.y)
                    && clip(&mut t0, &mut t1, y + self.band - o.y, -d.y)
                    && clip(&mut t0, &mut t1, u0.perp_dot(q), u0.perp_dot(dd))
                    && clip(&mut t0, &mut t1, q.perp_dot(u1), dd.perp_dot(u1))
                    && clip_disc(&mut t0, &mut t1, q, dd, r);
                if let Some(t) = inside.then(|| entry(t0, t1)).flatten() {
                    best = Some(best.map_or(t, |b| b.min(t)));
                }
            }
        }
        best
    }
}

/// Möller–Trumbore: where `r` crosses triangle `t` in front of its origin.
pub fn ray_triangle(r: Ray3d, t: &Tri) -> Option<f32> {
    let [a, b, c] = t.map(Vec3::from);
    let (e1, e2, d) = (b - a, c - a, *r.direction);
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let s = r.origin - a;
    let u = s.dot(p) / det;
    let q = s.cross(e1);
    let v = d.dot(q) / det;
    let t = e2.dot(q) / det;
    (u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t > 0.0).then_some(t)
}

/// Keeps `[t0, t1]` to where `num + t * den >= 0`; false when nothing is left.
fn clip(t0: &mut f32, t1: &mut f32, num: f32, den: f32) -> bool {
    if den.abs() < 1e-12 {
        return num >= 0.0;
    }
    let t = -num / den;
    if den > 0.0 {
        *t0 = t0.max(t);
    } else {
        *t1 = t1.min(t);
    }
    t0 <= t1
}

/// Keeps `[t0, t1]` to where the XZ point `q + t * dd` lies within `r` of the origin.
fn clip_disc(t0: &mut f32, t1: &mut f32, q: Vec2, dd: Vec2, r: f32) -> bool {
    let a = dd.length_squared();
    let c = q.length_squared() - r * r;
    if a < 1e-12 {
        return c <= 0.0;
    }
    let b = 2.0 * q.dot(dd);
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return false;
    }
    let s = disc.sqrt();
    *t0 = t0.max((-b - s) / (2.0 * a));
    *t1 = t1.min((-b + s) / (2.0 * a));
    t0 <= t1
}

/// The entry `t` of a span `[t0, t1]` along a ray, if it starts in front of the origin (a volume
/// around the origin, such as a crown the camera is in, is looked out of, not at).
fn entry(t0: f32, t1: f32) -> Option<f32> {
    (t0 <= t1 && t0 >= 0.0).then_some(t0)
}

/// Where `ray` enters a vertical cylinder about `c` (XZ) of radius `r` between `y0` and `y1`.
pub fn ray_cylinder(ray: Ray3d, c: Vec2, r: f32, y0: f32, y1: f32) -> Option<f32> {
    let (o, d) = (ray.origin, *ray.direction);
    let (mut t0, mut t1) = (f32::NEG_INFINITY, f32::INFINITY);
    if !clip(&mut t0, &mut t1, o.y - y0, d.y) || !clip(&mut t0, &mut t1, y1 - o.y, -d.y) {
        return None;
    }
    if !clip_disc(&mut t0, &mut t1, o.xz() - c, d.xz(), r) {
        return None;
    }
    entry(t0, t1)
}

/// Where `ray` enters the convex footprint `poly` (XZ, either winding) extruded from `y0` to `y1`.
pub fn ray_prism(ray: Ray3d, poly: &[Vec2], y0: f32, y1: f32) -> Option<f32> {
    if poly.len() < 3 {
        return None;
    }
    let (o, d) = (ray.origin, *ray.direction);
    let (mut t0, mut t1) = (f32::NEG_INFINITY, f32::INFINITY);
    if !clip(&mut t0, &mut t1, o.y - y0, d.y) || !clip(&mut t0, &mut t1, y1 - o.y, -d.y) {
        return None;
    }
    let n = poly.len();
    let area: f32 = (0..n).map(|i| poly[i].perp_dot(poly[(i + 1) % n])).sum();
    let side = area.signum();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let e = b - a;
        if !clip(
            &mut t0,
            &mut t1,
            side * e.perp_dot(o.xz() - a),
            side * e.perp_dot(d.xz()),
        ) {
            return None;
        }
    }
    entry(t0, t1)
}

/// What a piece offers walking and looking, in its own frame: the collider footprint walls stop
/// at, the height that footprint stands to when clicked on (the collider's foot to the top of the
/// piece's non-foliage LOD0 mesh), and a tree's crown.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PieceShape {
    pub hull: Vec<Vec2>,
    /// `None` until the piece's GLB is loaded: the prop blocks by its fallback footprint but
    /// cannot be clicked.
    pub body: Option<(f32, f32)>,
    pub crown: Option<Crown>,
    /// The LOD0 triangles: a ray that enters the body or crown lands on the first of them it
    /// crosses, or on the volume where it slips through a gap in the leaves.
    pub tris: Vec<Tri>,
}

impl PieceShape {
    /// From the collider's vertices and the LOD0 triangles with their slot names. On a piece with a
    /// crown footprint (`scatter::footprint`), every LOD0 slot but `<piece>_bark` is foliage.
    pub fn from_meshes(piece: &str, collider: &[Vec3], lod0: &[(Tri, String)]) -> Self {
        let hull = hull(&collider.iter().map(|p| p.xz()).collect::<Vec<_>>());
        let crowned = footprint(piece).crown > 0.0;
        let foliage = |slot: &str| crowned && !slot.ends_with("_bark");
        let foot = collider.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let top = lod0
            .iter()
            .filter(|(_, s)| !foliage(s))
            .flat_map(|(t, _)| t.iter().map(|p| p[1]))
            .chain(collider.iter().map(|p| p.y))
            .fold(f32::NEG_INFINITY, f32::max);
        Self {
            body: (hull.len() >= 3 && foot < top).then_some((foot, top)),
            crown: Crown::fit(lod0.iter().filter(|(_, s)| foliage(s)).map(|(t, _)| t)),
            hull,
            tris: lod0.iter().map(|(t, _)| *t).collect(),
        }
    }

    /// A piece whose GLB is not loaded: the scatter ground reach as its footprint, nothing to click.
    pub fn fallback(piece: &str) -> Self {
        Self {
            hull: fallback_hull(piece),
            ..default()
        }
    }
}

/// `building`, `tree` or `rock`: what a click on `piece` reports.
pub fn prop_kind(piece: &str) -> &'static str {
    match piece {
        p if p.starts_with("house") => "building",
        p if p.starts_with("tree") => "tree",
        _ => "rock",
    }
}

/// One drawn obstacle as photo mode's click sees it: the piece's shape and where it stands.
#[derive(Clone, Debug, PartialEq)]
pub struct PickProp {
    pub piece: String,
    shape: Arc<PieceShape>,
    /// World to the piece's frame, and the placement's uniform scale.
    to_local: Affine3A,
    scale: f32,
}

impl PickProp {
    fn new(piece: &str, shape: &Arc<PieceShape>, t: &Transform) -> Option<Self> {
        (shape.body.is_some() || shape.crown.is_some()).then(|| Self {
            piece: piece.to_owned(),
            shape: shape.clone(),
            to_local: t.compute_affine().inverse(),
            scale: t.scale.x,
        })
    }

    /// Where `ray` meets the prop (metres along the world ray): the first LOD0 triangle it
    /// crosses once it enters the body or crown, else where it enters them.
    pub fn hit(&self, ray: Ray3d) -> Option<f32> {
        let d = self.to_local.transform_vector3(*ray.direction);
        let local = Ray3d::new(
            self.to_local.transform_point3(ray.origin),
            Dir3::new(d).ok()?,
        );
        let s = &self.shape;
        let body = s.body.and_then(|(a, b)| ray_prism(local, &s.hull, a, b));
        let crown = s.crown.as_ref().and_then(|c| c.hit(local));
        let volume = body.into_iter().chain(crown).reduce(f32::min)?;
        let mesh = s
            .tris
            .iter()
            .filter_map(|t| ray_triangle(local, t))
            .reduce(f32::min);
        // A local metre is `scale` world metres.
        Some(mesh.unwrap_or(volume) * self.scale)
    }
}

/// A move's result: where the feet end (y on the ground) and why it stopped short, if it did.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub pos: Vec3,
    pub blocked: Option<Blocked>,
}

/// The walkable world, rebuilt by [`sync`] whenever the drawn tiles, their scatter or the kit change.
#[derive(Resource, Default)]
pub struct WalkGround {
    cells: HashMap<Axial, CellGround>,
    obstacles: HashMap<Axial, Vec<Obstacle>>,
    /// The same obstacles as volumes for photo mode's click to focus.
    props: Vec<PickProp>,
    /// Bumped on every rebuild.
    pub revision: u64,
}

/// The convex hull of points (Andrew's monotone chain), counter-clockwise.
pub fn hull(points: &[Vec2]) -> Vec<Vec2> {
    let mut p: Vec<Vec2> = points.to_vec();
    p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    p.dedup_by(|a, b| a.distance_squared(*b) < 1e-10);
    if p.len() < 3 {
        return p;
    }
    let cross = |o: Vec2, a: Vec2, b: Vec2| (a - o).perp_dot(b - o);
    let mut lower: Vec<Vec2> = vec![];
    for &q in &p {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], q) <= 0.0 {
            lower.pop();
        }
        lower.push(q);
    }
    let mut upper: Vec<Vec2> = vec![];
    for &q in p.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], q) <= 0.0 {
            upper.pop();
        }
        upper.push(q);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// A piece's footprint when its collider is not loaded: a 16-gon of its scatter ground reach.
pub fn fallback_hull(piece: &str) -> Vec<Vec2> {
    let r = footprint(piece).ground;
    (0..16)
        .map(|i| {
            let a = i as f32 * std::f32::consts::TAU / 16.0;
            Vec2::new(r * a.cos(), r * a.sin())
        })
        .collect()
}

/// The tile root's rest transform (`play::spawn_tile`).
fn root_transform(t: &PlayTile) -> Transform {
    let [x, z] = t.cell.to_world(HEX_RADIUS);
    Transform::from_xyz(x, 0.0, z).with_rotation(piece_rotation(t.rotation))
}

fn place_hull(hull: &[Vec2], t: &Transform) -> Vec<Vec2> {
    hull.iter()
        .map(|p| t.transform_point(Vec3::new(p.x, 0.0, p.y)).xz())
        .collect()
}

impl WalkGround {
    /// Builds the ground of the drawn `tiles` (minus `hidden` ones). `surface_of` gives a tile
    /// definition's ground surface (tile frame) or `None` for a placeholder; `shape_of` a piece's
    /// collider footprint and pickable volume in its own frame; `scatter` the drawn scatter by cell.
    pub fn build(
        tiles: &[PlayTile],
        catalog: &TileCatalog,
        hidden: &[Axial],
        surface_of: &mut dyn FnMut(&TileDef) -> Option<Arc<Surface>>,
        shape_of: &mut dyn FnMut(&str) -> PieceShape,
        scatter: &BTreeMap<Axial, TileScatter>,
    ) -> Self {
        let mut out = Self::default();
        let mut shapes: HashMap<String, Arc<PieceShape>> = HashMap::new();
        let mut shape = |piece: &str| -> Arc<PieceShape> {
            shapes
                .entry(piece.to_owned())
                .or_insert_with(|| Arc::new(shape_of(piece)))
                .clone()
        };
        for t in tiles.iter().filter(|t| !hidden.contains(&t.cell)) {
            let Some(def) = catalog.get(&t.tile) else {
                continue;
            };
            let [x, z] = t.cell.to_world(HEX_RADIUS);
            out.cells.insert(
                t.cell,
                CellGround {
                    tile: t.tile.clone(),
                    centre: Vec2::new(x, z),
                    rotation: t.rotation,
                    surface: surface_of(def),
                    water: water_kind(def, t.rotation).is_some(),
                },
            );
            let root = root_transform(t);
            let mut obs = vec![];
            for h in &def.props.hero {
                let piece = h.piece.as_str();
                if OBSTACLES.contains(&piece) {
                    let local = Transform::from_xyz(h.at[0], 0.0, h.at[1])
                        .with_rotation(Quat::from_rotation_y(-h.yaw_deg.to_radians()));
                    let (sh, t) = (shape(piece), root * local);
                    obs.push(Obstacle {
                        piece: piece.to_owned(),
                        poly: place_hull(&sh.hull, &t),
                    });
                    out.props.extend(PickProp::new(piece, &sh, &t));
                }
            }
            if let Some(s) = scatter.get(&t.cell).filter(|s| s.tile == t.tile) {
                for inst in &s.layout.instances {
                    let rule = inst.rule as usize;
                    let piece = s.piece_of(inst);
                    if s.drawn[rule] && OBSTACLES.contains(&piece) {
                        let (sh, t) = (shape(piece), root * inst.transform());
                        obs.push(Obstacle {
                            piece: piece.to_owned(),
                            poly: place_hull(&sh.hull, &t),
                        });
                        out.props.extend(PickProp::new(piece, &sh, &t));
                    }
                }
            }
            if !obs.is_empty() {
                out.obstacles.insert(t.cell, obs);
            }
        }
        out
    }

    /// The tile under world XZ `p`: its cell and id.
    pub fn tile_at(&self, p: Vec2) -> Option<(Axial, &str)> {
        let cell = cell_at(p);
        self.cells.get(&cell).map(|g| (cell, g.tile.as_str()))
    }

    /// Ground height under `p` and the water depth over it, or why nothing stands there.
    fn sample(&self, p: Vec2) -> Result<(f32, f32), Blocked> {
        let Some(g) = self.cells.get(&cell_at(p)) else {
            return Err(Blocked::Edge);
        };
        let d = p - g.centre;
        let l = turn((6 - g.rotation % 6) % 6, [d.x, 0.0, d.y]);
        let h = match &g.surface {
            Some(s) => s.at(l[0], l[2]).ok_or(Blocked::Edge)?.0,
            None => 0.0,
        };
        let depth = if g.water { (WATER_Y - h).max(0.0) } else { 0.0 };
        Ok((h, depth))
    }

    /// Ground height under `p` (the bed under shallow water), or why nothing stands there:
    /// [`Blocked::Edge`] or [`Blocked::Water`].
    pub fn ground(&self, p: Vec2) -> Result<f32, Blocked> {
        let (h, depth) = self.sample(p)?;
        if depth > MAX_WADE {
            return Err(Blocked::Water);
        }
        Ok(h)
    }

    /// The visible surface height at `p`: the water's surface over a water tile's bed, else the
    /// ground; `None` off the drawn tiles. Photo mode's click-to-focus marches a view ray over it.
    pub fn top(&self, p: Vec2) -> Option<f32> {
        self.sample(p)
            .ok()
            .map(|(h, depth)| if depth > 0.0 { WATER_Y } else { h })
    }

    /// The nearest drawn prop `ray` enters within `max` metres: its distance along the ray and the
    /// piece id.
    pub fn pick_prop(&self, ray: Ray3d, max: f32) -> Option<(f32, &str)> {
        self.props
            .iter()
            .filter_map(|p| p.hit(ray).map(|t| (t, p.piece.as_str())))
            .filter(|(t, _)| *t <= max)
            .min_by(|a, b| a.0.total_cmp(&b.0))
    }

    /// No drawn tile is ground yet (another scenario, or the kit still loading).
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Water depth (m) over the ground at `p`; 0 on dry ground and off the map.
    pub fn water_depth(&self, p: Vec2) -> f32 {
        self.sample(p).map_or(0.0, |(_, d)| d)
    }

    /// The obstacle a body at `p` overlaps, if any.
    pub fn obstacle(&self, p: Vec2) -> Option<&Obstacle> {
        let cell = cell_at(p);
        std::iter::once(cell)
            .chain((0..6).map(|e| cell.neighbor(e)))
            .filter_map(|c| self.obstacles.get(&c))
            .flatten()
            .find(|o| o.hits(p, BODY_RADIUS))
    }

    /// Ground height for a body standing at `p`: the centre stands on ground, the body's rim
    /// overhangs no edge and no deep water, and it overlaps no obstacle.
    pub fn stand(&self, p: Vec2) -> Result<f32, Blocked> {
        let h = self.ground(p)?;
        for k in 0..8 {
            let a = k as f32 * std::f32::consts::FRAC_PI_4;
            self.ground(p + BODY_RADIUS * Vec2::new(a.cos(), a.sin()))?;
        }
        if let Some(o) = self.obstacle(p) {
            return Err(Blocked::Obstacle(o.piece.clone()));
        }
        Ok(h)
    }

    fn try_move(&self, from: Vec3, d: Vec2) -> Result<Vec3, Blocked> {
        let to = from.xz() + d;
        let h = self.stand(to)?;
        if h - from.y > MAX_STEP {
            return Err(Blocked::Steep);
        }
        Ok(Vec3::new(to.x, h, to.y))
    }

    /// Moves feet at `from` by the ground offset `delta` in sub-steps of at most 0.1 m, sliding
    /// along whatever blocks the move (each world axis alone) and stopping where neither slides.
    /// The returned `pos.y` is the ground under the feet; `from.y` is where they are now, so a
    /// step up is measured from there.
    pub fn step(&self, from: Vec3, delta: Vec2) -> Step {
        let n = (delta.length() / SUBSTEP).ceil().max(1.0) as usize;
        let d = delta / n as f32;
        let mut pos = from;
        let mut blocked = None;
        for _ in 0..n {
            match self.try_move(pos, d) {
                Ok(p) => pos = p,
                Err(b) => {
                    let slid = [Vec2::new(d.x, 0.0), Vec2::new(0.0, d.y)]
                        .into_iter()
                        .filter(|a| a.length() > 1e-6)
                        .find_map(|a| self.try_move(pos, a).ok());
                    blocked = Some(b);
                    match slid {
                        Some(p) => pos = p,
                        None => break,
                    }
                }
            }
        }
        if pos == from
            && let Ok(h) = self.ground(from.xz())
        {
            pos.y = h;
        }
        Step { pos, blocked }
    }

    /// The standable point nearest `focus`: rings 0.5 m apart out to 48 m, then each drawn tile's
    /// own centre outward, nearest tile first.
    pub fn nearest_stand(&self, focus: Vec2) -> Option<Vec3> {
        let ring = |c: Vec2, max: f32| {
            let mut r = 0.0f32;
            std::iter::from_fn(move || {
                if r > max {
                    return None;
                }
                let n = ((r * std::f32::consts::TAU / 0.5).ceil() as usize).max(1);
                let pts: Vec<Vec2> = (0..n)
                    .map(|i| {
                        let a = i as f32 * std::f32::consts::TAU / n as f32;
                        c + r * Vec2::new(a.cos(), a.sin())
                    })
                    .collect();
                r += 0.5;
                Some(pts)
            })
            .flatten()
        };
        let found = |p: Vec2| self.stand(p).ok().map(|h| Vec3::new(p.x, h, p.y));
        if let Some(v) = ring(focus, 48.0).find_map(found) {
            return Some(v);
        }
        let mut cells: Vec<&CellGround> = self.cells.values().collect();
        cells.sort_by(|a, b| {
            a.centre
                .distance(focus)
                .total_cmp(&b.centre.distance(focus))
                .then(a.centre.x.total_cmp(&b.centre.x))
                .then(a.centre.y.total_cmp(&b.centre.y))
        });
        cells
            .into_iter()
            .find_map(|g| ring(g.centre, HEX_RADIUS).find_map(found))
    }
}

/// A loaded piece's shape: its `<id>_collider` vertices (footprint and foot) and LOD0 triangles
/// (height and crown).
fn piece_shape(
    id: &str,
    packs: &PlayPacks,
    assets @ (gltfs, gltf_meshes, meshes): (&Assets<Gltf>, &Assets<GltfMesh>, &Assets<Mesh>),
) -> Option<PieceShape> {
    let g = gltfs.get(packs.glb(id)?)?;
    let gm = gltf_meshes.get(g.named_meshes.get(format!("{id}_collider").as_str())?)?;
    let mut pts = vec![];
    for prim in &gm.primitives {
        let pos = meshes
            .get(&prim.mesh)?
            .attribute(Mesh::ATTRIBUTE_POSITION)?
            .as_float3()?;
        pts.extend(pos.iter().map(|p| Vec3::from(*p)));
    }
    let lod0 = scatter::kit_tris(id, packs, assets).unwrap_or_default();
    let s = PieceShape::from_meshes(id, &pts, &lod0);
    (s.hull.len() >= 3).then_some(s)
}

/// Surfaces by tile id and collider footprints by piece, kept across rebuilds, and what the
/// ground was last built from.
#[derive(Default)]
pub struct WalkCache {
    surfaces: HashMap<String, Arc<Surface>>,
    shapes: HashMap<String, PieceShape>,
    built: Option<BuiltFrom>,
}

/// The inputs of the last rebuild: the drawn tiles, the hidden ones, the scatter revision and how
/// many kit pieces were built. Change detection cannot stand in for it: `PlayView` changes with
/// every hovered cell and `PlayKit` is written every frame.
#[derive(Clone, PartialEq)]
struct BuiltFrom {
    tiles: Vec<PlayTile>,
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
    let mut surface_of = |def: &TileDef| -> Option<Arc<Surface>> {
        if let Some(s) = surfaces.get(&def.id) {
            return Some(s.clone());
        }
        let pieces = scatter::ground_pieces(def, &catalog.0);
        if !pieces.iter().all(|(id, _)| kit.contains_key(*id)) {
            return None;
        }
        let mut all: Vec<(Tri, String)> = vec![];
        for (id, k) in pieces {
            for (t, s) in scatter::kit_tris(id, &packs, assets)? {
                all.push((t.map(|p| turn(k, p)), s));
            }
        }
        let s = Arc::new(Surface::new(all));
        surfaces.insert(def.id.clone(), s.clone());
        Some(s)
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
        &from.hidden,
        &mut surface_of,
        &mut shape_of,
        &layouts.tiles,
    );
    ground.revision = revision;
    cache.built = Some(from);
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use hex_assets::parse_manifest;

    use super::*;
    use crate::{
        kit_scene::MANIFEST,
        play::scatter::{layout, prepare},
    };

    type Tris = Option<Vec<(Tri, String)>>;

    /// The windmill's trunk collider blocks the walker as a building, the sails do not reach it.
    #[test]
    fn windmill_blocks_as_a_building() {
        assert_eq!(Blocked::Obstacle("windmill".into()).reason(), "building");
        let hull = hull(
            &mesh_tris("windmill", "collider")
                .unwrap()
                .iter()
                .flat_map(|(t, _)| t.iter().map(|p| Vec2::new(p[0], p[2])))
                .collect::<Vec<_>>(),
        );
        let reach = hull.iter().map(|p| p.length()).fold(0.0, f32::max);
        assert!((2.5..3.2).contains(&reach), "trunk reach {reach}");
    }

    /// A placed windmill tile stops the walker at its tower, from every side and through its turn.
    #[test]
    fn a_placed_windmill_tile_blocks_at_the_tower() {
        let mut tested = 0;
        for rotation in [0, 3] {
            let g = world(&[(0, 0, "windmill", rotation)]);
            for k in 0..6 {
                let a = (60.0 * k as f32 + 30.0).to_radians();
                let start = Vec2::new(a.cos(), a.sin()) * 8.0;
                if g.stand(start).is_err() {
                    continue;
                }
                let s = walk(&g, start, -start, 8.0);
                let at = s.pos.xz().length();
                if s.blocked == Some(Blocked::Obstacle("rock_small".into())) {
                    continue;
                }
                assert_eq!(
                    s.blocked,
                    Some(Blocked::Obstacle("windmill".into())),
                    "r{rotation} side {k}: stopped at {at} m"
                );
                assert!((2.4..3.6).contains(&at), "r{rotation} side {k}: {at} m out");
                tested += 1;
            }
        }
        assert!(tested >= 8, "only {tested} approaches reached the tower");
    }

    fn catalog() -> TileCatalog {
        TileCatalog::builtin().unwrap()
    }

    fn glb(id: &str) -> Option<(gltf::Document, Vec<gltf::buffer::Data>)> {
        let m = parse_manifest(MANIFEST).unwrap();
        let e = m.pieces.iter().find(|p| p.id == id)?;
        let path = format!("{}/../../assets/kit/{}", env!("CARGO_MANIFEST_DIR"), e.glb);
        let bytes = std::fs::read(path).ok()?;
        let plain = hex_assets::rewrite_glb(&bytes).ok()?;
        let (doc, buffers, _) = gltf::import_slice(&plain).ok()?;
        Some((doc, buffers))
    }

    /// Triangles of mesh `<id>_<mesh>` with slot names, read once per mesh for the test binary.
    fn mesh_tris(id: &str, mesh: &str) -> Tris {
        static CACHE: OnceLock<Mutex<HashMap<String, Tris>>> = OnceLock::new();
        let key = format!("{id}_{mesh}");
        let cache = CACHE.get_or_init(Default::default);
        if let Some(t) = cache.lock().unwrap().get(&key) {
            return t.clone();
        }
        let read = || -> Tris {
            let (doc, buffers) = glb(id)?;
            let m = doc.meshes().find(|m| m.name() == Some(key.as_str()))?;
            let mut out = vec![];
            for prim in m.primitives() {
                let slot = prim.material().name().unwrap_or_default().to_string();
                let r = prim.reader(|b| Some(&buffers[b.index()]));
                let pos: Vec<[f32; 3]> = r.read_positions()?.collect();
                let idx: Vec<u32> = r.read_indices()?.into_u32().collect();
                for t in idx.as_chunks::<3>().0 {
                    out.push((
                        [pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]],
                        slot.clone(),
                    ));
                }
            }
            Some(out)
        };
        let t = read();
        cache.lock().unwrap().insert(key, t.clone());
        t
    }

    fn surface(def: &TileDef) -> Option<Arc<Surface>> {
        let mut all = vec![];
        for (id, k) in scatter::ground_pieces(def, &catalog()) {
            for (t, s) in mesh_tris(id, "lod0")? {
                all.push((t.map(|p| turn(k, p)), s));
            }
        }
        Some(Arc::new(Surface::new(all)))
    }

    fn shape(piece: &str) -> PieceShape {
        let pts: Vec<Vec3> = mesh_tris(piece, "collider")
            .expect("collider mesh")
            .iter()
            .flat_map(|(t, _)| t.iter().map(|p| Vec3::from(*p)))
            .collect();
        PieceShape::from_meshes(piece, &pts, &mesh_tris(piece, "lod0").expect("lod0 mesh"))
    }

    /// A world of `tiles` with the real kit ground, colliders and seeded scatter.
    fn world(tiles: &[(i32, i32, &str, u8)]) -> WalkGround {
        let c = catalog();
        let play: Vec<PlayTile> = tiles
            .iter()
            .map(|&(q, r, tile, rotation)| PlayTile {
                cell: Axial::new(q, r),
                tile: tile.into(),
                rotation,
            })
            .collect();
        let mut scatter = BTreeMap::new();
        for t in &play {
            let def = c.get(&t.tile).unwrap();
            if def.props.scatter.is_empty() {
                continue;
            }
            let prepared = prepare(def, &c, &mut |id| mesh_tris(id, "lod0")).unwrap();
            let seed = hex_core::cell_seed(1, t.cell);
            scatter.insert(
                t.cell,
                TileScatter {
                    tile: t.tile.clone(),
                    seed,
                    layout: Arc::new(layout(def, seed, &prepared)),
                    drawn: vec![true; def.props.scatter.len()],
                    pieces: def
                        .props
                        .scatter
                        .iter()
                        .map(|r| r.piece.as_str().to_owned())
                        .collect(),
                },
            );
        }
        WalkGround::build(
            &play,
            &c,
            &[],
            &mut |d| surface(d),
            &mut |p| shape(p),
            &scatter,
        )
    }

    fn centre(q: i32, r: i32) -> Vec2 {
        Vec2::from(Axial::new(q, r).to_world(HEX_RADIUS))
    }

    /// Walks from `from` along `dir` for `dist` metres in 5 cm strides, as the controller does.
    fn walk(g: &WalkGround, from: Vec2, dir: Vec2, dist: f32) -> Step {
        let mut pos = Vec3::new(from.x, g.ground(from).unwrap(), from.y);
        let mut last = None;
        for _ in 0..(dist / 0.05).round() as usize {
            let s = g.step(pos, dir.normalize() * 0.05);
            pos = s.pos;
            if s.blocked.is_some() {
                last = s.blocked;
            }
        }
        Step { pos, blocked: last }
    }

    #[test]
    fn hull_is_convex_and_counter_clockwise() {
        let pts = [
            Vec2::new(0.0, 0.0),
            Vec2::new(2.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(0.0, 2.0),
        ];
        let h = hull(&pts);
        assert_eq!(h.len(), 4, "{h:?}");
        let o = Obstacle {
            piece: "rock_small".into(),
            poly: h,
        };
        assert!(o.hits(Vec2::new(1.0, 1.0), 0.0), "inside");
        assert!(
            o.hits(Vec2::new(2.2, 1.0), 0.3),
            "within the radius of an edge"
        );
        assert!(!o.hits(Vec2::new(2.4, 1.0), 0.3));
    }

    #[test]
    fn ground_follows_the_meadow_road_beach_and_river_banks() {
        let g = world(&[
            (0, 0, "meadow", 0),
            (1, 0, "road_straight", 0),
            (0, 1, "beach", 0),
            (-1, 0, "river_straight", 0),
        ]);
        // Meadow: the cap stands at the border ring's height, give or take its relief.
        let m = g.ground(centre(0, 0) + Vec2::new(3.0, 2.0)).unwrap();
        assert!(m.abs() < 0.4, "meadow cap at {m}");
        // Road: the crown on the lane centre line, near the cap.
        let road = g.ground(centre(1, 0)).unwrap();
        assert!(road.abs() < 0.4, "road crown at {road}");
        // Beach: the sand falls toward the sea, from grass level to the waterline and below.
        let beach = (-12..=12)
            .filter_map(|i| {
                let p = centre(0, 1) + Vec2::new(0.0, i as f32);
                g.ground(p).ok().map(|h| (i, h))
            })
            .collect::<Vec<_>>();
        let lo = beach.iter().map(|b| b.1).fold(f32::MAX, f32::min);
        let hi = beach.iter().map(|b| b.1).fold(f32::MIN, f32::max);
        assert!(hi - lo > 0.8, "a beach slope: {beach:?}");
        // River: the banks fall from the cap toward the channel, and the channel is deep water.
        let c = centre(-1, 0);
        let bank_top = g.ground(c + Vec2::new(0.0, 10.0)).unwrap();
        let bank_low = g.ground(c + Vec2::new(0.0, 5.0)).unwrap();
        assert!(bank_top - bank_low > 0.3, "{bank_top} then {bank_low}");
        assert_eq!(g.ground(c), Err(Blocked::Water), "the channel centre");
    }

    #[test]
    fn a_house_wall_stops_the_walker_on_a_turned_tile() {
        for rotation in [0u8, 2] {
            let g = world(&[(0, 0, "house_cluster", rotation)]);
            // The two-storey house stands at (-7.5, 0) in the tile frame: walk at it from the
            // tile centre, through the tile's turn.
            let house = Vec2::from({
                let p = turn(rotation, [-7.5, 0.0, 0.0]);
                [p[0], p[2]]
            });
            let start = house * (2.0 / 7.5);
            assert!(g.stand(start).is_ok(), "r{rotation}: start on the pad");
            let s = walk(&g, start, house - start, 8.0);
            assert_eq!(
                s.blocked,
                Some(Blocked::Obstacle("house_twostorey".into())),
                "r{rotation}"
            );
            let gone = s.pos.xz().distance(start);
            assert!(
                gone > 0.5 && gone < 5.0,
                "r{rotation}: stopped {gone} m out, short of the house centre"
            );
            assert!(
                g.obstacle(s.pos.xz()).is_none(),
                "r{rotation}: never inside"
            );
        }
    }

    #[test]
    fn a_tree_trunk_stops_the_walker() {
        let g = world(&[(0, 0, "forest", 0), (1, 0, "meadow", 0)]);
        let trees: Vec<&Obstacle> = g.obstacles[&Axial::ORIGIN]
            .iter()
            .filter(|o| o.piece.starts_with("tree_forest"))
            .collect();
        assert!(trees.len() > 10, "{} spruces", trees.len());
        // Walk at a trunk from 3 m away along a line no other obstacle crosses.
        let mut tested = 0;
        for t in trees {
            let c = t.poly.iter().copied().sum::<Vec2>() / t.poly.len() as f32;
            let reach = t.poly.iter().map(|p| p.distance(c)).fold(0.0, f32::max);
            assert!(reach < 1.0, "a trunk, not a crown: {reach} m");
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::FRAC_PI_4;
                let start = c + 3.0 * Vec2::new(a.cos(), a.sin());
                if g.stand(start).is_err() {
                    continue;
                }
                let s = walk(&g, start, c - start, 3.0);
                if s.blocked != Some(Blocked::Obstacle(t.piece.clone())) {
                    continue;
                }
                let d = s.pos.xz().distance(c);
                assert!(
                    d > BODY_RADIUS && d < reach + BODY_RADIUS + 0.15,
                    "stopped {d} m from the trunk centre (reach {reach})"
                );
                tested += 1;
                break;
            }
            if tested >= 3 {
                break;
            }
        }
        assert!(tested >= 3, "three trunks walked into");
    }

    #[test]
    fn water_and_the_map_edge_stop_the_walker() {
        let g = world(&[(0, 0, "meadow", 0), (0, 1, "beach", 0), (1, 1, "ocean", 0)]);
        // The map edge: walk east off the lone meadow row, on a line no scattered rock crosses.
        let s = [0.0, 2.0, -2.0, 4.0, -4.0]
            .into_iter()
            .map(|dz| walk(&g, centre(0, 0) + Vec2::new(0.0, dz), Vec2::X, 30.0))
            .find(|s| !matches!(s.blocked, Some(Blocked::Obstacle(_))))
            .expect("a line east that no rock crosses");
        assert_eq!(s.blocked, Some(Blocked::Edge));
        let apothem = HEX_RADIUS * 3f32.sqrt() / 2.0;
        let x = s.pos.x - centre(0, 0).x;
        assert!(
            x > apothem - BODY_RADIUS - 0.2 && x < apothem,
            "stopped at the edge: {x}"
        );
        // The beach: walk out to sea until the water is too deep, with the feet in shallow water.
        let beach = centre(0, 1);
        let (seaward, _) = (0..12)
            .map(|k| {
                let a = k as f32 * std::f32::consts::TAU / 12.0;
                let d = Vec2::new(a.cos(), a.sin());
                (d, g.water_depth(beach + 12.0 * d))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        let start = beach - 6.0 * seaward;
        let s = walk(&g, start, seaward, 40.0);
        assert_eq!(s.blocked, Some(Blocked::Water));
        let depth = g.water_depth(s.pos.xz());
        assert!(depth <= MAX_WADE, "wading at {depth} m");
        assert!(g.water_depth(s.pos.xz() + seaward) > depth, "deeper ahead");
        // The ocean is never ground.
        assert_eq!(g.ground(centre(1, 1)), Err(Blocked::Water));
    }

    #[test]
    fn a_river_bank_stops_the_walker_at_the_water() {
        let g = world(&[(0, 0, "river_straight", 1)]);
        let c = centre(0, 0);
        let across = (0..12)
            .map(|k| {
                let a = k as f32 * std::f32::consts::TAU / 12.0;
                Vec2::new(a.cos(), a.sin())
            })
            .find(|d| g.stand(c + 11.0 * *d).is_ok())
            .expect("a bank");
        let s = walk(&g, c + 11.0 * across, -across, 11.0);
        assert_eq!(s.blocked, Some(Blocked::Water));
        assert!(s.pos.xz().distance(c) > 2.0, "short of the channel");
        assert!(s.pos.y < 0.0, "down the bank: {}", s.pos.y);
    }

    fn ray(o: Vec3, d: Vec3) -> Ray3d {
        Ray3d::new(o, Dir3::new(d).unwrap())
    }

    #[test]
    fn a_ray_enters_an_extruded_footprint_from_the_front() {
        let square = [
            Vec2::new(-1.0, -1.0),
            Vec2::new(1.0, -1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(-1.0, 1.0),
        ];
        let toward = ray(Vec3::new(0.0, 2.0, 10.0), Vec3::NEG_Z);
        assert_eq!(
            ray_prism(toward, &square, 0.0, 5.0),
            Some(9.0),
            "the near wall"
        );
        let cw: Vec<Vec2> = square.iter().rev().copied().collect();
        assert_eq!(
            ray_prism(toward, &cw, 0.0, 5.0),
            Some(9.0),
            "either winding"
        );
        assert_eq!(ray_prism(toward, &square, 0.0, 1.5), None, "over its top");
        let down = ray(Vec3::new(0.5, 9.0, 0.0), Vec3::NEG_Y);
        assert_eq!(
            ray_prism(down, &square, 0.0, 5.0),
            Some(4.0),
            "onto the roof"
        );
        assert_eq!(
            ray_prism(
                ray(Vec3::new(3.0, 2.0, 10.0), Vec3::NEG_Z),
                &square,
                0.0,
                5.0
            ),
            None
        );
        assert_eq!(
            ray_prism(
                ray(Vec3::new(0.0, 2.0, 0.0), Vec3::NEG_Z),
                &square,
                0.0,
                5.0
            ),
            None,
            "from inside"
        );
        assert_eq!(
            ray_prism(ray(Vec3::new(0.0, 2.0, 10.0), Vec3::Z), &square, 0.0, 5.0),
            None,
            "behind"
        );
    }

    #[test]
    fn a_crown_is_hit_on_its_widest_band_and_missed_beside_it() {
        // A cone of foliage: 4 m wide at 2 m, a point at 12 m.
        let step = std::f32::consts::TAU / 16.0;
        let tri = |a: f32| -> Tri {
            let (s0, c0) = a.sin_cos();
            let (s1, c1) = (a + step).sin_cos();
            [
                [4.0 * c0, 2.0, 4.0 * s0],
                [4.0 * c1, 2.0, 4.0 * s1],
                [0.0, 12.0, 0.0],
            ]
        };
        let tris: Vec<Tri> = (0..16)
            .map(|i| tri(i as f32 * std::f32::consts::TAU / 16.0))
            .collect();
        let c = Crown::fit(&tris).unwrap();
        assert!((c.y0 - 2.0).abs() < 1e-4 && (c.band * CROWN_BANDS as f32 - 10.0).abs() < 1e-3);
        let widest = |k: usize| c.radii[k].iter().copied().fold(0.0, f32::max);
        assert!(
            widest(0) > widest(CROWN_BANDS - 1) + 2.0,
            "a cone narrows: {:?}",
            c.radii
        );
        let low = c.hit(ray(Vec3::new(0.0, 2.5, 30.0), Vec3::NEG_Z)).unwrap();
        assert!(low > 25.0 && low < 27.0, "the wide skirt at 2.5 m: {low}");
        let high = c.hit(ray(Vec3::new(0.0, 11.0, 30.0), Vec3::NEG_Z)).unwrap();
        assert!(high > low + 2.0, "the narrow top is farther: {high}");
        assert!(
            c.hit(ray(Vec3::new(6.0, 5.0, 30.0), Vec3::NEG_Z)).is_none(),
            "beside it"
        );
        assert!(
            c.hit(ray(Vec3::new(0.0, 13.0, 30.0), Vec3::NEG_Z))
                .is_none(),
            "over it"
        );
    }

    #[test]
    fn a_click_picks_the_nearest_drawn_prop_and_its_kind() {
        let g = world(&[(0, 0, "oak", 0), (1, 0, "house_cluster", 0)]);
        assert!(g.props.iter().any(|p| p.piece == "tree_hero_oak"));
        assert!(
            g.props.iter().all(|p| p.piece != "wildflower_clump"),
            "flowers are not pickable"
        );
        let oak = centre(0, 0);
        // Level at eye height from 40 m south, up at the crown's middle (above the horizon).
        let eye = Vec3::new(oak.x, 1.7, oak.y + 40.0);
        let mid = Vec3::new(oak.x, 8.7, oak.y);
        let (t, piece) = g.pick_prop(ray(eye, mid - eye), 1000.0).unwrap();
        assert_eq!((piece, prop_kind(piece)), ("tree_hero_oak", "tree"));
        assert!(t > 25.0 && t < 40.0, "the crown's near side: {t}");
        // Beside the tree the ray meets nothing here (the ground is the caller's).
        let beside = Vec3::new(oak.x + 14.0, 8.7, oak.y);
        assert!(g.pick_prop(ray(eye, beside - eye), 1000.0).is_none());
        // The house is a building, and stands in front of whatever is behind it.
        let house = g.obstacles[&Axial::new(1, 0)]
            .iter()
            .find(|o| o.piece.starts_with("house"))
            .unwrap();
        let foot = house.poly.iter().copied().sum::<Vec2>() / house.poly.len() as f32;
        let from = Vec3::new(foot.x, 3.0, foot.y + 30.0);
        let (t, piece) = g.pick_prop(ray(from, Vec3::NEG_Z), 1000.0).unwrap();
        assert_eq!(prop_kind(piece), "building");
        assert!(t < 30.0, "{t}");
    }

    /// A ray into the oak's crown or trunk lands on its LOD0 mesh; the volume, which never
    /// starts behind the mesh, only decides that the tree was hit.
    #[test]
    fn a_click_on_the_oak_lands_on_its_mesh() {
        let tris = mesh_tris("tree_hero_oak", "lod0").unwrap();
        let s = Arc::new(shape("tree_hero_oak"));
        let turned = Transform::from_xyz(5.0, 0.2, -3.0)
            .with_rotation(Quat::from_rotation_y(0.7))
            .with_scale(Vec3::splat(1.1));
        for (h, d) in [(8.7, 40.0), (6.0, 25.0), (10.5, 60.0), (2.0, 30.0)] {
            let r = ray(Vec3::new(0.0, 1.7, d), Vec3::new(0.0, h - 1.7, -d));
            let mesh = tris
                .iter()
                .filter_map(|(t, _)| ray_triangle(r, t))
                .reduce(f32::min)
                .unwrap();
            let prop = PickProp::new("tree_hero_oak", &s, &Transform::IDENTITY).unwrap();
            let volume = s
                .body
                .and_then(|(a, b)| ray_prism(r, &s.hull, a, b))
                .into_iter()
                .chain(s.crown.and_then(|c| c.hit(r)))
                .reduce(f32::min)
                .unwrap();
            assert!(
                volume <= mesh + 1e-3,
                "the volume wraps the mesh: {volume} vs {mesh}"
            );
            assert!(
                (prop.hit(r).unwrap() - mesh).abs() < 1e-3,
                "at {h} m from {d} m"
            );
            // Placed, turned and scaled: the same ray in the tree's frame, in world metres.
            let placed = PickProp::new("tree_hero_oak", &s, &turned).unwrap();
            let world = Ray3d::new(
                turned.transform_point(r.origin),
                Dir3::new(turned.rotation * *r.direction).unwrap(),
            );
            let got = placed.hit(world).unwrap();
            assert!(
                (got - mesh * 1.1).abs() < 1e-2,
                "placed: {got} vs {}",
                mesh * 1.1
            );
        }
    }

    /// `make shots-photo`'s tree click (photo-09): level at eye height 40 m south of the oak's
    /// trunk, a click 0.29 down a 45° frame meets the oak's LOD0 mesh at 36.1 m of view depth.
    #[test]
    fn the_tree_click_shot_meets_the_oak_mesh_at_36_m() {
        let tris = mesh_tris("tree_hero_oak", "lod0").unwrap();
        let k = (0.5 - 0.29) * 2.0 * 22.5f32.to_radians().tan();
        let r = ray(Vec3::new(0.0, 1.7, 40.0), Vec3::new(0.0, k, -1.0));
        let t = tris
            .iter()
            .filter_map(|(t, _)| ray_triangle(r, t))
            .reduce(f32::min)
            .unwrap();
        let depth = t / (1.0 + k * k).sqrt();
        assert!((depth - 36.1).abs() < 0.05, "{depth}");
    }

    #[test]
    fn a_gap_in_the_leaves_still_hits_the_crown() {
        let step = std::f32::consts::TAU / 16.0;
        let cone: Vec<Tri> = (0..16)
            .map(|i| {
                let (s0, c0) = (i as f32 * step).sin_cos();
                let (s1, c1) = ((i + 1) as f32 * step).sin_cos();
                [
                    [4.0 * c0, 2.0, 4.0 * s0],
                    [4.0 * c1, 2.0, 4.0 * s1],
                    [0.0, 12.0, 0.0],
                ]
            })
            .collect();
        // Only the cone's west (-x) half is mesh; its east half is a gap the crown still covers.
        let shape = Arc::new(PieceShape {
            crown: Crown::fit(&cone),
            tris: cone
                .iter()
                .copied()
                .filter(|t| t[0][0] + t[1][0] < 0.0)
                .collect(),
            ..default()
        });
        let prop = PickProp::new("tree_forest", &shape, &Transform::IDENTITY).unwrap();
        let east = ray(Vec3::new(2.0, 4.0, 30.0), Vec3::NEG_Z);
        let volume = shape.crown.unwrap().hit(east).unwrap();
        assert_eq!(
            prop.hit(east),
            Some(volume),
            "through the gap only: the crown"
        );
        assert!(volume > 26.0 && volume < 29.0, "{volume}");
        let west = ray(Vec3::new(-2.0, 4.0, 30.0), Vec3::NEG_Z);
        let leaves = prop.hit(west).unwrap();
        assert!(
            leaves > volume - 0.5 && leaves < 30.0,
            "the west leaves: {leaves}"
        );
    }

    #[test]
    fn the_visible_top_is_the_ground_or_the_water_surface() {
        let g = world(&[(0, 0, "meadow", 0), (-1, 0, "river_straight", 0)]);
        let m = centre(0, 0) + Vec2::new(3.0, 2.0);
        assert_eq!(g.top(m), g.ground(m).ok(), "dry ground is its own top");
        let c = centre(-1, 0);
        assert_eq!(g.ground(c), Err(Blocked::Water));
        assert_eq!(
            g.top(c),
            Some(WATER_Y),
            "the channel shows its water surface"
        );
        assert_eq!(g.top(Vec2::new(500.0, 0.0)), None, "off the map");
        assert!(!g.is_empty() && WalkGround::default().is_empty());
    }

    #[test]
    fn the_entry_point_is_the_nearest_standable_ground() {
        let g = world(&[(0, 0, "oak", 0)]);
        // The oak's trunk stands on the centre.
        assert!(g.stand(Vec2::ZERO).is_err());
        let p = g.nearest_stand(Vec2::ZERO).unwrap();
        assert!(p.xz().length() < 4.0, "beside the trunk: {p}");
        // A focus off the map comes back to the tile.
        let far = g.nearest_stand(Vec2::new(200.0, 0.0)).unwrap();
        assert!(g.tile_at(far.xz()).is_some());
    }
}
