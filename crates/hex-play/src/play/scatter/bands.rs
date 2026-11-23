//! Where each kind of dressing may stand: floors, bands along a coast, a bank or a verge, and the
//! seeded counts that keep a beach, a river bank or a water surface sparse.

use bevy::prelude::*;
use hex_core::{Endpoint, LEVEL_STEP, NetworkKind, TileDef};

use super::super::APOTHEM;
use crate::water::tiles::{BEACH_TOP_S, Coastline};

use super::layout::{Instance, Layout, Rng, hex_reach, ring16};
use super::surface::Surface;
use super::tables::family;

/// The slots a scatter piece may stand on (the topmost LOD0 surface under its whole footprint):
/// its family's `data/pieces.ron` `floor_slots`. A piece without any is never placed.
pub fn floor_slots(piece: &str) -> &'static [&'static str] {
    hex_core::piece::builtin_floor_slots(family(piece))
}

/// The massif's Meadow-matched grass cap: the meadow pieces stand on it as on `meadow_base_cap`.
pub const MASSIF_CAP: &str = "massif_granite_cap";

/// The single-tile peak's Meadow-matched grass cap (spec 2026-11-13 D9), likewise.
pub const PEAK_CAP: &str = "peak_granite_cap";

/// The massif's lower slopes (m from its centre): the grass between the rock foot and the ring's
/// outer half, where its pines stand in small drifts (owner A54 Q2 O1, a runtime rule, never baked;
/// `docs/checkpoints/massif_granite/integration.md`).
pub const PINE_BAND: (f32, f32) = (17.0, 28.0);

/// The peak's lower slopes (m from the tile's centre): the grass between its rock foot and scree
/// fans (about 9 m out) and the rim, where its pines stand in small drifts (spec 2026-11-13 D9, a
/// runtime rule, never baked; docs/checkpoints/peak_granite/integration.md).
pub const PEAK_PINE_BAND: (f32, f32) = (8.5, 12.5);

/// The band of distances from a footprint tile's centre (a single tile's own) that a rule's
/// instance centres keep to: [`PINE_BAND`] for a footprint tile's pines, [`PEAK_PINE_BAND`] for the
/// peak's, `None` for every other rule and tile.
pub fn ring_band(def: &TileDef, piece: &str) -> Option<(f32, f32)> {
    if !matches!(family(piece), "tree_forest" | "sapling_spruce") {
        None
    } else if def.is_mega() {
        Some(PINE_BAND)
    } else {
        (def.id == "peak_granite").then_some(PEAK_PINE_BAND)
    }
}

/// Farthest a verge piece's centre stands from the nearest road or river centreline (m): it keeps
/// [`lane_clearance`](super::layout::lane_clearance) from every lane like any rule, and stays within this of one, so it lines the
/// verge beyond the kerb (road half 3 m, kerb to 3.25 m, verge sink to 4.6 m) instead of spreading
/// over the cap. `None` for every other piece.
pub fn verge_band(piece: &str) -> Option<f32> {
    match family(piece) {
        "bush_verge" => Some(VERGE_BAND),
        _ => None,
    }
}

/// [`verge_band`]'s outer limit for the verge bushes: 3 m half width + [`LANE_GAP`](super::layout::LANE_GAP) + the bush's
/// 0.52 m ground reach puts the nearest centre at 4.52 m; the band runs about 2 m beyond it.
pub const VERGE_BAND: f32 = 6.7;

