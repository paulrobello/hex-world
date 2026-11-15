import pathlib
import unittest

from lib import still


class StillHelpers(unittest.TestCase):
    def test_manifest_loads_with_pieces_and_tileable_sets(self):
        pieces, sets = still.load()
        self.assertIn("barn_red", pieces)
        self.assertTrue(sets, "the manifest carries tileable sets")
        self.assertIn("glb", pieces["barn_red"])

    def test_every_tileable_slot_names_a_set_the_manifest_defines(self):
        pieces, sets = still.load()
        for pid, piece in pieces.items():
            for name, slot in piece["slots"].items():
                if "tileable" in slot:
                    self.assertIn(slot["tileable"], sets, f"{pid}/{name}")

    def test_masks_of_is_a_png_path_or_none(self):
        pieces, _ = still.load()
        found = [still.masks_of(p) for p in pieces.values() if "masks" in p.get("maps", {})]
        self.assertTrue(any(f is not None for f in found), "some piece has masks on disk")
        for f in found:
            self.assertTrue(f is None or (isinstance(f, pathlib.Path) and f.suffix == ".png"))

    def test_camera_looks_at_the_piece_from_its_sun_and_shade_sides(self):
        hero, hero_at = still.camera_pose("hero", 10.0, 6.0)
        back, back_at = still.camera_pose("back", 10.0, 6.0)
        self.assertGreater(hero.x, 0.0)
        self.assertLess(back.x, 0.0)
        self.assertGreater(hero.z, hero_at.z, "the camera looks down at the piece")
        self.assertEqual((hero_at.x, hero_at.y), (0.0, 0.0))

    def test_unknown_piece_is_a_clear_exit(self):
        with self.assertRaises(SystemExit) as ctx:
            still.main("no_such_piece", "/tmp/hexw-still-test")
        self.assertIn("no_such_piece", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()
