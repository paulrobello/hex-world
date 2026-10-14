import json
import pathlib
import tempfile
import unittest

import bpy

from lib import export, gate
from tests import fixture_piece


class Gate(unittest.TestCase):
    def setUp(self):
        self.tmp = pathlib.Path(tempfile.mkdtemp())
        root, _ = fixture_piece.build()
        self.glb = self.tmp / "fixture.glb"
        self.report = self.tmp / "export-report.json"
        export.export_glb(root, str(self.glb))
        export.write_export_report(str(self.report), root, "fixture", __file__)

    def test_reimport_check_passes_on_a_fresh_export(self):
        gate.reimport_check(self.glb, self.report)

    def test_reimport_check_catches_moved_socket_and_missing_node(self):
        rep = json.loads(self.report.read_text())
        rep["sockets"][0]["position"][2] += 0.01
        rep["nodes"].append("fixture_ghost")
        self.report.write_text(json.dumps(rep))
        with self.assertRaisesRegex(AssertionError, "fixture_ghost missing.*socket-lane-0 at"):
            gate.reimport_check(self.glb, self.report)

    def test_six_views_written(self):
        """Committed evidence is JPEG q85 <= 1024 px wide; the lossless PNGs go to png_dir (target/)."""
        objs = gate.import_glb(self.glb)
        views = sorted(["front", "back", "left", "right", "top", "three_quarter"])
        out = gate.render_six_views(objs, self.tmp / "renders", size=1200, png_dir=self.tmp / "png")
        self.assertEqual([pathlib.Path(p).name for p in out], [f"{v}.jpg" for v in views])
        self.assertEqual(sorted(p.name for p in (self.tmp / "renders").iterdir()), [f"{v}.jpg" for v in views])
        self.assertEqual(sorted(p.stem for p in (self.tmp / "png").glob("*.png")), views)
        # No wall-clock metadata, so re-rendering an unchanged GLB does not churn committed bytes.
        self.assertFalse(bpy.context.scene.render.use_stamp_date)
        self.assertFalse(bpy.context.scene.render.use_stamp_render_time)
        for p in out:
            self.assertEqual(pathlib.Path(p).read_bytes()[:2], b"\xff\xd8")  # JPEG SOI
            self.assertNotIn(b"Date", pathlib.Path(p).read_bytes()[:4096])
            img = bpy.data.images.load(p)
            self.assertLessEqual(img.size[0], 1024)
            self.assertEqual(img.size[0], img.size[1])