/// Slots no scatter may stand on whatever [`floor_slots`] says: roads, river beds and banks,
/// house pads and driveways, beaches and the coast, bridges, every base's rim, mossy edge band
/// and skirt, and the massif's rock and scree (exact names: a `_rock` suffix would catch other
/// pieces' slots), and the single-tile hill's and peak's (spec 2026-11-13 D9). They stay ground for
/// the walker, so they are not [`is_not_ground`](super::tables::is_not_ground).
pub fn excluded_slot(slot: &str) -> bool {
    const EXACT: [&str; 6] = [
        "massif_granite_rock",
        "massif_granite_scree",
        "hill_meadow_rock",
        "hill_meadow_scree",
        "peak_granite_rock",
        "peak_granite_scree",
    ];
    const PREFIXES: [&str; 6] = [
        "road_",
        "river_",
        "house_pad_",
        "beach_",
        "coast_",
        "bridge_",
    ];
    const SUFFIXES: [&str; 3] = ["_rim", "_edge", "_skirt"];
    EXACT.contains(&slot)
        || PREFIXES.iter().any(|p| slot.starts_with(p))
        || SUFFIXES.iter().any(|s| slot.ends_with(s))
}

// ---------------------------------------------------------------------------------------------
// Beach dressing (owner D3, 2026-10-20): driftwood and marram clumps on the dry sand

/// The one [`excluded_slot`] a piece may stand on when its [`floor_slots`] list it: the dry sand
/// of the beach and river-mouth bases. The grass strip, damp band, seabed and every other
/// `beach_` slot stay excluded.
pub const BEACH_FLOOR: &str = "beach_sand";

/// Top of the damp band in s (m seaward of the coastline): `_beach.WET_TOP_S`, where the bed
/// stands 0.6 m above the water.
pub const DAMP_TOP_S: f32 = -1.953;

/// Beach dressing keeps its whole ground footprint this far (m) inside the hex border.
pub const BEACH_BORDER: f32 = 1.5;

/// Largest turn (rad) of a driftwood piece off the coast's direction (12°): the sand is level
/// along the coast and slopes across it (up to about 0.24 on the dry band), so a log lying across
/// the slope would float at one end.
pub const DRIFT_YAW_JITTER: f32 = 0.21;

/// Where on a coast tile a beach dressing piece stands, in s ([`Coastline::seaward`]) of its centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeachBand {
    pub near: f32,
    pub far: f32,
    /// Seaward thinning: a dart at `near` is always kept, one at `far` with probability 1 - thin.
    pub thin: f32,
    /// Laid along the coast within [`DRIFT_YAW_JITTER`] (driftwood) instead of at a free yaw.
    pub along: bool,
}

/// The band of a beach dressing piece (owner BD-Q2, BD-Q4). Marram clumps: from about one
/// clump's width (1.2 m) seaward of the grass line (s = -8) to 2 m short of the damp band, densest
/// at the grass and thinning seaward. Driftwood: the seaward dry sand, about 3 m short of the grass
/// line to the damp band (its 1.45 m footprint keeps it off the damp band). Pebble clusters (owner
/// 2026-10-26, the pebbles the beach centres baked at the grass line): the dry sand within 2 m of
/// the grass line, densest at it, where the bed is nearly level (slope under 0.09), so a cluster
/// stood at its lowest ring point (`layout::stand`) floats none of its pebbles. `None` for every other
/// piece.
pub fn beach_band(piece: &str) -> Option<BeachBand> {
    match family(piece) {
        "beach_pebbles" => Some(BeachBand {
            near: BEACH_TOP_S + 0.5,
            far: BEACH_TOP_S + 2.0,
            thin: 0.6,
            along: false,
        }),
        "beach_dunegrass" => Some(BeachBand {
            near: BEACH_TOP_S + 1.2,
            far: DAMP_TOP_S - 2.0,
            thin: 0.75,
            along: false,
        }),
        "beach_driftwood" => Some(BeachBand {
            near: BEACH_TOP_S + 3.0,
            far: DAMP_TOP_S - 0.5,
            thin: 0.0,
            along: true,
        }),
        // Granite boulders and the outcrop (owner A25, ref-4): the grass-to-sand band, centres
        // from 3 m landward of the grass line to 1.5 m onto the sand, thinning seaward. Only on a
        // coast tile ([`coast_band`]); inland they scatter freely.
        "boulder_granite" | "outcrop_granite" => Some(BeachBand {
            near: BEACH_TOP_S - 3.0,
            far: BEACH_TOP_S + 1.5,
            thin: 0.4,
            along: false,
        }),
        _ => None,
    }
}

