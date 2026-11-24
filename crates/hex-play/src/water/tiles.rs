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
    asset::RenderAssetUsages,
    core_pipeline::prepass::DepthPrepass,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    pbr::ScreenSpaceTransmission,
    post_process::dof::DepthOfField,
    prelude::*,
};
use hex_core::{
    Axial, Endpoint, HEX_RADIUS, LEVEL_STEP, NetworkKind, RiverTile, SocketKind, TileCatalog,
    TileDef, opposite_edge, river,
};

use super::{
    LAND, SHEET_TAG, TerrainMaterial, WaterField, WaterKey, WaterMaterial, WaterSample,
    WaterSurface, carve, cell_transform, materials, ribbon_mesh, river_flow, skirt_mesh_for,
    smoothstep, spawn_fixed_mesh, surface_mesh, surface_mesh_where, terrain_color,
    terrain_mesh_for,
};
use hex_render::{SceneEntity, post::QualityTier};

/// Water surface (m): [`LAND`] below the meadow cap's border ring at y = 0, the prototype's
/// plateau-to-water drop, which the terrain shader's grass line and wet band are tuned to.
pub const WATER_Y: f32 = -LAND;
/// Water surface at rest (m) on a tile at `level`: [`WATER_Y`] lifted by the level's steps. Every
/// water input (surface, bed profiles, cut faces, placeholder bed) of a tile is offset by the same
/// `level * LEVEL_STEP`.
pub fn water_y(level: u8) -> f32 {
    f32::from(level) * LEVEL_STEP + WATER_Y
}

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
/// Waterfall plunge pool: its foam and ripple radius (m) from the fall seam's midpoint, on the
/// lower tile.
pub const PLUNGE_R: f32 = 6.0;
/// The plunge pool's churn (m/s², `TileField::churn`) at the seam's midpoint, easing to 0 at
/// [`PLUNGE_R`]: past the 0.22 at which `water.wesl` turns churn fully white.
pub const PLUNGE_CHURN: f32 = 0.3;
/// Waterfall tiers (owner WF-Q3, 2026-11-22): the fall face steps out by this much (m) toward the
/// lower tile at each level of the drop, and the falling water is one sheet per tier.
pub const LEDGE_STEP: f32 = 1.5;
/// The sheet's half-width (m): the 8 m lane, so its edges meet the channel profile at the
/// waterline.
const SHEET_HALF: f32 = 4.0;
/// Lateral steps of the sheet and its rows per tier (each tier falls one level).
const SHEET_COLS: u32 = 8;
const SHEET_ROWS: u32 = 10;
/// Flow speed (m/s) carried on the sheet's vertices, pointing down the sheet (out over the lower
/// tile in plan), and the water depth they report.
const SHEET_SPEED: f32 = 6.0;
const SHEET_DEPTH: f32 = 0.3;
const POOL_SURFACE: f32 = 6.5;
const SPRING_RAMP: f32 = 6.5;
/// River junction (`river_fork`, water contract §2a): how far from an arm's centre line its
/// tangent still steers the blended flow. Under the 12 m between a River edge's lane point and
/// the other arms, so each edge's strip carries its own arm's flow, as the neighbour does.
const JUNCTION_BLEND: f32 = 8.0;

const APOTHEM: f32 = HEX_RADIUS * 0.866_025_4;

/// The tiers of a fall of `drop` levels, top first: tier i's lip stands `lip_drop_m` below the
/// upper water (`water_y(upper)`) and `setout_m` out from the seam toward the lower tile. Each
/// tier's sheet falls one [`LEVEL_STEP`], from its lip to the next lip (or to the pool), and steps
/// out [`LEDGE_STEP`] on the way.
pub fn fall_tiers(drop: u8) -> Vec<(f32, f32)> {
    (0..drop)
        .map(|i| (LEVEL_STEP * f32::from(i), LEDGE_STEP * f32::from(i)))
        .collect()
}

/// The falling sheet of `upper`'s waterfall `fall` (water contract §5), cell-centred and
/// unrotated: one strip per tier ([`fall_tiers`]), [`SHEET_ROWS`] + 1 rows of [`SHEET_COLS`] + 1
/// vertices each, 8 m wide on the lane. Each tier leaves its lip level and arcs out [`LEDGE_STEP`]
/// as it falls one level, so the top row is the upper water at the seam and the last row the lower
/// water. Returns positions, UV_0 (lateral offset along the lip, [`SHEET_TAG`] + metres below the
/// tier's lip) and COLOR (flow down the sheet in plan, river 1, depth).
pub fn sheet_mesh(
    upper: &CellWater,
    fall: FallEdge,
) -> (Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<[f32; 4]>) {
    sheet_at(upper.tile.level, fall.edge, fall.drop)
}

/// [`sheet_mesh`] of a fall of `drop` levels over `edge` of an upper tile at `level`, centred on
/// that tile.
fn sheet_at(level: u8, edge: u8, drop: u8) -> (Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<[f32; 4]>) {
    let out = edge_dir(edge);
    let lat = out.perp();
    let lip = out * APOTHEM;
    let top = water_y(level);
    let (mut pos, mut uv, mut col) = (vec![], vec![], vec![]);
    for (lip_drop, setout) in fall_tiers(drop) {
        for j in 0..=SHEET_ROWS {
            let f = j as f32 / SHEET_ROWS as f32;
            let below = LEVEL_STEP * f * f;
            let at = lip + out * (setout + LEDGE_STEP * f);
            for k in 0..=SHEET_COLS {
                let o = SHEET_HALF * (2.0 * k as f32 / SHEET_COLS as f32 - 1.0);
                let p = at + lat * o;
                pos.push([p.x, top - lip_drop - below, p.y]);
                uv.push([o, SHEET_TAG + below]);
                col.push([out.x * SHEET_SPEED, out.y * SHEET_SPEED, 1.0, SHEET_DEPTH]);
            }
        }
    }
    (pos, uv, col)
}

/// Where each tier of a fall of `drop` levels over `edge` of an upper tile at `level` lands
/// (cell-centred, unrotated, like `sheet_at`): the midpoint of its sheet's last row, one
/// [`LEVEL_STEP`] under its lip and [`LEDGE_STEP`] further out. Tier i lands on tier i + 1's lip;
/// the last tier (`true`) lands in the plunge pool, on the lower tile's water.
pub fn fall_landings(level: u8, edge: u8, drop: u8) -> Vec<(Vec3, bool)> {
    let out = edge_dir(edge);
    let top = water_y(level);
    fall_tiers(drop)
        .into_iter()
        .enumerate()
        .map(|(i, (lip_drop, setout))| {
            let p = out * (APOTHEM + setout + LEDGE_STEP);
            let pool = i + 1 == usize::from(drop);
            (Vec3::new(p.x, top - lip_drop - LEVEL_STEP, p.y), pool)
        })
        .collect()
}

/// One tier landing of a waterfall in the world, where its spray and mist rise
/// ([`super::mist`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MistLanding {
    /// World position of the landing's midpoint, on the water it falls onto.
    pub pos: Vec3,
    /// Unit plan direction down the fall, out over the lower tile.
    pub out: Vec2,
    /// Unit plan direction along the lip.
    pub lat: Vec2,
    /// The bottom tier's landing: the plunge pool.
    pub pool: bool,
    /// The fall's drop in levels.
    pub drop: u8,
    /// The upper tile, which seeds the landing's puffs.
    pub upper: Axial,
    /// The tier this landing ends (0 at the top).
    pub tier: u8,
}

