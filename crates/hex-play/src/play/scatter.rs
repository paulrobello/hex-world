//! Seeded prop scatter in play (PRD §5.3, §6): the pine forest's spruces and rocks, the oak tile's
//! rocks and flowers on the litter, and the meadow's wildflowers and rocks, from each tile's
//! `props.scatter` rules.
//!
//! - **Seed.** A tile scatters from `hex_core::cell_seed(world seed, cell)`, which is exactly its
//!   `PlacedTile::seed`. [`layout`] is a pure function of the definition, that seed and the tile's
//!   ground, so a world always looks the same, and undo, redo, a reload or a late pack redraw a tile
//!   as it was.
//! - **Density.** `ScatterRule::density` is the mean count per 100 m² of the hex cap ([`CAP_AREA`],
//!   665 m²), so the forest's 3.0 is 20 trees a tile. The count is rounded stochastically from the
//!   seed, which keeps its mean over cells exact. Exclusions move instances onto the ground that is
//!   left and do not thin the count; where blue-noise spacing cannot fit it, the report's `placed`
//!   falls short of `planned`.
//! - **Drifts.** A rule with `drift` (the meadow's wildflowers, A18) grows only on the [`DRIFT_SHARE`]
//!   of its ground where the tile's seeded value noise ([`drift_noise`]) is highest, so its
//!   instances gather in a few clustered drifts. The noise is tile-local and seeded from the tile
//!   alone, so a tile needs no neighbour to scatter and redraws identically.
//! - **Variants.** A rule names a piece; each instance draws one piece of its group ([`VARIANTS`]:
//!   the piece and its `_b`, `_c`, … variants, PRD §6 Variety), picked by [`pick_variant`] from a
//!   hash of the tile seed, the rule and the instance's place in it, so the dart stream is
//!   untouched and the same seed draws the same mix. A group draws its pieces equally unless
//!   [`VARIANT_WEIGHTS`] weighs them (the verge bushes favour `bush_verge`), or on a road or rail
//!   tile [`VERGE_VARIANT_WEIGHTS`] (more poppies on the verges). The rule's ground
//!   mask and spacing keep the group's largest footprint clear ([`group_footprint`]); each
//!   instance stands and clears others by its own. A rule draws once every piece of its group is in the kit.
//! - **Vary.** A rule with `vary` (the meadow's single wildflowers) spreads evenly but keeps a dart
//!   with probability 1 - vary × (1 - [`vary_noise`]), so its density varies gently over the cap
//!   while its count stays exact.
//! - **Beaches.** A tile that scatters only beach dressing ([`beach_family`]: beach, beach_inner,
//!   beach_outer and both river mouths) keeps a seeded 0 to [`BEACH_MAX`] of the pieces its rules
//!   place ([`beach_count`], owner 2026-10-21), picked jointly across its rules from a stream of
//!   its own, so a kept piece stands where it would among all of them and the dart stream is
//!   untouched. Its `planned` counts what it keeps.
//! - **Riverbanks.** Bank stone clusters and reed clumps ([`bank_band`]) stand in a band at a set
//!   distance from the river centreline, on the river bases' bed and bank grass ([`BANK_FLOORS`]).
//!   A tile that scatters only bank props ([`river_family`]: the river channel tiles) keeps a
//!   seeded 0 to [`RIVER_MAX`] of them ([`river_count`], owner 2026-10-26) the way a beach keeps
//!   its count; the river mouths keep theirs under the beach cap.
//! - **Seabed.** The ocean's rocks and pebble clusters ([`seabed`], owner 2026-10-26) stand on its
//!   floor ([`SEABED_FLOOR`]) at the bed's contract level ([`SEABED_Y`], under the floor's relief)
//!   and keep [`SEABED_BORDER`] inside the border, so the floor stays within its
//!   [`SEABED_ALLOWANCE`] (water contract §4).
//! - **Ground.** An instance stands only on its piece's floor slots ([`floor_slots`]) of the topmost
//!   LOD0 surface of the tile's base, centre and lane edge pieces, sampled the way the kit gate's
//!   `surface_at` does: trees on `forest_base_floor` (the litter), rocks and wildflowers on either
//!   that and `meadow_base_cap`. Its whole ground footprint must lie on that floor, so nothing
//!   stands on a road, a river bed or bank, a house pad or driveway, a beach, a rim, a skirt or water
//!   ([`excluded_slot`]), except the beach dressing on the dry sand ([`BEACH_FLOOR`],
//!   [`beach_band`]), the bank props on a river's bank ([`BANK_FLOORS`], [`bank_band`]) and the
//!   seabed dressing on the ocean floor ([`FLOOR_EXEMPT`]). It keeps clear of hero props (a tree
//!   clears the oak's canopy; a fence, [`FENCES`], by its line, dropped after the darts so nothing
//!   else moves), of road and
//!   river centrelines (`lane_clearance`, at least the lane's half width plus [`LANE_GAP`]) and of
//!   the hex border (a crown overhangs it by at most [`CROWN_OVERHANG`]), and stands upright at the
//!   surface height.
//! - **Spacing.** Dart throwing over the valid ground (Poisson-disc, never a grid): instances of one
//!   piece keep [`BLUE_NOISE`] of their mean spacing apart (trees at least [`CROWN_SPACING`] of a
//!   crown), and different pieces never share a footprint. Each gets a random yaw and a small scale
//!   jitter, both carried by its transform.
//! - **Drawing.** Instances are children of the tile root, which carries the tile's rotation, built
//!   from the kit's own parts (`spawn_kit_parts`): shared meshes and kit materials, so Bevy draws each
//!   (piece, LOD, slot) as one instanced batch per pass within the kit's LOD ranges, and no sampled
//!   texture is added. They grow in the rise's props stage in a ripple from the centre outward, and
//!   sink with the tile.
//! - **Streaming.** A tile scatters once its ground pieces are in the kit (the forest pack brings
//!   `forest_base`), and each rule draws once its piece is (the props pack brings rocks and
//!   wildflowers, the forest pack spruces). Until then the tile shows its placeholder or bare cap.
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    f32::consts::TAU,
    sync::Arc,
};

use bevy::{
    gltf::{Gltf, GltfMesh},
    prelude::*,
};
use hex_core::{
    Axial, Endpoint, HEX_RADIUS, LEVEL_STEP, NetworkKind, ScatterRule, SocketKind, TileCatalog,
    TileDef, cell_seed,
};

use super::{
    APOTHEM, KitPart, PlayCatalog, PlayKit, PlaySpawned, PlayView,
    packs::PlayPacks,
    rise::{PropKind, RiseRole},
    spawn_kit_parts,
};
use crate::water::tiles::{BEACH_TOP_S, Coastline};
use crate::{ActiveScenario, Scenario, SceneEntity};

/// Area of the hex cap in m² (pointy-top, circumradius [`HEX_RADIUS`]): what density counts per.
pub const CAP_AREA: f32 = 1.5 * 1.732_050_8 * HEX_RADIUS * HEX_RADIUS;
/// Instances of one piece keep this fraction of their mean spacing (sqrt(area / count)) apart.
pub const BLUE_NOISE: f32 = 0.6;
/// Trees of one piece keep at least this fraction of their crown radius apart.
pub const CROWN_SPACING: f32 = 0.7;
/// A crown may hang this far (m) over the hex border, into the neighbour.
pub const CROWN_OVERHANG: f32 = 1.0;
/// A tree keeps this fraction of a hero tree's crown radius, plus its own trunk, from the hero.
pub const HERO_CROWN: f32 = 0.85;
/// Gap (m) between footprints of a hero and an instance.
pub const HERO_GAP: f32 = 0.25;
/// Gap (m) between a lane's edge and an instance's footprint.
pub const LANE_GAP: f32 = 1.0;
/// Gap (m) between a footprint and the hex border.
pub const BORDER_GAP: f32 = 0.3;
/// Share of a drift rule's ground (its highest-noise cells) its instances grow on.
pub const DRIFT_SHARE: f32 = 0.25;
/// Same-piece instances in a drift keep this many ground reaches apart (2 when spread evenly), so
/// their clumps may overlap a little.
pub const DRIFT_PACK: f32 = 1.4;
/// Lattice spacing (m) of the drift noise's coarse octave; the fine octave is half of it.
pub const DRIFT_CELL: f32 = 7.0;
/// Dart throws per wanted instance before a rule gives up.
const ATTEMPTS: u32 = 60;
/// Raster step (m) of the ground the dart thrower samples, and its half extent (m).
const RASTER: f32 = 0.25;
const EXTENT: f32 = 16.5;
const CELLS: usize = 132;
/// Bucket size (m) of the surface's triangle grid.
const BUCKET: f32 = 1.0;
const BUCKETS: usize = 33;

/// The slots a scatter piece may stand on (the topmost LOD0 surface under its whole footprint).
/// A piece not listed has none, so it is never placed.
pub fn floor_slots(piece: &str) -> &'static [&'static str] {
    match family(piece) {
        "tree_forest" | "tree_oak" | "fern_clump" | "log_fallen" | "shrub_forest"
        | "sapling_spruce" => &["forest_base_floor"],
        "rock_small" | "wildflower_clump" => &["forest_base_floor", "meadow_base_cap"],
        // The granite boulders and outcrop (owner A25): the meadow, the forest and oak litter, and
        // on a coast the grass-to-sand band ([`coast_band`]).
        "boulder_granite" | "outcrop_granite" => &[
            "forest_base_floor",
            "meadow_base_cap",
            "beach_grass",
            BEACH_FLOOR,
        ],
        "wildflower_single" | "bush_verge" => &["meadow_base_cap"],
        "beach_dunegrass" | "beach_driftwood" | "beach_pebbles" => &[BEACH_FLOOR],
        "river_stones" | "river_reeds" => &BANK_FLOORS,
        "lily_cluster" => &[WATER_FLOOR],
        "reed_bed" => &[WATER_FLOOR, "river_grass"],
        "seabed_rock" | "seabed_pebbles" => &[SEABED_FLOOR],
        _ => &[],
    }
}

/// Farthest a verge piece's centre stands from the nearest road or river centreline (m): it keeps
/// [`lane_clearance`] from every lane like any rule, and stays within this of one, so it lines the
/// verge beyond the kerb (road half 3 m, kerb to 3.25 m, verge sink to 4.6 m) instead of spreading
/// over the cap. `None` for every other piece.
pub fn verge_band(piece: &str) -> Option<f32> {
    match family(piece) {
        "bush_verge" => Some(VERGE_BAND),
        _ => None,
    }
}

/// [`verge_band`]'s outer limit for the verge bushes: 3 m half width + [`LANE_GAP`] + the bush's
/// 0.52 m ground reach puts the nearest centre at 4.52 m; the band runs about 2 m beyond it.
pub const VERGE_BAND: f32 = 6.7;

/// Variants of a scatter piece (PRD §6 Variety), after the piece itself: a rule names the first
/// and each of its instances draws one of the group, picked by [`pick_variant`].
pub const VARIANTS: &[(&str, &[&str])] = &[
    (
        "beach_driftwood",
        &["beach_driftwood_b", "beach_driftwood_c"],
    ),
    (
        "beach_dunegrass",
        &["beach_dunegrass_b", "beach_dunegrass_c"],
    ),
    ("beach_pebbles", &["beach_pebbles_b", "beach_pebbles_c"]),
    ("tree_forest", &["tree_forest_b", "tree_forest_c"]),
    ("tree_oak", &["tree_oak_b", "tree_oak_c"]),
    ("fern_clump", &["fern_clump_b", "fern_clump_c"]),
    ("log_fallen", &["log_fallen_b", "log_fallen_c"]),
    ("rock_small", &["rock_small_b", "rock_small_c"]),
    (
        "boulder_granite",
        &[
            "boulder_granite_b",
            "boulder_granite_c",
            "boulder_granite_d",
        ],
    ),
    (
        "wildflower_clump",
        &["wildflower_clump_b", "wildflower_clump_c"],
    ),
    (
        "wildflower_single",
        &[
            "wildflower_single_b",
            "wildflower_single_c",
            "wildflower_single_d",
            "wildflower_single_e",
        ],
    ),
    ("bush_verge", &["bush_verge_b", "bush_verge_c"]),
    ("river_stones", &["river_stones_b", "river_stones_c"]),
    ("river_reeds", &["river_reeds_b"]),
    ("lily_cluster", &["lily_cluster_b", "lily_cluster_c"]),
    ("shrub_forest", &["shrub_forest_b", "shrub_forest_c"]),
    ("sapling_spruce", &["sapling_spruce_b", "sapling_spruce_c"]),
];

/// Pick weights of a [`VARIANTS`] group, the piece first then its variants in order. A group not
/// listed draws each piece equally. `bush_verge` stays the most common verge bush (owner,
/// 2026-10-20): half the picks, a quarter each for `_b` and `_c`. The scarlet poppy
/// `wildflower_single_e` is an accent among the single wildflowers, 1 pick in 10 (about 4 of a
/// meadow's 40), because its red is the most saturated colour in the meadow.
pub const VARIANT_WEIGHTS: &[(&str, &[u32])] = &[
    ("bush_verge", &[2, 1, 1]),
    ("wildflower_single", &[9, 9, 9, 9, 4]),
    // Owner A25: the two small boulders most, the big rounded one rarest.
    ("boulder_granite", &[3, 3, 2, 1]),
];

/// Pick weights that replace [`VARIANT_WEIGHTS`] on a verge tile ([`verge_tile`]): the poppy is 1
/// pick in 4 of a road or rail verge's single wildflowers (owner A9-Q4, 2026-10-20: more poppies
/// on the verges, the meadow kept at 1 in 10). Every other tile and group keeps its weights, so
/// it draws exactly as before.
pub const VERGE_VARIANT_WEIGHTS: &[(&str, &[u32])] = &[("wildflower_single", &[3, 3, 3, 3, 4])];

/// Whether `def` is a verge tile: it carries a road or rail lane, so its cap is the verge along it.
pub fn verge_tile(def: &TileDef) -> bool {
    def.networks
        .iter()
        .any(|n| matches!(n.kind, NetworkKind::Road | NetworkKind::Rail))
}

/// The pick weights of `piece`'s group on `def`: [`VERGE_VARIANT_WEIGHTS`] on a verge tile where it
/// lists the group, else [`VARIANT_WEIGHTS`]; `None` draws each piece equally.
pub fn variant_weights(def: &TileDef, piece: &str) -> Option<&'static [u32]> {
    let find = |table: &'static [(&str, &[u32])]| {
        table.iter().find(|(id, _)| *id == piece).map(|(_, w)| *w)
    };
    verge_tile(def)
        .then(|| find(VERGE_VARIANT_WEIGHTS))
        .flatten()
        .or_else(|| find(VARIANT_WEIGHTS))
}

fn extra_variants(piece: &str) -> &'static [&'static str] {
    VARIANTS
        .iter()
        .find(|(id, _)| *id == piece)
        .map_or(&[], |(_, v)| *v)
}

/// The piece a rule naming `piece` draws as variant `v` (0 is `piece` itself).
pub fn variant_piece(piece: &str, v: u8) -> &str {
    match v {
        0 => piece,
        _ => extra_variants(piece)
            .get(usize::from(v) - 1)
            .copied()
            .unwrap_or(piece),
    }
}

/// `piece` and its variants.
pub fn group(piece: &str) -> impl Iterator<Item = &str> {
    std::iter::once(piece).chain(extra_variants(piece).iter().copied())
}

/// The piece a variant belongs to (itself when it is not a variant).
pub fn family(piece: &str) -> &str {
    VARIANTS
        .iter()
        .find(|(_, v)| v.contains(&piece))
        .map_or(piece, |(id, _)| *id)
}

/// The largest ground and crown reach over `piece`'s group: what a rule's ground mask and spacing
/// keep clear, so any variant fits wherever the rule may place one.
pub fn group_footprint(piece: &str) -> Footprint {
    group(piece).map(footprint).fold(
        Footprint {
            ground: 0.0,
            crown: 0.0,
        },
        |a, f| Footprint {
            ground: a.ground.max(f.ground),
            crown: a.crown.max(f.crown),
        },
    )
}

