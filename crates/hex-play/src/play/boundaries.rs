//! Play's field boundaries (B2, `hex_core::boundary`): the hero props a boundary keeps clear of,
//! and the per-tile boundary footprints scatter and grass keep clear of.
//!
//! - **Heroes.** A run or corner arm whose footprint (its half width plus [`HERO_GAP`]) overlaps a
//!   hero prop of either tile it touches is left out: a rectangular building by its collider box
//!   ([`scatter::GROUND_BOXES`]), a fence by its run lines, anything else by its ground disc. Each
//!   hero is the piece its slot draws at that cell, so a variant clears its own footprint.
//! - **Scatter and grass.** A tile's scatter drops the instances whose ground reach lands within a
//!   boundary's footprint ([`clear_of`]); grass clears the wall footprints and the core of the hedge
//!   ones ([`super::grass`]). Both read [`local_segments`]: the footprints of every boundary on the
//!   tile's edges and corners, whichever field tile draws them, in the tile's own frame.
use std::collections::BTreeMap;

use bevy::prelude::*;
use hex_core::{
    Axial, HEX_RADIUS, TileDef,
    boundary::{self, Seg, View},
};

use super::{
    APOTHEM,
    grass::cos_sin_deg,
    scatter::{self, FENCE_HALF},
};

/// Gap (m) between a boundary's footprint and a hero prop's.
pub const HERO_GAP: f32 = 0.2;
/// Gap (m) between a boundary's footprint and a scatter instance's ground reach.
pub const SCATTER_GAP: f32 = 0.1;

/// Turn of a tile at rotation `rot`: tile-local (x, z) to world offsets.
fn turn(rot: u8) -> Vec2 {
    cos_sin_deg(60.0 * (rot % 6) as f32)
}

fn rotate(v: Vec2, axis: Vec2) -> Vec2 {
    axis * v.x + axis.perp() * v.y
}

fn seg_points(s: &Seg) -> impl Iterator<Item = Vec2> + '_ {
    let (a, b) = (Vec2::from(s.a), Vec2::from(s.b));
    let n = ((b - a).length() / 0.2).ceil().max(1.0) as usize;
    (0..=n).map(move |k| a.lerp(b, k as f32 / n as f32))
}

/// Whether `seg` (world) overlaps a hero prop of the tile at `cell`.
pub fn blocked<'a>(
    at: &impl Fn(Axial) -> Option<(&'a TileDef, u8)>,
    cell: Axial,
    seg: &Seg,
) -> bool {
    let Some((def, rot)) = at(cell) else {
        return false;
    };
    let centre = Vec2::from(cell.to_world(HEX_RADIUS));
    let r = turn(rot);
    let to_world = |p: Vec2| centre + rotate(p, r);
    let reach = seg.half + HERO_GAP;
    for (h, piece) in def.hero_pieces(cell) {
        let at = to_world(Vec2::from(h.at));
        let axis = rotate(cos_sin_deg(h.yaw_deg), r);
        if let Some(f) = scatter::fence(piece) {
            let line: Vec<Vec2> = f
                .line
                .iter()
                .map(|p| at + rotate(Vec2::from(*p), axis))
                .collect();
            if seg_points(seg)
                .any(|p| super::scatter::polyline_distance(p, &line) < reach + FENCE_HALF)
            {
                return true;
            }
        } else if let Some(b) = scatter::ground_box(piece) {
            let (min, max) = (Vec2::from(b.min), Vec2::from(b.max));
            let mid = (min + max) / 2.0;
            let half = (max - min) / 2.0;
            let c = at + rotate(mid, axis);
            if seg_points(seg).any(|p| {
                let d = p - c;
                let q = Vec2::new(
                    d.dot(axis).abs() - half.x,
                    d.dot(axis.perp()).abs() - half.y,
                );
                q.max(Vec2::ZERO).length() < reach
            }) {
                return true;
            }
        } else {
            let ground = scatter::footprint(piece).ground;
            if seg.distance(at.into()) < reach + ground {
                return true;
            }
        }
    }
    false
}

/// The boundary pieces field tile `owner` draws, in its frame (`hex_core::boundary::View`); none
/// under `kit_ablate=boundaries`.
pub fn placements<'a>(
    world_seed: u64,
    owner: Axial,
    at: &impl Fn(Axial) -> Option<(&'a TileDef, u8)>,
) -> Vec<boundary::Placement> {
    all_placements(world_seed, [owner], at)
        .remove(&owner)
        .unwrap_or_default()
}

