"""Tests for the shot runner's black-frame check and retry (`make shots-test`)."""

from __future__ import annotations

import json
import pathlib
import sys
import tempfile
import unittest

from PIL import Image

sys.path.insert(0, str(pathlib.Path(__file__).parent))
import piece  # noqa: E402

# A stand-in for hex-world: writes the frames named on its command line, black on the first run
# when BLACK_ONCE marks it, then normal. Exit code from EXIT.
FAKE_APP = """
import os, pathlib, sys
from PIL import Image
shots, marker = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
first = not marker.exists()
marker.write_text("ran")
colour = (0, 0, 0) if first and os.environ.get("BLACK_ONCE") else (90, 140, 60)
for n in sys.argv[3:]:
    Image.new("RGB", (64, 40), colour).save(shots / f"{n}.png")
sys.exit(int(os.environ.get("EXIT", "0")))
"""


class FrameCheck(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.d = pathlib.Path(self.tmp.name)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def png(self, name: str, colour: tuple[int, int, int], size: tuple[int, int] = (1600, 1000)) -> pathlib.Path:
        p = self.d / name
        Image.new("RGB", size, colour).save(p)
        return p

    def test_black_frame_is_rejected(self) -> None:
        problem, mean = piece.frame_problem(self.png("black.png", (0, 0, 0)))
        self.assertEqual(mean, 0)
        self.assertIn("black", problem)

    def test_near_black_frame_is_rejected(self) -> None:
        problem, mean = piece.frame_problem(self.png("dim.png", (4, 4, 4)))
        self.assertLess(mean, piece.BLACK_MEAN)
        self.assertIn("black", problem)

    def test_scene_frame_passes(self) -> None:
        problem, mean = piece.frame_problem(self.png("scene.png", (90, 140, 60)))
        self.assertIsNone(problem)
        self.assertAlmostEqual(mean, (90 + 140 + 60) / 3 / 255, places=4)

    def test_dark_scene_with_a_bright_patch_passes(self) -> None:
        im = Image.new("RGBA", (200, 100), (0, 0, 0, 255))
        im.paste((200, 200, 200, 255), (0, 0, 40, 100))
        p = self.d / "night.png"
        im.save(p)
        self.assertIsNone(piece.frame_problem(p)[0])

    def test_missing_empty_and_corrupt_frames_are_rejected(self) -> None:
        self.assertEqual(piece.frame_problem(self.d / "none.png")[0], "missing")
        (self.d / "empty.png").write_bytes(b"")
        self.assertEqual(piece.frame_problem(self.d / "empty.png")[0], "empty")
        (self.d / "bad.png").write_bytes(b"not a png")
        self.assertTrue(piece.frame_problem(self.d / "bad.png")[0].startswith("unreadable"))

    def test_script_outputs_lists_frames_and_dumps(self) -> None:
        s = self.d / "s.json"
        s.write_text(
            json.dumps(
                [
                    {"cmd": "screenshot", "name": "a"},
                    {"cmd": "dump_state", "name": "d"},
                    {"cmd": "photo_export", "scale": 2, "name": "b"},
                    {"cmd": "wait", "frames": 2},
                ]
            )
        )
        self.assertEqual(piece.script_outputs(s), (["a", "b"], ["d"]))


class Shoot(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.d = pathlib.Path(self.tmp.name)
        self.shots = self.d / "shots"
        self.app = self.d / "app.py"
        self.app.write_text(FAKE_APP)
        self.log = self.d / "log"

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def run_app(self, names: list[str], frames: list[str], env: dict[str, str]) -> list[dict]:
        cmd = ["env", *(f"{k}={v}" for k, v in env.items()), sys.executable, str(self.app)]
        cmd += [str(self.shots), str(self.d / "marker"), *names]
        return piece.shoot(cmd, frames, [], self.shots, 60, self.log)

    def test_clean_run_takes_one_attempt(self) -> None:
        tries = self.run_app(["a", "b"], ["a", "b"], {})
        self.assertEqual(len(tries), 1)
        self.assertEqual(tries[0]["problems"], [])
        self.assertEqual(len(tries[0]["load_before"]), 3)

    def test_black_frame_reruns_once(self) -> None:
        tries = self.run_app(["a"], ["a"], {"BLACK_ONCE": "1"})
        self.assertEqual(len(tries), 2)
        self.assertIn("black", tries[0]["problems"][0])
        self.assertEqual(tries[1]["problems"], [])

    def test_a_missing_frame_fails_both_attempts(self) -> None:
        tries = self.run_app(["a"], ["a", "b"], {})
        self.assertEqual(len(tries), 2)
        self.assertEqual(tries[1]["problems"], ["b: missing"])

    def test_a_stale_frame_does_not_pass(self) -> None:
        self.shots.mkdir()
        Image.new("RGB", (8, 8), (90, 140, 60)).save(self.shots / "b.png")
        tries = self.run_app(["a"], ["a", "b"], {})
        self.assertEqual(tries[-1]["problems"], ["b: missing"])

    def test_a_failing_exit_is_a_failed_attempt(self) -> None:
        tries = self.run_app(["a"], ["a"], {"EXIT": "3"})
        self.assertEqual(tries[-1]["problems"], ["exit 3"])

    def test_a_hang_times_out_and_retries_once(self) -> None:
        tries = piece.shoot(["sleep", "5"], ["a"], [], self.shots, 0.3, self.log)
        self.assertEqual(len(tries), 2)
        self.assertTrue(all(t["timed_out"] for t in tries))
        self.assertIn("timed out after 0.3 s", tries[-1]["problems"])


if __name__ == "__main__":
    unittest.main()
