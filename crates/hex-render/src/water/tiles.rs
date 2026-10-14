//! Water tiles in `play` (PRD §9.1): the approved water material on placed ocean, beach and river
//! tiles, the cut faces on their exposed edges, and placeholder beds until the river and coast kit
//! families exist. The numbers here are the bed contract in `docs/kit/water-contract.md`.
//!
//! Each water tile gets a [`TileField`]: tile-local bed profiles that depend only on the tile's
//! sockets, so they match across every legal seam, sampled at world positions. River centre lines
//! come from the tile's network segments and from its connected river neighbours, so a point on a
//! seam gets the same distance, flow and depth from both tiles.
use std::collections::{BTreeMap, BTreeSet, HashMap};

use bevy::{
    core_pipeline::prepass::DepthPrepass, light::NotShadowCaster, pbr::ScreenSpaceTransmission,
    post_process::dof::DepthOfField, prelude::*,
};
use hex_core::{
    Axial, Endpoint, HEX_RADIUS, LEVEL_STEP, NetworkKind, SocketKind, TileCatalog, TileDef,
    opposite_edge, world_edge,
};

use super::{
    LAND, TerrainMaterial, WaterField, WaterKey, WaterMaterial, WaterSample, WaterSurface, carve,
    cell_transform, materials, ribbon_mesh, river_flow, skirt_mesh_for, smoothstep, surface_mesh,
    terrain_color, terrain_mesh_for,
};
use crate::{SceneEntity, play::PlayTile, post::QualityTier};

/// Water surface (m): [`LAND`] below the meadow cap's border ring at y = 0, the prototype's
/// plateau-to-water drop, which the terrain shader's grass line and wet band are tuned to.
pub const WATER_Y: f32 = -LAND;
/// Tile base (m): one level step below the border ring, the bottom of every column.
pub const FLOOR_Y: f32 = -LEVEL_STEP;
/// Ocean bed (m): the tile base, so the water column fills the cut face down to the base. 2.6 m
/// of water: past the 2.5 m at which waves reach full height and the shoreline swash stops.
pub const OCEAN_BED_Y: f32 = FLOOR_Y;
/// `carve`'s half-width for play rivers: it puts the waterline at 4.0 m from the centre line, the
/// 8 m lane of `data/sockets.ron`, and the bank top (meadow level) at 6.83 m.
pub const RIVER_CUT_W: f32 = 4.33;
/// River surface strip half-width (m): the 4 m waterline plus 1 m tucked under the banks, where
/// the ground already stands 0.71 m above the water.
pub const RIBBON_HALF: f32 = 5.0;
/// Lateral steps of the river strip (0.5 m) and its row spacing along the centre line (m).
const RIBBON_COLS: u32 = 20;
const RIBBON_STEP: f32 = 0.5;
/// Coast profile along the seaward axis `s` (m from the tile centre): grass level up to
/// [`BEACH_TOP_S`], the beach slope down to the waterline at s = 0, a shallow shelf to
/// [`SHELF_S`], then the drop to the ocean bed, reached at [`DROP_END_S`].
pub const BEACH_TOP_S: f32 = -8.0;
pub const SHELF_S: f32 = 3.0;
pub const SHELF_SLOPE: f32 = 0.12;
pub const DROP_END_S: f32 = 8.0;
/// Shore distance reported where no coast is near (river tiles) and on the open ocean.
const INLAND: f32 = -40.0;
const OPEN_SEA: f32 = 2.0 * DROP_END_S;
/// Distance past the channel reported where no river is near.
const NO_RIVER: f32 = 1.0e4;
/// Largest angle one river arc sample spans (2°: under 4 mm off the circle at the 24 m bend).
const ARC_STEP: f32 = std::f32::consts::PI / 90.0;

const APOTHEM: f32 = HEX_RADIUS * 0.866_025_4;

/// Bed height (m) at `s` m along the seaward axis of a coast tile.
pub fn coast_height(s: f32) -> f32 {
    let rel = if s <= BEACH_TOP_S {
        LAND
    } else if s < 0.0 {
        let t = s / BEACH_TOP_S;
        LAND * t * (2.0 - t)
    } else {
        let floor = OCEAN_BED_Y - WATER_Y;
        -SHELF_SLOPE * s.min(SHELF_S)
            + (floor + SHELF_SLOPE * SHELF_S) * smoothstep(SHELF_S, DROP_END_S, s)
    };
    WATER_Y + rel
}

/// Bed height (m) at `dist` m from a river's centre line.
pub fn river_height(dist: f32) -> f32 {
    WATER_Y + carve(LAND, dist, RIVER_CUT_W)
}

fn edge_dir(e: u8) -> Vec2 {
    let a = (60.0 * (e % 6) as f32).to_radians();
    Vec2::new(a.cos(), a.sin())
}

fn center(cell: Axial) -> Vec2 {
    Vec2::from(cell.to_world(HEX_RADIUS))
}

