//! Pointy-top axial hex coordinates. Edge 0 faces east (+x); edges run clockwise viewed from +Y.

pub const HEX_RADIUS: f32 = 16.0;

/// Height of one elevation level in meters (PRD §5.1, frozen in P1 with HEX_RADIUS; D8).
pub const LEVEL_STEP: f32 = 4.0;

/// Ordered by `(q, r)`, so ordered maps and sorted lists of cells are deterministic.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct Axial {
    pub q: i32,
    pub r: i32,
}

/// Axial offsets for edges 0..6: E, SE, SW, W, NW, NE (+z is "south" on screen).
const DIRS: [(i32, i32); 6] = [(1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)];

impl Axial {
    pub const ORIGIN: Axial = Axial { q: 0, r: 0 };

    pub fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }

    /// Center of the cell on the XZ plane, in meters.
    pub fn to_world(self, radius: f32) -> [f32; 2] {
        let q = self.q as f32;
        let r = self.r as f32;
        [radius * 3f32.sqrt() * (q + r / 2.0), radius * 1.5 * r]
    }

    pub fn neighbor(self, edge: u8) -> Axial {
        let (dq, dr) = DIRS[(edge % 6) as usize];
        Axial::new(self.q + dq, self.r + dr)
    }

    pub fn distance(self, other: Axial) -> i32 {
        let dq = self.q - other.q;
        let dr = self.r - other.r;
        (dq.abs() + dr.abs() + (dq + dr).abs()) / 2
    }
}

pub fn ring_capacity(radius: u32) -> usize {
    let n = radius as usize;
    1 + 3 * n * (n + 1)
}

/// Cells in ring order (center, ring 1, ring 2, …), truncated to `count`.
pub fn spiral(count: usize) -> Vec<Axial> {
    let mut out = Vec::with_capacity(count);
    if count == 0 {
        return out;
    }
    out.push(Axial::ORIGIN);
    let mut radius = 1;
    while out.len() < count {
        // Start at the cell `radius` steps along edge 4 (NW), then walk each side.
        let mut cell = Axial::new(DIRS[4].0 * radius, DIRS[4].1 * radius);
        for side in 0..6u8 {
            for _ in 0..radius {
                if out.len() == count {
                    return out;
                }
                out.push(cell);
                cell = cell.neighbor(side);
            }
        }
        radius += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_maps_to_world_origin() {
        assert_eq!(Axial::ORIGIN.to_world(HEX_RADIUS), [0.0, 0.0]);
    }

    #[test]
    fn east_neighbor_is_flat_to_flat_away() {
        // Pointy-top: edge 0 faces east; centers are sqrt(3)*R apart.
        let [x, z] = Axial::ORIGIN.neighbor(0).to_world(HEX_RADIUS);
        assert!((x - 3f32.sqrt() * HEX_RADIUS).abs() < 1e-4);
        assert!(z.abs() < 1e-4);
    }

    #[test]
    fn all_six_neighbors_are_distance_one_and_distinct() {
        let n: Vec<Axial> = (0..6).map(|e| Axial::ORIGIN.neighbor(e)).collect();
        for a in &n {
            assert_eq!(Axial::ORIGIN.distance(*a), 1);
        }
        let set: std::collections::HashSet<_> = n.iter().collect();
        assert_eq!(set.len(), 6);
    }

    #[test]
    fn neighbor_edges_are_clockwise_from_east() {
        // Edge 1 is 60° clockwise from east when viewed from +Y looking down (south-east, +z).
        let [_, z] = Axial::ORIGIN.neighbor(1).to_world(HEX_RADIUS);
        assert!(z > 0.0);
    }

    #[test]
    fn opposite_edges_cancel() {
        for e in 0..3u8 {
            let a = Axial::ORIGIN.neighbor(e).neighbor(e + 3);
            assert_eq!(a, Axial::ORIGIN);
        }
    }

    #[test]
    fn ring_capacity_matches_formula() {
        assert_eq!(ring_capacity(0), 1);
        assert_eq!(ring_capacity(1), 7);
        assert_eq!(ring_capacity(25), 1951);
        assert_eq!(ring_capacity(26), 2107);
    }

    #[test]
    fn spiral_is_exact_length_unique_and_ring_ordered() {
        let cells = spiral(2000);
        assert_eq!(cells.len(), 2000);
        let set: std::collections::HashSet<_> = cells.iter().collect();
        assert_eq!(set.len(), 2000);
        assert_eq!(cells[0], Axial::ORIGIN);
        let mut last = 0;
        for c in &cells {
            let d = Axial::ORIGIN.distance(*c);
            assert!(d >= last, "spiral must never move inward");
            last = d;
        }
        assert_eq!(Axial::ORIGIN.distance(cells[1999]), 26);
    }

    #[test]
    fn spiral_zero_is_empty() {
        assert!(spiral(0).is_empty());
    }
}
