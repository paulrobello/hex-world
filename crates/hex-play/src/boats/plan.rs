//! Boats' plan and fixed-tick motion (owner D4, 2026-10-20), with no Bevy systems: which boats a
//! world holds, where they moor or anchor, the routes they take and how they move along them.
//!
//! - **Water.** The fish regions ([`FishWorld`]): water tiles joined by continuous water.
//! - **Rowing boats** row river runs: the oriented river centre lines ([`oriented_rivers`]) chained
//!   across plain river tiles (`river_straight`, `river_bend`). A run ends at a bridge tile (a River
//!   network beside a Road or Rail one: the 6 m sail mast and D4 keep boats off the arch), a spring,
//!   a mouth or an open end, and the boat turns back [`ROW_END_MARGIN`] short of it, on a half
//!   circle of [`ROW_LANE`] radius. It keeps to the right of its heading, so turning back puts it on
//!   the other half of the channel. Each run holds at most one moving rowing boat.
//! - **Sailing boats** sail closed routes over sea water (ocean tiles, and coast and mouth tiles at
//!   least [`SAIL_S`] seaward of the coastline: the 0.45 m of water a 0.32 m keel needs, runtime
//!   notes), every point of a route with a [`SAIL_CLEAR`] disc of such water round it.
//! - **Counts.** Per region: `(river cells + 1) / 2` rowing boats and `sea cells / 2` sailing boats,
//!   taken largest region first (ties to the lowest cell) up to [`MAX_ROWING`] and [`MAX_SAILING`]
//!   world-wide. Of each kind the second boat world-wide moors (a rowing boat at a landing stage on
//!   a plain river tile) or anchors (a sailing boat off a beach); the rest move. A boat that finds
//!   no route moors instead where it can, and is dropped where it cannot.
//! - **Determinism.** The plan is a function of the tiles and the world seed (SplitMix64 draws).
//!   The tick state is arc lengths, phases and offsets advanced with `+ - * /` alone, so a world
//!   runs the same on every platform. Boats are re-derived from the view and never saved.
use std::collections::{BTreeMap, BTreeSet};

use bevy::math::Vec2;
use hex_core::{Axial, HEX_RADIUS, NetworkKind, TileCatalog, TileDef, cell_seed};

use crate::{
    boids::plan::{FishWorld, Rng, cell_at},
    play::PlayTile,
    water::tiles::{CellWater, Coastline, RiverEnd, WaterKind, oriented_rivers},
};

/// Seconds per tick: the 20 Hz fixed tick the cars, trains and herds run on.
pub const TICK_SECONDS: f32 = 1.0 / 20.0;
pub const MAX_ROWING: usize = 3;
pub const MAX_SAILING: usize = 3;
/// A rowing boat's lane: its centre line this far right of the river centre line, so two boats
/// rowing past each other keep their own halves of the 8 m channel (runtime notes).
pub const ROW_LANE: f32 = 1.9;
/// How far short of a run's end a rowing boat turns: its half length plus the turn's reach past
/// the turning point (a [`ROW_LANE`] half circle), so neither hull nor oars enter the next tile.
pub const ROW_END_MARGIN: f32 = 4.2;
/// Shortest run a rowing boat rows (the turn needs room at both ends).
pub const MIN_RUN: f32 = 12.0;
pub const ROW_SPEED: f32 = 1.1;
/// Metres per oar stroke: one catch about every 2.5 s at [`ROW_SPEED`].
pub const STROKE_M: f32 = 2.8;
/// A moving boat passing a moored one within this far along the run trails its oars and keeps to
/// the far half of the channel (a boat with its oars out does not fit past a mooring).
pub const PASS_ZONE: f32 = 7.5;
/// Lateral offset (m, from the centre line toward the far side) of a boat passing a mooring.
pub const PASS_LANE: f32 = 1.5;
/// Seconds to swing the oars between rowing and trailing.
pub const TRAIL_SECONDS: f32 = 1.0;
/// Lane change speed (m/s) toward a new lateral offset.
pub const LANE_SPEED: f32 = 0.6;
pub const SAIL_SPEED: f32 = 2.4;
/// Least seaward distance (m) of sailing water on coast and mouth tiles (runtime notes: depth at
/// least 0.45 m, which the coast profile reaches at s = 3.6).
pub const SAIL_S: f32 = 3.6;
/// Radius (m) of the disc of sailing water every route point keeps round it: half the boat's
/// 6.13 m overall length and a margin. It does not depend on the heading.
pub const SAIL_CLEAR: f32 = 3.3;
/// Least distance (m) from a sailing route to an anchored boat: two hull lengths.
pub const ANCHOR_CLEAR: f32 = 12.0;
/// Seaward distance (m) of an anchored sailing boat off a beach.
pub const ANCHOR_S: f32 = 5.5;
/// The direction the wind blows toward (world XZ): the water's `WIND_ANGLE` (2.8 rad), the way its
/// waves run (`water_common.wesl`).
pub const WIND_TO: Vec2 = Vec2::new(-0.942_222_3, 0.334_988);
/// The landing stage's mooring (`anchor-prop-0`), stage frame (x toward the centre line).
pub const MOORING_X: f32 = 2.535;
/// The landing stage origin's distance from the river centre line (its bank waterline).
pub const STAGE_D: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BoatKind {
    Rowing,
    Sailing,
}

