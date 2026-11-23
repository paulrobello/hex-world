use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use bevy::prelude::*;
use hex_core::{
    Axial, HEX_RADIUS, LEVEL_STEP, NetworkKind, ParkPose, TileCatalog, TileDef,
    rail_end::{self, BUFFER_STOP},
};

use crate::{
    boids::plan::cell_at,
    play::{
        PlayTile,
        scatter::{self, Surface, TileScatter, Tri, turn},
    },
    water::tiles::{WATER_Y, WaterKind, water_kind},
};
use hex_render::kit_runtime::piece_rotation;

use super::shape::{Obstacle, PickProp, PieceShape, hull};
use super::tables::{
    CAR_GROUND, CENTRE_OBSTACLES, FOOTPRINT_OBSTACLES, PIECE_POSTS, is_obstacle, prop_kind,
};

/// The walker's body radius (m): how close its centre comes to a wall, a trunk or an edge.
pub const BODY_RADIUS: f32 = 0.3;

/// Deepest water (m) the walker wades into.
pub const MAX_WADE: f32 = 0.6;

/// Highest step (m) the walker climbs in one stride; railings and walls are taller.
pub const MAX_STEP: f32 = 0.4;

/// Longest sub-step (m) a move is checked in.
pub(super) const SUBSTEP: f32 = 0.1;

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
    /// A [`prop_kind`] string for an obstacle, else `water`, `edge` or `steep`.
    pub fn reason(&self) -> &'static str {
        match self {
            Blocked::Edge => "edge",
            Blocked::Water => "water",
            Blocked::Steep => "steep",
            Blocked::Obstacle(p) => prop_kind(p).as_str(),
        }
    }

    pub fn piece(&self) -> Option<&str> {
        match self {
            Blocked::Obstacle(p) => Some(p),
            _ => None,
        }
    }
}

/// A tile definition's walk ground, tile frame: its centre cell's [`Surface`] and, for a
/// footprint tile, each ring member's centre offset from it with that member's surface in its own
/// frame (`scatter::member_tris`), since a surface spans one hex.
pub struct TileSurface {
    pub centre: Arc<Surface>,
    pub ring: Vec<(Vec2, Arc<Surface>)>,
}

impl TileSurface {
    /// Builds a definition's walk ground from its ground pieces' LOD0 triangles (`tris_of`), or
    /// `None` while one is missing.
    pub fn new(
        def: &TileDef,
        catalog: &TileCatalog,
        tris_of: &mut dyn FnMut(&str) -> Option<Vec<(Tri, String)>>,
    ) -> Option<Self> {
        let all = crate::play::slope_ground::tile_ground_tris(def, catalog, tris_of)?;
        if !def.is_mega() {
            return Some(Self {
                centre: Arc::new(Surface::new(all)),
                ring: vec![],
            });
        }
        let ring = hex_core::footprint::members(Axial::ORIGIN)
            .into_iter()
            .skip(1)
            .map(|m| {
                let at = Vec2::from(m.to_world(HEX_RADIUS));
                (at, Arc::new(Surface::new(scatter::member_tris(&all, at))))
            })
            .collect();
        Some(Self {
            centre: Arc::new(Surface::new(scatter::member_tris(&all, Vec2::ZERO))),
            ring,
        })
    }
}

/// One placed tile as ground.
#[derive(Clone)]
struct CellGround {
    tile: String,
    centre: Vec2,
    rotation: u8,
    /// The tile's level height (P4), `level * LEVEL_STEP`: every query adds it to the
    /// tile-frame surface, so a cliff between two levels is a step [`MAX_STEP`] refuses.
    y0: f32,
    /// How far above `y0` the tile's rail stands (P4): a viaduct's deck, `deck * LEVEL_STEP`; 0 on
    /// every other tile. Trains ride `y0 + rail_lift` ([`WalkGround::level_y`]).
    rail_lift: f32,
    /// `None`: a placeholder tile, flat at `y0`.
    surface: Option<Arc<Surface>>,
    /// The tile carries a road or a parking spot: a car's wheel over any other tile (a tunnel's
    /// cover past the arch) rides the car's own tile ([`WalkGround::ride_car`]).
    car_paths: bool,
    water: bool,
    /// River or spring water: deeper than [`MAX_WADE`] is crossed at that depth, not refused.
    ford: bool,
    /// [`ride_max`] of the tile; `f32::MIN` for a placeholder.
    ride_max: f32,
}

