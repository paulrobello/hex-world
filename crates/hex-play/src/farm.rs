//! Farm animals wander inside their paddock (PRD §7.2, P3).
//!
//! A tile whose heroes include a fence ([`scatter::fence`]) keeps the animal heroes standing inside
//! its line as a [`Herd`]: each animal walks slowly between seeded spots inside the paddock and
//! pauses there, a grazing piece (`animal_cow_b`, `animal_sheep_c`) mostly grazing, a standing
//! piece idling. Each keeps its piece, so the grazers' lowered heads are the grazing pauses.
//!
//! - **Bodies.** An animal is a capsule over its kit LOD0 mesh and collider ([`BODIES`]): a segment
//!   along its +z with a radius of its half width. It turns about the segment's middle at
//!   [`TURN_STEP`] a tick and walks only while it faces within [`WALK_CONE`] of its goal.
//! - **Room.** A move or turn is taken only when the new capsule stays [`FENCE_CLEAR`] inside the
//!   paddock's line (closed across the gate, so the open gate is a wall to them), [`ANIMAL_GAP`]
//!   clear of every other animal, clear of the rocks scatter dropped inside, and
//!   [`WALKER_CLEAR`] clear of the walker. While the walker stands within that of an animal, it
//!   neither moves nor turns: it waits.
//! - **Ground.** The pose stands on the meadow relief ([`meadow_relief`], the `meadow_base` cap):
//!   y and tilt are the plane fitted through the relief under the four hooves.
//! - **Determinism.** The state is a pure function of the tile's slots, the world seed and the
//!   ticks run (and the walker's path): headings are unit vectors turned by fixed cosine and sine
//!   pairs, the random draws a SplitMix64 per animal, and every decision uses `+ - * /` and `sqrt`
//!   alone, so native and wasm agree. Trigonometry feeds only the drawn pose (the relief). Herds
//!   are re-derived from the view (a loaded world starts every animal at its slot) and never
//!   saved.
//! - **Drawing.** `play::sync` spawns each herd animal as a [`HerdAnimal`] parent at its slot with
//!   the kit parts as children (the rise grows them as props); `play::chunks` leaves that subtree
//!   on the per-entity path, so the parts still batch by mesh and material and the parent's
//!   transform is the only per-frame write. The ghost, the thumbnails and the grass keep the
//!   tile's static layout.
use std::collections::BTreeMap;

use bevy::prelude::*;
use hex_core::{Axial, HEX_RADIUS, TileDef, cell_seed};

use crate::{
    ActiveScenario, AgentsPaused, Scenario,
    play::{
        PlayCatalog, PlaySystems, PlayView,
        grass::cos_sin_deg,
        rise::PlayRises,
        scatter::{self, Layout, ScatterLayouts, footprint, turn},
    },
    walk::{BODY_RADIUS, OBSTACLES, WalkGround},
};

/// Seconds per herd tick: the 20 Hz fixed tick the cars and trains run on.
pub const TICK_SECONDS: f32 = 1.0 / 20.0;
/// Clearance (m) from a capsule to the fence line: the rails' 0.12 m collider half width, a lean
/// margin and a gap.
pub const FENCE_CLEAR: f32 = 0.35;
/// Clearance (m) between two animals' capsules.
pub const ANIMAL_GAP: f32 = 0.25;
/// Clearance (m) from a capsule to a rock's ground reach.
pub const ROCK_GAP: f32 = 0.2;
/// Clearance (m) from a capsule to the walker's centre: its body radius and a gap.
pub const WALKER_CLEAR: f32 = BODY_RADIUS + 0.5;
/// Heading change per tick: 2° (40°/s), as a fixed cosine and sine.
pub const TURN_STEP: (f32, f32) = (0.999_390_8, 0.034_899_5);
/// An animal walks only while its heading is within 30° of its goal (the cosine).
pub const WALK_CONE: f32 = 0.866_025_4;
/// Seconds to reach walking speed from a standstill.
const RAMP_SECONDS: f32 = 1.2;
/// Within this many metres of its goal an animal slows down.
const SLOW_RADIUS: f32 = 0.8;
/// An animal this close (m) to its goal has arrived.
const ARRIVE: f32 = 0.1;
/// Ticks an animal tries a blocked walk before it picks another goal (0.6 s).
const BLOCKED_TICKS: u16 = 12;

/// One animal piece: its capsule (along +z from `back` to `front`, `radius` its half width), the
/// hooves under its hips (x, z), its walking speed and whether it is a grazing pose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub back: f32,
    pub front: f32,
    pub radius: f32,
    pub hooves: [[f32; 2]; 4],
    /// Walking speed (m/s).
    pub speed: f32,
    pub grazer: bool,
}

impl Body {
    /// The capsule segment's half length and its middle's offset along +z from the pivot.
    fn half(&self) -> f32 {
        (self.front - self.back) / 2.0
    }

    fn mid(&self) -> f32 {
        (self.front + self.back) / 2.0
    }

    /// Ticks of a pause and the metres of a walk leg, as (min, max).
    fn pause(&self) -> (u32, u32) {
        if self.grazer { (300, 700) } else { (120, 400) }
    }

    fn leg(&self) -> (f32, f32) {
        if self.grazer { (0.8, 2.5) } else { (2.0, 7.0) }
    }
}

/// The herd pieces (`bodies_cover_the_kit_meshes` checks the capsules against every LOD0 and
/// collider vertex, `hooves_match_the_bake_reports` the hooves against `animal.hips`).
pub const BODIES: &[(&str, Body)] = &[
    (
        "animal_cow",
        Body {
            back: -1.045,
            front: 1.461,
            radius: 0.34,
            hooves: [[0.18, 0.55], [-0.18, 0.55], [0.17, -0.58], [-0.17, -0.58]],
            speed: 0.35,
            grazer: false,
        },
    ),
    (
        "animal_cow_b",
        Body {
            back: -1.045,
            front: 1.311,
            radius: 0.34,
            hooves: [[0.18, 0.55], [-0.18, 0.55], [0.17, -0.58], [-0.17, -0.58]],
            speed: 0.3,
            grazer: true,
        },
    ),
    (
        "animal_sheep",
        Body {
            back: -0.582,
            front: 0.72,
            radius: 0.33,
            hooves: [[0.13, 0.25], [-0.13, 0.25], [0.13, -0.27], [-0.13, -0.27]],
            speed: 0.32,
            grazer: false,
        },
    ),
    (
        "animal_sheep_b",
        Body {
            back: -0.338,
            front: 0.429,
            radius: 0.191,
            hooves: [
                [0.078, 0.15],
                [-0.078, 0.15],
                [0.078, -0.162],
                [-0.078, -0.162],
            ],
            speed: 0.4,
            grazer: false,
        },
    ),
    (
        "animal_sheep_c",
        Body {
            back: -0.582,
            front: 0.806,
            radius: 0.33,
            hooves: [[0.13, 0.25], [-0.13, 0.25], [0.13, -0.27], [-0.13, -0.27]],
            speed: 0.3,
            grazer: true,
        },
    ),
];

