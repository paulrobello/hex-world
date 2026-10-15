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
        tiles::{self, CellWater, WATER_Y, WaterKind},
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

    /// The caps per tier: PRD §9.2's agent caps (Low 25%, Medium 50%, High and Ultra 100%) applied to
    /// the High budget. The per-water-tile fish density stays, so only the caps shrink.
    pub fn for_tier(tier: QualityTier) -> BoidBudget {
        let percent = match tier {
            QualityTier::Low => 25,
            QualityTier::Medium => 50,
            QualityTier::High | QualityTier::Ultra => 100,
        };
        BoidBudget {
            total: Self::HIGH.total * percent / 100,
            fish_max: Self::HIGH.fish_max * percent / 100,
            ..Self::HIGH
        }
    }

    /// (birds, fish) for a world with `water_tiles` water tiles. Fish scale with the water up to
    /// `fish_max`; birds are [`BIRD_PERCENT`] of the slots the fish leave (owner A13: 25% fewer
    /// than filling them), and the rest stay idle.
    pub fn split(&self, water_tiles: usize) -> (u32, u32) {
        let fish = (u64::from(self.fish_per_water_tile) * water_tiles as u64)
            .min(u64::from(self.fish_max.min(self.total))) as u32;
        ((self.total - fish) * BIRD_PERCENT / 100, fish)
    }
}

/// Share of the slots fish leave that play fills with birds (owner A13, 2026-10-14: 25% fewer
/// birds than the 100% it was).
pub const BIRD_PERCENT: u32 = 75;
/// Birds per flock (owner U2: smaller flocks). A play bird coheres and aligns only with the birds
/// of its own flock (slots `k * FLOCK_SIZE ..`, within 22 m) and keeps clear of every bird. A flock
/// spawns together and roosts together, so an oak's 12 perches take one whole flock. Before U2
/// every free bird cohered with every other within 22 m, so the sky held one cloud.
pub const FLOCK_SIZE: u32 = 12;
/// Horizontal radius (m) a flock spawns within around its centre.
pub const FLOCK_SPREAD: f32 = 6.0;
/// Land tiles (placed tiles without water: water tiles bring fish instead) per flock in play
/// (owner, 2026-10-14: the bird count scales with the map, from none over the single start
/// tile). Whole flocks only, up to the bird slots the tier leaves ([`BoidBudget::split`]).
pub const LAND_PER_FLOCK: u32 = 4;
/// Seconds a removed flock flies outward before its slots go idle: at 12 m/s it is well past
/// the sky's edge by then.
pub const LEAVE: f32 = 12.0;
/// How far past the sky's edge (m) an arriving flock appears; the sky's pull brings it in.
pub const ARRIVE_OUT: f32 = 40.0;

/// Birds flying over `land` land tiles with `slots` bird slots: [`LAND_PER_FLOCK`] land tiles a
/// flock, whole flocks within the slots.
pub fn flying_birds(land: usize, slots: u32) -> u32 {
    let flocks = (land as u32 / LAND_PER_FLOCK).min(slots / FLOCK_SIZE);
    flocks * FLOCK_SIZE
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
/// The oak GLB is built at `OAK_SCALE` times the layout above (owner 2026-10-14: the tree 20% bigger;
/// `SCALE` in scripts/blender/kit/tree_hero_oak.py, applied to every LOD about the trunk base), so
/// clump centres, radii and the perch lifts scale by it.
const OAK_SCALE: f32 = 1.2;
/// `_tree.DOME_WIDE` and `_tree.DOME_TOP`: the master dome's horizontal and upper vertical scale.
const DOME_WIDE: f32 = 1.15;
const DOME_TOP: f32 = 0.9;
/// `tree_forest`'s spire top and its crown radius at 10 m (`_forest.HEIGHT`, `crown_r(10.0)`).
const SPRUCE_TOP: f32 = 16.0;
const SPRUCE_R10: f32 = 1.8;
/// The spruce variants' spire tops and a crown ring (height, radius), from their `_spruce` shapes
/// (`HEIGHT` less 0.2 m, `crown_r` at about 0.6 of the height): `tree_forest_b`, `tree_forest_c`.
const SPRUCE_B: (f32, f32, f32) = (20.3, 13.0, 1.5);
const SPRUCE_C: (f32, f32, f32) = (8.0, 5.0, 1.35);

/// A spruce's perches: its spire top and three points on a crown ring.
fn spruce_perches(top: f32, ring_y: f32, ring_r: f32) -> Vec<Vec3> {
    let mut v = vec![Vec3::new(0.0, top, 0.0)];
    v.extend((0..3).map(|k| {
        let a = (120.0 * k as f32 + 30.0).to_radians();
        Vec3::new(a.cos() * ring_r, ring_y, a.sin() * ring_r)
    }));
    v
}

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
/// each lies on a dome no higher dome covers, at its top or on the outer rim of its top, never
/// mid-slope (owner, 2026-10-14: at [`PERCH_MIN_ELEVATION`] or above the dome tilts at most about
/// 20 degrees, so a level bird sits on it). Only the domes of clumps 2, 3, 5, 9 and 10 have open
/// tops; the others' tops lie under a higher dome. Twelve, the forest's bird `max`, at least
/// about 0.6 m apart. The last field (1x metres, scaled by [`OAK_SCALE`]) lifts the dome point
/// (the canopy shell) to the top of the foliage there, leaf cards included, measured against the
/// GLB by the same test.
const OAK_PERCHES: [(usize, f32, f32, f32); 12] = [
    (10, 80.0, 0.0, 0.04),
    (10, 72.0, 1.2, 0.04),
    (10, 72.0, -1.2, 0.04),
    (9, 80.0, 0.0, 0.10),
    (9, 72.0, 1.2, 0.20),
    (9, 72.0, -1.2, 0.06),
    (5, 72.0, -0.6, 0.0),
    (5, 72.0, 0.3, 0.04),
    (2, 68.0, -0.3, 0.07),
    (2, 68.0, 0.9, 0.09),
    (3, 68.0, -0.3, -0.05),
    (3, 68.0, 0.9, 0.07),
];
/// The lowest perch elevation on an oak dome (degrees): the dome's normal tilts about 20 degrees
/// from vertical there.
pub const PERCH_MIN_ELEVATION: f32 = 68.0;

/// Perch points on a tree piece's canopy, piece-local in the runtime frame (Y up, trunk base at
/// the origin). Empty for a piece birds do not roost on.
pub fn canopy_perches(piece: &str) -> Vec<Vec3> {
    match piece {
        "tree_hero_oak" => OAK_PERCHES
            .iter()
            .map(|&(k, el, daz, lift)| {
                let (c, r, _) = OAK_CLUMPS[k];
                let c = c.map(|x| x * OAK_SCALE);
                dome_point(c, r * OAK_SCALE, c[1].atan2(c[0]) + daz, el.to_radians())
                    + Vec3::Y * (lift * OAK_SCALE)
            })
            .collect(),
        "tree_forest" => spruce_perches(SPRUCE_TOP, 10.0, SPRUCE_R10),
        "tree_forest_b" => spruce_perches(SPRUCE_B.0, SPRUCE_B.1, SPRUCE_B.2),
        "tree_forest_c" => spruce_perches(SPRUCE_C.0, SPRUCE_C.1, SPRUCE_C.2),
        _ => vec![],
    }
}

/// One tree on a tile, tile-local (before the tile's rotation).
#[derive(Clone, Debug, PartialEq)]
pub struct TreeInstance {
    pub piece: String,
    pub at: Vec3,
    pub yaw: Quat,
    pub scale: f32,
}

impl TreeInstance {
    /// Its canopy perches, tile-local, each with the level direction a bird sitting there faces:
    /// out from the trunk (the tree's +x for a perch on its axis, a spire top).
    fn perches(&self) -> impl Iterator<Item = (Vec3, Vec3)> + '_ {
        canopy_perches(&self.piece).into_iter().map(|p| {
            let out = Vec3::new(p.x, 0.0, p.z).try_normalize().unwrap_or(Vec3::X);
            (self.at + self.yaw * (p * self.scale), self.yaw * out)
        })
    }
}