impl BoatKind {
    pub fn piece(self) -> &'static str {
        match self {
            BoatKind::Rowing => "boat_rowing",
            BoatKind::Sailing => "boat_sail",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            BoatKind::Rowing => "rowing",
            BoatKind::Sailing => "sailing",
        }
    }
}

/// A polyline with its cumulative arc length, sampled by arc length.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub points: Vec<Vec2>,
    pub cum: Vec<f32>,
    pub closed: bool,
}

impl Line {
    pub fn new(mut points: Vec<Vec2>, closed: bool) -> Line {
        points.dedup_by(|a, b| a.distance_squared(*b) < 1e-6);
        if closed && points.len() > 1 && points[0].distance(points[points.len() - 1]) > 1e-3 {
            points.push(points[0]);
        }
        let mut cum = vec![0.0];
        for w in points.windows(2) {
            cum.push(cum[cum.len() - 1] + w[0].distance(w[1]));
        }
        Line {
            points,
            cum,
            closed,
        }
    }

    pub fn length(&self) -> f32 {
        self.cum.last().copied().unwrap_or(0.0)
    }

    /// The point and unit tangent `u` metres along (wrapped on a closed line, clamped otherwise).
    pub fn at(&self, u: f32) -> (Vec2, Vec2) {
        let len = self.length();
        if self.points.len() < 2 || len <= 0.0 {
            return (self.points.first().copied().unwrap_or(Vec2::ZERO), Vec2::X);
        }
        let u = if self.closed {
            u - (u / len).floor() * len
        } else {
            u.clamp(0.0, len)
        };
        let i = self
            .cum
            .partition_point(|&c| c <= u)
            .clamp(1, self.points.len() - 1)
            - 1;
        let (a, b) = (self.points[i], self.points[i + 1]);
        let seg = self.cum[i + 1] - self.cum[i];
        let t = if seg > 0.0 {
            (u - self.cum[i]) / seg
        } else {
            0.0
        };
        let d = b - a;
        let n = d.length();
        (a + d * t, if n > 0.0 { d / n } else { Vec2::X })
    }

    /// Points every `step` metres, both ends included.
    pub fn samples(&self, step: f32) -> Vec<Vec2> {
        let len = self.length();
        let n = (len / step).ceil().max(1.0) as usize;
        (0..=n)
            .map(|i| self.at(len * i as f32 / n as f32).0)
            .collect()
    }
}

/// How a river run ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunEnd {
    /// A bridge tile across it (the boat turns back short of its edge).
    Bridge,
    /// A spring or a mouth: the river leaves the plain tiles.
    Water,
    /// No river beyond (the map's edge or a river lane with no tile across it).
    Open,
}

/// A stretch of river centre line a rowing boat may row: plain river tiles in flow order.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub line: Line,
    pub cells: Vec<Axial>,
    /// Upstream end, then downstream.
    pub ends: [RunEnd; 2],
    /// Arc length at which each cell's stretch starts (with `cells`), for stage placement.
    pub cell_start: Vec<f32>,
}

/// A river tile a boat may row on: river water that is no bridge, spring or mouth.
pub fn rowable(def: &TileDef, cell: &CellWater) -> bool {
    cell.kind == WaterKind::River && !bridge(def)
}