/// Every tier landing of the waterfalls in `layout` (each seen from its lower tile), in the
/// layout's order. A layout without falls (`play::NoFalls`, the thumbnails) has none.
pub fn mist_landings(layout: &BTreeMap<Axial, CellWater>) -> Vec<MistLanding> {
    let mut out = vec![];
    for cw in layout.values() {
        for fall in cw.falls.iter().filter(|f| !f.upper) {
            let upper = cw.tile.cell.neighbor(fall.edge);
            let level = cw.tile.level + fall.drop;
            let edge = opposite_edge(fall.edge);
            let c = center(upper);
            let dir = edge_dir(edge);
            for (tier, (p, pool)) in fall_landings(level, edge, fall.drop)
                .into_iter()
                .enumerate()
            {
                out.push(MistLanding {
                    pos: Vec3::new(p.x + c.x, p.y, p.z + c.y),
                    out: dir,
                    lat: dir.perp(),
                    pool,
                    drop: fall.drop,
                    upper,
                    tier: tier as u8,
                });
            }
        }
    }
    out
}

/// The sheet's normal (world, unrotated) on `edge`'s fall `below` m under a tier's lip: up at the
/// lip, turning out toward the lower tile as the water falls.
fn sheet_normal(edge: u8, below: f32) -> Vec3 {
    let f = (below / LEVEL_STEP).max(0.0).sqrt();
    let out = edge_dir(edge) * (2.0 * LEVEL_STEP * f);
    Vec3::new(out.x, LEDGE_STEP, out.y).normalize()
}

/// The sheet of the fall seen from its lower tile `lower` (`fall.upper` false) as a [`Mesh`] in
/// the space of the lower tile's surface drawn under `t`, its triangles facing out over the lower
/// tile, to merge after that surface's triangles: the pool and the sheet are one draw, and the
/// sheet blends over the pool where Low sorts per draw. Its UV_1.x is the upper tile's lift, which
/// `water.wesl` measures the fall from.
fn sheet_for(lower: &CellWater, fall: FallEdge, t: &Transform) -> Mesh {
    let upper = lower.tile.cell.neighbor(fall.edge);
    let level = lower.tile.level + fall.drop;
    let edge = opposite_edge(fall.edge);
    let (pos, uv, col) = sheet_at(level, edge, fall.drop);
    let c = center(upper);
    let inv = t.compute_affine().inverse();
    let local: Vec<[f32; 3]> = pos
        .iter()
        .map(|p| {
            inv.transform_point3(Vec3::new(p[0] + c.x, p[1], p[2] + c.y))
                .to_array()
        })
        .collect();
    let normals: Vec<[f32; 3]> = uv
        .iter()
        .map(|u| (t.rotation.inverse() * sheet_normal(edge, u[1] - SHEET_TAG)).to_array())
        .collect();
    let stride = SHEET_COLS + 1;
    let mut idx = vec![];
    for tier in 0..u32::from(fall.drop) {
        let base = tier * (SHEET_ROWS + 1) * stride;
        for i in 0..SHEET_ROWS {
            for j in 0..SHEET_COLS {
                let a = base + i * stride + j;
                let (b, c, d) = (a + 1, a + stride, a + stride + 1);
                for mut tri in [[a, c, b], [b, c, d]] {
                    let [p0, p1, p2] = tri.map(|k| Vec3::from(local[k as usize]));
                    let n = Vec3::from(normals[tri[0] as usize]);
                    if (p1 - p0).cross(p2 - p0).dot(n) < 0.0 {
                        tri.swap(1, 2);
                    }
                    idx.extend(tri);
                }
            }
        }
    }
    let n = local.len();
    let lift = f32::from(level) * LEVEL_STEP;
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, local)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, vec![[lift, 0.0]; n])
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
    .with_inserted_indices(Indices::U32(idx))
}

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
    let r = hex_render::kit_runtime::piece_rotation(rotation) * Vec3::new(p.x, 0.0, p.y);
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
    let r = hex_render::kit_runtime::piece_rotation(rotation) * Vec3::new(p.x, 0.0, p.y);
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
    /// A river lane at the tile's level: the channel cross-section is symmetric about the edge
    /// midpoint.
    River(u8),
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
            (EdgeWater::River(a), EdgeWater::River(b)) => a == b,
            (EdgeWater::Ocean, EdgeWater::Ocean) => true,
            (EdgeWater::Coast(a), EdgeWater::Coast(b)) => a.distance(b) < 0.01,
            _ => false,
        }
    }
}

