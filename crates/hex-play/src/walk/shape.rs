use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use bevy::{
    gltf::{Gltf, GltfMesh},
    math::Affine3A,
    prelude::*,
};

use crate::play::{
    packs::PlayPacks,
    scatter::{self, Tri, footprint},
};

use super::tables::declared_parts;

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
pub(super) fn entry(t0: f32, t1: f32) -> Option<f32> {
    (t0 <= t1 && t0 >= 0.0).then_some(t0)
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
    /// A collider of several disjoint parts (the paddock fence's six run boxes) or of parts
    /// declared for the piece ([`DECLARED_PARTS`](super::tables::DECLARED_PARTS): `barn_red_b`'s closed walls and posts): each
    /// part's own footprint and body, which block and are clicked in place of `hull` and `body`.
    /// Empty for a one-part collider.
    pub parts: Vec<ColliderPart>,
}

/// One connected part of a collider: its convex footprint and its foot and top.
#[derive(Clone, Debug, PartialEq)]
pub struct ColliderPart {
    pub hull: Vec<Vec2>,
    pub body: Option<(f32, f32)>,
}

/// The collider's triangles (`collider`, three corners each) grouped into parts that share a
/// vertex position (to 1 mm). One group when the corners are not whole triangles.
fn collider_parts(collider: &[Vec3]) -> Vec<Vec<Vec3>> {
    let tris = collider.as_chunks::<3>().0;
    if tris.is_empty() || !collider.len().is_multiple_of(3) {
        return vec![collider.to_vec()];
    }
    fn root(up: &mut [usize], mut i: usize) -> usize {
        while up[i] != i {
            up[i] = up[up[i]];
            i = up[i];
        }
        i
    }
    let mut up: Vec<usize> = (0..tris.len()).collect();
    let mut first: HashMap<[i64; 3], usize> = HashMap::new();
    for (i, t) in tris.iter().enumerate() {
        for p in t {
            let key = p.to_array().map(|v| (v * 1000.0).round() as i64);
            let j = *first.entry(key).or_insert(i);
            let (a, b) = (root(&mut up, i), root(&mut up, j));
            up[a] = b;
        }
    }
    let mut groups: BTreeMap<usize, Vec<Vec3>> = BTreeMap::new();
    for (i, t) in tris.iter().enumerate() {
        let r = root(&mut up, i);
        groups.entry(r).or_default().extend(t);
    }
    groups.into_values().collect()
}

impl PieceShape {
    /// From the collider's triangle corners (three per triangle) and the LOD0 triangles with
    /// their slot names. On a piece with a crown footprint (`scatter::footprint`), every LOD0 slot
    /// but `<piece>_bark` is foliage. A collider of disjoint parts also gets each part's own
    /// footprint and body ([`PieceShape::parts`]).
    pub fn from_meshes(piece: &str, collider: &[Vec3], lod0: &[(Tri, String)]) -> Self {
        let hull = hull(&collider.iter().map(|p| p.xz()).collect::<Vec<_>>());
        let crowned = footprint(piece).crown > 0.0;
        let foliage = |slot: &str| crowned && !slot.ends_with("_bark");
        let foot_of = |pts: &[Vec3]| pts.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let foot = foot_of(collider);
        let top = lod0
            .iter()
            .filter(|(_, s)| !foliage(s))
            .flat_map(|(t, _)| t.iter().map(|p| p[1]))
            .chain(collider.iter().map(|p| p.y))
            .fold(f32::NEG_INFINITY, f32::max);
        let groups = collider_parts(collider);
        let parts = if let Some(declared) = declared_parts(piece) {
            declared
                .iter()
                .map(|&[x0, x1, z0, z1, t]| ColliderPart {
                    hull: self::hull(&[
                        Vec2::new(x0, z0),
                        Vec2::new(x1, z0),
                        Vec2::new(x1, z1),
                        Vec2::new(x0, z1),
                    ]),
                    body: (foot < t).then_some((foot, t)),
                })
                .collect()
        } else if groups.len() > 1 {
            groups
                .iter()
                .map(|g| {
                    let h = self::hull(&g.iter().map(|p| p.xz()).collect::<Vec<_>>());
                    let (f, t) = (
                        foot_of(g),
                        g.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max),
                    );
                    ColliderPart {
                        body: (h.len() >= 3 && f < t).then_some((f, t)),
                        hull: h,
                    }
                })
                .collect()
        } else {
            vec![]
        };
        Self {
            body: (hull.len() >= 3 && foot < top).then_some((foot, top)),
            crown: Crown::fit(lod0.iter().filter(|(_, s)| foliage(s)).map(|(t, _)| t)),
            hull,
            tris: lod0.iter().map(|(t, _)| *t).collect(),
            parts,
        }
    }

    /// A piece whose GLB is not loaded: the scatter ground reach as its footprint, nothing to click.
    /// A fence (`scatter::fence`) blocks nothing until its collider arrives: a disc at its origin
    /// would wall off the yard it encloses.
    pub fn fallback(piece: &str) -> Self {
        Self {
            hull: if scatter::fence(piece).is_some() {
                vec![]
            } else {
                fallback_hull(piece)
            },
            ..default()
        }
    }

    /// What blocks and is clicked: each part's footprint (`Some(part)`), or the whole collider's.
    pub(super) fn footprints(&self) -> Vec<(&[Vec2], Option<usize>)> {
        if self.parts.is_empty() {
            vec![(self.hull.as_slice(), None)]
        } else {
            self.parts
                .iter()
                .enumerate()
                .map(|(i, p)| (p.hull.as_slice(), Some(i)))
                .collect()
        }
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
    /// One part of a multi-part collider ([`PieceShape::parts`]): its prism alone is the volume.
    part: Option<usize>,
}