/// The body of herd piece `piece`.
pub fn body(piece: &str) -> Option<&'static Body> {
    BODIES.iter().find(|(id, _)| *id == piece).map(|(_, b)| b)
}

/// The meadow relief under tile-frame `(x, z)`: `relief()` in scripts/blender/kit/meadow_base.py
/// at Blender `(x, -z)`, 0 at the border ring and down to -0.25 m inside.
pub fn meadow_relief(x: f32, z: f32) -> f32 {
    let (bx, by) = (x, -z);
    let apothem = HEX_RADIUS * 3.0f32.sqrt() / 2.0;
    let (h, k) = (0.5, 3.0f32.sqrt() / 2.0);
    let reach = bx
        .abs()
        .max((h * bx + k * by).abs())
        .max((h * bx - k * by).abs());
    let t = ((apothem - reach) / 2.0).clamp(0.0, 1.0);
    let falloff = t * t * (3.0 - 2.0 * t);
    let swell = 0.6 * (0.31 * bx + 1.7).sin() * (0.27 * by - 0.4).cos()
        + 0.4 * (0.53 * bx - 0.45 * by + 0.9).sin();
    -0.25 * falloff * (0.5 + 0.5 * swell)
}

/// The paddock of `def` in the tile frame: its first hero fence's line, closed across the gate.
pub fn paddock(def: &TileDef) -> Option<Vec<Vec2>> {
    scatter::fence_lines(def).into_iter().next()
}

/// Whether `p` is inside the closed polygon `poly` (even-odd).
fn inside(poly: &[Vec2], p: Vec2) -> bool {
    let mut odd = false;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            odd = !odd;
        }
    }
    odd
}

/// The hero slots of `def` that wander: animal heroes standing inside its paddock.
pub fn herd_slots(def: &TileDef) -> Vec<usize> {
    let Some(pen) = paddock(def) else {
        return vec![];
    };
    def.props
        .hero
        .iter()
        .enumerate()
        .filter(|(_, h)| body(h.piece.as_str()).is_some() && inside(&pen, Vec2::from(h.at)))
        .map(|(i, _)| i)
        .collect()
}

/// The scattered instances of `layout` that block the walker (the farm's rocks; wildflowers do
/// not), as tile-frame centres and ground reaches: what a herd walks round.
pub fn obstacles_of(def: &TileDef, layout: &Layout) -> Vec<(Vec2, f32)> {
    layout
        .instances
        .iter()
        .filter_map(|i| {
            let piece = i.piece(def.props.scatter.get(i.rule as usize)?.piece.as_str());
            OBSTACLES
                .contains(&piece)
                .then(|| (i.pos.xz(), footprint(piece).ground * i.scale))
        })
        .collect()
}

/// SplitMix64 step.
fn split_mix(z: &mut u64) -> u64 {
    *z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut x = *z;
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// A uniform draw in [0, 1) from 24 bits.
fn unit(z: &mut u64) -> f32 {
    (split_mix(z) >> 40) as f32 / (1u64 << 24) as f32
}

/// What an animal is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    /// Standing still for this many more ticks: grazing for a grazing piece, idle otherwise.
    Pause(u32),
    /// Walking to a spot in the paddock.
    Walk(Vec2),
}

/// One animal of a herd, in the tile frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Animal {
    pub slot: usize,
    pub piece: &'static str,
    pub body: &'static Body,
    /// The capsule's middle and the unit heading (+z of the piece).
    pub pos: Vec2,
    pub dir: Vec2,
    prev_pos: Vec2,
    prev_dir: Vec2,
    pub mode: Mode,
    /// The walker stood too close on the latest tick.
    pub waiting: bool,
    ramp: f32,
    blocked: u16,
    rng: u64,
}

impl Animal {
    /// The capsule segment's ends at (`pos`, `dir`).
    fn segment(&self, pos: Vec2, dir: Vec2) -> (Vec2, Vec2) {
        let h = self.body.half();
        (pos + dir * h, pos - dir * h)
    }

    /// `pause`, `graze`, `walk` or `wait`.
    pub fn state(&self) -> &'static str {
        match (self.waiting, self.mode) {
            (true, _) => "wait",
            (_, Mode::Walk(_)) => "walk",
            (_, Mode::Pause(_)) if self.body.grazer => "graze",
            _ => "idle",
        }
    }
}

/// Distance from `p` to segment `a`-`b`.
fn point_segment(p: Vec2, (a, b): (Vec2, Vec2)) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Least distance between segments `p` and `q` (none of the farm's segments cross while apart:
/// a crossing reads as the least endpoint distance, still below any clearance it breaks).
fn segment_segment(p: (Vec2, Vec2), q: (Vec2, Vec2)) -> f32 {
    let cross = |o: Vec2, a: Vec2, b: Vec2| (a - o).perp_dot(b - o);
    let (d1, d2) = (cross(q.0, q.1, p.0), cross(q.0, q.1, p.1));
    let (d3, d4) = (cross(p.0, p.1, q.0), cross(p.0, p.1, q.1));
    if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
        return 0.0;
    }
    point_segment(p.0, q)
        .min(point_segment(p.1, q))
        .min(point_segment(q.0, p))
        .min(point_segment(q.1, p))
}

/// The animals of one farm tile and the room they share, in the tile frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Herd {
    /// The paddock's edges as (a point on it, its inward unit normal).
    fence: Vec<(Vec2, Vec2)>,
    /// The bounds the walk goals are drawn in.
    lo: Vec2,
    hi: Vec2,
    /// Rocks inside the paddock: centre and ground reach.
    rocks: Vec<(Vec2, f32)>,
    pub animals: Vec<Animal>,
    pub ticks: u64,
}