/// The variant of the `n`-th instance placed by rule `rule` (naming `piece`) on a tile `def` with
/// seed `seed`, among `piece`'s group: a hash of its own, so picking it never moves the placement
/// stream's darts, and the same seed always draws the same mix. A weighted group
/// ([`variant_weights`]) draws by its weights on that tile from the same hash; any other group
/// draws each piece equally.
pub fn pick_variant(def: &TileDef, piece: &str, seed: u64, rule: usize, n: u32) -> u8 {
    let count = group(piece).count();
    if count <= 1 {
        return 0;
    }
    let key = ((rule as u64) << 32) | u64::from(n);
    let mut r = Rng(seed ^ 0x7A21_C3E9_55B0_4D1F ^ key.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let Some(weights) = variant_weights(def, piece) else {
        return r.below(count) as u8;
    };
    let total: u64 = weights.iter().map(|&w| u64::from(w)).sum();
    let mut x = r.next() % total;
    for (v, &w) in weights.iter().enumerate() {
        if x < u64::from(w) {
            return v as u8;
        }
        x -= u64::from(w);
    }
    unreachable!("x < total")
}

/// Slots no scatter may stand on whatever [`floor_slots`] says: roads, river beds and banks,
/// house pads and driveways, beaches and the coast, bridges, and every base's rim, mossy edge band
/// and skirt.
pub fn excluded_slot(slot: &str) -> bool {
    const PREFIXES: [&str; 6] = [
        "road_",
        "river_",
        "house_pad_",
        "beach_",
        "coast_",
        "bridge_",
    ];
    const SUFFIXES: [&str; 3] = ["_rim", "_edge", "_skirt"];
    PREFIXES.iter().any(|p| slot.starts_with(p)) || SUFFIXES.iter().any(|s| slot.ends_with(s))
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
/// stood at its lowest ring point ([`stand`]) floats none of its pebbles. `None` for every other
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
fn off_beach_band(band: BeachBand, coast: Option<Coastline>, p: Vec2, reach: f32) -> bool {
    let Some(coast) = coast else {
        return true;
    };
    let s = coast.seaward(p);
    s < band.near || s > band.far || hex_reach(p) > APOTHEM - reach - BEACH_BORDER
}

/// The chance a beach dressing dart at `p` is kept ([`BeachBand::thin`]).
fn beach_keep(band: BeachBand, coast: Coastline, p: Vec2) -> f32 {
    let t = ((coast.seaward(p) - band.near) / (band.far - band.near)).clamp(0.0, 1.0);
    1.0 - band.thin * t
}

/// An instance's yaw from its yaw draw `u` in [0, 1): free (`u` × TAU), or for a piece laid along
/// the coast the coast's direction at `p`, either way round (u < 0.5 or not) and turned by up to
/// [`DRIFT_YAW_JITTER`] (from the same draw, so the dart stream is unchanged).
fn instance_yaw(band: Option<BeachBand>, coast: Option<Coastline>, p: Vec2, u: f32) -> f32 {
    let (Some(band), Some(coast)) = (band, coast) else {
        return u * TAU;
    };
    if !band.along {
        return u * TAU;
    }
    let t = match coast {
        Coastline::Straight { sea } => sea.perp(),
        Coastline::Arc { centre, .. } => (p - centre).perp(),
    };
    along_yaw(t, u)
}

/// A yaw from draw `u` in [0, 1) laying the piece's +x along `t`, either way round, within
/// [`DRIFT_YAW_JITTER`].
fn along_yaw(t: Vec2, u: f32) -> f32 {
    let flip = if u < 0.5 { 0.0 } else { std::f32::consts::PI };
    let jitter = ((u * 2.0).fract() * 2.0 - 1.0) * DRIFT_YAW_JITTER;
    // Quat::from_rotation_y(yaw) turns the piece's +x to (cos yaw, -sin yaw) in (x, z).
    portable_atan2(-t.y, t.x) + flip + jitter
}

/// The bank's contour direction at `p`: across the surface's slope (central differences 0.5 m
/// apart), or `None` where the ground is level or missing.
fn contour(surface: &Surface, p: Vec2) -> Option<Vec2> {
    let h = |dx: f32, dz: f32| surface.at(p.x + dx, p.y + dz).map(|(y, _)| y);
    let g = Vec2::new(h(0.5, 0.0)? - h(-0.5, 0.0)?, h(0.0, 0.5)? - h(0.0, -0.5)?);
    (g.length_squared() > 1e-6).then(|| g.perp())
}

/// `y.atan2(x)` from `+ - * /` alone (a degree-11 odd polynomial on [0, 1], max error
/// about 2.4e-5 rad), so native and wasm lay a beach out bit for bit: libm's `atan2` may differ by an
/// ulp between targets, which changes the layout fingerprint.
fn portable_atan2(y: f32, x: f32) -> f32 {
    use std::f32::consts::{FRAC_PI_2, PI};
    let (ax, ay) = (x.abs(), y.abs());
    if ax == 0.0 && ay == 0.0 {
        return 0.0;
    }
    let a = ax.min(ay) / ax.max(ay);
    let s = a * a;
    let mut r = ((((-0.011_721_2 * s + 0.052_653_3) * s - 0.116_432_2) * s + 0.193_543_5) * s
        - 0.332_623_9)
        * s
        * a
        + a;
    if ay > ax {
        r = FRAC_PI_2 - r;
    }
    if x < 0.0 {
        r = PI - r;
    }
    if y < 0.0 { -r } else { r }
}

/// Horizontal reach of a piece in metres: `ground` is its collider's (what it stands on and what
/// others must not overlap) and `crown` its LOD0 canopy's (0 for low props).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Footprint {
    pub ground: f32,
    pub crown: f32,
}

/// The pieces scatter places or keeps clear of (`footprints_match_the_kit` checks them against
/// the GLBs).
pub const FOOTPRINTS: &[(&str, Footprint)] = &[
    (
        "beach_driftwood",
        Footprint {
            ground: 1.45,
            crown: 0.0,
        },
    ),
    (
        "beach_driftwood_b",
        Footprint {
            ground: 1.03,
            crown: 0.0,
        },
    ),
    (
        "beach_driftwood_c",
        Footprint {
            ground: 0.66,
            crown: 0.0,
        },
    ),
    (
        "beach_dunegrass",
        Footprint {
            ground: 0.56,
            crown: 0.0,
        },
    ),
    (
        "beach_dunegrass_b",
        Footprint {
            ground: 0.43,
            crown: 0.0,
        },
    ),
    (
        "beach_dunegrass_c",
        Footprint {
            ground: 0.71,
            crown: 0.0,
        },
    ),
    // Stone clusters: their LOD0 convex hull collider's reach (`_stones.py`).
    (
        "beach_pebbles",
        Footprint {
            ground: 0.49,
            crown: 0.0,
        },
    ),
    (
        "beach_pebbles_b",
        Footprint {
            ground: 0.5,
            crown: 0.0,
        },
    ),
    (
        "beach_pebbles_c",
        Footprint {
            ground: 0.55,
            crown: 0.0,
        },
    ),
    (
        "seabed_rock",
        Footprint {
            ground: 0.43,
            crown: 0.0,
        },
    ),
    (
        "seabed_pebbles",
        Footprint {
            ground: 0.54,
            crown: 0.0,
        },
    ),
    (
        "river_stones",
        Footprint {
            ground: 1.05,
            crown: 0.0,
        },
    ),
    (
        "river_stones_b",
        Footprint {
            ground: 1.15,
            crown: 0.0,
        },
    ),
    (
        "river_stones_c",
        Footprint {
            ground: 1.30,
            crown: 0.0,
        },
    ),
    (
        "river_reeds",
        Footprint {
            ground: 0.45,
            crown: 0.0,
        },
    ),
    (
        "river_reeds_b",
        Footprint {
            ground: 0.83,
            crown: 0.0,
        },
    ),
    (
        "lily_cluster",
        Footprint {
            ground: 0.9,
            crown: 0.0,
        },
    ),
    (
        "lily_cluster_b",
        Footprint {
            ground: 1.4,
            crown: 0.0,
        },
    ),
    (
        "lily_cluster_c",
        Footprint {
            ground: 1.2,
            crown: 0.0,
        },
    ),
    (
        "reed_bed",
        Footprint {
            ground: 1.85,
            crown: 0.0,
        },
    ),
    (
        "tree_forest",
        Footprint {
            ground: 0.51,
            crown: 4.1,
        },
    ),
    (
        "tree_forest_b",
        Footprint {
            ground: 0.6,
            crown: 3.2,
        },
    ),
    (
        "tree_forest_c",
        Footprint {
            ground: 0.27,
            crown: 2.85,
        },
    ),
    (
        "rock_small",
        Footprint {
            ground: 0.62,
            crown: 0.0,
        },
    ),
    (
        "rock_small_b",
        Footprint {
            ground: 1.03,
            crown: 0.0,
        },
    ),
    (
        "rock_small_c",
        Footprint {
            ground: 1.07,
            crown: 0.0,
        },
    ),
    (
        "boulder_granite",
        Footprint {
            ground: 1.15,
            crown: 0.0,
        },
    ),
    (
        "boulder_granite_b",
        Footprint {
            ground: 1.36,
            crown: 0.0,
        },
    ),
    (
        "boulder_granite_c",
        Footprint {
            ground: 1.8,
            crown: 0.0,
        },
    ),
    (
        "boulder_granite_d",
        Footprint {
            ground: 2.31,
            crown: 0.0,
        },
    ),
    (
        "outcrop_granite",
        Footprint {
            ground: 4.25,
            crown: 0.0,
        },
    ),
    (
        "wildflower_clump",
        Footprint {
            ground: 0.9,
            crown: 0.0,
        },
    ),
    (
        "wildflower_clump_b",
        Footprint {
            ground: 0.96,
            crown: 0.0,
        },
    ),
    (
        "wildflower_clump_c",
        Footprint {
            ground: 0.96,
            crown: 0.0,
        },
    ),
    (
        "wildflower_single",
        Footprint {
            ground: 0.25,
            crown: 0.0,
        },
    ),
    (
        "wildflower_single_b",
        Footprint {
            ground: 0.25,
            crown: 0.0,
        },
    ),
    (
        "wildflower_single_c",
        Footprint {
            ground: 0.25,
            crown: 0.0,
        },
    ),
    (
        "wildflower_single_d",
        Footprint {
            ground: 0.25,
            crown: 0.0,
        },
    ),
    (
        "wildflower_single_e",
        Footprint {
            ground: 0.25,
            crown: 0.0,
        },
    ),
    (
        "tree_hero_oak",
        Footprint {
            ground: 2.5,
            crown: 8.55,
        },
    ),
    (
        "tree_hero_oak_b",
        Footprint {
            ground: 2.45,
            crown: 8.45,
        },
    ),
    (
        "tree_hero_oak_c",
        Footprint {
            ground: 2.45,
            crown: 8.25,
        },
    ),
    (
        "house_cottage",
        Footprint {
            ground: 4.3,
            crown: 0.0,
        },
    ),
    (
        "house_twostorey",
        Footprint {
            ground: 4.75,
            crown: 0.0,
        },
    ),
    (
        "house_longhouse",
        Footprint {
            ground: 4.21,
            crown: 0.0,
        },
    ),
    (
        "house_twostorey_b",
        Footprint {
            ground: 4.66,
            crown: 0.0,
        },
    ),
    (
        "house_twostorey_c",
        Footprint {
            ground: 4.64,
            crown: 0.0,
        },
    ),
    (
        "house_cottage_b",
        Footprint {
            ground: 4.21,
            crown: 0.0,
        },
    ),
    // The wells: their LOD2 hull collider's reach, the flagstone apron's rim (1.385 m), so grass
    // and scatter keep off the whole apron (`_house_well.py`; the walker stops at the wall instead,
    // `walk::DECLARED_PARTS`).
    (
        "well_stone",
        Footprint {
            ground: 1.39,
            crown: 0.0,
        },
    ),
    (
        "well_stone_b",
        Footprint {
            ground: 1.39,
            crown: 0.0,
        },
    ),
    // The tower trunk only: the sails turn overhead, from 3.7 m up.
    (
        "windmill",
        Footprint {
            ground: 2.9,
            crown: 0.0,
        },
    ),
    // The smock mill: its octagonal brick base's foot corners (the stage and sails are overhead,
    // from 2.66 m and 4.48 m up).
    (
        "windmill_b",
        Footprint {
            ground: 2.85,
            crown: 0.0,
        },
    ),
    // The farm tile's heroes: the barn (10 x 7 m, so its reach is its corner) and the static
    // bales and animals.
    (
        "barn_red",
        Footprint {
            ground: 6.6,
            crown: 0.0,
        },
    ),
    // The catslide barn: the same 10.6 x 7.7 m collider box as barn_red.
    (
        "barn_red_b",
        Footprint {
            ground: 6.6,
            crown: 0.0,
        },
    ),
    (
        "hay_bale_round",
        Footprint {
            ground: 0.9,
            crown: 0.0,
        },
    ),
    (
        "hay_bale_stack",
        Footprint {
            ground: 0.9,
            crown: 0.0,
        },
    ),
    // The upright bale's reach is its loose straw's outer edge, which the grass must leave clear.
    (
        "hay_bale_round_b",
        Footprint {
            ground: 0.9,
            crown: 0.0,
        },
    ),
    (
        "hay_bale_stack_b",
        Footprint {
            ground: 0.7,
            crown: 0.0,
        },
    ),
    (
        "animal_cow",
        Footprint {
            ground: 1.5,
            crown: 0.0,
        },
    ),
    (
        "animal_sheep",
        Footprint {
            ground: 0.8,
            crown: 0.0,
        },
    ),
    (
        "animal_cow_b",
        Footprint {
            ground: 1.4,
            crown: 0.0,
        },
    ),
    // animal_cow's body in another coat; its LOD2 head (the collider box) reaches 1.4 cm further.
    (
        "animal_cow_c",
        Footprint {
            ground: 1.5,
            crown: 0.0,
        },
    ),
    (
        "animal_sheep_b",
        Footprint {
            ground: 0.5,
            crown: 0.0,
        },
    ),
    (
        "animal_sheep_c",
        Footprint {
            ground: 0.9,
            crown: 0.0,
        },
    ),
    (
        "fern_clump",
        Footprint {
            ground: 0.3,
            crown: 0.0,
        },
    ),
    (
        "log_fallen",
        Footprint {
            ground: 1.9,
            crown: 0.0,
        },
    ),
    (
        "fern_clump_b",
        Footprint {
            ground: 0.2,
            crown: 0.0,
        },
    ),
    (
        "fern_clump_c",
        Footprint {
            ground: 0.3,
            crown: 0.0,
        },
    ),
    (
        "log_fallen_b",
        Footprint {
            ground: 1.31,
            crown: 0.0,
        },
    ),
    (
        "log_fallen_c",
        Footprint {
            ground: 1.72,
            crown: 0.0,
        },
    ),
    (
        "tree_oak",
        Footprint {
            ground: 0.59,
            crown: 4.86,
        },
    ),
    (
        "tree_oak_b",
        Footprint {
            ground: 0.35,
            crown: 3.35,
        },
    ),
    (
        "tree_oak_c",
        Footprint {
            ground: 0.58,
            crown: 4.81,
        },
    ),
    (
        "shrub_forest",
        Footprint {
            ground: 0.71,
            crown: 0.0,
        },
    ),
    (
        "shrub_forest_b",
        Footprint {
            ground: 0.56,
            crown: 0.0,
        },
    ),
    (
        "shrub_forest_c",
        Footprint {
            ground: 0.70,
            crown: 0.0,
        },
    ),
    (
        "sapling_spruce",
        Footprint {
            ground: 0.1,
            crown: 0.9,
        },
    ),
    (
        "sapling_spruce_b",
        Footprint {
            ground: 0.1,
            crown: 1.1,
        },
    ),
    (
        "sapling_spruce_c",
        Footprint {
            ground: 0.1,
            crown: 1.0,
        },
    ),
    (
        "bush_verge",
        Footprint {
            ground: 0.52,
            crown: 0.0,
        },
    ),
    (
        "bush_verge_b",
        Footprint {
            ground: 0.3,
            crown: 0.0,
        },
    ),
    (
        "bush_verge_c",
        Footprint {
            ground: 0.32,
            crown: 0.0,
        },
    ),
];

/// A piece missing from [`FOOTPRINTS`].
const DEFAULT_FOOTPRINT: Footprint = Footprint {
    ground: 1.0,
    crown: 0.0,
};

pub fn footprint(piece: &str) -> Footprint {
    FOOTPRINTS
        .iter()
        .find(|(id, _)| *id == piece)
        .map_or(DEFAULT_FOOTPRINT, |(_, f)| *f)
}

/// A rectangular piece's collider bounds in its own frame (x, z), before its yaw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundBox {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

/// The buildings whose collider is a rectangle (`ground_boxes_match_the_kit` checks them against
/// the GLBs). Every other piece, the windmill's round tower included, is a disc of its
/// [`Footprint::ground`].
pub const GROUND_BOXES: &[(&str, GroundBox)] = &[
    (
        "house_cottage",
        GroundBox {
            min: [-2.5, -3.85],
            max: [2.5, 3.45],
        },
    ),
    (
        "house_twostorey",
        GroundBox {
            min: [-2.8, -3.8],
            max: [2.8, 4.25],
        },
    ),
    (
        "house_longhouse",
        GroundBox {
            min: [-2.42, -3.35],
            max: [2.42, 3.44],
        },
    ),
    (
        "house_twostorey_b",
        GroundBox {
            min: [-2.75, -3.75],
            max: [2.75, 3.75],
        },
    ),
    (
        "house_twostorey_c",
        GroundBox {
            min: [-2.78, -3.7],
            max: [2.7, 3.4],
        },
    ),
    (
        "house_cottage_b",
        GroundBox {
            min: [-2.5, -3.38],
            max: [2.5, 3.38],
        },
    ),
    (
        "barn_red",
        GroundBox {
            min: [-5.3, -3.85],
            max: [5.3, 3.85],
        },
    ),
    (
        "barn_red_b",
        GroundBox {
            min: [-5.3, -3.85],
            max: [5.3, 3.85],
        },
    ),
    (
        "bakery_shop",
        GroundBox {
            min: [-2.8, -4.0],
            max: [3.15, 4.0],
        },
    ),
    (
        "bakery_shop_b",
        GroundBox {
            min: [-2.8, -3.8],
            max: [3.115, 3.8],
        },
    ),
    // The station platform's LOD2 box collider, ramps included (friction.md item 6): grass keeps
    // clear of its whole footprint, the ramp toes too.
    (
        "rail_station_platform",
        GroundBox {
            min: [-13.3, -6.8],
            max: [13.3, -1.66],
        },
    ),
];

pub fn ground_box(piece: &str) -> Option<GroundBox> {
    GROUND_BOXES
        .iter()
        .find(|(id, _)| *id == piece)
        .map(|(_, b)| *b)
}

/// A fence's layout in its own frame (x, z), from its `bake-report.json` `layout` block: the
/// fence line (runs between consecutive points; the gap between the last and first point is the
/// gate opening) and every post's centre and half width. A fence is a line, not a blob: it has no
/// [`FOOTPRINTS`] entry (its collider's reach, 14.3 m, would clear the whole yard), and scatter,
/// grass and walk's fallback treat it by these lines and posts instead.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fence {
    pub line: &'static [[f32; 2]],
    pub posts: &'static [([f32; 2], f32)],
}

/// Half width (m) of a fence's runs for clearance: the rails' 0.12 m collider half width plus a
/// lean and bend margin.
pub const FENCE_HALF: f32 = 0.2;

/// The fence pieces (`fences_match_the_kit` checks them against the GLBs: one collider part per
/// run, one LOD0 post per entry).
pub const FENCES: &[(&str, Fence)] = &[(
    "fence_paddock",
    Fence {
        line: &[
            [3.5, 2.5],
            [-12.256, 2.5],
            [-12.256, 7.076],
            [0.0, 14.152],
            [12.256, 7.076],
            [12.256, 2.5],
            [6.5, 2.5],
        ],
        posts: &[
            ([3.5, 2.5], 0.08),
            ([1.249, 2.5], 0.065),
            ([-1.002, 2.5], 0.065),
            ([-3.253, 2.5], 0.065),
            ([-5.504, 2.5], 0.065),
            ([-7.755, 2.5], 0.065),
            ([-10.005, 2.5], 0.065),
            ([-12.256, 2.5], 0.065),
            ([-12.256, 4.788], 0.065),
            ([-12.256, 7.076], 0.065),
            ([-10.214, 8.256], 0.065),
            ([-8.171, 9.435], 0.065),
            ([-6.128, 10.614], 0.065),
            ([-4.085, 11.794], 0.065),
            ([-2.043, 12.973], 0.065),
            ([0.0, 14.152], 0.065),
            ([2.043, 12.973], 0.065),
            ([4.085, 11.794], 0.065),
            ([6.128, 10.614], 0.065),
            ([8.171, 9.435], 0.065),
            ([10.214, 8.256], 0.065),
            ([12.256, 7.076], 0.065),
            ([12.256, 4.788], 0.065),
            ([12.256, 2.5], 0.065),
            ([10.338, 2.5], 0.065),
            ([8.419, 2.5], 0.065),
            ([6.5, 2.5], 0.08),
        ],
    },
)];

pub fn fence(piece: &str) -> Option<Fence> {
    FENCES.iter().find(|(id, _)| *id == piece).map(|(_, f)| *f)
}

/// The tile-local fence lines of `def`'s heroes: each hero fence's line turned by its yaw (as walk
/// and play place it, `Quat::from_rotation_y(-yaw)`, which takes +x to (cos, sin) in (x, z)) and
/// moved to its spot. The turn is `grass::cos_sin_deg`'s arithmetic, so native and wasm agree.
pub fn fence_lines(def: &TileDef) -> Vec<Vec<Vec2>> {
    def.props
        .hero
        .iter()
        .filter_map(|h| {
            let f = fence(h.piece.as_str())?;
            let axis = super::grass::cos_sin_deg(h.yaw_deg);
            Some(
                f.line
                    .iter()
                    .map(|p| Vec2::from(h.at) + axis * p[0] + axis.perp() * p[1])
                    .collect(),
            )
        })
        .collect()
}

/// Scale jitter (± fraction): trees vary less than rocks and flowers.
fn scale_jitter(f: Footprint) -> f32 {
    if f.crown > 0.0 { 0.12 } else { 0.15 }
}

/// A triangle's corners in the tile frame (runtime axes: Y up).
pub type Tri = [[f32; 3]; 3];

/// A tile's ground: the topmost upward triangles over each (x, z), with their slot names,
/// bucketed on a 1 m grid.
pub struct Surface {
    slots: Vec<String>,
    /// Each triangle with its doubled signed (x, z) area, which `top` divides by.
    tris: Vec<(Tri, f32, u16)>,
    buckets: Vec<Vec<u32>>,
}

fn bucket(v: f32) -> usize {
    (((v + EXTENT) / BUCKET).floor().max(0.0) as usize).min(BUCKETS - 1)
}

/// Slots of ground pieces that stand on the ground rather than being it: a crop's leaning row cards
/// on a centre piece. Nothing stands on them and the walker walks through them.
pub const NOT_GROUND: &[&str] = &["wheat_field_wheat"];