/// Tile-local XZ to world XZ (local edge e lands on world edge e + rotation).
fn to_world(cell: Axial, rotation: u8, p: Vec2) -> Vec2 {
    let r = crate::kit_scene::piece_rotation(rotation) * Vec3::new(p.x, 0.0, p.y);
    center(cell) + Vec2::new(r.x, r.z)
}

#[derive(Clone, Debug, PartialEq)]
pub enum WaterKind {
    /// Carries a river network: a channel and a surface strip along each river segment.
    River,
    /// Some ocean edges: a beach. `sea` is the unit seaward direction in world XZ.
    Coast { sea: Vec2 },
    /// Ocean on every edge.
    Ocean,
}

/// What water a tile carries at `rotation`, if any.
pub fn water_kind(def: &TileDef, rotation: u8) -> Option<WaterKind> {
    if def.networks.iter().any(|n| n.kind == NetworkKind::River) {
        return Some(WaterKind::River);
    }
    let ocean = def
        .edges
        .iter()
        .filter(|e| e.kind == SocketKind::Ocean)
        .count();
    match ocean {
        0 => None,
        6 => Some(WaterKind::Ocean),
        _ => {
            let mut sea = Vec2::ZERO;
            for (e, s) in def.edges.iter().enumerate() {
                match s.kind {
                    SocketKind::Ocean => sea += edge_dir(e as u8),
                    SocketKind::Grass | SocketKind::Forest => sea -= edge_dir(e as u8),
                    _ => {}
                }
            }
            let r = crate::kit_scene::piece_rotation(rotation) * Vec3::new(sea.x, 0.0, sea.y);
            Some(WaterKind::Coast {
                sea: Vec2::new(r.x, r.z).normalize_or(Vec2::X),
            })
        }
    }
}

/// The water a tile's world edge carries, which decides whether water continues across it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum EdgeWater {
    Dry,
    /// A river lane: the channel cross-section is symmetric about the edge midpoint.
    River,
    /// The ocean bed along the whole edge.
    Ocean,
    /// A beach edge: grass at one end, the ocean bed at the other. Holds the seaward corner.
    Coast(Vec2),
}

impl EdgeWater {
    fn wet(self) -> bool {
        self != EdgeWater::Dry
    }

    /// Whether the beds (and so the water inputs) on both sides of a shared edge agree.
    fn continues(self, other: EdgeWater) -> bool {
        match (self, other) {
            (EdgeWater::River, EdgeWater::River) | (EdgeWater::Ocean, EdgeWater::Ocean) => true,
            (EdgeWater::Coast(a), EdgeWater::Coast(b)) => a.distance(b) < 0.01,
            _ => false,
        }
    }
}

fn edge_water(def: &TileDef, t: &PlayTile, world: u8) -> EdgeWater {
    let socket = def.edge(world, t.rotation);
    match socket.kind {
        SocketKind::River if socket.lane.is_some() => EdgeWater::River,
        SocketKind::Ocean => EdgeWater::Ocean,
        SocketKind::Beach => {
            let Some(WaterKind::Coast { sea }) = water_kind(def, t.rotation) else {
                return EdgeWater::Dry;
            };
            let c = center(t.cell);
            let corner = |deg: f32| {
                let a = deg.to_radians();
                c + Vec2::new(a.cos(), a.sin()) * HEX_RADIUS
            };
            let mid = 60.0 * world as f32;
            let (a, b) = (corner(mid - 30.0), corner(mid + 30.0));
            EdgeWater::Coast(if (a - c).dot(sea) > (b - c).dot(sea) {
                a
            } else {
                b
            })
        }
        _ => EdgeWater::Dry,
    }
}

/// A river centre line in world XZ, sampled densely, upstream first.
#[derive(Clone, Debug, PartialEq)]
pub struct RiverPath {
    pub points: Vec<Vec2>,
    /// World edges the path enters and leaves the tile by.
    pub up: u8,
    pub down: u8,
}

/// The circle through `a`, `m` and `b`, sampled from `a` to `b` at most [`ARC_STEP`] apart (one
/// control point is the arc midpoint, as in `hex_sim::geom::segment_shape`); straight if collinear.
fn arc_through(a: Vec2, m: Vec2, b: Vec2) -> Vec<Vec2> {
    let turn = (m - a).perp_dot(b - m);
    let d = 2.0 * (m - a).perp_dot(b - a);
    if turn.abs() <= 1e-6 * a.distance_squared(b) || d == 0.0 {
        return vec![a, m, b];
    }
    let (u, v) = (m - a, b - a);
    let (uu, vv) = (u.dot(u), v.dot(v));
    let c = a + Vec2::new(v.y * uu - u.y * vv, u.x * vv - v.x * uu) / d;
    let radius = a.distance(c);
    let angle = |p: Vec2| (p.y - c.y).atan2(p.x - c.x);
    let start = angle(a);
    let tau = std::f32::consts::TAU;
    let mut sweep = (angle(b) - start).rem_euclid(tau);
    if turn < 0.0 {
        sweep -= tau;
    }
    let steps = ((sweep.abs() / ARC_STEP).ceil() as usize).max(2);
    let mut out = vec![a];
    for i in 1..steps {
        let t = start + sweep * i as f32 / steps as f32;
        out.push(c + Vec2::new(t.cos(), t.sin()) * radius);
    }
    out.push(b);
    out
}

