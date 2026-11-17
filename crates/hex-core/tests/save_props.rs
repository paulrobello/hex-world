//! Property tests for the `.hexworld` decoder (ENH-004): it never panics on arbitrary or mutated
//! bytes, encode/decode round-trips a played world, and a world the decoder accepts stays playable
//! at the coordinate limit — the operations play runs on a resumed world, the ones a `panic =
//! "abort"` build would repeat on every start.
//!
//! Default 256 cases per property; set `PROPTEST_CASES` to raise it locally. Found cases land in
//! `proptest-regressions/` and replay on every run.

mod common;

use std::sync::OnceLock;

use common::worlds::{arb_ops, covered_cells, played_world, q_extent, save_of, translated};
use proptest::prelude::*;

use hex_core::{
    Axial, Orientation, SAVE_FORMAT_VERSION, SAVE_MAGIC, SaveData, SaveError, TileCatalog, World,
};

/// The decoder's cell bound (`save.rs` `CELL_LIMIT`). If the crate's bound ever shrinks below
/// this, P5's translated worlds stop decoding and the test points at the drift.
const CELL_LIMIT: i32 = 1 << 24;

fn catalog() -> &'static TileCatalog {
    static C: OnceLock<TileCatalog> = OnceLock::new();
    C.get_or_init(|| TileCatalog::builtin().expect("the embedded catalog is validated by hex-core"))
}

/// Recomputes the envelope CRC over everything before the trailer, so a mutated file passes the
/// checksum and reaches the payload parse (the thing under test).
fn restamp(bytes: &mut [u8]) {
    let body = bytes.len() - 4;
    let crc = crc32fast::hash(&bytes[..body]);
    bytes[body..].copy_from_slice(&crc.to_le_bytes());
}

/// Rewrites the envelope's declared payload length (u32 LE at bytes 10..14).
fn declare_len(bytes: &mut [u8], payload_len: usize) {
    let len = u32::try_from(payload_len).expect("a mutated payload is far below 4 GiB");
    bytes[10..14].copy_from_slice(&len.to_le_bytes());
}

/// The operations play runs on a resumed world: undo to the stack's end, redo to its end, and
/// `can_bulldoze` on every covered cell. None may panic; results are not checked here.
fn run_play_operations(world: &World, c: &TileCatalog) {
    let mut w = world.clone();
    while w.undo(c) {}
    while w.redo(c) {}
    let cells: Vec<Axial> = w
        .tiles()
        .keys()
        .chain(w.members().keys())
        .copied()
        .collect();
    for cell in cells {
        let _ = w.can_bulldoze(c, cell);
    }
}