/// The boundary pieces of every field tile among `cells`, through one view, so each farm's gate is
/// worked out once; tiles that draw none are left out. None under `kit_ablate=boundaries`.
pub fn all_placements<'a>(
    world_seed: u64,
    cells: impl IntoIterator<Item = Axial>,
    at: &impl Fn(Axial) -> Option<(&'a TileDef, u8)>,
) -> BTreeMap<Axial, Vec<boundary::Placement>> {
    if hex_render::kit_instances::kit_ablate().boundaries {
        return BTreeMap::new();
    }
    let view = View::new(world_seed, at, |c: Axial, s: &Seg| blocked(at, c, s));
    cells
        .into_iter()
        .filter(|c| at(*c).is_some_and(|(d, _)| boundary::FIELD_TILES.contains(&d.id.as_str())))
        .map(|c| (c, view.placements(c)))
        .filter(|(_, p)| !p.is_empty())
        .collect()
}

/// The boundary footprints on `cell`'s edges and corners in `cell`'s tile frame (x, z), from
/// every field tile's placements (`all`, [`all_placements`]): its own and its neighbours'.
pub fn local_segments_from<'a>(
    all: &BTreeMap<Axial, Vec<boundary::Placement>>,
    cell: Axial,
    at: &impl Fn(Axial) -> Option<(&'a TileDef, u8)>,
) -> Vec<Seg> {
    let Some((_, rot)) = at(cell) else {
        return vec![];
    };
    let centre = Vec2::from(cell.to_world(HEX_RADIUS));
    let back = cos_sin_deg(-60.0 * (rot % 6) as f32);
    let local = |p: [f32; 2]| -> [f32; 2] { rotate(Vec2::from(p) - centre, back).into() };
    boundary::within(cell, 1)
        .filter_map(|o| all.get(&o))
        .flatten()
        .flat_map(|p| p.segs.iter().copied())
        .filter(|s| {
            let near = |p: [f32; 2]| Vec2::from(p).distance(centre) <= HEX_RADIUS + 0.5;
            near(s.a) && near(s.b) && s.distance(centre.into()) < APOTHEM + s.half + 0.5
        })
        .map(|s| Seg {
            a: local(s.a),
            b: local(s.b),
            half: s.half,
        })
        .collect()
}

/// Every boundary footprint on `cell`'s edges and corners, in `cell`'s tile frame (x, z), drawn by
/// whichever field tile owns it (all within one cell of it).
pub fn local_segments<'a>(
    world_seed: u64,
    cell: Axial,
    at: &impl Fn(Axial) -> Option<(&'a TileDef, u8)>,
) -> Vec<Seg> {
    let all = all_placements(world_seed, boundary::within(cell, 1), at);
    local_segments_from(&all, cell, at)
}

/// Whether a scatter instance at tile-local `p` of ground reach `reach` stays clear of `segs`.
pub fn clear_of(segs: &[Seg], p: Vec2, reach: f32) -> bool {
    segs.iter()
        .all(|s| s.distance(p.into()) >= s.half + reach + SCATTER_GAP)
}

/// FNV-1a over the footprints' bits: equal boundaries, equal value (0 for none).
pub fn fingerprint(segs: &[Seg]) -> u64 {
    if segs.is_empty() {
        return 0;
    }
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for s in segs {
        for w in [s.a[0], s.a[1], s.b[0], s.b[1], s.half] {
            for b in w.to_bits().to_le_bytes() {
                h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01B3);
            }
        }
    }
    h
}

/// A placement's transform under its owner's root.
pub fn transform(p: &boundary::Placement) -> Transform {
    Transform::from_xyz(p.at[0], 0.0, p.at[1])
        .with_rotation(Quat::from_rotation_y(-p.yaw_deg.to_radians()))
}

#[cfg(test)]
pub(crate) mod tests {
    use hex_core::{TileCatalog, World};

    use super::*;

    /// The review world (docs/checkpoints/walls-hedges/review.json): a farm east of the start
    /// meadow with land on five sides and a wheat field on the sixth.
    pub(crate) const REVIEW: [(i32, i32, &str, u8); 6] = [
        (1, 0, "farm", 0),
        (0, 1, "forest", 0),
        (1, 1, "oak", 0),
        (2, 0, "wheat_field", 0),
        (2, -1, "meadow", 0),
        (1, -1, "meadow", 0),
    ];

