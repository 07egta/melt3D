"""Fixed everyday kitchen models for MeltCAD's くらしのもの category.

Millimetres, Z up. Geometry specs use the existing lib.shape/group/item.
Run "python3 tools_b.py" from this folder to regenerate ../tools_b.json.
Original mug and glass are preserved as separate, distinguishable versions.
"""
import json
from lib import group, item, shape

_SPECS = json.loads(r'''[
  {
    "name": "陶器マグ",
    "tags": [
      "マグカップ",
      "mug",
      "ceramic mug",
      "縦長",
      "コーヒーカップ"
    ],
    "shapes": [
      {
        "kind": "Revolve",
        "name": "中空の陶器",
        "outline": [
          [
            0,
            0
          ],
          [
            29,
            0
          ],
          [
            31.5,
            4
          ],
          [
            33,
            12
          ],
          [
            33,
            68
          ],
          [
            32,
            74
          ],
          [
            28.5,
            74
          ],
          [
            28.5,
            67
          ],
          [
            28.8,
            12
          ],
          [
            27,
            6
          ],
          [
            0,
            6
          ]
        ],
        "color": [
          0.88,
          0.78,
          0.64
        ],
        "roughness": 0.3,
        "coat": 0.38,
        "blend": 0.3
      },
      {
        "kind": "Curve",
        "name": "大きな取っ手",
        "path": [
          [
            30,
            0,
            61,
            4.5
          ],
          [
            43,
            0,
            65,
            4.5
          ],
          [
            53,
            0,
            56,
            4.8
          ],
          [
            57,
            0,
            40,
            4.8
          ],
          [
            52,
            0,
            26,
            4.6
          ],
          [
            40,
            0,
            22,
            4.6
          ],
          [
            31,
            0,
            27,
            4.5
          ]
        ],
        "color": [
          0.88,
          0.78,
          0.64
        ],
        "roughness": 0.3,
        "coat": 0.38,
        "blend": 1.1
      },
      {
        "kind": "Torus",
        "name": "飲み口の縁",
        "pos": [
          0,
          0,
          73
        ],
        "size": [
          30,
          1.5,
          1
        ],
        "color": [
          0.88,
          0.78,
          0.64
        ],
        "roughness": 0.3,
        "coat": 0.38,
        "blend": 0.25
      }
    ]
  },
  {
    "name": "ロックグラス",
    "tags": [
      "コップ",
      "glass",
      "lowball glass",
      "rock glass",
      "厚底",
      "タンブラー"
    ],
    "shapes": [
      {
        "kind": "Revolve",
        "name": "厚底のグラス",
        "outline": [
          [
            0,
            0
          ],
          [
            33,
            0
          ],
          [
            35,
            3
          ],
          [
            37,
            12
          ],
          [
            39,
            67
          ],
          [
            38.5,
            71
          ],
          [
            34,
            71
          ],
          [
            33.2,
            65
          ],
          [
            31,
            14
          ],
          [
            29,
            10
          ],
          [
            0,
            10
          ]
        ],
        "color": [
          0.83,
          0.93,
          0.98
        ],
        "glass": 0.95,
        "roughness": 0.04,
        "blend": 0.2
      },
      {
        "kind": "Torus",
        "name": "底の縁",
        "pos": [
          0,
          0,
          3
        ],
        "size": [
          33,
          1.4,
          1
        ],
        "color": [
          0.83,
          0.93,
          0.98
        ],
        "glass": 0.8,
        "roughness": 0.07,
        "blend": 0.2
      }
    ]
  },
  {
    "name": "お皿",
    "tags": [
      "plate",
      "dish",
      "皿",
      "平皿",
      "食器",
      "dinner plate"
    ],
    "shapes": [
      {
        "kind": "Revolve",
        "name": "浅い皿の本体",
        "outline": [
          [
            0,
            0
          ],
          [
            35,
            0
          ],
          [
            69,
            1.8
          ],
          [
            85,
            4
          ],
          [
            97,
            9
          ],
          [
            108,
            16
          ],
          [
            110,
            19
          ],
          [
            107,
            21
          ],
          [
            102,
            18
          ],
          [
            95,
            13
          ],
          [
            83,
            9
          ],
          [
            68,
            6
          ],
          [
            33,
            5
          ],
          [
            0,
            5
          ]
        ],
        "color": [
          0.96,
          0.95,
          0.88
        ],
        "roughness": 0.32,
        "coat": 0.32,
        "blend": 0.2
      },
      {
        "kind": "Torus",
        "name": "細いリム",
        "pos": [
          0,
          0,
          19
        ],
        "size": [
          107,
          1.3,
          1
        ],
        "color": [
          0.96,
          0.95,
          0.88
        ],
        "roughness": 0.32,
        "coat": 0.32,
        "blend": 0.2
      }
    ]
  },
  {
    "name": "お茶碗",
    "tags": [
      "rice bowl",
      "bowl",
      "茶碗",
      "ご飯茶碗",
      "陶器",
      "食器"
    ],
    "shapes": [
      {
        "kind": "Revolve",
        "name": "高台付きの中空の碗",
        "outline": [
          [
            0,
            9
          ],
          [
            27,
            9
          ],
          [
            30,
            1
          ],
          [
            32,
            0
          ],
          [
            37,
            0
          ],
          [
            40,
            9
          ],
          [
            46,
            15
          ],
          [
            51,
            25
          ],
          [
            56,
            41
          ],
          [
            59,
            59
          ],
          [
            58.5,
            64
          ],
          [
            55,
            65
          ],
          [
            52.5,
            61
          ],
          [
            49,
            42
          ],
          [
            44,
            28
          ],
          [
            36,
            17
          ],
          [
            0,
            15
          ]
        ],
        "color": [
          0.27,
          0.48,
          0.66
        ],
        "roughness": 0.35,
        "coat": 0.35,
        "blend": 0.25
      },
      {
        "kind": "Torus",
        "name": "縁の釉薬",
        "pos": [
          0,
          0,
          64
        ],
        "size": [
          56.8,
          1.1,
          1
        ],
        "color": [
          0.34,
          0.55,
          0.71
        ],
        "roughness": 0.26,
        "coat": 0.42,
        "blend": 0.15
      }
    ]
  },
  {
    "name": "フライパン",
    "tags": [
      "frying pan",
      "pan",
      "skillet",
      "キッチン",
      "調理器具",
      "なべ"
    ],
    "shapes": [
      {
        "kind": "Revolve",
        "name": "浅く広い金属のパン",
        "outline": [
          [
            0,
            0
          ],
          [
            94,
            0
          ],
          [
            102,
            3
          ],
          [
            108,
            9
          ],
          [
            115,
            29
          ],
          [
            119,
            41
          ],
          [
            116,
            43
          ],
          [
            113,
            39
          ],
          [
            106,
            13
          ],
          [
            98,
            5
          ],
          [
            0,
            5
          ]
        ],
        "color": [
          0.15,
          0.16,
          0.17
        ],
        "metallic": 0.76,
        "roughness": 0.35,
        "blend": 0.15
      },
      {
        "kind": "Curve",
        "name": "長い持ち手",
        "path": [
          [
            116,
            0,
            34,
            8
          ],
          [
            137,
            0,
            42,
            8.5
          ],
          [
            175,
            0,
            48,
            10
          ],
          [
            228,
            0,
            49,
            10
          ],
          [
            286,
            0,
            50,
            8
          ],
          [
            301,
            0,
            50,
            6.5
          ]
        ],
        "color": [
          0.12,
          0.13,
          0.14
        ],
        "roughness": 0.44,
        "blend": 1.5
      },
      {
        "kind": "Cylinder",
        "name": "固定リベット1",
        "pos": [
          107,
          -14,
          30
        ],
        "size": [
          3.6,
          2,
          1
        ],
        "rot": [
          0,
          90,
          0
        ],
        "color": [
          0.74,
          0.77,
          0.79
        ],
        "metallic": 0.92,
        "roughness": 0.23,
        "blend": 0.2
      },
      {
        "kind": "Cylinder",
        "name": "固定リベット2",
        "pos": [
          107,
          14,
          30
        ],
        "size": [
          3.6,
          2,
          1
        ],
        "rot": [
          0,
          90,
          0
        ],
        "color": [
          0.74,
          0.77,
          0.79
        ],
        "metallic": 0.92,
        "roughness": 0.23,
        "blend": 0.2
      }
    ]
  },
  {
    "name": "両手鍋",
    "tags": [
      "pot",
      "cooking pot",
      "saucepot",
      "両手なべ",
      "鍋",
      "キッチン"
    ],
    "shapes": [
      {
        "kind": "Revolve",
        "name": "深い中空の鍋",
        "outline": [
          [
            0,
            0
          ],
          [
            88,
            0
          ],
          [
            94,
            4
          ],
          [
            97,
            9
          ],
          [
            97,
            94
          ],
          [
            99,
            101
          ],
          [
            97,
            104
          ],
          [
            93,
            102
          ],
          [
            92,
            9
          ],
          [
            86,
            6
          ],
          [
            0,
            6
          ]
        ],
        "color": [
          0.74,
          0.77,
          0.79
        ],
        "metallic": 0.84,
        "roughness": 0.28,
        "blend": 0.15
      },
      {
        "kind": "Torus",
        "name": "左の輪の取っ手",
        "pos": [
          -114,
          0,
          78
        ],
        "rot": [
          90,
          0,
          0
        ],
        "size": [
          17,
          4.8,
          1
        ],
        "color": [
          0.28,
          0.3,
          0.32
        ],
        "metallic": 0.58,
        "roughness": 0.32,
        "blend": 1
      },
      {
        "kind": "Torus",
        "name": "右の輪の取っ手",
        "pos": [
          114,
          0,
          78
        ],
        "rot": [
          90,
          0,
          0
        ],
        "size": [
          17,
          4.8,
          1
        ],
        "color": [
          0.28,
          0.3,
          0.32
        ],
        "metallic": 0.58,
        "roughness": 0.32,
        "blend": 1
      },
      {
        "kind": "Torus",
        "name": "上端の巻き縁",
        "pos": [
          0,
          0,
          101
        ],
        "size": [
          97,
          1.8,
          1
        ],
        "color": [
          0.74,
          0.77,
          0.79
        ],
        "metallic": 0.84,
        "roughness": 0.28,
        "blend": 0.3
      }
    ]
  },
  {
    "name": "やかん",
    "tags": [
      "kettle",
      "tea kettle",
      "湯沸かし",
      "ケトル",
      "調理器具"
    ],
    "shapes": [
      {
        "kind": "Revolve",
        "name": "中が空洞の金属胴",
        "outline": [
          [
            0,
            0
          ],
          [
            63,
            0
          ],
          [
            75,
            5
          ],
          [
            87,
            24
          ],
          [
            93,
            49
          ],
          [
            90,
            73
          ],
          [
            72,
            93
          ],
          [
            42,
            106
          ],
          [
            38,
            108
          ],
          [
            35,
            105
          ],
          [
            38,
            101
          ],
          [
            67,
            88
          ],
          [
            85,
            69
          ],
          [
            88,
            49
          ],
          [
            82,
            28
          ],
          [
            70,
            11
          ],
          [
            62,
            6
          ],
          [
            0,
            6
          ]
        ],
        "color": [
          0.74,
          0.77,
          0.79
        ],
        "metallic": 0.74,
        "roughness": 0.29,
        "blend": 0.15
      },
      {
        "kind": "Curve",
        "name": "注ぎ口の外壁",
        "path": [
          [
            73,
            0,
            70,
            17
          ],
          [
            89,
            0,
            78,
            16
          ],
          [
            108,
            0,
            100,
            13
          ],
          [
            128,
            0,
            121,
            11
          ]
        ],
        "params": [
          0,
          0.35,
          0,
          1
        ],
        "color": [
          0.74,
          0.77,
          0.79
        ],
        "metallic": 0.74,
        "roughness": 0.29,
        "blend": 0.7
      },
      {
        "kind": "Curve",
        "name": "注ぎ口の水路",
        "path": [
          [
            70,
            0,
            69,
            10
          ],
          [
            89,
            0,
            80,
            9
          ],
          [
            108,
            0,
            102,
            7.5
          ],
          [
            132,
            0,
            125,
            7.5
          ]
        ],
        "params": [
          0,
          0.35,
          0,
          1
        ],
        "op": "Subtract",
        "blend": 0.05
      },
      {
        "kind": "Curve",
        "name": "上に渡した持ち手",
        "path": [
          [
            -72,
            0,
            83,
            6
          ],
          [
            -76,
            0,
            111,
            6
          ],
          [
            -62,
            0,
            136,
            6
          ],
          [
            -33,
            0,
            152,
            6
          ],
          [
            0,
            0,
            156,
            6
          ],
          [
            33,
            0,
            152,
            6
          ],
          [
            62,
            0,
            136,
            6
          ],
          [
            76,
            0,
            111,
            6
          ],
          [
            72,
            0,
            83,
            6
          ]
        ],
        "params": [
          0,
          0.45,
          0,
          1
        ],
        "color": [
          0.12,
          0.13,
          0.14
        ],
        "roughness": 0.42,
        "blend": 0.8
      },
      {
        "kind": "Torus",
        "name": "注ぎ口の縁",
        "pos": [
          0,
          0,
          106
        ],
        "size": [
          37,
          1.8,
          1
        ],
        "color": [
          0.74,
          0.77,
          0.79
        ],
        "metallic": 0.74,
        "roughness": 0.29,
        "blend": 0.2
      }
    ]
  },
  {
    "name": "スプーン",
    "tags": [
      "spoon",
      "cutlery",
      "食器",
      "カトラリー",
      "匙"
    ],
    "shapes": [
      {
        "kind": "Sphere",
        "name": "楕円のすくう部分",
        "pos": [
          0,
          -64,
          7.5
        ],
        "size": [
          28,
          1,
          1
        ],
        "scale": [
          0.71,
          1,
          0.25
        ],
        "color": [
          0.74,
          0.77,
          0.79
        ],
        "metallic": 0.92,
        "roughness": 0.19,
        "blend": 0.1
      },
      {
        "kind": "Sphere",
        "name": "すくう部分のくぼみ",
        "pos": [
          0,
          -64,
          14
        ],
        "size": [
          23,
          1,
          1
        ],
        "scale": [
          0.72,
          1.06,
          0.47
        ],
        "op": "Subtract",
        "blend": 0.05
      },
      {
        "kind": "Extrude",
        "name": "細く広がる柄",
        "pos": [
          0,
          0,
          2.5
        ],
        "size": [
          4.4,
          1,
          1
        ],
        "outline": [
          [
            -11,
            -48
          ],
          [
            11,
            -48
          ],
          [
            8,
            -31
          ],
          [
            6,
            50
          ],
          [
            8,
            88
          ],
          [
            -8,
            88
          ],
          [
            -6,
            50
          ],
          [
            -8,
            -31
          ]
        ],
        "round": 0.65,
        "color": [
          0.74,
          0.77,
          0.79
        ],
        "metallic": 0.92,
        "roughness": 0.19,
        "blend": 0.8
      }
    ]
  },
  {
    "name": "フォーク",
    "tags": [
      "fork",
      "cutlery",
      "食器",
      "カトラリー",
      "四本歯"
    ],
    "shapes": [
      {
        "kind": "Extrude",
        "name": "細長い柄",
        "pos": [
          0,
          0,
          2.25
        ],
        "size": [
          4,
          1,
          1
        ],
        "outline": [
          [
            -7,
            -42
          ],
          [
            7,
            -42
          ],
          [
            6,
            31
          ],
          [
            8,
            89
          ],
          [
            -8,
            89
          ],
          [
            -6,
            31
          ]
        ],
        "round": 0.55,
        "color": [
          0.74,
          0.77,
          0.79
        ],
        "metallic": 0.9,
        "roughness": 0.21,
        "blend": 0.6
      },
      {
        "kind": "Extrude",
        "name": "歯の根元",
        "pos": [
          0,
          0,
          2
        ],
        "size": [
          3.4,
          1,
          1
        ],
        "outline": [
          [
            -7,
            -29
          ],
          [
            7,
            -29
          ],
          [
            20,
            -50
          ],
          [
            20,
            -61
          ],
          [
            -20,
            -61
          ],
          [
            -20,
            -50
          ]
        ],
        "round": 0.2,
        "color": [
          0.74,
          0.77,
          0.79
        ],
        "metallic": 0.9,
        "roughness": 0.21,
        "blend": 0.65
      },
      {
        "kind": "Box",
        "name": "歯1",
        "pos": [
          -15,
          -78,
          2
        ],
        "size": [
          5.4,
          38,
          3.4
        ],
        "round": 1.15,
        "color": [
          0.74,
          0.77,
          0.79
        ],
        "metallic": 0.9,
        "roughness": 0.21,
        "blend": 0.7
      },
      {
        "kind": "Box",
        "name": "歯2",
        "pos": [
          -5,
          -78,
          2
        ],
        "size": [
          5.4,
          38,
          3.4
        ],
        "round": 1.15,
        "color": [
          0.74,
          0.77,
          0.79
        ],
        "metallic": 0.9,
        "roughness": 0.21,
        "blend": 0.7
      },
      {
        "kind": "Box",
        "name": "歯3",
        "pos": [
          5,
          -78,
          2
        ],
        "size": [
          5.4,
          38,
          3.4
        ],
        "round": 1.15,
        "color": [
          0.74,
          0.77,
          0.79
        ],
        "metallic": 0.9,
        "roughness": 0.21,
        "blend": 0.7
      },
      {
        "kind": "Box",
        "name": "歯4",
        "pos": [
          15,
          -78,
          2
        ],
        "size": [
          5.4,
          38,
          3.4
        ],
        "round": 1.15,
        "color": [
          0.74,
          0.77,
          0.79
        ],
        "metallic": 0.9,
        "roughness": 0.21,
        "blend": 0.7
      }
    ]
  },
  {
    "name": "包丁",
    "tags": [
      "knife",
      "kitchen knife",
      "chef knife",
      "料理包丁",
      "ナイフ",
      "刃物"
    ],
    "shapes": [
      {
        "kind": "Extrude",
        "name": "薄い金属刃",
        "pos": [
          0,
          0,
          1.5
        ],
        "size": [
          3,
          1,
          1
        ],
        "outline": [
          [
            -177,
            -11
          ],
          [
            -169,
            -21
          ],
          [
            -142,
            -38
          ],
          [
            -105,
            -45
          ],
          [
            -20,
            -45
          ],
          [
            -12,
            -42
          ],
          [
            -12,
            -5
          ],
          [
            -52,
            -4
          ],
          [
            -130,
            -4
          ]
        ],
        "color": [
          0.74,
          0.77,
          0.79
        ],
        "metallic": 0.95,
        "roughness": 0.2,
        "blend": 0.12
      },
      {
        "kind": "Box",
        "name": "刃の根元の口金",
        "pos": [
          -4,
          -15,
          8.2
        ],
        "size": [
          13,
          31,
          15
        ],
        "round": 2,
        "color": [
          0.54,
          0.57,
          0.6
        ],
        "metallic": 0.91,
        "roughness": 0.25,
        "blend": 0.6
      },
      {
        "kind": "Box",
        "name": "黒い持ち手",
        "pos": [
          62,
          -15,
          8.3
        ],
        "size": [
          130,
          27,
          15
        ],
        "round": 5,
        "color": [
          0.12,
          0.13,
          0.14
        ],
        "roughness": 0.43,
        "blend": 0.8
      },
      {
        "kind": "Cylinder",
        "name": "柄の鋲1",
        "pos": [
          26,
          -29,
          8.5
        ],
        "rot": [
          90,
          0,
          0
        ],
        "size": [
          2.5,
          1.5,
          1
        ],
        "color": [
          0.74,
          0.77,
          0.79
        ],
        "metallic": 0.9,
        "roughness": 0.26,
        "blend": 0.2
      },
      {
        "kind": "Cylinder",
        "name": "柄の鋲2",
        "pos": [
          66,
          -29,
          8.5
        ],
        "rot": [
          90,
          0,
          0
        ],
        "size": [
          2.5,
          1.5,
          1
        ],
        "color": [
          0.74,
          0.77,
          0.79
        ],
        "metallic": 0.9,
        "roughness": 0.26,
        "blend": 0.2
      },
      {
        "kind": "Cylinder",
        "name": "柄の鋲3",
        "pos": [
          106,
          -29,
          8.5
        ],
        "rot": [
          90,
          0,
          0
        ],
        "size": [
          2.5,
          1.5,
          1
        ],
        "color": [
          0.74,
          0.77,
          0.79
        ],
        "metallic": 0.9,
        "roughness": 0.26,
        "blend": 0.2
      }
    ]
  }
]''')

def build_kitchen_items():
    return [
        item(spec["name"], spec["tags"], group(spec["name"], [
            shape(s["kind"], s["name"], **{k: v for k, v in s.items() if k not in ("kind", "name")})
            for s in spec["shapes"]
        ]))
        for spec in _SPECS
    ]

if __name__ == "__main__":
    xs = build_kitchen_items()
    assert len(xs) == 10 and len(set(i["name"] for i in xs)) == 10
    assert all(1 <= len(i["group"]["shapes"]) <= 24 for i in xs)
    print("10 valid models; regenerate tools_b.json through tools_b.py")