impl Herd {
    /// The herd of `def` at `cell` in a world of `world_seed`, every animal at its slot, among
    /// `rocks` (tile frame centres and ground reaches). `None` for a tile without one.
    pub fn new(def: &TileDef, cell: Axial, world_seed: u64, rocks: &[(Vec2, f32)]) -> Option<Self> {
        let slots = herd_slots(def);
        let pen = paddock(def)?;
        if slots.is_empty() || pen.len() < 3 {
            return None;
        }
        let area: f32 = (0..pen.len())
            .map(|i| pen[i].perp_dot(pen[(i + 1) % pen.len()]))
            .sum();
        let fence = (0..pen.len())
            .filter_map(|i| {
                let (a, b) = (pen[i], pen[(i + 1) % pen.len()]);
                let n = (b - a).perp().try_normalize()?;
                Some((a, if area > 0.0 { n } else { -n }))
            })
            .collect();
        let (lo, hi) = pen.iter().fold((Vec2::MAX, Vec2::MIN), |(lo, hi), p| {
            (lo.min(*p), hi.max(*p))
        });
        let seed = cell_seed(world_seed, cell);
        let animals = slots
            .into_iter()
            .map(|slot| {
                let h = &def.props.hero[slot];
                let piece: &'static str = BODIES
                    .iter()
                    .find(|(id, _)| *id == h.piece.as_str())
                    .map(|(id, _)| *id)
                    .expect("herd_slots keeps bodies");
                let body = body(piece).expect("herd_slots keeps bodies");
                // `Quat::from_rotation_y(-yaw)` takes +z to (-sin, cos) in (x, z).
                let cs = cos_sin_deg(h.yaw_deg);
                let dir = Vec2::new(-cs.y, cs.x);
                let pos = Vec2::from(h.at) + dir * body.mid();
                let mut rng = seed ^ (slot as u64).wrapping_mul(0xD1B5_4A32_D192_ED03);
                let (a, b) = body.pause();
                let first = (unit(&mut rng) * b.max(a) as f32) as u32;
                Animal {
                    slot,
                    piece,
                    body,
                    pos,
                    dir,
                    prev_pos: pos,
                    prev_dir: dir,
                    mode: Mode::Pause(first),
                    waiting: false,
                    ramp: 0.0,
                    blocked: 0,
                    rng,
                }
            })
            .collect();
        Some(Self {
            fence,
            lo,
            hi,
            rocks: rocks
                .iter()
                .copied()
                .filter(|(c, _)| inside(&pen, *c))
                .collect(),
            animals,
            ticks: 0,
        })
    }

    /// The room animal `i` has at (`pos`, `dir`): the least clearance left to the fence, the rocks
    /// and the other animals (negative when one is broken), and to the walker.
    fn room(&self, i: usize, pos: Vec2, dir: Vec2, walker: Option<Vec2>) -> (f32, f32) {
        let a = &self.animals[i];
        let r = a.body.radius;
        let seg = a.segment(pos, dir);
        let mut least = f32::MAX;
        for (p, n) in &self.fence {
            least = least
                .min((seg.0 - *p).dot(*n) - r - FENCE_CLEAR)
                .min((seg.1 - *p).dot(*n) - r - FENCE_CLEAR);
        }
        for (c, rr) in &self.rocks {
            least = least.min(point_segment(*c, seg) - r - rr - ROCK_GAP);
        }
        for (j, b) in self.animals.iter().enumerate() {
            if j != i {
                let other = b.segment(b.pos, b.dir);
                least = least.min(segment_segment(seg, other) - r - b.body.radius - ANIMAL_GAP);
            }
        }
        let walker = walker.map_or(f32::MAX, |w| point_segment(w, seg) - r - WALKER_CLEAR);
        (least, walker)
    }

    /// Whether a pose with `room` may replace one with `now`: it breaks no clearance, or it breaks
    /// less than the current pose does (a slot authored closer than the gaps works itself free).
    fn better(room: (f32, f32), now: (f32, f32)) -> bool {
        room.1 >= 0.0 && (room.0 >= 0.0 || room.0 > now.0)
    }

    /// A walk goal for animal `i`: a spot in the paddock a leg's length away with room for the
    /// animal facing it, reached on a straight path clear of the rocks and of the animals standing
    /// still (a walking one moves on). The first draws keep to the half it faces, so it rarely
    /// turns about against the fence. 48 draws at most.
    fn goal(&mut self, i: usize) -> Option<Vec2> {
        let (lo, hi) = (self.lo, self.hi);
        let (min, max) = self.animals[i].body.leg();
        let standing: Vec<(Vec2, f32)> = self
            .animals
            .iter()
            .enumerate()
            .filter(|(j, b)| *j != i && matches!(b.mode, Mode::Pause(_)))
            .map(|(_, b)| (b.pos, b.body.half() + b.body.radius))
            .collect();
        for k in 0..48 {
            let a = &mut self.animals[i];
            let p = Vec2::new(
                lo.x + (hi.x - lo.x) * unit(&mut a.rng),
                lo.y + (hi.y - lo.y) * unit(&mut a.rng),
            );
            let to = p - a.pos;
            let d = to.length();
            if !(min..=max).contains(&d) || (k < 16 && a.dir.dot(to) < 0.0) {
                continue;
            }
            let dir = to / d;
            let seg = a.segment(p, dir);
            let r = a.body.radius;
            let fenced = self.fence.iter().all(|(q, n)| {
                (seg.0 - *q).dot(*n) - r - FENCE_CLEAR >= 0.0
                    && (seg.1 - *q).dot(*n) - r - FENCE_CLEAR >= 0.0
            });
            // The path the capsule's middle takes, with its reach ahead of it: it keeps clear of
            // each, or comes no nearer one it already stands closer to.
            let path = (a.pos, p + dir * a.body.half());
            let clear = |(c, reach): &(Vec2, f32), gap: f32| {
                let need = r + reach + gap;
                point_segment(*c, path) >= need.min(c.distance(a.pos) - 1e-3)
            };
            if fenced
                && self.rocks.iter().all(|x| clear(x, ROCK_GAP))
                && standing.iter().all(|x| clear(x, ANIMAL_GAP))
            {
                return Some(p);
            }
        }
        None
    }

    fn pause(&mut self, i: usize, short: bool) {
        let a = &mut self.animals[i];
        let (lo, hi) = if short { (20, 60) } else { a.body.pause() };
        let n = lo + (unit(&mut a.rng) * (hi - lo) as f32) as u32;
        a.mode = Mode::Pause(n);
        a.ramp = 0.0;
        a.blocked = 0;
    }

