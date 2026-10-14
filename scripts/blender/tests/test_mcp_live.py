import pathlib
import unittest

import mcp_live


class LiveExec(unittest.TestCase):
    def test_exec_binds_repo_to_this_checkout(self):  # no host path in live steps
        root = pathlib.Path(mcp_live.__file__).resolve().parents[2]
        self.assertEqual(mcp_live.REPO, root)
        self.assertTrue((root / "Makefile").is_file())
        ns: dict = {}
        exec(mcp_live.with_repo("OUT = REPO + '/target/x.png'\n"), ns)
        self.assertEqual(ns["OUT"], f"{root}/target/x.png")

    def test_committed_live_steps_carry_no_host_path(self):
        for step in sorted((mcp_live.REPO / "assets-source/kit").glob("*/live/*.py")):
            text = step.read_text()
            self.assertNotIn("/Users/", text, step)
            self.assertNotIn("worktrees/", text, step)
            self.assertNotRegex(text, r"(?m)^REPO\s*=", f"{step} must take REPO from mcp_live exec")