/// `piece`'s [`beach_band`] on `def`. A piece that also stands inland (one of its
/// [`floor_slots`] is an ordinary floor: the granite boulders) keeps its band only on a tile with a
/// coastline and scatters freely elsewhere; beach-only dressing keeps its band everywhere, so it
/// never stands off a coast.
pub fn coast_band(def: &TileDef, piece: &str) -> Option<BeachBand> {
    let band = beach_band(piece)?;
    let inland = floor_slots(piece).iter().any(|s| !excluded_slot(s));
    (!inland || Coastline::of(def, 0).is_some()).then_some(band)
}

/// The [`excluded_slot`]s a piece may stand on when its [`floor_slots`] list them: the dry sand of
/// the beach and river-mouth bases, and the ocean floor.
pub const FLOOR_EXEMPT: [&str; 2] = [BEACH_FLOOR, SEABED_FLOOR];

/// Whether `slot`, under a footprint, is ground for a piece standing on `floors`: one of them and
/// never an [`excluded_slot`] outside [`FLOOR_EXEMPT`] and the [`BANK_FLOORS`] (only the bank
/// props list those).
pub fn floor_ok(floors: &[&str], slot: &str) -> bool {
    floors.contains(&slot)
        && (FLOOR_EXEMPT.contains(&slot) || BANK_FLOORS.contains(&slot) || !excluded_slot(slot))
}

// ---------------------------------------------------------------------------------------------
// Seabed dressing (owner 2026-10-26): rocks and pebble clusters on the ocean floor

/// The ocean base's floor, the one slot the seabed dressing stands on.
pub const SEABED_FLOOR: &str = "coast_seabed";

/// Where a seabed piece stands: the ocean bed's contract level, under the floor's relief (at most
/// 0.06 m above it, `_coast.RELIEF_MAX`), so its top, authored at most 0.085 m above its origin
/// (`_stones.TOP_MAX_SEABED`), stays within [`SEABED_ALLOWANCE`] at the largest scale jitter.
pub const SEABED_Y: f32 = -LEVEL_STEP;

/// How far the ocean floor may rise above the bed (m, water contract §4): more brings the shallow
/// water's damping and swash back.
pub const SEABED_ALLOWANCE: f32 = 0.10;

/// A seabed piece keeps its whole ground footprint this far (m) inside the hex border: the floor's
/// relief-free band (water contract §4), where its neighbours' beds meet it flat.
pub const SEABED_BORDER: f32 = 2.0;

/// Whether `piece` is seabed dressing: it stands on the ocean floor alone.
pub fn seabed(piece: &str) -> bool {
    floor_slots(piece) == [SEABED_FLOOR]
}

// ---------------------------------------------------------------------------------------------
// Riverbank props (owner, 2026-10-26: "scatter type items like rocks and reeds should not be built
// into the tile"): bank stone clusters and reed clumps, a seeded 0 to [`RIVER_MAX`] per river tile

/// The [`excluded_slot`]s the bank props stand on: the river bases' bed (the bank slope down to
/// the waterline) and bank-top grass, and the river mouths' grass. Only the bank props list them.
pub const BANK_FLOORS: [&str; 3] = ["river_bed", "river_grass", "beach_grass"];

