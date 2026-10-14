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
//! - **Obstacles.** Houses, the hero oak, spruces and rocks block the walker by their collider
//!   (`<piece>_collider` in the kit GLB) as a convex footprint on the ground, placed with the hero
//!   or scatter instance's transform under the tile's rest transform. Wildflowers do not block.
//! - **Edges.** A cell with no drawn tile is the map edge. The walker's body (a [`BODY_RADIUS`]
//!   circle) never overhangs an edge or deep water, and never climbs a step higher than
//!   [`MAX_STEP`] (a bridge railing, a house pad's lip if it were one).
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use bevy::{
    gltf::{Gltf, GltfMesh},
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
/// Pieces whose collider blocks the walker. Wildflowers and the ground pieces do not.
pub const OBSTACLES: [&str; 5] = [
    "house_cottage",
    "house_twostorey",
    "tree_hero_oak",
    "tree_forest",
    "rock_small",
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
            Blocked::Obstacle(p) if p.starts_with("house") => "building",
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
    /// definition's ground surface (tile frame) or `None` for a placeholder; `hull_of` a piece's
    /// collider footprint in its own frame; `scatter` the drawn scatter by cell.
    pub fn build(
        tiles: &[PlayTile],
        catalog: &TileCatalog,
        hidden: &[Axial],
        surface_of: &mut dyn FnMut(&TileDef) -> Option<Arc<Surface>>,
        hull_of: &mut dyn FnMut(&str) -> Vec<Vec2>,
        scatter: &BTreeMap<Axial, TileScatter>,
    ) -> Self {
        let mut out = Self::default();
        let mut hulls: HashMap<String, Vec<Vec2>> = HashMap::new();
        let mut hull = |piece: &str| -> Vec<Vec2> {
            hulls
                .entry(piece.to_owned())
                .or_insert_with(|| hull_of(piece))
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
                    obs.push(Obstacle {
                        piece: piece.to_owned(),
                        poly: place_hull(&hull(piece), &(root * local)),
                    });
                }
            }
            if let Some(s) = scatter.get(&t.cell).filter(|s| s.tile == t.tile) {
                for inst in &s.layout.instances {
                    let rule = inst.rule as usize;
                    let piece = s.pieces[rule].as_str();
                    if s.drawn[rule] && OBSTACLES.contains(&piece) {
                        obs.push(Obstacle {
                            piece: piece.to_owned(),
                            poly: place_hull(&hull(piece), &(root * inst.transform())),
                        });
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

/// A loaded piece's collider footprint (`<id>_collider` vertices, hulled on the ground plane).
fn collider_hull(
    id: &str,
    packs: &PlayPacks,
    (gltfs, gltf_meshes, meshes): (&Assets<Gltf>, &Assets<GltfMesh>, &Assets<Mesh>),
) -> Option<Vec<Vec2>> {
    let g = gltfs.get(packs.glb(id)?)?;
    let gm = gltf_meshes.get(g.named_meshes.get(format!("{id}_collider").as_str())?)?;
    let mut pts = vec![];
    for prim in &gm.primitives {
        let pos = meshes
            .get(&prim.mesh)?
            .attribute(Mesh::ATTRIBUTE_POSITION)?
            .as_float3()?;
        pts.extend(pos.iter().map(|p| Vec2::new(p[0], p[2])));
    }
    let h = hull(&pts);
    (h.len() >= 3).then_some(h)
}

/// Surfaces by tile id and collider footprints by piece, kept across rebuilds, and what the
/// ground was last built from.
#[derive(Default)]
pub struct WalkCache {
    surfaces: HashMap<String, Arc<Surface>>,
    hulls: HashMap<String, Vec<Vec2>>,
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
        cache.hulls.clear();
    }
    let assets = (&*gltfs, &*gltf_meshes, &*meshes);
    let WalkCache {
        surfaces, hulls, ..
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
    let mut hull_of = |piece: &str| -> Vec<Vec2> {
        if let Some(h) = hulls.get(piece) {
            return h.clone();
        }
        match kit
            .contains_key(piece)
            .then(|| collider_hull(piece, &packs, assets))
            .flatten()
        {
            Some(h) => {
                hulls.insert(piece.to_owned(), h.clone());
                h
            }
            None => fallback_hull(piece),
        }
    };
    let revision = ground.revision + 1;
    *ground = WalkGround::build(
        &from.tiles,
        &catalog.0,
        &from.hidden,
        &mut surface_of,
        &mut hull_of,
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

    fn collider(piece: &str) -> Vec<Vec2> {
        let tris = mesh_tris(piece, "collider").expect("collider mesh");
        let pts: Vec<Vec2> = tris
            .iter()
            .flat_map(|(t, _)| t.iter().map(|p| Vec2::new(p[0], p[2])))
            .collect();
        hull(&pts)
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
            &mut |p| collider(p),
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
            .filter(|o| o.piece == "tree_forest")
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
                if s.blocked != Some(Blocked::Obstacle("tree_forest".into())) {
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
        // The map edge: walk east off the lone meadow row.
        let s = walk(&g, centre(0, 0), Vec2::X, 30.0);
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