impl Surface {
    /// Vertical (skirt) and degenerate triangles carry no ground and are dropped, as are the
    /// [`NOT_GROUND`] slots.
    pub fn new(parts: impl IntoIterator<Item = (Tri, String)>) -> Self {
        let mut slots: Vec<String> = vec![];
        let mut tris = vec![];
        let mut buckets = vec![vec![]; BUCKETS * BUCKETS];
        for (t, name) in parts {
            if NOT_GROUND.contains(&name.as_str()) {
                continue;
            }
            let [a, b, c] = t;
            let d = (b[0] - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (b[2] - a[2]);
            if d.abs() < 1e-9 {
                continue;
            }
            let slot = match slots.iter().position(|s| *s == name) {
                Some(i) => i,
                None => {
                    slots.push(name);
                    slots.len() - 1
                }
            } as u16;
            let i = tris.len() as u32;
            tris.push((t, d, slot));
            let lo = |k: usize| bucket(t.iter().map(|p| p[k]).fold(f32::MAX, f32::min));
            let hi = |k: usize| bucket(t.iter().map(|p| p[k]).fold(f32::MIN, f32::max));
            for bz in lo(2)..=hi(2) {
                for bx in lo(0)..=hi(0) {
                    buckets[bz * BUCKETS + bx].push(i);
                }
            }
        }
        Self {
            slots,
            tris,
            buckets,
        }
    }

    /// Height and slot index ([`Surface::slots`]) of the topmost upward triangle over (x, z).
    pub(crate) fn top(&self, x: f32, z: f32) -> Option<(f32, u16)> {
        if x.abs() >= EXTENT || z.abs() >= EXTENT {
            return None;
        }
        let mut best: Option<(f32, u16)> = None;
        for &i in &self.buckets[bucket(z) * BUCKETS + bucket(x)] {
            let ([a, b, c], d, slot) = self.tris[i as usize];
            let u = ((x - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (z - a[2])) / d;
            let v = ((b[0] - a[0]) * (z - a[2]) - (x - a[0]) * (b[2] - a[2])) / d;
            if u < -1e-5 || v < -1e-5 || u + v > 1.0 + 1e-5 {
                continue;
            }
            let y = a[1] + u * (b[1] - a[1]) + v * (c[1] - a[1]);
            if best.is_none_or(|(h, _)| y > h) {
                best = Some((y, slot));
            }
        }
        best
    }

    /// Height and slot of the topmost upward triangle over (x, z), if any.
    pub fn at(&self, x: f32, z: f32) -> Option<(f32, &str)> {
        self.top(x, z)
            .map(|(h, s)| (h, self.slots[s as usize].as_str()))
    }

    /// The slot names, indexed by the slot index [`Surface::top`] returns.
    pub(crate) fn slots(&self) -> &[String] {
        &self.slots
    }

    /// Per bucket of the grid [`Surface::top`] searches, whether any of its triangles passes
    /// `keep` (given its slot index and corners).
    pub(crate) fn buckets_where(&self, keep: impl Fn(u16, &Tri) -> bool) -> Vec<bool> {
        self.buckets
            .iter()
            .map(|b| {
                b.iter().any(|&i| {
                    let (t, _, slot) = &self.tris[i as usize];
                    keep(*slot, t)
                })
            })
            .collect()
    }

    /// The bucket [`Surface::top`] searches for (x, z), `None` where it finds nothing.
    pub(crate) fn bucket_at(x: f32, z: f32) -> Option<usize> {
        (x.abs() < EXTENT && z.abs() < EXTENT).then(|| bucket(z) * BUCKETS + bucket(x))
    }
}

/// Hero props that are ground, not obstacles: the station platform, whose paving and ramps the
/// walker stands on and scatter keeps off. Each stands at the tile origin at a whole hex turn
/// (`ground_props_stand_at_the_origin_on_a_hex_turn`), so it joins the ground in the tile frame
/// as a centre piece does.
pub const GROUND_PROPS: &[&str] = &["rail_station_platform"];

/// The ground pieces of a tile and their turn about +Y in the tile frame ([`turn`] steps, as
/// `tile_children` draws them with `piece_rotation`): the base, the centre piece if it has one, its
/// lanes' edge pieces, and its [`GROUND_PROPS`] heroes (a yaw of 60 k degrees is turn k).
pub fn ground_pieces<'a>(def: &'a TileDef, catalog: &'a TileCatalog) -> Vec<(&'a str, u8)> {
    let mut v = vec![(def.base.as_str(), 0)];
    for h in &def.props.hero {
        if GROUND_PROPS.contains(&h.piece.as_str()) {
            v.push((
                h.piece.as_str(),
                (h.yaw_deg / 60.0).round().rem_euclid(6.0) as u8,
            ));
        }
    }
    if let Some(c) = &def.center {
        v.push((c.as_str(), 0));
    }
    let pieces = def.edge_pieces(catalog.sockets());
    for e in 0..6u8 {
        let s = &def.edges[e as usize];
        if s.lane.is_some()
            && matches!(s.kind, SocketKind::Road | SocketKind::River)
            && let Some(p) = pieces[e as usize]
        {
            v.push((p.as_str(), e));
        }
    }
    v
}

/// Where one rule's piece may stand: raster cells (row-major from (-EXTENT, -EXTENT)) whose
/// centre clears every rule. `area` is their total in m².
#[derive(Clone, Debug, Default)]
pub struct Mask {
    cells: Vec<u32>,
    pub area: f32,
}

/// A tile definition's ground, ready to scatter on: its surface and one mask per scatter rule.
pub struct Prepared {
    pub surface: Surface,
    pub masks: Vec<Mask>,
}

fn cell_centre(i: usize) -> f32 {
    -EXTENT + (i as f32 + 0.5) * RASTER
}

/// The unit direction of local edge `k`, `k` × 60° from +x toward +z. Built from `sqrt` alone,
/// which IEEE rounds exactly on every target, where `sin`/`cos` may differ by an ulp between the
/// native libm and wasm's: one flipped raster cell would reshuffle a whole layout.
pub fn hex_dir(k: u8) -> Vec2 {
    let h = 3f32.sqrt() / 2.0;
    [
        (1.0, 0.0),
        (0.5, h),
        (-0.5, h),
        (-1.0, 0.0),
        (-0.5, -h),
        (0.5, -h),
    ]
    .map(|(x, z)| Vec2::new(x, z))[usize::from(k % 6)]
}

/// The 16 unit directions `k` × 22.5° from +x toward +z, from `sqrt` alone ([`hex_dir`]).
fn ring16() -> [Vec2; 16] {
    let r = std::f32::consts::FRAC_1_SQRT_2;
    let c = (2.0 + 2f32.sqrt()).sqrt() / 2.0;
    let s = (2.0 - 2f32.sqrt()).sqrt() / 2.0;
    let q = [
        Vec2::new(1.0, 0.0),
        Vec2::new(c, s),
        Vec2::new(r, r),
        Vec2::new(s, c),
    ];
    std::array::from_fn(|i| {
        let v = q[i % 4];
        match i / 4 {
            0 => v,
            1 => Vec2::new(-v.y, v.x),
            2 => -v,
            _ => Vec2::new(v.y, -v.x),
        }
    })
}

/// `p` turned `k` × 60° about +Y the way `piece_rotation(k)` turns it (+x toward +z), exactly.
pub fn turn(k: u8, p: [f32; 3]) -> [f32; 3] {
    let d = hex_dir(k);
    [p[0] * d.x - p[2] * d.y, p[1], p[0] * d.y + p[2] * d.x]
}

/// How far `p` reaches toward the nearest hex edge: the apothem on the border.
pub fn hex_reach(p: Vec2) -> f32 {
    (0..3).map(|e| p.dot(hex_dir(e)).abs()).fold(0.0, f32::max)
}

/// Road and river centrelines of a tile as polylines in tile-local metres, with half widths.
pub fn lanes(def: &TileDef) -> Vec<(Vec<Vec2>, f32)> {
    let point = |end: &Endpoint| -> Option<(Vec2, Option<f32>)> {
        match end {
            Endpoint::Edge(k) => {
                let lane = def.edges.get(*k as usize)?.lane?;
                let dir = hex_dir(*k);
                let tangent = Vec2::new(-dir.y, dir.x);
                Some((
                    dir * APOTHEM + tangent * lane.offset,
                    Some(lane.width / 2.0),
                ))
            }
            Endpoint::Node(id) => def
                .nodes
                .iter()
                .find(|n| n.id == *id)
                .map(|n| (Vec2::from(n.at), None)),
        }
    };
    def.networks
        .iter()
        .filter_map(|s| {
            let (a, wa) = point(&s.from)?;
            let (b, wb) = point(&s.to)?;
            let mut line = vec![a];
            line.extend(s.points.iter().map(|p| Vec2::from(*p)));
            line.push(b);
            let half = [wa, wb]
                .into_iter()
                .flatten()
                .reduce(f32::max)
                .unwrap_or(3.0);
            Some((line, half))
        })
        .collect()
}

pub(crate) fn polyline_distance(p: Vec2, line: &[Vec2]) -> f32 {
    line.windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
            p.distance(a + ab * t)
        })
        .fold(f32::MAX, f32::min)
}

/// The footprint hero slot `slot` of `def` keeps clear: the widest of every piece it may draw
/// (`TileDef::hero_options`). A tile's ground and masks are prepared once per tile id, not per
/// cell, so they clear every variant a cell may pick.
pub fn hero_footprint(def: &TileDef, slot: usize) -> Footprint {
    def.hero_options(slot)
        .map(footprint)
        .reduce(|a, b| Footprint {
            ground: a.ground.max(b.ground),
            crown: a.crown.max(b.crown),
        })
        .unwrap_or(DEFAULT_FOOTPRINT)
}

/// Least distance between a hero prop's centre and an instance's.
pub fn hero_clearance(hero: Footprint, inst: Footprint) -> f32 {
    let ground = hero.ground + inst.ground + HERO_GAP;
    if hero.crown > 0.0 && inst.crown > 0.0 {
        ground.max(HERO_CROWN * hero.crown + inst.ground)
    } else {
        ground
    }
}

/// Least distance between a fence line and the centre of an instance of ground reach `reach`.
pub fn fence_clearance(reach: f32) -> f32 {
    FENCE_HALF + reach
}

/// Least distance between a lane's centreline and an instance's centre.
pub fn lane_clearance(rule: &ScatterRule, half_width: f32, inst: Footprint) -> f32 {
    rule.lane_clearance.max(half_width) + LANE_GAP + inst.ground.max(inst.crown)
}

/// Largest [`hex_reach`] of an instance's centre.
pub fn border_reach(inst: Footprint) -> f32 {
    APOTHEM - (inst.ground + BORDER_GAP).max(inst.crown - CROWN_OVERHANG)
}

fn mask(
    def: &TileDef,
    surface: &Surface,
    raster: &[Option<(f32, u16)>],
    rule: &ScatterRule,
) -> Mask {
    let piece = rule.piece.as_str();
    let fp = group_footprint(piece);
    let floors = floor_slots(piece);
    let ok_slot: Vec<bool> = surface.slots.iter().map(|s| floor_ok(floors, s)).collect();
    let beach = coast_band(def, piece).map(|b| (b, Coastline::of(def, 0)));
    let on_floor = |k: usize| raster[k].is_some_and(|(_, s)| ok_slot[s as usize]);
    // The footprint, the half diagonal a dart may land from its cell centre, and the half diagonal
    // from a footprint point to the nearest sample. `stand` then checks the footprint exactly.
    let r = fp.ground + RASTER * std::f32::consts::SQRT_2;
    let n = (r / RASTER).ceil() as i32;
    let disc: Vec<(i32, i32)> = (-n..=n)
        .flat_map(|j| (-n..=n).map(move |i| (i, j)))
        .filter(|&(i, j)| ((i * i + j * j) as f32).sqrt() * RASTER <= r)
        .collect();
    let border = border_reach(fp);
    // A fence hero is kept clear by its line after the darts (`layout`), not here: a mask change
    // would reshuffle every dart of the tile, the ones far from the fence too.
    let heroes: Vec<(Vec2, f32)> = def
        .props
        .hero
        .iter()
        .enumerate()
        .filter(|(_, h)| fence(h.piece.as_str()).is_none())
        .map(|(i, h)| {
            let c = hero_clearance(hero_footprint(def, i), fp);
            (Vec2::from(h.at), c)
        })
        .collect();
    let bank = bank_band(piece);
    let (rivers, lanes): (Vec<_>, Vec<_>) = def
        .networks
        .iter()
        .zip(lanes(def))
        .map(|(n, (line, half))| {
            let river = n.kind == NetworkKind::River;
            (river, (line, lane_clearance(rule, half, fp)))
        })
        .partition(|(river, _)| (bank.is_some() || water_prop(piece)) && *river);
    // A water prop: its own band, border and spring reach, at its largest scale from anywhere in
    // its cell, and a lily cluster's whole footprint over open water.
    let springs = spring_nodes(def);
    let water_reach = fp.ground * (1.0 + scale_jitter(fp));
    let off_water_cell = |k: usize, p: Vec2| {
        water_prop(piece)
            && raster[k].is_none_or(|(h, _)| {
                off_water(
                    lily(piece),
                    &springs,
                    p,
                    water_reach,
                    h,
                    RASTER * std::f32::consts::FRAC_1_SQRT_2,
                )
            })
    };
    let wet = |k: usize| !lily(piece) || raster[k].is_some_and(|(h, _)| h < WATER_REST);
    let rivers: Vec<Vec<Vec2>> = rivers.into_iter().map(|(_, (l, _))| l).collect();
    let lanes: Vec<(Vec<Vec2>, f32)> = lanes.into_iter().map(|(_, l)| l).collect();
    let off_bank = |k: usize, p: Vec2| {
        bank.is_some_and(|(low, high)| {
            let near = rivers.iter().any(|l| polyline_distance(p, l) <= BANK_REACH);
            !near || raster[k].is_none_or(|(h, _)| h < low || h > high)
        })
    };
    let band = verge_band(piece);
    // At its largest scale, from anywhere in its cell (a dart lands up to half a cell diagonal off).
    let seabed_border = seabed(piece).then_some(
        APOTHEM
            - SEABED_BORDER
            - fp.ground * (1.0 + scale_jitter(fp))
            - RASTER * std::f32::consts::FRAC_1_SQRT_2,
    );
    let mut cells = vec![];
    for j in 0..CELLS {
        for i in 0..CELLS {
            let k = j * CELLS + i;
            if !on_floor(k) {
                continue;
            }
            let p = Vec2::new(cell_centre(i), cell_centre(j));
            if hex_reach(p) > border
                || heroes.iter().any(|(h, c)| p.distance(*h) < *c)
                || lanes.iter().any(|(l, c)| polyline_distance(p, l) < *c)
                || band.is_some_and(|b| lanes.iter().all(|(l, _)| polyline_distance(p, l) > b))
                || beach.is_some_and(|(b, c)| off_beach_band(b, c, p, fp.ground))
                || off_bank(k, p)
                || off_water_cell(k, p)
                || seabed_border.is_some_and(|r| hex_reach(p) > r)
            {
                continue;
            }
            let covered = disc.iter().all(|&(di, dj)| {
                let (ii, jj) = (i as i32 + di, j as i32 + dj);
                (0..CELLS as i32).contains(&ii)
                    && (0..CELLS as i32).contains(&jj)
                    && on_floor(jj as usize * CELLS + ii as usize)
                    && wet(jj as usize * CELLS + ii as usize)
            });
            if covered {
                cells.push(k as u32);
            }
        }
    }
    let area = cells.len() as f32 * RASTER * RASTER;
    Mask { cells, area }
}

/// Builds a definition's ground from its ground pieces' LOD0 triangles (`tris_of`, tile-local
/// before the piece's turn), or `None` while one is missing.
pub fn prepare(
    def: &TileDef,
    catalog: &TileCatalog,
    tris_of: &mut dyn FnMut(&str) -> Option<Vec<(Tri, String)>>,
) -> Option<Prepared> {
    let mut all = vec![];
    for (id, k) in ground_pieces(def, catalog) {
        for (t, s) in tris_of(id)? {
            all.push((t.map(|p| turn(k, p)), s));
        }
    }
    let surface = Surface::new(all);
    let raster: Vec<Option<(f32, u16)>> = (0..CELLS * CELLS)
        .map(|k| surface.top(cell_centre(k % CELLS), cell_centre(k / CELLS)))
        .collect();
    let masks = def
        .props
        .scatter
        .iter()
        .map(|r| mask(def, &surface, &raster, r))
        .collect();
    Some(Prepared { surface, masks })
}

/// One placed prop, tile-local.
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    /// Index of its rule in `props.scatter`.
    pub rule: u8,
    /// Centre at the surface height under it.
    pub pos: Vec3,
    /// Turn about +Y (radians).
    pub yaw: f32,
    pub scale: f32,
    /// Rank by distance from the tile centre: the rise grows instances in this order.
    pub order: u8,
    /// Which piece of its rule's group it draws ([`variant_piece`]; 0 is the rule's own piece).
    pub variant: u8,
}

impl Instance {
    /// The piece it draws, for a rule naming `rule_piece`.
    pub fn piece<'a>(&self, rule_piece: &'a str) -> &'a str {
        variant_piece(rule_piece, self.variant)
    }

    pub fn rotation(&self) -> Quat {
        Quat::from_rotation_y(self.yaw)
    }

    pub fn transform(&self) -> Transform {
        Transform::from_translation(self.pos)
            .with_rotation(self.rotation())
            .with_scale(Vec3::splat(self.scale))
    }
}

/// A tile's scatter. `planned`, `area` (the rule's whole ground) and `spacing` (the least distance
/// between two of its instances) are per rule.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub instances: Vec<Instance>,
    pub planned: Vec<u32>,
    pub area: Vec<f32>,
    pub spacing: Vec<f32>,
}

impl Layout {
    pub fn placed(&self, rule: usize) -> usize {
        self.instances
            .iter()
            .filter(|i| i.rule as usize == rule)
            .count()
    }

    /// FNV-1a over every instance's rule, position, yaw and scale bits: equal layouts, equal value.
    pub fn fingerprint(&self) -> u64 {
        let mut h: u64 = 0xCBF2_9CE4_8422_2325;
        for i in &self.instances {
            let words = [
                u32::from(i.rule) | (u32::from(i.variant) << 8),
                i.pos.x.to_bits(),
                i.pos.y.to_bits(),
                i.pos.z.to_bits(),
                i.yaw.to_bits(),
                i.scale.to_bits(),
            ];
            for b in words.iter().flat_map(|w| w.to_le_bytes()) {
                h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01B3);
            }
        }
        h
    }
}

/// SplitMix64.
pub(crate) struct Rng(pub(crate) u64);

impl Rng {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub(crate) fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// The `unit` the `k`th next draw (from 0) would return, without drawing: SplitMix64's state
    /// steps by a constant, so a draw can be made out of order.
    pub(crate) fn unit_at(&self, k: u64) -> f32 {
        Rng(self.0.wrapping_add(k.wrapping_mul(0x9E37_79B9_7F4A_7C15))).unit()
    }