/// River segments of a placed tile as (from world edge, to world edge, centre line from -> to).
fn river_segments(def: &TileDef, t: &PlayTile) -> Vec<(u8, u8, Vec<Vec2>)> {
    let lane = |local: u8| {
        let s = &def.edges[local as usize];
        let n = edge_dir(local);
        n * APOTHEM + Vec2::new(-n.y, n.x) * s.lane.map_or(0.0, |l| l.offset)
    };
    def.networks
        .iter()
        .filter(|n| n.kind == NetworkKind::River)
        .filter_map(|n| match (&n.from, &n.to) {
            (Endpoint::Edge(a), Endpoint::Edge(b)) => {
                let pts: Vec<Vec2> = n.points.iter().map(|p| Vec2::from(*p)).collect();
                let local = match pts.as_slice() {
                    [] => vec![lane(*a), lane(*b)],
                    [m] => arc_through(lane(*a), *m, lane(*b)),
                    _ => [vec![lane(*a)], pts.clone(), vec![lane(*b)]].concat(),
                };
                let world = local
                    .into_iter()
                    .map(|p| to_world(t.cell, t.rotation, p))
                    .collect();
                Some((
                    world_edge(*a, t.rotation),
                    world_edge(*b, t.rotation),
                    world,
                ))
            }
            _ => None,
        })
        .collect()
}

/// Every placed river segment, oriented so the flow runs one way along each connected chain: each
/// chain starts at its open end with the lowest (cell, edge), or for a loop at its lowest cell.
/// Keyed by (cell, segment index among the tile's river segments).
pub fn oriented_rivers(
    tiles: &[PlayTile],
    catalog: &TileCatalog,
) -> BTreeMap<(Axial, usize), RiverPath> {
    let mut segs: BTreeMap<(Axial, usize), (u8, u8, Vec<Vec2>)> = BTreeMap::new();
    for t in tiles {
        if let Some(def) = catalog.get(&t.tile) {
            for (i, s) in river_segments(def, t).into_iter().enumerate() {
                segs.insert((t.cell, i), s);
            }
        }
    }
    // The segment across world edge `e` of `cell`, if it ends there.
    let across = |cell: Axial, e: u8| -> Option<(Axial, usize)> {
        let n = cell.neighbor(e);
        let back = opposite_edge(e);
        segs.range((n, 0)..=(n, usize::MAX))
            .find(|(_, (a, b, _))| *a == back || *b == back)
            .map(|(k, _)| *k)
    };
    let mut out = BTreeMap::new();
    let mut seen: BTreeSet<(Axial, usize)> = BTreeSet::new();
    for &key in segs.keys() {
        if seen.contains(&key) {
            continue;
        }
        // The connected chain, then its upstream end.
        let mut chain = vec![key];
        let mut i = 0;
        while i < chain.len() {
            let (a, b, _) = segs[&chain[i]];
            for e in [a, b] {
                if let Some(k) = across(chain[i].0, e)
                    && !chain.contains(&k)
                {
                    chain.push(k);
                }
            }
            i += 1;
        }
        let ends: Vec<(Axial, u8, usize)> = chain
            .iter()
            .flat_map(|k| {
                let (a, b, _) = segs[k];
                [a, b]
                    .into_iter()
                    .filter(|&e| across(k.0, e).is_none())
                    .map(move |e| (k.0, e, k.1))
            })
            .collect();
        let (mut cur, mut up) = match ends.iter().min() {
            Some(&(cell, e, i)) => ((cell, i), e),
            None => {
                let k = *chain.iter().min().expect("non-empty chain");
                let (a, b, _) = segs[&k];
                (k, a.min(b))
            }
        };
        loop {
            seen.insert(cur);
            let (a, b, pts) = &segs[&cur];
            let (down, points) = if *a == up {
                (*b, pts.clone())
            } else {
                (*a, pts.iter().rev().copied().collect())
            };
            out.insert(cur, RiverPath { points, up, down });
            match across(cur.0, down) {
                Some(next) if !seen.contains(&next) => {
                    up = opposite_edge(down);
                    cur = next;
                }
                _ => break,
            }
        }
    }
    out
}

/// One water tile as the meshes see it. Rebuilt when any field changes.
#[derive(Clone, Debug, PartialEq)]
pub struct CellWater {
    pub tile: PlayTile,
    pub kind: WaterKind,
    /// World edges whose cut face is drawn: water there and none across it.
    pub exposed: Vec<u8>,
    /// The tile's own river paths, then its connected neighbours' (river tiles only).
    pub paths: Vec<RiverPath>,
    pub own_paths: usize,
    /// Draw the placeholder bed (the tile's centre piece is not in the kit yet).
    pub bed: bool,
}

