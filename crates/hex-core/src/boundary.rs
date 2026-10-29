//! Field boundaries (B2, owner A31 O1, 2026-10-27): dry-stone walls and hedgerows on the shared
//! edges where a field tile (the farm or the wheat field) meets a grass land tile (meadow, forest,
//! oak, windmill). They are derived from the placed tiles, like scatter: never baked into a tile,
//! never saved, re-derived when a tile or a neighbour changes.
//!
//! - **Edges.** A 16 m edge carries a 2 m corner arm at each end and three 4 m runs between
//!   (2 + 4 + 4 + 4 + 2). The runs centre on the edge line, so a boundary straddles both tiles.
//! - **Kind.** Wall or hedge per edge, from the world seed and the edge. Each farm takes one gate
//!   edge (a wall, its middle run the gateway), preferring an edge whose ground inside the gate is
//!   clear of heroes ([`GATE_APPROACH`]: the paddock fence encloses the farm's local edges 1 and 2,
//!   so the gate opens onto the yard); a hedge edge takes a hedgerow oak in one run about
//!   one time in three ([`HEDGEROW_OAK`] stands at that run's centre).
//! - **Corners.** At a hex corner the boundary edges meeting there are joined by one corner piece
//!   per kind: an end (one arm), a 120 degree bend (two) or a three-way junction (three). A wall
//!   edge and a hedge edge meeting at a corner each end there. Under the field/land rule at most
//!   two of the three edges at a corner are boundaries (the third would join two fields or two land
//!   tiles), so the junction pieces are built but never placed.
//! - **Breaks.** Only edges whose two sockets are both grass or forest with no lane carry a
//!   boundary, so roads, rivers, rail and coast break them (every field and land tile listed has
//!   grass or forest edges only today, so the check is a guard for later tiles). A run or corner arm
//!   over a hero prop's footprint (the farm's barn stands 0.5 m inside its edge 5) is left out,
//!   which leaves a gap the walker passes through: `blocked` decides that per cell.
//! - **Ownership.** A run belongs to the edge's field tile, a corner piece to the lowest-ordered
//!   field tile at the corner that has a boundary there; pieces are placed in the owner's frame and
//!   drawn under its root, so they rise, sink and go with it. A change at a cell can change the
//!   pieces of field tiles up to two cells away (a farm's gate choice reads all its edges), so
//!   callers redraw that radius.
use crate::{Axial, HEX_RADIUS, SocketKind, TileDef, cell_seed};

/// Tiles boundaries enclose.
pub const FIELD_TILES: [&str; 2] = ["farm", "wheat_field"];
/// The grass land tiles a field is fenced from (every tile with only grass or forest edges and no
/// network, less the fields themselves).
pub const LAND_TILES: [&str; 4] = ["meadow", "forest", "oak", "windmill"];
/// The tile that takes one gate edge.
pub const GATE_TILE: &str = "farm";
/// The hedgerow oak a `hedge_oak` run carries at its centre.
pub const HEDGEROW_OAK: &str = "tree_oak_b";
/// How far (m) inside the farm the ground before its gate must be clear of heroes.
pub const GATE_APPROACH: f32 = 3.5;
/// Run and corner-arm lengths along an edge (m).
pub const RUN: f32 = 4.0;
pub const ARM: f32 = 2.0;
/// Half widths at the foot (m): the wall 0.7 m wide, the hedge 1.5 m.
pub const WALL_HALF: f32 = 0.35;
pub const HEDGE_HALF: f32 = 0.75;
/// Every boundary piece, for kit and table checks.
pub const PIECES: [&str; 12] = [
    "wall_run",
    "wall_run_b",
    "wall_end",
    "wall_bend",
    "wall_junction",
    "wall_gate",
    "hedge_run",
    "hedge_run_b",
    "hedge_end",
    "hedge_bend",
    "hedge_junction",
    "hedge_oak",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Wall,
    Hedge,
}

impl Kind {
    pub fn half_width(self) -> f32 {
        match self {
            Kind::Wall => WALL_HALF,
            Kind::Hedge => HEDGE_HALF,
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            Kind::Wall => "wall",
            Kind::Hedge => "hedge",
        }
    }
}

/// A boundary piece's footprint line in the world (x, z): from `a` to `b`, `half` m either side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seg {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub half: f32,
}

