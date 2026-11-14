#!/usr/bin/env python3
"""Tests of tools/craft_convert.py: python3 -m unittest tools/test_craft_convert.py"""
import glob
import os
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import craft_convert as c  # noqa: E402

ROOT = os.path.join(HERE, "..")
MODS_DIR = os.path.abspath(os.environ.get("VOXELRUST_MODS_DIR") or os.path.join(ROOT, "..", "VoxelRust-Mods"))  # the mods live outside the repository


def read(path):
    with open(path) as f:
        return f.read()


class Convert(unittest.TestCase):
    def test_a_known_shape_becomes_a_shaped_line(self):
        self.assertEqual(
            c.convert_line("craft iron_pick 1 iron_ingot 3 stick 2"),
            'shaped iron_pick 1 "III" ".S." ".S." with I iron_ingot S stick',
        )
        self.assertEqual(
            c.convert_line("craft stone_sword 1 cobble 2 stick 1"),
            'shaped stone_sword 1 "I" "I" "S" with I cobble S stick',
        )

    def test_the_cost_of_a_shape_must_agree_with_the_line(self):
        # Two sticks, not one: a sword is not what this is.
        self.assertEqual(c.convert_line("craft stone_sword 1 cobble 2 stick 2"), "craft stone_sword 1 cobble 2 stick 2")
        # Not a pick at all.
        self.assertEqual(c.convert_line("craft iron_pick 1 iron_ingot 3"), "craft iron_pick 1 iron_ingot 3")

    def test_stairs_slabs_and_walls_follow_the_usual_shapes(self):
        self.assertEqual(c.convert_line("craft stone_stairs 4 stone 3"), 'shaped stone_stairs 4 "M.." "MM." "MMM" with M stone')
        self.assertEqual(c.convert_line("craft stone_slab 2 stone 1"), 'shaped stone_slab 6 "MMM" with M stone')
        self.assertEqual(c.convert_line("craft stone_wall 2 stone 1"), 'shaped stone_wall 6 "MMM" "MMM" with M stone')

    def test_any_planks_except_in_what_is_made_of_one_wood(self):
        self.assertEqual(c.convert_line("craft wood_pick 1 planks 3 stick 2"), 'shaped wood_pick 1 "III" ".S." ".S." with I @planks S stick')
        self.assertEqual(c.convert_line("craft oak_door 1 planks 6"), 'shaped oak_door 1 "MM" "MM" "MM" with M planks')
        self.assertEqual(c.convert_line("craft pine_door 1 pine_planks 6"), 'shaped pine_door 1 "MM" "MM" "MM" with M pine_planks')
        self.assertEqual(c.convert_line("craft red_bed 1 red_wool 3 planks 3"), 'shaped red_bed 1 "WWW" "PPP" with P @planks W red_wool')
        self.assertEqual(c.convert_line("craft cupboard_thing 1 oak_chest 1 planks 2"), "craft cupboard_thing 1 oak_chest 1 @planks 2")

    def test_unknown_shapes_stay_shapeless_and_recipe_becomes_craft(self):
        self.assertEqual(c.convert_line("craft lamp 1 glass 1 crystal 1"), "craft lamp 1 glass 1 crystal 1")
        self.assertEqual(c.convert_line("recipe planks 4 log 1"), "craft planks 4 log 1")
        self.assertEqual(c.convert_line("craft plane 1 iron_ore 10 planks 8 white_wool 4"), "craft plane 1 iron_ore 10 @planks 8 white_wool 4")

    def test_other_lines_are_never_touched(self):
        for line in ["bake bread 1 dough 1", "smelt iron_ingot 1 iron_ore 1 coal 1", "# craft x 1 y 1", "", "block stone", "  tex stone",
                     "group planks planks pine_planks", 'shaped x 1 "A" with A stick']:
            self.assertEqual(c.convert_line(line), line)

    def test_converting_twice_changes_nothing_and_repeats_are_dropped(self):
        text = "recipe wood_pick 1 planks 3 stick 2\ncraft wood_pick 1 planks 3 stick 2\ncraft lamp 1 glass 1 crystal 1\n# x\n"
        once = c.convert_text(text)
        self.assertEqual(once.count("wood_pick"), 1)
        self.assertEqual(c.convert_text(once), once)


class Check(unittest.TestCase):
    def test_a_clash_is_found(self):
        a = c.find_conflicts(["craft a 1 stick 2\ncraft a 1 stick 1 coal 1\ncraft a 1 stick 2\n"])
        self.assertEqual(a, [])  # two ways to make the same thing: not a clash
        b = c.find_conflicts(["craft a 1 stick 2\ncraft b 1 stick 2\n"])
        self.assertEqual(len(b), 2)
        # A shaped and a shapeless recipe from the same items clash too.
        d = c.find_conflicts(['craft a 1 stick 2\nshaped b 1 "S" "S" with S stick\n'])
        self.assertEqual(len(d), 1)  # found from the shaped one
        # Told apart by shape.
        e = c.find_conflicts(['shaped a 1 "S" "S" with S stick\nshaped b 1 "SS" with S stick\n'])
        self.assertEqual(e, [])
        # A mirrored shape is the same shape.
        f = c.find_conflicts(['shaped a 1 "SX" with S stick X coal\nshaped b 1 "XS" with S stick X coal\n'])
        self.assertEqual(len(f), 2)

    def test_big_recipes_are_laid_out_in_their_own_bench(self):
        # A 5 x 5 and a 7 x 7 pattern are read without trouble, and two of
        # them told apart by shape do not clash with a table's recipe.
        five = 'shaped a 1 "S...S" "SSSSS" "S...S" with S stick\nshaped b 1 "SSS" with S stick\n'
        self.assertEqual(c.find_conflicts([five]), [])
        recipes = c.read_recipes([five])
        self.assertEqual([c.grid_side(r) for r in recipes], [5, 3])
        seven = 'shaped a 1 "S.....S" with S stick\ncraft b 1 stick 5\n'
        self.assertEqual([c.grid_side(r) for r in c.read_recipes([seven])], [7, 3])
        # The same big shape for two things clashes.
        same = 'shaped a 1 "S...S" "SSSSS" with S stick\nshaped b 1 "S...S" "SSSSS" with S stick\n'
        self.assertEqual(len(c.find_conflicts([same])), 2)
        # Mirrored, and offset in a bigger grid, is the same shape too.
        mirrored = 'shaped a 1 "S...C" "SSSSS" with S stick C coal\nshaped b 1 "C...S" "SSSSS" with S stick C coal\n'
        self.assertEqual(len(c.find_conflicts([mirrored])), 2)

    def test_groups_count(self):
        g = c.find_conflicts(["group w a b\n" + 'shaped x 1 "W" "W" with W @w\nshaped y 1 "A" "A" with A b\n'])
        self.assertEqual(len(g), 2)

    def test_the_games_own_recipes_have_no_clash(self):
        files = [os.path.join(ROOT, "data", "base.txt")] + sorted(glob.glob(os.path.join(MODS_DIR, "*", "pack.txt")))
        self.assertEqual(c.find_conflicts([read(f) for f in files]), [])

    def test_the_files_are_already_converted(self):
        for f in [os.path.join(ROOT, "data", "base.txt")] + sorted(glob.glob(os.path.join(MODS_DIR, "*", "pack.txt"))):
            text = read(f)
            self.assertEqual(c.convert_text(text), text, f)


if __name__ == "__main__":
    unittest.main()