proptest! {
    /// P1: decode returns (Ok or Err) for any bytes, and for any bytes behind the valid magic and
    /// version. Only a panic fails.
    #[test]
    fn p1_decode_never_panics(
        bytes in proptest::collection::vec(any::<u8>(), 0..4096),
    ) {
        let c = catalog();
        let _ = SaveData::decode(&bytes, c);
        let mut tagged = SAVE_MAGIC.to_vec();
        tagged.extend_from_slice(&SAVE_FORMAT_VERSION.to_le_bytes());
        tagged.extend_from_slice(&bytes);
        let _ = SaveData::decode(&tagged, c);
    }

    /// P2: a mutated valid file — flipped payload bytes, a truncated payload or an extended one,
    /// length field fixed and CRC restamped so the payload parse runs — answers or refuses, and
    /// never panics.
    #[test]
    fn p2_mutated_files_never_panic(
        seed in any::<u64>(),
        ops in arb_ops(),
        pokes in proptest::collection::vec(any::<u64>(), 1..=8),
        mode in 0u8..3,
    ) {
        let c = catalog();
        let original = save_of(played_world(c, seed, &ops)).encode();
        let body_end = original.len() - 4;
        let payload = 14..body_end;
        let mut bytes = match mode {
            // Flip 1..=8 payload bytes.
            0 => {
                let mut b = original.clone();
                for v in &pokes {
                    let i = payload.start + (*v as usize) % payload.len();
                    b[i] = b[i].wrapping_add(0x5f);
                }
                b
            }
            // Truncate the payload, keeping the envelope consistent.
            1 => {
                let keep = payload.start + (pokes[0] as usize) % payload.len();
                let mut b = original[..keep].to_vec();
                b.extend_from_slice(&[0; 4]);
                declare_len(&mut b, keep - payload.start);
                b
            }
            // Extend the payload with junk, keeping the envelope consistent.
            _ => {
                let mut b = original[..body_end].to_vec();
                b.extend(pokes.iter().map(|v| *v as u8));
                b.extend_from_slice(&[0; 4]);
                declare_len(&mut b, payload.len() + pokes.len());
                b
            }
        };
        restamp(&mut bytes);
        let _ = SaveData::decode(&bytes, c);
    }

    /// P3: encode and decode round-trip a played world exactly.
    #[test]
    fn p3_encode_decode_round_trips(seed in any::<u64>(), ops in arb_ops()) {
        let c = catalog();
        let save = save_of(played_world(c, seed, &ops));
        let bytes = save.encode();
        let back = SaveData::decode(&bytes, c)
            .unwrap_or_else(|e| panic!("a played world must decode: {e}"));
        prop_assert_eq!(back, save);
    }

    /// P4: a mutant that decodes stays playable — undo and redo run to the stack ends and
    /// `can_bulldoze` answers for every covered cell, without panicking. This is the property
    /// that guards the `panic = "abort"` resume loop.
    #[test]
    fn p4_accepted_mutants_stay_playable(
        seed in any::<u64>(),
        ops in arb_ops(),
        pokes in proptest::collection::vec(any::<u64>(), 1..=8),
        mode in 0u8..3,
    ) {
        let c = catalog();
        let original = save_of(played_world(c, seed, &ops)).encode();
        let body_end = original.len() - 4;
        let payload = 14..body_end;
        let mut bytes = match mode {
            0 => {
                let mut b = original.clone();
                for v in &pokes {
                    let i = payload.start + (*v as usize) % payload.len();
                    b[i] = b[i].wrapping_add(0x5f);
                }
                b
            }
            1 => {
                let keep = payload.start + (pokes[0] as usize) % payload.len();
                let mut b = original[..keep].to_vec();
                b.extend_from_slice(&[0; 4]);
                declare_len(&mut b, keep - payload.start);
                b
            }
            _ => {
                let mut b = original[..body_end].to_vec();
                b.extend(pokes.iter().map(|v| *v as u8));
                b.extend_from_slice(&[0; 4]);
                declare_len(&mut b, payload.len() + pokes.len());
                b
            }
        };
        restamp(&mut bytes);
        if let Ok(save) = SaveData::decode(&bytes, c) {
            run_play_operations(&save.world, c);
        }
    }

    /// P5: a played world translated so its covered extent (a mega tile's ring included) touches
    /// `CELL_LIMIT` encodes, decodes, and stays playable: the resume operations plus `fits` on
    /// every frontier slot. The decoder's bound is inclusive, so a ring cell exactly at the
    /// limit is accepted and one past it is refused (that refusal is `save.rs`'s own example).
    #[test]
    fn p5_world_at_the_coordinate_limit_decodes_and_stays_playable(
        seed in any::<u64>(),
        ops in arb_ops(),
        sign in 0u8..2,
    ) {
        let c = catalog();
        let mut w = played_world(c, seed, &ops);
        // Guarantee the mega case in every run: place the massif if this world has none.
        let massif = c.get("massif_granite").expect("a listed id");
        if !w.tiles().values().any(|t| c.get(&t.def).is_some_and(|d| d.is_mega()))
            && let Some((cell, os)) = w.mega_slots(c, massif).into_iter().next()
            && let Some(o) = os.first()
        {
            let _ = w.place(c, cell, "massif_granite", *o);
        }
        let (min_q, max_q) = q_extent(&w, c);
        let dq = if sign == 0 { CELL_LIMIT - max_q } else { -CELL_LIMIT - min_q };
        let at = translated(&w, dq, 0);
        let save = save_of(at);
        let back = SaveData::decode(&save.encode(), c)
            .unwrap_or_else(|e| panic!("a translated world must decode at the limit: {e}"));
        run_play_operations(&back.world, c);
        for cell in back.world.frontier() {
            for id in ["meadow", "road_straight", "massif_granite"] {
                let Some(def) = c.get(id) else { continue };
                let _ = back.world.fits(c, cell, def, Orientation::new(0, 0));
                for o in back.world.legal_orientations(c, cell, def) {
                    let _ = back.world.fits(c, cell, def, o);
                }
            }
        }
        // The one-past refusal, at the translated edge: an anchor whose mega ring leaves the
        // bound must be refused, never accepted with cells outside the limit.
        let ring_edge = covered_cells(&back.world, c).iter().map(|a| a.q).max().unwrap_or(0);
        let beyond = translated(&back.world, CELL_LIMIT - ring_edge + 1, 0);
        match SaveData::decode(&save_of(beyond).encode(), c) {
            Err(SaveError::InvalidWorld(hex_core::WorldIssue::OutOfRange(_))) => {}
            other => panic!("a world whose extent passes the limit must be refused, got {other:?}",),
        }
    }
}
