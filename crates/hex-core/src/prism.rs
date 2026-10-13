//! Procedural hex column used as the P0 placeholder tile.

pub struct PrismMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

/// Pointy-top corner `i` (0..6) at angle 30° + 60°·i, clockwise from +x viewed from +Y.
fn corner(radius: f32, i: usize) -> [f32; 2] {
    let a = (30.0 + 60.0 * i as f32).to_radians();
    [radius * a.cos(), radius * a.sin()]
}

pub fn hex_prism(radius: f32, height: f32) -> PrismMesh {
    let mut m = PrismMesh {
        positions: vec![],
        normals: vec![],
        uvs: vec![],
        indices: vec![],
    };
    let up = [0.0, 1.0, 0.0];

    m.positions.push([0.0, height, 0.0]);
    m.normals.push(up);
    m.uvs.push([0.5, 0.5]);
    for i in 0..6 {
        let [x, z] = corner(radius, i);
        m.positions.push([x, height, z]);
        m.normals.push(up);
        m.uvs
            .push([0.5 + x / (2.0 * radius), 0.5 + z / (2.0 * radius)]);
    }
    // +z increases with corner index, so corners run clockwise viewed from +Y;
    // CCW-from-above order is (center, next, current).
    for i in 0..6u32 {
        let cur = 1 + i;
        let next = 1 + (i + 1) % 6;
        m.indices.extend_from_slice(&[0, next, cur]);
    }

    for i in 0..6 {
        let [x0, z0] = corner(radius, i);
        let [x1, z1] = corner(radius, (i + 1) % 6);
        let mid = [(x0 + x1) / 2.0, (z0 + z1) / 2.0];
        let len = (mid[0] * mid[0] + mid[1] * mid[1]).sqrt();
        let n = [mid[0] / len, 0.0, mid[1] / len];
        let base = m.positions.len() as u32;
        for (x, y, z, u, v) in [
            (x0, 0.0, z0, 0.0, 1.0),
            (x1, 0.0, z1, 1.0, 1.0),
            (x1, height, z1, 1.0, 0.0),
            (x0, height, z0, 0.0, 0.0),
        ] {
            m.positions.push([x, y, z]);
            m.normals.push(n);
            m.uvs.push([u, v]);
        }
        m.indices
            .extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri_normal(m: &PrismMesh, t: usize) -> [f32; 3] {
        let [a, b, c] = [0, 1, 2].map(|k| m.positions[m.indices[t * 3 + k] as usize]);
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ]
    }

    #[test]
    fn counts_are_top_fan_plus_six_quads() {
        let m = hex_prism(16.0, 4.0);
        assert_eq!(m.positions.len(), 7 + 6 * 4);
        assert_eq!(m.indices.len(), 6 * 3 + 6 * 6);
        assert_eq!(m.normals.len(), m.positions.len());
        assert_eq!(m.uvs.len(), m.positions.len());
    }

    #[test]
    fn top_cap_is_flat_at_height_and_faces_up() {
        let m = hex_prism(16.0, 4.0);
        for i in 0..7 {
            assert_eq!(m.positions[i][1], 4.0);
            assert_eq!(m.normals[i], [0.0, 1.0, 0.0]);
        }
        for t in 0..6 {
            assert!(
                tri_normal(&m, t)[1] > 0.0,
                "top tri {t} must wind CCW from above"
            );
        }
    }

    #[test]
    fn corners_lie_on_circumradius() {
        let m = hex_prism(16.0, 4.0);
        for p in &m.positions[1..7] {
            let d = (p[0] * p[0] + p[2] * p[2]).sqrt();
            assert!((d - 16.0).abs() < 1e-4);
        }
    }

    #[test]
    fn side_faces_point_outward() {
        let m = hex_prism(16.0, 4.0);
        for t in 6..18 {
            let n = tri_normal(&m, t);
            let i = m.indices[t * 3] as usize;
            let p = m.positions[i];
            assert!(
                n[0] * p[0] + n[2] * p[2] > 0.0,
                "side tri {t} must face outward"
            );
            assert!(n[1].abs() < 1e-4);
        }
    }

    #[test]
    fn indices_in_range() {
        let m = hex_prism(16.0, 4.0);
        assert!(m.indices.iter().all(|&i| (i as usize) < m.positions.len()));
    }
}
