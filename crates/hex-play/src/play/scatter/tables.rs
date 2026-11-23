//! The per-piece lookups scatter, walking and the grass read (from `data/pieces.ron`: variants and
//! their weights, ground props) and the tables still here: footprints, ground boxes, fences and
//! landforms. A piece missing from a table silently takes its
//! default, so `every_prop_piece_has_a_footprint_row` iterates the catalog instead of the table.

use bevy::prelude::*;
use hex_core::piece::{PieceTable, WalkClass};
use hex_core::{NetworkKind, TileDef};

use super::layout::Rng;

/// Whether `def` is a verge tile: it carries a road or rail lane, so its cap is the verge along it.
pub fn verge_tile(def: &TileDef) -> bool {
    def.networks
        .iter()
        .any(|n| matches!(n.kind, NetworkKind::Road | NetworkKind::Rail))
}

/// The pick weights of `piece`'s group on `def` (PRD §6 Variety, `data/pieces.ron`): its
/// `verge_variant_weights` on a verge tile where it has them, else its `variant_weights`; `None`
/// draws each piece equally.
pub fn variant_weights(def: &TileDef, piece: &str) -> Option<&'static [u32]> {
    let entry = PieceTable::builtin().get(piece)?;
    verge_tile(def)
        .then_some(entry.verge_variant_weights.as_deref())
        .flatten()
        .or(entry.variant_weights.as_deref())
}

/// The scatter variants of `piece` after the piece itself (`data/pieces.ron` `variants`).
fn extra_variants(piece: &str) -> &'static [&'static str] {
    hex_core::piece::scatter_variants(piece)
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
    PieceTable::builtin().family(piece).unwrap_or(piece)
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
        "outcrop_granite_b",
        Footprint {
            ground: 3.92,
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
    // The bakeries: each a building whose collider reach is its corner
    // (`footprints_match_the_kit` measures 4.68 and 4.91 m), and the station shelter (4.77 m).
    (
        "bakery_shop",
        Footprint {
            ground: 4.7,
            crown: 0.0,
        },
    ),
    (
        "bakery_shop_b",
        Footprint {
            ground: 4.92,
            crown: 0.0,
        },
    ),
    (
        "rail_station_shelter",
        Footprint {
            ground: 4.8,
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
pub(super) const DEFAULT_FOOTPRINT: Footprint = Footprint {
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
            let axis = super::super::grass::cos_sin_deg(h.yaw_deg);
            Some(
                f.line
                    .iter()
                    .map(|p| Vec2::from(h.at) + axis * p[0] + axis.perp() * p[1])
                    .collect(),
            )
        })
        .collect()
}

/// Whether `slot` is a slot of a ground piece that stands on the ground rather than being it
/// (`data/pieces.ron` `not_ground`: a crop's row cards, the portals' fern cards, a viaduct's stone
/// and deck track). Nothing stands on it and the walker walks through, or under, it.
pub fn is_not_ground(slot: &str) -> bool {
    PieceTable::builtin().is_not_ground(slot)
}

/// Whether `piece` is a hero prop that is ground, not an obstacle (`data/pieces.ron`
/// `walk: Some(Ground)`: the station platform and the tunnel portals). Each stands at the tile
/// origin at a whole hex turn (`ground_props_stand_at_the_origin_on_a_hex_turn`), so it joins the
/// ground in the tile frame as a centre piece does.
pub fn is_ground_prop(piece: &str) -> bool {
    PieceTable::builtin().is_walk(piece, WalkClass::Ground)
}

/// The [`is_ground_prop`] pieces, in `data/pieces.ron` order.
pub fn ground_props() -> impl Iterator<Item = &'static str> {
    PieceTable::builtin().with_walk(WalkClass::Ground)
}

/// The single-tile landforms (spec 2026-11-13 D9): like a footprint tile's ring cells, they
/// scatter their Meadow rules at a Meadow's density per m² of free ground (`layout::meadow_share`).
pub const LANDFORMS: [&str; 2] = ["hill_meadow", "peak_granite"];