/// Where a bank prop's centre stands: `(low, high)`, the tile-frame height of the river bank
/// under it, which is `_river.profile_z` of the distance from the centreline, so it follows a bend
/// or a fork exactly where a centreline polyline would cut the arc. The bands are the ones
/// `_river_dress` placed them in: stone clusters 3.6-4.6 m out, straddling the waterline (4 m,
/// the water at rest at -1.4), reed clumps 4.7-5.8 m out on the bank above it (the bank top is
/// 6.83 m out, at 0). Within [`BANK_REACH`] of a river centreline, and instead of the river lanes'
/// `lane_clearance` (a road or rail lane on the same tile keeps its clearance). `None` for every
/// other piece.
pub fn bank_band(piece: &str) -> Option<(f32, f32)> {
    match family(piece) {
        "river_stones" => Some((-1.797, -0.912)),
        "river_reeds" => Some((-0.856, -0.262)),
        _ => None,
    }
}

/// Farthest a bank prop's centre stands from a river centreline polyline (m): the bank top plus
/// the bend polyline's chord sag (about 0.8 m), so only the channel's own banks qualify.
pub const BANK_REACH: f32 = 8.0;

/// Whether a beach dressing centre at `p` (reach `reach`) lies outside its band: off the band's
/// s range, within [`BEACH_BORDER`] of the border, or on a tile with no coastline.
pub(super) fn off_beach_band(
    band: BeachBand,
    coast: Option<Coastline>,
    p: Vec2,
    reach: f32,
) -> bool {
    let Some(coast) = coast else {
        return true;
    };
    let s = coast.seaward(p);
    s < band.near || s > band.far || hex_reach(p) > APOTHEM - reach - BEACH_BORDER
}

/// The chance a beach dressing dart at `p` is kept ([`BeachBand::thin`]).
pub(super) fn beach_keep(band: BeachBand, coast: Coastline, p: Vec2) -> f32 {
    let t = ((coast.seaward(p) - band.near) / (band.far - band.near)).clamp(0.0, 1.0);
    1.0 - band.thin * t
}

/// Most dressing pieces one coast tile draws: "the beaches have too much going on, every piece does
/// not need to have all scatter items, 0 to 6 is plenty" (owner, 2026-10-21).
pub const BEACH_MAX: usize = 6;

/// Whether a tile is dressed as a beach: it scatters beach dressing ([`beach_band`]), and nothing
/// but beach dressing and bank props ([`bank_band`]: the river mouths' banks).
pub fn beach_family(def: &TileDef) -> bool {
    let rules = &def.props.scatter;
    rules
        .iter()
        .any(|r| coast_band(def, r.piece.as_str()).is_some())
        && rules.iter().all(|r| {
            coast_band(def, r.piece.as_str()).is_some() || bank_band(r.piece.as_str()).is_some()
        })
}

/// How many dressing pieces a coast tile with seed `seed` keeps: 0 to [`BEACH_MAX`], evenly.
pub fn beach_count(seed: u64) -> usize {
    Rng(seed ^ 0xB3AC_4C0D_7E11_5A29).below(BEACH_MAX + 1)
}

/// Most bank props one river tile draws (owner, 2026-10-26: "riverbank scatter is too concentrated
/// and repetitive, change scatter to 0-4 things").
pub const RIVER_MAX: usize = 4;

/// Whether a tile is dressed as a riverbank: it scatters bank props ([`bank_band`]), and nothing
/// else but water props ([`water_prop`], capped on their own by `keep_water`).
pub fn river_family(def: &TileDef) -> bool {
    let rules = &def.props.scatter;
    rules.iter().any(|r| bank_band(r.piece.as_str()).is_some())
        && rules
            .iter()
            .all(|r| bank_band(r.piece.as_str()).is_some() || water_prop(r.piece.as_str()))
}

/// How many bank props a river tile with seed `seed` keeps: 0 to [`RIVER_MAX`], evenly.
pub fn river_count(seed: u64) -> usize {
    Rng(seed ^ 0x51D3_8E0A_C4B7_2F69).below(RIVER_MAX + 1)
}

