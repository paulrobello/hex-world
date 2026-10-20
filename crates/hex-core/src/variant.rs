//! Hero-prop variants (PRD §6 Variety): a tile authors one piece per hero slot, and a placed tile
//! may draw a variant of it in that slot instead. The pick is a pure function of the cell and the
//! slot index, so a save needs no field for it and every consumer (play, ghost, walk, grass,
//! scatter, boids) resolves the same piece through [`hero_piece`].

use crate::{Axial, HeroProp, TileDef};

/// Each authored hero piece with the variants a slot of it may draw instead. A new variant is one
/// more id in its row; a new family is one more row.
pub const HERO_VARIANTS: [(&str, &[&str]); 3] = [
    ("house_cottage", &["house_longhouse"]),
    ("house_twostorey", &["house_twostorey_b"]),
    ("tree_hero_oak", &["tree_hero_oak_b"]),
];

/// One slot in this many draws a variant (the rest keep the authored piece).
pub const VARIANT_ONE_IN: u64 = 3;

/// How a slot's variant is picked. The owner's choice is open (random per cluster or a fixed
/// slot), so switching is a change of [`VARIANT_PICK`] alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VariantPick {
    /// Each slot hashes (cell, slot): about one in [`VARIANT_ONE_IN`] varies, independently.
    PerSlot,
    /// The cell alone is hashed: about one cluster in [`VARIANT_ONE_IN`] draws the variant in
    /// every slot that has one.
    PerCluster,
    /// Slot `n` always draws its first variant; every other slot keeps its authored piece.
    FixedSlot(usize),
}

/// The pick play uses.
pub const VARIANT_PICK: VariantPick = VariantPick::PerSlot;

/// The variants of authored hero piece `piece`, empty for a piece without any.
pub fn hero_variants(piece: &str) -> &'static [&'static str] {
    HERO_VARIANTS
        .iter()
        .find(|(id, _)| *id == piece)
        .map_or(&[], |(_, v)| v)
}