impl Seg {
    /// Distance from `p` to the segment's centre line.
    pub fn distance(&self, p: [f32; 2]) -> f32 {
        let (ax, az, bx, bz) = (self.a[0], self.a[1], self.b[0], self.b[1]);
        let (dx, dz) = (bx - ax, bz - az);
        let len2 = dx * dx + dz * dz;
        let t = if len2 > 0.0 {
            (((p[0] - ax) * dx + (p[1] - az) * dz) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (qx, qz) = (ax + dx * t - p[0], az + dz * t - p[1]);
        (qx * qx + qz * qz).sqrt()
    }
}

/// One piece to draw under its owner's root: the piece, its spot in the owner's tile frame (x, z),
/// its yaw (degrees, as a hero's: the piece's +x points along `(cos, sin)` of it in the tile frame),
/// its footprint lines in the world, and the cells it stands on (a run: its two tiles; a corner
/// piece: the three at the corner), so it can rise with whichever of them is rising.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub piece: &'static str,
    pub at: [f32; 2],
    pub yaw_deg: f32,
    pub segs: Vec<Seg>,
    pub cells: Vec<Axial>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Field,
    Land,
}

fn class(def: &TileDef) -> Option<Class> {
    if FIELD_TILES.contains(&def.id.as_str()) {
        Some(Class::Field)
    } else if LAND_TILES.contains(&def.id.as_str()) {
        Some(Class::Land)
    } else {
        None
    }
}

fn sin_cos_deg(deg: f32) -> (f32, f32) {
    let r = deg.to_radians();
    (r.sin(), r.cos())
}

fn dir(deg: f32) -> [f32; 2] {
    let (s, c) = sin_cos_deg(deg);
    [c, s]
}

fn add(p: [f32; 2], d: [f32; 2], k: f32) -> [f32; 2] {
    [p[0] + d[0] * k, p[1] + d[1] * k]
}

fn centre(cell: Axial) -> [f32; 2] {
    cell.to_world(HEX_RADIUS)
}

/// World position of world corner `j` of `cell` (corner j sits at 30 + 60 j degrees, between
/// world edges j and j + 1).
fn corner(cell: Axial, j: u8) -> [f32; 2] {
    add(centre(cell), dir(30.0 + 60.0 * (j % 6) as f32), HEX_RADIUS)
}

fn mix(world_seed: u64, cell: Axial, salt: u64) -> u64 {
    cell_seed(world_seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15), cell)
}

/// The canonical (cell, world edge) of the edge between `a` and its neighbour across world edge
/// `e`: the lower-ordered cell's side.
fn canonical(a: Axial, e: u8) -> (Axial, u8) {
    let b = a.neighbor(e);
    if a <= b { (a, e % 6) } else { (b, (e + 3) % 6) }
}

fn edge_hash(world_seed: u64, a: Axial, e: u8, salt: u64) -> u64 {
    let (c, k) = canonical(a, e);
    mix(world_seed, c, salt * 8 + k as u64)
}