impl CellWater {
    pub fn field(&self) -> TileField<'_> {
        TileField { cell: self }
    }
}

/// The water layout of the placed world: one entry per water tile.
pub fn layout(
    tiles: &[PlayTile],
    catalog: &TileCatalog,
    kit_has: impl Fn(&str) -> bool,
) -> BTreeMap<Axial, CellWater> {
    let placed: HashMap<Axial, &PlayTile> = tiles.iter().map(|t| (t.cell, t)).collect();
    let rivers = oriented_rivers(tiles, catalog);
    let mut out = BTreeMap::new();
    for t in tiles {
        let Some(def) = catalog.get(&t.tile) else {
            continue;
        };
        let Some(kind) = water_kind(def, t.rotation) else {
            continue;
        };
        let exposed = (0..6u8)
            .filter(|&e| {
                let mine = edge_water(def, t, e);
                if !mine.wet() {
                    return false;
                }
                let theirs = placed.get(&t.cell.neighbor(e)).and_then(|n| {
                    catalog
                        .get(&n.tile)
                        .map(|d| edge_water(d, n, opposite_edge(e)))
                });
                !theirs.is_some_and(|w| mine.continues(w))
            })
            .collect();
        let own: Vec<RiverPath> = rivers
            .range((t.cell, 0)..=(t.cell, usize::MAX))
            .map(|(_, p)| p.clone())
            .collect();
        let own_paths = own.len();
        let mut paths = own.clone();
        for p in &own {
            for e in [p.up, p.down] {
                let n = t.cell.neighbor(e);
                paths.extend(
                    rivers
                        .range((n, 0)..=(n, usize::MAX))
                        .filter(|(_, q)| q.up == opposite_edge(e) || q.down == opposite_edge(e))
                        .map(|(_, q)| q.clone()),
                );
            }
        }
        let bed = !def.center.as_ref().is_some_and(|c| kit_has(c.as_str()));
        out.insert(
            t.cell,
            CellWater {
                tile: t.clone(),
                kind,
                exposed,
                paths,
                own_paths,
                bed,
            },
        );
    }
    out
}

/// A water tile's [`WaterField`].
pub struct TileField<'a> {
    cell: &'a CellWater,
}

impl TileField<'_> {
    /// Distance to the nearest river centre line and the downstream tangent there.
    fn river(&self, p: Vec2) -> (f32, Vec2) {
        let mut best = (f32::MAX, Vec2::X);
        for path in &self.cell.paths {
            for w in path.points.windows(2) {
                let (a, b) = (w[0], w[1]);
                let seg = b - a;
                let len2 = seg.length_squared();
                if len2 <= 0.0 {
                    continue;
                }
                let t = ((p - a).dot(seg) / len2).clamp(0.0, 1.0);
                let d = p.distance(a + seg * t);
                if d < best.0 {
                    best = (d, seg / len2.sqrt());
                }
            }
        }
        best
    }

    fn seaward(&self, p: Vec2, sea: Vec2) -> f32 {
        (p - center(self.cell.tile.cell)).dot(sea)
    }
}

impl WaterField for TileField<'_> {
    fn level(&self) -> f32 {
        WATER_Y
    }

    fn height(&self, x: f32, z: f32) -> f32 {
        let p = Vec2::new(x, z);
        match self.cell.kind {
            WaterKind::River => river_height(self.river(p).0),
            WaterKind::Coast { sea } => coast_height(self.seaward(p, sea)),
            WaterKind::Ocean => OCEAN_BED_Y,
        }
    }

    fn water(&self, x: f32, z: f32) -> WaterSample {
        let p = Vec2::new(x, z);
        let depth = (WATER_Y - self.height(x, z)).max(0.0);
        match self.cell.kind {
            WaterKind::River => {
                let (d, tangent) = self.river(p);
                let (river, speed) = river_flow(d, RIVER_CUT_W);
                WaterSample {
                    flow: tangent * speed * river,
                    river,
                    depth,
                    shore: INLAND,
                }
            }
            WaterKind::Coast { sea } => WaterSample {
                flow: Vec2::ZERO,
                river: 0.0,
                depth,
                shore: self.seaward(p, sea),
            },
            WaterKind::Ocean => WaterSample {
                flow: Vec2::ZERO,
                river: 0.0,
                depth,
                shore: OPEN_SEA,
            },
        }
    }

    fn terrain(&self, x: f32, z: f32) -> [f32; 4] {
        let p = Vec2::new(x, z);
        match self.cell.kind {
            WaterKind::River => terrain_color(self.river(p).0 - RIVER_CUT_W, INLAND),
            WaterKind::Coast { sea } => terrain_color(NO_RIVER, self.seaward(p, sea)),
            WaterKind::Ocean => terrain_color(NO_RIVER, OPEN_SEA),
        }
    }
}