    fn world(tiles: &[(i32, i32, &str, u8)]) -> (TileCatalog, World) {
        let c = TileCatalog::builtin().unwrap();
        let mut w = World::new(1);
        for &(q, r, id, rot) in tiles {
            w.place(&c, Axial::new(q, r), id, hex_core::Orientation::new(rot, 0))
                .unwrap_or_else(|e| panic!("{id} at {q},{r}: {e:?}"));
        }
        (c, w)
    }

    /// Scatter and grass on both sides of a boundary see it: the land tile across a farm edge gets
    /// the same footprints in its own frame (on its border, about an apothem out), and a scatter
    /// instance over one is not clear of it while one a metre further in is.
    #[test]
    fn both_tiles_see_the_boundary_on_their_shared_edge() {
        let mut tiles = vec![(0, 0, "meadow", 0)];
        tiles.extend(REVIEW);
        let (c, w) = world(&REVIEW);
        let at = |x: Axial| w.tile(x).and_then(|t| Some((c.get(&t.def)?, t.rotation)));
        let _ = tiles;
        for cell in [
            Axial::new(0, 0),
            Axial::new(0, 1),
            Axial::new(1, 0),
            Axial::new(2, -1),
        ] {
            let segs = local_segments(1, cell, &at);
            assert!(!segs.is_empty(), "{cell:?} sees no boundary");
            for s in &segs {
                let mid = (Vec2::from(s.a) + Vec2::from(s.b)) / 2.0;
                let reach = scatter::hex_reach(mid);
                assert!(
                    (APOTHEM - 2.5..=APOTHEM + 0.01).contains(&reach),
                    "{cell:?}: {s:?} at {reach}"
                );
            }
            let s = segs[0];
            let mid = (Vec2::from(s.a) + Vec2::from(s.b)) / 2.0;
            assert!(!clear_of(&segs, mid, 0.3));
            assert!(
                clear_of(
                    &segs,
                    mid * ((mid.length() - s.half - 1.4) / mid.length()),
                    0.3
                ) || segs.len() > 1
            );
        }
        // The wheat field's edge to the farm carries nothing; its edge to the meadow at (2, -1) does.
        let wheat = local_segments(1, Axial::new(2, 0), &at);
        let on_edge_3 = |p: [f32; 2]| cos_sin_deg(180.0).dot(Vec2::from(p)) > APOTHEM - 0.1;
        assert!(!wheat.is_empty());
        assert!(
            wheat.iter().all(|s| !(on_edge_3(s.a) && on_edge_3(s.b))),
            "{wheat:?}"
        );
    }

    /// Owner A31 Q3: boundaries keep off hero props. The farm's barn stands 0.5 m inside its edge
    /// 5, so no run or arm there overlaps its collider box, and the gate is on a clear edge.
    #[test]
    fn the_farm_keeps_its_barn_clear_and_takes_one_gate() {
        let (c, w) = world(&REVIEW);
        let at = |x: Axial| w.tile(x).and_then(|t| Some((c.get(&t.def)?, t.rotation)));
        let farm = Axial::new(1, 0);
        for seed in [1u64, 2, 3, 4, 5] {
            let p = placements(seed, farm, &at);
            assert_eq!(
                p.iter().filter(|x| x.piece == "wall_gate").count(),
                1,
                "seed {seed}: {p:#?}"
            );
            for s in p.iter().flat_map(|x| &x.segs) {
                assert!(
                    !blocked(&at, farm, s),
                    "seed {seed}: {s:?} overlaps a farm hero"
                );
            }
            // Edge 5 (toward (2, -1)): the barn takes at least its middle run.
            let runs_on_5 = p
                .iter()
                .filter(|x| {
                    x.piece.contains("_run") || x.piece == "wall_gate" || x.piece == "hedge_oak"
                })
                .filter(|x| {
                    let d = Vec2::from(x.at);
                    (cos_sin_deg(300.0).dot(d) - APOTHEM).abs() < 0.5
                })
                .count();
            assert!(
                runs_on_5 < 3,
                "seed {seed}: {runs_on_5} runs on the barn's edge"
            );
            if seed == 1 {
                for x in &p {
                    println!(
                        "{:16} at ({:7.2}, {:7.2}) yaw {:6.1}",
                        x.piece, x.at[0], x.at[1], x.yaw_deg
                    );
                }
            }
        }
    }
}