/// A piece in the world before it moves into its owner's frame: piece, spot, yaw, footprints, cells.
type Spot = (&'static str, [f32; 2], f32, Vec<Seg>, Vec<Axial>);

/// The world-frame view of the placed tiles boundaries read.
pub struct View<'a, A, B>
where
    A: Fn(Axial) -> Option<(&'a TileDef, u8)>,
    B: Fn(Axial, &Seg) -> bool,
{
    pub world_seed: u64,
    /// The definition and rotation of the tile at a cell.
    pub at: A,
    /// Whether a boundary footprint would overlap something on the tile at a cell (its hero props).
    pub blocked: B,
    /// Each farm's gate edge once worked out (`placements` asks for it on every edge and corner).
    gates: std::cell::RefCell<std::collections::BTreeMap<Axial, Option<u8>>>,
}

impl<'a, A, B> View<'a, A, B>
where
    A: Fn(Axial) -> Option<(&'a TileDef, u8)>,
    B: Fn(Axial, &Seg) -> bool,
{
    pub fn new(world_seed: u64, at: A, blocked: B) -> Self {
        Self {
            world_seed,
            at,
            blocked,
            gates: Default::default(),
        }
    }

    /// Whether the edge between `a` and its neighbour across world edge `e` is a boundary, and
    /// its field side.
    fn boundary(&self, a: Axial, e: u8) -> Option<Axial> {
        let b = a.neighbor(e);
        let ((da, ra), (db, rb)) = ((self.at)(a)?, (self.at)(b)?);
        let open = |s: &crate::EdgeSocket| {
            s.lane.is_none() && matches!(s.kind, SocketKind::Grass | SocketKind::Forest)
        };
        if !open(da.edge(e % 6, ra)) || !open(db.edge((e + 3) % 6, rb)) {
            return None;
        }
        match (class(da)?, class(db)?) {
            (Class::Field, Class::Land) => Some(a),
            (Class::Land, Class::Field) => Some(b),
            _ => None,
        }
    }

    /// The footprint of run `slot` (0, 1, 2) of `field`'s world edge `e`.
    fn run_seg(&self, field: Axial, e: u8, slot: u8, kind: Kind) -> Seg {
        let start = corner(field, (e + 5) % 6);
        let t = dir(60.0 * e as f32 + 90.0);
        let s = ARM + RUN * slot as f32;
        Seg {
            a: add(start, t, s),
            b: add(start, t, s + RUN),
            half: kind.half_width(),
        }
    }

    fn seg_blocked(&self, field: Axial, e: u8, seg: &Seg) -> bool {
        (self.blocked)(field, seg) || (self.blocked)(field.neighbor(e), seg)
    }

    /// A farm's gate edge: one of its boundary edges whose middle run is clear, picked from the
    /// world seed among those whose gate opens onto the farm's open ground (a [`GATE_APPROACH`] m
    /// strip inside it clear of heroes: not into the paddock or against the barn), or among every
    /// clear one when none does.
    fn gate_edge(&self, farm: Axial) -> Option<u8> {
        if let Some(g) = self.gates.borrow().get(&farm) {
            return *g;
        }
        let g = self.pick_gate(farm);
        self.gates.borrow_mut().insert(farm, g);
        g
    }

    fn pick_gate(&self, farm: Axial) -> Option<u8> {
        if (self.at)(farm).is_none_or(|(d, _)| d.id != GATE_TILE) {
            return None;
        }
        let clear: Vec<u8> = (0..6u8)
            .filter(|&e| self.boundary(farm, e) == Some(farm))
            .filter(|&e| !self.seg_blocked(farm, e, &self.run_seg(farm, e, 1, Kind::Wall)))
            .collect();
        let open: Vec<u8> = clear
            .iter()
            .copied()
            .filter(|&e| {
                let run = self.run_seg(farm, e, 1, Kind::Wall);
                let mid = [(run.a[0] + run.b[0]) / 2.0, (run.a[1] + run.b[1]) / 2.0];
                let inward = dir(60.0 * e as f32 + 180.0);
                let approach = Seg {
                    a: add(mid, inward, WALL_HALF + 0.4),
                    b: add(mid, inward, GATE_APPROACH),
                    half: 1.2,
                };
                !(self.blocked)(farm, &approach)
            })
            .collect();
        let edges = if open.is_empty() { clear } else { open };
        if edges.is_empty() {
            return None;
        }
        let pick = mix(self.world_seed, farm, 0x6A7E) % edges.len() as u64;
        Some(edges[pick as usize])
    }

    /// The kind of the boundary edge between `a` and its neighbour across world edge `e`.
    fn kind(&self, a: Axial, e: u8) -> Option<Kind> {
        let field = self.boundary(a, e)?;
        let fe = if field == a { e % 6 } else { (e + 3) % 6 };
        if self.gate_edge(field) == Some(fe) {
            return Some(Kind::Wall);
        }
        Some(if edge_hash(self.world_seed, a, e, 1).is_multiple_of(2) {
            Kind::Wall
        } else {
            Kind::Hedge
        })
    }

    /// Every boundary piece `owner` draws, in its tile frame. Empty unless `owner` is a field tile.
    pub fn placements(&self, owner: Axial) -> Vec<Placement> {
        let Some((def, rot)) = (self.at)(owner) else {
            return vec![];
        };
        if class(def) != Some(Class::Field) {
            return vec![];
        }
        let mut world: Vec<Spot> = vec![];
        let gate = self.gate_edge(owner);
        for e in 0..6u8 {
            if self.boundary(owner, e) != Some(owner) {
                continue;
            }
            let Some(kind) = self.kind(owner, e) else {
                continue;
            };
            let along = 60.0 * e as f32 + 90.0;
            let oak = (kind == Kind::Hedge
                && edge_hash(self.world_seed, owner, e, 2).is_multiple_of(3))
            .then(|| (edge_hash(self.world_seed, owner, e, 3) % 3) as u8);
            for slot in 0..3u8 {
                let seg = self.run_seg(owner, e, slot, kind);
                let piece = if gate == Some(e) && slot == 1 {
                    "wall_gate"
                } else if oak == Some(slot) {
                    "hedge_oak"
                } else {
                    let b = edge_hash(self.world_seed, owner, e, 10 + slot as u64) % 2 == 1;
                    match (kind, b) {
                        (Kind::Wall, false) => "wall_run",
                        (Kind::Wall, true) => "wall_run_b",
                        (Kind::Hedge, false) => "hedge_run",
                        (Kind::Hedge, true) => "hedge_run_b",
                    }
                };
                if piece != "wall_gate" && self.seg_blocked(owner, e, &seg) {
                    continue;
                }
                let mid = [(seg.a[0] + seg.b[0]) / 2.0, (seg.a[1] + seg.b[1]) / 2.0];
                let cells = vec![owner, owner.neighbor(e)];
                world.push((piece, mid, along, vec![seg], cells.clone()));
                if piece == "hedge_oak" {
                    let yaw = (edge_hash(self.world_seed, owner, e, 4) % 360) as f32;
                    world.push((HEDGEROW_OAK, mid, yaw, vec![], cells));
                }
            }
        }
        for j in 0..6u8 {
            world.extend(self.corner_pieces(owner, j));
        }
        let [cx, cz] = centre(owner);
        let turn = -60.0 * (rot % 6) as f32;
        let (s, c) = sin_cos_deg(turn);
        world
            .into_iter()
            .map(|(piece, p, yaw, segs, cells)| {
                let (dx, dz) = (p[0] - cx, p[1] - cz);
                Placement {
                    piece,
                    at: [dx * c - dz * s, dx * s + dz * c],
                    yaw_deg: (yaw + turn).rem_euclid(360.0),
                    segs,
                    cells,
                }
            })
            .collect()
    }

    /// The corner pieces at world corner `j` of `owner`, if `owner` owns that corner: world spot,
    /// yaw and footprints.
    fn corner_pieces(&self, owner: Axial, j: u8) -> Vec<Spot> {
        let n1 = owner.neighbor(j);

        // The three edges at the corner as (cell, world edge, arm direction from the corner).
        let edges = [
            (owner, j % 6, 60.0 * j as f32 - 90.0),
            (owner, (j + 1) % 6, 60.0 * j as f32 + 150.0),
            (n1, (j + 2) % 6, 60.0 * j as f32 + 30.0),
        ];
        let v = corner(owner, j);
        let mut arms: Vec<(Kind, f32, Seg)> = vec![];
        let mut fields = vec![];
        for &(a, e, angle) in &edges {
            let Some(field) = self.boundary(a, e) else {
                continue;
            };
            let Some(kind) = self.kind(a, e) else {
                continue;
            };
            fields.push(field);
            let seg = Seg {
                a: v,
                b: add(v, dir(angle), ARM),
                half: kind.half_width(),
            };
            if (self.blocked)(a, &seg) || (self.blocked)(a.neighbor(e), &seg) {
                continue;
            }
            arms.push((kind, angle.rem_euclid(360.0), seg));
        }

        if fields.iter().min() != Some(&owner) {
            return vec![];
        }
        let mut out = vec![];
        for kind in [Kind::Wall, Kind::Hedge] {
            let mine: Vec<&(Kind, f32, Seg)> = arms.iter().filter(|a| a.0 == kind).collect();
            let segs: Vec<Seg> = mine.iter().map(|a| a.2).collect();
            let (name, yaw) = match mine.as_slice() {
                [] => continue,
                [a] => ("end", a.1),
                [a, b] => {
                    // The bend's arms sit at 0 and -120 degrees in its frame: yaw on the arm whose
                    // partner lies 120 degrees clockwise (at -120) of it.
                    let (x, y) = (a.1, b.1);
                    if ((x - y).rem_euclid(360.0) - 120.0).abs() < 1.0 {
                        ("bend", x)
                    } else {
                        ("bend", y)
                    }
                }
                [a, ..] => ("junction", a.1),
            };
            let piece = match (kind, name) {
                (Kind::Wall, "end") => "wall_end",
                (Kind::Wall, "bend") => "wall_bend",
                (Kind::Wall, _) => "wall_junction",
                (Kind::Hedge, "end") => "hedge_end",
                (Kind::Hedge, "bend") => "hedge_bend",
                (Kind::Hedge, _) => "hedge_junction",
            };
            debug_assert!(piece.starts_with(kind.prefix()));
            out.push((
                piece,
                v,
                yaw,
                segs,
                vec![owner, n1, owner.neighbor((j + 1) % 6)],
            ));
        }
        out
    }
}

/// Every world footprint line of the boundaries drawn by `owners` (field tiles; others add none).
pub fn segments<'a, A, B>(
    view: &View<'a, A, B>,
    owners: impl IntoIterator<Item = Axial>,
) -> Vec<Seg>
where
    A: Fn(Axial) -> Option<(&'a TileDef, u8)>,
    B: Fn(Axial, &Seg) -> bool,
{
    owners
        .into_iter()
        .flat_map(|c| view.placements(c))
        .flat_map(|p| p.segs)
        .collect()
}

/// The cells within `radius` of `cell` (itself included).
pub fn within(cell: Axial, radius: i32) -> impl Iterator<Item = Axial> {
    (-radius..=radius).flat_map(move |dq| {
        (-radius..=radius).filter_map(move |dr| {
            let c = Axial::new(cell.q + dq, cell.r + dr);
            (c.distance(cell) <= radius).then_some(c)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TileCatalog, World};

    fn catalog() -> TileCatalog {
        TileCatalog::builtin().unwrap()
    }

    fn placements(c: &TileCatalog, w: &World, cell: Axial, seed: u64) -> Vec<Placement> {
        let view = View::new(
            seed,
            |x: Axial| w.tile(x).and_then(|t| Some((c.get(&t.def)?, t.rotation))),
            |_: Axial, _: &Seg| false,
        );
        view.placements(cell)
    }

    fn world(tiles: &[(i32, i32, &str, u8)]) -> (TileCatalog, World) {
        let c = catalog();
        let mut w = World::new(1);
        for &(q, r, id, rot) in tiles {
            w.place(
                &c,
                Axial::new(q, r),
                id,
                crate::world::Orientation::new(rot, 0),
            )
            .unwrap_or_else(|e| panic!("{id} at {q},{r}: {e:?}"));
        }
        (c, w)
    }

    #[test]
    fn a_farm_beside_a_meadow_gets_one_fenced_edge_with_a_gate() {
        let (c, w) = world(&[(1, 0, "farm", 0)]);
        let p = placements(&c, &w, Axial::new(1, 0), 7);
        // The start meadow at the origin is across the farm's world edge 3.
        let runs: Vec<&str> = p
            .iter()
            .map(|p| p.piece)
            .filter(|n| n.contains("run") || n.contains("gate"))
            .collect();
        assert_eq!(runs.len(), 3, "{p:?}");
        assert!(runs.contains(&"wall_gate"), "{runs:?}");
        let ends: Vec<&str> = p
            .iter()
            .map(|p| p.piece)
            .filter(|n| n.ends_with("_end"))
            .collect();
        assert_eq!(ends, ["wall_end", "wall_end"], "{p:?}");
        // Every footprint lies on the shared edge line, 13.856 m from the farm's centre.
        let fc = Axial::new(1, 0).to_world(HEX_RADIUS);
        for s in p.iter().flat_map(|p| &p.segs) {
            for q in [s.a, s.b] {
                let d = ((q[0] - fc[0]).powi(2) + (q[1] - fc[1]).powi(2)).sqrt();
                assert!((13.0..=16.01).contains(&d), "{q:?} at {d}");
            }
        }
    }

    #[test]
    fn nothing_without_a_land_neighbour_and_none_on_non_field_tiles() {
        let (c, w) = world(&[(1, 0, "road_straight", 1)]);
        assert!(placements(&c, &w, Axial::new(0, 0), 3).is_empty());
        assert!(placements(&c, &w, Axial::new(1, 0), 3).is_empty());
        let (c, w) = world(&[(1, 0, "farm", 0), (2, 0, "wheat_field", 0)]);
        // Farm and wheat field are both fields: no boundary between them.
        let p = placements(&c, &w, Axial::new(2, 0), 3);
        assert!(p.is_empty(), "{p:?}");
    }

    #[test]
    fn a_corner_where_two_boundary_edges_of_one_kind_meet_draws_one_bend() {
        // The farm east of the start meadow, a second meadow below both: the farm's world edges 3
        // and 2 are boundaries and meet at its world corner 2.
        let (c, w) = world(&[(1, 0, "farm", 0), (0, 1, "meadow", 0)]);
        for seed in 0..64u64 {
            let p = placements(&c, &w, Axial::new(1, 0), seed);
            let corner_pieces: Vec<&str> = p
                .iter()
                .map(|p| p.piece)
                .filter(|n| n.ends_with("_end") || n.ends_with("_bend") || n.ends_with("_junction"))
                .collect();
            let bends = corner_pieces
                .iter()
                .filter(|n| n.ends_with("_bend"))
                .count();
            let ends = corner_pieces.iter().filter(|n| n.ends_with("_end")).count();
            // Two outer ends, and at the shared corner one bend (same kind) or two ends (mixed).
            assert!(
                (bends == 1 && ends == 2) || (bends == 0 && ends == 4),
                "seed {seed}: {corner_pieces:?}"
            );
            assert!(!corner_pieces.iter().any(|n| n.ends_with("_junction")));
        }
    }

    #[test]
    fn bend_yaw_puts_both_arms_on_the_edges() {
        let (c, w) = world(&[(1, 0, "farm", 0), (0, 1, "meadow", 0)]);
        for seed in 0..64u64 {
            let p = placements(&c, &w, Axial::new(1, 0), seed);
            for b in p.iter().filter(|p| p.piece.ends_with("_bend")) {
                // Arms at 0 and -120 degrees in the piece frame, turned by the yaw, run along the
                // two segments the bend covers.
                let mut want: Vec<f32> = b
                    .segs
                    .iter()
                    .map(|s| {
                        (s.b[1] - s.a[1])
                            .atan2(s.b[0] - s.a[0])
                            .to_degrees()
                            .rem_euclid(360.0)
                    })
                    .collect();
                want.sort_by(f32::total_cmp);
                // Segments are in the world; the farm has rotation 0, so local = world angles.
                let mut got = vec![
                    b.yaw_deg.rem_euclid(360.0),
                    (b.yaw_deg - 120.0).rem_euclid(360.0),
                ];
                got.sort_by(f32::total_cmp);
                for (g, w) in got.iter().zip(&want) {
                    let d = (g - w).rem_euclid(360.0);
                    assert!(d.min(360.0 - d) < 0.01, "seed {seed}: {got:?} vs {want:?}");
                }
            }
        }
    }

    #[test]
    fn kinds_split_about_evenly_and_each_farm_has_one_gate() {
        let (c, w) = world(&[(1, 0, "farm", 0), (0, 1, "meadow", 0), (1, 1, "forest", 0)]);
        let mut walls = 0;
        let mut total = 0;
        for seed in 0..400u64 {
            let p = placements(&c, &w, Axial::new(1, 0), seed);
            assert_eq!(
                p.iter().filter(|p| p.piece == "wall_gate").count(),
                1,
                "seed {seed}"
            );
            for x in p.iter().filter(|p| p.piece.contains("_run")) {
                total += 1;
                walls += x.piece.starts_with("wall") as u32;
            }
        }
        let share = walls as f32 / total as f32;
        assert!((0.4..0.75).contains(&share), "wall share {share}");
    }

    #[test]
    fn placements_turn_with_the_owner() {
        // The same layout turned: a farm at rotation r draws the same world footprints.
        let (c, w0) = world(&[(1, 0, "farm", 0)]);
        let (_, w3) = world(&[(1, 0, "farm", 3)]);
        let a = placements(&c, &w0, Axial::new(1, 0), 5);
        let b = placements(&c, &w3, Axial::new(1, 0), 5);
        assert_eq!(a.len(), b.len());
        // The barn moves with the rotation, so only the footprints (world) are compared by set.
        let key = |p: &[Placement]| {
            let mut v: Vec<(i32, i32)> = p
                .iter()
                .flat_map(|p| &p.segs)
                .map(|s| {
                    (
                        ((s.a[0] + s.b[0]) * 50.0) as i32,
                        ((s.a[1] + s.b[1]) * 50.0) as i32,
                    )
                })
                .collect();
            v.sort();
            v
        };
        assert_eq!(key(&a), key(&b));
    }
}