    /// One tick of [`TICK_SECONDS`] for every animal in slot order; `walker` is the walker's
    /// feet in the tile frame while walk mode is on.
    pub fn tick(&mut self, walker: Option<Vec2>) {
        self.ticks += 1;
        for i in 0..self.animals.len() {
            let a = &mut self.animals[i];
            a.prev_pos = a.pos;
            a.prev_dir = a.dir;
            let (pos, dir) = (a.pos, a.dir);
            let now = self.room(i, pos, dir, walker);
            let a = &mut self.animals[i];
            a.waiting = now.1 < 0.0;
            if a.waiting {
                a.ramp = 0.0;
                continue;
            }
            let mode = a.mode;
            let target = match mode {
                Mode::Pause(n) if n > 0 => {
                    self.animals[i].mode = Mode::Pause(n - 1);
                    continue;
                }
                Mode::Pause(_) => {
                    match self.goal(i) {
                        Some(g) => self.animals[i].mode = Mode::Walk(g),
                        None => self.pause(i, true),
                    }
                    continue;
                }
                Mode::Walk(t) => t,
            };
            let to = target - pos;
            let d = to.length();
            if d < ARRIVE {
                self.pause(i, false);
                continue;
            }
            let want = to / d;
            let c = dir.dot(want);
            let new_dir = if c >= TURN_STEP.0 {
                want
            } else {
                let (cs, sn) = (TURN_STEP.0, TURN_STEP.1 * dir.perp_dot(want).signum());
                Vec2::new(dir.x * cs - dir.y * sn, dir.x * sn + dir.y * cs).normalize()
            };
            let a = &mut self.animals[i];
            let v = if c >= WALK_CONE {
                a.body.speed * a.ramp * (d / SLOW_RADIUS).min(1.0)
            } else {
                0.0
            };
            a.ramp = (a.ramp + TICK_SECONDS / RAMP_SECONDS).min(1.0);
            let step = pos + new_dir * (v * TICK_SECONDS).min(d);
            if Self::better(self.room(i, step, new_dir, walker), now) {
                let a = &mut self.animals[i];
                (a.pos, a.dir) = (step, new_dir);
                if v > 0.0 {
                    a.blocked = 0;
                }
                continue;
            }
            let turned = Self::better(self.room(i, pos, new_dir, walker), now);
            // Wedged (both ends at the fence in a corner, a neighbour at its shoulder): a
            // half-speed shuffle back or aside, turning or not, the one leaving it the most room.
            let shuffle = if turned {
                None
            } else {
                let s = self.animals[i].body.speed * 0.5 * TICK_SECONDS;
                let side = dir.perp();
                [
                    -dir,
                    side,
                    -side,
                    (side - dir) * std::f32::consts::FRAC_1_SQRT_2,
                    (-side - dir) * std::f32::consts::FRAC_1_SQRT_2,
                ]
                .into_iter()
                .flat_map(|o| [(pos + o * s, new_dir), (pos + o * s, dir)])
                .map(|(p, d)| (p, d, self.room(i, p, d, walker)))
                .filter(|(_, _, room)| Self::better(*room, now))
                .max_by(|a, b| a.2.0.total_cmp(&b.2.0))
                .map(|(p, d, _)| (p, d))
            };
            let a = &mut self.animals[i];
            if turned {
                a.dir = new_dir;
            } else if let Some((p, d)) = shuffle {
                (a.pos, a.dir) = (p, d);
            }
            a.blocked += 1;
            a.ramp = 0.0;
            if a.blocked > BLOCKED_TICKS {
                a.blocked = 0;
                match self.goal(i) {
                    Some(g) => self.animals[i].mode = Mode::Walk(g),
                    None => self.pause(i, true),
                }
            }
        }
    }

    /// Animal `i`'s pose in the tile frame `alpha` of the way from the previous tick to the
    /// latest: its pivot on the plane fitted through `ground` (tile-frame height) under its
    /// hooves, turned to its heading and tilted to that plane.
    pub fn pose(&self, i: usize, alpha: f32, ground: &dyn Fn(Vec2) -> f32) -> Transform {
        let a = &self.animals[i];
        let pos = a.prev_pos.lerp(a.pos, alpha);
        let dir = a
            .prev_dir
            .lerp(a.dir, alpha)
            .try_normalize()
            .unwrap_or(a.dir);
        pose_on_ground(a.body, pos, dir, ground)
    }
}

/// [`meadow_relief`] as a ground for [`Herd::pose`]: the formula the cap was built from, within
/// about 2 cm of the drawn cap where the border falloff bends it between the cap's vertices.
pub fn relief_ground(p: Vec2) -> f32 {
    meadow_relief(p.x, p.y)
}

/// A body's pose with its capsule's middle at `pos`, facing `dir`, standing on `ground`.
pub fn pose_on_ground(
    body: &Body,
    pos: Vec2,
    dir: Vec2,
    ground: &dyn Fn(Vec2) -> f32,
) -> Transform {
    let pivot = pos - dir * body.mid();
    // The piece's +x (its left) and +z in the tile frame.
    let (x_axis, z_axis) = (Vec2::new(dir.y, -dir.x), dir);
    let hooves = body.hooves.map(|h| {
        let p = pivot + x_axis * h[0] + z_axis * h[1];
        Vec3::new(p.x, ground(p), p.y)
    });
    // Least squares y = a + b x + c z over the hooves, about their mean.
    let mean = hooves.iter().copied().sum::<Vec3>() / 4.0;
    let (mut sxx, mut sxz, mut szz, mut sxy, mut szy) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for h in hooves {
        let d = h - mean;
        sxx += d.x * d.x;
        sxz += d.x * d.z;
        szz += d.z * d.z;
        sxy += d.x * d.y;
        szy += d.z * d.y;
    }
    let det = sxx * szz - sxz * sxz;
    let (b, c) = if det.abs() > 1e-9 {
        ((sxy * szz - szy * sxz) / det, (szy * sxx - sxy * sxz) / det)
    } else {
        (0.0, 0.0)
    };
    let y = mean.y + b * (pivot.x - mean.x) + c * (pivot.y - mean.z);
    let yaw = Quat::from_mat3(&Mat3::from_cols(
        Vec3::new(x_axis.x, 0.0, x_axis.y),
        Vec3::Y,
        Vec3::new(z_axis.x, 0.0, z_axis.y),
    ));
    let tilt = Quat::from_rotation_arc(Vec3::Y, Vec3::new(-b, 1.0, -c).normalize());
    Transform::from_xyz(pivot.x, y, pivot.y).with_rotation(tilt * yaw)
}

/// A herd animal's parent entity under its tile root: its kit parts are children at identity.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HerdAnimal {
    pub cell: Axial,
    pub slot: u8,
}

/// The walker's feet (world XZ) while walk mode is on, written by `hex-app`.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct WalkerAt(pub Option<Vec2>);

/// One farm tile's herd.
#[derive(Clone, Debug, PartialEq)]
pub struct TileHerd {
    pub tile: String,
    pub rotation: u8,
    pub herd: Herd,
}

/// The herds of the drawn farm tiles, re-derived from the view: a tile new to it starts its herd
/// at the slots, and a replaced world (a new view epoch or seed) starts them all again.
#[derive(Resource, Default)]
pub struct FarmHerds {
    pub herds: BTreeMap<Axial, TileHerd>,
    epoch: u64,
    seed: u64,
}

impl FarmHerds {
    /// Every animal: its cell, slot, piece, state and tile-frame position.
    pub fn animals(&self) -> impl Iterator<Item = (Axial, &Animal)> {
        self.herds
            .iter()
            .flat_map(|(c, h)| h.herd.animals.iter().map(move |a| (*c, a)))
    }

