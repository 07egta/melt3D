"""Data regressions for the ready-made MeltCAD animals.

Run: python3 -m unittest apps/meltcad/library/gen/test_animals.py
These tests do not need a graphics driver or an installed meltcad binary.
Rust UI/mesh export integration tests live in src/library.rs.
"""
import json
import math
from pathlib import Path
import sys
import unittest

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import animals


NEEDED = {
    "いぬ": ("dog", ("あし", "みみ", "しっぽ")),
    "ねこ": ("cat", ("あし", "みみ", "しっぽ")),
    "うさぎ": ("rabbit", ("みみ", "しっぽ")),
    "くま": ("bear", ("みみ", "め")),
    "パンダ": ("panda", ("みみ", "めのまわり", "しっぽ")),
    "ぞう": ("elephant", ("はな", "みみ")),
    "きりん": ("giraffe", ("くび", "つの")),
    "ペンギン": ("penguin", ("くちばし", "はね", "あし")),
    "うま": ("horse", ("たてがみ", "ひづめ", "くび", "しっぽ")),
    "さかな": ("fish", ("せびれ", "しっぽ")),
}


class AnimalLibraryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.saved = json.loads((HERE.parent / "animals.json").read_text(encoding="utf-8"))
        cls.index = {i["name"]: i for i in cls.saved["items"]}

    def test_json_matches_python_generator(self):
        expected = {"category": "どうぶつ", "items": animals.items}
        self.assertEqual(self.saved, expected, "Run python3 animals.py to regenerate the catalogue")

    def test_unique_names_and_required_models(self):
        self.assertEqual(self.saved["category"], "どうぶつ")
        self.assertEqual(len(self.index), len(self.saved["items"]), "duplicate animal names")
        for name, (tag, parts) in NEEDED.items():
            self.assertIn(name, self.index)
            animal = self.index[name]
            self.assertIn(tag, animal["tags"])
            shapes = animal["group"]["shapes"]
            self.assertLessEqual(len(shapes), 24, name)
            self.assertGreaterEqual(len(shapes), 6, name)
            labels = [s["name"] for s in shapes]
            for part in parts:
                self.assertTrue(any(part in label for label in labels), f"{name} lacks {part}")
            self.assertTrue(any(s["kind"] == "Curve" for s in shapes) or name == "くま",
                            f"{name} needs sculpted non-spherical parts")

    def test_shape_fields_and_scene_round_trip(self):
        for item in self.saved["items"]:
            self.assertEqual(item["group"]["op"], "Add")
            self.assertTrue(item["group"]["shapes"])
            scene = {"groups": [item["group"]]}
            self.assertEqual(json.loads(json.dumps(scene, ensure_ascii=False)), scene)
            for shape in item["group"]["shapes"]:
                self.assertIn(shape["kind"], ("Sphere", "Capsule", "Curve", "Box", "Revolve", "Extrude",
                                              "Loft", "Hemi", "Cylinder", "Cone", "Torus", "Sweep",
                                              "Prism3", "PrismN", "Octa", "Tetra", "Pyramid", "Tube",
                                              "Wedge", "Star"))
                for field in ("pos", "rot", "scale", "size"):
                    self.assertTrue(all(math.isfinite(v) for v in shape[field]), item["name"])
                self.assertTrue(all(0.01 <= v <= 2000 for v in shape["size"]))
                self.assertTrue(all(0.01 <= v <= 100 for v in shape["scale"]))
                self.assertTrue(all(0 <= c <= 1 for c in shape["material"]["color"]))
                self.assertTrue(0 <= shape["blend"] <= 200)
                self.assertLessEqual(len(shape["path"]), 32)


if __name__ == "__main__":
    unittest.main()