/// The hero trees on a tile. Its scattered spruces come from `play::scatter` through
/// [`BoidPlan::set_scatter`].
pub fn tree_instances(def: &TileDef) -> Vec<TreeInstance> {
    def.props
        .hero
        .iter()
        .map(|h| TreeInstance {
            piece: h.piece.as_str().to_owned(),
            at: Vec3::new(h.at[0], 0.0, h.at[1]),
            yaw: Quat::from_rotation_y(-h.yaw_deg.to_radians()),
            scale: 1.0,
        })
        .collect()
}

/// A perch birds settle on: slot `slot` of the tile at `cell`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Roost {
    pub cell: Axial,
    pub slot: u8,
    pub pos: Vec3,
    /// The level direction a perched bird faces: out from its tree's trunk.
    pub face: Vec3,
}

/// Tree pieces with canopy perches: what [`BoidPlan::set_world`] watches arrive in the kit.
pub const PERCH_PIECES: [&str; 4] = [
    "tree_hero_oak",
    "tree_forest",
    "tree_forest_b",
    "tree_forest_c",
];

/// Bird roosts of the placed tiles from their hero trees alone ([`roosts_with`], no scatter).
pub fn roosts(
    tiles: &[PlayTile],
    catalog: &TileCatalog,
    drawn: &dyn Fn(&str) -> bool,
) -> Vec<Roost> {
    roosts_with(tiles, catalog, drawn, &BTreeMap::new())
}

