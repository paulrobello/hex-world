//! Hero-prop variants (PRD §6 Variety): a tile authors one piece per hero slot, and a placed tile
//! may draw a variant of it in that slot instead. The pick is a pure function of the cell and the
//! slot index, so a save needs no field for it and every consumer (play, ghost, walk, grass,
//! scatter, boids) resolves the same piece through [`hero_piece`].

use crate::{Axial, HeroProp, TileDef};

/// Each authored hero piece with the variants a slot of it may draw instead. A new variant is one
/// more id in its row; a new family is one more row.
pub const HERO_VARIANTS: &[(&str, &[&str])] = &[
    ("house_cottage", &["house_longhouse", "house_cottage_b"]),
    (
        "house_twostorey",
        &["house_twostorey_b", "house_twostorey_c"],
    ),
    ("tree_hero_oak", &["tree_hero_oak_b", "tree_hero_oak_c"]),
    ("hay_bale_round", &["hay_bale_round_b"]),
    ("hay_bale_stack", &["hay_bale_stack_b"]),
    ("barn_red", &["barn_red_b"]),
    ("bakery_shop", &["bakery_shop_b"]),
    ("windmill", &["windmill_b"]),
    ("animal_cow", &["animal_cow_c"]),
    ("well_stone", &["well_stone_b"]),
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
        "house_cottage_b",
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
        let (mut cottage, mut longhouse, mut cottage_b) = (0, 0, 0);
        let (mut twostorey, mut twostorey_b, mut twostorey_c) = (0, 0, 0);
        for cell in cells() {
            for (i, (h, piece)) in def.hero_pieces(cell).enumerate() {
                match (h.piece.as_str(), piece) {
                    ("house_cottage", "house_cottage") => cottage += 1,
                    ("house_cottage", "house_longhouse") => longhouse += 1,
                    ("house_cottage", "house_cottage_b") => cottage_b += 1,
                    ("house_twostorey", "house_twostorey") => twostorey += 1,
                    ("house_twostorey", "house_twostorey_b") => twostorey_b += 1,
                    ("house_twostorey", "house_twostorey_c") => twostorey_c += 1,
                    // The well slot (its own test below).
                    ("well_stone", "well_stone" | "well_stone_b") => {}
                    (a, b) => assert_eq!(a, b, "slot {i} at {cell:?}"),
                }
            }
        }
        println!(
            "two-storey slot over {} cells: {twostorey} authored, {twostorey_b} _b, {twostorey_c} _c",
            twostorey + twostorey_b + twostorey_c
        );
        // About one cottage slot in three varies, split evenly between the two variants.
        let all = (cottage + longhouse + cottage_b) as f32;
        let share = (longhouse + cottage_b) as f32 / all;
        assert!(
            (0.28..0.39).contains(&share),
            "cottage variant share {share}"
        );
        for (n, v) in [("longhouse", longhouse), ("cottage_b", cottage_b)] {
            let share = v as f32 / all;
            assert!((0.13..0.2).contains(&share), "{n} share {share}");
        }
        // The two-storey slot splits its one in three between _b and _c the same way.
        let all = (twostorey + twostorey_b + twostorey_c) as f32;
        let share = (twostorey_b + twostorey_c) as f32 / all;
        assert!(
            (0.28..0.39).contains(&share),
            "two-storey variant share {share}"
        );
        for (n, v) in [("twostorey_b", twostorey_b), ("twostorey_c", twostorey_c)] {
            let share = v as f32 / all;
            assert!((0.13..0.2).contains(&share), "{n} share {share}");
        }
    }

    /// The house cluster's well slot (hero 3, after the houses) draws the open well_stone_b about
    /// one time in three. Each slot hashes (cell, slot) on its own, so the new slot moved no house
    /// (the cottage test above and `PINNED` hold their picks).
    #[test]
    fn some_house_cluster_wells_are_open() {
        let c = TileCatalog::builtin().unwrap();
        let def = c.get("house_cluster").unwrap();
        assert_eq!(def.props.hero[3].piece.as_str(), "well_stone");
        let (mut covered, mut open) = (0, 0);
        for cell in cells() {
            match hero_piece(def, cell, 3) {
                "well_stone" => covered += 1,
                "well_stone_b" => open += 1,
                p => panic!("{p} at {cell:?}"),
            }
        }
        println!(
            "well slot over {} cells: {covered} well_stone, {open} well_stone_b",
            covered + open
        );
        let share = open as f32 / (covered + open) as f32;
        assert!((0.28..0.39).contains(&share), "well_stone_b share {share}");
        // The thumbnail cell and the review shots' cells (docs/checkpoints/well_stone/play.json).
        assert_eq!(hero_piece(def, Axial::ORIGIN, 3), WELL_AT_ORIGIN);
    }

    const WELL_AT_ORIGIN: &str = "well_stone";

    /// Prints the cells near the origin whose well slot draws well_stone_b (for checkpoint scripts):
    /// `cargo test -p hex-core well_b_cells -- --nocapture --ignored`.
    #[test]
    #[ignore]
    fn well_b_cells() {
        let c = TileCatalog::builtin().unwrap();
        let def = c.get("house_cluster").unwrap();
        for cell in (-4..=4).flat_map(|q| (-4..=4).map(move |r| Axial::new(q, r))) {
            if hero_piece(def, cell, 3) == "well_stone_b" {
                println!("{} {}", cell.q, cell.r);
            }
        }
    }

    /// The farm's hay slots draw their variants about one time in three, and the thumbnail cell
    /// (the origin) draws hay_bale_round_b in the first round slot only.
    #[test]
    fn some_farm_hay_slots_draw_the_hay_variants() {
        let c = TileCatalog::builtin().unwrap();
        let def = c.get("farm").unwrap();
        let (mut authored, mut variant) = (0, 0);
        for cell in cells() {
            for (h, piece) in def.hero_pieces(cell) {
                match (h.piece.as_str(), piece) {
                    ("hay_bale_round", "hay_bale_round_b")
                    | ("hay_bale_stack", "hay_bale_stack_b") => variant += 1,
                    ("barn_red", "barn_red_b") | ("animal_cow", "animal_cow_c") => {}
                    (a, b) => {
                        assert_eq!(a, b);
                        authored += a.starts_with("hay_") as i32;
                    }
                }
            }
        }
        let share = variant as f32 / (authored + variant) as f32;
        assert!((0.28..0.39).contains(&share), "hay variant share {share}");
        let origin: Vec<&str> = def
            .hero_pieces(Axial::ORIGIN)
            .map(|(_, p)| p)
            .filter(|p| p.starts_with("hay_"))
            .collect();
        assert_eq!(
            origin,
            ["hay_bale_stack", "hay_bale_round_b", "hay_bale_round"]
        );
    }

    /// The farm's barn slot (hero 0) draws barn_red_b about one time in three and the thumbnail
    /// cell (the origin) keeps barn_red. Each slot hashes (cell, slot) on its own, so the barn's
    /// row moves no bale or animal (the hay test above pins their picks unchanged).
    #[test]
    fn some_farm_barn_slots_draw_barn_red_b() {
        let c = TileCatalog::builtin().unwrap();
        let def = c.get("farm").unwrap();
        assert_eq!(def.props.hero[0].piece.as_str(), "barn_red");
        let (mut barn, mut barn_b) = (0, 0);
        for cell in cells() {
            for (i, (h, piece)) in def.hero_pieces(cell).enumerate() {
                match (h.piece.as_str(), piece) {
                    ("barn_red", "barn_red") => barn += 1,
                    ("barn_red", "barn_red_b") => barn_b += 1,
                    (a, b) => assert!(
                        i > 0 && (a == b || hero_variants(a).contains(&b)),
                        "slot {i} at {cell:?}: {a} drew {b}"
                    ),
                }
            }
        }
        let share = barn_b as f32 / (barn + barn_b) as f32;
        assert!((0.28..0.39).contains(&share), "barn_red_b share {share}");
        assert_eq!(hero_piece(def, Axial::ORIGIN, 0), "barn_red");
        // Cells the checkpoint shots and the walk test use.
        assert_eq!(hero_piece(def, Axial::new(0, 1), 0), "barn_red_b");
        assert_eq!(hero_piece(def, Axial::new(-1, 0), 0), "barn_red_b");
        assert_eq!(hero_piece(def, Axial::new(1, 0), 0), "barn_red");
    }

    /// The farm's standing cow slot (hero 4) draws the Hereford-coat animal_cow_c about one time
    /// in three, and the thumbnail cell (the origin) keeps animal_cow. The grazing cow (hero 7)
    /// is its own hero with no variant.
    #[test]
    fn some_farm_cow_slots_draw_animal_cow_c() {
        let c = TileCatalog::builtin().unwrap();
        let def = c.get("farm").unwrap();
        assert_eq!(def.props.hero[4].piece.as_str(), "animal_cow");
        let (mut cow, mut cow_c) = (0, 0);
        for cell in cells() {
            match hero_piece(def, cell, 4) {
                "animal_cow" => cow += 1,
                "animal_cow_c" => cow_c += 1,
                p => panic!("{p} at {cell:?}"),
            }
            assert_eq!(hero_piece(def, cell, 7), "animal_cow_b");
        }
        let share = cow_c as f32 / (cow + cow_c) as f32;
        assert!((0.28..0.39).contains(&share), "animal_cow_c share {share}");
        assert_eq!(hero_piece(def, Axial::ORIGIN, 4), "animal_cow");
        // The review shots' cells (docs/checkpoints/animal_cow_c/play.json).
        assert_eq!(hero_piece(def, Axial::new(1, -2), 4), "animal_cow_c");
        assert_eq!(hero_piece(def, Axial::new(1, -1), 4), "animal_cow");
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
        assert_eq!(hero_variants("rock_small"), &[] as &[&str]);
    }

    /// The windmill tile's one hero slot draws the smock mill in about one cell in three, and the
    /// origin (the thumbnail cell) keeps the authored windmill.
    #[test]
    fn some_windmill_tiles_draw_the_smock_mill() {
        let c = TileCatalog::builtin().unwrap();
        let def = c.get("windmill").unwrap();
        let (mut mill, mut smock) = (0, 0);
        for cell in cells() {
            match hero_piece(def, cell, 0) {
                "windmill" => mill += 1,
                "windmill_b" => smock += 1,
                p => panic!("{p} at {cell:?}"),
            }
        }
        let share = smock as f32 / (mill + smock) as f32;
        assert!((0.28..0.39).contains(&share), "windmill_b share {share}");
        assert_eq!(hero_piece(def, Axial::ORIGIN, 0), "windmill");
        // The review shots' cells (docs/checkpoints/windmill_b/play.json).
        assert_eq!(hero_piece(def, Axial::new(0, 1), 0), "windmill_b");
        assert_eq!(hero_piece(def, Axial::new(1, 0), 0), "windmill");
    }

    /// The oak tile's one hero slot varies in about one cell in three, split evenly between the
    /// forked oak and the stag-headed oak, and the origin (the thumbnail cell) keeps the authored
    /// hero oak.
    #[test]
    fn some_oak_tiles_draw_the_forked_or_the_stag_headed_oak() {
        let c = TileCatalog::builtin().unwrap();
        let def = c.get("oak").unwrap();
        let (mut oak, mut forked, mut stag) = (0, 0, 0);
        for cell in cells() {
            match hero_piece(def, cell, 0) {
                "tree_hero_oak" => oak += 1,
                "tree_hero_oak_b" => forked += 1,
                "tree_hero_oak_c" => stag += 1,
                p => panic!("{p} at {cell:?}"),
            }
        }
        println!(
            "oak slot over {} cells: {oak} authored, {forked} _b, {stag} _c",
            oak + forked + stag
        );
        let all = (oak + forked + stag) as f32;
        let share = (forked + stag) as f32 / all;
        assert!((0.28..0.39).contains(&share), "oak variant share {share}");
        for (n, v) in [("tree_hero_oak_b", forked), ("tree_hero_oak_c", stag)] {
            let share = v as f32 / all;
            assert!((0.13..0.2).contains(&share), "{n} share {share}");
        }
        assert_eq!(hero_piece(def, Axial::new(0, 0), 0), "tree_hero_oak");
    }

    /// The bakery tile's one hero slot draws the twin-gabled bakehouse in about one cell in three,
    /// and the origin (the thumbnail cell) keeps the authored bakery_shop.
    #[test]
    fn some_bakery_tiles_draw_the_twin_gabled_bakehouse() {
        let c = TileCatalog::builtin().unwrap();
        let def = c.get("bakery").unwrap();
        let (mut shop, mut shop_b) = (0, 0);
        for cell in cells() {
            match hero_piece(def, cell, 0) {
                "bakery_shop" => shop += 1,
                "bakery_shop_b" => shop_b += 1,
                p => panic!("{p} at {cell:?}"),
            }
        }
        let share = shop_b as f32 / (shop + shop_b) as f32;
        assert!((0.28..0.39).contains(&share), "bakery_shop_b share {share}");
        assert_eq!(hero_piece(def, Axial::ORIGIN, 0), "bakery_shop");
    }

    /// Prints the cells near the origin whose bakery slot draws bakery_shop_b (for checkpoint
    /// scripts): `cargo test -p hex-core bakery_b_cells -- --nocapture --ignored`.
    #[test]
    #[ignore]
    fn bakery_b_cells() {
        let c = TileCatalog::builtin().unwrap();
        let def = c.get("bakery").unwrap();
        for cell in (-4..=4).flat_map(|q| (-4..=4).map(move |r| Axial::new(q, r))) {
            if hero_piece(def, cell, 0) == "bakery_shop_b" {
                println!("{} {}", cell.q, cell.r);
            }
        }
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
            "barn_red_b"
        );
        assert_eq!(
            pick_piece("barn_red", cell, 0, VariantPick::FixedSlot(1)),
            "barn_red"
        );
        assert_eq!(
            pick_piece("windmill", cell, 1, VariantPick::FixedSlot(1)),
            "windmill_b"
        );
        assert_eq!(
            pick_piece("animal_cow", cell, 1, VariantPick::FixedSlot(1)),
            "animal_cow_c"
        );
        assert_eq!(
            pick_piece("rock_small", cell, 1, VariantPick::FixedSlot(1)),
            "rock_small"
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
    /// hold for every pick; a hay variant is still `hay_`, so walk reports it as hay.
    #[test]
    fn variants_keep_their_family() {
        for &(piece, variants) in HERO_VARIANTS {
            let family = piece.split('_').next().unwrap();
            for v in variants {
                assert!(v.starts_with(&format!("{family}_")), "{v} of {piece}");
            }
        }
    }
}
