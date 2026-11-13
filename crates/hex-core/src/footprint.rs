//! Mega-tile footprint geometry: a centre cell (the anchor) and its six ring cells.
//!
//! Ring cell `i` sits at `anchor.neighbor(i)`. Its local edges `(i + 5) % 6`, `i` and `(i + 1) % 6`
//! face outside the footprint; `(i + 2) % 6`, `(i + 3) % 6` and `(i + 4) % 6` face other members.
//! Outer edge `j` in `0..18` is ring cell `j / 3`, edge `(j / 3 + 5 + j % 3) % 6`: clockwise from
//! ring cell 0. Rotation `k` maps ring cell `i` to `(i + k) % 6` and outer index `j` to
//! `(j + 3k) % 18`. The HUD and the Blender generator share this numbering.

use crate::axial::Axial;
use crate::tile::world_edge;

/// Outer edges of a seven-cell footprint: three on each ring cell.
pub const OUTER_EDGES: usize = 18;

/// The footprint of `anchor`: the anchor, then `anchor.neighbor(0..6)`.
pub fn members(anchor: Axial) -> [Axial; 7] {
    std::array::from_fn(|m| {
        if m == 0 {
            anchor
        } else {
            anchor.neighbor(m as u8 - 1)
        }
    })
}

/// The local (ring cell, local edge) of outer index `j`, modulo 18.
pub fn outer_edge(j: u8) -> (u8, u8) {
    let j = j % OUTER_EDGES as u8;
    let ring = j / 3;
    (ring, (ring + 5 + j % 3) % 6)
}

/// The outer index of ring cell `ring`'s local edge `edge`, or `None` where it faces a member.
pub fn outer_index(ring: u8, edge: u8) -> Option<u8> {
    let (ring, edge) = (ring % 6, edge % 6);
    let m = (edge + 6 - ring + 1) % 6;
    (m < 3).then_some(ring * 3 + m)
}

/// (member cell, world edge, local outer index) of every outer edge of a footprint at `anchor`
/// turned by `rotation`, in local outer-index order.
pub fn outer_faces(anchor: Axial, rotation: u8) -> [(Axial, u8, u8); OUTER_EDGES] {
    std::array::from_fn(|j| {
        let j = j as u8;
        let (ring, edge) = outer_edge(j);
        (
            anchor.neighbor(world_edge(ring, rotation)),
            world_edge(edge, rotation),
            j,
        )
    })
}

/// The twelve cells at distance 2 from `anchor`, sorted.
pub fn outer_neighbours(anchor: Axial) -> [Axial; 12] {
    let mut cells: [Axial; 12] = std::array::from_fn(|n| {
        let i = (n / 2) as u8;
        anchor.neighbor(i).neighbor(i + (n % 2) as u8)
    });
    cells.sort();
    cells
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    const A: Axial = Axial::new(3, -2);

    #[test]
    fn members_are_the_centre_and_its_ring() {
        let m = members(A);
        assert_eq!(m[0], A);
        for i in 0..6u8 {
            assert_eq!(m[1 + usize::from(i)], A.neighbor(i));
        }
    }

    #[test]
    fn outer_edges_cover_the_perimeter_once() {
        let m = members(A);
        let faces = outer_faces(A, 0);
        let pairs: BTreeSet<(Axial, u8)> = faces.iter().map(|&(c, w, _)| (c, w)).collect();
        assert_eq!(pairs.len(), OUTER_EDGES);
        for &(c, w) in &pairs {
            assert!(m.contains(&c), "{c:?} is not a member");
            assert!(!m.contains(&c.neighbor(w)), "{c:?} edge {w} faces a member");
        }
        let perimeter: BTreeSet<(Axial, u8)> = m
            .iter()
            .flat_map(|&c| (0..6u8).map(move |w| (c, w)))
            .filter(|&(c, w)| !m.contains(&c.neighbor(w)))
            .collect();
        assert_eq!(pairs, perimeter);
    }

    #[test]
    fn side_cells_face_two_outer_edges() {
        let mut seen: BTreeMap<Axial, usize> = BTreeMap::new();
        for (c, w, _) in outer_faces(A, 0) {
            *seen.entry(c.neighbor(w)).or_default() += 1;
        }
        let cells: Vec<Axial> = seen.keys().copied().collect();
        assert_eq!(cells, outer_neighbours(A).to_vec());
        for i in 0..6u8 {
            let corner = A.neighbor(i).neighbor(i);
            let side = A.neighbor(i).neighbor((i + 1) % 6);
            assert_eq!(seen[&corner], 1, "corner {corner:?}");
            assert_eq!(seen[&side], 2, "side {side:?}");
        }
        assert!(outer_neighbours(A).iter().all(|c| c.distance(A) == 2));
        assert!(outer_neighbours(A).is_sorted());
    }

    #[test]
    fn outer_index_inverts_outer_edge() {
        for j in 0..OUTER_EDGES as u8 {
            let (ring, edge) = outer_edge(j);
            assert_eq!(outer_index(ring, edge), Some(j), "outer {j}");
        }
        for i in 0..6u8 {
            for k in 2..5u8 {
                assert_eq!(
                    outer_index(i, (i + k) % 6),
                    None,
                    "ring {i} edge {}",
                    (i + k) % 6
                );
            }
        }
    }

    #[test]
    fn rotation_shifts_outer_indices_by_three() {
        let at = |k: u8, j: u8| -> (Axial, u8) {
            let &(c, w, _) = outer_faces(A, k)
                .iter()
                .find(|f| f.2 == j)
                .expect("every local index appears");
            (c, w)
        };
        for k in 0..6u8 {
            for j in 0..OUTER_EDGES as u8 {
                assert_eq!(at(k, j), at(0, (j + 3 * k) % 18), "rotation {k} outer {j}");
            }
        }
    }
}