/// How far (m) from a road lane's centreline a car's wheels reach: the widest lane offset
/// (`hex_sim::MAX_LANE_OFFSET`, 1.5 m) and rounded corners, short of the kerbs at 3 m. A Y
/// junction's right turns swing farther (`hex_sim::geom::TURN_SWING`), over the asphalt of the
/// kerb fillet between two arms, which lies at the bed height below the crown, so it never sets
/// the ride height.
pub const ROAD_CORRIDOR: f32 = 2.5;

/// How far (m) from the poses of a parking path a car's wheels reach.
pub const PARKING_CORRIDOR: f32 = 2.0;

/// Ground at most this far (m) above a car's floor is flat for it: lane paint and the crown.
pub const RIDE_FLAT: f32 = 0.01;

/// Spacing (m) of the samples [`ride_max`] takes.
const RIDE_STEP: f32 = 0.25;

/// The highest [`CAR_GROUND`] on `def`'s car paths, tile frame: within [`ROAD_CORRIDOR`] of each
/// road lane's centreline and [`PARKING_CORRIDOR`] of each parking path, sampled every 0.25 m.
/// Built once per tile id (a few thousand surface samples); [`WalkGround::ride`] skips every tile
/// whose paths stay flat.
pub fn ride_max(def: &TileDef, surface: &Surface) -> f32 {
    let mut paths: Vec<(Vec<Vec2>, f32)> = def
        .networks
        .iter()
        .zip(scatter::lanes(def))
        .filter(|(n, _)| n.kind == NetworkKind::Road)
        .map(|(_, (line, _))| (line, ROAD_CORRIDOR))
        .collect();
    for spot in &def.parking {
        let at = Vec2::from(spot.at);
        let pts = |v: &[ParkPose]| v.iter().map(|p| Vec2::from(p.at)).collect::<Vec<_>>();
        let mut into = pts(&spot.enter);
        into.push(at);
        for tail in [&spot.back, &spot.back_alt] {
            let mut line = vec![at];
            line.extend(pts(tail));
            let mut leave = vec![*line.last().unwrap()];
            leave.extend(pts(&spot.leave));
            paths.push((line, PARKING_CORRIDOR));
            paths.push((leave, PARKING_CORRIDOR));
        }
        paths.push((into, PARKING_CORRIDOR));
    }
    let mut top = f32::MIN;
    for (line, r) in &paths {
        let (lo, hi) = line.iter().fold((Vec2::MAX, Vec2::MIN), |(lo, hi), p| {
            (lo.min(*p), hi.max(*p))
        });
        let (lo, hi) = (lo - Vec2::splat(*r), hi + Vec2::splat(*r));
        let n = ((hi - lo) / RIDE_STEP).ceil().as_uvec2();
        for j in 0..=n.y {
            for i in 0..=n.x {
                let p = lo + Vec2::new(i as f32, j as f32) * RIDE_STEP;
                if scatter::polyline_distance(p, line) <= *r
                    && let Some((h, slot)) = surface.at(p.x, p.y)
                    && CAR_GROUND.contains(&slot)
                {
                    top = top.max(h);
                }
            }
        }
    }
    top
}

/// A move's result: where the feet end (y on the ground) and why it stopped short, if it did.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub pos: Vec3,
    pub blocked: Option<Blocked>,
}

/// Where a portal's face stands inside its tile edge, by tunnel kind: the road portal's 2.2 m and
/// the rail portal's 6 m (RL-Q1 O2). A bore corridor starts at the face.
const fn face_inset(kind: NetworkKind) -> f32 {
    match kind {
        NetworkKind::Rail => 6.0,
        _ => 2.2,
    }
}

/// How far (m) in front of a portal's face the bore's floor override starts: the coping, drip edge
/// and ring stand out from the face over the opening, and a heightfield reads their top as ground.
const BORE_LEAD: f32 = 1.0;

/// Half the width (m) of the floor a walker may use in a bore, by tunnel kind: the arch's half
/// width at the floor less the wall lining, the lamp stations and the walker's own radius.
const fn corridor_half(kind: NetworkKind) -> f32 {
    match kind {
        NetworkKind::Rail => 2.1,
        _ => 3.4,
    }
}