/// A bridge tile: a River network crossed by a Road or Rail one.
pub fn bridge(def: &TileDef) -> bool {
    def.networks.iter().any(|n| n.kind == NetworkKind::River)
        && def
            .networks
            .iter()
            .any(|n| matches!(n.kind, NetworkKind::Road | NetworkKind::Rail))
}

/// Every river run of the placed tiles, in the order of their first segment.
pub fn runs(
    tiles: &[PlayTile],
    catalog: &TileCatalog,
    cells: &BTreeMap<Axial, CellWater>,
) -> Vec<Run> {
    let rivers = oriented_rivers(tiles, catalog);
    let def_at = |c: Axial| {
        cells
            .get(&c)
            .and_then(|w| catalog.get(&w.tile.tile).map(|d| (d, w)))
    };
    let ok = |c: Axial| def_at(c).is_some_and(|(d, w)| rowable(d, w));
    let opposite = |e: u8| (e + 3) % 6;
    // The segment across `end` of `cell` whose other end meets it with `want` (up or down).
    let across = |cell: Axial, end: RiverEnd, up: bool| -> Option<(Axial, usize)> {
        let e = end.edge()?;
        let n = cell.neighbor(e);
        let back = RiverEnd::Edge(opposite(e));
        rivers
            .range((n, 0)..=(n, usize::MAX))
            .find(|(_, p)| if up { p.up == back } else { p.down == back })
            .map(|(k, _)| *k)
    };
    let end_kind = |cell: Axial, end: RiverEnd, up: bool| -> RunEnd {
        let Some(e) = end.edge() else {
            return RunEnd::Water;
        };
        match def_at(cell.neighbor(e)) {
            Some((d, _)) if bridge(d) => RunEnd::Bridge,
            Some(_) if across(cell, end, up).is_some() => RunEnd::Water,
            _ => RunEnd::Open,
        }
    };
    let keys: Vec<(Axial, usize)> = rivers.keys().copied().filter(|k| ok(k.0)).collect();
    let next = |k: (Axial, usize)| across(k.0, rivers[&k].down, true).filter(|n| ok(n.0));
    let prev = |k: (Axial, usize)| across(k.0, rivers[&k].up, false).filter(|n| ok(n.0));
    let mut seen = BTreeSet::new();
    let mut out = vec![];
    let starts: Vec<_> = keys
        .iter()
        .copied()
        .filter(|k| prev(*k).is_none())
        .collect();
    for start in starts.into_iter().chain(keys.iter().copied()) {
        if seen.contains(&start) {
            continue;
        }
        let mut chain = vec![];
        let mut cur = Some(start);
        while let Some(k) = cur {
            if !seen.insert(k) {
                break;
            }
            chain.push(k);
            cur = next(k);
        }
        let (first, last) = (chain[0], chain[chain.len() - 1]);
        let mut points: Vec<Vec2> = vec![];
        let mut cells_in = vec![];
        let mut cell_start = vec![];
        for k in &chain {
            let start_len = Line::new(points.clone(), false).length();
            cells_in.push(k.0);
            cell_start.push(start_len);
            points.extend(rivers[k].points.iter().copied());
        }
        let ends = [
            end_kind(first.0, rivers[&first].up, false),
            end_kind(last.0, rivers[&last].down, true),
        ];
        out.push(Run {
            line: Line::new(points, false),
            cells: cells_in,
            ends,
            cell_start,
        });
    }
    out
}

/// Whether `p` is sailing water of the sea cells `sea` (ocean, or coast and mouth at
/// [`SAIL_S`] or more seaward).
pub fn sea_water(cells: &BTreeMap<Axial, CellWater>, sea: &BTreeSet<Axial>, p: Vec2) -> bool {
    let c = cell_at(p);
    if !sea.contains(&c) {
        return false;
    }
    let Some(w) = cells.get(&c) else {
        return false;
    };
    let local = p - Vec2::from(c.to_world(HEX_RADIUS));
    match w.kind {
        WaterKind::Ocean => true,
        WaterKind::Coast(coast) | WaterKind::RiverMouth(coast) => coast.seaward(local) >= SAIL_S,
        _ => false,
    }
}

/// [`sea_water`] over the [`SAIL_CLEAR`] disc round `p` (its centre and 12 rim points).
pub fn sailable(cells: &BTreeMap<Axial, CellWater>, sea: &BTreeSet<Axial>, p: Vec2) -> bool {
    sea_water(cells, sea, p)
        && (0..12).all(|i| {
            let a = i as f32 * std::f32::consts::TAU / 12.0;
            sea_water(cells, sea, p + SAIL_CLEAR * Vec2::new(a.cos(), a.sin()))
        })
}

