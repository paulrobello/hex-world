import os
import pathlib
import subprocess
import sys
import unittest
from unittest import mock

from lib import paths

LIB = pathlib.Path(paths.__file__).resolve()


class BlendStore(unittest.TestCase):
    def test_store_is_main_checkout_assets_source_blend(self):
        with mock.patch.dict(os.environ, {}, clear=False):
            os.environ.pop(paths.ENV, None)
            common = subprocess.run(["git", "rev-parse", "--path-format=absolute", "--git-common-dir"],
                                    cwd=LIB.parent, capture_output=True, text=True, check=True).stdout.strip()
            main = pathlib.Path(common).parent
            self.assertEqual(paths.blend_store(), main / "assets-source" / "blend")
            # Same answer from a worktree and from the main checkout: the store is shared.
            self.assertTrue(paths.blend_store().is_absolute())

    def test_env_override_wins(self):
        with mock.patch.dict(os.environ, {paths.ENV: "/tmp/hexw-store-test"}):
            self.assertEqual(paths.blend_store(), pathlib.Path("/tmp/hexw-store-test"))
            self.assertEqual(paths.blend_path("tree_hero_oak"),
                             pathlib.Path("/tmp/hexw-store-test/tree_hero_oak/tree_hero_oak_high.blend"))

    def test_blend_key_is_store_relative(self):
        with mock.patch.dict(os.environ, {paths.ENV: "/tmp/hexw-store-test"}):
            self.assertEqual(paths.blend_key(paths.blend_path("p")), "p/p_high.blend")
            with self.assertRaises(ValueError):
                paths.blend_key(pathlib.Path("/elsewhere/p_high.blend"))

    def test_cli_prints_path(self):
        env = {**os.environ, paths.ENV: "/tmp/hexw-store-test"}
        out = subprocess.run([sys.executable or "python3", str(LIB), "blend", "p"], env=env,
                             capture_output=True, text=True, check=True).stdout.strip()
        self.assertEqual(out, "/tmp/hexw-store-test/p/p_high.blend")