    /// Steps past the next `n` draws, as `n` calls to `next` would.
    pub(crate) fn skip(&mut self, n: u64) {
        self.0 = self.0.wrapping_add(n.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// A lattice value in [0, 1) of the drift noise: SplitMix64 of the seed and the lattice point.
fn lattice(seed: u64, i: i32, j: i32) -> f32 {
    let key = (u64::from(i as u32) << 32) | u64::from(j as u32);
    Rng(seed ^ key.wrapping_mul(0xD6E8_FEB8_6659_FD93)).unit()
}

/// Value noise at `p` (tile-local metres) on a lattice `cell` metres apart, smoothstep-blended.
/// Arithmetic and `floor` alone, so native and wasm agree bit for bit.
fn value_noise(seed: u64, p: Vec2, cell: f32) -> f32 {
    let g = p / cell;
    let (fx, fz) = (g.x.floor(), g.y.floor());
    let (i, j) = (fx as i32, fz as i32);
    let ease = |t: f32| t * t * (3.0 - 2.0 * t);
    let (u, v) = (ease(g.x - fx), ease(g.y - fz));
    let a = lattice(seed, i, j) + u * (lattice(seed, i + 1, j) - lattice(seed, i, j));
    let b = lattice(seed, i, j + 1) + u * (lattice(seed, i + 1, j + 1) - lattice(seed, i, j + 1));
    a + v * (b - a)
}

/// The drift noise of a tile with seed `seed` at `p`: two octaves of value noise.
pub fn drift_noise(seed: u64, p: Vec2) -> f32 {
    let s = seed ^ 0xD71F_7A55_0C3E_91B5;
    (value_noise(s, p, DRIFT_CELL) + 0.35 * value_noise(s.rotate_left(17), p, 0.5 * DRIFT_CELL))
        / 1.35
}

/// The noise a `vary` rule thins its even spread by (`ScatterRule::vary`), in [0, 1]: the drift
/// noise on a seed of its own (single flowers do not just follow the clumps' drifts), stretched
/// by a smoothstep over its middle, where the two smoothed octaves spend most of their values.
pub fn vary_noise(seed: u64, p: Vec2) -> f32 {
    let n = drift_noise(seed.rotate_left(29) ^ 0x51F1_E5A3_0C77_9B2D, p);
    let t = ((n - 0.3) / 0.4).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The cells of `mask` a drift rule grows on: the [`DRIFT_SHARE`] with the highest drift noise.
fn drift_cells(seed: u64, mask: &Mask) -> Vec<u32> {
    let mut scored: Vec<(f32, u32)> = mask
        .cells
        .iter()
        .map(|&k| {
            let k_ = k as usize;
            let p = Vec2::new(cell_centre(k_ % CELLS), cell_centre(k_ / CELLS));
            (drift_noise(seed, p), k)
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let keep = ((scored.len() as f32 * DRIFT_SHARE).ceil() as usize).min(scored.len());
    let mut cells: Vec<u32> = scored[..keep].iter().map(|&(_, k)| k).collect();
    cells.sort_unstable();
    cells
}

/// Lowest surface height under the centre and a 0.3 m ring around `p` (the centre's own height
/// when `level` is false: a bank prop, laid along the bank's contour), if `p` stands on `floors`.
fn stand(surface: &Surface, p: Vec2, floors: &[&str], reach: f32, level: bool) -> Option<f32> {
    let on_floor = |q: Vec2| surface.at(q.x, q.y).filter(|(_, s)| floor_ok(floors, s));
    let (h, _) = on_floor(p)?;
    // The footprint's rim and half-rim, exactly (the mask only eroded the sampled ground).
    let ring16 = ring16();
    for d in ring16 {
        on_floor(p + reach * d)?;
    }
    for d in ring16.iter().step_by(2) {
        on_floor(p + 0.5 * reach * *d)?;
    }
    if !level {
        return Some(h);
    }
    let ring = (0..6).filter_map(|k| {
        let q = p + 0.3 * hex_dir(k);
        surface.at(q.x, q.y).map(|(y, _)| y)
    });
    Some(ring.fold(h, f32::min))
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
/// else but water props ([`water_prop`], capped on their own by [`keep_water`]).
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
fn keep_dressing(out: &mut Layout, seed: u64, cap: usize, water: &[bool]) {
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

fn lily(piece: &str) -> bool {
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
fn keep_water(out: &mut Layout, seed: u64, water: &[bool]) {
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
fn off_water(lily: bool, springs: &[Vec2], p: Vec2, reach: f32, h: f32, slop: f32) -> bool {
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
fn water_stand(
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

/// A reed bed's yaw from draw `u`: its -z (Blender +y) up the slope at `p` (central differences
/// 0.5 m apart), within [`DRIFT_YAW_JITTER`]; `None` where the ground is level or missing.
fn uphill_yaw(surface: &Surface, p: Vec2, u: f32) -> Option<f32> {
    let h = |dx: f32, dz: f32| surface.at(p.x + dx, p.y + dz).map(|(y, _)| y);
    let n = Vec2::new(h(0.5, 0.0)? - h(-0.5, 0.0)?, h(0.0, 0.5)? - h(0.0, -0.5)?);
    // Quat::from_rotation_y(yaw) turns the piece's -z to (-sin yaw, -cos yaw) in (x, z).
    (n.length_squared() > 1e-6)
        .then(|| portable_atan2(-n.x, -n.y) + (2.0 * u - 1.0) * DRIFT_YAW_JITTER)
}

/// Every instance the darts place for a tile before a capped tile keeps its [`dressing_cap`], in
/// dart order and not yet ranked.
fn every_dart(def: &TileDef, seed: u64, ground: &Prepared) -> Layout {
    let mut rng = Rng(seed ^ 0x5CA7_7E25_D3A1_0B07);
    let mut out = Layout::default();
    // (centre, ground reach, rule) of everything placed so far.
    let mut placed: Vec<(Vec2, f32, usize)> = vec![];
    let coast = Coastline::of(def, 0);
    let springs = spring_nodes(def);
    for (ri, rule) in def.props.scatter.iter().enumerate() {
        let piece = rule.piece.as_str();
        let fp = group_footprint(piece);
        let mask = &ground.masks[ri];
        let want = (rule.density.max(0.0) * CAP_AREA / 100.0 + rng.unit()).floor() as u32;
        out.planned.push(want);
        out.area.push(mask.area);
        let drift;
        let cells = if rule.drift {
            drift = drift_cells(seed, mask);
            &drift
        } else {
            &mask.cells
        };
        let grows_on = cells.len() as f32 * RASTER * RASTER;
        let pack = if rule.drift { DRIFT_PACK } else { 2.0 };
        let spacing = (BLUE_NOISE * (grows_on / want.max(1) as f32).sqrt())
            .max(pack * fp.ground)
            .max(CROWN_SPACING * fp.crown);
        out.spacing.push(spacing);
        if want == 0 || cells.is_empty() {
            continue;
        }
        let jitter = scale_jitter(fp);
        let floors = floor_slots(piece);
        let vary = rule.vary.clamp(0.0, 1.0);
        let beach = coast_band(def, piece);
        let bank = bank_band(piece);
        let water = water_prop(piece);
        let mut n = 0;
        for _ in 0..ATTEMPTS * want + 64 {
            if n == want {
                break;
            }
            let variant = pick_variant(def, piece, seed, ri, n);
            let reach = footprint(variant_piece(piece, variant)).ground;
            let k = cells[rng.below(cells.len())] as usize;
            let p = Vec2::new(
                cell_centre(k % CELLS) + (rng.unit() - 0.5) * RASTER,
                cell_centre(k / CELLS) + (rng.unit() - 0.5) * RASTER,
            );
            if vary > 0.0 && rng.unit() > 1.0 - vary * (1.0 - vary_noise(seed, p)) {
                continue;
            }
            if let (Some(b), Some(c)) = (beach, coast)
                && b.thin > 0.0
                && rng.unit() > beach_keep(b, c, p)
            {
                continue;
            }
            let clear = placed.iter().all(|&(q, g, r)| {
                let d = p.distance(q);
                if r == ri {
                    d >= spacing
                } else {
                    d >= g + reach
                }
            });
            if !clear {
                continue;
            }
            let level = bank.is_none() && !water;
            let Some(y) = stand(&ground.surface, p, floors, reach, level) else {
                continue;
            };
            // The mask checks the band at its cell centres; the dart, off-centre, exactly.
            if bank.is_some_and(|(low, high)| y < low || y > high) {
                continue;
            }
            let y = match water.then(|| {
                let r = reach * (1.0 + jitter);
                water_stand(&ground.surface, piece, &springs, p, r, y)
            }) {
                Some(None) => continue,
                Some(Some(w)) => w,
                None => y,
            };
            let y = if seabed(piece) { SEABED_Y } else { y };
            let u = rng.unit();
            let yaw = match bank.and_then(|_| contour(&ground.surface, p)) {
                Some(t) => along_yaw(t, u),
                None if water && !lily(piece) => {
                    uphill_yaw(&ground.surface, p, u).unwrap_or(u * TAU)
                }
                None => instance_yaw(beach, coast, p, u),
            };
            let scale = 1.0 + jitter * (2.0 * rng.unit() - 1.0);
            placed.push((p, reach, ri));
            out.instances.push(Instance {
                rule: ri as u8,
                pos: Vec3::new(p.x, y, p.y),
                yaw,
                scale,
                order: 0,
                variant,
            });
            n += 1;
        }
    }
    // Hero fences: nothing stands on a fence line. Dropped after the darts, so every instance
    // clear of the line keeps the place it has without the fence; a dropped one is not replaced,
    // so a fenced tile may place fewer than planned.
    let fences = fence_lines(def);
    if !fences.is_empty() {
        out.instances.retain(|i| {
            let piece = i.piece(def.props.scatter[i.rule as usize].piece.as_str());
            let need = fence_clearance(footprint(piece).ground * i.scale);
            fences
                .iter()
                .all(|l| polyline_distance(i.pos.xz(), l) >= need)
        });
    }
    out
}

/// The scatter of a tile with definition `def` and seed `seed` on its prepared ground.
pub fn layout(def: &TileDef, seed: u64, ground: &Prepared) -> Layout {
    let mut out = every_dart(def, seed, ground);
    let water: Vec<bool> = def
        .props
        .scatter
        .iter()
        .map(|r| water_prop(r.piece.as_str()))
        .collect();
    if let Some(cap) = dressing_cap(def, seed) {
        keep_dressing(&mut out, seed, cap, &water);
    }
    if water.contains(&true) {
        keep_water(&mut out, seed, &water);
    }
    let mut by_distance: Vec<usize> = (0..out.instances.len()).collect();
    by_distance.sort_by(|&a, &b| {
        let d = |i: usize| out.instances[i].pos.xz().length();
        d(a).total_cmp(&d(b)).then(a.cmp(&b))
    });
    for (rank, i) in by_distance.into_iter().enumerate() {
        out.instances[i].order = rank.min(u8::MAX as usize) as u8;
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Play: drawing, streaming, the shared layouts and the report

/// The entity holding one tile's scatter, a child of the tile root.
#[derive(Component)]
pub struct ScatterGroup;

/// What a tile's group was drawn from: its tile, seed and which rules' pieces the kit had.
#[derive(Clone, Debug, PartialEq)]
struct Key {
    tile: String,
    seed: u64,
    drawn: Vec<bool>,
    /// The field boundaries on the tile's edges ([`super::boundaries::fingerprint`]).
    cut: u64,
}

/// Prepared grounds by tile id, layouts by cell, and the group drawn under each tile root.
#[derive(Resource, Default)]
pub struct PlayScatter {
    prepared: HashMap<String, Arc<Prepared>>,
    layouts: HashMap<Axial, (String, u64, u64, Arc<Layout>)>,
    roots: HashMap<Entity, (Key, Option<Entity>)>,
    /// The field boundaries' footprints per tile (tile frame), with the fingerprint of the world
    /// seed and placed tiles they were derived from: rebuilt only when those change, not on every
    /// view change (a hovered cell).
    fences: (u64, Arc<HashMap<Axial, Vec<hex_core::boundary::Seg>>>),
    /// How many times `fences` was derived (tests).
    pub(crate) fence_builds: u64,
}

impl PlayScatter {
    /// The definition's prepared ground, built once its ground pieces are in the kit and shared by
    /// every placed tile of that definition and the ghost. A piece outside the kit holds it back
    /// only while `tris_of` has nothing for it: a piece lost for the session ([`ground_tris`])
    /// adds no ground, and the rest still grows grass and scatter.
    pub(crate) fn ground(
        &mut self,
        def: &TileDef,
        catalog: &TileCatalog,
        kit: &HashMap<String, Vec<KitPart>>,
        tris_of: &mut dyn FnMut(&str) -> Option<Vec<(Tri, String)>>,
    ) -> Option<Arc<Prepared>> {
        if !ground_pieces(def, catalog)
            .iter()
            .all(|(id, _)| kit.contains_key(*id) || tris_of(id).is_some())
        {
            return None;
        }
        if let Some(p) = self.prepared.get(&def.id) {
            return Some(p.clone());
        }
        let p = Arc::new(prepare(def, catalog, tris_of)?);
        self.prepared.insert(def.id.clone(), p.clone());
        Some(p)
    }
}

/// One drawn tile's scatter.
#[derive(Clone, Debug)]
pub struct TileScatter {
    pub tile: String,
    pub seed: u64,
    pub layout: Arc<Layout>,
    /// The field boundaries on the tile's edges in its frame (`hex_core::boundary`, B2): the
    /// layout has none of its instances over them, and grass keeps off them.
    pub fences: Arc<Vec<hex_core::boundary::Seg>>,
    /// Their fingerprint ([`super::boundaries::fingerprint`], 0 for none).
    pub cut: u64,
    /// Per rule: its piece is in the kit and its instances are drawn.
    pub drawn: Vec<bool>,
    /// Per rule: its piece.
    pub pieces: Vec<String>,
}

impl TileScatter {
    /// The piece instance `i` draws: its rule's piece or one of its variants.
    pub fn piece_of(&self, i: &Instance) -> &str {
        i.piece(self.pieces[i.rule as usize].as_str())
    }
}

/// The scatter of every drawn tile, for the boids' roosts; `revision` changes with it.
#[derive(Resource, Default)]
pub struct ScatterLayouts {
    pub revision: u64,
    pub tiles: BTreeMap<Axial, TileScatter>,
}

impl ScatterLayouts {
    /// Every drawn instance with its cell and piece.
    pub fn drawn(&self) -> impl Iterator<Item = (Axial, &str, &Instance)> {
        self.tiles.iter().flat_map(|(c, t)| {
            t.layout
                .instances
                .iter()
                .filter(|i| t.drawn[i.rule as usize])
                .map(move |i| (*c, t.piece_of(i), i))
        })
    }
}

/// `hexWorld.state().scatter` as JSON: per tile, the planned, placed and drawn count of each piece.
#[derive(Resource, Clone, Debug, Default)]
pub struct ScatterReport(pub String);

/// LOD0 triangles of a loaded kit piece with their slot names (`build_piece`'s slot walk).
pub(crate) fn kit_tris(
    id: &str,
    packs: &PlayPacks,
    (gltfs, gltf_meshes, meshes): (&Assets<Gltf>, &Assets<GltfMesh>, &Assets<Mesh>),
) -> Option<Vec<(Tri, String)>> {
    let g = gltfs.get(packs.glb(id)?)?;
    let gm = gltf_meshes.get(g.named_meshes.get(format!("{id}_lod0").as_str())?)?;
    let mut out = vec![];
    for prim in &gm.primitives {
        let name = prim
            .material
            .as_ref()
            .and_then(|mh| {
                g.named_materials
                    .iter()
                    .find(|(_, v)| *v == mh)
                    .map(|(k, _)| k.to_string())
            })
            .unwrap_or_default();
        let mesh = meshes.get(&prim.mesh)?;
        let pos = mesh.attribute(Mesh::ATTRIBUTE_POSITION)?.as_float3()?;
        let idx: Vec<usize> = mesh.indices()?.iter().collect();
        for t in idx.as_chunks::<3>().0 {
            out.push(([pos[t[0]], pos[t[1]], pos[t[2]]], name.clone()));
        }
    }
    Some(out)
}

/// [`kit_tris`] for a tile's ground: a piece whose GLB failed for good (`PlayPacks::lost`) has
/// none, so its tile's ground is built from the pieces that loaded.
pub(crate) fn ground_tris(
    id: &str,
    packs: &PlayPacks,
    assets: (&Assets<Gltf>, &Assets<GltfMesh>, &Assets<Mesh>),
) -> Option<Vec<(Tri, String)>> {
    kit_tris(id, packs, assets).or_else(|| packs.lost(id).then(Vec::new))
}

/// Spawns the drawn instances of `layout` under `root`, growing in the props stage of `cell`'s rise.
fn spawn_group(
    commands: &mut Commands,
    (root, cell): (Entity, Axial),
    kit: &HashMap<String, Vec<KitPart>>,
    def: &TileDef,
    layout: &Layout,
    drawn: &[bool],
) -> Option<Entity> {
    if !layout.instances.iter().any(|i| drawn[i.rule as usize]) {
        return None;
    }
    let count = layout.instances.len().min(u8::MAX as usize) as u8;
    let group = commands
        .spawn((
            ScatterGroup,
            Transform::IDENTITY,
            Visibility::default(),
            SceneEntity,
        ))
        .id();
    commands.entity(root).add_child(group);
    commands.entity(group).with_children(|p| {
        for inst in layout.instances.iter().filter(|i| drawn[i.rule as usize]) {
            let role = RiseRole::Prop {
                order: inst.order,
                count,
                kind: PropKind::Tree,
            };
            let piece = inst.piece(def.props.scatter[inst.rule as usize].piece.as_str());
            spawn_kit_parts(p, kit, piece, inst.transform(), Some((cell, role)));
        }
    });
    Some(group)
}

fn json_counts(pieces: &[String], n: impl Fn(usize) -> usize) -> String {
    let fields: Vec<String> = pieces
        .iter()
        .enumerate()
        .map(|(i, p)| format!("\"{p}\":{}", n(i)))
        .collect();
    format!("{{{}}}", fields.join(","))
}

fn report(
    world_seed: u64,
    tiles: &BTreeMap<Axial, TileScatter>,
    waiting: &[(Axial, String)],
    kit: &HashMap<String, Vec<KitPart>>,
) -> String {
    let mut rows = vec![];
    let mut drawn_by_piece: BTreeMap<&str, usize> = BTreeMap::new();
    let mut placed_total = 0;
    for (c, t) in tiles {
        let l = &t.layout;
        let drawn = |i: usize| if t.drawn[i] { l.placed(i) } else { 0 };
        for p in &t.pieces {
            drawn_by_piece.entry(p.as_str()).or_default();
        }
        for i in l.instances.iter().filter(|i| t.drawn[i.rule as usize]) {
            *drawn_by_piece.entry(t.piece_of(i)).or_default() += 1;
        }
        placed_total += l.instances.len();
        rows.push(format!(
            "{{\"q\":{},\"r\":{},\"tile\":\"{}\",\"layout\":\"{:016x}\",\"planned\":{},\"placed\":{},\"drawn\":{},\"area_m2\":{}}}",
            c.q,
            c.r,
            t.tile,
            l.fingerprint(),
            json_counts(&t.pieces, |i| l.planned[i] as usize),
            json_counts(&t.pieces, |i| l.placed(i)),
            json_counts(&t.pieces, drawn),
            json_counts(&t.pieces, |i| l.area[i].round() as usize),
        ));
    }
    let entities: usize = drawn_by_piece
        .iter()
        .map(|(p, n)| n * kit.get(*p).map_or(0, Vec::len))
        .sum();
    let pairs: HashSet<_> = drawn_by_piece
        .iter()
        .filter(|(_, n)| **n > 0)
        .flat_map(|(p, _)| kit.get(*p).into_iter().flatten())
        .map(|part| (part.lod.min(2), part.mesh.id(), part.material.id()))
        .collect();
    // Per LOD, tracked, not gated (owner A24 Q5, 2026-10-26, replacing A2 Q2's 60 per LOD).
    let mut by_lod = [0usize; 3];
    for (lod, _, _) in &pairs {
        by_lod[*lod] += 1;
    }
    let drawn_total: usize = drawn_by_piece.values().sum();
    let by_piece: Vec<String> = drawn_by_piece
        .iter()
        .map(|(p, n)| format!("\"{p}\":{n}"))
        .collect();
    let waiting: Vec<String> = waiting
        .iter()
        .map(|(c, t)| format!("{{\"q\":{},\"r\":{},\"tile\":\"{t}\"}}", c.q, c.r))
        .collect();
    format!(
        "{{\"world_seed\":{world_seed},\"tiles\":[{}],\"waiting\":[{}],\"placed\":{placed_total},\"drawn\":{drawn_total},\"drawn_by_piece\":{{{}}},\"entities\":{entities},\"mesh_material_pairs\":{},\"mesh_material_pairs_by_lod\":[{},{},{}]}}",
        rows.join(","),
        waiting.join(","),
        by_piece.join(","),
        pairs.len(),
        by_lod[0],
        by_lod[1],
        by_lod[2]
    )
}

/// After `play::sync`: draws each placed tile's scatter under its root once its ground is in the
/// kit, redraws it when a rule's piece arrives or a pack refresh rebuilt the tile's children, and
/// publishes the layouts and the report.
#[allow(clippy::too_many_arguments)]
pub fn sync(
    mut commands: Commands,
    active: Res<ActiveScenario>,
    view: Res<PlayView>,
    (spawned, kit, packs, catalog): (
        Option<Res<PlaySpawned>>,
        Option<Res<PlayKit>>,
        Option<Res<PlayPacks>>,
        Res<PlayCatalog>,
    ),
    (gltfs, gltf_meshes, meshes): (Res<Assets<Gltf>>, Res<Assets<GltfMesh>>, Res<Assets<Mesh>>),
    (mut state, mut layouts, mut out): (
        ResMut<PlayScatter>,
        ResMut<ScatterLayouts>,
        ResMut<ScatterReport>,
    ),
    groups: Query<(), With<ScatterGroup>>,
    mut removed: RemovedComponents<ScatterGroup>,
) {
    let gone = removed.read().count() > 0;
    let (Scenario::Play, Some(spawned), Some(kit_res), Some(packs)) =
        (active.0, spawned, kit, packs)
    else {
        return;
    };
    let PlayKit::Ready(kit) = &*kit_res else {
        return;
    };
    if !(view.is_changed() || spawned.is_changed() || kit_res.is_changed() || gone) {
        return;
    }
    let state = &mut *state;
    let assets = (&*gltfs, &*gltf_meshes, &*meshes);
    let mut roots = HashMap::new();
    let mut next_layouts = HashMap::new();
    let mut tiles = BTreeMap::new();
    let mut waiting = vec![];
    let mut cells: Vec<_> = spawned.tiles.iter().collect();
    cells.sort_by_key(|(c, _)| (c.q, c.r));
    let tile_at = |c: Axial| {
        spawned
            .tiles
            .get(&c)
            .and_then(|(t, _)| Some((catalog.0.get(&t.tile)?, t.rotation)))
    };
    let tiles_sig = {
        let mut h: u64 = 0xCBF2_9CE4_8422_2325 ^ view.world_seed;
        for (c, (t, _)) in &cells {
            for b in t
                .tile
                .bytes()
                .chain(c.q.to_le_bytes())
                .chain(c.r.to_le_bytes())
                .chain([t.rotation])
            {
                h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01B3);
            }
        }
        h
    };
    if state.fences.0 != tiles_sig || state.fence_builds == 0 {
        let all = super::boundaries::all_placements(
            view.world_seed,
            cells.iter().map(|(c, _)| **c),
            &tile_at,
        );
        let near: std::collections::BTreeSet<Axial> = all
            .keys()
            .flat_map(|c| hex_core::boundary::within(*c, 1))
            .collect();
        let map: HashMap<Axial, Vec<hex_core::boundary::Seg>> = near
            .into_iter()
            .map(|c| (c, super::boundaries::local_segments_from(&all, c, &tile_at)))
            .filter(|(_, s)| !s.is_empty())
            .collect();
        state.fences = (tiles_sig, Arc::new(map));
        state.fence_builds += 1;
    }
    let fence_map = state.fences.1.clone();
    for (&cell, (t, root)) in cells {
        let Some(def) = catalog
            .0
            .get(&t.tile)
            .filter(|d| !d.props.scatter.is_empty())
        else {
            continue;
        };
        let Some(prepared) = state.ground(def, &catalog.0, kit, &mut |id| {
            ground_tris(id, &packs, assets)
        }) else {
            waiting.push((cell, def.id.clone()));
            continue;
        };
        let seed = cell_seed(view.world_seed, cell);
        // The field boundaries on its edges (B2): instances over them are left out, after the darts,
        // so the rest of the layout stays where it was.
        let fences = fence_map.get(&cell).cloned().unwrap_or_default();
        let cut = super::boundaries::fingerprint(&fences);
        let layout = match state.layouts.get(&cell) {
            Some((id, s, k, l)) if *id == def.id && *s == seed && *k == cut => l.clone(),
            _ => {
                let mut l = layout(def, seed, &prepared);
                if !fences.is_empty() {
                    let pieces: Vec<&str> =
                        def.props.scatter.iter().map(|r| r.piece.as_str()).collect();
                    l.instances.retain(|i| {
                        let piece = i.piece(pieces[i.rule as usize]);
                        super::boundaries::clear_of(
                            &fences,
                            i.pos.xz(),
                            footprint(piece).ground * i.scale,
                        )
                    });
                }
                Arc::new(l)
            }
        };
        let drawn: Vec<bool> = def
            .props
            .scatter
            .iter()
            .map(|r| group(r.piece.as_str()).all(|p| kit.contains_key(p)))
            .collect();
        let key = Key {
            tile: def.id.clone(),
            seed,
            drawn: drawn.clone(),
            cut,
        };
        let group = match state.roots.get(root) {
            Some((k, g)) if *k == key && g.is_none_or(|g| groups.contains(g)) => *g,
            old => {
                if let Some((_, Some(g))) = old {
                    commands.entity(*g).try_despawn();
                }
                spawn_group(&mut commands, (*root, cell), kit, def, &layout, &drawn)
            }
        };
        roots.insert(*root, (key, group));
        next_layouts.insert(cell, (def.id.clone(), seed, cut, layout.clone()));
        tiles.insert(
            cell,
            TileScatter {
                tile: def.id.clone(),
                seed,
                layout,
                fences: Arc::new(fences),
                cut,
                drawn,
                pieces: def
                    .props
                    .scatter
                    .iter()
                    .map(|r| r.piece.as_str().to_owned())
                    .collect(),
            },
        );
    }
    state.roots = roots;
    state.layouts = next_layouts;
    let sig = |m: &BTreeMap<Axial, TileScatter>| -> Vec<(Axial, String, u64, Vec<bool>, u64)> {
        m.iter()
            .map(|(c, t)| (*c, t.tile.clone(), t.seed, t.drawn.clone(), t.cut))
            .collect()
    };
    if sig(&layouts.tiles) != sig(&tiles) {
        layouts.revision += 1;
        layouts.tiles = tiles;
    }
    let json = report(view.world_seed, &layouts.tiles, &waiting, kit);
    if out.0 != json {
        out.0 = json;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn portable_atan2_matches_atan2() {
        let mut worst = 0.0f32;
        for i in 0..3600 {
            let a = i as f32 / 3600.0 * std::f32::consts::TAU;
            for r in [0.01f32, 1.0, 37.5] {
                let (y, x) = (a.sin() * r, a.cos() * r);
                worst = worst.max((portable_atan2(y, x) - y.atan2(x)).abs());
            }
        }
        assert!(worst < 5e-5, "worst {worst}");
        assert_eq!(portable_atan2(0.0, 0.0), 0.0);
    }

    use std::sync::{Mutex, OnceLock};

    use hex_assets::parse_manifest;
    use hex_core::{AgentKind, World};

    use super::*;
    use crate::kit_scene::{MANIFEST, piece_rotation};

    type Tris = Option<Vec<(Tri, String)>>;

    fn catalog() -> TileCatalog {
        TileCatalog::builtin().unwrap()
    }

    /// The index of `def`'s scatter rule naming `piece` (the pine forest and the meadow lead with
    /// their granite outcrop and boulders).
    fn rule_of(def: &TileDef, piece: &str) -> u8 {
        def.props
            .scatter
            .iter()
            .position(|r| r.piece.as_str() == piece)
            .unwrap() as u8
    }

    fn glb(id: &str) -> Option<(gltf::Document, Vec<gltf::buffer::Data>)> {
        let m = parse_manifest(MANIFEST).unwrap();
        let e = m.pieces.iter().find(|p| p.id == id)?;
        let path = format!("{}/../../assets/kit/{}", env!("CARGO_MANIFEST_DIR"), e.glb);
        let bytes = std::fs::read(path).ok()?;
        let plain = hex_assets::rewrite_glb(&bytes).ok()?;
        let (doc, buffers, _) = gltf::import_slice(&plain).ok()?;
        Some((doc, buffers))
    }

    /// Triangles of mesh `<id>_<mesh>` from the piece's GLB, with slot names (the kit gate's
    /// `triangles_with_slot`).
    fn glb_mesh_tris(id: &str, mesh: &str) -> Tris {
        let (doc, buffers) = glb(id)?;
        let name = format!("{id}_{mesh}");
        let m = doc.meshes().find(|m| m.name() == Some(name.as_str()))?;
        let mut out = vec![];
        for prim in m.primitives() {
            let slot = prim.material().name().unwrap_or_default().to_string();
            let r = prim.reader(|b| Some(&buffers[b.index()]));
            let pos: Vec<[f32; 3]> = r.read_positions()?.collect();
            let idx: Vec<u32> = r.read_indices()?.into_u32().collect();
            for t in idx.as_chunks::<3>().0 {
                out.push((
                    [pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]],
                    slot.clone(),
                ));
            }
        }
        Some(out)
    }

    /// LOD0 triangles, read once per piece for the whole test binary.
    fn lod0(id: &str) -> Tris {
        static CACHE: OnceLock<Mutex<HashMap<String, Tris>>> = OnceLock::new();
        let cache = CACHE.get_or_init(Default::default);
        if let Some(t) = cache.lock().unwrap().get(id) {
            return t.clone();
        }
        let t = glb_mesh_tris(id, "lod0");
        cache.lock().unwrap().insert(id.to_owned(), t.clone());
        t
    }

    fn prepared(def: &TileDef) -> Option<Prepared> {
        prepare(def, &catalog(), &mut |id| lod0(id))
    }

    /// `Surface::top`, with each triangle's divisor found once in `Surface::new`, answers as a scan
    /// of every triangle whose (x, z) box meets the point's 1 m bucket, in order, computing it.
    #[test]
    fn top_matches_a_scan_of_its_bucket() {
        let c = catalog();
        for id in [
            "meadow",
            "road_junction",
            "rail_road_crossing",
            "river_rail_bridge",
            "beach",
        ] {
            let s = prepared(c.get(id).unwrap()).unwrap().surface;
            let scan = |x: f32, z: f32| {
                if x.abs() >= EXTENT || z.abs() >= EXTENT {
                    return None;
                }
                let mut best: Option<(f32, u16)> = None;
                for ([a, b, cc], _, slot) in &s.tris {
                    let t = [*a, *b, *cc];
                    let lo = |k: usize| bucket(t.iter().map(|p| p[k]).fold(f32::MAX, f32::min));
                    let hi = |k: usize| bucket(t.iter().map(|p| p[k]).fold(f32::MIN, f32::max));
                    if !(lo(0)..=hi(0)).contains(&bucket(x))
                        || !(lo(2)..=hi(2)).contains(&bucket(z))
                    {
                        continue;
                    }
                    let d = (b[0] - a[0]) * (cc[2] - a[2]) - (cc[0] - a[0]) * (b[2] - a[2]);
                    let u = ((x - a[0]) * (cc[2] - a[2]) - (cc[0] - a[0]) * (z - a[2])) / d;
                    let v = ((b[0] - a[0]) * (z - a[2]) - (x - a[0]) * (b[2] - a[2])) / d;
                    if u < -1e-5 || v < -1e-5 || u + v > 1.0 + 1e-5 {
                        continue;
                    }
                    let y = a[1] + u * (b[1] - a[1]) + v * (cc[1] - a[1]);
                    if best.is_none_or(|(h, _)| y > h) {
                        best = Some((y, *slot));
                    }
                }
                best
            };
            let mut rng = Rng(7);
            for _ in 0..20_000 {
                let (x, z) = (rng.unit() * 34.0 - 17.0, rng.unit() * 34.0 - 17.0);
                assert_eq!(s.top(x, z), scan(x, z), "{id} at ({x}, {z})");
            }
            // Points on and beside the bucket edges and quarter lines.
            for k in 0..=(EXTENT * 8.0) as i32 {
                let v = k as f32 * 0.25 - EXTENT;
                for e in [-1e-4, 0.0, 1e-4] {
                    for w in [-7.3, 0.05, 11.9] {
                        assert_eq!(s.top(v + e, w), scan(v + e, w), "{id} at ({}, {w})", v + e);
                        assert_eq!(s.top(w, v + e), scan(w, v + e), "{id} at ({w}, {})", v + e);
                    }
                }
            }
        }
    }

    fn rule(piece: &str, density: f32) -> ScatterRule {
        ScatterRule {
            piece: piece.into(),
            density,
            lane_clearance: 0.0,
            drift: false,
            vary: 0.0,
        }
    }

    /// Owner report 2026-10-19: a river_straight drew its banks bare of grass until a hard reload.
    /// A ground piece whose GLB failed for good (its pack ready, the piece never in the kit:
    /// `ground_tris` gives it no triangles) must not hold the tile's ground back for the session;
    /// a piece still streaming (`None`) still does.
    #[test]
    fn a_lost_ground_piece_does_not_hold_the_ground_back() {
        let c = catalog();
        let def = c.get("river_straight").unwrap();
        let centre = "river_edge";
        let kit: HashMap<String, Vec<KitPart>> = ground_pieces(def, &c)
            .into_iter()
            .filter(|(id, _)| *id != centre)
            .map(|(id, _)| (id.to_string(), vec![]))
            .collect();
        let pending = PlayScatter::default().ground(def, &c, &kit, &mut |id| {
            if id == centre { None } else { lod0(id) }
        });
        assert!(pending.is_none(), "a streaming centre waits");
        let g = PlayScatter::default()
            .ground(def, &c, &kit, &mut |id| {
                if id == centre { Some(vec![]) } else { lod0(id) }
            })
            .expect("the centre failed for good: the base and edges still make the ground");
        let m = parse_manifest(MANIFEST).unwrap();
        let slots = crate::play::grass::grass_slots(&m, def, &c);
        let blades = crate::play::grass::layout(7, &g.surface, &slots, &[])
            .blades
            .len();
        assert!(blades > 5000, "{blades} blades on the banks");
    }

    #[test]
    fn the_same_seed_gives_the_same_instances() {
        let c = catalog();
        for id in ["forest", "oak", "meadow"] {
            let def = c.get(id).unwrap();
            let g = prepared(def).expect("ground pieces in the kit");
            let a = layout(def, 7, &g);
            assert!(!a.instances.is_empty(), "{id}");
            assert_eq!(a, layout(def, 7, &g), "{id}: same seed");
            // A second preparation of the same ground scatters identically too.
            assert_eq!(a, layout(def, 7, &prepared(def).unwrap()), "{id}");
            let b = layout(def, 8, &g);
            assert_ne!(a.instances, b.instances, "{id}: another seed");
        }
        // Seeded from the world seed and the cell: `PlacedTile::seed`.
        let w = World::new(3);
        let origin = w.tiles()[&Axial::ORIGIN].seed;
        assert_eq!(origin, cell_seed(3, Axial::ORIGIN));
        assert_ne!(
            cell_seed(3, Axial::new(1, 0)),
            cell_seed(4, Axial::new(1, 0))
        );
    }

    /// Every MVP tile, every rotation: rules that declare no scatter get one rule per scatter piece,
    /// so roads, rivers, beaches, the ocean and the house pad are exercised too. Each instance's
    /// centre and footprint ring, turned with the tile, stand on its piece's floor in the tile's
    /// turned ground, never on an excluded slot, clear of heroes, lanes and the border.
    #[test]
    fn no_instance_on_an_excluded_slot_for_every_mvp_tile_and_rotation() {
        let c = catalog();
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for base in c.tiles() {
            let mut def = base.clone();
            if def.props.scatter.is_empty() {
                def.props.scatter = vec![
                    rule("tree_forest", 2.0),
                    rule("rock_small", 1.0),
                    rule("wildflower_clump", 3.0),
                ];
            }
            let g = prepared(&def).unwrap_or_else(|| panic!("{}: ground pieces missing", def.id));
            let local: Vec<(Tri, String)> = ground_pieces(&def, &c)
                .into_iter()
                .flat_map(|(id, k)| {
                    lod0(id)
                        .unwrap()
                        .into_iter()
                        .map(move |(t, s)| (t.map(|p| turn(k, p)), s))
                })
                .collect();
            let lanes = lanes(&def);
            for k in 0..6u8 {
                let rot = piece_rotation(k);
                let world = Surface::new(
                    local
                        .iter()
                        .map(|(t, s)| (t.map(|p| (rot * Vec3::from(p)).to_array()), s.clone())),
                );
                for seed in 0..4u64 {
                    let l = layout(&def, seed * 7919 + u64::from(k), &g);
                    *counts.entry(def.id.clone()).or_default() += l.instances.len();
                    for inst in &l.instances {
                        let r = &def.props.scatter[inst.rule as usize];
                        let piece = r.piece.as_str();
                        let fp = footprint(inst.piece(piece));
                        let floors = floor_slots(inst.piece(piece));
                        let mut probes = vec![Vec3::ZERO];
                        probes.extend(
                            ring16()
                                .iter()
                                .step_by(2)
                                .map(|d| Vec3::new(d.x, 0.0, d.y) * fp.ground),
                        );
                        for off in probes {
                            let p = rot * (inst.pos + off);
                            let (h, slot) = world.at(p.x, p.z).unwrap_or_else(|| {
                                panic!("{} k{k}: {piece} over no ground at {p}", def.id)
                            });
                            assert!(
                                floor_ok(floors, slot),
                                "{} k{k} seed {seed}: {piece} at {} over {slot}",
                                def.id,
                                inst.pos
                            );
                            // A water prop stands on the water, not the bed
                            // (`water_props_stand_on_the_water`).
                            if off == Vec3::ZERO && !water_prop(piece) {
                                assert!(
                                    inst.pos.y <= h + 1e-4 && inst.pos.y > h - 0.1,
                                    "{} {piece}: y {} vs surface {h}",
                                    def.id,
                                    inst.pos.y
                                );
                            }
                        }
                        let p = inst.pos.xz();
                        assert!(hex_reach(p) <= border_reach(fp) + 0.2, "{} border", def.id);
                        // Every piece a hero slot may draw at any cell is cleared, a fence by
                        // its line.
                        for line in fence_lines(&def) {
                            let need = fence_clearance(fp.ground * inst.scale);
                            assert!(
                                polyline_distance(p, &line) >= need - 1e-4,
                                "{} fence",
                                def.id
                            );
                        }
                        for (i, h) in def.props.hero.iter().enumerate() {
                            for piece in def.hero_options(i).filter(|p| fence(p).is_none()) {
                                let need = hero_clearance(footprint(piece), fp);
                                assert!(
                                    p.distance(Vec2::from(h.at)) >= need - 0.2,
                                    "{} hero {piece}",
                                    def.id
                                );
                            }
                        }
                        for (n, (line, half)) in def.networks.iter().zip(&lanes) {
                            if n.kind == NetworkKind::River
                                && (bank_band(piece).is_some() || water_prop(piece))
                            {
                                continue;
                            }
                            let need = lane_clearance(r, *half, fp);
                            assert!(polyline_distance(p, line) >= need - 0.2, "{} lane", def.id);
                        }
                    }
                }
            }
        }
        assert_eq!(counts.len(), 31, "every inventory tile ran");
        for id in [
            "forest",
            "oak",
            "windmill",
            "farm",
            "wheat_field",
            "meadow",
            "road_straight",
            "house_cluster",
            "beach",
            "beach_inner",
            "beach_outer",
            "river_mouth",
            "river_mouth_l",
            "river_straight",
            "river_bend",
            "river_bend_60",
            "river_fork",
            "ocean",
        ] {
            assert!(counts[id] > 0, "{id} scatters: {counts:?}");
        }
    }

    /// Density is per 100 m² of the hex cap: the mean count over cells matches it within three
    /// standard errors of the per-tile rounding (plus 3%), every
    /// tile places its whole planned count, and same-piece neighbours keep the rule's spacing: the
    /// blue-noise spacing over its ground (a drift's share of it), never under two ground reaches
    /// ([`DRIFT_PACK`] in a drift).
    #[test]
    fn density_is_per_100_m2_of_cap_and_spacing_is_blue_noise() {
        let c = catalog();
        for id in ["forest", "oak", "meadow"] {
            let def = c.get(id).unwrap();
            let g = prepared(def).unwrap();
            let seeds = 64u64;
            let mut sums = vec![0usize; def.props.scatter.len()];
            for seed in 0..seeds {
                let l = layout(
                    def,
                    cell_seed(1, Axial::new(seed as i32, -(seed as i32))),
                    &g,
                );
                for (ri, r) in def.props.scatter.iter().enumerate() {
                    let want = r.density * CAP_AREA / 100.0;
                    let n = l.placed(ri);
                    assert_eq!(
                        n as u32,
                        l.planned[ri],
                        "{id} {}: shortfall",
                        r.piece.as_str()
                    );
                    assert!((n as f32 - want).abs() < 1.0, "{id}: {n} vs {want}");
                    sums[ri] += n;
                    let fp = group_footprint(r.piece.as_str());
                    let pts: Vec<Vec2> = l
                        .instances
                        .iter()
                        .filter(|i| i.rule as usize == ri)
                        .map(|i| i.pos.xz())
                        .collect();
                    let share = if r.drift { DRIFT_SHARE } else { 1.0 };
                    let even = BLUE_NOISE * (share * l.area[ri] / n.max(1) as f32).sqrt();
                    let pack = if r.drift { DRIFT_PACK } else { 2.0 };
                    let want = even.max(pack * fp.ground).max(CROWN_SPACING * fp.crown);
                    assert!(
                        (l.spacing[ri] - want).abs() < 0.05,
                        "{id}: spacing {} vs {want}",
                        l.spacing[ri]
                    );
                    let spacing = l.spacing[ri];
                    for (a, p) in pts.iter().enumerate() {
                        for q in &pts[a + 1..] {
                            assert!(p.distance(*q) >= spacing - 1e-4, "{id}: too close");
                        }
                    }
                }
            }
            for (ri, r) in def.props.scatter.iter().enumerate() {
                // A tile plans floor(want + u), so its count is Bernoulli on want's fraction: the
                // tolerance is three standard errors of the 64-seed mean (plus 3% of the density),
                // relative to how sparse the rule is, so a rare rule that places half or none fails.
                let want = r.density * CAP_AREA / 100.0;
                let f = want.fract();
                let se = (f * (1.0 - f) / seeds as f32).sqrt() * 100.0 / CAP_AREA;
                let tol = 3.0 * se + 0.03 * r.density;
                let mean = sums[ri] as f32 / seeds as f32 / CAP_AREA * 100.0;
                assert!(
                    (mean - r.density).abs() <= tol,
                    "{id} {}: {mean} per 100 m² vs {} (tolerance {tol})",
                    r.piece.as_str(),
                    r.density
                );
                if want * seeds as f32 >= 3.0 {
                    assert!(sums[ri] > 0, "{id} {}: never placed", r.piece.as_str());
                }
            }
        }
    }

    /// The nearest-neighbour distances of a forest's trees vary: Poisson-disc, not a lattice.
    #[test]
    fn spacing_is_not_a_grid() {
        let c = catalog();
        let def = c.get("forest").unwrap();
        let l = layout(def, 11, &prepared(def).unwrap());
        let trees: Vec<Vec2> = l
            .instances
            .iter()
            .filter(|i| i.rule == rule_of(def, "tree_forest"))
            .map(|i| i.pos.xz())
            .collect();
        let nn: Vec<f32> = trees
            .iter()
            .enumerate()
            .map(|(a, p)| {
                trees
                    .iter()
                    .enumerate()
                    .filter(|(b, _)| *b != a)
                    .map(|(_, q)| p.distance(*q))
                    .fold(f32::MAX, f32::min)
            })
            .collect();
        let mean = nn.iter().sum::<f32>() / nn.len() as f32;
        let sd = (nn.iter().map(|d| (d - mean).powi(2)).sum::<f32>() / nn.len() as f32).sqrt();
        // A lattice spreads ~0%; dense Poisson-disc packing (20 spruces on the litter) still spreads
        // several per cent.
        assert!(
            sd > 0.05 * mean,
            "nearest-neighbour spread {sd} of mean {mean}"
        );
        let yaws: HashSet<u32> = l.instances.iter().map(|i| i.yaw.to_bits()).collect();
        assert_eq!(
            yaws.len(),
            l.instances.len(),
            "every instance has its own yaw"
        );
        assert!(l.instances.iter().all(|i| (0.85..=1.15).contains(&i.scale)));
    }

    /// Undo and redo (and bulldoze then undo) bring a tile back with the seed it had, so it
    /// scatters identically.
    #[test]
    fn undo_and_redo_restore_an_identical_scatter() {
        let c = catalog();
        let def = c.get("forest").unwrap();
        let g = prepared(def).unwrap();
        let mut w = World::new(42);
        let cell = Axial::new(1, 0);
        let k = w
            .auto_rotate(&c, cell, def)
            .expect("a forest fits next to the meadow");
        w.place(&c, cell, "forest", k).unwrap();
        let scatter = |w: &World| {
            let t = &w.tiles()[&cell];
            assert_eq!(t.seed, cell_seed(w.seed(), cell));
            layout(c.get(&t.def).unwrap(), cell_seed(w.seed(), cell), &g)
        };
        let first = scatter(&w);
        assert!(w.undo());
        assert!(!w.tiles().contains_key(&cell));
        assert!(w.redo());
        assert_eq!(scatter(&w), first, "redo");
        w.bulldoze(cell).unwrap();
        assert!(w.undo());
        assert_eq!(scatter(&w), first, "undo of a bulldoze");
    }

    /// PRD §6 Variety: a rule's instances draw every piece of its group over a few tiles, the same
    /// seed draws the same mix, and every variant is a known piece of its family with a footprint.
    #[test]
    fn instances_draw_every_variant_of_their_rule() {
        let c = catalog();
        for (id, rule) in [("forest", 0), ("forest", 1), ("meadow", 0), ("meadow", 2)] {
            let def = c.get(id).unwrap();
            let g = prepared(def).unwrap();
            let piece = def.props.scatter[rule].piece.as_str();
            let mut seen = std::collections::BTreeSet::new();
            for seed in 0..12u64 {
                let l = layout(def, seed, &g);
                assert_eq!(l, layout(def, seed, &g), "{id}: same seed");
                for i in l.instances.iter().filter(|i| i.rule as usize == rule) {
                    seen.insert(i.piece(piece).to_owned());
                }
            }
            let want: std::collections::BTreeSet<String> =
                group(piece).map(str::to_owned).collect();
            assert_eq!(seen, want, "{id} {piece}");
        }
        for &(id, variants) in VARIANTS {
            for v in variants {
                assert_eq!(family(v), id);
                assert!(
                    FOOTPRINTS.iter().any(|(f, _)| f == v),
                    "{v} has no footprint"
                );
                assert_eq!(floor_slots(v), floor_slots(id));
            }
        }
    }

    /// Weighted groups: every [`VARIANT_WEIGHTS`] entry names a group and weighs each of its
    /// pieces above 0 (a new variant without a weight would never be drawn), and `bush_verge` is
    /// the most common verge bush, about half the picks (owner, 2026-10-20).
    #[test]
    fn weighted_groups_draw_by_their_weights() {
        for &(id, weights) in VARIANT_WEIGHTS.iter().chain(VERGE_VARIANT_WEIGHTS) {
            assert_eq!(weights.len(), group(id).count(), "{id}: a weight per piece");
            assert!(weights.iter().all(|&w| w > 0), "{id}: {weights:?}");
        }
        let c = catalog();
        let meadow = c.get("meadow").unwrap();
        let road = c.get("road_straight").unwrap();
        let mut counts = [0u32; 3];
        for seed in 0..64u64 {
            for n in 0..64 {
                counts[usize::from(pick_variant(road, "bush_verge", seed, 3, n))] += 1;
            }
        }
        let total = counts.iter().sum::<u32>() as f32;
        let share = counts.map(|c| c as f32 / total);
        assert!((0.46..0.54).contains(&share[0]), "{share:?}");
        assert!(share[1] > 0.2 && share[2] > 0.2, "{share:?}");
        assert!(share[0] > share[1] + 0.15 && share[0] > share[2] + 0.15);
        let poppy_share = |def: &TileDef| {
            let poppies = (0..64u64)
                .flat_map(|seed| {
                    (0..64).map(move |n| pick_variant(def, "wildflower_single", seed, 0, n))
                })
                .filter(|&v| variant_piece("wildflower_single", v) == "wildflower_single_e")
                .count();
            poppies as f32 / 4096.0
        };
        let share = poppy_share(meadow);
        assert!((0.08..0.12).contains(&share), "meadow poppy share {share}");
        let share = poppy_share(road);
        assert!((0.22..0.28).contains(&share), "verge poppy share {share}");
    }

    /// More poppies on the road and rail verges (owner A9-Q4, 2026-10-20): every tile with a road
    /// or rail lane and single wildflowers draws them by [`VERGE_VARIANT_WEIGHTS`], 1 poppy in 4;
    /// every other tile keeps the meadow's 1 in 10, and the meadow draws exactly the picks it drew
    /// before per-tile weights (a digest of 4096 picks under the old weights `[9, 9, 9, 9, 4]`).
    #[test]
    fn verges_draw_more_poppies_and_the_meadow_is_unchanged() {
        let c = catalog();
        let verges: Vec<&str> = c
            .tiles()
            .iter()
            .filter(|d| {
                d.props
                    .scatter
                    .iter()
                    .any(|r| r.piece.as_str() == "wildflower_single")
            })
            .filter(|d| verge_tile(d))
            .map(|d| d.id.as_str())
            .collect();
        for id in [
            "road_straight",
            "road_bend_60",
            "road_bend_120",
            "road_junction",
            "rail_straight",
            "rail_bend_60",
            "rail_bend_120",
            "rail_junction",
            "rail_road_crossing",
            "rail_station",
        ] {
            assert!(verges.contains(&id), "{id} is a verge tile: {verges:?}");
        }
        assert_eq!(verges.len(), 10, "{verges:?}");
        for d in c.tiles() {
            let want = if verge_tile(d) {
                [3, 3, 3, 3, 4]
            } else {
                [9, 9, 9, 9, 4]
            };
            assert_eq!(
                variant_weights(d, "wildflower_single"),
                Some(&want[..]),
                "{}",
                d.id
            );
            assert_eq!(
                variant_weights(d, "bush_verge"),
                Some(&[2, 1, 1][..]),
                "{}",
                d.id
            );
        }
        let meadow = c.get("meadow").unwrap();
        let mut digest = 0u64;
        for seed in 0..64u64 {
            for n in 0..64 {
                let v = pick_variant(meadow, "wildflower_single", seed, 0, n);
                digest = digest.wrapping_mul(31).wrapping_add(u64::from(v));
            }
        }
        assert_eq!(
            digest, 8_349_994_037_374_194_218,
            "the meadow's picks moved"
        );
    }

    /// A `vary` rule (the meadow's single wildflowers) stands where its seeded noise is high more
    /// often than its ground does on average, so the field's density varies gently, never in a grid.
    #[test]
    fn single_flowers_thin_by_their_seeded_noise() {
        let c = catalog();
        let def = c.get("meadow").unwrap();
        let (ri, r) = def
            .props
            .scatter
            .iter()
            .enumerate()
            .find(|(_, r)| r.piece.as_str() == "wildflower_single")
            .unwrap();
        assert!(r.vary > 0.0 && !r.drift);
        let g = prepared(def).unwrap();
        let (mut at, mut ground) = (0.0, 0.0);
        for seed in 0..16u64 {
            let l = layout(def, seed, &g);
            let pts: Vec<Vec2> = l
                .instances
                .iter()
                .filter(|i| i.rule as usize == ri)
                .map(|i| i.pos.xz())
                .collect();
            assert_eq!(pts.len() as u32, l.planned[ri]);
            at += pts.iter().map(|p| vary_noise(seed, *p)).sum::<f32>() / pts.len() as f32;
            let cells = &g.masks[ri].cells;
            ground += cells
                .iter()
                .map(|&k| {
                    let k = k as usize;
                    vary_noise(
                        seed,
                        Vec2::new(cell_centre(k % CELLS), cell_centre(k / CELLS)),
                    )
                })
                .sum::<f32>()
                / cells.len() as f32;
        }
        let (at, ground) = (at / 16.0, ground / 16.0);
        assert!(
            at > ground + 0.03,
            "noise at flowers {at} vs ground {ground}"
        );
    }

    /// The footprint table matches the kit: ground reach is the collider's, crown reach the widest
    /// LOD's (within 0.1 m, never under).
    /// The wheat field's ground (scatter, grass and the walker all stand on it) is its ploughed
    /// earth inside the field and the meadow cap on the grass margin; the wheat cards are never
    /// ground (`NOT_GROUND`), and the meadow cap never pokes through the earth.
    #[test]
    fn the_wheat_field_ground_is_its_earth_not_its_wheat() {
        let def = catalog().get("wheat_field").unwrap().clone();
        let g = prepared(&def).expect("wheat_field ground in the kit");
        let field = |p: Vec2| (0..3).map(|e| p.dot(hex_dir(e)).abs()).fold(0.0, f32::max);
        let (mut earth, mut cap) = (0, 0);
        for j in -60..=60 {
            for i in -60..=60 {
                let p = Vec2::new(i as f32 * 0.25, j as f32 * 0.25);
                let reach = field(p);
                let Some((_, slot)) = g.surface.at(p.x, p.y) else {
                    continue;
                };
                assert_ne!(slot, "wheat_field_wheat", "wheat is ground at {p}");
                if reach < 10.3 {
                    assert_eq!(slot, "wheat_field_soil", "at {p}");
                    earth += 1;
                } else if reach > 10.9 && reach < 13.5 {
                    assert_eq!(slot, "meadow_base_cap", "at {p}");
                    cap += 1;
                }
            }
        }
        assert!(earth > 4000 && cap > 1500, "{earth} {cap}");
        // LOD1 and LOD2 draw from 150 m and 400 m, base and centre switching together (one
        // origin): the meadow cap never pokes through the field's earth there either.
        for lod in ["lod1", "lod2"] {
            let mut all = vec![];
            for (id, k) in ground_pieces(&def, &catalog()) {
                for (t, s) in glb_mesh_tris(id, lod).unwrap() {
                    all.push((t.map(|p| turn(k, p)), s));
                }
            }
            let s = Surface::new(all);
            for j in -40..=40 {
                for i in -40..=40 {
                    let p = Vec2::new(i as f32 * 0.25, j as f32 * 0.25);
                    if field(p) < 10.3 {
                        let (_, slot) = s.at(p.x, p.y).unwrap();
                        assert_eq!(slot, "wheat_field_soil", "{lod} at {p}");
                    }
                }
            }
        }
        // The wildflowers and rocks stand on the margin only.
        let l = layout(&def, 1, &g);
        assert!(!l.instances.is_empty());
        for inst in &l.instances {
            assert!(field(inst.pos.xz()) > 10.8, "{:?}", inst.pos);
        }
    }

    /// A hero variant stands inside the authored piece's ground box and reach, so the clearances
    /// sized for the authored piece (hex-sim's parking boxes, the scatter masks) hold for it. A
    /// round piece (a tree, no ground box) keeps inside its ground and crown reach.
    #[test]
    fn hero_variants_fit_inside_the_authored_piece() {
        for &(piece, variants) in hex_core::HERO_VARIANTS {
            for v in variants {
                // A building's ground box nests in the authored one's; a disc piece (the oak, the
                // hay) has no box, only its reach.
                assert_eq!(ground_box(piece).is_some(), ground_box(v).is_some(), "{v}");
                if let (Some(b), Some(vb)) = (ground_box(piece), ground_box(v)) {
                    assert!(
                        vb.min[0] >= b.min[0]
                            && vb.min[1] >= b.min[1]
                            && vb.max[0] <= b.max[0]
                            && vb.max[1] <= b.max[1],
                        "{v} {vb:?} outside {piece} {b:?}"
                    );
                }
                assert!(footprint(v).ground <= footprint(piece).ground, "{v}");
                assert!(footprint(v).crown <= footprint(piece).crown, "{v}");
            }
        }
    }

    /// Owner D3 (2026-10-20): on every coast tile the driftwood and marram clumps stand on the dry
    /// sand inside their [`beach_band`], their whole footprint [`BEACH_BORDER`] inside the border
    /// and clear of the river mouth's channel; every planned instance is placed, the driftwood lies
    /// along the coast and the clumps thin seaward. Prints the mean count per tile.
    #[test]
    fn beach_dressing_stays_on_its_dry_sand_band() {
        let c = catalog();
        for id in [
            "beach",
            "beach_inner",
            "beach_outer",
            "river_mouth",
            "river_mouth_l",
        ] {
            let def = c.get(id).unwrap();
            let g = prepared(def).unwrap();
            let coast = Coastline::of(def, 0).unwrap();
            let lanes = lanes(def);
            let (mut near, mut far) = (0usize, 0usize);
            let mut per_rule = vec![0usize; def.props.scatter.len()];
            let mut area = vec![];
            let mut short = vec![];
            let seeds = 24u64;
            for seed in 0..seeds {
                let l = layout(def, cell_seed(7, Axial::new(seed as i32, 0)), &g);
                area.clone_from(&l.area);
                for (ri, r) in def.props.scatter.iter().enumerate() {
                    let piece = r.piece.as_str();
                    if bank_band(piece).is_some() {
                        // The mouths' bank props: `bank_props_stand_in_their_band`.
                        continue;
                    }
                    assert!(beach_band(piece).is_some(), "{id}: {piece}");
                    if l.placed(ri) as u32 != l.planned[ri] {
                        short.push(format!(
                            "{id} {piece}: {} of {}",
                            l.placed(ri),
                            l.planned[ri]
                        ));
                    }
                    per_rule[ri] += l.placed(ri);
                }
                for inst in &l.instances {
                    let r = &def.props.scatter[inst.rule as usize];
                    let piece = r.piece.as_str();
                    let Some(band) = beach_band(piece) else {
                        continue;
                    };
                    let fp = group_footprint(piece);
                    let p = inst.pos.xz();
                    let s = coast.seaward(p);
                    assert!(
                        s >= band.near - 0.2 && s <= band.far + 0.2,
                        "{id}: {piece} at s {s}"
                    );
                    assert!(
                        s + fp.ground <= DAMP_TOP_S + 0.2,
                        "{id}: {piece} footprint on the damp band at s {s}"
                    );
                    assert!(
                        hex_reach(p) <= APOTHEM - fp.ground - BEACH_BORDER + 0.2,
                        "{id}: {piece} border"
                    );
                    for (line, half) in &lanes {
                        let need = lane_clearance(r, *half, fp);
                        assert!(
                            polyline_distance(p, line) >= need - 0.2,
                            "{id}: {piece} lane"
                        );
                    }
                    if band.along {
                        let t = match coast {
                            Coastline::Straight { sea } => sea.perp(),
                            Coastline::Arc { centre, .. } => (p - centre).perp(),
                        }
                        .normalize();
                        let axis = Vec2::new(inst.yaw.cos(), -inst.yaw.sin());
                        assert!(
                            axis.dot(t).abs() >= DRIFT_YAW_JITTER.cos() - 1e-3,
                            "{id}: {piece} lies across the coast"
                        );
                    } else if s < 0.5 * (band.near + band.far) {
                        near += 1;
                    } else {
                        far += 1;
                    }
                }
            }
            assert!(
                near > far,
                "{id}: clumps thin seaward ({near} near, {far} far)"
            );
            let mean: Vec<f32> = per_rule.iter().map(|&n| n as f32 / seeds as f32).collect();
            let pieces: Vec<&str> = def.props.scatter.iter().map(|r| r.piece.as_str()).collect();
            println!(
                "{id}: mean per tile {mean:?} for {pieces:?} on {area:?} m² ({near} near, {far} far)"
            );
            assert!(short.is_empty(), "shortfall: {short:?}");
        }
    }

    /// Owner 2026-10-21: "the beaches have too much going on, every piece does not need to have all
    /// scatter items, 0 to 6 is plenty". Every beach-family tile draws 0 to [`BEACH_MAX`] dressing
    /// pieces, the same for the same seed, each standing exactly where the full dart set put it,
    /// and the count varies over seeds. Prints how often each count occurs.
    #[test]
    fn beach_tiles_draw_zero_to_six_dressing_pieces() {
        let c = catalog();
        let mut family: Vec<&str> = c
            .tiles()
            .iter()
            .filter(|d| beach_family(d))
            .map(|d| d.id.as_str())
            .collect();
        family.sort_unstable();
        assert_eq!(
            family,
            [
                "beach",
                "beach_inner",
                "beach_outer",
                "river_mouth",
                "river_mouth_l"
            ]
        );
        for id in family {
            let def = c.get(id).unwrap();
            let g = prepared(def).unwrap();
            let mut seen = [0u32; BEACH_MAX + 1];
            for s in 0..64 {
                let seed = cell_seed(11, Axial::new(s, -s / 2));
                let l = layout(def, seed, &g);
                let n = l.instances.len();
                assert!(n <= BEACH_MAX, "{id} seed {seed}: {n} pieces");
                seen[n] += 1;
                assert_eq!(l, layout(def, seed, &g), "{id}: same seed, same beach");
                let all = every_dart(def, seed, &g);
                assert_eq!(n, beach_count(seed).min(all.instances.len()), "{id}");
                for inst in &l.instances {
                    assert!(
                        all.instances.iter().any(|a| Instance {
                            order: 0,
                            ..inst.clone()
                        } == *a),
                        "{id}: a kept piece moved"
                    );
                }
                for ri in 0..def.props.scatter.len() {
                    assert_eq!(l.planned[ri], l.placed(ri) as u32, "{id}: planned is kept");
                }
            }
            println!("{id}: tiles per count 0..=6 over 64 seeds {seen:?}");
            assert!(
                seen.iter().filter(|&&k| k > 0).count() >= 3,
                "{id}: the count varies {seen:?}"
            );
        }
    }

    /// Owner 2026-10-26: "riverbank scatter is too concentrated and repetitive, change scatter to
    /// 0-4 things", and "scatter type items like rocks and reeds should not be built into the
    /// tile". Every river channel tile (with no centre piece), both river bridges (whose centre is
    /// only the deck) and the spring (granite and lilies) is a [`river_family`] tile and draws 0 to [`RIVER_MAX`] bank props, the same for the same seed, each standing where the full dart
    /// set put it. Over seeds every count 0 to 4 occurs, both kinds and at least four of the five
    /// pieces turn up, and no two tiles with props share a layout. Prints the count histogram.
    #[test]
    fn river_tiles_draw_zero_to_four_bank_props() {
        let c = catalog();
        let mut family: Vec<&str> = c
            .tiles()
            .iter()
            .filter(|d| river_family(d))
            .map(|d| d.id.as_str())
            .collect();
        family.sort_unstable();
        assert_eq!(
            family,
            [
                "river_bend",
                "river_bend_60",
                "river_fork",
                "river_rail_bridge",
                "river_road_bridge",
                "river_spring",
                "river_straight"
            ]
        );
        for id in family {
            let def = c.get(id).unwrap();
            assert!(
                def.center.is_none() || id.ends_with("_bridge") || id == "river_spring",
                "{id}: the banks are scatter, not a centre"
            );
            let g = prepared(def).unwrap();
            let mut seen = [0u32; RIVER_MAX + 1];
            let mut pieces = HashSet::new();
            let mut layouts = HashSet::new();
            let mut with_props = 0;
            for s in 0..64 {
                let seed = cell_seed(13, Axial::new(s, -s / 2));
                let full = layout(def, seed, &g);
                assert_eq!(full, layout(def, seed, &g), "{id}: same seed, same bank");
                let l = bank_only(def, &full);
                let n = l.instances.len();
                assert!(n <= RIVER_MAX, "{id} seed {seed}: {n} props");
                seen[n] += 1;
                let all = bank_only(def, &every_dart(def, seed, &g));
                assert!(
                    all.instances.len() >= RIVER_MAX,
                    "{id} seed {seed}: room for 4"
                );
                assert_eq!(n, river_count(seed).min(all.instances.len()), "{id}");
                for inst in &l.instances {
                    assert!(
                        all.instances.iter().any(|a| Instance {
                            order: 0,
                            ..inst.clone()
                        } == *a),
                        "{id}: a kept prop moved"
                    );
                    let piece = inst.piece(def.props.scatter[inst.rule as usize].piece.as_str());
                    pieces.insert(piece.to_owned());
                }
                if n > 0 {
                    with_props += 1;
                    let key: Vec<[i32; 3]> = l
                        .instances
                        .iter()
                        .map(|i| (i.pos * 100.0).round().as_ivec3().to_array())
                        .collect();
                    layouts.insert(key);
                }
            }
            println!("{id}: tiles per count 0..=4 over 64 seeds {seen:?}, pieces {pieces:?}");
            assert!(
                seen.iter().all(|&k| k > 0),
                "{id}: every count occurs {seen:?}"
            );
            assert_eq!(layouts.len(), with_props, "{id}: no two banks alike");
            assert!(pieces.len() >= 4, "{id}: the variants vary {pieces:?}");
            assert!(
                pieces.iter().any(|p| p.starts_with("river_stones"))
                    && pieces.iter().any(|p| p.starts_with("river_reeds")),
                "{id}: stones and reeds {pieces:?}"
            );
        }
    }

    /// Owner A22 Q2 (2026-10-26): reeds and stone clusters about half and half (the densities
    /// were 0.75 stones to 0.6 reeds, a 43% reed share over 1024 seeds). Over 64 seeds every
    /// [`river_family`] tile's kept bank props are 35% to 65% reed clumps, and over 1024 seeds 45%
    /// to 55%. Prints each tile's share and its histogram of reeds per tile.
    #[test]
    fn river_tiles_draw_reeds_and_stones_about_half_and_half() {
        let c = catalog();
        let mut ids: Vec<&str> = c
            .tiles()
            .iter()
            .filter(|d| river_family(d))
            .map(|d| d.id.as_str())
            .collect();
        ids.sort_unstable();
        for id in ids {
            let def = c.get(id).unwrap();
            let g = prepared(def).unwrap();
            for (seeds, band) in [(64, 0.35..=0.65), (1024, 0.45..=0.55)] {
                let (mut reeds, mut stones) = (0u32, 0u32);
                let mut reeds_per_tile = [0u32; RIVER_MAX + 1];
                for s in 0..seeds {
                    let seed = cell_seed(13, Axial::new(s, -s / 2));
                    let l = layout(def, seed, &g);
                    let mut r = 0;
                    for inst in &l.instances {
                        let rule = def.props.scatter[inst.rule as usize].piece.as_str();
                        match family(inst.piece(rule)) {
                            "river_reeds" => r += 1,
                            "river_stones" => stones += 1,
                            "lily_cluster" | "reed_bed" => {}
                            other => panic!("{id}: {other} is not a bank prop"),
                        }
                    }
                    reeds += r;
                    reeds_per_tile[r as usize] += 1;
                }
                let share = f64::from(reeds) / f64::from(reeds + stones);
                println!(
                    "{id} over {seeds} seeds: {reeds} reeds, {stones} stones, reed share {:.0}%, tiles per reed count 0..=4 {reeds_per_tile:?}",
                    share * 100.0
                );
                assert!(
                    band.contains(&share),
                    "{id} over {seeds} seeds: reed share {share:.2} ({reeds} reeds, {stones} stones)"
                );
            }
        }
    }

    /// The mixed scatter world (`docs/checkpoints/scatter/scatter-mixed.json`, seed 1) draws every
    /// water prop, as `check.mjs` asserts: the bend at -3,2 keeps a lily_cluster, the spring at -4,3
    /// a lily_cluster_c, and the bend at 0,3 (below the spring at 0,2) a lily_cluster_b and a reed bed.
    #[test]
    fn the_mixed_world_draws_every_water_prop() {
        let c = catalog();
        for (id, q, r, want) in [
            ("river_bend", -3, 2, &["lily_cluster", "lily_cluster"][..]),
            ("river_spring", -4, 3, &["lily_cluster_c"][..]),
            ("river_bend", 0, 3, &["lily_cluster_b", "reed_bed"][..]),
        ] {
            let def = c.get(id).unwrap();
            let l = layout(def, cell_seed(1, Axial::new(q, r)), &prepared(def).unwrap());
            let water: Vec<&str> = l
                .instances
                .iter()
                .map(|i| i.piece(def.props.scatter[i.rule as usize].piece.as_str()))
                .filter(|p| water_prop(p))
                .collect();
            assert_eq!(water, want, "{id} at {q},{r}");
        }
    }

    /// `l` without its water props ([`water_prop`]).
    fn bank_only(def: &TileDef, l: &Layout) -> Layout {
        let mut out = l.clone();
        out.instances
            .retain(|i| !water_prop(def.props.scatter[i.rule as usize].piece.as_str()));
        out
    }

    /// An instance with its rise rank cleared, for comparing places across layouts.
    fn placed_as(i: &Instance) -> Instance {
        Instance {
            order: 0,
            ..i.clone()
        }
    }

    /// Owner A26 (2026-10-27): the lily clusters and reed beds are listed by the spring and the 120°
    /// bend alone (not the straights, forks, mouths, bridges, beaches or ocean), after their bank
    /// props, and adding them leaves every bank prop where it stood and the bank cap as it was: over
    /// 256 seeds each tile's bank props, kept and darted, equal the same tile's without the water
    /// rules, instance for instance.
    #[test]
    fn water_props_leave_the_bank_props_as_they_were() {
        let c = catalog();
        let mut listed: Vec<&str> = c
            .tiles()
            .iter()
            .filter(|d| d.props.scatter.iter().any(|r| water_prop(r.piece.as_str())))
            .map(|d| d.id.as_str())
            .collect();
        listed.sort_unstable();
        assert_eq!(listed, ["river_bend", "river_spring"]);
        for id in listed {
            let def = c.get(id).unwrap();
            let first = def
                .props
                .scatter
                .iter()
                .position(|r| water_prop(r.piece.as_str()))
                .unwrap();
            assert!(
                def.props.scatter[first..]
                    .iter()
                    .all(|r| water_prop(r.piece.as_str())),
                "{id}: water props after the bank props"
            );
            assert!(river_family(def), "{id}: still capped as a riverbank");
            let mut dry = def.clone();
            dry.props.scatter.truncate(first);
            let (g, gd) = (prepared(def).unwrap(), prepared(&dry).unwrap());
            for s in 0..256 {
                let seed = cell_seed(29, Axial::new(s, 1 - s));
                for (with, without) in [
                    (
                        bank_only(def, &layout(def, seed, &g)),
                        layout(&dry, seed, &gd),
                    ),
                    (
                        bank_only(def, &every_dart(def, seed, &g)),
                        every_dart(&dry, seed, &gd),
                    ),
                ] {
                    let a: Vec<Instance> = with.instances.iter().map(placed_as).collect();
                    let b: Vec<Instance> = without.instances.iter().map(placed_as).collect();
                    assert_eq!(a, b, "{id} seed {seed}: the bank props moved");
                    assert_eq!(with.planned[..first], without.planned[..], "{id}");
                }
            }
        }
    }

    /// Each tile keeps a seeded 0 to [`WATER_MAX`] water props ([`water_count`], a stream of its
    /// own), each standing where the full dart set put it; over seeds every count occurs and the
    /// three lily clusters and the reed bed all turn up. Prints placed against planned darts per
    /// rule and the kept histogram.
    #[test]
    fn water_props_keep_their_own_zero_to_two() {
        let c = catalog();
        for id in ["river_spring", "river_bend"] {
            let def = c.get(id).unwrap();
            let g = prepared(def).unwrap();
            let rules: Vec<usize> = (0..def.props.scatter.len())
                .filter(|&r| water_prop(def.props.scatter[r].piece.as_str()))
                .collect();
            let mut seen = [0u32; WATER_MAX + 1];
            let mut pieces = HashSet::new();
            let (mut planned, mut darted) = (vec![0u32; rules.len()], vec![0usize; rules.len()]);
            let seeds = 256;
            for s in 0..seeds {
                let seed = cell_seed(31, Axial::new(s, -s));
                let l = layout(def, seed, &g);
                let all = every_dart(def, seed, &g);
                let water: Vec<&Instance> = l
                    .instances
                    .iter()
                    .filter(|i| rules.contains(&(i.rule as usize)))
                    .collect();
                let room: usize = rules.iter().map(|&r| all.placed(r)).sum();
                assert_eq!(water.len(), water_count(seed).min(room), "{id} seed {seed}");
                seen[water.len()] += 1;
                for inst in water {
                    assert!(
                        all.instances.iter().any(|a| placed_as(inst) == *a),
                        "{id}: a kept water prop moved"
                    );
                    let rule = def.props.scatter[inst.rule as usize].piece.as_str();
                    pieces.insert(inst.piece(rule).to_owned());
                }
                for (k, &r) in rules.iter().enumerate() {
                    planned[k] += all.planned[r];
                    darted[k] += all.placed(r);
                }
            }
            let names: Vec<&str> = rules
                .iter()
                .map(|&r| def.props.scatter[r].piece.as_str())
                .collect();
            println!(
                "{id} over {seeds} seeds: rules {names:?} planned {planned:?} placed {darted:?}, \
                 tiles per kept count 0..=2 {seen:?}, pieces {pieces:?}"
            );
            assert!(seen.iter().all(|&k| k > 0), "{id}: every count {seen:?}");
            for p in [
                "lily_cluster",
                "lily_cluster_b",
                "lily_cluster_c",
                "reed_bed",
            ] {
                assert!(pieces.contains(p), "{id}: {p} turns up {pieces:?}");
            }
        }
    }

    /// Every water prop stands where approval A26 puts it, at its largest footprint and scale: a lily
    /// cluster's pads at [`LILY_Y`], its every rim, half-rim and centre point over the bed under the
    /// water at rest (4.0 m from a river centreline less its radius) with the bed topmost, so no
    /// built-in spring pad lies under it; a reed bed at [`WATER_REST`] over a bed 0.4 to 0.8 m deep,
    /// its -z up the slope; both [`WATER_BORDER`] inside the border and, on the spring, within
    /// [`SPRING_REACH`] of the node.
    #[test]
    fn water_props_stand_on_the_water() {
        let c = catalog();
        for id in ["river_spring", "river_bend"] {
            let def = c.get(id).unwrap();
            let g = prepared(def).unwrap();
            let springs = spring_nodes(def);
            assert_eq!(springs.len(), usize::from(id == "river_spring"), "{id}");
            let pads: Vec<Vec2> = g
                .surface
                .tris
                .iter()
                .filter(|(_, _, s)| g.surface.slots[*s as usize] == "river_lily")
                .flat_map(|(t, _, _)| t.iter().map(|p| Vec2::new(p[0], p[2])))
                .collect();
            assert_eq!(
                pads.is_empty(),
                id != "river_spring",
                "{id}: the spring's own pads"
            );
            let (mut lilies, mut reeds) = (0, 0);
            for s in 0..128 {
                let all = every_dart(def, cell_seed(37, Axial::new(s, 3)), &g);
                for inst in &all.instances {
                    let rule = def.props.scatter[inst.rule as usize].piece.as_str();
                    if !water_prop(rule) {
                        continue;
                    }
                    let piece = inst.piece(rule);
                    let r = footprint(piece).ground * inst.scale;
                    let p = inst.pos.xz();
                    assert!(
                        hex_reach(p) + r <= APOTHEM - WATER_BORDER + 0.02,
                        "{id}: {piece} at {p} within 1.5 m of the edge"
                    );
                    let (h, slot) = g.surface.at(p.x, p.y).unwrap();
                    if lily(rule) {
                        lilies += 1;
                        assert_eq!(inst.pos.y, LILY_Y, "{id}: pads on the water");
                        let mut probes = vec![p];
                        for d in ring16() {
                            probes.push(p + r * d);
                            probes.push(p + 0.5 * r * d);
                        }
                        for q in probes {
                            let (y, s) = g.surface.at(q.x, q.y).unwrap();
                            assert!(
                                s == WATER_FLOOR && y < WATER_REST,
                                "{id}: {piece} at {p} reaches {s} at {y}"
                            );
                        }
                        for &v in &pads {
                            assert!(
                                v.distance(p) > 0.9 * r,
                                "{id}: {piece} at {p} over a built-in pad at {v}"
                            );
                        }
                        for n in &springs {
                            assert!(n.distance(p) + r <= SPRING_REACH + 1e-3, "{id}: spring");
                        }
                    } else {
                        reeds += 1;
                        assert_eq!(inst.pos.y, WATER_REST, "{id}: reed bed at the water");
                        assert!(
                            (REED_BED_BAND.0..=REED_BED_BAND.1).contains(&h),
                            "{id}: reed bed over {slot} at {h}"
                        );
                        let up = Vec2::new(
                            g.surface.at(p.x + 0.5, p.y).unwrap().0
                                - g.surface.at(p.x - 0.5, p.y).unwrap().0,
                            g.surface.at(p.x, p.y + 0.5).unwrap().0
                                - g.surface.at(p.x, p.y - 0.5).unwrap().0,
                        )
                        .normalize();
                        let minus_z = Vec2::new(-inst.yaw.sin(), -inst.yaw.cos());
                        assert!(
                            minus_z.dot(up) >= DRIFT_YAW_JITTER.cos() - 1e-3,
                            "{id}: reed bed faces the bank"
                        );
                        let q = inst.transform().transform_point(Vec3::new(0.0, 0.0, -1.0));
                        let along = (q.xz() - p).dot(up) / inst.scale;
                        assert!(along > DRIFT_YAW_JITTER.cos() - 1e-3, "{id}: -z is uphill");
                        for n in &springs {
                            assert!(n.distance(p) <= SPRING_REACH + 1e-3, "{id}: spring");
                        }
                    }
                }
            }
            println!("{id}: {lilies} lily clusters and {reeds} reed beds darted over 128 seeds");
            assert!(lilies > 64, "{id}: {lilies} lily clusters");
            assert!(reeds > 0, "{id}: {reeds} reed beds");
        }
    }

    /// Every bank prop stands on the bank of its band ([`bank_band`]: the surface height under its
    /// centre, which is how far from the centreline it stands), on the [`BANK_FLOORS`], at that
    /// height, and a stone cluster lies along the bank's contour within [`DRIFT_YAW_JITTER`].
    #[test]
    fn bank_props_stand_in_their_band() {
        let c = catalog();
        for id in [
            "river_straight",
            "river_bend",
            "river_bend_60",
            "river_fork",
            "river_mouth",
            "river_mouth_l",
            "river_road_bridge",
            "river_rail_bridge",
            "river_spring",
        ] {
            let def = c.get(id).unwrap();
            let g = prepared(def).unwrap();
            let mut n = 0;
            for s in 0..32 {
                let all = every_dart(def, cell_seed(17, Axial::new(s, 0)), &g);
                for inst in &all.instances {
                    let rule = def.props.scatter[inst.rule as usize].piece.as_str();
                    let Some((low, high)) = bank_band(rule) else {
                        continue;
                    };
                    n += 1;
                    let p = inst.pos.xz();
                    let (h, slot) = g.surface.at(p.x, p.y).unwrap();
                    assert!(BANK_FLOORS.contains(&slot), "{id}: {rule} on {slot}");
                    assert!(
                        (inst.pos.y - h).abs() < 1e-4,
                        "{id}: {rule} at the bank's height"
                    );
                    assert!(
                        h >= low && h <= high,
                        "{id}: {rule} at height {h}, band {low}..{high}"
                    );
                    if rule == "river_stones" {
                        let t = contour(&g.surface, p).unwrap().normalize();
                        let axis = Vec2::new(inst.yaw.cos(), -inst.yaw.sin());
                        assert!(
                            axis.dot(t).abs() >= DRIFT_YAW_JITTER.cos() - 1e-3,
                            "{id}: stones across the bank"
                        );
                    }
                }
            }
            assert!(n > 32, "{id}: {n} bank props over 32 seeds");
        }
    }

    /// Owner 2026-10-26 ("0 to 6 is plenty" stands): the beach pebble clusters count inside a coast
    /// tile's [`BEACH_MAX`], drawn like any other dressing piece. Prints the mean kept per tile of
    /// each piece, and the share of tiles that keep a pebble cluster.
    #[test]
    fn beach_pebbles_count_inside_the_beach_cap() {
        let c = catalog();
        for id in ["beach", "beach_inner", "beach_outer"] {
            let def = c.get(id).unwrap();
            let g = prepared(def).unwrap();
            let rule = def
                .props
                .scatter
                .iter()
                .position(|r| r.piece.as_str() == "beach_pebbles")
                .unwrap_or_else(|| panic!("{id}: a beach_pebbles rule"));
            // The pebbles come last among the beach dressing; only the inland granite boulders
            // (owner A25), band-limited on a coast, follow them.
            let inland = def
                .props
                .scatter
                .iter()
                .filter(|r| family(r.piece.as_str()) == "boulder_granite")
                .count();
            assert_eq!(
                rule + 1 + inland,
                def.props.scatter.len(),
                "{id}: pebbles last"
            );
            let seeds = 256;
            let mut kept = vec![0usize; def.props.scatter.len()];
            let (mut with_pebbles, mut darts) = (0, 0);
            for s in 0..seeds {
                let seed = cell_seed(13, Axial::new(s, 2 - s));
                let l = layout(def, seed, &g);
                assert!(l.instances.len() <= BEACH_MAX, "{id}");
                for (ri, k) in kept.iter_mut().enumerate() {
                    *k += l.placed(ri);
                }
                with_pebbles += usize::from(l.placed(rule) > 0);
                darts += every_dart(def, seed, &g).placed(rule);
            }
            let mean: Vec<f32> = kept.iter().map(|&n| n as f32 / seeds as f32).collect();
            println!(
                "{id}: mean kept per tile {mean:?}, a pebble cluster on {with_pebbles} of {seeds} \
                 tiles, {} clusters darted per tile",
                darts as f32 / seeds as f32
            );
            assert!(with_pebbles > 0, "{id}: some tile keeps a pebble cluster");
        }
    }

    /// Owner 2026-10-26: the ocean's rocks and pebble clusters are scatter on its floor, standing
    /// at the bed's contract level, so the floor stays within its 0.10 m upward allowance (water
    /// contract §4): every seabed instance's top (its piece's highest vertex in any LOD, times its
    /// scale) stays within [`SEABED_ALLOWANCE`] of the bed, it never stands above the floor, and
    /// its footprint keeps [`SEABED_BORDER`] inside the border. Every rule places its whole count.
    #[test]
    fn seabed_stones_stay_within_the_ocean_floor_allowance() {
        let c = catalog();
        let def = c.get("ocean").unwrap();
        let g = prepared(def).unwrap();
        let top = |id: &str| {
            (0..3)
                .flat_map(|l| glb_mesh_tris(id, &format!("lod{l}")).unwrap())
                .flat_map(|(t, _)| t)
                .map(|p| p[1])
                .fold(f32::MIN, f32::max)
        };
        let mut per_rule = vec![0usize; def.props.scatter.len()];
        let mut highest = 0.0f32;
        let seeds = 32;
        for s in 0..seeds {
            let l = layout(def, cell_seed(5, Axial::new(s, 1 - s)), &g);
            for inst in &l.instances {
                let piece = inst.piece(def.props.scatter[inst.rule as usize].piece.as_str());
                assert!(seabed(piece), "{piece}");
                assert_eq!(inst.pos.y, SEABED_Y, "{piece}");
                let p = inst.pos.xz();
                let (floor, slot) = g.surface.at(p.x, p.y).unwrap();
                assert_eq!(slot, SEABED_FLOOR);
                assert!(inst.pos.y <= floor + 1e-4, "{piece} above the floor");
                let rise = inst.pos.y + inst.scale * top(piece) - SEABED_Y;
                highest = highest.max(rise);
                assert!(
                    rise <= SEABED_ALLOWANCE,
                    "{piece}: top {rise} m over the bed"
                );
                assert!(
                    hex_reach(p) + footprint(piece).ground * inst.scale
                        <= APOTHEM - SEABED_BORDER + 1e-3,
                    "{piece} within {SEABED_BORDER} m of the border"
                );
                per_rule[inst.rule as usize] += 1;
            }
            for ri in 0..def.props.scatter.len() {
                assert_eq!(l.placed(ri) as u32, l.planned[ri], "ocean: shortfall");
            }
        }
        let mean: Vec<f32> = per_rule.iter().map(|&n| n as f32 / seeds as f32).collect();
        println!("ocean: mean per tile {mean:?}, highest top {highest} m over the bed");
        assert!(per_rule.iter().all(|&n| n > 0), "{per_rule:?}");
    }

    #[test]
    fn footprints_match_the_kit() {
        let reach = |tris: &[(Tri, String)]| {
            tris.iter()
                .flat_map(|(t, _)| t.iter())
                .map(|p| Vec2::new(p[0], p[2]).length())
                .fold(0.0, f32::max)
        };
        for &(id, f) in FOOTPRINTS {
            let ground = reach(&glb_mesh_tris(id, "collider").unwrap());
            assert!(
                f.ground >= ground && f.ground <= ground + 0.1,
                "{id} ground {ground}"
            );
            if f.crown > 0.0 {
                let crown = (0..3)
                    .map(|l| reach(&glb_mesh_tris(id, &format!("lod{l}")).unwrap()))
                    .fold(0.0, f32::max);
                assert!(
                    f.crown >= crown && f.crown <= crown + 0.1,
                    "{id} crown {crown}"
                );
            }
        }
    }

    /// Each [`FENCES`] entry is its GLB: the collider has one disjoint box per run, each centred
    /// on its run, and LOD0 has one post per entry, a square of that half width (its foot, buried
    /// below y -0.3, leaning at most 0.022 rad) centred on the entry. A fence has no [`FOOTPRINTS`]
    /// entry.
    #[test]
    fn fences_match_the_kit() {
        for &(id, f) in FENCES {
            assert!(
                FOOTPRINTS.iter().all(|(p, _)| *p != id),
                "{id} in FOOTPRINTS"
            );
            let runs = f.line.len() - 1;
            let walk = crate::walk::PieceShape::from_meshes(
                id,
                &glb_mesh_tris(id, "collider")
                    .unwrap()
                    .iter()
                    .flat_map(|(t, _)| t.iter().map(|p| Vec3::from(*p)))
                    .collect::<Vec<_>>(),
                &[],
            );
            assert_eq!(walk.parts.len(), runs, "{id}: one collider part per run");
            for w in f.line.windows(2) {
                let (a, b) = (Vec2::from(w[0]), Vec2::from(w[1]));
                let part = walk.parts.iter().find(|p| {
                    let c = p.hull.iter().copied().sum::<Vec2>() / p.hull.len() as f32;
                    polyline_distance(c, &[a, b]) < 0.2
                });
                assert!(part.is_some(), "{id}: no collider part on run {a}..{b}");
            }
            // The post feet: LOD0 corners at or below y -0.3 (0.3 m under the meadow relief, which
            // is at most 0), grouped by the nearest entry.
            let feet: Vec<Vec2> = glb_mesh_tris(id, "lod0")
                .unwrap()
                .iter()
                .flat_map(|(t, _)| t.iter())
                .filter(|p| p[1] <= -0.3 + 1e-4)
                .map(|p| Vec2::new(p[0], p[2]))
                .collect();
            let mut per: Vec<Vec<Vec2>> = vec![vec![]; f.posts.len()];
            for q in &feet {
                let (k, d) = f
                    .posts
                    .iter()
                    .enumerate()
                    .map(|(k, (at, _))| (k, q.distance(Vec2::from(*at))))
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .unwrap();
                assert!(d < 0.2, "{id}: a buried corner {q} off every post");
                per[k].push(*q);
            }
            for ((at, half), pts) in f.posts.iter().zip(&mut per) {
                // Each corner once (triangles repeat them).
                pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
                pts.dedup_by(|a, b| a.distance(*b) < 1e-3);
                assert_eq!(
                    pts.len(),
                    4,
                    "{id}: post at {at:?} is not a square: {pts:?}"
                );
                let c = pts.iter().copied().sum::<Vec2>() / pts.len() as f32;
                // The foot is 0.3 m (DROP) below the ground, so a lean moves it under 1 cm.
                assert!(
                    c.distance(Vec2::from(*at)) < 0.02,
                    "{id} post {at:?} at {c}: {pts:?}"
                );
                let r = pts.iter().map(|p| p.distance(c)).fold(0.0, f32::max);
                assert!(
                    (r - half * std::f32::consts::SQRT_2).abs() < 0.01,
                    "{id} post {at:?}: corner reach {r}, half {half}"
                );
            }
        }
    }

    /// The paddock fence moves no farm scatter that stands clear of its line: with and without the
    /// fence hero, a seed lays out the same instances but those on the line, and none of those
    /// left stands within the line's clearance. The fence drops some, so it does matter.
    #[test]
    fn the_farm_fence_moves_no_scatter_off_its_line() {
        let c = catalog();
        let fenced = c.get("farm").unwrap().clone();
        let mut open = fenced.clone();
        open.props
            .hero
            .retain(|h| fence(h.piece.as_str()).is_none());
        assert_eq!(open.props.hero.len() + 1, fenced.props.hero.len());
        let (gf, go) = (prepared(&fenced).unwrap(), prepared(&open).unwrap());
        for (f, o) in gf.masks.iter().zip(&go.masks) {
            assert_eq!(
                (&f.cells, f.area),
                (&o.cells, o.area),
                "the fence leaves the masks alone"
            );
        }
        let lines = fence_lines(&fenced);
        let mut dropped = 0;
        for seed in 0..16u64 {
            let seed = cell_seed(1, Axial::new(seed as i32, 3 - seed as i32));
            let (a, b) = (layout(&fenced, seed, &gf), layout(&open, seed, &go));
            let clear = |i: &Instance| {
                let piece = i.piece(fenced.props.scatter[i.rule as usize].piece.as_str());
                let need = fence_clearance(footprint(piece).ground * i.scale);
                lines
                    .iter()
                    .all(|l| polyline_distance(i.pos.xz(), l) >= need)
            };
            let kept: Vec<&Instance> = b.instances.iter().filter(|i| clear(i)).collect();
            dropped += b.instances.len() - kept.len();
            assert_eq!(a.instances.len(), kept.len(), "seed {seed}");
            for (x, y) in a.instances.iter().zip(kept) {
                assert_eq!(
                    (x.rule, x.pos, x.yaw, x.scale, x.variant),
                    (y.rule, y.pos, y.yaw, y.scale, y.variant),
                    "seed {seed}"
                );
            }
            assert_eq!(a.planned, b.planned);
        }
        assert!(dropped > 0, "no seed put a flower or rock on the line");
    }

    /// Each [`GROUND_BOXES`] entry is its collider's bounds (to 1 cm) and the collider fills
    /// them (a rectangle with at most chamfered corners), so a box clears no more than the piece.
    #[test]
    fn ground_boxes_match_the_kit() {
        for &(id, b) in GROUND_BOXES {
            let pts: Vec<Vec2> = glb_mesh_tris(id, "collider")
                .unwrap()
                .iter()
                .flat_map(|(t, _)| t.iter().map(|p| Vec2::new(p[0], p[2])))
                .collect();
            let lo = pts.iter().fold(Vec2::MAX, |a, p| a.min(*p));
            let hi = pts.iter().fold(Vec2::MIN, |a, p| a.max(*p));
            assert!(
                lo.distance(Vec2::from(b.min)) < 0.01 && hi.distance(Vec2::from(b.max)) < 0.01,
                "{id} collider bounds {lo}..{hi}"
            );
            let h = crate::walk::hull(&pts);
            let area = (0..h.len())
                .map(|i| h[i].perp_dot(h[(i + 1) % h.len()]))
                .sum::<f32>()
                / 2.0;
            let size = hi - lo;
            assert!(
                area >= 0.95 * size.x * size.y,
                "{id} fills {area} of its {size} box"
            );
        }
    }

    /// A [`GROUND_PROPS`] hero joins the ground in the tile frame, so every one in the catalog
    /// stands at the origin at a whole hex turn, unlifted.
    #[test]
    fn ground_props_stand_at_the_origin_on_a_hex_turn() {
        let c = catalog();
        let mut seen = 0;
        for def in c.tiles() {
            for h in &def.props.hero {
                if GROUND_PROPS.contains(&h.piece.as_str()) {
                    seen += 1;
                    assert_eq!((h.at, h.y), ([0.0, 0.0], 0.0), "{}", def.id);
                    assert_eq!(h.yaw_deg.rem_euclid(60.0), 0.0, "{}", def.id);
                }
            }
        }
        assert!(seen > 0, "the station tile carries its platform");
    }

    /// Station criterion 3 (scatter): the platform is the station tile's ground, so its
    /// wildflowers keep off its whole footprint (its `GROUND_BOXES` box, ramps included) for
    /// any seed: the paving is not their floor.
    #[test]
    fn station_scatter_keeps_off_the_platform() {
        let c = catalog();
        let def = c.get("rail_station").unwrap();
        assert!(
            ground_pieces(def, &c).contains(&("rail_station_platform", 0)),
            "the platform is ground"
        );
        let b = ground_box("rail_station_platform").unwrap();
        let g = prepared(def).unwrap();
        let mut n = 0;
        for seed in 0..16u64 {
            for inst in &layout(def, seed, &g).instances {
                n += 1;
                let fp =
                    footprint(inst.piece(def.props.scatter[inst.rule as usize].piece.as_str()));
                let p = inst.pos.xz();
                assert!(
                    p.x < b.min[0] - fp.ground
                        || p.x > b.max[0] + fp.ground
                        || p.y < b.min[1] - fp.ground
                        || p.y > b.max[1] + fp.ground,
                    "seed {seed}: a flower at {p} on the platform"
                );
            }
        }
        assert!(n > 100, "{n} flowers over 16 seeds");
    }

    /// The sqrt-built directions and turns agree with the trig and `piece_rotation` the renderer
    /// uses, so the exact layout frame is the drawn one.
    #[test]
    fn exact_directions_match_trig_and_piece_rotation() {
        for k in 0..6u8 {
            let a = (60.0 * f32::from(k)).to_radians();
            assert!(
                hex_dir(k).distance(Vec2::new(a.cos(), a.sin())) < 1e-6,
                "edge {k}"
            );
            let p = [3.0, 1.5, -7.0];
            let want = piece_rotation(k) * Vec3::from(p);
            assert!(Vec3::from(turn(k, p)).distance(want) < 1e-5, "turn {k}");
        }
        for (k, d) in ring16().iter().enumerate() {
            let a = TAU * k as f32 / 16.0;
            assert!(d.distance(Vec2::new(a.cos(), a.sin())) < 1e-6, "ring {k}");
        }
    }

    /// Spruces scattered round a hero oak leave its canopy clear, so its perches stay open. No
    /// shipped tile pairs them since the split (U3), so the pine forest is given the oak tile's oak.
    #[test]
    fn spruces_clear_the_oak_canopy() {
        let c = catalog();
        let oak = c.get("oak").unwrap();
        assert!(oak.agents.iter().any(|a| a.agent == AgentKind::Bird));
        let mut def = c.get("forest").unwrap().clone();
        def.props.hero = oak.props.hero.clone();
        let g = prepared(&def).unwrap();
        for seed in 0..16 {
            for i in layout(&def, seed, &g)
                .instances
                .iter()
                .filter(|i| i.rule == rule_of(&def, "tree_forest"))
            {
                assert!(
                    i.pos.xz().length() >= HERO_CROWN * 7.1 + 0.5 - 0.2,
                    "{}",
                    i.pos
                );
            }
        }
    }

    /// The pine forest's undergrowth (shrubs and saplings, forest ref-1) comes after its other rules:
    /// the earlier rules' instances keep their spots, and the shrubs and saplings stand on the
    /// litter with every planned instance placed.
    #[test]
    fn forest_undergrowth_keeps_the_earlier_spots() {
        let c = catalog();
        let pine = c.get("forest").unwrap();
        let n = pine.props.scatter.len();
        assert_eq!(pine.props.scatter[n - 2].piece.as_str(), "shrub_forest");
        assert_eq!(pine.props.scatter[n - 1].piece.as_str(), "sapling_spruce");
        let mut before = pine.clone();
        before.props.scatter.truncate(n - 2);
        let g = prepared(pine).unwrap();
        let (mut planned, mut placed) = ([0u32; 2], [0usize; 2]);
        for seed in 0..32 {
            let l = layout(pine, seed, &g);
            let old = layout(&before, seed, &g);
            let kept: Vec<_> = l
                .instances
                .iter()
                .filter(|i| usize::from(i.rule) < n - 2)
                .collect();
            assert_eq!(kept.len(), old.instances.len(), "seed {seed}");
            for (a, b) in kept.iter().zip(&old.instances) {
                assert_eq!((a.rule, a.pos), (b.rule, b.pos), "seed {seed}");
            }
            for k in 0..2 {
                planned[k] += l.planned[n - 2 + k];
                placed[k] += l.placed(n - 2 + k);
            }
            for i in l.instances.iter().filter(|i| usize::from(i.rule) >= n - 2) {
                let (_, slot) = g.surface.at(i.pos.x, i.pos.z).unwrap();
                assert_eq!(slot, "forest_base_floor", "undergrowth on the litter");
            }
        }
        assert_eq!(
            placed[0], planned[0] as usize,
            "shrubs {placed:?} of {planned:?}"
        );
        assert_eq!(
            placed[1], planned[1] as usize,
            "saplings {placed:?} of {planned:?}"
        );
        assert!(planned[0] > 32 && planned[1] > 32, "{planned:?}");
    }

    /// Verge bushes (road ref-1's "grass verges with tufts and small bushes") come last on the four
    /// road tiles without parking: the wildflowers keep their spots, every planned bush is placed,
    /// and each bush's widest reach over its LODs (its leaves, wider than its collider; LOD2's
    /// scaled-up cards reach furthest), taken per variant, at its scale stays beyond the kerb
    /// (3.25 m from the centreline), never over asphalt, kerb or paint, with its centre inside the
    /// verge band, on every seed; every variant is drawn.
    #[test]
    fn verge_bushes_line_the_road_beyond_the_kerb() {
        const KERB_OUT: f32 = 3.25;
        let leaves: std::collections::BTreeMap<&str, f32> = group("bush_verge")
            .map(|v| {
                let r = (0..3)
                    .flat_map(|l| glb_mesh_tris(v, &format!("lod{l}")).unwrap())
                    .flat_map(|(t, _)| t)
                    .map(|p| Vec2::new(p[0], p[2]).length())
                    .fold(0.0, f32::max);
                (v, r)
            })
            .collect();
        assert!(
            (0.55..0.8).contains(&leaves["bush_verge"]),
            "widest LOD reach {leaves:?}"
        );
        assert!(
            (0.4..0.6).contains(&leaves["bush_verge_b"]),
            "widest LOD reach {leaves:?}"
        );
        assert!(
            (0.5..0.7).contains(&leaves["bush_verge_c"]),
            "widest LOD reach {leaves:?}"
        );
        let mut drawn = std::collections::BTreeSet::new();
        let c = catalog();
        for id in [
            "road_straight",
            "road_bend_60",
            "road_bend_120",
            "road_junction",
        ] {
            let def = c.get(id).unwrap();
            let n = def.props.scatter.len();
            assert_eq!(
                def.props.scatter[n - 1].piece.as_str(),
                "bush_verge",
                "{id}"
            );
            let mut before = def.clone();
            before.props.scatter.truncate(n - 1);
            let g = prepared(def).unwrap();
            let lanes = lanes(def);
            let (mut planned, mut placed) = (0u32, 0usize);
            for seed in 0..32u64 {
                let l = layout(def, seed, &g);
                let old = layout(&before, seed, &g);
                let kept: Vec<_> = l
                    .instances
                    .iter()
                    .filter(|i| usize::from(i.rule) < n - 1)
                    .collect();
                assert_eq!(kept.len(), old.instances.len(), "{id} seed {seed}");
                for (a, b) in kept.iter().zip(&old.instances) {
                    assert_eq!((a.rule, a.pos), (b.rule, b.pos), "{id} seed {seed}");
                }
                planned += l.planned[n - 1];
                placed += l.placed(n - 1);
                for i in l.instances.iter().filter(|i| usize::from(i.rule) == n - 1) {
                    let p = i.pos.xz();
                    let piece = i.piece("bush_verge");
                    drawn.insert(piece);
                    let reach = leaves[piece] * i.scale;
                    let d = lanes
                        .iter()
                        .map(|(line, _)| polyline_distance(p, line))
                        .fold(f32::MAX, f32::min);
                    assert!(
                        d >= KERB_OUT + reach,
                        "{id} seed {seed}: bush at {p} {d} m out"
                    );
                    // The band is tested on raster cell centres; a dart lands within half a cell.
                    let jitter = RASTER * std::f32::consts::FRAC_1_SQRT_2;
                    assert!(
                        d <= VERGE_BAND + jitter,
                        "{id} seed {seed}: bush at {p} {d} m out"
                    );
                    for q in std::iter::once(p).chain(ring16().iter().map(|r| p + reach * *r)) {
                        let (_, slot) = g.surface.at(q.x, q.y).unwrap();
                        assert!(
                            !["road_asphalt", "road_kerb", "road_paint"].contains(&slot),
                            "{id} seed {seed}: bush at {p} over {slot}"
                        );
                    }
                    let (_, slot) = g.surface.at(p.x, p.y).unwrap();
                    assert_eq!(slot, "meadow_base_cap", "{id} seed {seed}: bush at {p}");
                }
            }
            assert_eq!(
                placed, planned as usize,
                "{id}: bushes {placed} of {planned}"
            );
            assert!(planned > 32, "{id}: {planned}");
        }
        assert_eq!(drawn.len(), leaves.len(), "variants drawn {drawn:?}");
    }

    /// No tile with parking carries verge bushes: hex-sim's parking sweep (tests/parking.rs) checks
    /// only each tile's declared obstacle boxes, so a scattered bush could stand in a car's reverse
    /// swing across the verge unseen. A parking tile needs its own bush slots cleared against the
    /// sweep first.
    #[test]
    fn no_verge_bushes_on_parking_tiles() {
        for def in catalog().tiles() {
            if !def.parking.is_empty() {
                assert!(
                    def.props
                        .scatter
                        .iter()
                        .all(|r| verge_band(r.piece.as_str()).is_none()),
                    "{} has parking and verge bushes",
                    def.id
                );
            }
        }
    }

    /// The split (U3): the pine forest is spruces and rocks with no hero and no bird roost; the oak
    /// tile is the hero oak, rocks and a few flowers on forest_base's litter, and the roost. Both
    /// have Forest on every edge.
    #[test]
    fn pine_and_oak_tiles_split_the_forest() {
        let c = catalog();
        let pine = c.get("forest").unwrap();
        let oak = c.get("oak").unwrap();
        for d in [pine, oak] {
            assert_eq!(d.base.as_str(), "forest_base", "{}", d.id);
            assert!(
                d.edges.iter().all(|e| e.kind == SocketKind::Forest),
                "{}",
                d.id
            );
        }
        let pieces = |d: &TileDef| -> Vec<String> {
            d.props
                .scatter
                .iter()
                .map(|r| r.piece.as_str().to_owned())
                .collect()
        };
        assert_eq!(
            pieces(pine),
            [
                "outcrop_granite",
                "boulder_granite",
                "log_fallen",
                "tree_forest",
                "rock_small",
                "fern_clump",
                "shrub_forest",
                "sapling_spruce"
            ]
        );
        assert!(pine.props.hero.is_empty() && pine.agents.is_empty());
        assert_eq!(
            pieces(oak),
            [
                "rock_small",
                "wildflower_clump",
                "tree_oak",
                "boulder_granite"
            ]
        );
        assert_eq!(oak.props.hero.len(), 1);
        assert_eq!(oak.props.hero[0].piece.as_str(), "tree_hero_oak");
        let g = prepared(oak).unwrap();
        let mut flowers = 0;
        for seed in 0..32 {
            let l = layout(oak, seed, &g);
            assert_eq!(l.placed(0), l.planned[0] as usize, "rocks");
            assert_eq!(l.placed(1), l.planned[1] as usize, "flowers");
            flowers += l.placed(1);
            for i in l.instances.iter().filter(|i| i.rule == 1) {
                let (_, slot) = g.surface.at(i.pos.x, i.pos.z).unwrap();
                assert_eq!(slot, "forest_base_floor", "a flower on the litter");
            }
        }
        let mean = flowers as f32 / 32.0;
        assert!((4.0..=8.0).contains(&mean), "a few flowers: {mean}");
    }

    /// Drift rules cluster: over 4 m quadrats of the ground, the meadow's flowers have an index of
    /// dispersion (variance over mean count) well above 1, where the same rule spread evenly stays
    /// below it. The drifts follow the tile's seed, not a fixed pattern.
    #[test]
    fn meadow_flowers_grow_in_clustered_drifts() {
        let c = catalog();
        let meadow = c.get("meadow").unwrap();
        let fl = usize::from(rule_of(meadow, "wildflower_clump"));
        assert!(meadow.props.scatter[fl].drift, "the meadow's flowers drift");
        let rocks = usize::from(rule_of(meadow, "rock_small"));
        assert!(!meadow.props.scatter[rocks].drift, "its rocks do not");
        let mut even = meadow.clone();
        even.props.scatter[fl].drift = false;
        let g = prepared(meadow).unwrap();
        let quad = |x: f32, z: f32| ((x / 4.0).floor() as i32, (z / 4.0).floor() as i32);
        let dispersion = |def: &TileDef, seed: u64| {
            let l = layout(def, seed, &g);
            let mut quads: BTreeMap<(i32, i32), u32> = BTreeMap::new();
            for &k in &g.masks[fl].cells {
                let k = k as usize;
                quads
                    .entry(quad(cell_centre(k % CELLS), cell_centre(k / CELLS)))
                    .or_default();
            }
            for i in l.instances.iter().filter(|i| usize::from(i.rule) == fl) {
                *quads.entry(quad(i.pos.x, i.pos.z)).or_default() += 1;
            }
            let n = quads.len() as f32;
            let mean = quads.values().map(|&v| v as f32).sum::<f32>() / n;
            let var = quads
                .values()
                .map(|&v| (v as f32 - mean).powi(2))
                .sum::<f32>()
                / n;
            var / mean
        };
        let seeds: Vec<u64> = (0..16)
            .map(|k| cell_seed(5, Axial::new(k, 1 - k)))
            .collect();
        let drift = seeds.iter().map(|&s| dispersion(meadow, s)).sum::<f32>() / 16.0;
        let spread = seeds.iter().map(|&s| dispersion(&even, s)).sum::<f32>() / 16.0;
        assert!(
            drift > 1.6,
            "drifts cluster: dispersion {drift} (even {spread})"
        );
        assert!(spread < 1.0, "an even spread does not: dispersion {spread}");
        let (a, b) = (seeds[0], seeds[1]);
        let p = Vec2::new(3.0, -4.0);
        assert_ne!(drift_noise(a, p), drift_noise(b, p), "seeded per tile");
        for k in 0..200 {
            let q = Vec2::new(k as f32 * 0.37 - 30.0, 17.0 - k as f32 * 0.21);
            assert!((0.0..1.0).contains(&drift_noise(a, q)), "{q}");
        }
    }
}
