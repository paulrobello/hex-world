//! Water tiles in `play` (PRD §9.1): the approved water material on placed ocean, beach and river
//! tiles, the cut faces on their exposed edges, and a placeholder bed where a tile's base piece is
//! missing from the kit. The numbers here are the bed contract in `docs/kit/water-contract.md`.
//!
//! Each water tile gets a [`TileField`]: tile-local bed profiles that depend only on the tile's
//! sockets, so they match across every legal seam, sampled at world positions. River centre lines
//! come from the tile's network segments and from its connected river neighbours, so a point on a
//! seam gets the same distance, flow and depth from both tiles.
use hex_play_state::{PlayTile, RisePart, RiseRole};
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
    surface_mesh_where, terrain_color, terrain_mesh_for,
};
use crate::{SceneEntity, post::QualityTier};

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
/// Coast profile along `s`, the signed distance from the tile's [`Coastline`] (m, positive out to
/// sea): grass level up to [`BEACH_TOP_S`], the beach slope down to the waterline at s = 0, a
/// shallow shelf to [`SHELF_S`], then the drop to the ocean bed, reached at [`DROP_END_S`].
pub const BEACH_TOP_S: f32 = -8.0;
pub const SHELF_S: f32 = 3.0;
pub const SHELF_SLOPE: f32 = 0.12;
pub const DROP_END_S: f32 = 8.0;
/// Radius (m) of the coast-corner tiles' coastline: the circle centred where the lines of the two
/// beach edges meet, two apothems out past the edge between them, through both beach-edge
/// midpoints (1.5 R).
pub const COAST_ARC_RADIUS: f32 = 1.5 * HEX_RADIUS;
/// Shore distance reported where no coast is near (river tiles) and on the open ocean.
const INLAND: f32 = -40.0;
const OPEN_SEA: f32 = 2.0 * DROP_END_S;
/// Distance past the channel reported where no river is near.
const NO_RIVER: f32 = 1.0e4;
/// Largest angle one river arc sample spans (2°: under 4 mm off the circle at the 24 m bend).
const ARC_STEP: f32 = std::f32::consts::PI / 90.0;
/// River mouth (water contract §3a): the channel's carve half-width grows from the river's
/// [`RIVER_CUT_W`] at the beach top to this at the waterline, over [`MOUTH_W_RAMP`] in s.
pub const MOUTH_W_SEA: f32 = 5.41;
pub const MOUTH_W_RAMP: (f32, f32) = (BEACH_TOP_S, 0.0);
/// The channel's bowl below its 0.05 m lip: the river's 1.5 m inland, shallowing over
/// [`MOUTH_BOWL_RAMP`] to the shelf depth at the shelf joint (0.36 m of water on the centre line).
const RIVER_BOWL: f32 = 1.5;
pub const MOUTH_BOWL_SHELF: f32 = 0.31;
pub const MOUTH_BOWL_RAMP: (f32, f32) = (BEACH_TOP_S, SHELF_S);
/// River weight and flow hand over to the swash over this range of s.
pub const MOUTH_FADE: (f32, f32) = (-3.0, 0.0);
/// Half-width in s of the foam band where the river meets the surf.
const MOUTH_FOAM_S: f32 = 1.0;
/// Spring pool (water contract §3b): the bowl's carve half-width (5.5 m of water, the channel's
/// 4.33 / 4.0 ratio), the surface disc's radius and the run over which the flow picks up.
pub const POOL_W: f32 = 5.95;
const POOL_SURFACE: f32 = 6.5;
const SPRING_RAMP: f32 = 6.5;
/// River junction (`river_fork`, water contract §2a): how far from an arm's centre line its
/// tangent still steers the blended flow. Under the 12 m between a River edge's lane point and
/// the other arms, so each edge's strip carries its own arm's flow, as the neighbour does.
const JUNCTION_BLEND: f32 = 8.0;

const APOTHEM: f32 = HEX_RADIUS * 0.866_025_4;

/// Bed height (m) at `s` m seaward of a coast tile's [`Coastline`].
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

/// A spring's pool bed (m) at `dist` m from its node.
pub fn pool_height(dist: f32) -> f32 {
    WATER_Y + carve(LAND, dist, POOL_W)
}

/// The mouth channel's carve half-width at `s`.
pub fn mouth_w(s: f32) -> f32 {
    RIVER_CUT_W + (MOUTH_W_SEA - RIVER_CUT_W) * smoothstep(MOUTH_W_RAMP.0, MOUTH_W_RAMP.1, s)
}

fn mouth_bowl(s: f32) -> f32 {
    RIVER_BOWL
        + (MOUTH_BOWL_SHELF - RIVER_BOWL) * smoothstep(MOUTH_BOWL_RAMP.0, MOUTH_BOWL_RAMP.1, s)
}

/// River mouth bed (m) at `s` m seaward of the coastline and `dist` m from the channel's centre
/// line: the coast profile, lowered by the channel. Where s <= -8 (the grass reach and the whole
/// River edge) the channel is [`river_height`] exactly; past s = 3 the coast is deeper.
pub fn mouth_height(s: f32, dist: f32) -> f32 {
    let (w, bowl) = (mouth_w(s), mouth_bowl(s));
    let bed = -bowl * (1.0 - (dist / w).powi(2)).max(0.0) - 0.05;
    let bank = smoothstep(w * 0.7, w + 2.5, dist);
    let channel = WATER_Y + (bed + (LAND - bed) * bank).min(LAND);
    coast_height(s).min(channel)
}

/// A river mouth's bed in tile-local metres (world orientation), as the kit gate reads it: the
/// straight coast and the channel's centre line, a ray from the River edge's lane point through
/// the mouth node and on out to sea.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mouth {
    pub coast: Coastline,
    pub start: Vec2,
    pub dir: Vec2,
}

impl Mouth {
    /// The mouth of `def` placed at `rotation`: a coastline and a River segment from an edge to a
    /// node.
    pub fn of(def: &TileDef, rotation: u8) -> Option<Mouth> {
        let coast = Coastline::of(def, rotation)?;
        let (edge, node) = river_node_end(def)?;
        let start = turn(rotation, lane_point(def, edge));
        let node = turn(rotation, node);
        Some(Mouth {
            coast,
            start,
            dir: (node - start).normalize(),
        })
    }

    pub fn distance(&self, p: Vec2) -> f32 {
        let t = (p - self.start).dot(self.dir).max(0.0);
        p.distance(self.start + self.dir * t)
    }

    pub fn height(&self, p: Vec2) -> f32 {
        mouth_height(self.coast.seaward(p), self.distance(p))
    }
}

/// The lane point of local edge `local` (tile-local, unrotated).
fn lane_point(def: &TileDef, local: u8) -> Vec2 {
    let s = &def.edges[local as usize];
    let n = edge_dir(local);
    n * APOTHEM + Vec2::new(-n.y, n.x) * s.lane.map_or(0.0, |l| l.offset)
}

/// A tile-local offset turned to world orientation by `rotation`.
fn turn(rotation: u8, p: Vec2) -> Vec2 {
    let r = crate::kit_scene::piece_rotation(rotation) * Vec3::new(p.x, 0.0, p.y);
    Vec2::new(r.x, r.z)
}

/// The first River segment that joins an edge to a node that ends the river (a spring or a mouth,
/// not a [`river_junction`]): (local edge, node position, tile-local).
fn river_node_end(def: &TileDef) -> Option<(u8, Vec2)> {
    def.networks
        .iter()
        .filter(|n| n.kind == NetworkKind::River)
        .find_map(|n| match (&n.from, &n.to) {
            (Endpoint::Edge(e), Endpoint::Node(id)) | (Endpoint::Node(id), Endpoint::Edge(e))
                if !river_junction(def, id) =>
            {
                def.nodes
                    .iter()
                    .find(|k| &k.id == id)
                    .map(|k| (*e, Vec2::from(k.at)))
            }
            _ => None,
        })
}

/// Whether node `id` of `def` joins two or more River segments (`river_fork`'s centre): the arms
/// meet there, so it is neither a spring nor a mouth, and the tile is a plain river tile whose
/// bed is the channel along every arm (water contract §2a).
fn river_junction(def: &TileDef, id: &str) -> bool {
    let at = |e: &Endpoint| matches!(e, Endpoint::Node(k) if k == id);
    def.networks
        .iter()
        .filter(|n| n.kind == NetworkKind::River && (at(&n.from) || at(&n.to)))
        .count()
        >= 2
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

/// A coast tile's coastline, in world-oriented metres from the tile centre: the waterline through
/// the midpoints of its two beach edges, crossing each at a right angle (owner O1). Its signed
/// distance [`Coastline::seaward`] is the `s` of [`coast_height`]. Along every beach edge `s` runs
/// linearly from -8 at the land corner to +8 at the sea corner, the same for all three shapes, so
/// beds meet across every beach-to-beach seam. Ocean edges lie at s >= 8 (the ocean bed) and grass
/// edges at s <= -8 (grass level), so beds also meet ocean and grass neighbours.
///
/// It comes from the edge kinds alone (beach edges and the grass beside them), never from the
/// sockets' land sides, so the seam test checks those sides against the geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Coastline {
    /// `beach`: the straight line through the centre; `sea` is the unit seaward normal.
    Straight { sea: Vec2 },
    /// `beach_outer` (the headland, land inside) and `beach_inner` (the bay, sea inside): the
    /// [`COAST_ARC_RADIUS`] circle about `centre`, two apothems out past the edge between the two
    /// beach edges, on both beach edges' lines.
    Arc { centre: Vec2, land_inside: bool },
}

impl Coastline {
    /// The coastline of `def` placed at `rotation`: two beach edges, one with land (grass, forest
    /// or a river mouth's River) on the next edge clockwise, where the coast leaves with the land on its right, and
    /// one with land on the edge before it, 3 edges apart (straight), 2 (land between them: a
    /// headland) or 4 (sea between them: a bay). `None` for any other edge layout.
    pub fn of(def: &TileDef, rotation: u8) -> Option<Coastline> {
        let kind = |w: u8| def.edge(w % 6, rotation).kind;
        let land = |w: u8| {
            matches!(
                kind(w),
                SocketKind::Grass | SocketKind::Forest | SocketKind::River
            )
        };
        let beach: Vec<u8> = (0..6).filter(|&w| kind(w) == SocketKind::Beach).collect();
        let [a, b] = beach[..] else {
            return None;
        };
        let (right, left) = if land(a + 1) { (a, b) } else { (b, a) };
        if !land(right + 1) || !land(left + 5) {
            return None;
        }
        let beyond = |e: u8| edge_dir(e) * 2.0 * APOTHEM;
        match (left + 6 - right) % 6 {
            // Sea on the left of the right-hand beach edge, looking out across it.
            3 => Some(Coastline::Straight {
                sea: -edge_dir(right).perp(),
            }),
            2 => Some(Coastline::Arc {
                centre: beyond((right + 1) % 6),
                land_inside: true,
            }),
            4 => Some(Coastline::Arc {
                centre: beyond((right + 5) % 6),
                land_inside: false,
            }),
            _ => None,
        }
    }