fn edge_water(def: &TileDef, t: &PlayTile, world: u8) -> EdgeWater {
    let socket = def.edge(world, t.rotation);
    match socket.kind {
        SocketKind::River if socket.lane.is_some() => EdgeWater::River(t.level),
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

pub use hex_core::RiverEnd;

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

/// Every placed river segment as a world centre line, upstream first, oriented by
/// [`hex_core::river::orient`] (water contract §6). Keyed by (cell, segment index among the
/// tile's river segments). Tiles missing from `catalog` carry no river.
pub fn oriented_rivers(
    tiles: &[PlayTile],
    catalog: &TileCatalog,
) -> BTreeMap<(Axial, usize), RiverPath> {
    let placed: Vec<(&PlayTile, &TileDef)> = tiles
        .iter()
        .filter_map(|t| catalog.get(&t.tile).map(|def| (t, def)))
        .collect();
    let river_tiles: Vec<RiverTile> = placed
        .iter()
        .map(|&(t, def)| RiverTile {
            cell: t.cell,
            def,
            rotation: t.rotation,
            level: t.level,
        })
        .collect();
    // Placement (`World::fits`) and decode (`WorldIssue::RiverUphill`) never build an uphill
    // network; should one arrive, flow as if flat.
    let rivers = river::orient(&river_tiles).unwrap_or_else(|_| {
        let flat: Vec<RiverTile> = river_tiles
            .iter()
            .map(|t| RiverTile { level: 0, ..*t })
            .collect();
        river::orient(&flat).expect("a network without waterfalls is always legal")
    });
    let mut out = BTreeMap::new();
    for &(t, def) in &placed {
        for (i, (_, _, local)) in river::river_segment_points(def, t.rotation)
            .into_iter()
            .enumerate()
        {
            let Some(seg) = rivers.segs.get(&(t.cell, i)) else {
                continue;
            };
            let mut points: Vec<Vec2> = local
                .into_iter()
                .map(|p| to_world(t.cell, t.rotation, Vec2::from(p)))
                .collect();
            if seg.reversed {
                points.reverse();
            }
            out.insert(
                (t.cell, i),
                RiverPath {
                    points,
                    up: seg.up,
                    down: seg.down,
                },
            );
        }
    }
    out
}

/// The waterfalls of the placed `tiles` (`hex_core::river::orient`'s `Rivers::falls`, the seams
/// [`CellWater::falls`] names from both sides). An uphill network, which placement never builds,
/// has none.
pub fn waterfalls(tiles: &[PlayTile], catalog: &TileCatalog) -> Vec<river::Waterfall> {
    let river_tiles: Vec<RiverTile> = tiles
        .iter()
        .filter_map(|t| {
            Some(RiverTile {
                cell: t.cell,
                def: catalog.get(&t.tile)?,
                rotation: t.rotation,
                level: t.level,
            })
        })
        .collect();
    river::orient(&river_tiles).map_or_else(|_| vec![], |r| r.falls)
}

/// A waterfall seam on a water tile: a River edge facing River at another level. It draws no cut
/// face on either side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FallEdge {
    /// The tile's world edge.
    pub edge: u8,
    /// Levels between the two sides.
    pub drop: u8,
    /// Whether this tile is the upper side (the lip) rather than the lower (the pool).
    pub upper: bool,
}

/// One water tile as the meshes see it. Rebuilt when any field changes.
#[derive(Clone, Debug, PartialEq)]
pub struct CellWater {
    pub tile: PlayTile,
    pub kind: WaterKind,
    /// World edges whose cut face is drawn: water there and none across it.
    pub exposed: Vec<u8>,
    /// Waterfall seams: River edges facing River at another level (never in `exposed`).
    pub falls: Vec<FallEdge>,
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
        let (mut exposed, mut falls) = (vec![], vec![]);
        for e in 0..6u8 {
            let mine = edge_water(def, t, e);
            if !mine.wet() {
                continue;
            }
            let theirs = placed.get(&t.cell.neighbor(e)).and_then(|n| {
                catalog
                    .get(&n.tile)
                    .map(|d| edge_water(d, n, opposite_edge(e)))
            });
            match (mine, theirs) {
                (EdgeWater::River(a), Some(EdgeWater::River(b))) if a != b => {
                    falls.push(FallEdge {
                        edge: e,
                        drop: a.abs_diff(b),
                        upper: a > b,
                    });
                }
                _ if theirs.is_some_and(|w| mine.continues(w)) => {}
                _ => exposed.push(e),
            }
        }
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
                falls,
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

    /// Distance (m) from `p` to the midpoint of the nearest fall seam this tile is the lower side
    /// of (its plunge pool), if it has one.
    fn pool_distance(&self, p: Vec2) -> Option<f32> {
        let c = center(self.cell.tile.cell);
        self.cell
            .falls
            .iter()
            .filter(|f| !f.upper)
            .map(|f| p.distance(c + edge_dir(f.edge) * APOTHEM))
            .min_by(f32::total_cmp)
    }

    /// The plunge pool's churn at `p`: [`PLUNGE_CHURN`] at the seam's midpoint, easing to 0 at
    /// [`PLUNGE_R`]. A fall seam is not a continuous seam, so it is exempt from the churn's edge
    /// fade.
    fn pool_churn(&self, p: Vec2) -> f32 {
        self.pool_distance(p)
            .map_or(0.0, |d| PLUNGE_CHURN * (1.0 - smoothstep(0.0, PLUNGE_R, d)))
    }

    /// The tile's level offset (m), added to every bed and water height.
    fn lift(&self) -> f32 {
        f32::from(self.cell.tile.level) * LEVEL_STEP
    }
}

impl WaterField for TileField<'_> {
    fn level(&self) -> f32 {
        water_y(self.cell.tile.level)
    }

    fn water_lift(&self) -> f32 {
        self.lift()
    }

    fn height(&self, x: f32, z: f32) -> f32 {
        let p = Vec2::new(x, z);
        self.lift()
            + match self.cell.kind {
                WaterKind::River => river_height(self.river(p).0),
                WaterKind::Spring(node) => {
                    river_height(self.river(p).0).min(pool_height(self.node_distance(p, node)))
                }
                WaterKind::RiverMouth(coast) => {
                    mouth_height(self.seaward(p, coast), self.river(p).0)
                }
                WaterKind::Coast(coast) => coast_height(self.seaward(p, coast)),
                WaterKind::Ocean => OCEAN_BED_Y,
            }
    }

    fn water(&self, x: f32, z: f32) -> WaterSample {
        let p = Vec2::new(x, z);
        let depth = (self.level() - self.height(x, z)).max(0.0);
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
    /// (central difference over `CHURN_STEP`): 0 on a straight, `speed² / R` round a bend of
    /// radius R, and where the fork's arms turn into each other and slow. Faded to 0 over
    /// `CHURN_EDGE_FADE` inside the hex border, so every continuous river edge reads 0 on both
    /// sides. A plunge pool's churn ([`TileField::pool_churn`]) is added at its waterfall seam, on
    /// any river or mouth tile below a fall.
    fn churn(&self, x: f32, z: f32) -> f32 {
        let p = Vec2::new(x, z);
        let pool = self.pool_churn(p);
        if self.cell.kind != WaterKind::River {
            return pool;
        }
        let u = self.water(x, z).flow;
        let speed = u.length();
        if speed < 1e-3 {
            return pool;
        }
        let step = u / speed * CHURN_STEP;
        let ahead = self.water(p.x + step.x, p.y + step.y).flow;
        let behind = self.water(p.x - step.x, p.y - step.y).flow;
        let accel = speed * (ahead - behind).length() / (2.0 * CHURN_STEP);
        let d = p - center(self.cell.tile.cell);
        let inside = APOTHEM - (0..6).map(|e| d.dot(edge_dir(e))).fold(f32::MIN, f32::max);
        (accel * smoothstep(0.0, CHURN_EDGE_FADE, inside)).max(pool)
    }

    fn plunge(&self, x: f32, z: f32) -> f32 {
        self.pool_distance(Vec2::new(x, z))
            .map_or(0.0, |d| (1.0 - d / PLUNGE_R).max(0.0))
    }
}

/// Step (m) of the churn's central difference along the flow.
const CHURN_STEP: f32 = 0.5;
/// Width (m) inside the hex border over which the churn fades to 0 at the edge.
const CHURN_EDGE_FADE: f32 = 3.0;

/// Rows of a river strip along `path`: every `RIBBON_STEP` m, each with the lateral axis
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
    landings: Vec<MistLanding>,
}