/// The seeded cap on a tile's scatter: [`beach_count`] on a [`beach_family`] tile, [`river_count`]
/// on a [`river_family`] tile, none on every other tile.
pub fn dressing_cap(def: &TileDef, seed: u64) -> Option<usize> {
    if beach_family(def) {
        Some(beach_count(seed))
    } else if river_family(def) {
        Some(river_count(seed))
    } else {
        None
    }
}

/// Keeps `cap` of a capped tile's placed pieces ([`dressing_cap`]), picked jointly across its rules
/// from its own stream, so a kept piece stands exactly where it stood among all of them, and plans
/// what it keeps (a shortfall still means only that the spacing could not fit a piece).
/// Water props ([`water_prop`]) are not dressing: [`keep_water`] caps them on a stream of their own.
pub(super) fn keep_dressing(out: &mut Layout, seed: u64, cap: usize, water: &[bool]) {
    keep_among(
        out,
        Rng(seed ^ 0x6E3D_19B7_A2C5_F041),
        cap,
        |i: &Instance| !water[i.rule as usize],
    );
}

/// Keeps `cap` of the instances `among` selects, picked from `rng`, and every other instance.
fn keep_among(out: &mut Layout, mut rng: Rng, cap: usize, among: impl Fn(&Instance) -> bool) {
    let idx: Vec<usize> = (0..out.instances.len())
        .filter(|&i| among(&out.instances[i]))
        .collect();
    let n = idx.len();
    let k = cap.min(n);
    let mut pick: Vec<usize> = (0..n).collect();
    for i in 0..k {
        let j = i + rng.below(n - i);
        pick.swap(i, j);
    }
    let mut keep: Vec<bool> = out.instances.iter().map(|i| !among(i)).collect();
    for &i in &pick[..k] {
        keep[idx[i]] = true;
    }
    let mut kept = keep.into_iter();
    out.instances.retain(|_| kept.next().unwrap_or(false));
    out.planned = (0..out.planned.len())
        .map(|ri| out.placed(ri) as u32)
        .collect();
}

// ---------------------------------------------------------------------------------------------
// Water props (owner A26, 2026-10-27): lily pad clusters floating on the river and spring water,
// and reed beds standing in it, a seeded 0 to [`WATER_MAX`] per tile of their own, so the bank
// props' 0 to [`RIVER_MAX`] keep exactly the draw and the places they had.

/// Whether `piece` is a water prop: a lily cluster ([`LILY_Y`], over the bed's open water) or a
/// reed bed ([`REED_BED_BAND`]). Only the spring and the 120° bend list them.
pub fn water_prop(piece: &str) -> bool {
    matches!(family(piece), "lily_cluster" | "reed_bed")
}

pub(super) fn lily(piece: &str) -> bool {
    family(piece) == "lily_cluster"
}

/// The slot a water prop stands over: the river and spring bases' bed, an [`excluded_slot`] that
/// [`floor_ok`] passes as one of the [`BANK_FLOORS`]. The whole footprint must have it topmost, so
/// a prop keeps clear of the spring centre's own lily pads and granite, which lie over it.
pub const WATER_FLOOR: &str = "river_bed";

/// The water at rest (water contract §1).
pub const WATER_REST: f32 = -1.4;

/// Where a lily cluster's pads lie (its pivot): 1 cm over the water at rest, as
/// `river_spring_center`'s own pads, where the river weight is 1 and the waves and swash are still.
/// Every footprint point's bed lies under [`WATER_REST`]: on a river that is 4.0 m from the
/// centreline less the cluster's radius (`_river.profile_z`).
pub const LILY_Y: f32 = WATER_REST + 0.01;

/// Bed height band of a reed bed's centre: 0.4 to 0.8 m of water, 3.0 to 3.6 m from a river
/// centreline on `_river.profile_z`. Its pivot stands at [`WATER_REST`], its +x along the bank's
/// contour and its -z (Blender +y) uphill, so its stems reach the bank under every one of them.
pub const REED_BED_BAND: (f32, f32) = (-2.23, -1.80);

