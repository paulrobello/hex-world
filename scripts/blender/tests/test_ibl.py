import pathlib
import struct
import tempfile
import unittest

import numpy as np

from lib import ibl, ktx2


class Ibl(unittest.TestCase):
    def test_face_directions_follow_bevy_lookup(self):
        # Bevy samples the cube at (w.x, w.y, -w.z) (environment_map.wesl), so the +X face center
        # holds world +X and the +Z face center holds world -Z.
        d = ibl.face_dirs(0, 1)[0, 0]
        self.assertTrue(np.allclose(d, [1, 0, 0]))
        w = ibl.cube_to_world(ibl.face_dirs(4, 1)[0, 0])
        self.assertTrue(np.allclose(w, [0, 0, -1]))

    def test_equirect_follows_cycles_mapping(self):
        # Cycles: u = 0.5 - atan2(y_b, x_b) / 2pi; runtime = (x_b, z_b, -y_b).
        eq = np.zeros((8, 16, 3), np.float32)
        eq[4, 8] = 1.0  # u = 0.5, v = 0.5: Blender +X = runtime +X
        self.assertEqual(float(ibl.sample_equirect(eq, np.array([1.0, 0.0, 0.0]))[0]), 1.0)
        eq = np.zeros((8, 16, 3), np.float32)
        eq[4, 4] = 1.0  # u = 0.25: Blender +Y = runtime -Z
        self.assertEqual(float(ibl.sample_equirect(eq, np.array([0.0, 0.0, -1.0]))[0]), 1.0)
        self.assertEqual(float(ibl.sample_equirect(eq, np.array([0.0, 0.0, 1.0]))[0]), 0.0)

    def test_blender_plus_x_half_lights_runtime_plus_x_face(self):
        eq = np.zeros((16, 32, 3), np.float32)
        eq[:, 8:24] = 1.0  # u in [0.25, 0.75): the Blender +X hemisphere
        irr = ibl.irradiance(eq, size=4)[0]
        self.assertGreater(irr[0, ..., :3].mean(), irr[1, ..., :3].mean() + 0.1)
        spec = ibl.prefilter(eq, base=4, samples=8)[0]
        self.assertGreater(spec[0, ..., :3].mean(), spec[1, ..., :3].mean() + 0.5)

    def test_constant_sky_prefilters_to_constant(self):
        eq = np.full((32, 64, 3), 2.0, np.float32)
        levels = ibl.prefilter(eq, base=16, samples=16)
        self.assertEqual(len(levels), 5)  # 16 -> 1
        for lv in levels:
            self.assertTrue(np.allclose(lv[..., :3], 2.0, atol=1e-3))
        irr = ibl.irradiance(eq, size=8)[0]
        self.assertTrue(np.allclose(irr[..., :3], 2.0, atol=2e-2))  # SH9 of a constant = constant

    def test_ktx2_writer_header(self):
        p = pathlib.Path(tempfile.mkdtemp()) / "c.ktx2"
        ktx2.write_cubemap_rgba16f(str(p), [np.zeros((6, 4, 4, 4), np.float32), np.zeros((6, 2, 2, 4), np.float32)])
        b = p.read_bytes()
        vk, _, w, h, _, layers, faces, levels, sc = struct.unpack_from("<9I", b, 12)
        self.assertEqual((vk, w, h, layers, faces, levels, sc), (97, 4, 4, 0, 6, 2, 0))

    def test_ktx2_writer_clamps_to_half_max(self):
        # The HDRI sun exceeds float16's 65504; an inf texel would poison the specular lookup.
        p = pathlib.Path(tempfile.mkdtemp()) / "c.ktx2"
        lv = np.full((6, 1, 1, 4), 1e6, np.float32)
        lv[0, 0, 0, 3] = np.nan
        lv[1, 0, 0, 3] = 1.0
        counts = ktx2.write_cubemap_rgba16f(str(p), [lv])
        self.assertEqual(counts, {"clamped": 22, "nonFinite": 1})
        b = p.read_bytes()
        off, length, _ = struct.unpack_from("<QQQ", b, 80)
        a = np.frombuffer(b[off:off + length], np.float16).reshape(6, 4)
        self.assertTrue(np.all(np.isfinite(a)))
        self.assertEqual(float(a[0, 3]), 0.0)  # NaN -> 0
        self.assertEqual(float(a[1, 3]), 1.0)
        self.assertEqual(float(a[2, 0]), 65504.0)
