//! The CPU half of the play boids (PRD §7.2): the bird and fish budget, bird roosts read from tile
//! data, the fish's water regions and the bed field that confines them, and the roost wave
//! schedule. Everything here is a pure function of the placed tiles, the world seed and the boid
//! clock, so it is tested without a GPU. `boids.rs` uploads what this decides and the compute pass
//! (`assets/shaders/boids_compute.wgsl`) carries it out. Nothing is read back from the GPU: a bird
//! sent to a roost is pulled onto it by [`APPROACH`] seconds, so the roosting count published here
//! is the one on screen.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use bevy::math::{Quat, Vec2, Vec3, Vec3Swizzles, Vec4};
use hex_core::{AgentKind, Axial, HEX_RADIUS, SocketKind, TileCatalog, TileDef, cell_seed};

use crate::{
    kit_scene::piece_rotation,
    play::PlayTile,
    post::QualityTier,
    water::{
        WaterField,
        tiles::{self, CellWater, WATER_Y},
    },
};

/// Boid caps for one quality tier: one budget shared by birds and fish (PRD §7.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoidBudget {
    /// Boid slots, birds plus fish: the GPU buffer and the flock mesh are sized to it.
    pub total: u32,
    /// Fish once the world holds enough water.
    pub fish_max: u32,
    /// Fish each water tile adds until `fish_max`.
    pub fish_per_water_tile: u32,
}

impl BoidBudget {
    /// The reference target at High: 1,000 boids, by default 700 birds and 300 fish.
    pub const HIGH: BoidBudget = BoidBudget {
        total: 1_000,
        fish_max: 300,
        fish_per_water_tile: 12,
    };

    /// The tier hook. PRD §7.2 sets only the High caps so far, so every tier gets them until the
    /// per-tier caps are decided.
    pub fn for_tier(tier: QualityTier) -> BoidBudget {
        match tier {
            QualityTier::Low | QualityTier::Medium | QualityTier::High | QualityTier::Ultra => {
                Self::HIGH
            }
        }
    }

    /// (birds, fish) for a world with `water_tiles` water tiles. Fish scale with the water up to
    /// `fish_max`, and the fish budget the water cannot use goes to the birds.
    pub fn split(&self, water_tiles: usize) -> (u32, u32) {
        let fish = (u64::from(self.fish_per_water_tile) * water_tiles as u64)
            .min(u64::from(self.fish_max.min(self.total))) as u32;
        (self.total - fish, fish)
    }
}

/// `total` shared out in proportion to `weights` by largest remainder (ties to the lower index).
pub fn allocate(total: u32, weights: &[u32]) -> Vec<u32> {
    let sum: u64 = weights.iter().map(|&w| u64::from(w)).sum();
    if sum == 0 {
        return vec![0; weights.len()];
    }
    let exact: Vec<(u32, u64)> = weights
        .iter()
        .map(|&w| {
            let x = u64::from(total) * u64::from(w);
            ((x / sum) as u32, x % sum)
        })
        .collect();
    let mut out: Vec<u32> = exact.iter().map(|e| e.0).collect();
    let mut left = total - out.iter().sum::<u32>();
    let mut order: Vec<usize> = (0..weights.len()).collect();
    order.sort_by_key(|&i| (std::cmp::Reverse(exact[i].1), i));
    for i in order {
        if left == 0 {
            break;
        }
        if weights[i] > 0 {
            out[i] += 1;
            left -= 1;
        }
    }
    out
}

/// SplitMix64: the deterministic stream behind spawn points and launch headings.
#[derive(Clone, Copy, Debug)]
pub struct Rng(pub u64);

impl Rng {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The cell containing world XZ `p`: the inverse of [`Axial::to_world`] with cube rounding.
pub fn cell_at(p: Vec2) -> Axial {
    let r = p.y / (1.5 * HEX_RADIUS);
    let q = p.x / (HEX_RADIUS * 3f32.sqrt()) - r / 2.0;
    let s = -q - r;
    let (mut rq, mut rr, rs) = (q.round(), r.round(), s.round());
    let (dq, dr, ds) = ((rq - q).abs(), (rr - r).abs(), (rs - s).abs());
    if dq > dr && dq > ds {
        rq = -rr - rs;
    } else if dr > ds {
        rr = -rq - rs;
    }
    Axial::new(rq as i32, rr as i32)
}

// ---------------------------------------------------------------------------------------------
// Roosts

/// Canopy dome clumps of `tree_hero_oak`: `_tree.layout()` in scripts/blender/kit/_tree.py, a
/// seeded `random.Random("tree_hero_oak:layout")` that plain python3 evaluates the same way.
/// Blender frame (Z up) centre and radius, with the tier (0 lowest).
const OAK_CLUMPS: [([f32; 3], f32, u8); 11] = [
    ([-2.525, -3.083, 4.843], 2.076, 0),
    ([2.687, -2.941, 4.738], 1.962, 0),
    ([4.206, 0.951, 5.536], 2.133, 0),
    ([0.030, 4.006, 5.196], 2.019, 0),
    ([-3.823, 0.565, 4.733], 1.859, 0),
    ([1.007, 2.027, 6.792], 1.920, 1),
    ([-1.779, 0.875, 6.833], 1.938, 1),
    ([-1.224, -1.619, 6.954], 2.082, 1),
    ([1.989, -0.877, 7.325], 2.193, 1),
    ([-0.722, 0.419, 8.824], 1.908, 2),
    ([0.516, -0.574, 9.324], 1.949, 2),
];
/// `_tree.DOME_WIDE` and `_tree.DOME_TOP`: the master dome's horizontal and upper vertical scale.
const DOME_WIDE: f32 = 1.15;
const DOME_TOP: f32 = 0.9;
/// `tree_forest`'s spire top and its crown radius at 10 m (`_forest.HEIGHT`, `crown_r(10.0)`).
const SPRUCE_TOP: f32 = 16.0;
const SPRUCE_R10: f32 = 1.8;

/// A point on an oak clump's dome at `azimuth` and `elevation` (radians, Blender frame), returned
/// in the runtime frame (glTF Y up: Blender (x, y, z) is (x, z, -y)).
fn dome_point(c: [f32; 3], r: f32, azimuth: f32, elevation: f32) -> Vec3 {
    let (sa, ca) = azimuth.sin_cos();
    let (se, ce) = elevation.sin_cos();
    let h = ce * DOME_WIDE * r;
    Vec3::new(c[0] + ca * h, c[2] + se * DOME_TOP * r, -(c[1] + sa * h))
}

/// The oak's perches: (clump, elevation in degrees, azimuth offset in radians from the clump's
/// outward direction). Chosen against the GLB's canopy shell (`oak_perches_sit_on_the_canopy`):
/// each lies on a dome no higher dome covers. Twelve, the forest's bird `max`.
const OAK_PERCHES: [(usize, f32, f32); 12] = [
    (0, 45.0, 0.0),
    (1, 45.0, 0.0),
    (2, 45.0, -0.6),
    (3, 45.0, -0.6),
    (4, 45.0, -0.6),
    (5, 65.0, 0.0),
    (6, 35.0, 0.0),
    (7, 45.0, 0.6),
    (8, 45.0, 0.0),
    (9, 65.0, 1.2),
    (9, 65.0, -0.6),
    (10, 90.0, 0.0),
];

/// Perch points on a tree piece's canopy, piece-local in the runtime frame (Y up, trunk base at
/// the origin). Empty for a piece birds do not roost on.
pub fn canopy_perches(piece: &str) -> Vec<Vec3> {
    match piece {
        "tree_hero_oak" => OAK_PERCHES
            .iter()
            .map(|&(k, el, daz)| {
                let (c, r, _) = OAK_CLUMPS[k];
                dome_point(c, r, c[1].atan2(c[0]) + daz, el.to_radians())
            })
            .collect(),
        "tree_forest" => {
            let mut v = vec![Vec3::new(0.0, SPRUCE_TOP, 0.0)];
            v.extend((0..3).map(|k| {
                let a = (120.0 * k as f32 + 30.0).to_radians();
                Vec3::new(a.cos() * SPRUCE_R10, 10.0, a.sin() * SPRUCE_R10)
            }));
            v
        }
        _ => vec![],
    }
}

/// One tree on a tile, tile-local (before the tile's rotation).
#[derive(Clone, Debug, PartialEq)]
pub struct TreeInstance {
    pub piece: String,
    pub at: Vec3,
    pub yaw: Quat,
}

/// The trees birds can roost in on a tile: its hero props. Play does not place scatter props yet
/// (the forest's `tree_forest` spruces); once it does, their instances join this list and their
/// canopies become roosts through [`canopy_perches`].
pub fn tree_instances(def: &TileDef) -> Vec<TreeInstance> {
    def.props
        .hero
        .iter()
        .map(|h| TreeInstance {
            piece: h.piece.as_str().to_owned(),
            at: Vec3::new(h.at[0], 0.0, h.at[1]),
            yaw: Quat::from_rotation_y(-h.yaw_deg.to_radians()),
        })
        .collect()
}

/// A perch birds settle on: slot `slot` of the tile at `cell`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Roost {
    pub cell: Axial,
    pub slot: u8,
    pub pos: Vec3,
}

