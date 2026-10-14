import math
import pathlib
import re
import unittest

from lib import hexgeo

REPO = pathlib.Path(__file__).resolve().parents[3]


def rust_const(name):
    src = (REPO / "crates/hex-core/src/axial.rs").read_text()
    m = re.search(rf"pub const {name}: f32 = ([0-9.]+);", src)
    assert m, f"{name} not found in axial.rs"
    return float(m.group(1))


class HexGeo(unittest.TestCase):
    def test_constants_match_rust(self):  # D8
        self.assertEqual(hexgeo.HEX_R, rust_const("HEX_RADIUS"))
        self.assertEqual(hexgeo.LEVEL_STEP, rust_const("LEVEL_STEP"))
        self.assertAlmostEqual(hexgeo.APOTHEM, 13.856406, places=5)

    def test_edge0_faces_plus_x_and_edge1_is_south(self):
        x, y = hexgeo.edge_midpoint(0)
        self.assertAlmostEqual(x, hexgeo.APOTHEM, places=5)
        self.assertAlmostEqual(y, 0.0, places=6)
        _, _, z_rt = hexgeo.blender_to_rt(*hexgeo.edge_midpoint(1), 0.0)
        self.assertGreater(z_rt, 0.0)  # axial.rs: edge 1 is +z (south)

    def test_frame_round_trip(self):
        p = (1.0, 2.0, 3.0)
        self.assertEqual(hexgeo.rt_to_blender(*hexgeo.blender_to_rt(*p)), p)

    def test_border_ring_on_border_and_rotation_invariant(self):
        ring = hexgeo.border_ring(8)
        self.assertEqual(len(ring), 48)
        for p in ring:
            self.assertAlmostEqual(hexgeo.dist_to_border(*p), 0.0, places=5)
        key = lambda p: (round(p[0], 4), round(p[1], 4))
        rotated = {key(hexgeo.rotate_xy(p, 1)) for p in ring}
        self.assertEqual(rotated, {key(p) for p in ring})

    def test_lattice_respects_margin(self):
        pts = hexgeo.lattice_points(2.0, 1.0)
        self.assertGreater(len(pts), 100)
        self.assertTrue(all(hexgeo.dist_to_border(*p) >= 1.0 - 1e-9 for p in pts))

    def test_lane_frame_edge0(self):
        f = hexgeo.lane_frame(0, 0.0, 6.0)
        self.assertAlmostEqual(f["center"][0], hexgeo.APOTHEM, places=5)
        self.assertEqual(f["width"], 6.0)
        self.assertAlmostEqual(math.hypot(*f["tangent"]), 1.0, places=6)

    # Road bends reuse the edge contract (owner): every edge is edge 0 rotated, point for point.
    def test_border_ring_edge_slices_are_rotations_of_edge0(self):
        s = 8
        ring = hexgeo.border_ring(s)
        for k in range(6):
            for p0, pk in zip(ring[:s], ring[k * s:(k + 1) * s]):
                r = hexgeo.rotate_xy(p0, k)
                self.assertAlmostEqual(r[0], pk[0], places=6)
                self.assertAlmostEqual(r[1], pk[1], places=6)

    def test_lane_frame_is_edge_generic(self):
        off, w = 1.75, 6.0
        f0 = hexgeo.lane_frame(0, off, w)
        for k in range(6):
            f = hexgeo.lane_frame(k, off, w)
            for key in ("center", "tangent", "normal"):
                want = hexgeo.rotate_xy(f0[key], k)
                self.assertAlmostEqual(f[key][0], want[0], places=6, msg=(k, key))
                self.assertAlmostEqual(f[key][1], want[1], places=6, msg=(k, key))
            self.assertEqual(f["width"], w)
            self.assertAlmostEqual(f["tangent"][0] * f["normal"][0] + f["tangent"][1] * f["normal"][1], 0.0)
            self.assertAlmostEqual(hexgeo.dist_to_border(*f["center"]), 0.0, places=6)

    def test_lane_frame_matches_kit_gate_socket_formula(self):  # all six edges
        a_, off = hexgeo.APOTHEM, 1.75
        for k in range(6):
            a = math.radians(60.0 * k)
            c = hexgeo.lane_frame(k, off)["center"]
            x_rt, _, z_rt = hexgeo.blender_to_rt(c[0], c[1], 0.0)
            self.assertAlmostEqual(x_rt, a_ * math.cos(a) + math.sin(a) * off, places=5, msg=k)
            self.assertAlmostEqual(z_rt, a_ * math.sin(a) - math.cos(a) * off, places=5, msg=k)