    /// `play.farm` in the state JSON: herds, animals and each animal's world position and state.
    pub fn state_json(&self) -> serde_json::Value {
        let list: Vec<serde_json::Value> = self
            .herds
            .iter()
            .flat_map(|(cell, h)| {
                h.herd.animals.iter().map(move |a| {
                    let w = to_world(*cell, h.rotation, a.pos);
                    serde_json::json!({
                        "cell": [cell.q, cell.r],
                        "slot": a.slot,
                        "piece": a.piece,
                        "state": a.state(),
                        "pos": [round(w.x), round(w.y)],
                    })
                })
            })
            .collect();
        serde_json::json!({
            "herds": self.herds.len(),
            "animals": list.len(),
            "list": list,
        })
    }
}

fn round(v: f32) -> f32 {
    (v * 1000.0).round() / 1000.0
}

/// World XZ of tile-frame `p` on the tile at `cell` turned by `rotation`.
pub fn to_world(cell: Axial, rotation: u8, p: Vec2) -> Vec2 {
    let l = turn(rotation % 6, [p.x, 0.0, p.y]);
    Vec2::from(cell.to_world(HEX_RADIUS)) + Vec2::new(l[0], l[2])
}

/// Tile-frame position of world XZ `p` on the tile at `cell` turned by `rotation`.
pub fn to_tile(cell: Axial, rotation: u8, p: Vec2) -> Vec2 {
    let d = p - Vec2::from(cell.to_world(HEX_RADIUS));
    let l = turn((6 - rotation % 6) % 6, [d.x, 0.0, d.y]);
    Vec2::new(l[0], l[2])
}

pub struct FarmPlugin;

impl Plugin for FarmPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FarmHerds>()
            .init_resource::<WalkerAt>()
            .add_systems(FixedUpdate, tick)
            .add_systems(
                Update,
                (sync, pose)
                    .chain()
                    .after(PlaySystems)
                    .after(crate::walk::sync),
            );
    }
}

/// Starts a herd for every drawn farm tile once its scatter layout exists (its rocks), and drops
/// the herds of tiles gone from the view.
pub fn sync(
    active: Res<ActiveScenario>,
    view: Res<PlayView>,
    catalog: Res<PlayCatalog>,
    layouts: Res<ScatterLayouts>,
    mut herds: ResMut<FarmHerds>,
) {
    if active.0 != Scenario::Play {
        if !herds.herds.is_empty() {
            herds.herds.clear();
        }
        return;
    }
    if !view.is_changed() && !layouts.is_changed() && !active.is_changed() {
        return;
    }
    if herds.epoch != view.epoch || herds.seed != view.world_seed {
        herds.herds.clear();
        herds.epoch = view.epoch;
        herds.seed = view.world_seed;
    }
    herds.herds.retain(|c, h| {
        view.tiles
            .iter()
            .any(|t| t.cell == *c && t.tile == h.tile && t.rotation == h.rotation)
    });
    for t in &view.tiles {
        if herds.herds.contains_key(&t.cell) {
            continue;
        }
        let Some(def) = catalog.0.get(&t.tile) else {
            continue;
        };
        if herd_slots(def).is_empty() {
            continue;
        }
        let rocks: Vec<(Vec2, f32)> = match layouts.tiles.get(&t.cell) {
            Some(s) if s.tile == t.tile => obstacles_of(def, &s.layout),
            _ if def.props.scatter.is_empty() => vec![],
            _ => continue,
        };
        if let Some(herd) = Herd::new(def, t.cell, view.world_seed, &rocks) {
            herds.herds.insert(
                t.cell,
                TileHerd {
                    tile: t.tile.clone(),
                    rotation: t.rotation,
                    herd,
                },
            );
        }
    }
}

/// The fixed tick: every herd on a settled tile, while the play scenario runs and photo mode has
/// not paused the agents.
pub fn tick(
    active: Res<ActiveScenario>,
    paused: Res<AgentsPaused>,
    rises: Res<PlayRises>,
    walker: Res<WalkerAt>,
    mut herds: ResMut<FarmHerds>,
) {
    if active.0 != Scenario::Play || paused.0 {
        return;
    }
    for (cell, h) in herds.herds.iter_mut() {
        if rises.settled_cell(*cell) {
            let w = walker.0.map(|p| to_tile(*cell, h.rotation, p));
            h.herd.tick(w);
        }
    }
}

