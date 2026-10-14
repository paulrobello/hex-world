import os
import unittest


class RunnerSelfCheck(unittest.TestCase):
    def test_runs_inside_blender(self):
        import bpy  # fails outside Blender
        self.assertTrue(bpy.app.version >= (5, 2, 0), bpy.app.version_string)

    def test_forced_failure_when_requested(self):
        # `make blender-test-selfcheck` sets this to prove a failing test fails the make target.
        self.assertNotEqual(os.environ.get("HEXW_FORCE_FAIL"), "1", "forced failure")