    /// Signed distance (m) from the coastline to `p`, metres from the tile centre in world
    /// orientation: positive out to sea.
    pub fn seaward(&self, p: Vec2) -> f32 {
        match *self {
            Coastline::Straight { sea } => p.dot(sea),
            Coastline::Arc {
                centre,
                land_inside,
            } => {
                let d = p.distance(centre) - COAST_ARC_RADIUS;
                if land_inside { d } else { -d }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum WaterKind {
    /// Carries a river network: a channel and a surface strip along each river segment. A tile
    /// whose segments meet at a junction (`river_fork`) is one too: the channel along every arm,
    /// under one surface.
    River,
    /// A river network ending at a node on a tile without a coastline: the pool around the node
    /// (offset from the tile centre, world orientation) and its stream, under one surface.
    Spring(Vec2),
    /// A river network ending at a node on a coast tile: the coast and the channel fanning into
    /// it, under one coast surface.
    RiverMouth(Coastline),
    /// Some ocean edges: a coast (straight beach, headland or bay).
    Coast(Coastline),
    /// Ocean on every edge.
    Ocean,
}

/// What water a tile carries at `rotation`, if any.
/// Whether `def` takes the placeholder bed: a water tile whose base piece is missing from the kit
/// (failed, or outside the manifest). Every water tile's base is its bed (water contract §5), so a
/// missing or loading centre piece never puts the stand-in over a kit base.
pub fn stand_in_bed(def: &TileDef, kit_has: impl Fn(&str) -> bool) -> bool {
    water_kind(def, 0).is_some() && !kit_has(def.base.as_str())
}

pub fn water_kind(def: &TileDef, rotation: u8) -> Option<WaterKind> {
    if def.networks.iter().any(|n| n.kind == NetworkKind::River) {
        if let Some(coast) = Coastline::of(def, rotation) {
            return Some(WaterKind::RiverMouth(coast));
        }
        if let Some((_, node)) = river_node_end(def) {
            return Some(WaterKind::Spring(turn(rotation, node)));
        }
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
        _ => Coastline::of(def, rotation).map(WaterKind::Coast),
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
            let Some(WaterKind::Coast(coast) | WaterKind::RiverMouth(coast)) =
                water_kind(def, t.rotation)
            else {
                return EdgeWater::Dry;
            };
            let corner = |deg: f32| {
                let a = deg.to_radians();
                Vec2::new(a.cos(), a.sin()) * HEX_RADIUS
            };
            let mid = 60.0 * world as f32;
            let (a, b) = (corner(mid - 30.0), corner(mid + 30.0));
            let sea = if coast.seaward(a) > coast.seaward(b) {
                a
            } else {
                b
            };
            EdgeWater::Coast(center(t.cell) + sea)
        }
        _ => EdgeWater::Dry,
    }
}

/// Where a river segment ends: a world edge's lane, a spring (a node on a tile without a
/// coastline), a mouth (a node on a coast tile) or a junction (a node where two or more of the
/// tile's River segments meet, [`river_junction`]; the tile's arms continue each other there).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiverEnd {
    Edge(u8),
    Spring,
    Mouth,
    Junction,
}

impl RiverEnd {
    pub fn edge(self) -> Option<u8> {
        match self {
            RiverEnd::Edge(e) => Some(e),
            _ => None,
        }
    }

    /// Which end a chain's flow starts from: a spring, else an open edge, else a mouth (water
    /// contract §6). Chains that reach a junction are oriented by [`junction_sources`] instead.
    fn source_rank(self) -> u8 {
        match self {
            RiverEnd::Spring => 0,
            RiverEnd::Edge(_) | RiverEnd::Junction => 1,
            RiverEnd::Mouth => 2,
        }
    }
}

/// A river centre line in world XZ, sampled densely, upstream first.
#[derive(Clone, Debug, PartialEq)]
pub struct RiverPath {
    pub points: Vec<Vec2>,
    /// Where the path enters and leaves the tile.
    pub up: RiverEnd,
    pub down: RiverEnd,
}

impl RiverPath {
    /// One arm of a junction tile: the path enters or leaves at the junction node.
    pub fn at_junction(&self) -> bool {
        self.up == RiverEnd::Junction || self.down == RiverEnd::Junction
    }
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

/// River segments of a placed tile as (from end, to end, centre line from -> to). A mouth's
/// centre line carries on past its node by an apothem, out to sea: the fan's bed follows it.
fn river_segments(def: &TileDef, t: &PlayTile) -> Vec<(RiverEnd, RiverEnd, Vec<Vec2>)> {
    let node_end = if Coastline::of(def, t.rotation).is_some() {
        RiverEnd::Mouth
    } else {
        RiverEnd::Spring
    };
    let end = |e: &Endpoint| match e {
        Endpoint::Edge(k) => Some((
            RiverEnd::Edge(world_edge(*k, t.rotation)),
            lane_point(def, *k),
        )),
        Endpoint::Node(id) => def.nodes.iter().find(|n| &n.id == id).map(|n| {
            let kind = if river_junction(def, id) {
                RiverEnd::Junction
            } else {
                node_end
            };
            (kind, Vec2::from(n.at))
        }),
    };
    def.networks
        .iter()
        .filter(|n| n.kind == NetworkKind::River)
        .filter_map(|n| {
            let ((a, pa), (b, pb)) = (end(&n.from)?, end(&n.to)?);
            let pts: Vec<Vec2> = n.points.iter().map(|p| Vec2::from(*p)).collect();
            let mut local = match pts.as_slice() {
                [] => vec![pa, pb],
                [m] => arc_through(pa, *m, pb),
                _ => [vec![pa], pts.clone(), vec![pb]].concat(),
            };
            let k = local.len();
            if b == RiverEnd::Mouth {
                let dir = (local[k - 1] - local[k - 2]).normalize();
                local.push(pb + dir * APOTHEM);
            } else if a == RiverEnd::Mouth {
                let dir = (local[0] - local[1]).normalize();
                local.insert(0, pa + dir * APOTHEM);
            }
            let world = local
                .into_iter()
                .map(|p| to_world(t.cell, t.rotation, p))
                .collect();
            Some((a, b, world))
        })
        .collect()
}

/// One river segment of a placed tile: (from end, to end, world centre line from -> to).
type Segment = (RiverEnd, RiverEnd, Vec<Vec2>);
/// A chain end: (cell, the end, segment index on that cell).
type ChainEnd = (Axial, RiverEnd, usize);

/// Every placed river segment, oriented so the flow runs one way along each connected chain (water
/// contract §6): each chain starts at its spring, else at its open end with the lowest
/// (cell, edge), else at its mouth; among several of the same kind, at the lowest cell. A loop
/// starts at its lowest cell. Chains that meet at a junction (`river_fork`) are oriented together
/// by [`junction_sources`]. Keyed by (cell, segment index among the tile's river segments).
pub fn oriented_rivers(
    tiles: &[PlayTile],
    catalog: &TileCatalog,
) -> BTreeMap<(Axial, usize), RiverPath> {
    let mut segs: BTreeMap<(Axial, usize), Segment> = BTreeMap::new();
    let mut rotations: HashMap<Axial, u8> = HashMap::new();
    for t in tiles {
        if let Some(def) = catalog.get(&t.tile) {
            for (i, s) in river_segments(def, t).into_iter().enumerate() {
                segs.insert((t.cell, i), s);
            }
            rotations.insert(t.cell, t.rotation);
        }
    }
    // The segment across `end` of `cell`, if it is an edge with a segment ending across it.
    let across = |cell: Axial, end: RiverEnd| -> Option<(Axial, usize)> {
        let e = end.edge()?;
        let n = cell.neighbor(e);
        let back = RiverEnd::Edge(opposite_edge(e));
        segs.range((n, 0)..=(n, usize::MAX))
            .find(|(_, (a, b, _))| *a == back || *b == back)
            .map(|(k, _)| *k)
    };
    // The connected chains, each with its open ends.
    let mut chains: Vec<(Vec<(Axial, usize)>, Vec<ChainEnd>)> = vec![];
    let mut seen: BTreeSet<(Axial, usize)> = BTreeSet::new();
    for &key in segs.keys() {
        if !seen.insert(key) {
            continue;
        }
        let mut chain = vec![key];
        let mut i = 0;
        while i < chain.len() {
            let (a, b, _) = segs[&chain[i]];
            for e in [a, b] {
                if let Some(k) = across(chain[i].0, e)
                    && seen.insert(k)
                {
                    chain.push(k);
                }
            }
            i += 1;
        }
        let ends: Vec<ChainEnd> = chain
            .iter()
            .flat_map(|k| {
                let (a, b, _) = segs[k];
                [a, b]
                    .into_iter()
                    .filter(|&e| across(k.0, e).is_none())
                    .map(move |e| (k.0, e, k.1))
            })
            .collect();
        chains.push((chain, ends));
    }
    let picked = junction_sources(&chains, &segs, &rotations);
    let mut out = BTreeMap::new();
    let mut walked: BTreeSet<(Axial, usize)> = BTreeSet::new();
    for (ci, (chain, ends)) in chains.iter().enumerate() {
        let start = picked.get(&ci).copied().or_else(|| {
            ends.iter()
                .map(|&(cell, e, i)| (e.source_rank(), cell, e, i))
                .min()
                .map(|(_, cell, e, i)| (cell, e, i))
        });
        let (mut cur, mut up) = match start {
            Some((cell, e, i)) => ((cell, i), e),
            None => {
                let k = *chain.iter().min().expect("non-empty chain");
                let (a, b, _) = segs[&k];
                (k, a.min(b))
            }
        };
        loop {
            walked.insert(cur);
            let (a, b, pts) = &segs[&cur];
            let (down, points) = if *a == up {
                (*b, pts.clone())
            } else {
                (*a, pts.iter().rev().copied().collect())
            };
            out.insert(cur, RiverPath { points, up, down });
            match (across(cur.0, down), down) {
                (Some(next), RiverEnd::Edge(e)) if !walked.contains(&next) => {
                    up = RiverEnd::Edge(opposite_edge(e));
                    cur = next;
                }
                _ => break,
            }
        }
    }
    out
}

/// A node of the river network that chains join at: a junction (one per tile, keyed by its cell)
/// or a chain's open end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum NetVertex {
    Junction(Axial),
    End(Axial, RiverEnd, usize),
}

impl NetVertex {
    fn of(&(cell, e, i): &ChainEnd) -> Self {
        if e == RiverEnd::Junction {
            NetVertex::Junction(cell)
        } else {
            NetVertex::End(cell, e, i)
        }
    }
}

/// The upstream end of every chain that reaches a junction (`river_fork`), by chain index (water
/// contract §6, junctions). Chains joined at junctions form one network, and every chain in it runs
/// downhill, measured in metres along the centre lines:
/// - toward the nearest mouth when the network reaches one, so a fork with one arm toward the sea
///   is a confluence and a fork with two arms that each reach the sea first is a split;
/// - else away from the nearest spring, so one spring through a fork splits and two springs join;
/// - else out through the lowest junction's local edge 0 arm, as if the sea lay past that arm's
///   open end (the lowest open end when that arm meets another junction): a lone fork is a
///   confluence whose rotation picks the outlet.
///
/// A chain whose two ends are equally far keeps the plain chain rule of [`oriented_rivers`].
fn junction_sources(
    chains: &[(Vec<(Axial, usize)>, Vec<ChainEnd>)],
    segs: &BTreeMap<(Axial, usize), Segment>,
    rotations: &HashMap<Axial, u8>,
) -> BTreeMap<usize, ChainEnd> {
    let length = |chain: &[(Axial, usize)]| -> f32 {
        chain
            .iter()
            .map(|k| {
                let pts = &segs[k].2;
                pts.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>()
            })
            .sum()
    };
    // Linear chains with a junction end: (chain index, end a, end b, length).
    let links: Vec<(usize, ChainEnd, ChainEnd, f32)> = chains
        .iter()
        .enumerate()
        .filter_map(|(ci, (chain, ends))| {
            let [a, b] = ends[..] else {
                return None;
            };
            (a.1 == RiverEnd::Junction || b.1 == RiverEnd::Junction)
                .then(|| (ci, a, b, length(chain)))
        })
        .collect();
    let mut out = BTreeMap::new();
    let mut done = vec![false; links.len()];
    for first in 0..links.len() {
        if done[first] {
            continue;
        }
        // The network: links joined at their junctions.
        done[first] = true;
        let mut net = vec![first];
        let mut i = 0;
        while i < net.len() {
            let (_, a, b, _) = links[net[i]];
            for v in [NetVertex::of(&a), NetVertex::of(&b)] {
                if !matches!(v, NetVertex::Junction(_)) {
                    continue;
                }
                for (j, l) in links.iter().enumerate() {
                    if !done[j] && (NetVertex::of(&l.1) == v || NetVertex::of(&l.2) == v) {
                        done[j] = true;
                        net.push(j);
                    }
                }
            }
            i += 1;
        }
        let ends: Vec<ChainEnd> = net
            .iter()
            .flat_map(|&j| [links[j].1, links[j].2])
            .filter(|e| e.1 != RiverEnd::Junction)
            .collect();
        let leaves = |kind: fn(&RiverEnd) -> bool| -> Vec<NetVertex> {
            ends.iter()
                .filter(|e| kind(&e.1))
                .map(NetVertex::of)
                .collect()
        };
        let mouths = leaves(|e| *e == RiverEnd::Mouth);
        let springs = leaves(|e| *e == RiverEnd::Spring);
        // The distance seeds, and whether the flow runs toward them or away from them.
        let (seeds, toward) = if !mouths.is_empty() {
            (mouths, true)
        } else if !springs.is_empty() {
            (springs, false)
        } else {
            let Some(outlet) = open_outlet(&net, &links, chains, segs, rotations) else {
                continue;
            };
            (vec![outlet], true)
        };
        let mut dist: BTreeMap<NetVertex, f32> = seeds.iter().map(|&v| (v, 0.0)).collect();
        loop {
            let mut changed = false;
            for &j in &net {
                let (_, a, b, len) = links[j];
                let (va, vb) = (NetVertex::of(&a), NetVertex::of(&b));
                for (x, y) in [(va, vb), (vb, va)] {
                    let Some(&dx) = dist.get(&x) else {
                        continue;
                    };
                    if dist.get(&y).is_none_or(|&dy| dx + len < dy - 1e-4) {
                        dist.insert(y, dx + len);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        for &j in &net {
            let (ci, a, b, _) = links[j];
            let (Some(&da), Some(&db)) =
                (dist.get(&NetVertex::of(&a)), dist.get(&NetVertex::of(&b)))
            else {
                continue;
            };
            if (da - db).abs() < 1e-3 {
                continue;
            }
            out.insert(ci, if (da > db) == toward { a } else { b });
        }
    }
    out
}

/// The outlet of a junction network with neither a mouth nor a spring: the open end of the chain
/// that leaves the lowest junction through its local edge 0, else the network's lowest open end.
fn open_outlet(
    net: &[usize],
    links: &[(usize, ChainEnd, ChainEnd, f32)],
    chains: &[(Vec<(Axial, usize)>, Vec<ChainEnd>)],
    segs: &BTreeMap<(Axial, usize), Segment>,
    rotations: &HashMap<Axial, u8>,
) -> Option<NetVertex> {
    let junction = net
        .iter()
        .flat_map(|&j| [links[j].1, links[j].2])
        .filter(|e| e.1 == RiverEnd::Junction)
        .map(|e| e.0)
        .min()?;
    let out_edge = RiverEnd::Edge(world_edge(
        0,
        rotations.get(&junction).copied().unwrap_or(0),
    ));
    let arm = segs
        .range((junction, 0)..=(junction, usize::MAX))
        .find(|(_, (a, b, _))| *a == out_edge || *b == out_edge)
        .map(|(k, _)| *k);
    let far = arm.and_then(|arm| {
        let &(_, a, b, _) = links.iter().find(|l| chains[l.0].0.contains(&arm))?;
        let far = if a == (junction, RiverEnd::Junction, arm.1) {
            b
        } else {
            a
        };
        matches!(far.1, RiverEnd::Edge(_)).then(|| NetVertex::of(&far))
    });
    far.or_else(|| {
        net.iter()
            .flat_map(|&j| [links[j].1, links[j].2])
            .filter(|e| matches!(e.1, RiverEnd::Edge(_)))
            .min()
            .map(|e| NetVertex::of(&e))
    })
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
    /// Draw the placeholder bed ([`stand_in_bed`]: the tile's base piece is not in the kit).
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
            for e in [p.up, p.down].into_iter().filter_map(RiverEnd::edge) {
                let n = t.cell.neighbor(e);
                let back = RiverEnd::Edge(opposite_edge(e));
                paths.extend(
                    rivers
                        .range((n, 0)..=(n, usize::MAX))
                        .filter(|(_, q)| q.up == back || q.down == back)
                        .map(|(_, q)| q.clone()),
                );
            }
        }
        let bed = stand_in_bed(def, &kit_has);
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

    /// On a junction tile, the flow direction at `p`: the tile's arms' downstream tangents averaged
    /// with weights that fall from 1 on an arm's centre line to 0 at [`JUNCTION_BLEND`] from it, so
    /// the flow turns from the inflows into the outflows across the centre instead of switching
    /// along the bisectors, and slows where they disagree (to 2/3 at the centre of a confluence or
    /// a split). `None` off junction tiles and wherever fewer than two arms are that near (all of
    /// each River edge's strip): there the nearest centre line's tangent applies, which is what the
    /// neighbouring tile computes from the same paths, and which the blend reaches as the second
    /// arm's weight falls to 0.
    fn junction_flow(&self, p: Vec2) -> Option<Vec2> {
        let arms = self.cell.paths[..self.cell.own_paths]
            .iter()
            .filter(|path| path.at_junction());
        let (mut sum, mut weight, mut near) = (Vec2::ZERO, 0.0, 0);
        for path in arms {
            let mut best = (f32::MAX, Vec2::ZERO);
            for w in path.points.windows(2) {
                let seg = w[1] - w[0];
                let len2 = seg.length_squared();
                if len2 <= 0.0 {
                    continue;
                }
                let t = ((p - w[0]).dot(seg) / len2).clamp(0.0, 1.0);
                let d = p.distance(w[0] + seg * t);
                if d < best.0 {
                    best = (d, seg / len2.sqrt());
                }
            }
            let k = 1.0 - smoothstep(0.0, JUNCTION_BLEND, best.0);
            sum += best.1 * k;
            weight += k;
            near += usize::from(k > 0.0);
        }
        (near >= 2).then(|| sum / weight)
    }

    fn seaward(&self, p: Vec2, coast: Coastline) -> f32 {
        coast.seaward(p - center(self.cell.tile.cell))
    }

    /// Distance from a spring's node.
    fn node_distance(&self, p: Vec2, node: Vec2) -> f32 {
        p.distance(center(self.cell.tile.cell) + node)
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
            WaterKind::Spring(node) => {
                river_height(self.river(p).0).min(pool_height(self.node_distance(p, node)))
            }
            WaterKind::RiverMouth(coast) => mouth_height(self.seaward(p, coast), self.river(p).0),
            WaterKind::Coast(coast) => coast_height(self.seaward(p, coast)),
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
                let dir = self.junction_flow(p).unwrap_or(tangent);
                WaterSample {
                    flow: dir * speed * river,
                    river,
                    depth,
                    shore: INLAND,
                }
            }
            WaterKind::Spring(node) => {
                let (d, tangent) = self.river(p);
                let r = self.node_distance(p, node);
                let (stream, speed) = river_flow(d, RIVER_CUT_W);
                let pool = 1.0 - smoothstep(POOL_W, POOL_W + 3.0, r);
                WaterSample {
                    flow: tangent * speed * stream * smoothstep(0.0, SPRING_RAMP, r),
                    river: stream.max(pool),
                    depth,
                    shore: INLAND,
                }
            }
            WaterKind::RiverMouth(coast) => {
                let s = self.seaward(p, coast);
                let (d, tangent) = self.river(p);
                let (channel, speed) = river_flow(d, mouth_w(s));
                let river = channel * (1.0 - smoothstep(MOUTH_FADE.0, MOUTH_FADE.1, s));
                WaterSample {
                    flow: tangent * speed * river,
                    river,
                    depth,
                    shore: s,
                }
            }
            WaterKind::Coast(coast) => WaterSample {
                flow: Vec2::ZERO,
                river: 0.0,
                depth,
                shore: self.seaward(p, coast),
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
            WaterKind::Spring(node) => terrain_color(
                (self.river(p).0 - RIVER_CUT_W).min(self.node_distance(p, node) - POOL_W),
                INLAND,
            ),
            WaterKind::RiverMouth(coast) => {
                let s = self.seaward(p, coast);
                terrain_color(self.river(p).0 - mouth_w(s), s)
            }
            WaterKind::Coast(coast) => terrain_color(NO_RIVER, self.seaward(p, coast)),
            WaterKind::Ocean => terrain_color(NO_RIVER, OPEN_SEA),
        }
    }

    fn meet(&self, x: f32, z: f32) -> f32 {
        let WaterKind::RiverMouth(coast) = self.cell.kind else {
            return 0.0;
        };
        let p = Vec2::new(x, z);
        let s = self.seaward(p, coast);
        let w = mouth_w(s);
        (1.0 - smoothstep(0.0, MOUTH_FOAM_S, s.abs()))
            * (1.0 - smoothstep(0.75 * w, w + 1.0, self.river(p).0))
    }

    /// The flow's advective acceleration on a river tile, `speed * |d flow / d s|` along the flow
    /// (central difference over [`CHURN_STEP`]): 0 on a straight, `speed² / R` round a bend of
    /// radius R, and where the fork's arms turn into each other and slow. Faded to 0 over
    /// [`CHURN_EDGE_FADE`] inside the hex border, so every river edge reads 0 on both sides.
    fn churn(&self, x: f32, z: f32) -> f32 {
        if self.cell.kind != WaterKind::River {
            return 0.0;
        }
        let p = Vec2::new(x, z);
        let u = self.water(x, z).flow;
        let speed = u.length();
        if speed < 1e-3 {
            return 0.0;
        }
        let step = u / speed * CHURN_STEP;
        let ahead = self.water(p.x + step.x, p.y + step.y).flow;
        let behind = self.water(p.x - step.x, p.y - step.y).flow;
        let accel = speed * (ahead - behind).length() / (2.0 * CHURN_STEP);
        let d = p - center(self.cell.tile.cell);
        let inside = APOTHEM - (0..6).map(|e| d.dot(edge_dir(e))).fold(f32::MIN, f32::max);
        accel * smoothstep(0.0, CHURN_EDGE_FADE, inside)
    }
}

/// Step (m) of the churn's central difference along the flow.
const CHURN_STEP: f32 = 0.5;
/// Width (m) inside the hex border over which the churn fades to 0 at the edge.
const CHURN_EDGE_FADE: f32 = 3.0;

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
            let lat = match (i, path.up, path.down) {
                (0, RiverEnd::Edge(e), _) => (-edge_dir(e)).perp(),
                (0, _, _) => (centres[1] - centres[0]).normalize().perp(),
                (_, _, RiverEnd::Edge(e)) if i == n => edge_dir(e).perp(),
                _ if i == n => (centres[n] - centres[n - 1]).normalize().perp(),
                _ => (centres[i + 1] - centres[i - 1]).normalize().perp(),
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
    set_transmission(cam, tier);
    cam.remove::<DepthOfField>();
}

/// Screen-space transmission on the play camera exactly where the tier's water refracts.
pub fn set_transmission(cam: &mut EntityCommands, tier: QualityTier) {
    if WaterKey::for_tier(tier).refraction {
        cam.insert_if_new(ScreenSpaceTransmission::default());
    } else {
        cam.remove::<ScreenSpaceTransmission>();
    }
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
///
/// Rise hooks: every spawned entity carries a [`RisePart`] (the bed moves with the column, the
/// surfaces and cut faces also fill), and the entities of a cell whose tile left the world are
/// returned instead of despawned, so the tile's sink can take them down with it.
#[must_use]
pub fn sync(
    commands: &mut Commands,
    water: &mut PlayWater,
    assets: WaterAssets,
    tier: QualityTier,
    tiles: &[PlayTile],
    catalog: &TileCatalog,
    kit_has: impl Fn(&str) -> bool,
    rebuild: bool,
) -> Vec<(Axial, Vec<Entity>)> {
    if rebuild {
        for (_, (_, es)) in water.cells.drain() {
            for e in es {
                commands.entity(e).try_despawn();
            }
        }
    }
    let want = layout(tiles, catalog, kit_has);
    let placed: BTreeSet<Axial> = tiles.iter().map(|t| t.cell).collect();
    let mut released = vec![];
    water.cells.retain(|cell, (cw, es)| {
        let keep = want.get(cell) == Some(cw);
        if !keep {
            if placed.contains(cell) {
                for e in es.drain(..) {
                    commands.entity(e).try_despawn();
                }
            } else {
                released.push((*cell, std::mem::take(es)));
            }
        }
        keep
    });
    if want.keys().all(|c| water.cells.contains_key(c)) {
        return released;
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
    released
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
        let t = cell_transform(cell, rot);
        out.push(
            commands
                .spawn((
                    Mesh3d(meshes.add(terrain_mesh_for(&field, cell, rot, FLOOR_Y))),
                    MeshMaterial3d(terrain.clone()),
                    t,
                    RisePart::new(cell, RiseRole::Bed, t),
                    SceneEntity,
                ))
                .id(),
        );
    }
    let mut surfaces = vec![];
    match cw.kind {
        WaterKind::River if cw.paths[..cw.own_paths].iter().any(RiverPath::at_junction) => {
            // A junction's arms meet at the centre, where strips would overlap: one hex-grid
            // surface kept where any of them reaches, as on a spring.
            let keep = |w: Vec2| field.river(w).0 < RIBBON_HALF + RIBBON_STEP;
            surfaces.push((
                surface_mesh_where(&field, cell, rot, keep),
                cell_transform(cell, rot),
            ));
        }
        WaterKind::River => {
            for p in &cw.paths[..cw.own_paths] {
                let rows = ribbon_rows(p);
                surfaces.push((
                    ribbon_mesh(&field, c, &rows, RIBBON_HALF, RIBBON_COLS),
                    at_cell,
                ));
            }
        }
        WaterKind::Spring(node) => {
            // One surface for the pool and its stream (a disc and a strip would overlap):
            // the hex grid, kept where either reaches.
            let pool = c + node;
            let keep = |w: Vec2| {
                w.distance(pool) < POOL_SURFACE || field.river(w).0 < RIBBON_HALF + RIBBON_STEP
            };
            surfaces.push((
                surface_mesh_where(&field, cell, rot, keep),
                cell_transform(cell, rot),
            ));
        }
        WaterKind::RiverMouth(_) | WaterKind::Coast(_) | WaterKind::Ocean => {
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
                    RisePart::new(cell, RiseRole::Water, t),
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
    use hex_core::{Side, World, opposite_edge};

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

    /// The world edge of a river end that must be one.
    fn edge(end: RiverEnd) -> u8 {
        end.edge()
            .unwrap_or_else(|| panic!("{end:?} is not an edge"))
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

    /// The kit wet band reads the shore distance back from the bed height
    /// (`water_common.wesl` `coast_shore`): its constants are this profile's, and the inversion
    /// (mirrored here) returns `s` on the beach slope and the shelf, where the band and swash live.
    #[test]
    fn wet_band_inverts_the_coast_profile() {
        let src = include_str!("../../../hex-render/src/water_common.wesl");
        for (name, v) in [
            ("COAST_WATER_Y", WATER_Y),
            ("COAST_LAND", LAND),
            ("COAST_BEACH_TOP_S", BEACH_TOP_S),
            ("COAST_SHELF_SLOPE", SHELF_SLOPE),
        ] {
            let line = format!("const {name}: f32 = {v:?};");
            assert!(src.contains(&line), "water_common.wesl lacks `{line}`");
        }
        let shore = |rel: f32| {
            let t = 1.0 - (1.0 - rel / LAND).max(0.0).sqrt();
            if rel < 0.0 {
                -rel / SHELF_SLOPE
            } else {
                BEACH_TOP_S * t
            }
        };
        for i in 0..=110 {
            let s = BEACH_TOP_S + i as f32 * 0.1;
            let back = shore(coast_height(s) - WATER_Y);
            assert!((back - s).abs() < 2e-3, "s {s}: inverted {back}");
        }
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
            ("river_bend_60", 0),
            ("river_bend_60", 3),
            ("river_straight", 1),
            ("river_road_bridge", 4),
        ] {
            let t = tile(0, 0, id, rot);
            for (a, b, pts) in river_segments(cat.get(id).unwrap(), &t) {
                let (a, b) = (edge(a), edge(b));
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
            if let Some((_, q)) = r.iter().find(|(j, _)| j.0 == k.0.neighbor(edge(p.down))) {
                assert_eq!(edge(q.up), opposite_edge(edge(p.down)), "{k:?}");
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

    /// The tight bend between two straights: the fields agree across both seams, and along the
    /// bend's centre line the flow keeps the straights' speed and turns smoothly with the path
    /// (no reversal, no jump at the stubs or between arc samples).
    #[test]
    fn tight_bend_flow_turns_smoothly_between_straights() {
        let cat = catalog();
        let tiles = [
            tile(0, 0, "river_straight", 0),
            tile(1, 0, "river_bend_60", 3),
            tile(1, -1, "river_straight", 1),
        ];
        let lay = layout(&tiles, &cat, |_| false);
        for (a, b) in [((0, 0), (1, 0)), ((1, 0), (1, -1))] {
            let (ca, cb) = (Axial::new(a.0, a.1), Axial::new(b.0, b.1));
            let e = (0..6u8).find(|&e| ca.neighbor(e) == cb).unwrap();
            assert!(!lay[&ca].exposed.contains(&e), "{a:?} -> {b:?} exposed");
            let (fa, fb) = (lay[&ca].field(), lay[&cb].field());
            let mid = (center(ca) + center(cb)) * 0.5;
            let along = edge_dir(e).perp();
            for i in 0..=32 {
                let p = mid + along * (-8.0 + 0.5 * i as f32);
                let (wa, wb) = (fa.water(p.x, p.y), fb.water(p.x, p.y));
                assert!((fa.height(p.x, p.y) - fb.height(p.x, p.y)).abs() < 1e-3);
                assert!((wa.depth - wb.depth).abs() < 1e-3 && (wa.river - wb.river).abs() < 1e-4);
                assert!(wa.flow.distance(wb.flow) < 1e-3, "{a:?}|{b:?} at {p}");
            }
        }
        let r = oriented_rivers(&tiles, &cat);
        let path = &r[&(Axial::new(1, 0), 0)];
        let straight = lay[&Axial::new(0, 0)].field();
        let c0 = center(Axial::new(0, 0));
        let speed = straight.water(c0.x, c0.y).flow.length();
        assert!(speed > 0.1, "{speed}");
        let bend = lay[&Axial::new(1, 0)].field();
        let mut prev: Option<Vec2> = None;
        for w in path.points.windows(2) {
            let p = (w[0] + w[1]) * 0.5;
            let f = bend.water(p.x, p.y).flow;
            assert!(
                (f.length() - speed).abs() < 1e-3,
                "speed {} at {p}",
                f.length()
            );
            assert!(
                f.normalize().dot((w[1] - w[0]).normalize()) > 0.999,
                "flow off the path at {p}"
            );
            if let Some(q) = prev {
                assert!(f.normalize().dot(q) > 0.995, "flow jumps at {p}");
            }
            prev = Some(f.normalize());
        }
        // 120 degrees of turn, start to end.
        let first = (path.points[1] - path.points[0]).normalize();
        let n = path.points.len();
        let last = (path.points[n - 1] - path.points[n - 2]).normalize();
        assert!(
            (first.dot(last) - (-0.5)).abs() < 1e-3,
            "{}",
            first.dot(last)
        );
    }

    /// Exposed edges follow adjacency: open river ends and lone ocean sides get a cut face; joined
    /// water edges and dry edges do not; a beach edge against an ocean side is a mismatched seam
    /// (the rules refuse it since owner O1, but the layout stays safe if one is ever loaded).
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
        // An ocean tile beside the beach's edge 2: beds that differ along the seam.
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
                (edge(p.up), edge(p.down))
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
            for (end, e) in [(rows[0], edge(p.up)), (rows[rows.len() - 1], edge(p.down))] {
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

    /// Tiles with a beach edge, in catalog order: `beach`, `beach_outer`, `beach_inner`.
    fn coast_tiles(cat: &TileCatalog) -> Vec<&TileDef> {
        cat.tiles()
            .iter()
            .filter(|t| t.edges.iter().any(|e| e.kind == SocketKind::Beach))
            .collect()
    }

    /// One tile's water on its own, so nothing a neighbour contributes can hide a mismatch.
    fn alone(cat: &TileCatalog, cell: Axial, id: &str, rotation: u8) -> CellWater {
        let t = tile(cell.q, cell.r, id, rotation);
        layout(&[t], cat, |_| false)
            .remove(&cell)
            .unwrap_or_else(|| panic!("{id} r{rotation} carries no water"))
    }

    /// `n + 1` points from the corner at 60 e - 30 degrees to the one at 60 e + 30 (the corner
    /// world edge `e` of `cell` shares with edge `e + 1`).
    fn along_edge(cell: Axial, e: u8, n: usize) -> Vec<Vec2> {
        let c = center(cell);
        let corner = |deg: f32| {
            let a = deg.to_radians();
            c + Vec2::new(a.cos(), a.sin()) * HEX_RADIUS
        };
        let mid = 60.0 * e as f32;
        let (a, b) = (corner(mid - 30.0), corner(mid + 30.0));
        (0..=n).map(|i| a.lerp(b, i as f32 / n as f32)).collect()
    }

    #[test]
    fn coastline_shapes_follow_the_edge_layout() {
        let cat = catalog();
        let shape = |id: &str, rot: u8| Coastline::of(cat.get(id).unwrap(), rot);
        let deg = |d: f32| Vec2::new(d.to_radians().cos(), d.to_radians().sin());
        for rot in 0..6u8 {
            let turn = 60.0 * rot as f32;
            // The straight beach keeps its seaward axis: between its ocean edges 0 and 1.
            let Some(Coastline::Straight { sea }) = shape("beach", rot) else {
                panic!("beach r{rot}");
            };
            assert!(sea.distance(deg(30.0 + turn)) < 1e-5, "beach r{rot} {sea}");
            // Headland: centred two apothems past its grass edge 3, land inside.
            let Some(Coastline::Arc {
                centre,
                land_inside: true,
            }) = shape("beach_outer", rot)
            else {
                panic!("beach_outer r{rot}");
            };
            assert!(centre.distance(deg(180.0 + turn) * 2.0 * APOTHEM) < 1e-3);
            // Bay: centred two apothems past its ocean edge 0, sea inside.
            let Some(Coastline::Arc {
                centre,
                land_inside: false,
            }) = shape("beach_inner", rot)
            else {
                panic!("beach_inner r{rot}");
            };
            assert!(centre.distance(deg(turn) * 2.0 * APOTHEM) < 1e-3);
        }
        for id in ["meadow", "ocean", "river_straight"] {
            assert_eq!(shape(id, 0), None, "{id}");
        }
    }

    /// The coast bed contract on every coast tile at every rotation, each tile alone: along each
    /// beach edge `s` runs linearly from -8 at the land corner (named by the socket's land side)
    /// to +8 at the sea corner, every ocean edge lies on the ocean bed and every grass edge at
    /// grass level, so beds meet ocean and grass neighbours without a step.
    #[test]
    fn coast_beds_meet_grass_and_ocean_and_run_down_each_beach_edge() {
        let cat = catalog();
        let mut edges = 0;
        for def in coast_tiles(&cat) {
            for rot in 0..6u8 {
                let cw = alone(&cat, Axial::ORIGIN, &def.id, rot);
                let (WaterKind::Coast(coast) | WaterKind::RiverMouth(coast)) = cw.kind else {
                    panic!("{} r{rot} is {:?}", def.id, cw.kind);
                };
                let f = cw.field();
                for e in 0..6u8 {
                    let socket = def.edge(e, rot);
                    for (i, p) in along_edge(Axial::ORIGIN, e, 32).into_iter().enumerate() {
                        let h = f.height(p.x, p.y);
                        let at = format!("{} r{rot} edge {e} point {i}", def.id);
                        match socket.kind {
                            SocketKind::Ocean => assert!((h - OCEAN_BED_Y).abs() < 1e-5, "{at}"),
                            SocketKind::Grass => assert!(h.abs() < 1e-6, "{at}: {h}"),
                            SocketKind::Beach => {
                                // Points run toward the corner shared with edge e + 1: the land
                                // corner when the land is on the right.
                                let t = -8.0 + 16.0 * i as f32 / 32.0;
                                let want = if socket.land == Some(Side::Right) {
                                    -t
                                } else {
                                    t
                                };
                                let s = coast.seaward(p);
                                assert!((s - want).abs() < 1e-3, "{at}: s {s}, want {want}");
                                assert!((h - coast_height(want)).abs() < 1e-3, "{at}: {h}");
                            }
                            // A mouth's River edge: river_straight's channel across the lane.
                            SocketKind::River => {
                                let o = (-8.0 + 16.0 * i as f32 / 32.0f32).abs();
                                assert!((h - river_height(o)).abs() < 1e-3, "{at}: {h}");
                            }
                            k => panic!("{at}: {k:?}"),
                        }
                    }
                    edges += 1;
                }
            }
        }
        assert_eq!(edges, 5 * 6 * 6);
    }

    /// Owner O1 proof: for every pair of coast tiles and every pair of rotations the rules accept
    /// as neighbours across a beach edge, the two tiles' beds agree along the whole shared edge,
    /// and so do water depth and shore distance, each tile's field computed from that tile alone.
    /// Every beach-to-beach pairing the rules refuse (the land sides do not oppose) is a mirrored
    /// seam whose beds step by the full coast drop.
    #[test]
    fn coast_beds_match_across_every_legal_beach_seam() {
        let cat = catalog();
        let sockets = cat.sockets();
        let coast = coast_tiles(&cat);
        let a_cell = Axial::ORIGIN;
        let (mut accepted, mut refused) = (0, 0);
        let mut pairs: BTreeSet<(&str, &str)> = BTreeSet::new();
        for a in &coast {
            for ra in 0..6u8 {
                let wa = alone(&cat, a_cell, &a.id, ra);
                for w in (0..6u8).filter(|&w| a.edge(w, ra).kind == SocketKind::Beach) {
                    let b_cell = a_cell.neighbor(w);
                    let back = opposite_edge(w);
                    for b in &coast {
                        for rb in (0..6u8).filter(|&rb| b.edge(back, rb).kind == SocketKind::Beach)
                        {
                            let wb = alone(&cat, b_cell, &b.id, rb);
                            let (fa, fb) = (wa.field(), wb.field());
                            let seam = format!("{} r{ra} | {} r{rb} across edge {w}", a.id, b.id);
                            let pts = along_edge(a_cell, w, 64);
                            let step = pts
                                .iter()
                                .map(|p| (fa.height(p.x, p.y) - fb.height(p.x, p.y)).abs())
                                .fold(0.0, f32::max);
                            if !sockets.edges_compatible(a.edge(w, ra), b.edge(back, rb)) {
                                assert!(step > 3.9, "{seam}: refused, yet beds step only {step}");
                                refused += 1;
                                continue;
                            }
                            assert!(step < 1e-4, "{seam}: beds step {step}");
                            for p in &pts {
                                let (sa, sb) = (fa.water(p.x, p.y), fb.water(p.x, p.y));
                                assert!((sa.depth - sb.depth).abs() < 1e-4, "{seam} at {p}");
                                assert!((sa.shore - sb.shore).abs() < 1e-3, "{seam} at {p}");
                            }
                            accepted += 1;
                            pairs.insert((&a.id, &b.id));
                        }
                    }
                }
            }
        }
        // 5 tiles (beach, headland, bay, both mouths) x 6 rotations x 2 beach edges, each facing
        // 10 (tile, rotation) beach edges, of which the 5 with the opposite land side are accepted.
        assert_eq!((accepted, refused), (300, 300));
        assert_eq!(
            pairs.len(),
            25,
            "every coast tile meets every coast tile: {pairs:?}"
        );
    }

    /// `docs/checkpoints/coast/coast.json` replayed on the rules with play's input semantics
    /// (`hex-app` `game.rs`): hovering a new slot or selecting a tile snaps to the first legal
    /// rotation, `rotate` cycles the legal rotations (one step when there are none), and `place`
    /// hovers its cell first. Exactly three placements are refused: ocean against a beach edge,
    /// grass against a beach edge, and a beach edge against grass. The rest close an island of
    /// straight, headland and bay tiles under many rotations, ringed by ocean, with no beach edge
    /// left open and matching beds across every beach seam.
    #[test]
    fn coast_proof_script_closes_an_island() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/checkpoints/coast/coast.json"
        );
        let text = std::fs::read_to_string(path).unwrap();
        let script: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
        let cat = catalog();
        let mut world = World::new(1);
        let (mut selected, mut hovered, mut rotation) = ("meadow".to_owned(), None, 0u8);
        let snap = |world: &World, selected: &str, hovered: Option<Axial>, rotation: &mut u8| {
            if let Some(c) = hovered.filter(|&c| world.is_slot(c))
                && let Some(k) = world.auto_rotate(&cat, c, cat.get(selected).unwrap())
            {
                *rotation = k;
            }
        };
        let mut refused = vec![];
        for cmd in &script {
            let at = || {
                let n = |k: &str| i32::try_from(cmd[k].as_i64().unwrap()).unwrap();
                Axial::new(n("q"), n("r"))
            };
            match cmd["cmd"].as_str().unwrap() {
                "select_tile" => {
                    selected = cmd["id"].as_str().unwrap().to_owned();
                    snap(&world, &selected, hovered, &mut rotation);
                }
                "hover" | "place" if hovered != Some(at()) => {
                    hovered = Some(at());
                    snap(&world, &selected, hovered, &mut rotation);
                    if cmd["cmd"] == "place"
                        && let Err(e) = world.place(&cat, at(), &selected, rotation)
                    {
                        refused.push((at(), selected.clone(), e));
                    }
                }
                "place" => {
                    if let Err(e) = world.place(&cat, at(), &selected, rotation) {
                        refused.push((at(), selected.clone(), e));
                    }
                }
                "rotate" => {
                    let dir = match cmd["dir"].as_str().unwrap() {
                        "cw" => hex_core::RotateDir::Forward,
                        _ => hex_core::RotateDir::Back,
                    };
                    let def = cat.get(&selected).unwrap();
                    rotation = hovered
                        .filter(|&c| world.is_slot(c))
                        .and_then(|c| world.cycle_rotation(&cat, c, def, rotation, dir))
                        .unwrap_or(match dir {
                            hex_core::RotateDir::Forward => (rotation + 1) % 6,
                            hex_core::RotateDir::Back => (rotation + 5) % 6,
                        });
                }
                _ => {}
            }
        }
        let refusal = |q, r, id: &str, edges: Vec<u8>| {
            (
                Axial::new(q, r),
                id.to_owned(),
                hex_core::PlaceError::Incompatible(edges),
            )
        };
        assert_eq!(
            refused,
            [
                refusal(-1, -1, "ocean", vec![0]),
                refusal(1, -1, "meadow", vec![3]),
                refusal(1, 0, "beach", vec![3]),
            ]
        );
        let tiles: Vec<PlayTile> = world
            .tiles()
            .iter()
            .map(|(c, t)| PlayTile {
                cell: *c,
                tile: t.def.clone(),
                rotation: t.rotation,
            })
            .collect();
        let coast: Vec<&PlayTile> = tiles
            .iter()
            .filter(|t| t.tile.starts_with("beach"))
            .collect();
        let count = |id: &str| coast.iter().filter(|t| t.tile == id).count();
        assert_eq!(
            (count("beach"), count("beach_outer"), count("beach_inner")),
            (2, 8, 2)
        );
        let placements: BTreeSet<(&str, u8)> = coast
            .iter()
            .map(|t| (t.tile.as_str(), t.rotation))
            .collect();
        assert_eq!(placements.len(), 10, "{placements:?}");
        assert_eq!(world.tiles().len(), 4 + 12 + 18);
        let lay = layout(&tiles, &cat, |_| false);
        let mut seams = 0;
        for t in &coast {
            let cw = &lay[&t.cell];
            assert_eq!(cw.exposed, Vec::<u8>::new(), "{t:?}: open water edge");
            let def = cat.get(&t.tile).unwrap();
            for w in (0..6u8).filter(|&w| def.edge(w, t.rotation).kind == SocketKind::Beach) {
                let other = &lay[&t.cell.neighbor(w)];
                for p in along_edge(t.cell, w, 32) {
                    let (ha, hb) = (cw.field().height(p.x, p.y), other.field().height(p.x, p.y));
                    assert!(
                        (ha - hb).abs() < 1e-4,
                        "{t:?} edge {w} at {p}: {ha} vs {hb}"
                    );
                }
                seams += 1;
            }
        }
        assert_eq!(
            seams,
            2 * 12,
            "every coast tile joins the next on both beach edges"
        );
    }

    /// The engine's mouth bed is the kit's pinned profile (`scripts/blender/kit/_mouth_profile.py`
    /// `SPOT`, Blender (x, y) = runtime (x, -z)) on both handednesses.
    #[test]
    fn mouth_bed_matches_the_kit_profile() {
        let cat = catalog();
        let spots = [
            ("river_mouth", -13.856406, 0.0, -2.95),
            ("river_mouth", -13.856406, 4.0, -1.3994),
            ("river_mouth", -6.0, 0.0, -2.7575),
            ("river_mouth", -3.0, 4.0, -1.6653),
            ("river_mouth", 0.0, 0.0, -1.9773),
            ("river_mouth", 3.4641, 0.0, -1.76),
            ("river_mouth", 8.0, 0.0, -3.7353),
            ("river_mouth", 0.0, 8.0, -0.35),
            ("river_mouth_l", -6.928203, 12.0, -2.95),
            ("river_mouth_l", 0.0, 0.0, -1.9773),
        ];
        for (id, x, y, want) in spots {
            let m = Mouth::of(cat.get(id).unwrap(), 0).unwrap();
            let p = Vec2::new(x, -y);
            assert!(
                (m.height(p) - want).abs() < 2e-3,
                "{id} {p}: {}",
                m.height(p)
            );
            let cw = alone(&cat, Axial::ORIGIN, id, 0);
            assert!(
                (cw.field().height(p.x, p.y) - m.height(p)).abs() < 1e-4,
                "{id} field {p}"
            );
        }
        // The mirror: river_mouth_l is river_mouth reflected across the seaward axis.
        let (r, l) = (
            Mouth::of(cat.get("river_mouth").unwrap(), 0).unwrap(),
            Mouth::of(cat.get("river_mouth_l").unwrap(), 0).unwrap(),
        );
        let axis = Vec2::new(30f32.to_radians().cos(), 30f32.to_radians().sin());
        for i in 0..400 {
            let p = Vec2::new((i % 20) as f32 * 1.4 - 13.0, (i / 20) as f32 * 1.4 - 13.0);
            let q = 2.0 * p.dot(axis) * axis - p;
            assert!((r.height(p) - l.height(q)).abs() < 1e-4, "{p} vs {q}");
        }
    }

    /// The river inputs meet their river neighbours: a mouth's River edge against river_straight,
    /// a spring's against river_straight. Shore distance agrees wherever the swash acts (river
    /// tiles report INLAND and the mouth its seaward s; swash weighs (1 - river), and the river
    /// weight is 1 over all water at the seam).
    #[test]
    fn mouth_and_spring_meet_their_river_neighbours() {
        let cat = catalog();
        let o = Axial::ORIGIN;
        for (id, rot, e) in [
            ("river_mouth", 0u8, 3u8),
            ("river_mouth", 2, 5),
            ("river_mouth_l", 1, 5),
            ("river_spring", 0, 0),
            ("river_spring", 4, 4),
        ] {
            let n = o.neighbor(e);
            let back = opposite_edge(e);
            let rn = (0..6u8).find(|&r| world_edge(0, r) == back).unwrap();
            let tiles = [
                tile(o.q, o.r, id, rot),
                tile(n.q, n.r, "river_straight", rn),
            ];
            let lay = layout(&tiles, &cat, |_| false);
            assert!(!lay[&o].exposed.contains(&e), "{id} r{rot}: seam exposed");
            let (fa, fb) = (lay[&o].field(), lay[&n].field());
            let mid = (center(o) + center(n)) * 0.5;
            let along = edge_dir(e).perp();
            for i in 0..=32 {
                let p = mid + along * (-8.0 + 0.5 * i as f32);
                let (wa, wb) = (fa.water(p.x, p.y), fb.water(p.x, p.y));
                let at = format!("{id} r{rot} at {p}");
                assert!(
                    (fa.height(p.x, p.y) - fb.height(p.x, p.y)).abs() < 1e-3,
                    "{at}"
                );
                assert!((wa.depth - wb.depth).abs() < 1e-3, "{at}");
                assert!((wa.river - wb.river).abs() < 1e-4, "{at}");
                assert!(
                    wa.flow.distance(wb.flow) < 1e-3,
                    "{at}: {} vs {}",
                    wa.flow,
                    wb.flow
                );
                if wa.depth > 0.0 && wa.river < 1.0 {
                    assert!((wa.shore - wb.shore).abs() < 1e-3, "{at}");
                }
            }
        }
    }

    /// The flow rule (water contract §6): away from a spring, into a mouth, spring to mouth; two
    /// springs or two mouths start at the lower cell.
    #[test]
    fn river_chains_flow_from_springs_into_mouths() {
        let cat = catalog();
        let o = Axial::ORIGIN;
        let (e0, e3) = (o.neighbor(0), o.neighbor(3));
        let ends = |tiles: &[PlayTile]| -> Vec<(Axial, RiverEnd, RiverEnd)> {
            oriented_rivers(tiles, &cat)
                .into_iter()
                .map(|(k, p)| (k.0, p.up, p.down))
                .collect()
        };
        use RiverEnd::{Edge, Mouth as M, Spring as S};
        // Spring, then an open river end.
        let got = ends(&[
            tile(o.q, o.r, "river_spring", 0),
            tile(e0.q, e0.r, "river_straight", 0),
        ]);
        assert!(
            got.contains(&(o, S, Edge(0))) && got.contains(&(e0, Edge(3), Edge(0))),
            "{got:?}"
        );
        // An open river end, then a mouth.
        let got = ends(&[
            tile(o.q, o.r, "river_mouth", 0),
            tile(e3.q, e3.r, "river_straight", 0),
        ]);
        assert!(
            got.contains(&(e3, Edge(3), Edge(0))) && got.contains(&(o, Edge(3), M)),
            "{got:?}"
        );
        // Spring to mouth, whichever cell is lower.
        let got = ends(&[
            tile(o.q, o.r, "river_spring", 0),
            tile(e0.q, e0.r, "river_mouth", 0),
        ]);
        assert!(
            got.contains(&(o, S, Edge(0))) && got.contains(&(e0, Edge(3), M)),
            "{got:?}"
        );
        let got = ends(&[
            tile(o.q, o.r, "river_mouth", 0),
            tile(e3.q, e3.r, "river_spring", 0),
        ]);
        assert!(
            got.contains(&(e3, S, Edge(0))) && got.contains(&(o, Edge(3), M)),
            "{got:?}"
        );
        // Two springs and two mouths: deterministic, from the lower cell.
        for (a, b) in [("river_spring", 3u8), ("river_mouth", 3u8)] {
            let (cell, rot0) = if a == "river_spring" {
                (e0, 0u8)
            } else {
                (e3, 0u8)
            };
            let tiles = [tile(o.q, o.r, a, rot0), tile(cell.q, cell.r, a, b)];
            let low = o.min(cell);
            let r = oriented_rivers(&tiles, &cat);
            let first = r.iter().find(|(k, _)| k.0 == low).unwrap().1;
            assert!(matches!(first.up, S | M), "{a}: {:?}", first.up);
            let other = r.iter().find(|(k, _)| k.0 != low).unwrap().1;
            assert!(matches!(other.down, S | M), "{a}: {:?}", other.down);
        }
    }

    /// The fork's centre node joins three River segments, so it is a junction: the tile is a plain
    /// river tile with an arm on each River edge, not a spring, and alone it is open on all three.
    #[test]
    fn the_fork_is_a_river_tile_with_three_arms() {
        let cat = catalog();
        let def = cat.get("river_fork").unwrap();
        for rot in 0..6u8 {
            assert_eq!(water_kind(def, rot), Some(WaterKind::River), "r{rot}");
            let cw = alone(&cat, Axial::ORIGIN, "river_fork", rot);
            assert_eq!(cw.own_paths, 3);
            assert!(cw.paths.iter().all(RiverPath::at_junction));
            let mut want: Vec<u8> = [0, 2, 4].map(|e| world_edge(e, rot)).to_vec();
            want.sort();
            assert_eq!(cw.exposed, want, "r{rot}");
        }
        assert!(Mouth::of(def, 0).is_none());
        // The spring keeps its own node end.
        let spring = cat.get("river_spring").unwrap();
        assert!(matches!(water_kind(spring, 0), Some(WaterKind::Spring(_))));
    }

    /// The engine's fork bed is the kit's (`scripts/blender/kit/_river_fork.py` `_check` spots,
    /// Blender (x, y) = runtime (x, -z)): the channel profile at the distance to the nearest arm.
    #[test]
    fn fork_bed_matches_the_kit_profile() {
        let cat = catalog();
        let cw = alone(&cat, Axial::ORIGIN, "river_fork", 0);
        let f = cw.field();
        let sin60 = 60f32.to_radians().sin();
        let (wb, bb) = (4.0 / sin60, 6.83 / sin60);
        for (x, y, want) in [
            (0.0, 0.0, -2.95),
            (APOTHEM, 4.0, -1.40),
            (APOTHEM, 0.0, -2.95),
            (0.5 * wb, sin60 * wb, -1.40),
            (0.5 * bb, sin60 * bb, 0.0),
            (-APOTHEM, 0.0, 0.0),
        ] {
            let h = f.height(x, -y);
            assert!((h - want).abs() < 6e-3, "({x}, {y}): {h} vs {want}");
        }
        // Three-fold: each arm carries the same profile.
        for i in 0..40 {
            let p = Vec2::new((i % 8) as f32 * 1.6 - 1.0, (i / 8) as f32 * 1.5 - 3.0);
            let h = f.height(p.x, p.y);
            for k in [2u8, 4] {
                let q = turn(k, p);
                assert!((f.height(q.x, q.y) - h).abs() < 1e-4, "{p} turned {k}");
            }
        }
    }

    /// The fork flow rule (water contract §6, junctions): out through local edge 0 when nothing
    /// decides; toward a mouth (a confluence); away from one spring (a split); two springs join.
    #[test]
    fn fork_arms_flow_by_the_junction_rule() {
        let cat = catalog();
        let o = Axial::ORIGIN;
        use RiverEnd::{Edge, Junction as J, Mouth as M, Spring as S};
        let arms = |tiles: &[PlayTile]| -> BTreeMap<u8, (RiverEnd, RiverEnd)> {
            oriented_rivers(tiles, &cat)
                .into_iter()
                .filter(|(k, _)| k.0 == o)
                .map(|(_, p)| {
                    let e = p.up.edge().or(p.down.edge()).unwrap();
                    (e, (p.up, p.down))
                })
                .collect()
        };
        // Spring or mouth tile across world edge e of the fork, its River edge (local 0 or 3)
        // facing back.
        let across = |id: &str, local: u8, e: u8| {
            let n = o.neighbor(e);
            let r = (0..6u8)
                .find(|&r| world_edge(local, r) == opposite_edge(e))
                .unwrap();
            tile(n.q, n.r, id, r)
        };
        let out = |e: u8| (J, Edge(e));
        let inn = |e: u8| (Edge(e), J);
        // Alone: a confluence leaving through local edge 0, whatever the rotation.
        for rot in 0..6u8 {
            let got = arms(&[tile(0, 0, "river_fork", rot)]);
            let e0 = world_edge(0, rot);
            for (e, ends) in &got {
                assert_eq!(*ends, if *e == e0 { out(*e) } else { inn(*e) }, "r{rot}");
            }
        }
        // A mouth past arm 2: arm 2 leaves, 0 and 4 join it.
        let tiles = [tile(0, 0, "river_fork", 0), across("river_mouth", 3, 2)];
        assert_eq!(
            arms(&tiles),
            BTreeMap::from([(0, inn(0)), (2, out(2)), (4, inn(4))])
        );
        let r = oriented_rivers(&tiles, &cat);
        assert!(r.values().any(|p| (p.up, p.down) == (Edge(5), M)));
        // One spring past arm 4: arm 4 feeds the fork, which splits into 0 and 2.
        let tiles = [tile(0, 0, "river_fork", 0), across("river_spring", 0, 4)];
        assert_eq!(
            arms(&tiles),
            BTreeMap::from([(0, out(0)), (2, out(2)), (4, inn(4))])
        );
        let r = oriented_rivers(&tiles, &cat);
        assert!(r.values().any(|p| (p.up, p.down) == (S, Edge(1))));
        // Springs past arms 0 and 2: they join and leave through 4.
        let tiles = [
            tile(0, 0, "river_fork", 0),
            across("river_spring", 0, 0),
            across("river_spring", 0, 2),
        ];
        assert_eq!(
            arms(&tiles),
            BTreeMap::from([(0, inn(0)), (2, inn(2)), (4, out(4))])
        );
        // A spring and a mouth: the arm toward the mouth leaves, the other two (the spring's and
        // the open one) join it, through a river tile between the fork and the mouth.
        let mid = o.neighbor(0);
        let tiles = [
            tile(0, 0, "river_fork", 0),
            tile(mid.q, mid.r, "river_straight", 0),
            {
                let n = mid.neighbor(0);
                tile(n.q, n.r, "river_mouth", 0)
            },
            across("river_spring", 0, 2),
        ];
        assert_eq!(
            arms(&tiles),
            BTreeMap::from([(0, out(0)), (2, inn(2)), (4, inn(4))])
        );
    }

    /// The fork meets river_straight and river_bend neighbours on all three arms with the same bed,
    /// depth, river weight and flow along every shared edge, and its blended flow turns across the
    /// bisectors instead of switching there.
    #[test]
    fn fork_meets_its_river_neighbours_and_turns_its_flow() {
        let cat = catalog();
        let o = Axial::ORIGIN;
        for rot in [0u8, 1, 3] {
            let mut tiles = vec![tile(0, 0, "river_fork", rot)];
            let edges: Vec<u8> = [0, 2, 4].map(|k| world_edge(k, rot)).to_vec();
            for (i, &e) in edges.iter().enumerate() {
                let n = o.neighbor(e);
                let back = opposite_edge(e);
                let (id, r) = if i == 1 {
                    (
                        "river_bend",
                        (0..6u8).find(|&r| world_edge(0, r) == back).unwrap(),
                    )
                } else {
                    (
                        "river_straight",
                        (0..6u8).find(|&r| world_edge(0, r) == back).unwrap(),
                    )
                };
                tiles.push(tile(n.q, n.r, id, r));
            }
            let lay = layout(&tiles, &cat, |_| false);
            assert!(lay[&o].exposed.is_empty(), "r{rot}: {:?}", lay[&o].exposed);
            let fa = lay[&o].field();
            for &e in &edges {
                let n = o.neighbor(e);
                let fb = lay[&n].field();
                let mid = (center(o) + center(n)) * 0.5;
                let along = edge_dir(e).perp();
                for i in 0..=32 {
                    let p = mid + along * (-8.0 + 0.5 * i as f32);
                    let (wa, wb) = (fa.water(p.x, p.y), fb.water(p.x, p.y));
                    let at = format!("r{rot} edge {e} at {p}");
                    assert!(
                        (fa.height(p.x, p.y) - fb.height(p.x, p.y)).abs() < 1e-3,
                        "{at}"
                    );
                    assert!((wa.depth - wb.depth).abs() < 1e-3, "{at}");
                    assert!((wa.river - wb.river).abs() < 1e-4, "{at}");
                    assert!(
                        wa.flow.distance(wb.flow) < 1e-3,
                        "{at}: {} vs {}",
                        wa.flow,
                        wb.flow
                    );
                }
            }
            // Across each bisector in the water the flow keeps its direction within 15 degrees.
            for k in 0..3 {
                let bis = edge_dir(world_edge(2 * k + 1, rot));
                for r in [1.0f32, 2.0, 3.0, 4.0] {
                    let p = bis * r;
                    let side = bis.perp() * 0.25;
                    let (a, b) = (
                        fa.water(p.x + side.x, p.y + side.y),
                        fa.water(p.x - side.x, p.y - side.y),
                    );
                    assert!(a.flow.length() > 0.0 && b.flow.length() > 0.0);
                    let cos = a.flow.normalize().dot(b.flow.normalize());
                    assert!(
                        cos > 15f32.to_radians().cos(),
                        "r{rot} bisector {k} r {r}: {} vs {}",
                        a.flow,
                        b.flow
                    );
                }
            }
        }
    }

    /// The fish band's room along a mouth's centre line, from its River edge out to sea (water
    /// contract §3a, Fish).
    #[test]
    #[ignore = "prints the measured fish room across the mouth fan"]
    fn print_fish_room_along_the_mouth() {
        use crate::boids::plan::{FishWorld, SPAWN_ROOM, band_at};
        let cat = catalog();
        let o = Axial::ORIGIN;
        let (r, m) = (o.neighbor(3), o.neighbor(0));
        let sea = m.neighbor(0);
        let tiles = [
            tile(r.q, r.r, "river_straight", 0),
            tile(o.q, o.r, "river_straight", 0),
            tile(m.q, m.r, "river_mouth", 0),
            tile(sea.q, sea.r, "ocean", 0),
        ];
        let w = FishWorld::new(&tiles, &cat);
        let c = center(m);
        let sea_dir = Vec2::new(30f32.to_radians().cos(), 30f32.to_radians().sin());
        let (mut lo, mut closed) = (f32::MAX, vec![]);
        for i in 0..=280 {
            let x = -APOTHEM + i as f32 * 0.1;
            let p = c + Vec2::new(x, 0.0);
            let s = Vec2::new(x, 0.0).dot(sea_dir);
            let (band, baked) = (band_at(&w.cells, p), w.field.sample(p));
            let room = band[2].min(baked[2]);
            lo = lo.min(room);
            if room < 0.0 {
                closed.push(s);
            }
            if i % 20 == 0 {
                println!(
                    "s {s:6.2}: depth {:.3} band {:.3}..{:.3} under the surface, room analytic {:.3} baked {:.3}",
                    WATER_Y - lay_height(&w, p),
                    WATER_Y - band[0],
                    WATER_Y - band[1],
                    band[2],
                    baked[2]
                );
            }
        }
        println!(
            "min room {lo:.3}; closed s {:?}..{:?}; SPAWN_ROOM {SPAWN_ROOM}",
            closed.first(),
            closed.last()
        );
    }

    /// Fish skim a river mouth's shallow fan (owner, 2026-10-14; the fan is not deepened): along
    /// both mouth handednesses' centre lines, from the river tile through the mouth into the
    /// ocean, the band stays open with the trout inside the water, the room never dips under the
    /// river channel's (so the shore turn does not send fish back), and a fish swimming the line
    /// is never turned back.
    #[test]
    fn the_fish_band_is_open_along_both_mouth_centre_lines() {
        use crate::boids::plan::{
            FishWorld, TROUT_BELLY, TROUT_TOP, band_at, cell_at, confine, river_room,
        };
        let cat = catalog();
        let tiles: Vec<PlayTile> = MOUTH_PROOF
            .iter()
            .map(|&(q, r, id, rotation)| tile(q, r, id, rotation))
            .collect();
        let w = FishWorld::new(&tiles, &cat);
        let floor = river_room() - 0.02;
        for (q, r, id) in [(3, 0, "river_mouth"), (-3, 0, "river_mouth_l")] {
            let cell = Axial::new(q, r);
            let rot = tiles.iter().find(|t| t.cell == cell).unwrap().rotation;
            let m = Mouth::of(cat.get(id).unwrap(), rot).unwrap();
            let start = center(cell) + m.start;
            let (from, to) = (-3.0, 2.0 * APOTHEM + 5.0);
            assert!(w.cells[&cell_at(start + m.dir * to)].kind == WaterKind::Ocean);
            let mut lo = f32::MAX;
            for i in 0..=((to - from) * 10.0) as usize {
                let p = start + m.dir * (from + i as f32 * 0.1);
                let (a, g) = (band_at(&w.cells, p), w.field.sample(p));
                lo = lo.min(a[2]).min(g[2]);
                assert!(a[2] >= floor && g[2] >= floor, "{id} at {p}: {a:?} {g:?}");
                let bed = lay_height(&w, p);
                assert!(a[0] <= a[1], "{id} at {p}: {a:?}");
                assert!(
                    a[1] + TROUT_TOP < WATER_Y,
                    "{id} at {p}: fin through the surface"
                );
                assert!(
                    a[0] - TROUT_BELLY > bed,
                    "{id} at {p}: belly in the bed {bed}"
                );
            }
            println!("{id}: min room along the centre line {lo:.3}");
            // Swim the line at 1 m/s: never stopped and turned.
            let dt = 1.0 / 30.0;
            let v = Vec3::new(m.dir.x, 0.0, m.dir.y);
            let a0 = start + m.dir * from;
            let mut pos = Vec3::new(a0.x, WATER_Y - 0.4, a0.y);
            for _ in 0..((to - from) * 30.0) as usize {
                let (np, nv) = confine(&w.field, pos, v, dt);
                assert!(nv.xz().dot(m.dir) > 0.99, "{id}: turned back at {pos}");
                pos = np;
            }
            assert!(
                w.cells[&cell_at(pos.xz())].kind == WaterKind::Ocean,
                "{id}: reached {pos}"
            );
        }
    }

    fn lay_height(w: &crate::boids::plan::FishWorld, p: Vec2) -> f32 {
        let cell = w
            .cells
            .keys()
            .copied()
            .min_by(|a, b| center(*a).distance(p).total_cmp(&center(*b).distance(p)))
            .unwrap();
        w.cells[&cell].field().height(p.x, p.y)
    }

    /// Fish (Q5): a river chain from a spring through a mouth into the sea is one fish region.
    #[test]
    fn spring_river_mouth_and_sea_form_one_fish_region() {
        let cat = catalog();
        let o = Axial::ORIGIN;
        let (s, m) = (o.neighbor(3), o.neighbor(0));
        let sea = m.neighbor(0);
        let tiles = [
            tile(s.q, s.r, "river_spring", 0),
            tile(o.q, o.r, "river_straight", 0),
            tile(m.q, m.r, "river_mouth", 0),
            tile(sea.q, sea.r, "ocean", 0),
        ];
        let w = crate::boids::plan::FishWorld::new(&tiles, &cat);
        assert_eq!(w.regions.len(), 1, "{:?}", w.regions);
        assert_eq!(w.regions[0].cells.len(), 4);
        assert!(!w.regions[0].river, "the joined region holds sea water");
    }

    /// The river-mouth evidence layout (`docs/checkpoints/river-mouth/river-mouth.json`): two
    /// chains from a spring through a straight and a bend into the sea, one through each mouth
    /// handedness, with beaches continuing the coast. (q, r, tile, rotation).
    const MOUTH_PROOF: [(i32, i32, &str, u8); 18] = [
        (1, 0, "meadow", 0),
        (2, 0, "river_bend", 0),
        (3, 0, "river_mouth", 0),
        (4, 0, "ocean", 0),
        (3, 1, "ocean", 0),
        (4, -1, "beach", 0),
        (2, 1, "beach", 0),
        (1, 1, "river_straight", 2),
        (0, 2, "river_spring", 5),
        (5, -1, "ocean", 0),
        (-1, 0, "meadow", 0),
        (-2, 0, "meadow", 0),
        (-3, 0, "river_mouth_l", 3),
        (-4, 0, "ocean", 0),
        (-3, -1, "ocean", 0),
        (-3, 1, "river_straight", 1),
        (-3, 2, "river_bend", 2),
        (-4, 3, "river_spring", 5),
    ];

    /// Prints the script commands that place [`MOUTH_PROOF`] with play's input semantics (hover
    /// snaps to the first legal rotation, `rotate` steps through the distinct legal ones).
    #[test]
    #[ignore = "generates docs/checkpoints/river-mouth/river-mouth.json placements"]
    fn print_mouth_proof_commands() {
        let cat = catalog();
        let mut world = World::new(1);
        for (q, r, id, want) in MOUTH_PROOF {
            let def = cat.get(id).unwrap();
            let cell = Axial::new(q, r);
            let legal = world.legal_rotations(&cat, cell, def);
            assert!(legal.contains(&want), "{id} at ({q}, {r}): legal {legal:?}");
            let mut rot = world.auto_rotate(&cat, cell, def).unwrap();
            let mut steps = 0;
            while !def.same_orientation(rot, want) {
                rot = world
                    .cycle_rotation(&cat, cell, def, rot, hex_core::RotateDir::Forward)
                    .unwrap();
                steps += 1;
                assert!(steps < 7, "{id} at ({q}, {r})");
            }
            println!("  {{\"cmd\": \"select_tile\", \"id\": \"{id}\"}},");
            println!("  {{\"cmd\": \"hover\", \"q\": {q}, \"r\": {r}}},");
            for _ in 0..steps {
                println!("  {{\"cmd\": \"rotate\", \"dir\": \"cw\"}},");
            }
            println!("  {{\"cmd\": \"place\", \"q\": {q}, \"r\": {r}}},");
            world.place(&cat, cell, id, rot).unwrap();
        }
    }

    /// `docs/checkpoints/river-mouth/river-mouth.json` replayed on the rules with play's input
    /// semantics places every tile of [`MOUTH_PROOF`] in its orientation, refuses nothing, and
    /// gives two chains that flow from a spring into a mouth, each one fish region with the sea.
    #[test]
    fn mouth_proof_script_places_both_chains() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/checkpoints/river-mouth/river-mouth.json"
        );
        let text = std::fs::read_to_string(path).unwrap();
        let script: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
        let cat = catalog();
        let mut world = World::new(1);
        let (mut selected, mut hovered, mut rotation) = ("meadow".to_owned(), None, 0u8);
        let n = |cmd: &serde_json::Value, k: &str| i32::try_from(cmd[k].as_i64().unwrap()).unwrap();
        for cmd in &script {
            match cmd["cmd"].as_str().unwrap() {
                "select_tile" => selected = cmd["id"].as_str().unwrap().to_owned(),
                "hover" => {
                    let c = Axial::new(n(cmd, "q"), n(cmd, "r"));
                    hovered = Some(c);
                    if let Some(k) = world.auto_rotate(&cat, c, cat.get(&selected).unwrap()) {
                        rotation = k;
                    }
                }
                "rotate" => {
                    let c = hovered.unwrap();
                    let def = cat.get(&selected).unwrap();
                    rotation = world
                        .cycle_rotation(&cat, c, def, rotation, hex_core::RotateDir::Forward)
                        .unwrap();
                }
                "place" => {
                    let c = Axial::new(n(cmd, "q"), n(cmd, "r"));
                    assert_eq!(hovered, Some(c), "place without hover at {c:?}");
                    world.place(&cat, c, &selected, rotation).unwrap();
                }
                _ => {}
            }
        }
        for (q, r, id, want) in MOUTH_PROOF {
            let t = world.tile(Axial::new(q, r)).unwrap();
            let def = cat.get(id).unwrap();
            assert_eq!(t.def, id);
            assert!(
                def.same_orientation(t.rotation, want),
                "{id} at ({q}, {r}): r{}",
                t.rotation
            );
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
        let lay = layout(&tiles, &cat, |_| false);
        for (spring, mouth) in [((0, 2), (3, 0)), ((-4, 3), (-3, 0))] {
            let s = &lay[&Axial::new(spring.0, spring.1)].paths[0];
            let m = &lay[&Axial::new(mouth.0, mouth.1)].paths[0];
            assert_eq!((s.up, m.down), (RiverEnd::Spring, RiverEnd::Mouth));
        }
        let fish = crate::boids::plan::FishWorld::new(&tiles, &cat);
        for cell in [(0, 2), (-4, 3)] {
            let region = fish
                .regions
                .iter()
                .find(|g| g.cells.contains(&Axial::new(cell.0, cell.1)))
                .unwrap();
            assert!(!region.river, "{cell:?}: the spring's water joins the sea");
        }
    }

    /// `script` replayed on the rules with play's input semantics (hover snaps to the first legal
    /// rotation, `rotate` steps through the distinct legal ones); every placement, bulldoze, undo
    /// and redo must succeed.
    fn replay(script: &[serde_json::Value], cat: &TileCatalog) -> World {
        let mut world = World::new(1);
        let (mut selected, mut hovered, mut rotation) = ("meadow".to_owned(), None, 0u8);
        let n = |cmd: &serde_json::Value, k: &str| i32::try_from(cmd[k].as_i64().unwrap()).unwrap();
        for cmd in script {
            let at = || Axial::new(n(cmd, "q"), n(cmd, "r"));
            match cmd["cmd"].as_str().unwrap() {
                "select_tile" => selected = cmd["id"].as_str().unwrap().to_owned(),
                "hover" => {
                    hovered = Some(at());
                    if let Some(k) = world.auto_rotate(cat, at(), cat.get(&selected).unwrap()) {
                        rotation = k;
                    }
                }
                "rotate" => {
                    let def = cat.get(&selected).unwrap();
                    rotation = world
                        .cycle_rotation(
                            cat,
                            hovered.unwrap(),
                            def,
                            rotation,
                            hex_core::RotateDir::Forward,
                        )
                        .unwrap();
                }
                "place" => {
                    if hovered != Some(at()) {
                        hovered = Some(at());
                        rotation = world
                            .auto_rotate(cat, at(), cat.get(&selected).unwrap())
                            .unwrap_or_else(|| {
                                panic!("{selected} has no legal rotation at {:?}", at())
                            });
                    }
                    world
                        .place(cat, at(), &selected, rotation)
                        .unwrap_or_else(|e| panic!("{selected} at {:?}: {e:?}", at()));
                }
                "bulldoze" => {
                    world.bulldoze(at()).unwrap();
                }
                "undo" => assert!(world.undo()),
                "redo" => assert!(world.redo()),
                _ => {}
            }
        }
        world
    }

    /// The boids checkpoint's additions (owner, 2026-10-14): nine more meadows (birds scale with
    /// land) and, after the redo, a chain spring, straight, mouth into the coast's ocean. Prints
    /// the commands that place them; `boids_script_builds_its_world` checks the script.
    #[test]
    #[ignore = "generates docs/checkpoints/boids/boids.json placements"]
    fn print_boids_additions() {
        let cat = catalog();
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/checkpoints/boids/boids.json"
        );
        let script: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let mut world = replay(&script, &cat);
        let meadow = cat.get("meadow").unwrap();
        let candidates = [
            (1, -2),
            (1, -3),
            (2, -3),
            (0, -3),
            (-1, -2),
            (-1, -1),
            (2, -2),
            (-1, -3),
            (0, -4),
            (1, -4),
            (2, -4),
            (3, -3),
        ];
        let mut placed = 0;
        for (q, r) in candidates {
            let c = Axial::new(q, r);
            if placed < 9 && world.tile(c).is_none() && world.auto_rotate(&cat, c, meadow).is_some()
            {
                world.place(&cat, c, "meadow", 0).unwrap();
                println!("  {{\"cmd\": \"place\", \"q\": {q}, \"r\": {r}}},");
                placed += 1;
            }
        }
        println!("placed {placed} meadows");
        let want = |id: &str, faces: &[(u8, SocketKind)]| -> u8 {
            let def = cat.get(id).unwrap();
            (0..6u8)
                .find(|&k| faces.iter().all(|&(e, kind)| def.edge(e, k).kind == kind))
                .unwrap()
        };
        let chain = [
            (
                5,
                -1,
                "river_mouth",
                want(
                    "river_mouth",
                    &[
                        (2, SocketKind::Ocean),
                        (3, SocketKind::Ocean),
                        (5, SocketKind::River),
                    ],
                ),
            ),
            (
                6,
                -2,
                "river_straight",
                want(
                    "river_straight",
                    &[(2, SocketKind::River), (5, SocketKind::River)],
                ),
            ),
            (
                7,
                -3,
                "river_spring",
                want("river_spring", &[(2, SocketKind::River)]),
            ),
        ];
        for (q, r, id, rot) in chain {
            let def = cat.get(id).unwrap();
            let cell = Axial::new(q, r);
            let legal = world.legal_rotations(&cat, cell, def);
            assert!(
                legal.contains(&rot),
                "{id} at ({q}, {r}) r{rot}: legal {legal:?}"
            );
            let mut k = world.auto_rotate(&cat, cell, def).unwrap();
            let mut steps = 0;
            while !def.same_orientation(k, rot) {
                k = world
                    .cycle_rotation(&cat, cell, def, k, hex_core::RotateDir::Forward)
                    .unwrap();
                steps += 1;
                assert!(steps < 7, "{id}");
            }
            println!("  {{\"cmd\": \"select_tile\", \"id\": \"{id}\"}},");
            println!("  {{\"cmd\": \"hover\", \"q\": {q}, \"r\": {r}}},");
            for _ in 0..steps {
                println!("  {{\"cmd\": \"rotate\", \"dir\": \"cw\"}},");
            }
            println!("  {{\"cmd\": \"place\", \"q\": {q}, \"r\": {r}}},");
            world.place(&cat, cell, id, k).unwrap();
        }
    }

    /// `docs/checkpoints/boids/boids.json` replayed on the rules places every tile, bulldozes,
    /// undoes and redoes as scripted, and ends with 12 land tiles (three flocks, as before the
    /// bulldoze's 13) and two spring, straight and mouth chains (river_mouth, then the mirrored
    /// river_mouth_l) joined to the coast's sea as one fish region.
    #[test]
    fn boids_script_builds_its_world() {
        let cat = catalog();
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/checkpoints/boids/boids.json"
        );
        let script: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let world = replay(&script, &cat);
        let tiles: Vec<PlayTile> = world
            .tiles()
            .iter()
            .map(|(c, t)| PlayTile {
                cell: *c,
                tile: t.def.clone(),
                rotation: t.rotation,
            })
            .collect();
        assert_eq!(tiles.len(), 34);
        let fish = crate::boids::plan::FishWorld::new(&tiles, &cat);
        assert_eq!(fish.cells.len(), 22);
        assert_eq!(
            tiles.len() - fish.cells.len(),
            12,
            "land tiles after the redo"
        );
        let region = fish
            .regions
            .iter()
            .find(|g| g.cells.contains(&Axial::new(7, -3)))
            .unwrap();
        for c in [(6, -2), (5, -1), (4, -1), (1, 0), (3, 4), (3, 3), (3, 2)] {
            assert!(
                region.cells.contains(&Axial::new(c.0, c.1)),
                "{c:?} {region:?}"
            );
        }
        assert_eq!(region.cells.len(), 18);
        let at = |q, r| tiles.iter().find(|t| t.cell == Axial::new(q, r)).unwrap();
        assert_eq!(at(5, -1).tile, "river_mouth");
        assert_eq!(at(3, 2).tile, "river_mouth_l");
        let lay = layout(&tiles, &cat, |_| false);
        for (spring, mouth) in [((7, -3), (5, -1)), ((3, 4), (3, 2))] {
            let (s, m) = (
                &lay[&Axial::new(spring.0, spring.1)].paths[0],
                &lay[&Axial::new(mouth.0, mouth.1)].paths[0],
            );
            assert_eq!((s.up, m.down), (RiverEnd::Spring, RiverEnd::Mouth));
        }
    }

    /// Owner 2026-10-14: `river_spring_center` failed to load and the stand-in bed covered the
    /// spring's kit base (a pale hex). The stand-in follows the base, the bed, never the centre.
    #[test]
    fn the_stand_in_bed_is_drawn_only_where_the_base_is_missing() {
        let cat = catalog();
        let water: Vec<&TileDef> = cat
            .tiles()
            .iter()
            .filter(|d| water_kind(d, 0).is_some())
            .collect();
        for id in [
            "river_spring",
            "river_mouth",
            "river_straight",
            "beach",
            "ocean",
        ] {
            assert!(water.iter().any(|d| d.id == id), "{id} is a water tile");
        }
        // The coast and ocean tiles have no centre: their dressing is scatter (owner 2026-10-26).
        assert!(
            water.iter().any(|d| d.center.is_none()),
            "a water tile without a centre"
        );
        for def in &water {
            let base = def.base.as_str();
            let bed = |has: &dyn Fn(&str) -> bool| {
                let lay = layout(&[tile(0, 0, &def.id, 0)], &cat, has);
                let bed = lay[&Axial::ORIGIN].bed;
                assert_eq!(bed, stand_in_bed(def, has), "{}", def.id);
                bed
            };
            assert!(!bed(&|_| true), "{}: whole kit", def.id);
            if let Some(center) = &def.center {
                let center = center.as_str();
                assert!(!bed(&|p| p != center), "{}: centre missing", def.id);
            }
            assert!(bed(&|p| p != base), "{}: base missing", def.id);
            assert!(bed(&|_| false), "{}: no kit", def.id);
        }
        // The rule leans on every water tile's base being its bed: no land tile shares one.
        for land in cat.tiles().iter().filter(|d| water_kind(d, 0).is_none()) {
            for def in &water {
                assert_ne!(land.base, def.base, "{} and {}", land.id, def.id);
            }
        }
        let meadow = cat.get("meadow").unwrap();
        assert!(!stand_in_bed(meadow, |_| false), "land tiles never take it");
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

    /// The river churn (foam where the flow turns or slows) is 0 on a straight and on every river
    /// edge, so neighbouring strips agree at each seam under every rotation, and it reads round the
    /// bends and in the fork, strongest in the tight bend.
    #[test]
    fn river_churn_reads_on_bends_and_the_fork_and_vanishes_on_straights_and_edges() {
        let cat = catalog();
        let o = Axial::ORIGIN;
        let peak = |id: &str, rot: u8| {
            let lay = layout(&[tile(0, 0, id, rot)], &cat, |_| false);
            let f = lay[&o].field();
            let mut best = 0.0f32;
            for i in -32..=32 {
                for j in -32..=32 {
                    let p = Vec2::new(i as f32 * 0.5, j as f32 * 0.5);
                    let c = f.churn(p.x, p.y);
                    assert!(c.is_finite() && c >= 0.0, "{id} r{rot} at {p}: {c}");
                    best = best.max(c);
                    let inside =
                        APOTHEM - (0..6).map(|e| p.dot(edge_dir(e))).fold(f32::MIN, f32::max);
                    if inside <= 0.0 {
                        assert!(c < 1e-6, "{id} r{rot}: churn {c} on the border at {p}");
                    }
                }
            }
            best
        };
        for rot in 0..6u8 {
            let straight = peak("river_straight", rot);
            let bend = peak("river_bend", rot);
            let tight = peak("river_bend_60", rot);
            let fork = peak("river_fork", rot);
            eprintln!(
                "r{rot}: straight {straight:.4} bend {bend:.4} bend_60 {tight:.4} fork {fork:.4}"
            );
            assert!(straight < 1e-3, "r{rot}: straight churn {straight}");
            assert!(
                bend > 0.03 && bend < tight,
                "r{rot}: bend {bend}, bend_60 {tight}"
            );
            assert!(tight > 0.12, "r{rot}: bend_60 {tight}");
            assert!(fork > 0.08, "r{rot}: fork {fork}");
        }
    }
}
