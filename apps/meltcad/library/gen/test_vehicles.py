"""Regression checks for fixed everyday vehicle generators.

Run: cd apps/meltcad/library/gen && python3 -m unittest test_vehicles
"""
import json
import os
import unittest
from vehicles import make_items

NAMES = ["セダン", "軽自動車", "SUV", "バス", "トラック", "自転車", "バイク", "電車", "飛行機", "船"]
PARTS = {
    "セダン": ["キャビン", "左前タイヤ", "フロントガラス"],
    "軽自動車": ["背の高いキャビン", "左前タイヤ", "フロントガラス"],
    "SUV": ["ルーフレール左", "右後タイヤ", "フロントガラス"],
    "バス": ["車体", "左前タイヤ"],
    "トラック": ["運転席", "荷台", "左後タイヤ"],
    "自転車": ["ダウンチューブ", "ハンドル", "前タイヤ"],
    "バイク": ["燃料タンク", "ハンドル", "左フロントフォーク"],
    "電車": ["車体", "パンタグラフ", "左車輪1"],
    "飛行機": ["機体", "主翼", "垂直尾翼", "水平尾翼"],
    "船": ["船体", "操縦室"],
}

class TestEverydayVehicles(unittest.TestCase):
    def setUp(self):
        self.items = make_items()
        self.models = {model["name"]: model for model in self.items}

    def test_no_duplicate_preset(self):
        self.assertEqual(len(self.items), len(self.models))
        self.assertEqual(len(self.items), 15)  # five additional distinct existing models
        for name in NAMES:
            self.assertIn(name, self.models)

    def test_required_real_3d_parts(self):
        for name, required in PARTS.items():
            shapes = self.models[name]["group"]["shapes"]
            self.assertTrue(5 <= len(shapes) <= 40, name)
            parts = {shape["name"] for shape in shapes}
            for part in required:
                self.assertIn(part, parts)
            for shape in shapes:
                self.assertTrue(all(0.01 <= v <= 2000 for v in shape["size"]))
                self.assertTrue(all(abs(v) <= 5000 for v in shape["pos"]))

    def test_generated_library_matches_source(self):
        path = os.path.join(os.path.dirname(__file__), "..", "vehicles.json")
        with open(path, encoding="utf-8") as f:
            stored = json.load(f)
        self.assertEqual(stored, {"category": "のりもの", "items": self.items})

    def test_old_japanese_search_aliases(self):
        for name, old in [("セダン", "くるま"), ("自転車", "じてんしゃ"),
                          ("電車", "でんしゃ"), ("飛行機", "ひこうき"), ("船", "ふね")]:
            self.assertIn(old, self.models[name]["tags"])

if __name__ == "__main__":
    unittest.main()