/// The floor of a complete tunnel (P4 open tunnels): from one portal's face to the other's, the
/// walker, the cars and the trains stand on the portal level's floor, not on the cover's top.
/// Outside `half` of its axis the ground is the cover's top as before, so the walls still block.
#[derive(Clone, Debug)]
struct Bore {
    a: Vec2,
    b: Vec2,
    half: f32,
    floor: f32,
}

impl Bore {
    /// Whether `p` is on the floor: between the two faces and within `half` of the axis.
    fn holds(&self, p: Vec2) -> bool {
        let axis = self.b - self.a;
        let len = axis.length().max(1e-4);
        let d = axis / len;
        let rel = p - self.a;
        let along = rel.dot(d);
        (0.0..=len).contains(&along) && (rel.x * d.y - rel.y * d.x).abs() <= self.half
    }
}

/// The walkable world, rebuilt by [`sync`](super::sync::sync) whenever the drawn tiles, their scatter or the kit change.
#[derive(Resource, Default)]
pub struct WalkGround {
    cells: HashMap<Axial, CellGround>,
    /// The complete tunnels' floors ([`WalkGround::set_tunnels`]).
    bores: Vec<Bore>,
    pub(super) obstacles: HashMap<Axial, Vec<Obstacle>>,
    /// The same obstacles as volumes for photo mode's click to focus.
    pub(super) props: Vec<PickProp>,
    /// The farm herds' animals (`farm`): they block and are clicked where they are drawn, so they
    /// are kept apart from the static obstacles and moved by [`WalkGround::pose_herd`].
    herd: Vec<HerdBody>,
    /// The boats' pieces (`boats`), shaped once per rebuild.
    boat_shapes: HashMap<String, Arc<PieceShape>>,
    /// The drawn boats and the landing stages' bollards, where `boats::draw` last put them.
    boats: Vec<(Obstacle, Option<PickProp>)>,
    /// The landing stages' decks: walker ground ([`STAGE_DECK`]), placed by `boats::draw`.
    decks: Vec<Transform>,
    /// Bumped on every rebuild.
    pub revision: u64,
}

/// A landing stage deck in its frame: x and z ranges and the top's world height (the runtime notes:
/// 0.45 m over the water at rest).
pub const STAGE_DECK: ([f32; 2], [f32; 2], f32) = ([-0.75, 1.40], [-1.4, 1.4], WATER_Y + 0.45);

/// The stage's bollards (outer posts, 0.55 m over the deck: taller than [`MAX_STEP`]), stage frame.
pub const STAGE_BOLLARDS: [[f32; 2]; 3] = [[1.3, -1.25], [1.3, 0.0], [1.3, 1.25]];

const BOLLARD_HALF: f32 = 0.09;

/// One wandering animal as walk mode sees it: its shape, its tile's rest transform, and its
/// footprint and pickable volume at its latest drawn pose.
#[derive(Clone)]
struct HerdBody {
    cell: Axial,
    slot: usize,
    piece: String,
    shape: Arc<PieceShape>,
    root: Transform,
    obstacle: Obstacle,
    pick: Option<PickProp>,
}

impl HerdBody {
    fn place(&mut self, local: &Transform) {
        let t = self.root * *local;
        self.obstacle.poly = place_hull(&self.shape.hull, &t);
        self.pick = PickProp::new(&self.piece, &self.shape, &t);
    }
}

/// The tile root's rest transform (`play::spawn_tile`), on its level.
fn root_transform(t: &PlayTile) -> Transform {
    let [x, z] = t.cell.to_world(HEX_RADIUS);
    Transform::from_xyz(x, f32::from(t.level) * LEVEL_STEP, z)
        .with_rotation(piece_rotation(t.rotation))
}

fn place_hull(hull: &[Vec2], t: &Transform) -> Vec<Vec2> {
    hull.iter()
        .map(|p| t.transform_point(Vec3::new(p.x, 0.0, p.y)).xz())
        .collect()
}