/// Bird roosts of the placed tiles: the canopy perches of each tile with a `Bird` agent rule,
/// capped at the rule's `max` (the forest's 12), in world space. Only hero trees `drawn` reports
/// as kit pieces count, so a forest still on its placeholder (its pack streaming) has no roosts
/// until the oak swaps in. `scatter` holds each cell's drawn scattered trees, nearest the centre
/// first. With both, the hero canopies keep half the cap (rounded up) and the scattered trees take
/// the rest, every spire top before any lower perch; either side's unused share goes to the other.
/// Sorted by cell, then slot (hero perches first).
pub fn roosts_with(
    tiles: &[PlayTile],
    catalog: &TileCatalog,
    drawn: &dyn Fn(&str) -> bool,
    scatter: &BTreeMap<Axial, Vec<TreeInstance>>,
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
        let max = max.min(usize::from(u8::MAX));
        let hero: Vec<(Vec3, Vec3)> = tree_instances(def)
            .iter()
            .filter(|t| drawn(&t.piece))
            .flat_map(|t| t.perches().collect::<Vec<_>>())
            .collect();
        // Perch k of every scattered tree before perch k + 1 of any.
        let trees: Vec<Vec<(Vec3, Vec3)>> = scatter
            .get(&t.cell)
            .map(|v| v.iter().map(|tree| tree.perches().collect()).collect())
            .unwrap_or_default();
        let deepest = trees.iter().map(Vec::len).max().unwrap_or(0);
        let spread: Vec<(Vec3, Vec3)> = (0..deepest)
            .flat_map(|k| trees.iter().filter_map(move |p| p.get(k).copied()))
            .collect();
        let hero_share = if spread.is_empty() {
            max
        } else {
            max.div_ceil(2)
        };
        let take_spread = spread.len().min(max - hero.len().min(hero_share));
        let take_hero = hero.len().min(max - take_spread);
        let perches = hero
            .into_iter()
            .take(take_hero)
            .chain(spread.into_iter().take(take_spread));
        for (slot, (local, face)) in perches.enumerate() {
            out.push(Roost {
                cell: t.cell,
                slot: slot as u8,
                pos: centre + rot * local,
                face: rot * face,
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
/// The trout at display scale (`fish_trout`'s LOD0 spans y -0.062..0.086 m, drawn at
/// `boids::FISH_SCALE` 2): its dorsal fin's top over the pivot and its belly under it (m).
pub const TROUT_TOP: f32 = 0.172;
pub const TROUT_BELLY: f32 = 0.124;
/// A river mouth's fan shallows to 0.36 m at the shelf joint, too thin for the band above (owner,
/// 2026-10-14: fish skim the shallows and the fan is not deepened). Over a mouth tile the band
/// hugs the surface instead, the dorsal fin and the belly each [`SKIM_GAP`] clear, blending to the
/// usual band over the water depths [`SKIM_DEPTH`].
pub const SKIM_GAP: f32 = 0.02;
pub const SKIM_DEPTH: (f32, f32) = (0.8, 1.2);
/// River and spring water is murky (owner, 2026-10-14: river fish were not seen), so there the
/// band is the top [`RIVER_BAND`] metres under a ceiling as close to the surface as the mouth's
/// skim band: the fish hold the top of the water. The room is the usual band's, so the channel's
/// extent and steering do not change.
pub const RIVER_BAND: f32 = 0.35;
/// River-palette fish draw up to this much larger than the trout's display scale where the band's
/// ceiling leaves their fin [`SKIM_GAP`] under the surface (the compute pass eases `anim.w`
/// toward it; the material scales by `1 + anim.w`): about 1 m long in river water, the usual size
/// on a mouth's skim water. The river band's ceiling leaves that room.
pub const RIVER_GROW: f32 = 0.3;
/// Band height (m) over which skim water's room rises to [`river_room`]: the shore turn in
/// boids_compute.wgsl pushes up the room gradient where room is under 1, so a fan whose room
/// dipped below the channel's would turn fish back at both ends.
pub const SKIM_RAMP: f32 = 0.02;
/// Band height (m) under which a fish's climb is damped toward level: the trout is 0.8 m long at
/// display scale and pitches with its velocity, so in a thin band its nose would cross the surface
/// or the bed.
pub const LEVEL_BAND: f32 = 0.5;

/// The room on a river channel's centre line (m): the room a mouth's skim water holds.
pub fn river_room() -> f32 {
    WATER_Y - tiles::river_height(0.0) - BED_CLEAR - SURF_CLEAR
}

/// River water for the fish: a river or spring tile (river fish keep small groups and the river
/// palette there); a mouth, coast or ocean tile is sea water.
pub fn river_water(kind: &WaterKind) -> bool {
    matches!(kind, WaterKind::River | WaterKind::Spring(_))
}

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
/// exposed edge (a cut face); below 0 a fish may not be there. Over a river mouth the band skims
/// the shallows ([`SKIM_GAP`]) and open skim water holds [`river_room`]. [`LAND`] outside water
/// tiles.
pub fn band_at(cells: &BTreeMap<Axial, CellWater>, p: Vec2) -> [f32; 4] {
    let cell = cell_at(p);
    let Some(cw) = cells.get(&cell) else {
        return LAND;
    };
    let field = cw.field();
    let bed = field.height(p.x, p.y);
    let s = field.water(p.x, p.y);
    let atten = smoothstep(0.05, 2.5, s.depth) * (1.0 - s.river);
    let mut floor = bed + BED_CLEAR;
    let mut ceil = WATER_Y - SURF_CLEAR - WAVE_BOUND * atten;
    let mut height = ceil - floor;
    if matches!(cw.kind, WaterKind::RiverMouth(_)) {
        let w = 1.0 - smoothstep(SKIM_DEPTH.0, SKIM_DEPTH.1, s.depth);
        floor += (bed + TROUT_BELLY + SKIM_GAP - floor) * w;
        ceil += (WATER_Y - TROUT_TOP - SKIM_GAP - ceil) * w;
        let h = ceil - floor;
        height = if h < 0.0 {
            h
        } else {
            h.max(river_room() * smoothstep(0.0, SKIM_RAMP, h))
        };
    }
    if river_water(&cw.kind) {
        ceil = WATER_Y - TROUT_TOP * (1.0 + RIVER_GROW) - SKIM_GAP;
        floor = floor.max(ceil - RIVER_BAND);
    }
    let local = p - Vec2::from(cell.to_world(HEX_RADIUS));
    let wall = cw
        .exposed
        .iter()
        .map(|&e| edge_distance(local, e))
        .fold(f32::MAX, f32::min);
    [floor, ceil, height.min(wall - WALL_CLEAR), 0.0]
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
/// then it clamps y into the band and damps the climb where the band is under [`LEVEL_BAND`].
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
    nv.y *= ((b[1] - b[0]) / LEVEL_BAND).clamp(0.0, 1.0);
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
    /// River water alone (no ocean or beach tile): its fish swim alone or in small groups
    /// ([`river_groups`]) instead of one school.
    pub river: bool,
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
                let river = cells_in.iter().all(|c| {
                    catalog.get(&cells[c].tile.tile).is_some_and(|d| {
                        d.edges
                            .iter()
                            .all(|e| !matches!(e.kind, SocketKind::Ocean | SocketKind::Beach))
                    })
                });
                FishRegion {
                    cells: cells_in,
                    fish: 0,
                    best,
                    room,
                    river,
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

    /// Region `region`'s cells of one water side: river and spring tiles (`river`), else the
    /// mouth, coast and ocean tiles. Sorted, as the region's.
    pub fn side(&self, region: usize, river: bool) -> Vec<Axial> {
        self.regions[region]
            .cells
            .iter()
            .copied()
            .filter(|c| self.cells.get(c).is_some_and(|w| river_water(&w.kind)) == river)
            .collect()
    }

    /// Whether `p` lies in river water ([`river_water`]).
    pub fn river_at(&self, p: Vec2) -> bool {
        self.cells
            .get(&cell_at(p))
            .is_some_and(|w| river_water(&w.kind))
    }

    /// The band overlap at `p` where both the analytic band and the baked one leave a fish room
    /// and the band is [`SPAWN_ROOM`] thick (a mouth's skim water holds room but is too thin).
    fn roomy(&self, p: Vec2) -> Option<(f32, f32)> {
        let (a, g) = (band_at(&self.cells, p), self.field.sample(p));
        let (lo, hi) = (a[0].max(g[0]), a[1].min(g[1]));
        (a[2] >= SPAWN_ROOM && g[2] >= SPAWN_ROOM && a[1] - a[0] >= SPAWN_ROOM && hi >= lo)
            .then_some((lo, hi))
    }

    /// A spawn point for member `k` of a group whose first fish spawned at `lead`: beside it where
    /// the water has room and is of the lead's side, else on it.
    pub fn beside(&self, lead: Vec3, k: usize) -> Vec3 {
        let a = k as f32 * 2.4;
        let q = lead.xz() + Vec2::new(a.cos(), a.sin()) * 0.6;
        match self.roomy(q) {
            Some((lo, hi)) if self.river_at(q) == self.river_at(lead.xz()) => {
                Vec3::new(q.x, lead.y.clamp(lo, hi), q.y)
            }
            _ => lead,
        }
    }

    /// A spawn point in region `region` for boid slot `slot`: a seeded point of its water (of
    /// `cells`, one side of it) where both the analytic band and the baked one leave room, at the
    /// middle of their overlap.
    pub fn spawn(&self, region: usize, cells: &[Axial], seed: u64, slot: u32) -> Vec3 {
        let r = &self.regions[region];
        let mut rng =
            Rng(cell_seed(seed, r.cells[0]) ^ u64::from(slot).wrapping_mul(0xA24B_AED4_963E_E407));
        if cells.is_empty() {
            return r.best;
        }
        for _ in 0..64 {
            let cell = cells[(rng.next_u64() % cells.len() as u64) as usize];
            let c = Vec2::from(cell.to_world(HEX_RADIUS));
            let p = c + Vec2::new(rng.unit() * 2.0 - 1.0, rng.unit() * 2.0 - 1.0) * HEX_RADIUS;
            if cell_at(p) != cell {
                continue;
            }
            if let Some((lo, hi)) = self.roomy(p) {
                return Vec3::new(p.x, 0.5 * (lo + hi), p.y);
            }
        }
        if cells.len() == r.cells.len() {
            r.best
        } else {
            best_point(&self.field, cells).0
        }
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

/// Share of a river region's fish that swim alone (owner, 2026-10-14: rivers hold mostly single
/// fish and small groups of 2 to 5).
pub const RIVER_ALONE: f32 = 0.6;
/// The largest group of river fish.
pub const RIVER_GROUP_MAX: u32 = 5;

/// The group sizes `fish` river fish of the region named by `cell` split into, in slot order:
/// [`RIVER_ALONE`] of them (rounded) alone, the rest in groups of 2 to [`RIVER_GROUP_MAX`] (a
/// last odd fish alone too), shuffled. Seeded from the world seed and the cell.
pub fn river_groups(seed: u64, cell: Axial, fish: u32) -> Vec<u32> {
    let mut rng = Rng(cell_seed(seed, cell) ^ 0x6E1D_F15A_0B5E_2C47);
    let alone = ((fish as f32 * RIVER_ALONE).round() as u32).min(fish);
    let mut out = vec![1; alone as usize];
    let mut left = fish - alone;
    while left > 0 {
        let n = (2 + (rng.next_u64() % u64::from(RIVER_GROUP_MAX - 1)) as u32).min(left);
        out.push(n);
        left -= n;
    }
    for k in (1..out.len()).rev() {
        out.swap(k, (rng.next_u64() % (k as u64 + 1)) as usize);
    }
    out
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
    /// A slot play leaves empty ([`BIRD_PERCENT`], [`flying_birds`]): parked and not drawn.
    Idle,
    /// A bird of a flock the world no longer holds: it flies off the map (from `assigned_at`)
    /// and its slot goes idle after [`LEAVE`] seconds.
    Leaving,
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

/// One fish region as the published state reports it.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionCount {
    /// Its first cell.
    pub cell: Axial,
    pub tiles: u32,
    pub fish: u32,
    pub river: bool,
    /// The most fish that school together (a river's largest group; an ocean's every fish).
    pub largest_group: u32,
    /// Fish swimming alone.
    pub singles: u32,
    /// Fish spawned in its river water ([`river_water`]): they keep [`river_groups`] even where the
    /// region joins the sea. Their largest group and how many swim alone.
    pub river_fish: u32,
    pub river_largest_group: u32,
    pub river_singles: u32,
}

/// What the published state reports.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoidCounts {
    pub budget: u32,
    /// Birds in play ([`flying_birds`]), out of `bird_slots`; `leaving` birds are flying off.
    pub birds: u32,
    pub bird_slots: u32,
    pub leaving: u32,
    pub fish: u32,
    pub roosts: u32,
    pub roosting: u32,
    pub settling: u32,
    pub water_tiles: u32,
    pub regions: Vec<RegionCount>,
}

/// The CPU state of the boids. `play` worlds get roosts, fish and the play sky; the other
/// scenarios keep the P0 stress flock (every slot a bird over a 800 m square, no roosts).
#[derive(Clone, Debug)]
pub struct BoidPlan {
    pub budget: BoidBudget,
    pub seed: u64,
    pub play: bool,
    pub slots: Vec<Slot>,
    /// Bird slots (`0..birds`; fish follow): the tier's share the fish leave.
    pub birds: u32,
    /// Birds in play: slots `0..active`, whole flocks ([`flying_birds`]).
    pub active: u32,
    pub fish: u32,
    pub roosts: Vec<Roost>,
    /// Roost groups by cell: (cycle start, perches taken).
    groups: BTreeMap<Axial, (f32, bool)>,
    pub water: FishWorld,
    /// Each fish's school by fish index (slot minus `birds`): 0 for the fish spawned in a region's
    /// sea water (one school), else a key its river group alone shares (the group's first fish
    /// index plus 1). A fish's spawn water decides, so a river joined to the sea keeps its groups.
    pub schools: Vec<u32>,
    /// Flock centre (xyz) and horizontal radius (w).
    pub sky: Vec4,
    cursor: usize,
    tiles: Option<Vec<PlayTile>>,
    /// Which of [`PERCH_PIECES`] the kit draws, as of the last `set_world`.
    drawn: Vec<bool>,
    /// Drawn scattered trees by cell ([`Self::set_scatter`]), their revision, and whether they
    /// changed since the last `set_world`.
    scatter: BTreeMap<Axial, Vec<TreeInstance>>,
    scatter_rev: u64,
    scatter_dirty: bool,
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
            active: budget.total,
            fish: 0,
            roosts: vec![],
            groups: BTreeMap::new(),
            water: FishWorld::default(),
            schools: vec![],
            sky: Vec4::new(0.0, 80.0, 0.0, 400.0),
            cursor: 0,
            tiles: None,
            drawn: vec![],
            scatter: BTreeMap::new(),
            scatter_rev: 0,
            scatter_dirty: false,
            slots_dirty: true,
            world_dirty: true,
        }
    }

    /// The drawn scattered trees by cell (tile-local, nearest the centre first) at `revision`;
    /// a new revision makes the next `set_world` rebuild the roosts.
    pub fn set_scatter(
        &mut self,
        revision: u64,
        trees: impl FnOnce() -> BTreeMap<Axial, Vec<TreeInstance>>,
    ) {
        if revision != self.scatter_rev {
            self.scatter_rev = revision;
            self.scatter = trees();
            self.scatter_dirty = true;
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

    /// A play plan with no world yet: every slot idle, the sky over the origin.
    pub fn play(budget: BoidBudget, seed: u64) -> BoidPlan {
        let mut plan = Self::empty(budget, seed, true);
        plan.sky = Vec4::new(
            0.0,
            0.5 * (SKY_LOW + SKY_HIGH),
            0.0,
            HEX_RADIUS + SKY_MARGIN,
        );
        plan.active = 0;
        plan.slots = vec![
            Slot {
                kind: SlotKind::Idle,
                generation: 1,
                spawn: Vec3::ZERO,
                roost: None,
                assigned_at: 0.0,
                region: None,
            };
            budget.total as usize
        ];
        plan
    }

    /// Where bird slot `slot` arrives from: its flock's seeded heading, [`ARRIVE_OUT`] past the
    /// sky's edge, at its sky height.
    fn arrive_point(&self, slot: u32) -> Vec3 {
        let (p, c) = (self.sky_point(slot), self.sky_point_centre(slot));
        let sky = Vec2::new(self.sky.x, self.sky.z);
        let off = (c.xz() - sky).try_normalize().unwrap_or(Vec2::X);
        let at = sky + off * (self.sky.w + ARRIVE_OUT) + (p.xz() - c.xz());
        Vec3::new(at.x, p.y, at.y)
    }

    /// The seeded centre of bird slot `slot`'s flock (y 0): [`Self::sky_point`]'s first two draws.
    fn sky_point_centre(&self, slot: u32) -> Vec3 {
        let flock = slot / FLOCK_SIZE;
        let mut rng = Rng(self.seed ^ u64::from(flock).wrapping_mul(0xD1B5_4A32_D192_ED03));
        let a = rng.unit() * std::f32::consts::TAU;
        let d = rng.unit().sqrt() * self.sky.w;
        Vec3::new(self.sky.x + a.cos() * d, 0.0, self.sky.z + a.sin() * d)
    }

    /// Where bird slot `slot` spawns: within [`FLOCK_SPREAD`] of its flock's seeded point in the sky.
    fn sky_point(&self, slot: u32) -> Vec3 {
        let flock = slot / FLOCK_SIZE;
        let mut rng = Rng(self.seed ^ u64::from(flock).wrapping_mul(0xD1B5_4A32_D192_ED03));
        let a = rng.unit() * std::f32::consts::TAU;
        let d = rng.unit().sqrt() * self.sky.w;
        let y = SKY_LOW + 2.0 + rng.unit() * (SKY_HIGH - SKY_LOW - 4.0);
        let mut own = Rng(self.seed ^ u64::from(slot).wrapping_mul(0xA076_1D64_78BD_642F));
        let b = own.unit() * std::f32::consts::TAU;
        let e = own.unit().sqrt() * FLOCK_SPREAD;
        Vec3::new(
            self.sky.x + a.cos() * d + b.cos() * e,
            y + (own.unit() - 0.5) * 3.0,
            self.sky.z + a.sin() * d + b.sin() * e,
        )
    }

    /// Rebuilds roosts, fish regions, the field and the bird/fish/idle split from the placed tiles
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
        if !self.play
            || (self.tiles.as_deref() == Some(tiles) && self.drawn == now && !self.scatter_dirty)
        {
            return false;
        }
        // The world appears all at once (entering play draws the start world, or a loaded one, at
        // rest), so the first non-empty tile set is the opening world.
        let first = self.tiles.as_ref().is_none_or(|t| t.is_empty());
        self.tiles = Some(tiles.to_vec());
        self.drawn = now;
        self.scatter_dirty = false;
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
        (self.birds, self.fish) = ((self.budget.total - fish) * BIRD_PERCENT / 100, fish);
        let land = tiles.len().saturating_sub(self.water.cells.len());
        self.active = flying_birds(land, self.birds);
        // Each region's fish split between its river and sea water by tile count, river first:
        // river fish in small groups, sea fish one school.
        let mut region_of = vec![];
        let mut sides: Vec<[Vec<Axial>; 2]> = vec![];
        self.schools.clear();
        for (ri, r) in self.water.regions.iter().enumerate() {
            let side = [self.water.side(ri, true), self.water.side(ri, false)];
            let n = allocate(r.fish, &[side[0].len() as u32, side[1].len() as u32]);
            region_of.extend(std::iter::repeat_n((ri, 0), n[0] as usize));
            region_of.extend(std::iter::repeat_n((ri, 1), n[1] as usize));
            for g in river_groups(self.seed, r.cells[0], n[0]) {
                let key = self.schools.len() as u32 + 1;
                self.schools.extend(std::iter::repeat_n(key, g as usize));
            }
            self.schools.extend(std::iter::repeat_n(0, n[1] as usize));
            sides.push(side);
        }
        for i in 0..self.slots.len() {
            let old = self.slots[i];
            let mut s = old;
            if (i as u32) < self.active {
                // A leaving bird turns back where it is; a new flock flies in from off the map
                // (the opening world's birds start over it).
                if old.kind == SlotKind::Leaving {
                    s.kind = SlotKind::Bird;
                } else if old.kind != SlotKind::Bird {
                    s = Slot {
                        kind: SlotKind::Bird,
                        generation: old.generation + 1,
                        spawn: if first {
                            self.sky_point(i as u32)
                        } else {
                            self.arrive_point(i as u32)
                        },
                        roost: None,
                        assigned_at: 0.0,
                        region: None,
                    };
                }
            } else if (i as u32) < self.birds {
                match old.kind {
                    SlotKind::Bird if !first => {
                        s.kind = SlotKind::Leaving;
                        s.roost = None;
                        s.assigned_at = t;
                    }
                    SlotKind::Leaving | SlotKind::Idle => {}
                    SlotKind::Bird | SlotKind::Fish => {
                        s = Slot {
                            kind: SlotKind::Idle,
                            generation: old.generation + 1,
                            spawn: Vec3::ZERO,
                            roost: None,
                            assigned_at: 0.0,
                            region: None,
                        };
                    }
                }
            } else if (i as u32) >= self.birds + self.fish {
                if old.kind != SlotKind::Idle {
                    s = Slot {
                        kind: SlotKind::Idle,
                        generation: old.generation + 1,
                        spawn: Vec3::ZERO,
                        roost: None,
                        assigned_at: 0.0,
                        region: None,
                    };
                }
            } else {
                let f = i - self.birds as usize;
                let (ri, side) = region_of[f];
                let school = self.schools[f];
                // A river group's later fish spawn beside its first.
                let lead = (school as usize).checked_sub(1).filter(|&l| l < f);
                let spawn = match lead {
                    Some(l) => self
                        .water
                        .beside(self.slots[self.birds as usize + l].spawn, f - l),
                    None => self.water.spawn(ri, &sides[ri][side], self.seed, i as u32),
                };
                let keep = old.kind == SlotKind::Fish
                    && sides[ri][side].contains(&cell_at(old.spawn.xz()));
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
        self.roosts = roosts_with(tiles, catalog, drawn, &self.scatter);
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

    /// Sends free birds to every free perch of the group at `cell`: whole free flocks first (from
    /// the cursor on), so a flock roosts and launches together, then any free bird.
    fn assign(&mut self, cell: Axial, at: f32, perched: bool) {
        let perches: Vec<Roost> = self
            .roosts
            .iter()
            .filter(|r| r.cell == cell)
            .filter(|r| !self.slots.iter().any(|s| s.roost == Some((r.cell, r.slot))))
            .copied()
            .collect();
        let n = self.active as usize;
        if n == 0 || perches.is_empty() {
            return;
        }
        let free = |s: &Slot| s.kind == SlotKind::Bird && s.roost.is_none();
        let size = FLOCK_SIZE as usize;
        let flocks = n.div_ceil(size);
        let mut birds: Vec<usize> = vec![];
        for o in 0..flocks {
            if birds.len() >= perches.len() {
                break;
            }
            let f = (self.cursor / size + o) % flocks;
            let members = f * size..((f + 1) * size).min(n);
            if members.clone().all(|k| free(&self.slots[k])) {
                birds.extend(members);
            }
        }
        for o in 0..n {
            if birds.len() >= perches.len() {
                break;
            }
            let k = (self.cursor + o) % n;
            if free(&self.slots[k]) && !birds.contains(&k) {
                birds.push(k);
            }
        }
        birds.truncate(perches.len());
        if let Some(&last) = birds.last() {
            self.cursor = (last + 1) % n;
        }
        for (k, r) in birds.into_iter().zip(perches) {
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
        for s in &mut self.slots {
            if s.kind == SlotKind::Leaving && t - s.assigned_at >= LEAVE {
                *s = Slot {
                    kind: SlotKind::Idle,
                    generation: s.generation + 1,
                    spawn: Vec3::ZERO,
                    roost: None,
                    assigned_at: 0.0,
                    region: None,
                };
                self.slots_dirty = true;
            }
        }
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
            birds: self.active,
            bird_slots: self.birds,
            leaving: self
                .slots
                .iter()
                .filter(|s| s.kind == SlotKind::Leaving)
                .count() as u32,
            fish: self.fish,
            roosts: self.roosts.len() as u32,
            roosting,
            settling,
            water_tiles: self.water.cells.len() as u32,
            regions: {
                let mut first = 0;
                self.water
                    .regions
                    .iter()
                    .map(|r| {
                        let schools = &self.schools[first..first + r.fish as usize];
                        first += r.fish as usize;
                        let mut sizes: BTreeMap<u32, u32> = BTreeMap::new();
                        for &k in schools {
                            *sizes.entry(k).or_default() += 1;
                        }
                        let river = || sizes.iter().filter(|(k, _)| **k > 0).map(|(_, n)| *n);
                        RegionCount {
                            cell: r.cells[0],
                            tiles: r.cells.len() as u32,
                            fish: r.fish,
                            river: r.river,
                            largest_group: sizes.values().copied().max().unwrap_or(0),
                            singles: sizes.values().filter(|&&n| n == 1).count() as u32,
                            river_fish: river().sum(),
                            river_largest_group: river().max().unwrap_or(0),
                            river_singles: river().filter(|&n| n == 1).count() as u32,
                        }
                    })
                    .collect()
            },
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

    /// `world` plus `n` meadows in a row far off: land tiles that bring birds
    /// ([`LAND_PER_FLOCK`]).
    fn with_land(world: &[PlayTile], n: i32) -> Vec<PlayTile> {
        let mut v = world.to_vec();
        v.extend((0..n).map(|k| tile(40 + k, -40, "meadow", 0)));
        v
    }

    /// Owner, 2026-10-14: play does not fill every bird slot from the start; the bird count
    /// scales with the land, a flock per [`LAND_PER_FLOCK`] land tiles, none over the start
    /// tile, whole flocks within the tier's bird slots. New flocks fly in from off the map and
    /// removed flocks fly away before their slots idle.
    #[test]
    fn bird_count_scales_with_land_tiles() {
        let tiers = [
            (QualityTier::High, [0, 24, 144, 600, 744]),
            (QualityTier::Medium, [0, 24, 144, 372, 372]),
            (QualityTier::Low, [0, 24, 144, 180, 180]),
        ];
        for (tier, want) in tiers {
            let slots = BoidBudget::for_tier(tier).split(0).0;
            let got = [1, 10, 50, 200, 2_000].map(|n| flying_birds(n, slots));
            assert_eq!(got, want, "{tier:?}");
        }
        let c = catalog();
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 5);
        let start = [tile(0, 0, "meadow", 0)];
        plan.set_world_all(&start, &c, 0.0);
        assert_eq!(plan.counts(0.0).birds, 0, "the start tile has no birds");
        assert!(plan.slots.iter().all(|s| s.kind == SlotKind::Idle));
        // Nine more land tiles: two flocks fly in from past the sky's edge.
        let town = with_land(&start, 9);
        plan.set_world_all(&town, &c, 1.0);
        assert_eq!(plan.active, 24);
        let sky = Vec2::new(plan.sky.x, plan.sky.z);
        for s in &plan.slots[..24] {
            assert_eq!(s.kind, SlotKind::Bird);
            assert!(
                s.spawn.xz().distance(sky) > plan.sky.w + 0.5 * ARRIVE_OUT,
                "{s:?}"
            );
        }
        assert!(plan.slots[24..].iter().all(|s| s.kind != SlotKind::Bird));
        // Bulldozing back to five land tiles: the second flock leaves whole, in the air, and
        // its slots idle once it is off the map.
        let generations: Vec<u32> = plan.slots[12..24].iter().map(|s| s.generation).collect();
        plan.set_world_all(&with_land(&start, 4), &c, 2.0);
        let n = plan.counts(2.0);
        assert_eq!((n.birds, n.leaving), (12, 12));
        assert!(
            plan.slots[12..24]
                .iter()
                .all(|s| s.kind == SlotKind::Leaving)
        );
        let kept: Vec<u32> = plan.slots[12..24].iter().map(|s| s.generation).collect();
        assert_eq!(
            kept, generations,
            "leaving birds keep flying, not respawned"
        );
        plan.tick(2.0 + LEAVE - 0.1);
        assert_eq!(plan.counts(2.0).leaving, 12);
        plan.tick(2.0 + LEAVE);
        assert_eq!(plan.counts(2.0 + LEAVE).leaving, 0);
        assert!(plan.slots[12..].iter().all(|s| s.kind == SlotKind::Idle));
        // A flock that comes back while another is leaving turns the leaving one round.
        plan.set_world_all(&town, &c, 20.0);
        plan.set_world_all(&with_land(&start, 4), &c, 21.0);
        plan.set_world_all(&town, &c, 22.0);
        assert!(plan.slots[..24].iter().all(|s| s.kind == SlotKind::Bird));
    }

    impl BoidPlan {
        /// `set_world` with every tree drawn as a kit piece.
        fn set_world_all(&mut self, tiles: &[PlayTile], c: &TileCatalog, t: f32) -> bool {
            self.set_world(tiles, c, &|_| true, t)
        }
    }

    #[test]
    fn an_oak_tile_gains_its_roosts_when_its_pack_swaps_the_oak_in() {
        let c = catalog();
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 2);
        let world = with_land(&[tile(0, 0, "meadow", 0), tile(1, 0, "oak", 0)], 6);
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
    fn budget_follows_the_tier_agent_caps_and_fish_scale_with_water() {
        assert_eq!(BoidBudget::for_tier(QualityTier::High), BoidBudget::HIGH);
        assert_eq!(BoidBudget::for_tier(QualityTier::Ultra), BoidBudget::HIGH);
        let m = BoidBudget::for_tier(QualityTier::Medium);
        assert_eq!((m.total, m.fish_max, m.fish_per_water_tile), (500, 150, 12));
        let l = BoidBudget::for_tier(QualityTier::Low);
        assert_eq!((l.total, l.fish_max, l.fish_per_water_tile), (250, 75, 12));
        assert_eq!(l.split(100), (131, 75), "Low caps fish at 75");
        let b = BoidBudget::HIGH;
        // A13: birds are 75% of what they were (1,000 with no water, 808 in the boids world).
        assert_eq!(
            b.split(0),
            (750, 0),
            "no water: three quarters of the fish budget goes to birds"
        );
        assert_eq!(b.split(1), (741, 12));
        assert_eq!(b.split(10), (660, 120));
        assert_eq!(
            b.split(16),
            (606, 192),
            "the boids checkpoint's 16 water tiles"
        );
        assert_eq!(b.split(25), (525, 300), "the default split");
        assert_eq!(b.split(400), (525, 300), "fish cap at 300");
        for n in 0..60 {
            let (birds, fish) = b.split(n);
            assert_eq!(birds, (b.total - fish) * 3 / 4);
            assert!(birds + fish <= b.total);
        }
    }

    /// Birds spawn by flock: a flock's members start within its spread, and two flocks apart.
    #[test]
    fn flocks_spawn_together() {
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 9);
        plan.set_world_all(
            &with_land(&[tile(0, 0, "meadow", 0), tile(3, 0, "meadow", 0)], 400),
            &catalog(),
            0.0,
        );
        assert_eq!(plan.active, 744, "the High tier's whole flocks");
        let birds: Vec<Vec3> = plan.slots[..plan.active as usize]
            .iter()
            .map(|s| s.spawn)
            .collect();
        let size = FLOCK_SIZE as usize;
        let mut apart = 0;
        for (f, members) in birds.chunks(size).enumerate() {
            let c = members.iter().copied().sum::<Vec3>() / members.len() as f32;
            for p in members {
                assert!(p.xz().distance(c.xz()) <= 2.0 * FLOCK_SPREAD, "flock {f}");
                assert!((SKY_LOW..=SKY_HIGH).contains(&p.y), "flock {f} at {p}");
            }
            if let Some(next) = birds.get((f + 1) * size) {
                apart += usize::from(next.xz().distance(c.xz()) > 2.0 * FLOCK_SPREAD);
            }
        }
        assert!(
            apart > birds.len() / size / 2,
            "flocks spread over the sky: {apart}"
        );
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
        let oak = c.get("oak").unwrap();
        let rule = oak
            .agents
            .iter()
            .find(|a| a.agent == AgentKind::Bird)
            .unwrap();
        assert_eq!(rule.max, 12);
        assert_eq!(canopy_perches("tree_hero_oak").len(), 12);
        let tiles = [
            tile(0, 0, "meadow", 0),
            tile(1, 0, "oak", 2),
            tile(0, 1, "road_straight", 0),
        ];
        let r = roosts(&tiles, &c, &|_| true);
        assert_eq!(r.len(), 12, "the oak's 12 perches, within the rule's max");
        assert!(r.iter().all(|x| x.cell == Axial::new(1, 0)));
        let centre = Vec2::from(Axial::new(1, 0).to_world(HEX_RADIUS));
        for x in &r {
            assert!(
                x.pos.xz().distance(centre) < 8.0 * OAK_SCALE,
                "{x:?} over the oak"
            );
            assert!(
                x.pos.y > 5.0 * OAK_SCALE && x.pos.y < 12.0 * OAK_SCALE,
                "{x:?} in the canopy"
            );
        }
        // The roosts turn with the tile.
        let r0 = roosts(&[tile(1, 0, "oak", 0)], &c, &|_| true);
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

    /// The pine forest has no Bird rule since the split (U3), so it never roosts: a world without
    /// an oak keeps every bird in the air.
    #[test]
    fn pine_forests_have_no_roosts_and_birds_stay_airborne() {
        let c = catalog();
        let cell = Axial::new(1, 0);
        let trees: Vec<TreeInstance> = (0..20)
            .map(|k| TreeInstance {
                piece: "tree_forest".into(),
                at: Vec3::new(k as f32 - 10.0, 0.0, 4.0),
                yaw: Quat::IDENTITY,
                scale: 1.0,
            })
            .collect();
        let scatter = BTreeMap::from([(cell, trees)]);
        let world = [tile(0, 0, "meadow", 0), tile(1, 0, "forest", 0)];
        assert!(roosts_with(&world, &c, &|_| true, &scatter).is_empty());
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 4);
        plan.set_scatter(1, || scatter.clone());
        assert!(plan.set_world_all(&world, &c, 1.0));
        plan.tick(1.0 + APPROACH);
        let n = plan.counts(1.0 + APPROACH);
        assert_eq!((n.roosts, n.roosting, n.settling), (0, 0, 0));
        assert!(plan.slots.iter().all(|s| s.roost.is_none()));
    }

    /// The pre-split forest (a hero oak among the spruces, with a Bird rule), as a catalog of its
    /// own: scattered trees on a roosting tile still share its cap with the hero.
    fn mixed_forest_catalog() -> TileCatalog {
        let mut tiles: Vec<(&str, String)> = hex_core::BUILTIN_TILES
            .iter()
            .map(|(id, text)| (*id, (*text).to_owned()))
            .collect();
        let forest = tiles.iter_mut().find(|(id, _)| *id == "forest").unwrap();
        forest.1 = forest.1.replace(
            "(piece: \"rock_small\", density: 0.2),\n        ],",
            "(piece: \"rock_small\", density: 0.2),\n        ],\n        \
             hero: [(piece: \"tree_hero_oak\", at: (0.0, 0.0))],",
        );
        forest.1 = forest.1.replace(
            "    ),\n)",
            "    ),\n    agents: [(agent: Bird, max: 12)],\n)",
        );
        let refs: Vec<(&str, &str)> = tiles.iter().map(|(id, t)| (*id, t.as_str())).collect();
        let c = TileCatalog::from_ron(hex_core::SOCKETS_RON, &refs).unwrap();
        assert_eq!(c.get("forest").unwrap().props.hero.len(), 1);
        assert_eq!(c.get("forest").unwrap().agents.len(), 1);
        c
    }

    /// Scattered spruces share a roosting tile's bird cap with its oak: half each, spire tops
    /// first, scaled and turned with each spruce and the tile; with no oak drawn they take the
    /// whole cap.
    #[test]
    fn scattered_spruces_roost_within_the_bird_max() {
        let c = mixed_forest_catalog();
        let cell = Axial::new(1, 0);
        let spruce = |x: f32, z: f32, scale: f32| TreeInstance {
            piece: "tree_forest".into(),
            at: Vec3::new(x, -0.1, z),
            yaw: Quat::from_rotation_y(0.7),
            scale,
        };
        let trees: Vec<TreeInstance> = (0..20)
            .map(|k| {
                let a = k as f32 * 0.9;
                spruce(8.0 * a.cos(), 8.0 * a.sin(), 0.9 + 0.01 * k as f32)
            })
            .collect();
        let scatter = BTreeMap::from([(cell, trees.clone())]);
        let world = [tile(0, 0, "meadow", 0), tile(1, 0, "forest", 3)];
        let r = roosts_with(&world, &c, &|_| true, &scatter);
        assert_eq!(r.len(), 12, "the forest's bird max");
        let [cx, cz] = cell.to_world(HEX_RADIUS);
        let centre = Vec3::new(cx, 0.0, cz);
        let (oak, spires) = r.split_at(6);
        let oak_only = roosts(&world, &c, &|_| true);
        assert_eq!(oak, &oak_only[..6], "the oak keeps its first six perches");
        let rot = piece_rotation(3);
        for (k, x) in spires.iter().enumerate() {
            let t = &trees[k];
            let want = centre + rot * (t.at + Vec3::Y * SPRUCE_TOP * t.scale);
            assert!(x.pos.distance(want) < 1e-3, "spire {k}: {x:?} vs {want}");
        }
        let no_oak = roosts_with(&world, &c, &|p| p != "tree_hero_oak", &scatter);
        assert_eq!(no_oak.len(), 12);
        assert!(no_oak.iter().all(|x| x.pos.y > 10.0), "spire tops");
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 2);
        assert!(plan.set_world_all(&world, &c, 1.0));
        assert!(!plan.set_world_all(&world, &c, 1.1));
        plan.set_scatter(1, || scatter.clone());
        assert!(
            plan.set_world_all(&world, &c, 1.2),
            "new scatter rebuilds the roosts"
        );
        assert_eq!(plan.roosts.len(), 12);
        plan.set_scatter(1, || unreachable!("same revision"));
        assert!(!plan.set_world_all(&world, &c, 1.3));
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
        // Triangles of the canopy shell and the leaf cards.
        let mut tris: Vec<[Vec3; 3]> = vec![];
        for prim in mesh.primitives() {
            let shell = match prim.material().name() {
                Some("tree_hero_oak_canopy") => true,
                Some("tree_hero_oak_leaves") => false,
                _ => continue,
            };
            let rd = prim.reader(|b| Some(&buffers[b.index()]));
            let pos: Vec<Vec3> = rd.read_positions().unwrap().map(Vec3::from).collect();
            let idx: Vec<u32> = rd.read_indices().unwrap().into_u32().collect();
            tris.extend(
                idx.as_chunks::<3>()
                    .0
                    .iter()
                    .map(|t| [pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]]),
            );
            if shell {
                canopy.extend(pos);
            }
        }
        assert!(canopy.len() > 100, "canopy primitive found");
        assert!(
            OAK_PERCHES.iter().all(|p| p.1 >= PERCH_MIN_ELEVATION),
            "a perch mid-slope"
        );
        // The highest foliage surface over XZ point `q`: a vertical line against every triangle.
        let surface = |q: Vec2| -> f32 {
            let mut top = f32::MIN;
            for [a, b, c] in &tris {
                let (a2, b2, c2) = (a.xz(), b.xz(), c.xz());
                let d = (b2 - a2).perp_dot(c2 - a2);
                if d.abs() < 1e-9 {
                    continue;
                }
                let u = (q - a2).perp_dot(c2 - a2) / d;
                let v = (b2 - a2).perp_dot(q - a2) / d;
                if u >= 0.0 && v >= 0.0 && u + v <= 1.0 {
                    top = top.max(a.y + u * (b.y - a.y) + v * (c.y - a.y));
                }
            }
            top
        };
        // Owner feedback (2026-10-14): a perched bird sits on the foliage, not in it. Each perch
        // is at the highest canopy-or-leaf surface under the folded bird (0.35 m around it: the
        // body is 0.79 m long at display scale), so the pivot-to-feet lift puts it on the leaves.
        let mut off = vec![];
        for p in canopy_perches("tree_hero_oak") {
            let top = (0..8)
                .map(|k| Vec2::from_angle(k as f32 * std::f32::consts::FRAC_PI_4) * 0.35)
                .chain([Vec2::ZERO])
                .map(|o| surface(p.xz() + o))
                .fold(f32::MIN, f32::max);
            if p.y < top - 0.02 || p.y > top + 0.08 {
                off.push((p, p.y - top));
            }
        }
        assert!(off.is_empty(), "perches off the foliage top: {off:?}");
        for p in canopy_perches("tree_hero_oak") {
            let near = canopy
                .iter()
                .map(|v| v.distance(p))
                .fold(f32::MAX, f32::min);
            // Shell vertices are spaced at the tree's scale, so the nearest one is too.
            assert!(
                near < 0.5 * OAK_SCALE,
                "perch {p} is {near} m from the canopy shell"
            );
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
    fn placing_and_bulldozing_an_oak_adds_and_removes_roosts() {
        let c = catalog();
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 7);
        let meadows = with_land(&[tile(0, 0, "meadow", 0)], 8);
        assert!(plan.set_world_all(&meadows, &c, 0.0));
        assert!(plan.roosts.is_empty());
        assert!(!plan.set_world_all(&meadows, &c, 0.1), "unchanged world");
        // Place: 12 roosts, birds fly in and are roosting after the approach.
        let world = with_land(&[tile(0, 0, "meadow", 0), tile(1, 0, "oak", 0)], 8);
        assert!(plan.set_world_all(&world, &c, 1.0));
        assert_eq!(plan.roosts.len(), 12);
        plan.tick(1.0);
        let n = plan.counts(1.0);
        assert_eq!((n.roosting, n.settling), (0, 12));
        let flocks: BTreeSet<usize> = plan
            .slots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.roost.is_some())
            .map(|(k, _)| k / FLOCK_SIZE as usize)
            .collect();
        assert_eq!(flocks.len(), 1, "one whole flock takes the oak: {flocks:?}");
        let n = plan.counts(1.0 + APPROACH);
        assert_eq!((n.roosting, n.settling), (12, 0));
        // The wave launches them, and the next cycle takes birds again.
        plan.tick(1.0 + OCCUPIED + 0.1);
        assert_eq!(plan.counts(1.0 + OCCUPIED + 0.1).roosting, 0);
        plan.tick(1.0 + CYCLE + 0.1);
        assert_eq!(plan.counts(1.0 + CYCLE + 0.1).settling, 12);
        // Bulldoze: the roosts go and every bird on them is released.
        assert!(plan.set_world_all(&meadows, &c, 50.0));
        assert!(plan.roosts.is_empty());
        assert!(plan.slots.iter().all(|s| s.roost.is_none()));
        // Undo the bulldoze: back again, a fresh cycle.
        assert!(plan.set_world_all(&world, &c, 60.0));
        plan.tick(60.0);
        assert_eq!(plan.counts(60.0).settling, 12);
    }

    #[test]
    fn a_world_opening_with_an_oak_opens_with_perched_birds() {
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 3);
        // Before play's first rebuild no tile is past its rise.
        plan.set_world_all(&[], &catalog(), 0.0);
        assert!(plan.roosts.is_empty());
        plan.set_world_all(&with_land(&[tile(0, 0, "oak", 0)], 7), &catalog(), 0.5);
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
        // The play plan: 7 water tiles, 84 fish, 687 bird slots (75% of the 916 slots fish leave),
        // of which the one land tile flies none, and the last 229 slots idle.
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 1);
        plan.set_world_all(&water_world(), &c, 0.0);
        assert_eq!((plan.birds, plan.active, plan.fish), (687, 0, 84));
        let n = plan.counts(0.0);
        assert_eq!(n.regions.iter().map(|r| r.fish).sum::<u32>(), 84);
        assert!(plan.slots[..687].iter().all(|s| s.kind == SlotKind::Idle));
        assert!(
            plan.slots[687..771]
                .iter()
                .all(|s| s.kind == SlotKind::Fish)
        );
        assert!(plan.slots[771..].iter().all(|s| s.kind == SlotKind::Idle));
        // Drying the world up gives the fish slots back to the birds' share and idles the rest.
        plan.set_world_all(&with_land(&[tile(0, 0, "meadow", 0)], 7), &c, 1.0);
        assert_eq!((plan.birds, plan.active, plan.fish), (750, 24, 0));
        assert!(plan.slots[..24].iter().all(|s| s.kind == SlotKind::Bird));
        assert!(plan.slots[24..].iter().all(|s| s.kind == SlotKind::Idle));
    }

    /// River fish swim alone or in groups of 2 to 5, most of them alone, each group spawning
    /// together; sea fish keep one school per region (owner, 2026-10-14).
    #[test]
    fn river_fish_swim_alone_or_in_small_groups() {
        let c = catalog();
        for seed in 0..32u64 {
            let sizes = river_groups(seed, Axial::new(-4, 0), 48);
            assert_eq!(sizes.iter().sum::<u32>(), 48);
            assert!(
                sizes.iter().all(|&n| (1..=RIVER_GROUP_MAX).contains(&n)),
                "{sizes:?}"
            );
            let alone = sizes.iter().filter(|&&n| n == 1).count();
            assert!(alone * 2 > 48, "seed {seed}: {alone} of 48 alone");
            assert!(
                sizes.iter().any(|&n| n > 1),
                "seed {seed}: some small groups"
            );
            assert_eq!(sizes, river_groups(seed, Axial::new(-4, 0), 48));
        }
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 1);
        plan.set_world_all(&water_world(), &c, 0.0);
        let counts = plan.counts(0.0);
        let river = counts.regions.iter().find(|r| r.river).unwrap();
        let sea = counts.regions.iter().find(|r| !r.river).unwrap();
        assert!(river.largest_group <= RIVER_GROUP_MAX && river.singles * 2 > river.fish);
        assert_eq!(sea.largest_group, sea.fish, "the sea keeps one school");
        assert_eq!(sea.singles, 0);
        let b = plan.birds as usize;
        for (f, &k) in plan.schools.iter().enumerate() {
            if k > 0 && (k as usize - 1) < f {
                let lead = plan.slots[b + k as usize - 1].spawn;
                assert!(
                    plan.slots[b + f].spawn.distance(lead) < 1.0,
                    "fish {f} beside its group"
                );
            }
        }
    }

    /// Owner, 2026-10-14: a river joined to the sea through a mouth is one region (fish may swim
    /// across), but grouping follows each fish's spawn water. Fish spawned in river and spring
    /// water swim alone or in groups of 2 to 5 and take the river palette; fish spawned in the
    /// mouth, coast and ocean water school as one and take the ocean palette.
    #[test]
    fn grouping_follows_the_spawn_water_in_a_joined_region() {
        let c = catalog();
        let o = Axial::new(0, 0);
        let (spring, mouth) = (o.neighbor(3), o.neighbor(0));
        let sea = mouth.neighbor(0);
        let world = [
            tile(spring.q, spring.r, "river_spring", 0),
            tile(o.q, o.r, "river_straight", 0),
            tile(mouth.q, mouth.r, "river_mouth", 0),
            tile(sea.q, sea.r, "ocean", 0),
            tile(sea.neighbor(1).q, sea.neighbor(1).r, "ocean", 0),
            tile(sea.neighbor(5).q, sea.neighbor(5).r, "ocean", 0),
        ];
        for seed in 1..9u64 {
            let mut plan = BoidPlan::play(BoidBudget::HIGH, seed);
            plan.set_world_all(&world, &c, 0.0);
            assert_eq!(plan.water.regions.len(), 1, "one joined region");
            let counts = plan.counts(0.0);
            let r = &counts.regions[0];
            assert!(!r.river, "the region holds sea water");
            assert_eq!(r.fish, 72);
            assert_eq!(r.river_fish, 24, "two river tiles of six: {r:?}");
            assert!(
                (2..=RIVER_GROUP_MAX).contains(&r.river_largest_group),
                "{r:?}"
            );
            assert!(r.river_singles * 2 > r.river_fish, "{r:?}");
            assert_eq!(r.largest_group, 48, "the sea fish school as one: {r:?}");
            let b = plan.birds as usize;
            for (f, &k) in plan.schools.iter().enumerate() {
                let p = plan.slots[b + f].spawn;
                let river = plan.water.river_at(p.xz());
                assert_eq!(
                    river,
                    k > 0,
                    "seed {seed} fish {f} (school {k}) spawned at {p}"
                );
                let palette = crate::boids::fish_palette(&plan.water.cells, p);
                assert_eq!(palette > 0.0, river, "seed {seed} fish {f}: palette");
                let band = band_at(&plan.water.cells, p.xz());
                assert!(
                    band[1] - band[0] >= SPAWN_ROOM,
                    "fish {f} spawned in skim water"
                );
            }
        }
        // A spring alone is river water too: its fish swim in small groups with the river palette.
        let mut plan = BoidPlan::play(BoidBudget::HIGH, 3);
        plan.set_world_all(&world[..2], &c, 0.0);
        let r = &plan.counts(0.0).regions[0];
        assert!(r.river && r.river_fish == r.fish && r.river_largest_group <= RIVER_GROUP_MAX);
        assert_eq!(crate::boids::river_palettes(&plan), vec![r.fish]);
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
                let p = w.spawn(ri, &r.cells, 1, k + 100 * ri as u32);
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