/// Tree pieces with canopy perches: what [`BoidPlan::set_world`] watches arrive in the kit.
pub const PERCH_PIECES: [&str; 2] = ["tree_hero_oak", "tree_forest"];

/// Bird roosts of the placed tiles: the canopy perches of each tile with a `Bird` agent rule,
/// capped at the rule's `max` (the forest's 12), in world space. Only trees `drawn` reports as
/// kit pieces count, so a forest still on its placeholder (its pack streaming) has no roosts
/// until the oak swaps in. Sorted by cell, then slot.
pub fn roosts(
    tiles: &[PlayTile],
    catalog: &TileCatalog,
    drawn: &dyn Fn(&str) -> bool,
) -> Vec<Roost> {
    let mut sorted: Vec<&PlayTile> = tiles.iter().collect();
    sorted.sort_by_key(|t| (t.cell.q, t.cell.r));
    let mut out = vec![];
    for t in sorted {
        let Some(def) = catalog.get(&t.tile) else {
            continue;
        };
        let max: usize = def
            .agents
            .iter()
            .filter(|a| a.agent == AgentKind::Bird)
            .map(|a| usize::from(a.max))
            .sum();
        let [x, z] = t.cell.to_world(HEX_RADIUS);
        let (centre, rot) = (Vec3::new(x, 0.0, z), piece_rotation(t.rotation));
        let trees = tree_instances(def).into_iter().filter(|t| drawn(&t.piece));
        let perches = trees.flat_map(|tree| {
            canopy_perches(&tree.piece)
                .into_iter()
                .map(move |p| tree.at + tree.yaw * p)
        });
        for (slot, local) in perches.take(max.min(usize::from(u8::MAX))).enumerate() {
            out.push(Roost {
                cell: t.cell,
                slot: slot as u8,
                pos: centre + rot * local,
            });
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Fish

/// The confinement field is baked in square chunks of [`CHUNK`] metres, [`SIDE`] samples a side
/// (both borders included, so a lookup never needs a neighbouring chunk), [`STEP`] apart.
pub const CHUNK: f32 = 16.0;
pub const SIDE: usize = 17;
pub const STEP: f32 = CHUNK / (SIDE - 1) as f32;
/// Half the height of the placeholder fish (m): the band below bounds the fish's centre.
pub const FISH_HALF_H: f32 = 0.15;
/// Gap kept above the bed (m): the kit beds' 0.10 m floor relief (ocean stones) and the band's
/// bilinear error on the steepest banks (about 0.12 m at 1 m spacing), plus half a fish.
pub const BED_CLEAR: f32 = 0.30 + FISH_HALF_H;
/// Gap kept under the surface at rest (m) where the water is calm, plus half a fish.
pub const SURF_CLEAR: f32 = 0.10 + FISH_HALF_H;
/// Summed amplitude of the High tier's 8 Gerstner waves (m): `water_common.wesl`'s
/// 0.015 m per metre of wavelength from 7.5 m down by 0.74 per wave. Waves reach full height on
/// water deeper than 2.5 m and none in rivers (`wave_atten`), and the band follows that.
pub const WAVE_BOUND: f32 = 0.40;
/// Distance kept from an exposed water edge (m), where the cut face is drawn.
pub const WALL_CLEAR: f32 = 2.0;
/// Room a spawn point needs (m).
pub const SPAWN_ROOM: f32 = 0.3;
/// A sample outside every water tile: an empty band, no room.
pub const LAND: [f32; 4] = [0.0, -1.0, -4.0, 0.0];

/// Distance from `local` (metres from the cell centre) to the segment of world edge `e`.
fn edge_distance(local: Vec2, e: u8) -> f32 {
    let corner = |deg: f32| {
        let a = deg.to_radians();
        Vec2::new(a.cos(), a.sin()) * HEX_RADIUS
    };
    let mid = 60.0 * f32::from(e % 6);
    let (a, b) = (corner(mid - 30.0), corner(mid + 30.0));
    let ab = b - a;
    let t = ((local - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    local.distance(a + ab * t)
}

/// The fish band at world XZ `p` from the water module's analytic bed (`water::tiles`, the bed
/// contract in docs/kit/water-contract.md): `[lowest centre y, highest centre y, room, 0]`. Room
/// is the smaller of the band's height and the distance past [`WALL_CLEAR`] to the nearest
/// exposed edge (a cut face); below 0 a fish may not be there. [`LAND`] outside water tiles.
pub fn band_at(cells: &BTreeMap<Axial, CellWater>, p: Vec2) -> [f32; 4] {
    let cell = cell_at(p);
    let Some(cw) = cells.get(&cell) else {
        return LAND;
    };
    let field = cw.field();
    let bed = field.height(p.x, p.y);
    let s = field.water(p.x, p.y);
    let atten = smoothstep(0.05, 2.5, s.depth) * (1.0 - s.river);
    let floor = bed + BED_CLEAR;
    let ceil = WATER_Y - SURF_CLEAR - WAVE_BOUND * atten;
    let local = p - Vec2::from(cell.to_world(HEX_RADIUS));
    let wall = cw
        .exposed
        .iter()
        .map(|&e| edge_distance(local, e))
        .fold(f32::MAX, f32::min);
    [floor, ceil, (ceil - floor).min(wall - WALL_CLEAR), 0.0]
}

/// The baked fish band ([`band_at`] every [`STEP`] metres) over the chunks that touch a water
/// tile, as the compute pass reads it: a chunk table over the chunks' bounding box (-1: no
/// chunk, land) and [`SIDE`]² samples per chunk, rows along +z.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FishField {
    pub x0: i32,
    pub z0: i32,
    pub w: u32,
    pub h: u32,
    pub chunks: Vec<i32>,
    pub samples: Vec<[f32; 4]>,
}

impl FishField {
    pub fn bake(cells: &BTreeMap<Axial, CellWater>) -> FishField {
        let chunk = |v: f32| (v / CHUNK).floor() as i32;
        let mut keys: BTreeSet<(i32, i32)> = BTreeSet::new();
        for c in cells.keys() {
            let [x, z] = c.to_world(HEX_RADIUS);
            for cz in chunk(z - HEX_RADIUS)..=chunk(z + HEX_RADIUS) {
                for cx in chunk(x - HEX_RADIUS)..=chunk(x + HEX_RADIUS) {
                    keys.insert((cx, cz));
                }
            }
        }
        let Some(&(x0, _)) = keys.iter().min_by_key(|k| k.0) else {
            return FishField::default();
        };
        let z0 = keys.iter().map(|k| k.1).min().unwrap_or(0);
        let x1 = keys.iter().map(|k| k.0).max().unwrap_or(0);
        let z1 = keys.iter().map(|k| k.1).max().unwrap_or(0);
        let (w, h) = ((x1 - x0 + 1) as u32, (z1 - z0 + 1) as u32);
        let mut out = FishField {
            x0,
            z0,
            w,
            h,
            chunks: vec![-1; (w * h) as usize],
            samples: vec![],
        };
        for (cx, cz) in keys {
            let origin = Vec2::new(cx as f32, cz as f32) * CHUNK;
            let block: Vec<[f32; 4]> = (0..SIDE * SIDE)
                .map(|n| {
                    let o = Vec2::new((n % SIDE) as f32, (n / SIDE) as f32) * STEP;
                    band_at(cells, origin + o)
                })
                .collect();
            if block.iter().all(|s| *s == LAND) {
                continue;
            }
            let k = (out.samples.len() / (SIDE * SIDE)) as i32;
            out.chunks[((cz - z0) as u32 * w + (cx - x0) as u32) as usize] = k;
            out.samples.extend(block);
        }
        out
    }

    /// The bilinear band at `p`; `band` in boids_compute.wgsl does the same arithmetic.
    pub fn sample(&self, p: Vec2) -> [f32; 4] {
        let c = (p / CHUNK).floor();
        let (cx, cz) = (c.x as i32 - self.x0, c.y as i32 - self.z0);
        if cx < 0 || cz < 0 || cx >= self.w as i32 || cz >= self.h as i32 {
            return LAND;
        }
        let k = self.chunks[(cz as u32 * self.w + cx as u32) as usize];
        if k < 0 {
            return LAND;
        }
        let g = ((p - c * CHUNK) / STEP).clamp(Vec2::ZERO, Vec2::splat((SIDE - 1) as f32 - 0.001));
        let i = g.floor();
        let f = g - i;
        let base = k as usize * SIDE * SIDE + i.y as usize * SIDE + i.x as usize;
        let s = |o: usize| Vec4::from(self.samples[base + o]);
        let v = s(0)
            .lerp(s(1), f.x)
            .lerp(s(SIDE).lerp(s(SIDE + 1), f.x), f.y);
        v.to_array()
    }

    /// Gradient of the room at `p` (central differences 0.5 m apart); `room_grad` in the shader.
    pub fn grad(&self, p: Vec2) -> Vec2 {
        let r = |d: Vec2| self.sample(p + d)[2];
        let h = 0.5;
        Vec2::new(
            r(Vec2::new(h, 0.0)) - r(Vec2::new(-h, 0.0)),
            r(Vec2::new(0.0, h)) - r(Vec2::new(0.0, -h)),
        ) / (2.0 * h)
    }
}

/// One fish step's confinement, mirrored by `confine` in boids_compute.wgsl. It integrates; if
/// the new XZ has no room it stays put and turns up the room gradient (back into open water);
/// then it clamps y into the band.
pub fn confine(field: &FishField, p: Vec3, v: Vec3, dt: f32) -> (Vec3, Vec3) {
    let mut np = p + v * dt;
    let mut nv = v;
    let mut b = field.sample(np.xz());
    if b[2] < 0.0 {
        let g = field.grad(p.xz());
        let away = if g.length_squared() > 1e-8 {
            g.normalize()
        } else {
            -v.xz().normalize_or_zero()
        };
        let speed = v.xz().length();
        nv = Vec3::new(away.x * speed, v.y, away.y * speed);
        np = Vec3::new(p.x, np.y, p.z);
        b = field.sample(p.xz());
    }
    np.y = np.y.clamp(b[0], b[1].max(b[0]));
    (np, nv)
}

/// Water tiles joined by continuous water: the fish of one region can swim to every point of it.
#[derive(Clone, Debug, PartialEq)]
pub struct FishRegion {
    /// Sorted; the first names the region.
    pub cells: Vec<Axial>,
    pub fish: u32,
    /// The sample with the most room: where a fish goes when no spawn point is found.
    pub best: Vec3,
    /// Room at `best`; regions with less than [`SPAWN_ROOM`] get no fish.
    pub room: f32,
}

/// The fish's water: the water module's layout of the placed tiles, its regions and the field.
#[derive(Clone, Debug, Default)]
pub struct FishWorld {
    pub cells: BTreeMap<Axial, CellWater>,
    pub regions: Vec<FishRegion>,
    pub field: FishField,
}

fn wet(def: &TileDef, rotation: u8, world: u8) -> bool {
    let s = def.edge(world, rotation);
    matches!(s.kind, SocketKind::Ocean | SocketKind::Beach)
        || (s.kind == SocketKind::River && s.lane.is_some())
}

impl FishWorld {
    pub fn new(tiles: &[PlayTile], catalog: &TileCatalog) -> FishWorld {
        let cells = tiles::layout(tiles, catalog, |_| true);
        let field = FishField::bake(&cells);
        let mut seen: BTreeSet<Axial> = BTreeSet::new();
        let mut regions = vec![];
        for &start in cells.keys() {
            if !seen.insert(start) {
                continue;
            }
            let mut group = vec![start];
            let mut queue = VecDeque::from([start]);
            while let Some(c) = queue.pop_front() {
                let cw = &cells[&c];
                let Some(def) = catalog.get(&cw.tile.tile) else {
                    continue;
                };
                for e in 0..6u8 {
                    let n = c.neighbor(e);
                    if wet(def, cw.tile.rotation, e)
                        && !cw.exposed.contains(&e)
                        && cells.contains_key(&n)
                        && seen.insert(n)
                    {
                        group.push(n);
                        queue.push_back(n);
                    }
                }
            }
            group.sort_by_key(|c| (c.q, c.r));
            regions.push(group);
        }
        regions.sort_by_key(|g| (g[0].q, g[0].r));
        let regions = regions
            .into_iter()
            .map(|cells_in| {
                let (best, room) = best_point(&field, &cells_in);
                FishRegion {
                    cells: cells_in,
                    fish: 0,
                    best,
                    room,
                }
            })
            .collect();
        FishWorld {
            cells,
            regions,
            field,
        }
    }

    /// Shares `fish` between the regions by water tile count; regions without room get none.
    pub fn allocate(&mut self, fish: u32) {
        let weights: Vec<u32> = self
            .regions
            .iter()
            .map(|r| {
                if r.room >= SPAWN_ROOM {
                    r.cells.len() as u32
                } else {
                    0
                }
            })
            .collect();
        for (r, n) in self.regions.iter_mut().zip(allocate(fish, &weights)) {
            r.fish = n;
        }
    }

    /// A spawn point in region `region` for boid slot `slot`: a seeded point of its water where
    /// both the analytic band and the baked one leave room, at the middle of their overlap.
    pub fn spawn(&self, region: usize, seed: u64, slot: u32) -> Vec3 {
        let r = &self.regions[region];
        let mut rng =
            Rng(cell_seed(seed, r.cells[0]) ^ u64::from(slot).wrapping_mul(0xA24B_AED4_963E_E407));
        for _ in 0..64 {
            let cell = r.cells[(rng.next_u64() % r.cells.len() as u64) as usize];
            let c = Vec2::from(cell.to_world(HEX_RADIUS));
            let p = c + Vec2::new(rng.unit() * 2.0 - 1.0, rng.unit() * 2.0 - 1.0) * HEX_RADIUS;
            if cell_at(p) != cell {
                continue;
            }
            let (a, g) = (band_at(&self.cells, p), self.field.sample(p));
            let (lo, hi) = (a[0].max(g[0]), a[1].min(g[1]));
            if a[2] >= SPAWN_ROOM && g[2] >= SPAWN_ROOM && hi >= lo {
                return Vec3::new(p.x, 0.5 * (lo + hi), p.y);
            }
        }
        r.best
    }
}

/// The field sample of `cells` with the most room, at the middle of its band.
fn best_point(field: &FishField, cells: &[Axial]) -> (Vec3, f32) {
    let mut best = (Vec3::new(0.0, WATER_Y - 1.0, 0.0), f32::MIN);
    for cz in 0..field.h as i32 {
        for cx in 0..field.w as i32 {
            let k = field.chunks[(cz as u32 * field.w + cx as u32) as usize];
            if k < 0 {
                continue;
            }
            let origin = Vec2::new((cx + field.x0) as f32, (cz + field.z0) as f32) * CHUNK;
            for n in 0..SIDE * SIDE {
                let s = field.samples[k as usize * SIDE * SIDE + n];
                let p = origin + Vec2::new((n % SIDE) as f32, (n / SIDE) as f32) * STEP;
                if s[2] > best.1
                    && cells
                        .binary_search_by_key(&(cell_at(p).q, cell_at(p).r), |c| (c.q, c.r))
                        .is_ok()
                {
                    best = (Vec3::new(p.x, 0.5 * (s[0] + s[1]), p.y), s[2]);
                }
            }
        }
    }
    best
}

// ---------------------------------------------------------------------------------------------
// The plan: slots, roost waves, counts

/// Seconds a bird takes from leaving the flock to sitting on its perch; it is pulled onto the
/// perch by then, so a bird is roosting from `assigned_at + APPROACH`.
pub const APPROACH: f32 = 8.0;
/// A roost group's cycle (s): its perches are taken for the first [`OCCUPIED`] seconds (the
/// approach, then sitting), then every bird on it launches together and the perches stay empty
/// until the cycle repeats. The cycle starts when the forest appears.
pub const OCCUPIED: f32 = 22.0;
pub const CYCLE: f32 = 36.0;
/// The play flock's altitude band (m) and its margin past the world's edge.
pub const SKY_LOW: f32 = 22.0;
pub const SKY_HIGH: f32 = 44.0;
pub const SKY_MARGIN: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    Bird,
    Fish,
}

/// One boid slot as the CPU sees it. A changed `generation` makes the compute pass reset the
/// boid to `spawn`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    pub kind: SlotKind,
    pub generation: u32,
    pub spawn: Vec3,
    /// The roost (cell, slot) a bird is sent to.
    pub roost: Option<(Axial, u8)>,
    pub assigned_at: f32,
    /// The fish's region index.
    pub region: Option<usize>,
}

/// What the published state reports.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoidCounts {
    pub budget: u32,
    pub birds: u32,
    pub fish: u32,
    pub roosts: u32,
    pub roosting: u32,
    pub settling: u32,
    pub water_tiles: u32,
    /// (first cell, water tiles, fish) per region.
    pub regions: Vec<(Axial, u32, u32)>,
}

/// The CPU state of the boids. `play` worlds get roosts, fish and the play sky; the other
/// scenarios keep the P0 stress flock (every slot a bird over a 800 m square, no roosts).
#[derive(Clone, Debug)]
pub struct BoidPlan {
    pub budget: BoidBudget,
    pub seed: u64,
    pub play: bool,
    pub slots: Vec<Slot>,
    pub birds: u32,
    pub fish: u32,
    pub roosts: Vec<Roost>,
    /// Roost groups by cell: (cycle start, perches taken).
    groups: BTreeMap<Axial, (f32, bool)>,
    pub water: FishWorld,
    /// Flock centre (xyz) and horizontal radius (w).
    pub sky: Vec4,
    cursor: usize,
    tiles: Option<Vec<PlayTile>>,
    /// Which of [`PERCH_PIECES`] the kit draws, as of the last `set_world`.
    drawn: Vec<bool>,
    /// Slot data changed (the ctl buffer).
    pub slots_dirty: bool,
    /// Roosts or the fish field changed.
    pub world_dirty: bool,
}

impl BoidPlan {
    fn empty(budget: BoidBudget, seed: u64, play: bool) -> BoidPlan {
        BoidPlan {
            budget,
            seed,
            play,
            slots: vec![],
            birds: budget.total,
            fish: 0,
            roosts: vec![],
            groups: BTreeMap::new(),
            water: FishWorld::default(),
            sky: Vec4::new(0.0, 80.0, 0.0, 400.0),
            cursor: 0,
            tiles: None,
            drawn: vec![],
            slots_dirty: true,
            world_dirty: true,
        }
    }

    /// The P0 stress flock: every slot a bird, spawned by [`super::initial_boids`].
    pub fn stress(budget: BoidBudget, seed: u64) -> BoidPlan {
        let mut plan = Self::empty(budget, seed, false);
        plan.slots = super::initial_boids(budget.total, seed as u32)
            .into_iter()
            .map(|b| Slot {
                kind: SlotKind::Bird,
                generation: 1,
                spawn: Vec3::new(b.pos[0], b.pos[1], b.pos[2]),
                roost: None,
                assigned_at: 0.0,
                region: None,
            })
            .collect();
        plan
    }

    /// A play plan with no world yet: every slot a bird in the sky over the origin.
    pub fn play(budget: BoidBudget, seed: u64) -> BoidPlan {
        let mut plan = Self::empty(budget, seed, true);
        plan.sky = Vec4::new(
            0.0,
            0.5 * (SKY_LOW + SKY_HIGH),
            0.0,
            HEX_RADIUS + SKY_MARGIN,
        );
        plan.slots = (0..budget.total)
            .map(|i| Slot {
                kind: SlotKind::Bird,
                generation: 1,
                spawn: plan.sky_point(i),
                roost: None,
                assigned_at: 0.0,
                region: None,
            })
            .collect();
        plan
    }

    fn sky_point(&self, slot: u32) -> Vec3 {
        let mut rng = Rng(self.seed ^ u64::from(slot).wrapping_mul(0xD1B5_4A32_D192_ED03));
        let a = rng.unit() * std::f32::consts::TAU;
        let d = rng.unit().sqrt() * self.sky.w;
        let y = SKY_LOW + rng.unit() * (SKY_HIGH - SKY_LOW);
        Vec3::new(self.sky.x + a.cos() * d, y, self.sky.z + a.sin() * d)
    }

    /// Rebuilds roosts, fish regions, the field and the bird/fish split from the placed tiles
    /// (in play, those past their rise's agents stage) and the tree pieces the kit draws (`drawn`;
    /// a forest swapping from its placeholder to the kit gains its roosts). Returns false when
    /// nothing changed.
    pub fn set_world(
        &mut self,
        tiles: &[PlayTile],
        catalog: &TileCatalog,
        drawn: &dyn Fn(&str) -> bool,
        t: f32,
    ) -> bool {
        let now: Vec<bool> = PERCH_PIECES.iter().map(|p| drawn(p)).collect();
        if !self.play || (self.tiles.as_deref() == Some(tiles) && self.drawn == now) {
            return false;
        }
        // The world appears all at once (entering play draws the start world, or a loaded one, at
        // rest), so the first non-empty tile set is the opening world.
        let first = self.tiles.as_ref().is_none_or(|t| t.is_empty());
        self.tiles = Some(tiles.to_vec());
        self.drawn = now;
        // Sky over the world's bounding box.
        if !tiles.is_empty() {
            let pts: Vec<Vec2> = tiles
                .iter()
                .map(|t| Vec2::from(t.cell.to_world(HEX_RADIUS)))
                .collect();
            let lo = pts.iter().fold(Vec2::splat(f32::MAX), |a, p| a.min(*p));
            let hi = pts.iter().fold(Vec2::splat(f32::MIN), |a, p| a.max(*p));
            let c = 0.5 * (lo + hi);
            self.sky = Vec4::new(
                c.x,
                0.5 * (SKY_LOW + SKY_HIGH),
                c.y,
                0.5 * lo.distance(hi) + HEX_RADIUS + SKY_MARGIN,
            );
        }
        // Water, then the split: birds fill the low slots, fish the high ones.
        self.water = FishWorld::new(tiles, catalog);
        let (_, fish) = self.budget.split(self.water.cells.len());
        self.water.allocate(fish);
        let fish: u32 = self.water.regions.iter().map(|r| r.fish).sum();
        (self.birds, self.fish) = (self.budget.total - fish, fish);
        let mut region_of = vec![];
        for (ri, r) in self.water.regions.iter().enumerate() {
            region_of.extend(std::iter::repeat_n(ri, r.fish as usize));
        }
        for i in 0..self.slots.len() {
            let old = self.slots[i];
            let mut s = old;
            if (i as u32) < self.birds {
                if old.kind != SlotKind::Bird {
                    s = Slot {
                        kind: SlotKind::Bird,
                        generation: old.generation + 1,
                        spawn: self.sky_point(i as u32),
                        roost: None,
                        assigned_at: 0.0,
                        region: None,
                    };
                }
            } else {
                let ri = region_of[i - self.birds as usize];
                let spawn = self.water.spawn(ri, self.seed, i as u32);
                let keep = old.kind == SlotKind::Fish
                    && self.water.regions[ri]
                        .cells
                        .contains(&cell_at(old.spawn.xz()));
                s = Slot {
                    kind: SlotKind::Fish,
                    generation: old.generation + u32::from(!keep),
                    spawn: if keep { old.spawn } else { spawn },
                    roost: None,
                    assigned_at: 0.0,
                    region: Some(ri),
                };
            }
            self.slots[i] = s;
        }
        // Roosts: perches that vanished release their birds; new forests start a cycle now.
        self.roosts = roosts(tiles, catalog, drawn);
        let cells: BTreeSet<Axial> = self.roosts.iter().map(|r| r.cell).collect();
        let keys: BTreeSet<(Axial, u8)> = self.roosts.iter().map(|r| (r.cell, r.slot)).collect();
        for s in &mut self.slots {
            if s.roost.is_some_and(|k| !keys.contains(&k)) {
                s.roost = None;
            }
        }
        self.groups.retain(|c, _| cells.contains(c));
        for c in cells {
            self.groups.entry(c).or_insert((t, false));
        }
        if first {
            // A world that opens with drawn forests opens with their birds already perched.
            let groups: Vec<Axial> = self.groups.keys().copied().collect();
            for c in groups {
                self.assign(c, t - APPROACH, true);
                self.groups.insert(c, (t, true));
            }
        }
        self.slots_dirty = true;
        self.world_dirty = true;
        true
    }

    /// Sends free birds to every perch of the group at `cell`.
    fn assign(&mut self, cell: Axial, at: f32, perched: bool) {
        let perches: Vec<Roost> = self
            .roosts
            .iter()
            .filter(|r| r.cell == cell)
            .copied()
            .collect();
        let n = self.birds as usize;
        if n == 0 {
            return;
        }
        for r in perches {
            if self.slots.iter().any(|s| s.roost == Some((r.cell, r.slot))) {
                continue;
            }
            let Some(k) = (0..n)
                .map(|o| (self.cursor + o) % n)
                .find(|&k| self.slots[k].kind == SlotKind::Bird && self.slots[k].roost.is_none())
            else {
                return;
            };
            self.cursor = (k + 1) % n;
            let s = &mut self.slots[k];
            s.roost = Some((r.cell, r.slot));
            s.assigned_at = at;
            if perched {
                s.generation += 1;
                s.spawn = r.pos;
            }
        }
    }

    /// Advances the roost waves to time `t`: groups entering their occupied phase take birds from
    /// the flock, groups leaving it launch every bird on them.
    pub fn tick(&mut self, t: f32) {
        let groups: Vec<(Axial, f32, bool)> =
            self.groups.iter().map(|(c, &(s, o))| (*c, s, o)).collect();
        for (cell, start, occupied) in groups {
            let now = (t - start).rem_euclid(CYCLE) < OCCUPIED;
            if now == occupied {
                continue;
            }
            if now {
                self.assign(cell, t, false);
            } else {
                for s in &mut self.slots {
                    if s.roost.is_some_and(|(c, _)| c == cell) {
                        s.roost = None;
                    }
                }
            }
            self.groups.insert(cell, (start, now));
            self.slots_dirty = true;
        }
    }

    /// Index of each roost key in [`Self::roosts`].
    pub fn roost_index(&self, key: (Axial, u8)) -> Option<usize> {
        self.roosts.iter().position(|r| (r.cell, r.slot) == key)
    }

    pub fn counts(&self, t: f32) -> BoidCounts {
        let assigned = self
            .slots
            .iter()
            .filter_map(|s| s.roost.map(|_| s.assigned_at));
        let (mut roosting, mut settling) = (0, 0);
        for at in assigned {
            if t - at >= APPROACH {
                roosting += 1;
            } else {
                settling += 1;
            }
        }
        BoidCounts {
            budget: self.budget.total,
            birds: self.birds,
            fish: self.fish,
            roosts: self.roosts.len() as u32,
            roosting,
            settling,
            water_tiles: self.water.cells.len() as u32,
            regions: self
                .water
                .regions
                .iter()
                .map(|r| (r.cells[0], r.cells.len() as u32, r.fish))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    impl BoidPlan {
        /// `set_world` with every tree drawn as a kit piece.
        fn set_world_all(&mut self, tiles: &[PlayTile], c: &TileCatalog, t: f32) -> bool {
            self.set_world(tiles, c, &|_| true, t)
        }
    }

    #[test]
    fn a_forest_gains_its_roosts_when_its_pack_swaps_the_oak_in() {
        let c = catalog();
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 2);
        let world = [tile(0, 0, "meadow", 0), tile(1, 0, "forest", 0)];
        assert!(plan.set_world(&world, &c, &|p| p != "tree_hero_oak", 1.0));
        assert!(plan.roosts.is_empty(), "the oak is still a placeholder");
        assert!(!plan.set_world(&world, &c, &|p| p != "tree_hero_oak", 1.5));
        assert!(
            plan.set_world(&world, &c, &|_| true, 2.0),
            "the pack arrived"
        );
        assert_eq!(plan.roosts.len(), 12);
        plan.tick(2.0);
        assert_eq!(plan.counts(2.0 + APPROACH).roosting, 12);
    }

    #[test]
    fn budget_is_high_on_every_tier_and_fish_scale_with_water() {
        for t in [
            QualityTier::Low,
            QualityTier::Medium,
            QualityTier::High,
            QualityTier::Ultra,
        ] {
            assert_eq!(BoidBudget::for_tier(t), BoidBudget::HIGH);
        }
        let b = BoidBudget::HIGH;
        assert_eq!(
            b.split(0),
            (1_000, 0),
            "no water: the fish budget goes to birds"
        );
        assert_eq!(b.split(1), (988, 12));
        assert_eq!(b.split(10), (880, 120));
        assert_eq!(b.split(25), (700, 300), "the default split");
        assert_eq!(b.split(400), (700, 300), "fish cap at 300");
        for n in 0..60 {
            let (birds, fish) = b.split(n);
            assert_eq!(birds + fish, b.total);
        }
    }

    #[test]
    fn largest_remainder_allocation_sums_and_skips_zero_weights() {
        assert_eq!(allocate(300, &[1, 1, 1]), vec![100, 100, 100]);
        assert_eq!(allocate(10, &[1, 1, 1]), vec![4, 3, 3]);
        assert_eq!(allocate(7, &[0, 3, 1]), vec![0, 5, 2]);
        assert_eq!(allocate(5, &[0, 0]), vec![0, 0]);
        let a = allocate(299, &[7, 13, 2, 1]);
        assert_eq!(a.iter().sum::<u32>(), 299);
    }

    #[test]
    fn cell_at_inverts_to_world() {
        for q in -4..=4 {
            for r in -4..=4 {
                let c = Axial::new(q, r);
                let p = Vec2::from(c.to_world(HEX_RADIUS));
                assert_eq!(cell_at(p), c);
                for e in 0..6u8 {
                    let a = (60.0 * f32::from(e)).to_radians();
                    let near = p + Vec2::new(a.cos(), a.sin()) * (0.95 * HEX_RADIUS * 0.866);
                    assert_eq!(cell_at(near), c, "{c:?} edge {e}");
                }
            }
        }
    }

    #[test]
    fn roosts_come_from_bird_rules_and_hero_canopies() {
        let c = catalog();
        let forest = c.get("forest").unwrap();
        let rule = forest
            .agents
            .iter()
            .find(|a| a.agent == AgentKind::Bird)
            .unwrap();
        assert_eq!(rule.max, 12);
        assert_eq!(canopy_perches("tree_hero_oak").len(), 12);
        let tiles = [
            tile(0, 0, "meadow", 0),
            tile(1, 0, "forest", 2),
            tile(0, 1, "road_straight", 0),
        ];
        let r = roosts(&tiles, &c, &|_| true);
        assert_eq!(r.len(), 12, "the oak's 12 perches, within the rule's max");
        assert!(r.iter().all(|x| x.cell == Axial::new(1, 0)));
        let centre = Vec2::from(Axial::new(1, 0).to_world(HEX_RADIUS));
        for x in &r {
            assert!(x.pos.xz().distance(centre) < 8.0, "{x:?} over the oak");
            assert!(x.pos.y > 5.0 && x.pos.y < 12.0, "{x:?} in the canopy");
        }
        // The roosts turn with the tile.
        let r0 = roosts(&[tile(1, 0, "forest", 0)], &c, &|_| true);
        assert!(r0.iter().zip(&r).any(|(a, b)| a.pos.distance(b.pos) > 0.5));
        assert!(
            roosts(
                &[tile(0, 0, "meadow", 0), tile(0, 1, "ocean", 0)],
                &c,
                &|_| true
            )
            .is_empty()
        );
        assert!(
            roosts(&tiles, &c, &|p| p != "tree_hero_oak").is_empty(),
            "no oak drawn, no roosts"
        );
    }

    /// The oak perches sit on the GLB's canopy shell and are open from above.
    #[test]
    fn oak_perches_sit_on_the_canopy() {
        let root = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/kit/p1/tree_hero_oak/tree_hero_oak.glb"
        );
        let bytes = std::fs::read(root).unwrap();
        let plain = hex_assets::rewrite_glb(&bytes).unwrap();
        let (doc, buffers, _) = gltf::import_slice(&plain).unwrap();
        let mesh = doc
            .meshes()
            .find(|m| m.name() == Some("tree_hero_oak_lod0"))
            .unwrap();
        let mut canopy: Vec<Vec3> = vec![];
        for prim in mesh.primitives() {
            if prim.material().name() != Some("tree_hero_oak_canopy") {
                continue;
            }
            let rd = prim.reader(|b| Some(&buffers[b.index()]));
            canopy.extend(rd.read_positions().unwrap().map(Vec3::from));
        }
        assert!(canopy.len() > 100, "canopy primitive found");
        for p in canopy_perches("tree_hero_oak") {
            let near = canopy
                .iter()
                .map(|v| v.distance(p))
                .fold(f32::MAX, f32::min);
            assert!(near < 0.5, "perch {p} is {near} m from the canopy shell");
            let above = canopy
                .iter()
                .filter(|v| v.xz().distance(p.xz()) < 0.8)
                .map(|v| v.y)
                .fold(f32::MIN, f32::max);
            assert!(
                above < p.y + 0.35,
                "perch {p} is buried {} m under the canopy",
                above - p.y
            );
        }
    }

    #[test]
    fn placing_and_bulldozing_a_forest_adds_and_removes_roosts() {
        let c = catalog();
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 7);
        assert!(plan.set_world_all(&[tile(0, 0, "meadow", 0)], &c, 0.0));
        assert!(plan.roosts.is_empty());
        assert!(
            !plan.set_world_all(&[tile(0, 0, "meadow", 0)], &c, 0.1),
            "unchanged world"
        );
        // Place: 12 roosts, birds fly in and are roosting after the approach.
        let world = [tile(0, 0, "meadow", 0), tile(1, 0, "forest", 0)];
        assert!(plan.set_world_all(&world, &c, 1.0));
        assert_eq!(plan.roosts.len(), 12);
        plan.tick(1.0);
        let n = plan.counts(1.0);
        assert_eq!((n.roosting, n.settling), (0, 12));
        let n = plan.counts(1.0 + APPROACH);
        assert_eq!((n.roosting, n.settling), (12, 0));
        // The wave launches them, and the next cycle takes birds again.
        plan.tick(1.0 + OCCUPIED + 0.1);
        assert_eq!(plan.counts(1.0 + OCCUPIED + 0.1).roosting, 0);
        plan.tick(1.0 + CYCLE + 0.1);
        assert_eq!(plan.counts(1.0 + CYCLE + 0.1).settling, 12);
        // Bulldoze: the roosts go and every bird on them is released.
        assert!(plan.set_world_all(&[tile(0, 0, "meadow", 0)], &c, 50.0));
        assert!(plan.roosts.is_empty());
        assert!(plan.slots.iter().all(|s| s.roost.is_none()));
        // Undo the bulldoze: back again, a fresh cycle.
        assert!(plan.set_world_all(&world, &c, 60.0));
        plan.tick(60.0);
        assert_eq!(plan.counts(60.0).settling, 12);
    }

    #[test]
    fn a_world_opening_with_a_forest_opens_with_perched_birds() {
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 3);
        // Before play's first rebuild no tile is past its rise.
        plan.set_world_all(&[], &catalog(), 0.0);
        assert!(plan.roosts.is_empty());
        plan.set_world_all(&[tile(0, 0, "forest", 0)], &catalog(), 0.5);
        plan.tick(0.5);
        assert_eq!(plan.counts(0.5).roosting, 12);
        let perched: Vec<&Slot> = plan.slots.iter().filter(|s| s.roost.is_some()).collect();
        for s in perched {
            let i = plan.roost_index(s.roost.unwrap()).unwrap();
            assert_eq!(s.spawn, plan.roosts[i].pos);
        }
    }

    /// A coast closing on an ocean, and a river chain away from it: two regions.
    fn water_world() -> Vec<PlayTile> {
        vec![
            tile(0, 0, "meadow", 0),
            tile(1, 0, "ocean", 0),
            tile(2, 0, "ocean", 0),
            tile(1, 1, "ocean", 0),
            tile(0, -1, "beach", 0),
            tile(-3, 0, "river_straight", 0),
            tile(-2, 0, "river_straight", 0),
            tile(-4, 0, "river_bend", 0),
        ]
    }

    #[test]
    fn fish_regions_follow_continuous_water_and_share_fish_by_tiles() {
        let c = catalog();
        let mut w = FishWorld::new(&water_world(), &c);
        assert_eq!(w.cells.len(), 7);
        let sizes: Vec<usize> = w.regions.iter().map(|r| r.cells.len()).collect();
        assert!(sizes.contains(&3), "the ocean tiles join: {sizes:?}");
        assert!(
            w.regions.iter().all(|r| r.room >= SPAWN_ROOM),
            "every region has room"
        );
        w.allocate(84);
        assert_eq!(w.regions.iter().map(|r| r.fish).sum::<u32>(), 84);
        let ocean = w
            .regions
            .iter()
            .find(|r| r.cells.contains(&Axial::new(1, 0)))
            .unwrap();
        assert!(ocean.fish >= 36, "{ocean:?}");
        // The play plan: 7 water tiles, 84 fish, 916 birds.
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 1);
        plan.set_world_all(&water_world(), &c, 0.0);
        assert_eq!((plan.birds, plan.fish), (916, 84));
        let n = plan.counts(0.0);
        assert_eq!(n.regions.iter().map(|r| r.2).sum::<u32>(), 84);
        assert!(plan.slots[..916].iter().all(|s| s.kind == SlotKind::Bird));
        assert!(plan.slots[916..].iter().all(|s| s.kind == SlotKind::Fish));
    }

    #[test]
    fn the_baked_band_matches_the_analytic_bed() {
        let w = FishWorld::new(&water_world(), &catalog());
        let mut rng = Rng(5);
        let mut checked = 0;
        for _ in 0..20_000 {
            let cell = *w
                .cells
                .keys()
                .nth((rng.next_u64() % w.cells.len() as u64) as usize)
                .unwrap();
            let p = Vec2::from(cell.to_world(HEX_RADIUS))
                + Vec2::new(rng.unit() * 2.0 - 1.0, rng.unit() * 2.0 - 1.0) * HEX_RADIUS;
            let (a, g) = (band_at(&w.cells, p), w.field.sample(p));
            // Exact at sample points; between them only the bilinear error on steep beds.
            if a[2] >= 0.0 && g[2] >= 0.0 {
                assert!((a[0] - g[0]).abs() < 0.2, "floor at {p}: {a:?} vs {g:?}");
                assert!((a[1] - g[1]).abs() < 0.2, "ceil at {p}: {a:?} vs {g:?}");
                checked += 1;
            }
        }
        assert!(checked > 2_000, "{checked}");
        let s = w.field.samples[0];
        let k = w.field.chunks.iter().position(|&k| k == 0).unwrap() as i32;
        let origin = Vec2::new(
            (k % w.field.w as i32 + w.field.x0) as f32,
            (k / w.field.w as i32 + w.field.z0) as f32,
        ) * CHUNK;
        assert_eq!(band_at(&w.cells, origin), s);
    }

    /// Spawn points and a long confined random walk stay in the water: above the analytic bed,
    /// below the surface, inside water tiles and clear of the cut faces.
    #[test]
    fn fish_spawn_and_stay_inside_the_water_volume() {
        let c = catalog();
        let mut w = FishWorld::new(&water_world(), &c);
        w.allocate(84);
        let check = |p: Vec3, what: &str| {
            let cell = cell_at(p.xz());
            let cw = w
                .cells
                .get(&cell)
                .unwrap_or_else(|| panic!("{what} {p} left the water tiles"));
            let bed = cw.field().height(p.x, p.z);
            assert!(
                p.y - FISH_HALF_H >= bed + 0.1,
                "{what} {p} under the bed {bed}"
            );
            assert!(
                p.y + FISH_HALF_H <= WATER_Y - 0.08,
                "{what} {p} through the surface"
            );
            let local = p.xz() - Vec2::from(cell.to_world(HEX_RADIUS));
            for &e in &cw.exposed {
                assert!(
                    edge_distance(local, e) > 1.0,
                    "{what} {p} at the cut face of edge {e}"
                );
            }
        };
        let mut fish = vec![];
        for (ri, r) in w.regions.iter().enumerate() {
            for k in 0..r.fish {
                let p = w.spawn(ri, 1, k + 100 * ri as u32);
                check(p, "spawn");
                assert!(r.cells.contains(&cell_at(p.xz())), "spawn in its region");
                fish.push(p);
            }
        }
        let mut rng = Rng(11);
        let dt = 1.0 / 30.0;
        let mut turned_back = 0;
        for p in &mut fish {
            let a = rng.unit() * std::f32::consts::TAU;
            let mut v = Vec3::new(a.cos(), 0.0, a.sin()) * 2.0;
            for _ in 0..1_500 {
                let turn = (rng.unit() - 0.5) * 0.6;
                v = Quat::from_rotation_y(turn) * v + Vec3::Y * (rng.unit() - 0.5) * 0.4;
                v = v.clamp_length(0.5, 2.5);
                turned_back += usize::from(w.field.sample((*p + v * dt).xz())[2] < 0.0);
                (*p, v) = confine(&w.field, *p, v, dt);
                check(*p, "swim");
            }
        }
        assert!(
            turned_back > 100,
            "the walk reached the walls: {turned_back}"
        );
    }
}
