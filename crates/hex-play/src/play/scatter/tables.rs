//! The per-piece lookups scatter, walking and the grass read from `data/pieces.ron`: variants and
//! their weights, footprints, ground boxes, fences and ground props, and the landform tiles. A piece
//! without a footprint takes the default disc, so `every_prop_piece_has_a_footprint_row` iterates the
//! catalog instead of the table.

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

/// Horizontal reach of a piece in metres (`data/pieces.ron` `footprint`).
pub use hex_core::piece::{Footprint, GroundBox};

/// The reach of `piece`: its `data/pieces.ron` `footprint` (`footprints_match_the_kit` checks
/// them against the GLBs), else [`Footprint::DEFAULT`].
pub fn footprint(piece: &str) -> Footprint {
    PieceTable::builtin().footprint(piece)
}

/// The collider rectangle of `piece` (`data/pieces.ron` `ground_box`, checked against the GLBs by
/// `ground_boxes_match_the_kit`). Every other piece, the windmill's round tower included, is a
/// disc of its [`Footprint::ground`].
pub fn ground_box(piece: &str) -> Option<GroundBox> {
    PieceTable::builtin().get(piece)?.ground_box
}

/// A fence's layout in its own frame (x, z), from its `bake-report.json` `layout` block: the
/// fence line (runs between consecutive points; the gap between the last and first point is the
/// gate opening) and every post's centre and half width. A fence is a line, not a blob: it has no
/// `footprint` (its collider's reach, 14.3 m, would clear the whole yard), and scatter,
/// grass and walk's fallback treat it by these lines and posts instead.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fence {
    pub line: &'static [[f32; 2]],
    pub posts: &'static [([f32; 2], f32)],
}

/// Half width (m) of a fence's runs for clearance: the rails' 0.12 m collider half width plus a
/// lean and bend margin.
pub const FENCE_HALF: f32 = 0.2;

/// The fence layout of `piece` (`data/pieces.ron` `fence`; `fences_match_the_kit` checks it
/// against the GLB: one collider part per run, one LOD0 post per entry).
pub fn fence(piece: &str) -> Option<Fence> {
    let f = PieceTable::builtin().get(piece)?.fence.as_ref()?;
    Some(Fence {
        line: &f.line,
        posts: &f.posts,
    })
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