impl PlayWater {
    /// The drawn waterfalls' tier landings ([`mist_landings`] of the last [`sync`]).
    pub fn landings(&self) -> &[MistLanding] {
        &self.landings
    }
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
/// returned instead of despawned, so the tile's sink can take them down with it. With `falls`
/// false (`play::NoFalls`) the layout keeps no waterfall ([`without_falls`]).
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
    falls: bool,
) -> Vec<(Axial, Vec<Entity>)> {
    if rebuild {
        for (_, (_, es)) in water.cells.drain() {
            for e in es {
                commands.entity(e).try_despawn();
            }
        }
    }
    let mut want = layout(tiles, catalog, kit_has);
    if !falls {
        without_falls(&mut want);
    }
    let landings = mist_landings(&want);
    if water.landings != landings {
        water.landings = landings;
    }
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

/// Drops every waterfall from `layout` (`play::NoFalls`: the thumbnails): no sheet, no plunge
/// pool. A fall edge stays out of `exposed`, so no cut face appears in its place.
pub fn without_falls(layout: &mut BTreeMap<Axial, CellWater>) {
    for cw in layout.values_mut() {
        cw.falls.clear();
    }
}

/// A water tile's meshes, each with the transform it is drawn under: its surfaces (with the
/// falling sheet of every waterfall it is the lower side of merged into the first after the pool's
/// triangles, so the sheet costs no draw and draws over its pool), then its cut faces.
fn water_meshes(cw: &CellWater) -> Vec<(Mesh, Transform)> {
    let field = cw.field();
    let (cell, rot) = (cw.tile.cell, cw.tile.rotation);
    let c = center(cell);
    let at_cell = Transform::from_xyz(c.x, 0.0, c.y);
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
    if let Some((surface, t)) = surfaces.first_mut() {
        for fall in cw.falls.iter().filter(|f| !f.upper) {
            surface
                .merge(&sheet_for(cw, *fall, t))
                .expect("water meshes share one attribute layout");
        }
    }
    if let Some(skirt) = skirt_mesh_for(&field, cell, &cw.exposed) {
        surfaces.push((skirt, at_cell));
    }
    surfaces
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
    let mut out = vec![];
    if cw.bed {
        let t = cell_transform(cell, rot);
        out.push(
            spawn_fixed_mesh(
                commands,
                meshes,
                terrain_mesh_for(&field, cell, rot, FLOOR_Y + field.lift()),
                (
                    MeshMaterial3d(terrain.clone()),
                    t,
                    RisePart::new(cell, RiseRole::Bed, t),
                    SceneEntity,
                ),
            )
            .id(),
        );
    }
    for (mesh, t) in water_meshes(cw) {
        out.push(
            spawn_fixed_mesh(
                commands,
                meshes,
                mesh,
                (
                    MeshMaterial3d(water.clone()),
                    t,
                    RisePart::new(cell, RiseRole::Water, t),
                    NotShadowCaster,
                    WaterSurface,
                    SceneEntity,
                ),
            )
            .id(),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_core::{Side, World, opposite_edge, world_edge};

    fn tile(q: i32, r: i32, id: &str, rotation: u8) -> PlayTile {
        PlayTile {
            cell: Axial::new(q, r),
            tile: id.into(),
            rotation,
            level: 0,
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
            for (a, b, local) in river::river_segment_points(cat.get(id).unwrap(), rot) {
                let pts: Vec<Vec2> = local
                    .into_iter()
                    .map(|p| to_world(t.cell, rot, Vec2::from(p)))
                    .collect();
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

    fn river(id: &str, (q, r): (i32, i32), rotation: u8, level: u8) -> PlayTile {
        PlayTile {
            level,
            ..tile(q, r, id, rotation)
        }
    }

    fn cat() -> TileCatalog {
        catalog()
    }

    fn ax(q: i32, r: i32) -> Axial {
        Axial::new(q, r)
    }

    #[test]
    fn a_raised_river_surface_and_bed_stand_at_its_level() {
        for level in 1..=3u8 {
            let lay = layout(&[river("river_straight", (0, 0), 0, level)], &cat(), |_| {
                true
            });
            let f = lay[&ax(0, 0)].field();
            assert_eq!(f.level(), water_y(level));
            assert!((f.height(0.0, 0.0) - (-2.95 + 4.0 * f32::from(level))).abs() < 1e-3);
            // The water column over the bed is as deep as at level 0.
            assert!(
                (f.water(0.0, 0.0).depth - 1.55).abs() < 1e-3,
                "level {level}"
            );
        }
    }

    /// Every water and placeholder-bed mesh of a tile carries its lift over `WATER_Y` (the
    /// materials' `sea_level`) in UV_1.x for the shaders: exactly 0 at level 0, `level *
    /// LEVEL_STEP` raised, and the surface (and each cut face's rim) stands at `WATER_Y` plus it.
    #[test]
    fn water_meshes_carry_their_lift_over_the_sea_level() {
        use bevy::mesh::VertexAttributeValues;
        let uv1 = |m: &Mesh| match m.attribute(Mesh::ATTRIBUTE_UV_1) {
            Some(VertexAttributeValues::Float32x2(v)) => v.clone(),
            other => panic!("no UV_1: {other:?}"),
        };
        let ys = |m: &Mesh| match m.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(v)) => v.iter().map(|p| p[1]).collect::<Vec<_>>(),
            _ => unreachable!(),
        };
        for level in [0u8, 1, 3] {
            let lay = layout(&[river("river_straight", (0, 0), 0, level)], &cat(), |_| {
                true
            });
            let cw = &lay[&ax(0, 0)];
            let f = cw.field();
            let (cell, rot) = (cw.tile.cell, cw.tile.rotation);
            let lift = f32::from(level) * LEVEL_STEP;
            assert_eq!(f.water_lift(), lift);
            let rows = ribbon_rows(&cw.paths[0]);
            let ribbon = ribbon_mesh(&f, center(cell), &rows, RIBBON_HALF, RIBBON_COLS);
            let surface = surface_mesh(&f, cell, rot);
            let skirt = skirt_mesh_for(&f, cell, &cw.exposed).expect("open ends");
            let bed = terrain_mesh_for(&f, cell, rot, FLOOR_Y + f.lift());
            for (name, m) in [
                ("ribbon", &ribbon),
                ("surface", &surface),
                ("skirt", &skirt),
                ("bed", &bed),
            ] {
                let uv = uv1(m);
                assert!(!uv.is_empty());
                assert!(
                    uv.iter()
                        .all(|u| u[0].to_bits() == lift.to_bits() && u[1] == 0.0),
                    "{name} at level {level}"
                );
            }
            for m in [&ribbon, &surface] {
                assert!(ys(m).iter().all(|&y| (y - (WATER_Y + lift)).abs() < 1e-5));
            }
            let rim = ys(&skirt).into_iter().step_by(2).collect::<Vec<_>>();
            assert!(
                rim.iter().all(|&y| (y - (WATER_Y + lift)).abs() < 1e-5),
                "{rim:?}"
            );
        }
    }

    #[test]
    fn river_at_different_levels_is_a_fall_not_a_cut_face() {
        let lay = layout(
            &[
                river("river_straight", (0, 0), 0, 2),
                river("river_straight", (1, 0), 0, 0),
            ],
            &cat(),
            |_| true,
        );
        assert_eq!(
            lay[&ax(0, 0)].falls,
            vec![FallEdge {
                edge: 0,
                drop: 2,
                upper: true
            }]
        );
        assert_eq!(
            lay[&ax(1, 0)].falls,
            vec![FallEdge {
                edge: 3,
                drop: 2,
                upper: false
            }]
        );
        assert!(!lay[&ax(0, 0)].exposed.contains(&0) && !lay[&ax(1, 0)].exposed.contains(&3));
    }

    /// A waterfall of `drop` levels: a raised `river_straight` at (0, 0) falling over its edge 0
    /// onto one at (1, 0), level 0, or onto `lower` there.
    fn fall_pair(drop: u8, lower: &str) -> BTreeMap<Axial, CellWater> {
        let lower_rot = (0..6u8)
            .find(|&k| {
                let d = cat().get(lower).cloned().unwrap();
                d.edge(3, k).kind == SocketKind::River
            })
            .unwrap();
        layout(
            &[
                river("river_straight", (0, 0), 0, drop),
                river(lower, (1, 0), lower_rot, 0),
            ],
            &cat(),
            |_| true,
        )
    }

    fn upper_fall(lay: &BTreeMap<Axial, CellWater>) -> (&CellWater, FallEdge) {
        let cw = &lay[&ax(0, 0)];
        (cw, cw.falls[0])
    }

    fn min_max_y(p: &[[f32; 3]]) -> (f32, f32) {
        p.iter().fold((f32::MIN, f32::MAX), |(hi, lo), q| {
            (hi.max(q[1]), lo.min(q[1]))
        })
    }

    #[test]
    fn sheet_vertices_are_tagged_and_drop_from_lip_to_pool() {
        for drop in 1..=3u8 {
            let lay = fall_pair(drop, "river_straight");
            let (cw, fall) = upper_fall(&lay);
            assert_eq!((fall.edge, fall.drop, fall.upper), (0, drop, true));
            let (p, uv, flow) = sheet_mesh(cw, fall);
            assert!(uv.iter().all(|u| u[1] >= SHEET_TAG), "drop {drop}");
            let (top, bot) = min_max_y(&p);
            assert!((top - water_y(drop)).abs() < 1e-3, "drop {drop}: top {top}");
            assert!((bot - water_y(0)).abs() < 1e-3, "drop {drop}: bottom {bot}");
            let lip: Vec<Vec2> = p
                .iter()
                .filter(|q| (q[1] - top).abs() < 1e-4)
                .map(|q| Vec2::new(q[0], q[2]))
                .collect();
            let width = lip
                .iter()
                .flat_map(|a| lip.iter().map(move |b| a.distance(*b)))
                .fold(0.0, f32::max);
            assert!((width - 8.0).abs() < 0.05, "drop {drop}: width {width}");
            // The lip row lies on the seam, its edges on the channel's waterline.
            let f = cw.field();
            let c = center(cw.tile.cell);
            for q in &lip {
                assert!(((*q).dot(edge_dir(0)) - APOTHEM).abs() < 1e-3);
                let w = c + *q;
                assert!(f.height(w.x, w.y) <= water_y(drop) + 1e-3);
            }
            // Its two edge points (±4 m on the lane) meet the bank at the waterline.
            for q in [lip[0], lip[lip.len() - 1]] {
                assert!(((q.dot(edge_dir(0).perp())).abs() - 4.0).abs() < 1e-4);
                let w = c + q;
                let bed = f.height(w.x, w.y);
                assert!(
                    (bed - water_y(drop)).abs() < 0.02,
                    "drop {drop}: bank {bed} at {q}"
                );
            }
            // At least 8 rows per level of drop, and the flow points down the sheet (out over the
            // lower tile in plan).
            let rows = p.len() / (SHEET_COLS as usize + 1);
            assert!(rows >= 8 * usize::from(drop), "{rows} rows");
            assert!(
                flow.iter()
                    .all(|c| Vec2::new(c[0], c[1]).dot(edge_dir(0)) > 1.0)
            );
        }
    }

    /// WF-Q3: one sheet per tier. Tier i's lip stands `lip_drop` below the upper water and
    /// `setout` out from the seam ([`fall_tiers`]), and it falls one level to the next lip.
    #[test]
    fn the_tiers_lips_follow_fall_tiers() {
        assert_eq!(fall_tiers(1), vec![(0.0, 0.0)]);
        assert_eq!(fall_tiers(3), vec![(0.0, 0.0), (4.0, 1.5), (8.0, 3.0)]);
        assert_eq!(LEDGE_STEP, 1.5);
        for drop in 1..=3u8 {
            let lay = fall_pair(drop, "river_straight");
            let (cw, fall) = upper_fall(&lay);
            let (p, uv, _) = sheet_mesh(cw, fall);
            let tier_len = ((SHEET_ROWS + 1) * (SHEET_COLS + 1)) as usize;
            assert_eq!(p.len(), tier_len * usize::from(drop));
            let tiers = fall_tiers(drop);
            for (i, (lip_drop, setout)) in tiers.iter().enumerate() {
                let tier = &p[i * tier_len..(i + 1) * tier_len];
                let out = |q: &[f32; 3]| Vec2::new(q[0], q[2]).dot(edge_dir(0)) - APOTHEM;
                let lip_y = water_y(drop) - lip_drop;
                for q in &tier[..=SHEET_COLS as usize] {
                    assert!((q[1] - lip_y).abs() < 1e-4, "tier {i}: lip y {}", q[1]);
                    assert!(
                        (out(q) - setout).abs() < 1e-4,
                        "tier {i}: setout {}",
                        out(q)
                    );
                }
                for q in &tier[tier_len - SHEET_COLS as usize - 1..] {
                    assert!((q[1] - (lip_y - LEVEL_STEP)).abs() < 1e-4, "tier {i}: foot");
                    assert!((out(q) - (setout + LEDGE_STEP)).abs() < 1e-4);
                }
                let below = &uv[i * tier_len..(i + 1) * tier_len];
                assert!(
                    below
                        .iter()
                        .all(|u| (0.0..=LEVEL_STEP + 1e-4).contains(&(u[1] - SHEET_TAG)))
                );
            }
        }
    }

    /// Checks the sheet of the fall over `edge` of `upper` (`drop` levels): the upper tile's water
    /// carries no sheet vertex, and the lower tile's first surface carries the whole sheet after its
    /// own (pool) triangles, so within that one draw the sheet blends over the pool. In world space
    /// the lip row stands at the upper water on the seam, 8 m wide, and every sheet triangle faces
    /// out over the lower tile and up. Returns the lower surface's transform.
    fn sheet_on_seam(
        lay: &BTreeMap<Axial, CellWater>,
        upper: Axial,
        edge: u8,
        drop: u8,
    ) -> Transform {
        use bevy::mesh::VertexAttributeValues;
        let uv0 = |m: &Mesh| match m.attribute(Mesh::ATTRIBUTE_UV_0) {
            Some(VertexAttributeValues::Float32x2(v)) => v.clone(),
            other => panic!("{other:?}"),
        };
        for (m, _) in water_meshes(&lay[&upper]) {
            assert!(
                uv0(&m).iter().all(|u| u[1] < SHEET_TAG - 1.0),
                "the upper side draws no sheet"
            );
        }
        let (mesh, t) = water_meshes(&lay[&upper.neighbor(edge)]).swap_remove(0);
        let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions");
        };
        let uv = uv0(&mesh);
        let Some(Indices::U32(idx)) = mesh.indices() else {
            panic!("indices");
        };
        let world = |i: usize| t.transform_point(Vec3::from(pos[i]));
        let is_sheet = |i: u32| uv[i as usize][1] >= SHEET_TAG;
        let tris: Vec<&[u32]> = idx.chunks(3).collect();
        let first = tris
            .iter()
            .position(|tri| tri.iter().any(|&i| is_sheet(i)))
            .expect("the lower surface carries the sheet");
        assert!(first > 0, "the pool's own triangles come first");
        assert!(
            tris[..first]
                .iter()
                .all(|tri| tri.iter().all(|&i| !is_sheet(i)))
        );
        assert!(
            tris[first..]
                .iter()
                .all(|tri| tri.iter().all(|&i| is_sheet(i)))
        );
        assert_eq!(
            (tris.len() - first) as u32,
            2 * u32::from(drop) * SHEET_ROWS * SHEET_COLS
        );
        let level = lay[&upper].tile.level;
        let (c, out) = (center(upper), edge_dir(edge));
        let lip: Vec<Vec3> = (0..pos.len())
            .filter(|&i| uv[i][1] == SHEET_TAG && world(i).y > water_y(level) - 1e-3)
            .map(world)
            .collect();
        assert_eq!(lip.len(), SHEET_COLS as usize + 1);
        for p in &lip {
            assert!((p.y - water_y(level)).abs() < 1e-4, "{p}");
            assert!(
                ((p.xz() - c).dot(out) - APOTHEM).abs() < 1e-3,
                "{p} is off the seam"
            );
        }
        let width = lip
            .iter()
            .flat_map(|a| lip.iter().map(move |b| a.distance(*b)))
            .fold(0.0, f32::max);
        assert!((width - 8.0).abs() < 0.05, "{width}");
        let out3 = Vec3::new(out.x, 0.0, out.y);
        for tri in &tris[first..] {
            let [a, b, c] = [0, 1, 2].map(|k| world(tri[k] as usize));
            let n = (b - a).cross(c - a);
            assert!(n.dot(out3) > 0.0 && n.y > 0.0, "{n}");
        }
        t
    }

    /// I1 (fix round 1): the sheet rides the lower tile's water after the pool, so at Low (one
    /// blended draw per cell) it draws over its pool from every side; the upper tile carries none.
    /// Cut-face tags stay far below [`SHEET_TAG`] at the top level.
    #[test]
    fn the_sheet_draws_after_its_pool_in_the_lower_tile() {
        use bevy::mesh::VertexAttributeValues;
        for drop in 1..=3u8 {
            sheet_on_seam(&fall_pair(drop, "river_straight"), ax(0, 0), 0, drop);
        }
        let raised = layout(&[river("river_straight", (0, 0), 0, 3)], &cat(), |_| true);
        let cw = &raised[&ax(0, 0)];
        let skirt = skirt_mesh_for(&cw.field(), cw.tile.cell, &cw.exposed).unwrap();
        let Some(VertexAttributeValues::Float32x2(uv)) = skirt.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("uv");
        };
        let top = uv.iter().map(|u| u[1]).fold(f32::MIN, f32::max);
        assert!(
            top > super::super::SKIRT_TAG && top < SHEET_TAG - 50.0,
            "{top}"
        );
        let src = include_str!("../../../../assets/shaders/water.wesl");
        for (name, v) in [("SHEET_TAG", SHEET_TAG), ("PLUNGE_R", PLUNGE_R)] {
            let line = format!("const {name}: f32 = {v:?};");
            assert!(src.contains(&line), "water.wesl lacks `{line}`");
        }
    }

    /// The sheet lands on the seam in world space whatever the rotations: a raised spring at
    /// rotation 2 over a straight, a straight at rotation 1 falling over edge 1 onto a straight, and
    /// onto a mouth whose surface is the hex grid under a rotated `cell_transform`.
    #[test]
    fn rotated_tiles_carry_the_sheet_onto_the_seam() {
        let o = Axial::ORIGIN;
        let rot_for = |id: &str, e: u8| {
            let d = cat().get(id).cloned().unwrap();
            (0..6u8)
                .find(|&k| d.edge(e, k).kind == SocketKind::River)
                .unwrap()
        };
        let pair = |up: (&str, u8, u8), edge: u8, down: &str| {
            let n = o.neighbor(edge);
            let mut lower = river(down, (n.q, n.r), rot_for(down, opposite_edge(edge)), 0);
            if down == "river_straight" {
                lower.rotation = up.1 % 3;
            }
            layout(&[river(up.0, (0, 0), up.1, up.2), lower], &cat(), |_| true)
        };
        let lay = pair(("river_spring", 2, 1), 2, "river_straight");
        assert!(matches!(lay[&o].kind, WaterKind::Spring(_)));
        sheet_on_seam(&lay, o, 2, 1);
        let lay = pair(("river_straight", 1, 2), 1, "river_straight");
        assert_eq!(
            lay[&o].falls,
            vec![FallEdge {
                edge: 1,
                drop: 2,
                upper: true
            }]
        );
        sheet_on_seam(&lay, o, 1, 2);
        let lay = pair(("river_straight", 1, 1), 1, "river_mouth");
        assert!(matches!(lay[&o.neighbor(1)].kind, WaterKind::RiverMouth(_)));
        let t = sheet_on_seam(&lay, o, 1, 1);
        assert!(
            t.rotation != Quat::IDENTITY,
            "the mouth's surface is rotated"
        );
    }

    #[test]
    fn plunge_churn_is_full_at_the_seam_and_zero_past_plunge_r() {
        for lower in ["river_straight", "river_mouth", "river_mouth_l"] {
            let lay = fall_pair(1, lower);
            let cw = &lay[&ax(1, 0)];
            assert_eq!(
                cw.falls,
                vec![FallEdge {
                    edge: 3,
                    drop: 1,
                    upper: false
                }],
                "{lower}"
            );
            let f = cw.field();
            let c = center(cw.tile.cell);
            let seam = c + edge_dir(3) * APOTHEM;
            assert!(f.churn(seam.x, seam.y) >= 0.22, "{lower}");
            assert_eq!(f.meet(seam.x, seam.y), 0.0, "{lower}");
            assert!(
                super::super::foam_tag(&f, seam.x, seam.y) <= -0.22,
                "{lower}"
            );
            assert_eq!(f.plunge(seam.x, seam.y), 1.0);
            let past = seam - edge_dir(3) * (PLUNGE_R + 0.01);
            assert_eq!(f.plunge(past.x, past.y), 0.0, "{lower}");
            if lower == "river_straight" {
                assert_eq!(f.churn(past.x, past.y), 0.0);
            }
            // The upper side's water keeps no pool.
            let up = lay[&ax(0, 0)].field();
            assert_eq!(up.plunge(seam.x, seam.y), 0.0);
            assert_eq!(up.churn(seam.x, seam.y), 0.0);
        }
    }

    #[test]
    fn churn_still_fades_at_continuous_seams() {
        let tiles = [
            river("river_straight", (0, 0), 0, 1),
            river("river_straight", (1, 0), 0, 0),
            river("river_straight", (2, 0), 0, 0),
        ];
        let lay = layout(&tiles, &cat(), |_| true);
        let mut flat = lay.clone();
        without_falls(&mut flat);
        let (f, plain) = (lay[&ax(1, 0)].field(), flat[&ax(1, 0)].field());
        // The continuous seam to (2, 0) reads 0 on both sides.
        for p in along_edge(ax(1, 0), 0, 32) {
            assert!(f.churn(p.x, p.y) < 1e-6);
            assert!(lay[&ax(2, 0)].field().churn(p.x, p.y) < 1e-6);
        }
        // Past the pool the churn is the fall-free field's.
        let c = center(ax(1, 0));
        for i in -24..=24 {
            for j in -24..=24 {
                let p = c + Vec2::new(i as f32, j as f32) * 0.5;
                if p.distance(c + edge_dir(3) * APOTHEM) > PLUNGE_R {
                    assert_eq!(f.churn(p.x, p.y), plain.churn(p.x, p.y), "{p}");
                }
            }
        }
    }

    /// Each tier's mist landing is the midpoint of its sheet strip's last row, so the spray stands
    /// where the water lands; a fall of `drop` levels has `drop` landings, the last in the pool.
    #[test]
    fn mist_landings_are_the_sheet_strips_bottom_midpoints() {
        let stride = (SHEET_COLS + 1) as usize;
        for drop in 1..=3u8 {
            let lay = fall_pair(drop, "river_straight");
            let (cw, fall) = upper_fall(&lay);
            let (p, _, _) = sheet_mesh(cw, fall);
            let c = center(cw.tile.cell);
            let ls = mist_landings(&lay);
            assert_eq!(ls.len(), usize::from(drop));
            for (i, l) in ls.iter().enumerate() {
                let k = i * (SHEET_ROWS as usize + 1) * stride
                    + SHEET_ROWS as usize * stride
                    + SHEET_COLS as usize / 2;
                let want = Vec3::new(p[k][0] + c.x, p[k][1], p[k][2] + c.y);
                assert!(
                    l.pos.distance(want) < 1e-4,
                    "drop {drop} tier {i}: {l:?} vs {want}"
                );
                assert_eq!(
                    (l.pool, l.drop, l.tier),
                    (i + 1 == usize::from(drop), drop, i as u8)
                );
                assert_eq!(l.upper, cw.tile.cell);
                assert!(l.out.distance(edge_dir(fall.edge)) < 1e-5);
            }
        }
    }

    /// The thumbnails' layout (`play::NoFalls`) has no landings, so it draws no mist.
    #[test]
    fn thumbs_world_has_no_mist_landings() {
        let mut lay = fall_pair(3, "river_straight");
        assert_eq!(mist_landings(&lay).len(), 3);
        without_falls(&mut lay);
        assert!(mist_landings(&lay).is_empty());
        assert!(mist_landings(&fall_pair(0, "river_straight")).is_empty());
    }

    /// `play::NoFalls` (the thumbnails): with the falls dropped the upper tile's water carries no
    /// sheet vertex and the lower tile's no pool.
    #[test]
    fn thumbs_world_draws_no_falls() {
        use bevy::mesh::VertexAttributeValues;
        let attr = |m: &Mesh, a| match m.attribute(a) {
            Some(VertexAttributeValues::Float32x2(v)) => v.clone(),
            other => panic!("{other:?}"),
        };
        let sheet = |cw: &CellWater| {
            water_meshes(cw)
                .iter()
                .flat_map(|(m, _)| attr(m, Mesh::ATTRIBUTE_UV_0))
                .filter(|u| u[1] >= SHEET_TAG - 1.0)
                .count()
        };
        let pool = |cw: &CellWater| {
            water_meshes(cw)
                .iter()
                .flat_map(|(m, _)| attr(m, Mesh::ATTRIBUTE_UV_1))
                .filter(|u| u[1] > 0.0)
                .count()
        };
        let mut lay = fall_pair(2, "river_straight");
        assert!(sheet(&lay[&ax(1, 0)]) > 0, "the pool side draws the sheet");
        assert!(pool(&lay[&ax(1, 0)]) > 0);
        assert_eq!(sheet(&lay[&ax(0, 0)]), 0, "the lip side draws none");
        without_falls(&mut lay);
        for cw in lay.values() {
            assert_eq!((sheet(cw), pool(cw)), (0, 0), "{:?}", cw.tile);
        }
        assert!(!lay[&ax(0, 0)].exposed.contains(&0) && !lay[&ax(1, 0)].exposed.contains(&3));
    }

    /// `CellWater::falls` and `hex_core::river::orient`'s `Rivers::falls` describe the same seams.
    #[test]
    fn cell_falls_match_the_core_waterfalls() {
        let tiles = [
            river("river_straight", (0, 0), 0, 3),
            river("river_straight", (1, 0), 0, 1),
            river("river_straight", (2, 0), 0, 1),
            river("river_straight", (3, 0), 0, 0),
        ];
        let cat = cat();
        let lay = layout(&tiles, &cat, |_| true);
        let from_cells: BTreeSet<(Axial, u8, u8, bool)> = lay
            .iter()
            .flat_map(|(c, cw)| cw.falls.iter().map(move |f| (*c, f.edge, f.drop, f.upper)))
            .collect();
        let river_tiles: Vec<RiverTile> = tiles
            .iter()
            .map(|t| RiverTile {
                cell: t.cell,
                def: cat.get(&t.tile).unwrap(),
                rotation: t.rotation,
                level: t.level,
            })
            .collect();
        let core = river::orient(&river_tiles).expect("downhill");
        assert_eq!(core.falls.len(), 2);
        let from_core: BTreeSet<(Axial, u8, u8, bool)> = core
            .falls
            .iter()
            .flat_map(|w| {
                [
                    (w.upper, w.edge, w.drop, true),
                    (w.lower, opposite_edge(w.edge), w.drop, false),
                ]
            })
            .collect();
        assert_eq!(from_cells, from_core);
    }

    /// An uphill network (falls facing both ways) flows as the same tiles would at level 0.
    #[test]
    fn an_uphill_network_flows_as_if_flat() {
        let at = |levels: [u8; 3]| -> Vec<PlayTile> {
            (0..3)
                .map(|q| river("river_straight", (q, 0), 0, levels[q as usize]))
                .collect()
        };
        assert_eq!(
            oriented_rivers(&at([0, 1, 0]), &cat()),
            oriented_rivers(&at([0, 0, 0]), &cat())
        );
    }

    /// The proof world (rivers, a bridge, coasts and ocean, all at level 0) keeps its cut faces
    /// and has no falls.
    #[test]
    fn a_level_zero_world_lays_out_exactly_as_before() {
        let cat = cat();
        let mut world = World::new(1);
        for (q, r, id, steps) in PROOF {
            let def = cat.get(id).unwrap();
            let cell = ax(q, r);
            let legal = world.legal_orientations(&cat, cell, def);
            world.place(&cat, cell, id, legal[steps]).unwrap();
        }
        let tiles: Vec<PlayTile> = world
            .tiles()
            .iter()
            .map(|(c, t)| PlayTile {
                cell: *c,
                tile: t.def.clone(),
                rotation: t.rotation,
                level: t.level,
            })
            .collect();
        let lay = layout(&tiles, &cat, |_| false);
        let want: [((i32, i32), Vec<u8>); 9] = [
            ((-3, 3), vec![2]),
            ((-2, 2), vec![]),
            ((-1, 0), vec![4]),
            ((-1, 1), vec![]),
            ((1, 0), vec![2]),
            ((1, 1), vec![0, 1, 2, 3]),
            ((2, -1), vec![5]),
            ((2, 0), vec![0, 1]),
            ((3, -1), vec![0, 1, 4, 5]),
        ];
        assert_eq!(lay.len(), want.len());
        for ((q, r), exposed) in want {
            let cw = &lay[&ax(q, r)];
            assert_eq!(cw.exposed, exposed, "({q}, {r})");
            assert!(cw.falls.is_empty(), "({q}, {r}): {:?}", cw.falls);
            assert_eq!(cw.field().level(), WATER_Y);
        }
    }

    /// The proof layout (docs/checkpoints/water-play/water-play.json) is legal in placement order.
    #[test]
    fn proof_layout_places_legally() {
        let cat = catalog();
        let mut world = World::new(1);
        for (q, r, id, steps) in PROOF {
            let def = cat.get(id).unwrap();
            let cell = Axial::new(q, r);
            let legal = world.legal_orientations(&cat, cell, def);
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
                level: t.level,
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
                            if !sockets.edges_compatible(a.edge(w, ra), 0, b.edge(back, rb), 0) {
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
        let (mut selected, mut hovered, mut rotation) =
            ("meadow".to_owned(), None, hex_core::Orientation::default());
        let snap = |world: &World,
                    selected: &str,
                    hovered: Option<Axial>,
                    rotation: &mut hex_core::Orientation| {
            if let Some(c) = hovered.filter(|&c| world.is_slot(c))
                && let Some(k) = world.auto_orient(&cat, c, cat.get(selected).unwrap())
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
                        .and_then(|c| {
                            world
                                .cycle_group(&cat, c, &[def], (&def.id, rotation), dir)
                                .map(|(_, o)| o)
                        })
                        .unwrap_or(match dir {
                            hex_core::RotateDir::Forward => hex_core::Orientation::new(
                                (rotation.rotation + 1) % 6,
                                rotation.level,
                            ),
                            hex_core::RotateDir::Back => hex_core::Orientation::new(
                                (rotation.rotation + 5) % 6,
                                rotation.level,
                            ),
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
                level: t.level,
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
            let legal: Vec<u8> = world
                .legal_orientations(&cat, cell, def)
                .iter()
                .map(|o| o.rotation)
                .collect();
            assert!(legal.contains(&want), "{id} at ({q}, {r}): legal {legal:?}");
            let mut rot = world.auto_orient(&cat, cell, def).unwrap();
            let mut steps = 0;
            while !def.same_orientation(rot.rotation, want) {
                rot = world
                    .cycle_group(
                        &cat,
                        cell,
                        &[def],
                        (&def.id, rot),
                        hex_core::RotateDir::Forward,
                    )
                    .map(|(_, o)| o)
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
        let (mut selected, mut hovered, mut rotation) =
            ("meadow".to_owned(), None, hex_core::Orientation::default());
        let n = |cmd: &serde_json::Value, k: &str| i32::try_from(cmd[k].as_i64().unwrap()).unwrap();
        for cmd in &script {
            match cmd["cmd"].as_str().unwrap() {
                "select_tile" => selected = cmd["id"].as_str().unwrap().to_owned(),
                "hover" => {
                    let c = Axial::new(n(cmd, "q"), n(cmd, "r"));
                    hovered = Some(c);
                    if let Some(k) = world.auto_orient(&cat, c, cat.get(&selected).unwrap()) {
                        rotation = k;
                    }
                }
                "rotate" => {
                    let c = hovered.unwrap();
                    let def = cat.get(&selected).unwrap();
                    rotation = world
                        .cycle_group(
                            &cat,
                            c,
                            &[def],
                            (&def.id, rotation),
                            hex_core::RotateDir::Forward,
                        )
                        .map(|(_, o)| o)
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
                level: t.level,
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
        let (mut selected, mut hovered, mut rotation) =
            ("meadow".to_owned(), None, hex_core::Orientation::default());
        let n = |cmd: &serde_json::Value, k: &str| i32::try_from(cmd[k].as_i64().unwrap()).unwrap();
        for cmd in script {
            let at = || Axial::new(n(cmd, "q"), n(cmd, "r"));
            match cmd["cmd"].as_str().unwrap() {
                "select_tile" => selected = cmd["id"].as_str().unwrap().to_owned(),
                "hover" => {
                    hovered = Some(at());
                    if let Some(k) = world.auto_orient(cat, at(), cat.get(&selected).unwrap()) {
                        rotation = k;
                    }
                }
                "rotate" => {
                    let def = cat.get(&selected).unwrap();
                    rotation = world
                        .cycle_group(
                            cat,
                            hovered.unwrap(),
                            &[def],
                            (&def.id, rotation),
                            hex_core::RotateDir::Forward,
                        )
                        .map(|(_, o)| o)
                        .unwrap();
                }
                "place" => {
                    if hovered != Some(at()) {
                        hovered = Some(at());
                        rotation = world
                            .auto_orient(cat, at(), cat.get(&selected).unwrap())
                            .unwrap_or_else(|| {
                                panic!("{selected} has no legal rotation at {:?}", at())
                            });
                    }
                    world
                        .place(cat, at(), &selected, rotation)
                        .unwrap_or_else(|e| panic!("{selected} at {:?}: {e:?}", at()));
                }
                "bulldoze" => {
                    world.bulldoze(cat, at()).unwrap();
                }
                "undo" => assert!(world.undo(cat)),
                "redo" => assert!(world.redo(cat)),
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
            if placed < 9 && world.tile(c).is_none() && world.auto_orient(&cat, c, meadow).is_some()
            {
                world
                    .place(&cat, c, "meadow", hex_core::Orientation::new(0, 0))
                    .unwrap();
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
            let legal: Vec<u8> = world
                .legal_orientations(&cat, cell, def)
                .iter()
                .map(|o| o.rotation)
                .collect();
            assert!(
                legal.contains(&rot),
                "{id} at ({q}, {r}) r{rot}: legal {legal:?}"
            );
            let mut k = world.auto_orient(&cat, cell, def).unwrap();
            let mut steps = 0;
            while !def.same_orientation(k.rotation, rot) {
                k = world
                    .cycle_group(
                        &cat,
                        cell,
                        &[def],
                        (&def.id, k),
                        hex_core::RotateDir::Forward,
                    )
                    .map(|(_, o)| o)
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
                level: t.level,
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

    /// Every committed save fixture, as play tiles.
    fn fixture_worlds() -> impl Iterator<Item = Vec<PlayTile>> {
        let cat = catalog();
        [
            &include_bytes!("../../../hex-core/tests/fixtures/save/v1-small-world.hexworld")[..],
            include_bytes!("../../../hex-core/tests/fixtures/save/v2-tilt-chosen.hexworld"),
            include_bytes!("../../../hex-core/tests/fixtures/save/v3-massif.hexworld"),
            include_bytes!("../../../hex-core/tests/fixtures/save/v3-replaced.hexworld"),
        ]
        .into_iter()
        .map(move |bytes| {
            let save = hex_core::SaveData::decode(bytes, &cat).expect("fixture decodes");
            play_tiles(&save.world)
        })
    }

    fn play_tiles(world: &World) -> Vec<PlayTile> {
        world
            .tiles()
            .iter()
            .map(|(c, t)| PlayTile {
                cell: *c,
                tile: t.def.clone(),
                rotation: t.rotation,
                level: t.level,
            })
            .collect()
    }

    /// splitmix64: a seeded stream with no dependency.
    fn next(state: &mut u64) -> u64 {
        *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    const RIVER_TILES: [&str; 9] = [
        "river_straight",
        "river_bend",
        "river_bend_60",
        "river_fork",
        "river_spring",
        "river_mouth",
        "river_mouth_l",
        "river_road_bridge",
        "river_rail_bridge",
    ];

    /// `n` worlds grown by legal level-0 placement of river tiles around the start tile, each up
    /// to 24 river tiles, seeded from `seed`.
    fn random_flat_river_worlds(n: usize, seed: u64) -> impl Iterator<Item = Vec<PlayTile>> {
        let cat = catalog();
        let mut state = seed;
        (0..n).map(move |_| {
            let mut world = World::new(next(&mut state));
            for _ in 0..120 {
                if world.tiles().len() > 24 {
                    break;
                }
                let frontier = world.frontier();
                let cell = frontier[(next(&mut state) % frontier.len() as u64) as usize];
                let id = RIVER_TILES[(next(&mut state) % 9) as usize];
                let def = cat.get(id).unwrap();
                let legal: Vec<_> = world
                    .legal_orientations(&cat, cell, def)
                    .into_iter()
                    .filter(|o| o.level == 0)
                    .collect();
                if legal.is_empty() {
                    continue;
                }
                let o = legal[(next(&mut state) % legal.len() as u64) as usize];
                world.place(&cat, cell, id, o).unwrap();
            }
            play_tiles(&world)
        })
    }

    /// A fork at rotation `k` with an equal straight on each arm, then the same with a spring past
    /// the straights on two arms, and with springs in place of the straights on two arms. Built as
    /// tile lists, away from the start tile, with each arm piece turned to meet its arm.
    fn symmetric_fork_worlds(k: u8) -> Vec<Vec<PlayTile>> {
        let cat = catalog();
        let fork = cat.get("river_fork").unwrap();
        let origin = Axial::new(0, 0);
        let arms: Vec<u8> = (0..6)
            .filter(|&e| fork.edge(e, k).kind == SocketKind::River)
            .collect();
        assert_eq!(arms.len(), 3);
        // `id` at `cell`, turned so its River edge faces back along arm `e`.
        let piece = |cell: Axial, id: &str, e: u8| {
            let def = cat.get(id).unwrap();
            let rotation = (0..6)
                .find(|&r| def.edge(opposite_edge(e), r).kind == SocketKind::River)
                .unwrap();
            PlayTile {
                cell,
                tile: id.into(),
                rotation,
                level: 0,
            }
        };
        let fork_tile = tile(0, 0, "river_fork", k);
        let straights: Vec<PlayTile> = std::iter::once(fork_tile.clone())
            .chain(
                arms.iter()
                    .map(|&e| piece(origin.neighbor(e), "river_straight", e)),
            )
            .collect();
        let mut out = vec![straights.clone()];
        for skip in 0..3 {
            let mut far = straights.clone();
            let mut near = vec![fork_tile.clone()];
            for (i, &e) in arms.iter().enumerate() {
                if i == skip {
                    near.push(piece(origin.neighbor(e), "river_straight", e));
                } else {
                    far.push(piece(origin.neighbor(e).neighbor(e), "river_spring", e));
                    near.push(piece(origin.neighbor(e), "river_spring", e));
                }
            }
            out.push(far);
            out.push(near);
        }
        out
    }

    /// Same keys and ends, the same point counts, and every point within 1e-4 m.
    fn assert_same_paths(
        got: &BTreeMap<(Axial, usize), RiverPath>,
        want: &BTreeMap<(Axial, usize), RiverPath>,
    ) {
        assert_eq!(
            got.keys().collect::<Vec<_>>(),
            want.keys().collect::<Vec<_>>()
        );
        for (k, g) in got {
            let w = &want[k];
            assert_eq!((g.up, g.down), (w.up, w.down), "{k:?}");
            assert_eq!(g.points.len(), w.points.len(), "{k:?}");
            for (a, b) in g.points.iter().zip(&w.points) {
                assert!(a.distance(*b) <= 1e-4, "{k:?}: {a} vs {b}");
            }
        }
    }

    #[test]
    fn orientation_matches_legacy_on_fixtures_and_random_flat_worlds() {
        let cat = catalog();
        let (mut junctions, mut springs, mut mouths, mut bridges, mut reversed) = (0, 0, 0, 0, 0);
        for world in fixture_worlds().chain(random_flat_river_worlds(512, 0xA41)) {
            let got = oriented_rivers(&world, &cat);
            let want = crate::water::legacy::oriented_rivers(&world, &cat);
            assert_same_paths(&got, &want);
            assert_eq!(got, want, "bit-equal world points");
            for ((cell, i), p) in &got {
                junctions += usize::from(p.at_junction());
                springs += usize::from(p.up == RiverEnd::Spring || p.down == RiverEnd::Spring);
                mouths += usize::from(p.up == RiverEnd::Mouth || p.down == RiverEnd::Mouth);
                let t = world.iter().find(|t| t.cell == *cell).unwrap();
                bridges += usize::from(t.tile.contains("bridge"));
                let def = cat.get(&t.tile).unwrap();
                let (a, _, _) = hex_core::river::river_segment_ends(def, t.rotation)[*i];
                reversed += usize::from(a != p.up);
            }
        }
        assert!(
            junctions > 0 && springs > 0 && mouths > 0 && bridges > 0 && reversed > 0,
            "coverage: {junctions} junction arms, {springs} springs, {mouths} mouths, \
             {bridges} bridges, {reversed} reversed"
        );
    }

    #[test]
    fn junction_ties_match_legacy_on_symmetric_forks() {
        let cat = catalog();
        for k in 0..6 {
            for w in symmetric_fork_worlds(k) {
                let got = oriented_rivers(&w, &cat);
                let want = crate::water::legacy::oriented_rivers(&w, &cat);
                assert_same_paths(&got, &want);
                assert_eq!(got, want, "bit-equal world points");
            }
        }
    }
}