/// Poses every herd animal entity between the last two ticks on the drawn ground (the walk
/// ground's kit surface, the relief formula until it is built) and gives walk mode and photo
/// mode's click their drawn poses. Paused, the fraction of a tick holds where it was: the fixed
/// clock's overstep keeps cycling, and interpolating with it would rock each animal.
pub fn pose(
    paused: Res<AgentsPaused>,
    fixed: Res<Time<Fixed>>,
    herds: Res<FarmHerds>,
    mut animals: Query<(&HerdAnimal, &mut Transform)>,
    ground: Option<ResMut<WalkGround>>,
    mut held: Local<f32>,
) {
    if !paused.0 {
        *held = fixed.overstep_fraction();
    }
    let mut posed = vec![];
    for (a, mut t) in &mut animals {
        let Some(h) = herds.herds.get(&a.cell) else {
            continue;
        };
        let Some(i) = h
            .herd
            .animals
            .iter()
            .position(|x| x.slot == a.slot as usize)
        else {
            continue;
        };
        let (cell, rotation) = (a.cell, h.rotation);
        let on = |p: Vec2| {
            ground
                .as_deref()
                .and_then(|g| g.ground(to_world(cell, rotation, p)).ok())
                .unwrap_or_else(|| relief_ground(p))
        };
        let p = h.herd.pose(i, *held, &on);
        if *t != p {
            *t = p;
        }
        posed.push((a.cell, a.slot as usize, p));
    }
    if let Some(mut ground) = ground {
        ground.bypass_change_detection().pose_herd(&posed);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use hex_core::{SaveData, TileCatalog, World};

    use super::*;
    use crate::{
        kit_scene::piece_rotation,
        play::scatter::{layout, prepare},
        walk::{Blocked, Obstacle, hull, tests as walk_tests},
    };

    fn farm() -> TileDef {
        TileCatalog::builtin().unwrap().get("farm").unwrap().clone()
    }

    /// The farm's rocks at `cell` in a world of seed 1 (as `walk::tests::world` scatters it).
    fn rocks(def: &TileDef, cell: Axial) -> Vec<(Vec2, f32)> {
        let c = TileCatalog::builtin().unwrap();
        let prepared = prepare(def, &c, &mut |id| walk_tests::mesh_tris(id, "lod0")).unwrap();
        obstacles_of(def, &layout(def, cell_seed(1, cell), &prepared))
    }

    /// A piece's collider and LOD0 vertices (x, z) in its own frame.
    fn outline(piece: &str) -> Vec<Vec2> {
        ["collider", "lod0"]
            .iter()
            .flat_map(|m| walk_tests::mesh_tris(piece, m).unwrap())
            .flat_map(|(t, _)| t.map(|p| Vec2::new(p[0], p[2])))
            .collect()
    }

    /// The tile root's rest transform, as `play::sync` and walk place it.
    fn root(cell: Axial, rotation: u8) -> Transform {
        let [x, z] = cell.to_world(HEX_RADIUS);
        Transform::from_xyz(x, 0.0, z).with_rotation(piece_rotation(rotation))
    }

    /// Separating-axis test of two convex polygons.
    fn overlap(a: &[Vec2], b: &[Vec2]) -> bool {
        for poly in [a, b] {
            for i in 0..poly.len() {
                let n = (poly[(i + 1) % poly.len()] - poly[i]).perp();
                let span = |p: &[Vec2]| {
                    p.iter()
                        .map(|q| q.dot(n))
                        .fold((f32::MAX, f32::MIN), |(lo, hi), d| (lo.min(d), hi.max(d)))
                };
                let (s, t) = (span(a), span(b));
                if s.1 < t.0 || t.1 < s.0 {
                    return false;
                }
            }
        }
        true
    }

    #[test]
    fn the_relief_matches_the_kit_anchors() {
        // kit_scene.rs's relief heights at runtime spots (the windmill, hay bales, animals).
        for (x, z, y) in [
            (0.0, 0.0, -0.233),
            (-2.0, 2.5, -0.198),
            (-9.5, 1.0, -0.095),
            (-8.0, -4.5, -0.129),
            (7.0, 2.5, -0.075),
            (3.5, 7.0, -0.089),
            (2.0, 9.0, -0.059),
        ] {
            let h = meadow_relief(x, z);
            assert!((h - y).abs() < 1e-3, "({x}, {z}): {h} vs {y}");
        }
        // 0 on the border ring.
        assert!(meadow_relief(0.0, HEX_RADIUS).abs() < 1e-6);
    }

    /// The formula is the drawn cap to 2 cm over the paddock: 1 cm in its middle, more within the
    /// border falloff, which bends between the cap's 0.9 m vertices. So the hooves stand on the
    /// drawn surface (`farm::pose` samples the walk ground), and the formula stands in only until
    /// that surface is built.
    #[test]
    fn the_relief_matches_the_meadow_mesh() {
        let s = walk_tests::surface(&farm()).unwrap();
        let pen = paddock(&farm()).unwrap();
        let (mut worst, mut middle) = (0.0f32, 0.0f32);
        let mut n = 0;
        for i in -26..=26 {
            for j in 0..=30 {
                let p = Vec2::new(i as f32 * 0.5, j as f32 * 0.5);
                if !inside(&pen, p) {
                    continue;
                }
                let (h, slot) = s.at(p.x, p.y).unwrap();
                if slot != "meadow_base_cap" {
                    continue;
                }
                let e = (h - meadow_relief(p.x, p.y)).abs();
                let reach = scatter::hex_reach(Vec2::new(p.x, -p.y));
                if reach < HEX_RADIUS * 3.0f32.sqrt() / 2.0 - 2.0 {
                    middle = middle.max(e);
                }
                worst = worst.max(e);
                n += 1;
            }
        }
        assert!(n > 500, "{n} samples");
        assert!(middle < 0.01, "middle {middle}");
        assert!(worst < 0.02, "worst {worst}");
    }

    #[test]
    fn bodies_cover_the_kit_meshes() {
        for (piece, b) in BODIES {
            for p in outline(piece) {
                let z = p.y.clamp(b.back, b.front);
                let d = Vec2::new(p.x, p.y - z).length();
                assert!(d <= b.radius + 1e-3, "{piece}: {p} is {d} out");
            }
        }
    }

    #[test]
    fn hooves_match_the_bake_reports() {
        let reports = [
            (
                "animal_cow",
                include_str!("../../../assets-source/kit/animal_cow/bake-report.json"),
            ),
            (
                "animal_cow_b",
                include_str!("../../../assets-source/kit/animal_cow_b/bake-report.json"),
            ),
            (
                "animal_sheep",
                include_str!("../../../assets-source/kit/animal_sheep/bake-report.json"),
            ),
            (
                "animal_sheep_b",
                include_str!("../../../assets-source/kit/animal_sheep_b/bake-report.json"),
            ),
            (
                "animal_sheep_c",
                include_str!("../../../assets-source/kit/animal_sheep_c/bake-report.json"),
            ),
        ];
        for (piece, text) in reports {
            let v: serde_json::Value = serde_json::from_str(text).unwrap();
            let hips = v["animal"]["hips"].as_object().unwrap();
            let want: BTreeSet<[i32; 2]> = hips
                .values()
                .map(|h| {
                    [
                        (h[0].as_f64().unwrap() * 1000.0).round() as i32,
                        (h[2].as_f64().unwrap() * 1000.0).round() as i32,
                    ]
                })
                .collect();
            let have: BTreeSet<[i32; 2]> = body(piece)
                .unwrap()
                .hooves
                .iter()
                .map(|h| h.map(|v| (v * 1000.0).round() as i32))
                .collect();
            assert_eq!(have, want, "{piece}");
        }
    }

    /// The farm's six animals wander; the barn, the bales and the fence do not. The paddock is
    /// convex and closed across the gate.
    #[test]
    fn the_farm_herd_is_its_six_animals() {
        let def = farm();
        let slots = herd_slots(&def);
        let pieces: Vec<&str> = slots
            .iter()
            .map(|&i| def.props.hero[i].piece.as_str())
            .collect();
        assert_eq!(
            pieces,
            [
                "animal_cow",
                "animal_sheep",
                "animal_sheep",
                "animal_cow_b",
                "animal_sheep_b",
                "animal_sheep_c"
            ]
        );
        let pen = paddock(&def).unwrap();
        assert_eq!(pen.first(), Some(&Vec2::new(3.5, 2.5)));
        assert_eq!(pen.last(), Some(&Vec2::new(6.5, 2.5)));
        for i in 0..pen.len() {
            let (a, b, c) = (pen[i], pen[(i + 1) % pen.len()], pen[(i + 2) % pen.len()]);
            assert!((b - a).perp_dot(c - b) <= 1e-4, "convex at {b}");
        }
        // Every other tile has no herd.
        for d in TileCatalog::builtin().unwrap().tiles() {
            if d.id != "farm" {
                assert!(herd_slots(d).is_empty(), "{}", d.id);
            }
        }
    }

    /// Each animal starts at its slot (pivot and heading as authored) and works free of any gap
    /// the slots break (today the second sheep and the grazing sheep stand 0.33 m closer than
    /// [`ANIMAL_GAP`] allows): within ten minutes every animal has its room, and no animal's room
    /// ever shrinks below where it started. Prints how each spent its time (ticks walking, idle,
    /// grazing, blocked).
    #[test]
    fn every_animal_starts_at_its_slot_and_works_free() {
        let def = farm();
        let h = Herd::new(&def, Axial::ORIGIN, 1, &rocks(&def, Axial::ORIGIN)).unwrap();
        for (i, a) in h.animals.iter().enumerate() {
            let t = h.pose(i, 1.0, &relief_ground);
            let at = Vec2::from(def.props.hero[a.slot].at);
            assert!(t.translation.xz().distance(at) < 1e-4, "{}", a.piece);
            let yaw = Quat::from_rotation_y(-def.props.hero[a.slot].yaw_deg.to_radians());
            assert!(((t.rotation * Vec3::Z).xz() - (yaw * Vec3::Z).xz()).length() < 0.06);
        }
        let start: Vec<f32> = (0..h.animals.len())
            .map(|i| h.room(i, h.animals[i].pos, h.animals[i].dir, None).0)
            .collect();
        let mut d = h.clone();
        let mut counts = vec![[0u32; 4]; d.animals.len()];
        for tick in 0..24_000 {
            d.tick(None);
            for (i, a) in d.animals.iter().enumerate() {
                let room = d.room(i, a.pos, a.dir, None).0;
                assert!(
                    room >= start[i].min(0.0) - 1e-5,
                    "{} at {tick}: {room}",
                    a.piece
                );
                if tick >= 12_000 {
                    assert!(room >= -1e-5, "{} still short of room at {tick}", a.piece);
                }
                let k = match a.state() {
                    "walk" if a.blocked > 0 => 3,
                    "walk" => 0,
                    "idle" => 1,
                    _ => 2,
                };
                counts[i][k] += 1;
            }
        }
        for (i, a) in d.animals.iter().enumerate() {
            println!("{} walk/idle/graze/blocked ticks {:?}", a.piece, counts[i]);
        }
    }

    /// 20 simulated minutes per rotation: each animal's collider stays inside the paddock (on a
    /// world tile at every rotation) and off every other animal and rock, its hooves stay on the
    /// drawn ground within 1 cm, and the herd moves.
    #[test]
    fn animals_stay_inside_apart_and_on_the_ground_over_a_long_run() {
        let def = farm();
        let pen = paddock(&def).unwrap();
        let hulls: BTreeMap<&str, Vec<Vec2>> = BODIES
            .iter()
            .map(|(p, _)| {
                let pts: Vec<Vec2> = walk_tests::mesh_tris(p, "collider")
                    .unwrap()
                    .iter()
                    .flat_map(|(t, _)| t.map(|q| Vec2::new(q[0], q[2])))
                    .collect();
                (*p, hull(&pts))
            })
            .collect();
        for rotation in 0..6u8 {
            let cell = Axial::new(1, -1);
            let g = walk_tests::world(&[(cell.q, cell.r, "farm", rotation)]);
            let root = root(cell, rotation);
            let place = |p: Vec2| to_world(cell, rotation, p);
            let on = |p: Vec2| g.ground(place(p)).unwrap();
            let pen_w: Vec<Vec2> = pen.iter().map(|p| place(*p)).collect();
            let normals: Vec<(Vec2, Vec2)> = (0..pen_w.len())
                .map(|i| {
                    let (a, b) = (pen_w[i], pen_w[(i + 1) % pen_w.len()]);
                    (a, (b - a).perp().normalize())
                })
                .collect();
            let centre = pen_w.iter().copied().sum::<Vec2>() / pen_w.len() as f32;
            let normals: Vec<(Vec2, Vec2)> = normals
                .into_iter()
                .map(|(a, n)| (a, if (centre - a).dot(n) > 0.0 { n } else { -n }))
                .collect();
            let rocks_w: Vec<(Vec2, f32)> = rocks(&def, cell)
                .into_iter()
                .map(|(c, r)| (place(c), r))
                .collect();
            let mut h = Herd::new(&def, cell, 1, &rocks(&def, cell)).unwrap();
            let start: Vec<Vec2> = h.animals.iter().map(|a| a.pos).collect();
            let mut walked = vec![0.0f32; h.animals.len()];
            let mut worst_foot = 0.0f32;
            for tick in 0..24_000 {
                let before: Vec<Vec2> = h.animals.iter().map(|a| a.pos).collect();
                h.tick(None);
                for (i, a) in h.animals.iter().enumerate() {
                    walked[i] += a.pos.distance(before[i]);
                }
                if tick % 10 != 0 {
                    continue;
                }
                let placed: Vec<(Transform, Vec<Vec2>)> = (0..h.animals.len())
                    .map(|i| {
                        let t = root * h.pose(i, 1.0, &on);
                        let poly = hulls[h.animals[i].piece]
                            .iter()
                            .map(|p| t.transform_point(Vec3::new(p.x, 0.0, p.y)).xz())
                            .collect();
                        (t, poly)
                    })
                    .collect();
                for (i, (t, poly)) in placed.iter().enumerate() {
                    let piece = h.animals[i].piece;
                    for p in poly {
                        for (a, n) in &normals {
                            let d = (*p - *a).dot(*n);
                            assert!(d > 0.3, "r{rotation} t{tick}: {piece} {d} m from the fence");
                        }
                    }
                    for (c, r) in &rocks_w {
                        let o = Obstacle {
                            piece: piece.into(),
                            poly: poly.clone(),
                        };
                        assert!(!o.hits(*c, *r), "r{rotation} t{tick}: {piece} on a rock");
                    }
                    for (j, (_, other)) in placed.iter().enumerate().skip(i + 1) {
                        assert!(
                            !overlap(poly, other),
                            "r{rotation} t{tick}: {piece} overlaps {}",
                            h.animals[j].piece
                        );
                    }
                    if tick % 50 == 0 {
                        for hoof in h.animals[i].body.hooves {
                            let w = t.transform_point(Vec3::new(hoof[0], 0.0, hoof[1]));
                            let ground = g.ground(w.xz()).unwrap();
                            worst_foot = worst_foot.max((w.y - ground).abs());
                        }
                    }
                }
            }
            println!("r{rotation}: worst hoof {worst_foot} m off the drawn ground");
            assert!(
                worst_foot < 0.01,
                "r{rotation}: a hoof {worst_foot} m off the ground"
            );
            for (i, a) in h.animals.iter().enumerate() {
                assert!(
                    walked[i] > 20.0,
                    "r{rotation}: {} walked {}",
                    a.piece,
                    walked[i]
                );
                assert!(a.pos.distance(start[i]) > 0.0, "{}", a.piece);
            }
        }
    }

    /// The same seed draws the same wander; another seed another.
    #[test]
    fn the_wander_is_deterministic_per_seed() {
        let def = farm();
        let rocks = rocks(&def, Axial::ORIGIN);
        let run = |seed: u64| {
            let mut h = Herd::new(&def, Axial::ORIGIN, seed, &rocks).unwrap();
            for _ in 0..6000 {
                h.tick(None);
            }
            h
        };
        assert_eq!(run(7), run(7));
        assert_ne!(run(7).animals, run(8).animals);
        // A herd rebuilt from the view starts at the slots again, whatever ran before.
        assert_eq!(
            Herd::new(&def, Axial::ORIGIN, 7, &rocks),
            Herd::new(&def, Axial::ORIGIN, 7, &rocks)
        );
    }

    /// An animal never closes on a standing walker: within [`WALKER_CLEAR`] of it, it holds its
    /// pose (waits), and no move brings it inside.
    #[test]
    fn animals_wait_for_the_walker_and_never_walk_into_it() {
        let def = farm();
        let rocks = rocks(&def, Axial::ORIGIN);
        let mut waited = 0;
        for walker in [
            Vec2::new(0.0, 8.0),
            Vec2::new(-1.5, 10.0),
            Vec2::new(4.0, 6.0),
            Vec2::new(-6.0, 5.0),
        ] {
            let mut h = Herd::new(&def, Axial::ORIGIN, 3, &rocks).unwrap();
            for _ in 0..12_000 {
                let before: Vec<(Vec2, Vec2, f32)> = (0..h.animals.len())
                    .map(|i| {
                        let a = &h.animals[i];
                        (a.pos, a.dir, h.room(i, a.pos, a.dir, Some(walker)).1)
                    })
                    .collect();
                h.tick(Some(walker));
                for (i, a) in h.animals.iter().enumerate() {
                    let (pos, dir, room) = before[i];
                    let now = h.room(i, a.pos, a.dir, Some(walker)).1;
                    if room < 0.0 {
                        assert_eq!(
                            (a.pos, a.dir),
                            (pos, dir),
                            "{} moved by the walker",
                            a.piece
                        );
                        assert!(a.waiting);
                        waited += 1;
                    } else {
                        assert!(now >= 0.0, "{} walked into the walker", a.piece);
                    }
                }
            }
        }
        assert!(waited > 0, "no animal ever stood by the walker");
    }

    /// Walk mode: a herd animal blocks the walker as `animal` where it is drawn, not at its slot,
    /// and photo mode's click picks it there.
    #[test]
    fn a_wandering_animal_blocks_the_walker_where_it_stands() {
        let def = farm();
        let cell = Axial::ORIGIN;
        for rotation in [0u8, 2, 5] {
            let mut g = walk_tests::world(&[(0, 0, "farm", rotation)]);
            let mut h = Herd::new(&def, cell, 1, &rocks(&def, cell)).unwrap();
            for _ in 0..4000 {
                h.tick(None);
            }
            let posed: Vec<(Axial, usize, Transform)> = (0..h.animals.len())
                .map(|i| (cell, h.animals[i].slot, h.pose(i, 1.0, &relief_ground)))
                .collect();
            g.pose_herd(&posed);
            let cow = h
                .animals
                .iter()
                .position(|a| a.piece == "animal_cow")
                .unwrap();
            let t = root(cell, rotation) * h.pose(cow, 1.0, &relief_ground);
            let at = t.translation.xz();
            let side = (t.rotation * Vec3::X).xz().normalize();
            // Walk at its flank from 3 m out (whichever side is open).
            let s = [side, -side]
                .into_iter()
                .map(|s| {
                    let from = at + s * 3.0;
                    let mut pos = Vec3::new(from.x, g.ground(from).unwrap(), from.y);
                    let mut last = None;
                    for _ in 0..60 {
                        let st = g.step(pos, -s * 0.05);
                        pos = st.pos;
                        last = st.blocked.or(last);
                    }
                    (pos, last)
                })
                .find(|(_, b)| matches!(b, Some(Blocked::Obstacle(p)) if p == "animal_cow"))
                .unwrap_or_else(|| panic!("r{rotation}: the cow did not block"));
            assert_eq!(s.1.as_ref().map(Blocked::reason), Some("animal"));
            assert!(s.0.xz().distance(at) < 1.0);
            // Its slot is open ground now (unless another animal wandered onto it).
            let slot = to_world(
                cell,
                rotation,
                Vec2::from(def.props.hero[h.animals[cow].slot].at),
            );
            if let Err(Blocked::Obstacle(p)) = g.stand(slot) {
                assert_ne!(
                    p, "animal_cow",
                    "r{rotation}: the cow still blocks its slot"
                );
            }
            // A click from above lands on it.
            let ray = Ray3d::new(
                t.translation + Vec3::Y * 20.0 + (t.rotation * Vec3::Z) * 0.2,
                Dir3::NEG_Y,
            );
            assert_eq!(g.pick_prop(ray, 50.0).map(|(_, p)| p), Some("animal_cow"));
        }
    }

    /// Herds are not saved: a saved world's bytes are the same however long its herd ran, and a
    /// loaded world draws the same herd as the one saved.
    #[test]
    fn the_save_is_unchanged_and_a_load_redraws_the_same_herd() {
        let c = TileCatalog::builtin().unwrap();
        let def = farm();
        let mut w = World::new(11);
        let cell = w
            .frontier()
            .into_iter()
            .find(|&cell| !w.legal_rotations(&c, cell, &def).is_empty())
            .unwrap();
        let k = w.legal_rotations(&c, cell, &def)[0];
        w.place(&c, cell, "farm", k).unwrap();
        let save = SaveData {
            world: w.clone(),
            unlocks: BTreeSet::new(),
            camera: hex_core::CameraState {
                focus: [0.0, 0.0, 0.0],
                yaw: 0.5,
                pitch: 0.9,
                distance: 120.0,
            },
            settings: hex_core::GameSettings {
                quality: hex_core::QualitySetting::High,
                tilt_shift: false,
                tilt_shift_chosen: false,
                tilt_shift_strength: 0.5,
                volumes: hex_core::AudioVolumes {
                    master: 1.0,
                    music: 0.5,
                    sfx: 0.5,
                    ambience: 0.5,
                    ui: 0.5,
                },
            },
        };
        let bytes = save.encode();
        let rocks = rocks(&def, cell);
        let mut h = Herd::new(&def, cell, w.seed(), &rocks).unwrap();
        for _ in 0..2000 {
            h.tick(None);
        }
        assert_eq!(save.encode(), bytes);
        let loaded = SaveData::decode(&bytes, &c).unwrap();
        assert_eq!(loaded, save);
        assert_eq!(
            Herd::new(&def, cell, loaded.world.seed(), &rocks),
            Herd::new(&def, cell, w.seed(), &rocks)
        );
    }
}