/// Rows of a river strip along `path`: every [`RIBBON_STEP`] m, each with the lateral axis
/// across the flow. The end rows lie exactly along their tile edges.
pub fn ribbon_rows(path: &RiverPath) -> Vec<(Vec2, Vec2)> {
    let pts = &path.points;
    let mut cum = vec![0.0];
    for w in pts.windows(2) {
        cum.push(cum.last().copied().unwrap_or(0.0) + w[0].distance(w[1]));
    }
    let total = cum.last().copied().unwrap_or(0.0);
    let n = ((total / RIBBON_STEP).ceil() as usize).max(1);
    let at = |s: f32| {
        let i = cum.partition_point(|&c| c <= s).clamp(1, pts.len() - 1);
        let (a, b) = (pts[i - 1], pts[i]);
        let span = (cum[i] - cum[i - 1]).max(1e-6);
        a.lerp(b, ((s - cum[i - 1]) / span).clamp(0.0, 1.0))
    };
    let centres: Vec<Vec2> = (0..=n)
        .map(|i| match i {
            0 => pts[0],
            _ if i == n => pts[pts.len() - 1],
            _ => at(total * i as f32 / n as f32),
        })
        .collect();
    // Lateral axes follow the travel direction: inward at the upstream edge, outward downstream.
    centres
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let lat = if i == 0 {
                (-edge_dir(path.up)).perp()
            } else if i == n {
                edge_dir(path.down).perp()
            } else {
                (centres[i + 1] - centres[i - 1]).normalize().perp()
            };
            (c, lat)
        })
        .collect()
}

/// Water entities of the play world, keyed by cell, with what they were built from.
#[derive(Resource, Default)]
pub struct PlayWater {
    cells: HashMap<Axial, (CellWater, Vec<Entity>)>,
    materials: Option<(Handle<WaterMaterial>, Handle<TerrainMaterial>)>,
}

/// Marks the play camera once it carries the water components.
#[derive(Component)]
pub struct PlayWaterCamera;

/// Gives the play camera what the water shader needs, like `water::insert_camera` does for the
/// look-dev: the depth prepass (the bed under the water) and, on refracting tiers, screen-space
/// transmission. Drops the gate's far-focus DOF, which blurs every close-up (as the look-dev does).
pub fn prepare_camera(cam: &mut EntityCommands, tier: QualityTier) {
    cam.insert((DepthPrepass, PlayWaterCamera));
    if WaterKey::for_tier(tier).refraction {
        cam.insert(ScreenSpaceTransmission::default());
    }
    cam.remove::<DepthOfField>();
}

/// Assets the water sync builds with.
pub struct WaterAssets<'a> {
    pub meshes: &'a mut Assets<Mesh>,
    pub images: &'a mut Assets<Image>,
    pub water_mats: &'a mut Assets<WaterMaterial>,
    pub terrain_mats: &'a mut Assets<TerrainMaterial>,
}

/// Spawns and despawns water entities to match the placed tiles. A cell is rebuilt only when its
/// [`CellWater`] changes (its tile, its exposed edges, its river paths or its bed), so placing,
/// bulldozing, undoing and redoing refresh the neighbours' cut faces too. `rebuild` drops all.
pub fn sync(
    commands: &mut Commands,
    water: &mut PlayWater,
    assets: WaterAssets,
    tier: QualityTier,
    tiles: &[PlayTile],
    catalog: &TileCatalog,
    kit_has: impl Fn(&str) -> bool,
    rebuild: bool,
) {
    if rebuild {
        for (_, (_, es)) in water.cells.drain() {
            for e in es {
                commands.entity(e).try_despawn();
            }
        }
    }
    let want = layout(tiles, catalog, kit_has);
    water.cells.retain(|cell, (cw, es)| {
        let keep = want.get(cell) == Some(cw);
        if !keep {
            for e in es.drain(..) {
                commands.entity(e).try_despawn();
            }
        }
        keep
    });
    if want.keys().all(|c| water.cells.contains_key(c)) {
        return;
    }
    let (water_mat, terrain_mat) = water
        .materials
        .get_or_insert_with(|| {
            materials(
                assets.images,
                assets.water_mats,
                assets.terrain_mats,
                tier,
                WATER_Y,
            )
        })
        .clone();
    for (cell, cw) in want {
        if water.cells.contains_key(&cell) {
            continue;
        }
        let es = spawn_cell(commands, assets.meshes, &water_mat, &terrain_mat, &cw);
        info!(
            "play water: ({}, {}) {} r{} exposed {:?} bed {} flow {:?}",
            cell.q,
            cell.r,
            cw.tile.tile,
            cw.tile.rotation,
            cw.exposed,
            cw.bed,
            cw.paths[..cw.own_paths]
                .iter()
                .map(|p| (p.up, p.down))
                .collect::<Vec<_>>()
        );
        water.cells.insert(cell, (cw, es));
    }
}