/// SplitMix64 of the packed cell and the slot (explicit integer mixing, so native and wasm agree).
fn slot_hash(cell: Axial, slot: u64) -> u64 {
    let packed = ((cell.q as u32 as u64) << 32) | cell.r as u32 as u64;
    let mut z = packed
        ^ slot
            .wrapping_add(0x5EED_4E50)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The piece authored as `piece` draws in slot `slot` of the tile at `cell` under `pick`.
pub fn pick_piece(piece: &str, cell: Axial, slot: usize, pick: VariantPick) -> &str {
    let variants = hero_variants(piece);
    if variants.is_empty() {
        return piece;
    }
    let h = match pick {
        VariantPick::PerSlot => slot_hash(cell, slot as u64),
        VariantPick::PerCluster => slot_hash(cell, u64::MAX),
        VariantPick::FixedSlot(n) => return if slot == n { variants[0] } else { piece },
    };
    if h % VARIANT_ONE_IN == 0 {
        variants[((h / VARIANT_ONE_IN) % variants.len() as u64) as usize]
    } else {
        piece
    }
}

/// The piece hero slot `slot` of `def` draws when the tile stands at `cell`.
pub fn hero_piece(def: &TileDef, cell: Axial, slot: usize) -> &str {
    pick_piece(
        def.props.hero[slot].piece.as_str(),
        cell,
        slot,
        VARIANT_PICK,
    )
}

impl TileDef {
    /// Each hero prop with the piece it draws when the tile stands at `cell` ([`hero_piece`]).
    pub fn hero_pieces(&self, cell: Axial) -> impl Iterator<Item = (&HeroProp, &str)> {
        self.props
            .hero
            .iter()
            .enumerate()
            .map(move |(i, h)| (h, hero_piece(self, cell, i)))
    }

    /// Every piece hero slot `slot` may draw: the authored one, then its variants.
    pub fn hero_options(&self, slot: usize) -> impl Iterator<Item = &str> {
        let piece = self.props.hero[slot].piece.as_str();
        std::iter::once(piece).chain(hero_variants(piece).iter().copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TileCatalog;

    fn cells() -> impl Iterator<Item = Axial> {
        (-20..20).flat_map(|q| (-20..20).map(move |r| Axial::new(q, r)))
    }

    #[test]
    fn the_pick_is_deterministic_per_cell_and_slot() {
        let c = TileCatalog::builtin().unwrap();
        let def = c.get("house_cluster").unwrap();
        for cell in cells() {
            for slot in 0..def.props.hero.len() {
                assert_eq!(hero_piece(def, cell, slot), hero_piece(def, cell, slot));
                assert_eq!(
                    hero_piece(def, cell, slot),
                    pick_piece(
                        def.props.hero[slot].piece.as_str(),
                        cell,
                        slot,
                        VARIANT_PICK
                    )
                );
            }
        }
        // Pinned picks: a change to the hash moves every placed cluster's houses. The origin (the
        // thumbnail cell) keeps both cottages.
        let at = |q, r, s| hero_piece(def, Axial::new(q, r), s);
        let pinned: Vec<&str> = (0..2)
            .flat_map(|s| [at(0, 0, s), at(1, 0, s), at(2, -1, s)])
            .collect();
        assert_eq!(pinned, PINNED);
    }

    const PINNED: [&str; 6] = [
        "house_cottage",
        "house_cottage",
        "house_cottage",
        "house_cottage",
        "house_longhouse",
        "house_cottage",
    ];

    /// Prints the cells near the origin whose cottage slots draw the longhouse (for checkpoint
    /// scripts): `cargo test -p hex-core longhouse_cells -- --nocapture --ignored`.
    #[test]
    #[ignore]
    fn longhouse_cells() {
        let c = TileCatalog::builtin().unwrap();
        let def = c.get("house_cluster").unwrap();
        for cell in (-4..=4).flat_map(|q| (-4..=4).map(move |r| Axial::new(q, r))) {
            let picks: Vec<&str> = def.hero_pieces(cell).map(|(_, p)| p).collect();
            println!("{} {} {picks:?}", cell.q, cell.r);
        }
    }

    #[test]
    fn some_cottage_slots_draw_the_longhouse_and_most_keep_the_cottage() {
        let c = TileCatalog::builtin().unwrap();
        let def = c.get("house_cluster").unwrap();
        let (mut cottage, mut longhouse) = (0, 0);
        let (mut twostorey, mut twostorey_b) = (0, 0);
        for cell in cells() {
            for (i, (h, piece)) in def.hero_pieces(cell).enumerate() {
                match (h.piece.as_str(), piece) {
                    ("house_cottage", "house_cottage") => cottage += 1,
                    ("house_cottage", "house_longhouse") => longhouse += 1,
                    ("house_twostorey", "house_twostorey") => twostorey += 1,
                    ("house_twostorey", "house_twostorey_b") => twostorey_b += 1,
                    (a, b) => assert_eq!(a, b, "slot {i} at {cell:?}"),
                }
            }
        }
        let share = longhouse as f32 / (cottage + longhouse) as f32;
        assert!((0.28..0.39).contains(&share), "longhouse share {share}");
        assert!(cottage > longhouse);
        let share = twostorey_b as f32 / (twostorey + twostorey_b) as f32;
        assert!((0.28..0.39).contains(&share), "twostorey_b share {share}");
        assert!(twostorey > twostorey_b);
    }

    #[test]
    fn slots_without_variants_are_untouched() {
        let c = TileCatalog::builtin().unwrap();
        for def in c.tiles() {
            for cell in cells().step_by(7) {
                for (h, piece) in def.hero_pieces(cell) {
                    if hero_variants(h.piece.as_str()).is_empty() {
                        assert_eq!(piece, h.piece.as_str(), "{} at {cell:?}", def.id);
                    }
                }
            }
        }
        assert_eq!(hero_variants("barn_red"), &[] as &[&str]);
        assert_eq!(hero_variants("windmill"), &[] as &[&str]);
    }

    /// The oak tile's one hero slot draws the forked oak in about one cell in three, and the
    /// origin (the thumbnail cell) keeps the authored hero oak.
    #[test]
    fn some_oak_tiles_draw_the_forked_oak() {
        let c = TileCatalog::builtin().unwrap();
        let def = c.get("oak").unwrap();
        let (mut oak, mut forked) = (0, 0);
        for cell in cells() {
            match hero_piece(def, cell, 0) {
                "tree_hero_oak" => oak += 1,
                "tree_hero_oak_b" => forked += 1,
                p => panic!("{p} at {cell:?}"),
            }
        }
        let share = forked as f32 / (oak + forked) as f32;
        assert!(
            (0.28..0.39).contains(&share),
            "tree_hero_oak_b share {share}"
        );
        assert_eq!(hero_piece(def, Axial::new(0, 0), 0), "tree_hero_oak");
    }

    #[test]
    fn every_pick_rule_draws_only_authored_pieces_or_their_variants() {
        let cell = Axial::new(3, -2);
        assert_eq!(
            pick_piece("house_cottage", cell, 1, VariantPick::FixedSlot(1)),
            "house_longhouse"
        );
        assert_eq!(
            pick_piece("house_cottage", cell, 0, VariantPick::FixedSlot(1)),
            "house_cottage"
        );
        assert_eq!(
            pick_piece("house_twostorey", cell, 1, VariantPick::FixedSlot(1)),
            "house_twostorey_b"
        );
        assert_eq!(
            pick_piece("house_twostorey", cell, 2, VariantPick::FixedSlot(1)),
            "house_twostorey"
        );
        assert_eq!(
            pick_piece("barn_red", cell, 1, VariantPick::FixedSlot(1)),
            "barn_red"
        );
        // Per cluster: every cottage slot of a cell agrees.
        for cell in cells() {
            assert_eq!(
                pick_piece("house_cottage", cell, 0, VariantPick::PerCluster),
                pick_piece("house_cottage", cell, 1, VariantPick::PerCluster)
            );
        }
    }

    /// A house variant is still a house, so parking (`house_` heroes) and walk's `building` kind
    /// hold for every pick.
    #[test]
    fn variants_keep_their_family() {
        for (piece, variants) in HERO_VARIANTS {
            let family = piece.split('_').next().unwrap();
            for v in variants {
                assert!(v.starts_with(&format!("{family}_")), "{v} of {piece}");
            }
        }
    }
}