fn sea_kind(kind: &WaterKind) -> bool {
    matches!(
        kind,
        WaterKind::Ocean | WaterKind::Coast(_) | WaterKind::RiverMouth(_)
    )
}

/// Candidate sailing points of the sea cells: a 4 m lattice over each cell, the sailable ones.
fn sea_points(cells: &BTreeMap<Axial, CellWater>, sea: &BTreeSet<Axial>) -> Vec<Vec2> {
    let mut out = vec![];
    for c in sea {
        let centre = Vec2::from(c.to_world(HEX_RADIUS));
        for i in -3..=3 {
            for j in -3..=3 {
                let p = centre + Vec2::new(i as f32 * 4.0 + (j & 1) as f32 * 2.0, j as f32 * 3.464);
                if cell_at(p) == *c && sailable(cells, sea, p) {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// A closed route through seeded sailable points, every metre of it sailable: a loop round a few
/// points sorted by angle about their centroid, its corners cut, else a circle. `None` when the
/// sea has no room.
pub fn sail_route(
    cells: &BTreeMap<Axial, CellWater>,
    sea: &BTreeSet<Axial>,
    avoid: &[Vec2],
    rng: &mut Rng,
) -> Option<Line> {
    let pts = sea_points(cells, sea);
    if pts.is_empty() {
        return None;
    }
    let ok = |l: &Line| {
        l.length() > 20.0
            && l.samples(1.0).iter().all(|p| {
                sailable(cells, sea, *p) && avoid.iter().all(|a| a.distance(*p) >= ANCHOR_CLEAR)
            })
    };
    for _ in 0..24 {
        let k = 3 + (rng.next_u64() % 3) as usize;
        let mut way: Vec<Vec2> = (0..k)
            .map(|_| pts[(rng.next_u64() % pts.len() as u64) as usize])
            .collect();
        way.dedup_by(|a, b| a.distance(*b) < 6.0);
        if way.len() < 3 {
            continue;
        }
        let c = way.iter().copied().sum::<Vec2>() / way.len() as f32;
        way.sort_by(|a, b| {
            let (pa, pb) = (*a - c, *b - c);
            pa.y.atan2(pa.x).total_cmp(&pb.y.atan2(pb.x))
        });
        // Chaikin corner cutting, twice: a smooth loop inside the polygon's hull.
        let mut loop_pts = way;
        for _ in 0..3 {
            let n = loop_pts.len();
            loop_pts = (0..n)
                .flat_map(|i| {
                    let (a, b) = (loop_pts[i], loop_pts[(i + 1) % n]);
                    [a * 0.75 + b * 0.25, a * 0.25 + b * 0.75]
                })
                .collect();
        }
        let l = Line::new(loop_pts, true);
        if ok(&l) {
            return Some(l);
        }
    }
    let start = (rng.next_u64() % pts.len() as u64) as usize;
    for r in [12.0f32, 9.0, 6.5] {
        for i in 0..pts.len() {
            let c = pts[(start + i) % pts.len()];
            let l = Line::new(
                (0..48)
                    .map(|k| {
                        let a = k as f32 * std::f32::consts::TAU / 48.0;
                        c + r * Vec2::new(a.cos(), a.sin())
                    })
                    .collect(),
                true,
            );
            if ok(&l) {
                return Some(l);
            }
        }
    }
    None
}

/// A landing stage: origin on the bank's waterline [`STAGE_D`] from the centre line, `x` the unit
/// direction toward the centre line, the flow along its `z`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stage {
    pub cell: Axial,
    pub origin: Vec2,
    pub x: Vec2,
    /// Index of the run, the arc length of the stage along it, and the side (+1 right of the flow).
    pub run: usize,
    pub u: f32,
    pub side: f32,
}

impl Stage {
    /// The stage's local `z` axis in world XZ (Bevy's frame for yaw [`Stage::yaw`]).
    pub fn z(&self) -> Vec2 {
        Vec2::new(-self.x.y, self.x.x)
    }

    /// Yaw (radians from +x toward +z) of the stage's `x` axis.
    pub fn yaw(&self) -> f32 {
        self.x.y.atan2(self.x.x)
    }

    /// The moored rowing boat's origin and heading: the mooring, bow toward the stage's -z.
    pub fn mooring(&self) -> (Vec2, Vec2) {
        (self.origin + self.x * MOORING_X, -self.z())
    }

    /// Whether world XZ `p` stands on the deck (stage frame x -0.75..1.40, z -1.4..1.4).
    pub fn on_deck(&self, p: Vec2) -> bool {
        let d = p - self.origin;
        let (x, z) = (d.dot(self.x), d.dot(self.z()));
        (-0.75..=1.40).contains(&x) && (-1.4..=1.4).contains(&z)
    }
}

/// What a boat does.
#[derive(Clone, Debug, PartialEq)]
pub enum Motion {
    /// A rowing boat at a landing stage (index into [`BoatPlan::stages`]), oars trailed.
    Moored { stage: usize },
    /// A sailing boat at anchor, at `pos` heading `heading` (unit).
    Anchored { pos: Vec2, heading: Vec2 },
    /// A rowing boat on run `run`.
    Rowing(Rower),
    /// A sailing boat on its route.
    Sailing(Sailor),
}

/// A rowing boat's tick state along its run.
#[derive(Clone, Debug, PartialEq)]
pub struct Rower {
    pub run: usize,
    /// Arc length along the run's centre line.
    pub u: f32,
    /// +1 downstream, -1 upstream.
    pub dir: f32,
    /// Progress through a turn back (0 while rowing; (0, 1) turning).
    pub turn: f32,
    /// Lateral offset right of the heading (m).
    pub lat: f32,
    /// Oar stroke phase in [0, 1): the catch at 0.
    pub stroke: f32,
    /// 0 rowing, 1 oars trailed.
    pub trail: f32,
}

/// A sailing boat's tick state along its route.
#[derive(Clone, Debug, PartialEq)]
pub struct Sailor {
    pub route: usize,
    pub s: f32,
    /// The boom's side: +1 starboard, -1 port (leeward of the wind).
    pub boom: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Boat {
    pub id: u64,
    pub kind: BoatKind,
    pub motion: Motion,
}

impl Boat {
    pub fn moving(&self) -> bool {
        matches!(self.motion, Motion::Rowing(_) | Motion::Sailing(_))
    }
}

/// A sound cue a boat asks for on a tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BoatCue {
    /// The oars catch the water.
    Stroke { id: u64, at: Vec2 },
    /// The boom crosses sides on a turn.
    SailFlap { id: u64, at: Vec2 },
}

/// A world's boats: derived from its tiles and seed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoatPlan {
    /// The water layout the plan was made from (the boats ride its surface).
    pub cells: BTreeMap<Axial, CellWater>,
    pub runs: Vec<Run>,
    pub routes: Vec<Line>,
    pub stages: Vec<Stage>,
    pub boats: Vec<Boat>,
    /// Ticks run.
    pub ticks: u64,
}

/// One water region's share: (first cell, river cells, sea cells).
fn region_sizes(fish: &FishWorld) -> Vec<(Vec<Axial>, usize, usize)> {
    fish.regions
        .iter()
        .map(|r| {
            let river = r
                .cells
                .iter()
                .filter(|c| fish.cells.get(c).is_some_and(|w| !sea_kind(&w.kind)))
                .count();
            (r.cells.clone(), river, r.cells.len() - river)
        })
        .collect()
}

/// Takes boats per region largest first (ties to the lowest cell) up to `cap` world-wide.
/// `sizes` holds (size, first cell, boats wanted) per region; the result (region, boats) is in
/// the order taken.
pub fn share(sizes: &[(usize, Axial, usize)], cap: usize) -> Vec<(usize, usize)> {
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&i| (std::cmp::Reverse(sizes[i].0), sizes[i].1.q, sizes[i].1.r));
    let mut left = cap;
    order
        .into_iter()
        .map(|i| {
            let n = sizes[i].2.min(left);
            left -= n;
            (i, n)
        })
        .collect()
}

impl BoatPlan {
    /// The boats of `tiles` under world seed `seed`.
    pub fn new(tiles: &[PlayTile], catalog: &TileCatalog, seed: u64) -> BoatPlan {
        let fish = FishWorld::new(tiles, catalog);
        let cells = &fish.cells;
        let runs = runs(tiles, catalog, cells);
        let sizes = region_sizes(&fish);
        let rowing_want: Vec<(usize, Axial, usize)> = sizes
            .iter()
            .map(|(c, river, _)| (*river, c[0], river.div_ceil(2)))
            .collect();
        let sailing_want: Vec<(usize, Axial, usize)> = sizes
            .iter()
            .map(|(c, _, sea)| (*sea, c[0], sea / 2))
            .collect();
        let mut plan = BoatPlan {
            cells: cells.clone(),
            runs,
            ..Default::default()
        };
        let mut rng = Rng(cell_seed(seed, Axial::new(0, 0)) ^ 0xB0A7_5EED);
        let mut id = 0u64;
        // Rowing: one moving boat per run long enough, the second boat world-wide at a stage.
        let mut taken_runs: BTreeSet<usize> = BTreeSet::new();
        let mut rowing = 0usize;
        for (region, n) in share(&rowing_want, MAX_ROWING) {
            let region_cells: BTreeSet<Axial> = sizes[region].0.iter().copied().collect();
            let mut region_runs: Vec<usize> = (0..plan.runs.len())
                .filter(|&i| region_cells.contains(&plan.runs[i].cells[0]))
                .collect();
            region_runs.sort_by(|a, b| {
                plan.runs[*b]
                    .line
                    .length()
                    .total_cmp(&plan.runs[*a].line.length())
                    .then(a.cmp(b))
            });
            for _ in 0..n {
                let moor = rowing == 1;
                let free = region_runs
                    .iter()
                    .copied()
                    .find(|r| !taken_runs.contains(r) && plan.runs[*r].line.length() >= MIN_RUN);
                let motion = match (moor, free) {
                    (false, Some(r)) => {
                        taken_runs.insert(r);
                        let len = plan.runs[r].line.length();
                        let u = ROW_END_MARGIN + rng.unit() * (len - 2.0 * ROW_END_MARGIN);
                        let dir = if rng.unit() < 0.5 { 1.0 } else { -1.0 };
                        Some(Motion::Rowing(Rower {
                            run: r,
                            u,
                            dir,
                            turn: 0.0,
                            lat: ROW_LANE,
                            stroke: rng.unit(),
                            trail: 0.0,
                        }))
                    }
                    _ => plan
                        .place_stage(&region_runs, cells, catalog, &mut rng)
                        .map(|stage| Motion::Moored { stage }),
                };
                if let Some(motion) = motion {
                    plan.boats.push(Boat {
                        id,
                        kind: BoatKind::Rowing,
                        motion,
                    });
                    id += 1;
                    rowing += 1;
                }
            }
        }
        // Sailing: a route per moving boat, the second boat world-wide at anchor.
        let mut sailing = 0usize;
        for (region, n) in share(&sailing_want, MAX_SAILING) {
            let sea: BTreeSet<Axial> = sizes[region]
                .0
                .iter()
                .copied()
                .filter(|c| cells.get(c).is_some_and(|w| sea_kind(&w.kind)))
                .collect();
            // The anchorage first, so the routes keep clear of it.
            let anchor_slot = (sailing..sailing + n).position(|k| k == 1);
            let anchored = anchor_slot.and_then(|_| anchorage(cells, &sea, &mut rng));
            let avoid: Vec<Vec2> = anchored.iter().map(|(p, _)| *p).collect();
            for slot in 0..n {
                if Some(slot) == anchor_slot {
                    if let Some((pos, heading)) = anchored {
                        plan.boats.push(Boat {
                            id,
                            kind: BoatKind::Sailing,
                            motion: Motion::Anchored { pos, heading },
                        });
                        id += 1;
                        sailing += 1;
                    }
                    continue;
                }
                let route = sail_route(cells, &sea, &avoid, &mut rng);
                let motion = match route {
                    Some(line) => {
                        plan.routes.push(line);
                        let r = plan.routes.len() - 1;
                        let s = rng.unit() * plan.routes[r].length();
                        let (_, h) = plan.routes[r].at(s);
                        Some(Motion::Sailing(Sailor {
                            route: r,
                            s,
                            boom: leeward(h, 1.0),
                        }))
                    }
                    None => None,
                };
                if let Some(motion) = motion {
                    plan.boats.push(Boat {
                        id,
                        kind: BoatKind::Sailing,
                        motion,
                    });
                    id += 1;
                    sailing += 1;
                }
            }
        }
        plan
    }

    /// Places a landing stage on a plain river tile of `region_runs` (straight or bend, not taken
    /// by a stage yet), its side seeded. Returns its index.
    fn place_stage(
        &mut self,
        region_runs: &[usize],
        cells: &BTreeMap<Axial, CellWater>,
        catalog: &TileCatalog,
        rng: &mut Rng,
    ) -> Option<usize> {
        let mut options = vec![];
        for &r in region_runs {
            let run = &self.runs[r];
            for (i, c) in run.cells.iter().enumerate() {
                let plain = cells
                    .get(c)
                    .and_then(|w| catalog.get(&w.tile.tile))
                    .is_some_and(|d| d.id == "river_straight" || d.id == "river_bend");
                if plain && self.stages.iter().all(|s| s.cell != *c) {
                    let start = run.cell_start[i];
                    let end = run
                        .cell_start
                        .get(i + 1)
                        .copied()
                        .unwrap_or(run.line.length());
                    options.push((r, *c, (start + end) / 2.0));
                }
            }
        }
        if options.is_empty() {
            return None;
        }
        let (run, cell, u) = options[(rng.next_u64() % options.len() as u64) as usize];
        let side = if rng.unit() < 0.5 { 1.0 } else { -1.0 };
        let (p, t) = self.runs[run].line.at(u);
        let right = Vec2::new(-t.y, t.x);
        let out = right * side;
        self.stages.push(Stage {
            cell,
            origin: p + out * STAGE_D,
            x: -out,
            run,
            u,
            side,
        });
        Some(self.stages.len() - 1)
    }

    /// Advances every moving boat one tick, returning the cues it asks for.
    pub fn tick(&mut self) -> Vec<BoatCue> {
        self.ticks += 1;
        let dt = TICK_SECONDS;
        let mut cues = vec![];
        let stages = &self.stages;
        for b in &mut self.boats {
            match &mut b.motion {
                Motion::Rowing(r) => {
                    let run = &self.runs[r.run];
                    let len = run.line.length();
                    let (lo, hi) = (ROW_END_MARGIN, len - ROW_END_MARGIN);
                    // A mooring on this run within the pass zone, on this boat's side of the channel.
                    let passing = stages.iter().any(|s| {
                        s.run == r.run && (s.u - r.u).abs() < PASS_ZONE && s.side == r.dir
                    });
                    let want_lat = if passing && r.turn == 0.0 {
                        -PASS_LANE
                    } else {
                        ROW_LANE
                    };
                    let step = LANE_SPEED * dt;
                    r.lat += (want_lat - r.lat).clamp(-step, step);
                    let want_trail = if passing { 1.0 } else { 0.0 };
                    let tstep = dt / TRAIL_SECONDS;
                    r.trail += (want_trail - r.trail).clamp(-tstep, tstep);
                    if r.turn > 0.0 {
                        r.turn += dt * ROW_SPEED / (std::f32::consts::PI * ROW_LANE);
                        if r.turn >= 1.0 {
                            r.turn = 0.0;
                            r.dir = -r.dir;
                        }
                    } else {
                        r.u += r.dir * ROW_SPEED * dt;
                        if (r.dir > 0.0 && r.u >= hi) || (r.dir < 0.0 && r.u <= lo) {
                            r.u = r.u.clamp(lo, hi);
                            r.turn = 1e-4;
                        }
                    }
                    let before = r.stroke;
                    r.stroke += dt * ROW_SPEED / STROKE_M;
                    if r.stroke >= 1.0 {
                        r.stroke -= 1.0;
                    }
                    if r.stroke < before && r.trail < 0.5 {
                        let (p, _) = run.line.at(r.u);
                        cues.push(BoatCue::Stroke { id: b.id, at: p });
                    }
                }
                Motion::Sailing(s) => {
                    let route = &self.routes[s.route];
                    s.s += SAIL_SPEED * dt;
                    let len = route.length();
                    if s.s >= len {
                        s.s -= len;
                    }
                    let (p, h) = route.at(s.s);
                    let side = leeward(h, s.boom);
                    if side != s.boom {
                        s.boom = side;
                        cues.push(BoatCue::SailFlap { id: b.id, at: p });
                    }
                }
                _ => {}
            }
        }
        cues
    }

    /// Every boat's pose: (id, kind, world XZ, unit heading).
    pub fn poses(&self) -> Vec<(u64, BoatKind, Vec2, Vec2)> {
        self.boats
            .iter()
            .map(|b| {
                let (p, h) = self.pose(b);
                (b.id, b.kind, p, h)
            })
            .collect()
    }

    /// A boat's world XZ and unit heading on the current tick.
    pub fn pose(&self, b: &Boat) -> (Vec2, Vec2) {
        match &b.motion {
            Motion::Moored { stage } => self.stages[*stage].mooring(),
            Motion::Anchored { pos, heading } => (*pos, *heading),
            Motion::Rowing(r) => rower_pose(&self.runs[r.run].line, r),
            Motion::Sailing(s) => self.routes[s.route].at(s.s),
        }
    }
}

/// A rowing boat's world XZ and heading: right of its heading by `lat`, or on the turn's half
/// circle about the turning point.
pub fn rower_pose(line: &Line, r: &Rower) -> (Vec2, Vec2) {
    let (c, t) = line.at(r.u);
    let right = Vec2::new(-t.y, t.x);
    if r.turn > 0.0 {
        let a = std::f32::consts::PI * r.turn;
        let lat = r.lat.abs().max(0.5);
        let p = c + right * (r.dir * lat * a.cos()) + t * (r.dir * lat * a.sin());
        let h = (-right * a.sin() + t * a.cos()) * r.dir;
        (p, h.normalize_or(t))
    } else {
        (c + right * (r.dir * r.lat), t * r.dir)
    }
}

/// The boom's leeward side (+1 starboard, -1 port) for heading `h`: away from the wind, keeping
/// `current` while the wind is near dead ahead or astern (no flapping back and forth).
pub fn leeward(h: Vec2, current: f32) -> f32 {
    // The wind comes from -WIND_TO. Starboard is the heading's right.
    let right = Vec2::new(-h.y, h.x);
    let from = -WIND_TO;
    let x = right.dot(from);
    if x > 0.15 {
        -1.0
    } else if x < -0.15 {
        1.0
    } else {
        current
    }
}

/// An anchorage: a seeded coast or mouth cell's point [`ANCHOR_S`] seaward of the coastline (else
/// any sailable sea point), heading into the wind, its hull ends and sides on sailing water.
fn anchorage(
    cells: &BTreeMap<Axial, CellWater>,
    sea: &BTreeSet<Axial>,
    rng: &mut Rng,
) -> Option<(Vec2, Vec2)> {
    let into_wind = -WIND_TO;
    let hull_ok = |p: Vec2, h: Vec2| {
        let side = Vec2::new(-h.y, h.x);
        [h * 3.1, -h * 3.1, side * 1.0, -side * 1.0, Vec2::ZERO]
            .iter()
            .all(|o| sea_water(cells, sea, p + *o))
    };
    let coasts: Vec<(Axial, Coastline)> = sea
        .iter()
        .filter_map(|c| match cells.get(c).map(|w| &w.kind) {
            Some(WaterKind::Coast(k) | WaterKind::RiverMouth(k)) => Some((*c, *k)),
            _ => None,
        })
        .collect();
    if !coasts.is_empty() {
        let start = (rng.next_u64() % coasts.len() as u64) as usize;
        for i in 0..coasts.len() {
            let (c, coast) = coasts[(start + i) % coasts.len()];
            let centre = Vec2::from(c.to_world(HEX_RADIUS));
            // Walk out from the centre along the seaward gradient to s = ANCHOR_S.
            let grad = |p: Vec2| {
                let s = coast.seaward(p);
                Vec2::new(
                    coast.seaward(p + Vec2::X * 0.1) - s,
                    coast.seaward(p + Vec2::Y * 0.1) - s,
                )
                .normalize_or(Vec2::X)
            };
            let mut p = Vec2::ZERO;
            for _ in 0..40 {
                p += grad(p) * (ANCHOR_S - coast.seaward(p));
            }
            // Along the coastline first: both hull ends keep its seaward distance.
            let g = grad(p);
            let along = Vec2::new(-g.y, g.x);
            let along = if along.dot(into_wind) < 0.0 {
                -along
            } else {
                along
            };
            let p = centre + p;
            for h in [along, into_wind, Vec2::new(-into_wind.y, into_wind.x)] {
                if cell_at(p) == c && hull_ok(p, h) {
                    return Some((p, h));
                }
            }
        }
    }
    let pts = sea_points(cells, sea);
    (!pts.is_empty()).then(|| (pts[(rng.next_u64() % pts.len() as u64) as usize], into_wind))
}
