import unittest

from lib import budgets


class Budgets(unittest.TestCase):
    def test_kind_and_flag_ceilings(self):
        self.assertEqual(budgets.lod_budget("prop"), (1500, 0.5, 0.15))
        self.assertEqual(budgets.lod_budget("prop", "river_spring_center"), (2500, 0.5, 0.15))
        self.assertEqual(budgets.lod_budget("base", lane_cut=True), (4000, 0.5, 0.20))
        self.assertEqual(budgets.lod_budget("base", coast_arc=True), (4000, 0.5, 0.20))
        self.assertEqual(budgets.lod_budget("center", lane_cut=True), (4000, 0.5, 0.15))  # bases only

    def test_targets_and_floor(self):
        self.assertEqual(budgets.lod_targets(1048, "prop"), (524, 157))
        # gates.md: a 96-vertex ring with one skirt row fixes LOD2 at about 286, so LOD0 >= 1,907.
        self.assertEqual(budgets.min_lod0(286, "base"), 1907)
        self.assertEqual(budgets.min_lod0(724, "base", lane_cut=True), 3620)  # river_bend_base ships 3,678
        with self.assertRaises(ValueError):
            budgets.min_lod0(700, "base")  # needs 4,667 > 4,000

    def test_check_matches_the_gate(self):
        budgets.check((1048, 436, 54), "prop", "house_cottage")
        budgets.check((3678, 1839, 724), "base", "river_bend_base", lane_cut=True)
        with self.assertRaisesRegex(AssertionError, "LOD2 724 > 0.15"):
            budgets.check((3678, 1839, 724), "base", "river_bend_base")
        with self.assertRaisesRegex(AssertionError, "LOD0 1501 > 1500"):
            budgets.check((1501, 700, 200), "prop")