fn spawn_cell(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    water: &Handle<WaterMaterial>,
    terrain: &Handle<TerrainMaterial>,
    cw: &CellWater,
) -> Vec<Entity> {
    let field = cw.field();
    let (cell, rot) = (cw.tile.cell, cw.tile.rotation);
    let c = center(cell);
    let at_cell = Transform::from_xyz(c.x, 0.0, c.y);
    let mut out = vec![];
    if cw.bed {
        out.push(
            commands
                .spawn((
                    Mesh3d(meshes.add(terrain_mesh_for(&field, cell, rot, FLOOR_Y))),
                    MeshMaterial3d(terrain.clone()),
                    cell_transform(cell, rot),
                    SceneEntity,
                ))
                .id(),
        );
    }
    let mut surfaces = vec![];
    match cw.kind {
        WaterKind::River => {
            for p in &cw.paths[..cw.own_paths] {
                let rows = ribbon_rows(p);
                surfaces.push((
                    ribbon_mesh(&field, c, &rows, RIBBON_HALF, RIBBON_COLS),
                    at_cell,
                ));
            }
        }
        WaterKind::Coast { .. } | WaterKind::Ocean => {
            surfaces.push((surface_mesh(&field, cell, rot), cell_transform(cell, rot)));
        }
    }
    if let Some(skirt) = skirt_mesh_for(&field, cell, &cw.exposed) {
        surfaces.push((skirt, at_cell));
    }
    for (mesh, t) in surfaces {
        out.push(
            commands
                .spawn((
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(water.clone()),
                    t,
                    NotShadowCaster,
                    WaterSurface,
                    SceneEntity,
                ))
                .id(),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_core::{World, opposite_edge};

    fn tile(q: i32, r: i32, id: &str, rotation: u8) -> PlayTile {
        PlayTile {
            cell: Axial::new(q, r),
            tile: id.into(),
            rotation,
        }
    }

    fn catalog() -> TileCatalog {
        TileCatalog::builtin().unwrap()
    }

    #[test]
    fn levels_sit_between_the_meadow_ring_and_the_base() {
        // Below the meadow relief, and deep enough over the ocean bed for full waves.
        const { assert!(WATER_Y < -0.25 && OCEAN_BED_Y - WATER_Y <= -2.5) };
        assert_eq!(coast_height(BEACH_TOP_S), 0.0);
        assert_eq!(coast_height(-16.0), 0.0);
        assert!((coast_height(0.0) - WATER_Y).abs() < 1e-6);
        assert!((coast_height(DROP_END_S) - OCEAN_BED_Y).abs() < 1e-5);
        assert!(river_height(20.0) == 0.0, "meadow level past the bank");
        let bed = river_height(0.0);
        assert!(
            (bed - (WATER_Y - 1.55)).abs() < 1e-5 && bed > FLOOR_Y,
            "{bed}"
        );
    }

    /// The 8 m lane is the water width: the bank crosses the water level 4 m from the centre line.
    #[test]
    fn river_waterline_is_the_lane_half_width() {
        let lane = catalog().sockets().lane(SocketKind::River).unwrap().width;
        assert!((river_height(lane / 2.0) - WATER_Y).abs() < 0.01);
        assert!(river_height(lane / 2.0 - 0.1) < WATER_Y);
        assert!(
            river_height(RIBBON_HALF) > WATER_Y + 0.5,
            "strip edge under the bank"
        );
    }

    /// The bend's control point is the midpoint of a 24 m arc whose centre lies on both lane edges'
    /// lines, so the centre line leaves each lane edge at a right angle.
    #[test]
    fn river_paths_cross_lane_edges_at_right_angles() {
        let cat = catalog();
        for (id, rot) in [
            ("river_bend", 0),
            ("river_bend", 3),
            ("river_straight", 1),
            ("river_road_bridge", 4),
        ] {
            let t = tile(0, 0, id, rot);
            for (a, b, pts) in river_segments(cat.get(id).unwrap(), &t) {
                let n = pts.len();
                let start = (pts[1] - pts[0]).normalize();
                let end = (pts[n - 1] - pts[n - 2]).normalize();
                assert!(
                    start.dot(-edge_dir(a)) > 0.9995,
                    "{id} r{rot} start {start}"
                );
                assert!(end.dot(edge_dir(b)) > 0.9995, "{id} r{rot} end {end}");
                assert!(pts[0].distance(edge_dir(a) * APOTHEM) < 1e-3);
            }
        }
    }

    /// A chain of three river tiles flows one way: each tile's downstream edge faces the next
    /// tile's upstream edge.
    #[test]
    fn river_chain_flows_one_way() {
        let tiles = [
            tile(0, 0, "river_straight", 0),
            tile(1, 0, "river_bend", 3),
            tile(-1, 0, "river_road_bridge", 3),
        ];
        let r = oriented_rivers(&tiles, &catalog());
        assert_eq!(r.len(), 3);
        let mut links = 0;
        for (k, p) in &r {
            if let Some((_, q)) = r.iter().find(|(j, _)| j.0 == k.0.neighbor(p.down)) {
                assert_eq!(q.up, opposite_edge(p.down), "{k:?}");
                links += 1;
            }
        }
        assert_eq!(links, 2);
    }

    /// Water inputs agree at every point two tiles share, so the displaced surfaces and the beds
    /// meet without a step: river to river (straight, bend and bridge under different rotations),
    /// beach to ocean, beach to beach.
    #[test]
    fn neighbouring_fields_agree_on_shared_edges() {
        let cat = catalog();
        let tiles = [
            tile(0, 0, "river_straight", 0),
            tile(1, 0, "river_bend", 3),
            tile(-1, 0, "river_road_bridge", 3),
            tile(5, 0, "beach", 0),
            tile(6, 0, "ocean", 2),
            tile(5, 1, "ocean", 5),
            tile(4, 1, "beach", 0),
        ];
        let lay = layout(&tiles, &cat, |_| false);
        let mut checked = 0;
        for (a, b) in [
            ((0, 0), (1, 0)),
            ((0, 0), (-1, 0)),
            ((5, 0), (6, 0)),
            ((5, 0), (5, 1)),
            ((5, 0), (4, 1)),
        ] {
            let (ca, cb) = (Axial::new(a.0, a.1), Axial::new(b.0, b.1));
            let e = (0..6u8).find(|&e| ca.neighbor(e) == cb).unwrap();
            assert!(!lay[&ca].exposed.contains(&e), "{a:?} -> {b:?} exposed");
            assert!(!lay[&cb].exposed.contains(&opposite_edge(e)));
            let (fa, fb) = (lay[&ca].field(), lay[&cb].field());
            let mid = (center(ca) + center(cb)) * 0.5;
            let along = edge_dir(e).perp();
            for i in 0..=32 {
                let p = mid + along * (-8.0 + 0.5 * i as f32);
                let (wa, wb) = (fa.water(p.x, p.y), fb.water(p.x, p.y));
                assert!(
                    (fa.height(p.x, p.y) - fb.height(p.x, p.y)).abs() < 1e-3,
                    "{a:?}|{b:?} at {p}"
                );
                assert!((wa.depth - wb.depth).abs() < 1e-3 && (wa.river - wb.river).abs() < 1e-4);
                assert!(
                    wa.flow.distance(wb.flow) < 1e-3,
                    "{a:?}|{b:?} flow {} vs {}",
                    wa.flow,
                    wb.flow
                );
                // Swash only acts in water shallower than 2.5 m, where the shore must agree too.
                if wa.depth < 2.5 {
                    assert!((wa.shore - wb.shore).abs() < 1e-3);
                }
                checked += 1;
            }
        }
        assert_eq!(checked, 5 * 33);
    }

    /// Exposed edges follow adjacency: open river ends and lone ocean sides get a cut face; joined
    /// water edges and dry edges do not; a beach edge against an ocean side is a mismatched seam.
    #[test]
    fn exposed_edges_follow_adjacency_and_sockets() {
        let cat = catalog();
        let lone = layout(&[tile(0, 0, "river_straight", 0)], &cat, |_| false);
        assert_eq!(lone[&Axial::new(0, 0)].exposed, vec![0, 3]);
        let pair = layout(
            &[
                tile(0, 0, "river_straight", 0),
                tile(1, 0, "river_straight", 0),
            ],
            &cat,
            |_| false,
        );
        assert_eq!(pair[&Axial::new(0, 0)].exposed, vec![3]);
        assert_eq!(pair[&Axial::new(1, 0)].exposed, vec![0]);
        let ocean = layout(&[tile(0, 0, "ocean", 0)], &cat, |_| false);
        assert_eq!(ocean[&Axial::new(0, 0)].exposed, vec![0, 1, 2, 3, 4, 5]);
        // Beach at rotation 0: ocean edges 0 and 1, beach edges 2 and 5, grass edges 3 and 4.
        let beach = layout(&[tile(0, 0, "beach", 0)], &cat, |_| false);
        assert_eq!(beach[&Axial::new(0, 0)].exposed, vec![0, 1, 2, 5]);
        // An ocean tile beside the beach's edge 2: a legal seam whose beds differ.
        let side = layout(
            &[tile(0, 0, "beach", 0), tile(-1, 1, "ocean", 0)],
            &cat,
            |_| false,
        );
        assert!(side[&Axial::new(0, 0)].exposed.contains(&2));
        assert!(side[&Axial::new(-1, 1)].exposed.contains(&5));
    }

    /// The proof layout (docs/checkpoints/water-play/water-play.json) is legal in placement order.
    #[test]
    fn proof_layout_places_legally() {
        let cat = catalog();
        let mut world = World::new(1);
        for (q, r, id, steps) in PROOF {
            let def = cat.get(id).unwrap();
            let cell = Axial::new(q, r);
            let legal = world.legal_rotations(&cat, cell, def);
            let rot = *legal
                .get(steps)
                .unwrap_or_else(|| panic!("{id} at ({q}, {r}): legal {legal:?}"));
            world.place(&cat, cell, id, rot).unwrap();
        }
        let tiles: Vec<PlayTile> = world
            .tiles()
            .iter()
            .map(|(c, t)| PlayTile {
                cell: *c,
                tile: t.def.clone(),
                rotation: t.rotation,
            })
            .collect();
        let rot = |q, r| world.tile(Axial::new(q, r)).unwrap().rotation;
        // River seams join different rotations: straight r1 | bend r2 | bridge r5 | straight r2.
        assert_eq!(
            [rot(-1, 0), rot(-1, 1), rot(-2, 2), rot(-3, 3)],
            [1, 2, 5, 2]
        );
        // Ocean seams join rotations 0, 2 and 4.
        assert_eq!([rot(2, 0), rot(1, 1), rot(3, -1)], [0, 2, 4]);
        let lay = layout(&tiles, &cat, |_| false);
        assert_eq!(lay.len(), 9);
        // The chain starts at its lowest open end, (-3, 3) edge 2, and flows to (-1, 0) edge 4.
        let flows: Vec<(u8, u8)> = [(-3, 3), (-2, 2), (-1, 1), (-1, 0)]
            .map(|(q, r)| {
                let p = &lay[&Axial::new(q, r)].paths[0];
                (p.up, p.down)
            })
            .to_vec();
        // One way along the chain: each downstream edge faces the next tile's upstream edge.
        for w in flows.windows(2) {
            assert_eq!(w[1].0, opposite_edge(w[0].1), "{flows:?}");
        }
        let exposed = |q, r| lay[&Axial::new(q, r)].exposed.clone();
        assert_eq!(flows[0].0, 2);
        assert_eq!(flows[3].1, 4);
        assert_eq!(exposed(-1, 0), vec![4], "open downstream end");
        assert_eq!(exposed(-1, 1), Vec::<u8>::new());
        assert_eq!(exposed(-3, 3), vec![2], "open upstream end");
        // The coast: beach (1,0) meets the meadows on 3 and 4, beach (2,-1) on 5 (one coast line)
        // and ocean on 0 and 1, so only its beach edge 2 is open; (2,-1) mirrors it on edge 5.
        assert_eq!(exposed(1, 0), vec![2]);
        assert_eq!(exposed(2, -1), vec![5]);
        assert_eq!(exposed(1, 1), vec![0, 1, 2, 3]);
        assert_eq!(exposed(3, -1), vec![0, 1, 4, 5]);
    }

    /// River strips: the end rows lie on their lane edges, centred on the midpoint, and every row's
    /// lateral axis keeps the side of its neighbours (a flipped end row twists the first quad
    /// strip into a bow tie that leaves a hole at the seam).
    #[test]
    fn ribbon_rows_end_on_the_edges_without_twisting() {
        let cat = catalog();
        let tiles = [
            tile(-1, 0, "river_straight", 1),
            tile(-1, 1, "river_bend", 2),
            tile(-2, 2, "river_road_bridge", 5),
        ];
        let lay = layout(&tiles, &cat, |_| false);
        for cw in lay.values() {
            let p = &cw.paths[0];
            let rows = ribbon_rows(p);
            let c = center(cw.tile.cell);
            for (end, e) in [(rows[0], p.up), (rows[rows.len() - 1], p.down)] {
                assert!(end.0.distance(c + edge_dir(e) * APOTHEM) < 1e-3);
                assert!(
                    end.1.dot(edge_dir(e)).abs() < 1e-5,
                    "end row along edge {e}"
                );
            }
            for w in rows.windows(2) {
                assert!(
                    w[0].1.dot(w[1].1) > 0.99,
                    "{:?}: {:?} then {:?}",
                    cw.tile,
                    w[0],
                    w[1]
                );
            }
        }
    }

    /// Placement order of `docs/checkpoints/water-play/water-play.json`: (q, r, tile, rotate steps
    /// past the auto rotation). A river chain south-west of the start meadow (straight, bend,
    /// bridge, straight) and a coast to the east (beach, ocean, beach, ocean) with a meadow.
    const PROOF: [(i32, i32, &str, usize); 10] = [
        (-1, 0, "river_straight", 0),
        (-1, 1, "river_bend", 0),
        (-2, 2, "river_road_bridge", 1),
        (-3, 3, "river_straight", 0),
        (1, 0, "beach", 0),
        (2, 0, "ocean", 0),
        (1, 1, "ocean", 2),
        (2, -1, "beach", 0),
        (3, -1, "ocean", 4),
        (1, -1, "meadow", 0),
    ];
}
