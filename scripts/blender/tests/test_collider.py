import math
import unittest

import bmesh
import bpy

from lib import collider, scene


def _ring_hull(radii, sides=8):
    """Stacked, half-step staggered rings closed into a hull mesh (the tree_hero_oak trunk collider source)."""
    bm = bmesh.new()
    for zi, rad in enumerate(radii):
        for k in range(sides):
            a = 2 * math.pi * (k + 0.5 * zi) / sides
            bm.verts.new((rad * math.cos(a), rad * math.sin(a), float(zi)))
    bmesh.ops.convex_hull(bm, input=bm.verts)
    me = bpy.data.meshes.new("ring_hull")
    bm.to_mesh(me)
    bm.free()
    return scene.link(bpy.data.objects.new("ring_hull", me))


class ColliderHull(unittest.TestCase):
    def setUp(self):
        scene.reset_scene("test_collider")

    def test_hull_of_a_hull_with_interior_points(self):
        # bmesh.ops.convex_hull can list one vertex as both unused and interior; delete must not
        # receive it twice (regression: ValueError "found the same ... used multiple times").
        src = _ring_hull([1.6, 1.2, 1.0, 0.95, 0.9])
        col = collider.collider_from(src, "hull", "oak_collider")
        self.assertEqual(col["collider"], "hull")
        self.assertLessEqual(len(col.data.vertices), 40)
        self.assertTrue(all(len(p.vertices) == 3 for p in col.data.polygons))