/// A water prop keeps its whole footprint, at its largest scale, this far inside the hex border.
pub const WATER_BORDER: f32 = 1.5;

/// On a spring tile ([`spring_nodes`]) a reed bed's centre and a lily cluster's whole footprint stay
/// within this of the spring node: the pool's water surface reaches 6.5 m from it (§3b).
pub const SPRING_REACH: f32 = 6.5;

/// Most water props one tile keeps (owner A26: "their own seeded 0-2").
pub const WATER_MAX: usize = 2;

/// How many water props a tile with seed `seed` keeps: 0 to [`WATER_MAX`], evenly.
pub fn water_count(seed: u64) -> usize {
    Rng(seed ^ 0x3C9A_E1F7_0B52_D84B).below(WATER_MAX + 1)
}

/// Keeps [`water_count`] of a tile's water props (`water` per rule), picked on a stream of their
/// own; every other instance stays.
pub(super) fn keep_water(out: &mut Layout, seed: u64, water: &[bool]) {
    keep_among(
        out,
        Rng(seed ^ 0x8F14_6B2D_C7A9_3E05),
        water_count(seed),
        |i: &Instance| water[i.rule as usize],
    );
}

/// The spring nodes of a tile: the river network nodes of a tile with no coastline (water
/// contract §3b), in the tile frame the scatter works in.
pub fn spring_nodes(def: &TileDef) -> Vec<Vec2> {
    if Coastline::of(def, 0).is_some() {
        return vec![];
    }
    let node = |e: &Endpoint| match e {
        Endpoint::Node(id) => def
            .nodes
            .iter()
            .find(|n| n.id == *id)
            .map(|n| Vec2::from(n.at)),
        Endpoint::Edge(_) => None,
    };
    def.networks
        .iter()
        .filter(|n| n.kind == NetworkKind::River)
        .flat_map(|n| [node(&n.from), node(&n.to)])
        .flatten()
        .collect()
}

/// Whether a water prop with its centre at `p` over the bed height `h` and a ground reach `reach`
/// (at its largest scale) stands out of place, `slop` metres allowed for a dart off its cell centre:
/// within [`WATER_BORDER`] of the border, beyond [`SPRING_REACH`] of a spring node, a lily
/// cluster's centre over no water, or a reed bed's off its [`REED_BED_BAND`].
pub(super) fn off_water(
    lily: bool,
    springs: &[Vec2],
    p: Vec2,
    reach: f32,
    h: f32,
    slop: f32,
) -> bool {
    let (low, high) = REED_BED_BAND;
    hex_reach(p) + reach + slop > APOTHEM - WATER_BORDER
        || springs
            .iter()
            .any(|s| p.distance(*s) + slop + if lily { reach } else { 0.0 } > SPRING_REACH)
        || if lily {
            h >= WATER_REST
        } else {
            h + slop < low || h - slop > high
        }
}

/// The height a water prop at `p` stands at ([`LILY_Y`] or [`WATER_REST`]), checked exactly at
/// the dart: [`off_water`], and a lily cluster's every rim and half-rim point over the
/// [`WATER_FLOOR`] under the water at rest. `h` is the bed under the centre.
pub(super) fn water_stand(
    surface: &Surface,
    piece: &str,
    springs: &[Vec2],
    p: Vec2,
    reach: f32,
    h: f32,
) -> Option<f32> {
    let lily = lily(piece);
    if off_water(lily, springs, p, reach, h, 0.0) {
        return None;
    }
    if !lily {
        return Some(WATER_REST);
    }
    let wet = |q: Vec2| {
        surface
            .at(q.x, q.y)
            .is_some_and(|(y, s)| s == WATER_FLOOR && y < WATER_REST)
    };
    let ring = ring16();
    (ring.iter().all(|d| wet(p + reach * *d)) && ring.iter().all(|d| wet(p + 0.5 * reach * *d)))
        .then_some(LILY_Y)
}