impl PickProp {
    pub(super) fn new(piece: &str, shape: &Arc<PieceShape>, t: &Transform) -> Option<Self> {
        (shape.body.is_some() || shape.crown.is_some()).then(|| Self {
            piece: piece.to_owned(),
            shape: shape.clone(),
            to_local: t.compute_affine().inverse(),
            scale: t.scale.x,
            part: None,
        })
    }

    /// Part `part` of a multi-part collider, clicked as its own prism.
    pub(super) fn new_part(
        piece: &str,
        shape: &Arc<PieceShape>,
        t: &Transform,
        part: usize,
    ) -> Option<Self> {
        shape.parts.get(part)?.body?;
        Some(Self {
            piece: piece.to_owned(),
            shape: shape.clone(),
            to_local: t.compute_affine().inverse(),
            scale: t.scale.x,
            part: Some(part),
        })
    }

    /// Where `ray` meets the prop (metres along the world ray): the first LOD0 triangle it
    /// crosses once it enters the body or crown, else where it enters them. A part of a
    /// multi-part collider is where the ray enters its prism: the LOD0 mesh spans every part, so
    /// its first triangle may lie on another.
    pub fn hit(&self, ray: Ray3d) -> Option<f32> {
        let d = self.to_local.transform_vector3(*ray.direction);
        let local = Ray3d::new(
            self.to_local.transform_point3(ray.origin),
            Dir3::new(d).ok()?,
        );
        let s = &self.shape;
        if let Some(i) = self.part {
            let p = &s.parts[i];
            let (a, b) = p.body?;
            return ray_prism(local, &p.hull, a, b).map(|t| t * self.scale);
        }
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

/// A loaded piece's shape: its `<id>_collider` triangle corners (footprint, foot and parts) and
/// LOD0 triangles (height and crown).
pub(super) fn piece_shape(
    id: &str,
    packs: &PlayPacks,
    assets @ (gltfs, gltf_meshes, meshes): (&Assets<Gltf>, &Assets<GltfMesh>, &Assets<Mesh>),
) -> Option<PieceShape> {
    let g = gltfs.get(packs.glb(id)?)?;
    let gm = gltf_meshes.get(g.named_meshes.get(format!("{id}_collider").as_str())?)?;
    let mut pts = vec![];
    for prim in &gm.primitives {
        let mesh = meshes.get(&prim.mesh)?;
        let pos = mesh.attribute(Mesh::ATTRIBUTE_POSITION)?.as_float3()?;
        match mesh.indices() {
            Some(idx) => pts.extend(idx.iter().map(|i| Vec3::from(pos[i]))),
            None => pts.extend(pos.iter().map(|p| Vec3::from(*p))),
        }
    }
    let lod0 = scatter::kit_tris(id, packs, assets).unwrap_or_default();
    let s = PieceShape::from_meshes(id, &pts, &lod0);
    (s.hull.len() >= 3).then_some(s)
}
