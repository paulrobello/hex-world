import math
import unittest

import bpy

from lib import geom, hexgeo, lod, scene


def bump(x, y):
    d = hexgeo.dist_to_border(x, y)
    fall = min(d / 2.0, 1.0)
    fall = fall * fall * (3 - 2 * fall)  # smoothstep: zero value and zero slope at the border
    return 0.3 * fall * math.sin(x * 0.5) * math.cos(y * 0.4)


class GeomLod(unittest.TestCase):
    def setUp(self):
        scene.reset_scene("test")

    def caps(self):
        ring = hexgeo.border_ring(8)
        return [geom.cdt_cap(f"cap{i}", ring, hexgeo.lattice_points(s, 1.0), bump)
                for i, s in enumerate((1.5, 3.0, 50.0))]

    def test_cap_lods_share_border(self):
        caps = self.caps()
        lod.assert_border_identical(caps)
        tris = [geom.tri_count(c) for c in caps]
        self.assertTrue(tris[0] > tris[1] > tris[2], tris)
        self.assertEqual(len(lod.border_verts(caps[2])), 48)

    def test_border_normals_up_and_faces_up(self):
        cap = self.caps()[0]
        geom.set_border_normals_up(cap)
        mesh = cap.data
        for poly in mesh.polygons:
            self.assertGreater(poly.normal.z, 0.0)
        border = {i for i, v in enumerate(mesh.vertices) if abs(hexgeo.dist_to_border(v.co.x, v.co.y)) < 1e-4}
        for li, loop in enumerate(mesh.loops):
            if loop.vertex_index in border:
                self.assertGreater(mesh.corner_normals[li].vector.z, 0.999)

    def test_cap_normals_follow_the_slope_and_stay_up_on_the_border(self):
        cap = self.caps()[0]
        geom.set_cap_normals(cap, lambda x, y: (0.1, -0.2))
        mesh = cap.data
        want = (-0.1, 0.2, 1.0)
        s = math.sqrt(sum(c * c for c in want))
        for li, loop in enumerate(mesh.loops):
            v = mesh.vertices[loop.vertex_index].co
            n = mesh.corner_normals[li].vector
            if abs(hexgeo.dist_to_border(v.x, v.y)) < 1e-4:
                self.assertGreater(n.z, 0.999)
            else:
                for got, w in zip(n, want):
                    self.assertAlmostEqual(got, w / s, places=3)  # custom normals are stored quantized

    def test_skirt_outward_and_counted(self):
        ring = hexgeo.border_ring(4)
        s = geom.skirt("skirt", ring, hexgeo.LEVEL_STEP, 2)
        self.assertEqual(geom.tri_count(s), 24 * 2 * 2)
        for poly in s.data.polygons:
            c = poly.center
            self.assertGreater(poly.normal.x * c.x + poly.normal.y * c.y, 0.0)

    def test_decimate_hits_target(self):
        bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=5, radius=1.0)
        obj = bpy.context.active_object
        got = lod.decimate_to(obj, 1000)
        self.assertLessEqual(got, 1000)
        self.assertGreater(got, 900)