impl WalkGround {
    /// Builds the ground of the drawn `tiles` (minus `hidden` ones). `surface_of` gives a tile
    /// definition's ground surface (tile frame) and its [`ride_max`], or `None` for a placeholder;
    /// `shape_of` a piece's collider footprint and pickable volume in its own frame; `scatter` the
    /// drawn scatter by cell.
    pub fn build(
        tiles: &[PlayTile],
        catalog: &TileCatalog,
        world_seed: u64,
        hidden: &[Axial],
        surface_of: &mut dyn FnMut(&TileDef) -> Option<(Arc<TileSurface>, f32)>,
        shape_of: &mut dyn FnMut(&str) -> PieceShape,
        scatter: &BTreeMap<Axial, TileScatter>,
    ) -> Self {
        let mut out = Self::default();
        let tile_at: HashMap<Axial, (&TileDef, u8)> = tiles
            .iter()
            .filter_map(|t| Some((t.cell, (catalog.get(&t.tile)?, t.rotation))))
            .collect();
        let at = crate::play::covering(|c: Axial| tile_at.get(&c).copied());
        let levels: HashMap<Axial, u8> = tiles.iter().map(|t| (t.cell, t.level)).collect();
        let level = |c: Axial| levels.get(&c).copied().unwrap_or(0);
        for piece in ["boat_rowing", "boat_sail", "landing_stage"] {
            out.boat_shapes
                .insert(piece.to_owned(), Arc::new(shape_of(piece)));
        }
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
            let (surface, ride_max) = surface_of(def).unzip();
            let ground = CellGround {
                tile: t.tile.clone(),
                centre: Vec2::new(x, z),
                rotation: t.rotation,
                y0: f32::from(t.level) * LEVEL_STEP,
                rail_lift: f32::from(def.edges.iter().map(|e| e.deck()).max().unwrap_or(0))
                    * LEVEL_STEP,
                surface: surface.as_ref().map(|s| s.centre.clone()),
                car_paths: !def.parking.is_empty()
                    || def.networks.iter().any(|n| n.kind == NetworkKind::Road),
                ride_max: ride_max.unwrap_or(f32::MIN),
                water: water_kind(def, t.rotation).is_some(),
                ford: matches!(
                    water_kind(def, t.rotation),
                    Some(WaterKind::River | WaterKind::Spring(_))
                ),
            };
            if def.is_mega() {
                // Each ring cell stands on its own member's ground; until the piece loads, flat at
                // the footprint's level.
                for m in hex_core::footprint::members(t.cell).into_iter().skip(1) {
                    let mid = Vec2::from(m.to_world(HEX_RADIUS));
                    let d = mid - ground.centre;
                    let l = turn((6 - t.rotation % 6) % 6, [d.x, 0.0, d.y]);
                    let own = surface.as_ref().and_then(|s| {
                        s.ring
                            .iter()
                            .find(|(o, _)| o.distance(Vec2::new(l[0], l[2])) < 1.0)
                            .map(|(_, s)| s.clone())
                    });
                    out.cells.insert(
                        m,
                        CellGround {
                            centre: mid,
                            surface: own,
                            ride_max: f32::MIN,
                            ..ground.clone()
                        },
                    );
                }
            }
            out.cells.insert(t.cell, ground);
            let root = root_transform(t);
            let mut obs = vec![];
            // A footprint's ring scatter, by the ring cell it stands on, so a body beside that
            // cell meets it ([`WalkGround::obstacle`] looks one cell around).
            let mut ring_obs: HashMap<Axial, Vec<Obstacle>> = HashMap::new();
            let mut place =
                |obs: &mut Vec<Obstacle>, piece: &str, sh: &Arc<PieceShape>, t: &Transform| {
                    for (poly, part) in sh.footprints() {
                        obs.push(Obstacle {
                            piece: piece.to_owned(),
                            poly: place_hull(poly, t),
                        });
                        out.props.extend(match part {
                            None => PickProp::new(piece, sh, t),
                            Some(i) => PickProp::new_part(piece, sh, t, i),
                        });
                    }
                    for (_, corners) in PIECE_POSTS.iter().filter(|(p, _)| *p == piece) {
                        let poly = hull(&corners.map(Vec2::from));
                        obs.push(Obstacle {
                            piece: piece.to_owned(),
                            poly: place_hull(&poly, t),
                        });
                    }
                };
            let herd = crate::farm::herd_slots(def);
            for (slot, (h, piece)) in def.hero_pieces(t.cell).enumerate() {
                if is_obstacle(piece) {
                    let local = Transform::from_xyz(h.at[0], h.y, h.at[1])
                        .with_rotation(Quat::from_rotation_y(-h.yaw_deg.to_radians()));
                    if herd.contains(&slot) {
                        let mut b = HerdBody {
                            cell: t.cell,
                            slot,
                            piece: piece.to_owned(),
                            shape: shape(piece),
                            root,
                            obstacle: Obstacle {
                                piece: piece.to_owned(),
                                poly: vec![],
                            },
                            pick: None,
                        };
                        b.place(&local);
                        out.herd.push(b);
                        continue;
                    }
                    place(&mut obs, piece, &shape(piece), &(root * local));
                }
            }
            if let Some(s) = scatter.get(&t.cell).filter(|s| s.tile == t.tile) {
                for inst in &s.layout.instances {
                    let rule = inst.rule as usize;
                    let piece = s.piece_of(inst);
                    if s.drawn[rule] && is_obstacle(piece) {
                        let placed = root * inst.transform();
                        let into = if def.is_mega() {
                            ring_obs
                                .entry(cell_at(placed.translation.xz()))
                                .or_default()
                        } else {
                            &mut obs
                        };
                        place(into, piece, &shape(piece), &placed);
                    }
                }
            }
            // The buffer stops at the tile's open rail ends (`play::respawn_pairs` draws them).
            for s in rail_end::open_stops(def, t.cell, t.rotation, &at) {
                let local = crate::play::buffer_stop_local(&s);
                place(&mut obs, BUFFER_STOP, &shape(BUFFER_STOP), &(root * local));
            }
            // The field boundaries this tile owns (`play::respawn_pairs` draws them), and the
            // hedgerow oaks they carry.
            for f in crate::play::boundaries::placements(world_seed, t.cell, &at, &level) {
                if is_obstacle(f.piece) {
                    let local = crate::play::boundaries::transform(&f);
                    place(&mut obs, f.piece, &shape(f.piece), &(root * local));
                }
            }
            for (centre, part, corners) in CENTRE_OBSTACLES {
                if def.center.as_ref().is_some_and(|c| c.as_str() == *centre) {
                    let poly: Vec<Vec2> = corners.iter().map(|c| Vec2::from(*c)).collect();
                    obs.push(Obstacle {
                        piece: (*part).to_owned(),
                        poly: place_hull(&poly, &root),
                    });
                }
            }
            for (tile, part, parts) in FOOTPRINT_OBSTACLES {
                if def.id == *tile {
                    for corners in *parts {
                        let poly: Vec<Vec2> = corners.iter().map(|c| Vec2::from(*c)).collect();
                        obs.push(Obstacle {
                            piece: (*part).to_owned(),
                            poly: place_hull(&poly, &root),
                        });
                    }
                }
            }
            if !obs.is_empty() {
                out.obstacles.insert(t.cell, obs);
            }
            for (cell, o) in ring_obs {
                out.obstacles.entry(cell).or_default().extend(o);
            }
        }
        out
    }

    /// The tile under world XZ `p`: its cell and id.
    pub fn tile_at(&self, p: Vec2) -> Option<(Axial, &str)> {
        let cell = cell_at(p);
        self.cells.get(&cell).map(|g| (cell, g.tile.as_str()))
    }

    /// Ground height under `p`, the water depth over it and whether that water is a ford, or why
    /// nothing stands there.
    fn sample_ford(&self, p: Vec2) -> Result<(f32, f32, bool), Blocked> {
        let Some(g) = self.cells.get(&cell_at(p)) else {
            return Err(Blocked::Edge);
        };
        if self.on_deck(p) {
            return Ok((STAGE_DECK.2, 0.0, false));
        }
        if let Some(floor) = self.bore_floor(p) {
            return Ok((floor, 0.0, false));
        }
        let d = p - g.centre;
        let l = turn((6 - g.rotation % 6) % 6, [d.x, 0.0, d.y]);
        let h = g.y0
            + match &g.surface {
                Some(s) => s.at(l[0], l[2]).ok_or(Blocked::Edge)?.0,
                None => 0.0,
            };
        let depth = if g.water { (WATER_Y - h).max(0.0) } else { 0.0 };
        Ok((h, depth, g.ford))
    }

    /// Ground height under `p` and the water depth over it, or why nothing stands there.
    fn sample(&self, p: Vec2) -> Result<(f32, f32), Blocked> {
        self.sample_ford(p).map(|(h, depth, _)| (h, depth))
    }

    /// Where the feet stand at `p` (the bed under shallow water, [`MAX_WADE`] under the surface
    /// in a deeper ford), or why nothing stands there: [`Blocked::Edge`] or [`Blocked::Water`].
    pub fn ground(&self, p: Vec2) -> Result<f32, Blocked> {
        let (h, depth, ford) = self.sample_ford(p)?;
        if depth > MAX_WADE {
            if ford {
                return Ok(WATER_Y - MAX_WADE);
            }
            return Err(Blocked::Water);
        }
        Ok(h)
    }

    /// Where a wheel rests at `p` when it never sinks below `floor` (cars: the road crown): the
    /// higher of `floor` and the ground there when that is [`CAR_GROUND`]. A tile whose car paths never rise more than
    /// [`RIDE_FLAT`] above `floor` ([`ride_max`], cached per tile id) answers `floor` after one
    /// cell lookup: every flat road tile and the river road bridge. Only a tile with a ramp or a
    /// raised deck searches its surface. `floor` is over the tile's level (P4): the answer adds
    /// the level's height, so a car on a raised road rides its top and one on the road ramp
    /// climbs its draped asphalt. Off the drawn tiles, `floor`.
    pub fn ride(&self, p: Vec2, floor: f32) -> f32 {
        let Some(g) = self.cells.get(&cell_at(p)) else {
            return floor;
        };
        g.y0 + Self::ride_on(g, p, floor)
    }

    /// Sets the complete tunnels' floors from the world's tunnels (`hex_core::tunnels`): each runs
    /// from one portal's face to the other's along the line between their cells.
    pub fn set_tunnels(&mut self, tunnels: &[hex_core::Tunnel]) {
        self.bores = tunnels
            .iter()
            .map(|t| {
                let (ca, cb) = (t.a.to_world(HEX_RADIUS), t.b.to_world(HEX_RADIUS));
                let (ca, cb) = (Vec2::new(ca[0], ca[1]), Vec2::new(cb[0], cb[1]));
                let d = (cb - ca).normalize_or_zero();
                let reach = HEX_RADIUS * 0.866_025_4 - face_inset(t.kind) - BORE_LEAD;
                Bore {
                    a: ca + d * reach,
                    b: cb - d * reach,
                    half: corridor_half(t.kind),
                    floor: f32::from(t.level) * LEVEL_STEP,
                }
            })
            .collect();
    }

    /// The floor height of the tunnel `p` is inside, if any.
    fn bore_floor(&self, p: Vec2) -> Option<f32> {
        self.bores.iter().find(|b| b.holds(p)).map(|b| b.floor)
    }

    /// The height the rail under `p` stands at (P4): the level of the tile under `p`, plus a
    /// viaduct's deck; the floor of a tunnel when `p` is inside one; `0.0` on the ground and off
    /// the drawn tiles. Trains ride the rail top over it.
    pub fn level_y(&self, p: Vec2) -> f32 {
        if let Some(floor) = self.bore_floor(p) {
            return floor;
        }
        self.cells
            .get(&cell_at(p))
            .map_or(0.0, |g| g.y0 + g.rail_lift)
    }

    /// [`WalkGround::ride`] in the tile's own frame, over its level.
    fn ride_on(g: &CellGround, p: Vec2, floor: f32) -> f32 {
        let Some(s) = g
            .surface
            .as_ref()
            .filter(|_| g.ride_max > floor + RIDE_FLAT)
        else {
            return floor;
        };
        let d = p - g.centre;
        let l = turn((6 - g.rotation % 6) % 6, [d.x, 0.0, d.y]);
        match s.at(l[0], l[2]) {
            Some((h, slot)) if CAR_GROUND.contains(&slot) => h.max(floor),
            _ => floor,
        }
    }

    /// [`WalkGround::ride`] for a wheel at `p` of a car whose centre is at `centre`. A wheel over
    /// a tile with no road or parking (a tunnel's cover, past the arch) or off the drawn tiles (a
    /// raised road's open end) rides `floor` on the level of the tile under the centre: a car
    /// entering a tunnel stays on its road while it fades, rather than climbing onto the cover one
    /// level up, and one at a raised dead end does not tip toward the ground.
    pub fn ride_car(&self, p: Vec2, floor: f32, centre: Vec2) -> f32 {
        match self.cells.get(&cell_at(p)) {
            Some(g) if g.car_paths => g.y0 + Self::ride_on(g, p, floor),
            _ => {
                let level = self
                    .bore_floor(centre)
                    .or_else(|| self.cells.get(&cell_at(centre)).map(|c| c.y0))
                    .unwrap_or(0.0);
                level + floor
            }
        }
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
            .chain(self.herd.iter().filter_map(|b| b.pick.as_ref()))
            .chain(self.boats.iter().filter_map(|b| b.1.as_ref()))
            .filter_map(|p| p.hit(ray).map(|t| (t, p.piece.as_str())))
            .filter(|(t, _)| *t <= max)
            .min_by(|a, b| a.0.total_cmp(&b.0))
    }

    /// No drawn tile is ground yet (another scenario, or the kit still loading).
    /// Every collider footprint, for the debug overlay.
    pub fn obstacles(&self) -> impl Iterator<Item = &Obstacle> {
        self.obstacles
            .values()
            .flatten()
            .chain(self.herd.iter().map(|b| &b.obstacle))
            .chain(self.boats.iter().map(|b| &b.0))
    }

    /// Moves the herd animals to their drawn poses: (cell, hero slot, transform under the tile
    /// root). An animal not listed keeps its last pose.
    pub fn pose_herd(&mut self, posed: &[(Axial, usize, Transform)]) {
        for (cell, slot, local) in posed {
            if let Some(b) = self
                .herd
                .iter_mut()
                .find(|b| b.cell == *cell && b.slot == *slot)
            {
                b.place(local);
            }
        }
    }

    /// Moves the boats to their drawn poses (piece, world transform) and the landing stages to
    /// theirs: each boat blocks by its collider footprint and is clicked as its volume, each deck
    /// is ground and its bollards block.
    pub fn pose_boats(&mut self, boats: &[(&str, Transform)], stages: &[Transform]) {
        self.boats.clear();
        for (piece, t) in boats {
            let Some(shape) = self.boat_shapes.get(*piece) else {
                continue;
            };
            self.boats.push((
                Obstacle {
                    piece: (*piece).to_owned(),
                    poly: place_hull(&shape.hull, t),
                },
                PickProp::new(piece, shape, t),
            ));
        }
        for t in stages {
            let pick = self
                .boat_shapes
                .get("landing_stage")
                .and_then(|s| PickProp::new("landing_stage", s, t));
            for (k, [x, z]) in STAGE_BOLLARDS.iter().enumerate() {
                let h = BOLLARD_HALF;
                let poly = [[-h, -h], [h, -h], [h, h], [-h, h]]
                    .map(|[a, b]| Vec2::new(x + a, z + b))
                    .to_vec();
                self.boats.push((
                    Obstacle {
                        piece: "landing_stage".to_owned(),
                        poly: place_hull(&poly, t),
                    },
                    if k == 0 { pick.clone() } else { None },
                ));
            }
        }
        self.decks = stages.to_vec();
    }

    /// Whether `p` stands on a landing stage's deck.
    pub fn on_deck(&self, p: Vec2) -> bool {
        let ([x0, x1], [z0, z1], _) = STAGE_DECK;
        self.decks.iter().any(|t| {
            let d = p - t.translation.xz();
            let (xa, za) = (t.rotation * Vec3::X, t.rotation * Vec3::Z);
            let (x, z) = (d.dot(xa.xz()), d.dot(za.xz()));
            (x0..=x1).contains(&x) && (z0..=z1).contains(&z)
        })
    }

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
            .chain(self.herd.iter().map(|b| &b.obstacle))
            .chain(self.boats.iter().map(|b| &b.0))
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
        // The scatter on a cover cell stands on its top, over the bore's floor.
        if self.bore_floor(p).is_none()
            && let Some(o) = self.obstacle(p)
        {
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
