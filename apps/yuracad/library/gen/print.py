"""3Dプリント (test prints and useful printable parts) -> ../print.json

Run:  YURACAD_BIN=/path/to/yuracad python3 print.py
Previews go to /tmp/lib_preview_print when YURACAD_BIN is set.

Conventions: Box/Cylinder/Tube/PrismN/Extrude are centred on pos in Z, so a part standing on
the bed uses pos z = height / 2. Every part is hard-edged (blend 0-1.5) and sits on z = 0.
rot (-a, 0, 0) leans a Box's local Z toward +Y by a degrees (see tilted()).
"""
import math
import os
from lib import *

GREY = dict(color=(0.56, 0.58, 0.61), roughness=0.55)
ORANGE = dict(color=(0.96, 0.46, 0.13), roughness=0.45)


def P(kind, name, pos, size=None, mat=GREY, blend=0.0, op="Add", **kw):
    """A solid (Add) shape with hard edges."""
    return shape(kind, name, pos=pos, size=size, blend=blend, op=op, **mat, **kw)


def Sub(kind, name, pos, size=None, **kw):
    """A cut (Subtract). It only affects the shapes listed before it in the group."""
    return P(kind, name, pos, size, blend=0.0, op="Subtract", **kw)


def tilted(name, size, bottom_mid, angle, mat=GREY, **kw):
    """A Box whose height (size[2]) leans toward +Y by `angle` degrees from vertical.
    bottom_mid is the middle of its lower face, so the part can be placed on the bed."""
    a = math.radians(angle)
    h = size[2]
    c = (bottom_mid[0], bottom_mid[1] + math.sin(a) * h / 2, bottom_mid[2] + math.cos(a) * h / 2)
    return P("Box", name, c, size, mat=mat, rot=(-angle, 0, 0), **kw)


items = []

# 1. キャリブレーションキューブ: a 20 mm cube with raised X, Y and Z marks on top and front
items.append(item("キャリブレーションキューブ", ["calibration", "cube", "test print", "3dprint", "キャリブレーション", "テスト"],
                  group("キャリブレーションキューブ", [
    P("Box", "立方体", (0, 0, 10), (20, 20, 20), mat=GREY),
    P("Box", "Xの棒", (0, -4, 20.5), (16, 2.4, 2.0), mat=ORANGE, blend=0.5),
    P("Box", "Yの棒", (6, 4, 20.5), (2.4, 10, 2.0), mat=ORANGE, blend=0.5),
    P("Box", "Zの棒", (-6, -10.5, 10), (2.4, 1.6, 14), mat=ORANGE, blend=0.5),
])))

# 2. オーバーハングテスト: a row of overhangs from 20 to 70 degrees (from vertical), one per cell.
#    Each is a right triangle: a vertical wall at its left and a sloped underside going out to +X.
OH_H = 18.0
OH_DEPTH = 30.0
OH_GAP = 3.0
oh_angles = [20, 30, 40, 50, 60, 70]
oh_lengths = [OH_H * math.tan(math.radians(a)) for a in oh_angles]
oh_total = sum(oh_lengths) + OH_GAP * (len(oh_angles) - 1)
oh_shapes = [P("Box", "ベース", (0, 0, 1.5), (oh_total + 10, OH_DEPTH, 3), mat=GREY)]
x = -oh_total / 2
for a, L in zip(oh_angles, oh_lengths):
    # outline in XY is turned by rot X 90: its Y becomes Z (height), the extrude depth becomes Y
    oh_shapes.append(P("Extrude", f"{a}度", (x, 0, 0), (OH_DEPTH, 1, 1), mat=ORANGE,
                       outline=[[0, 0], [L, OH_H], [0, OH_H]], rot=(90, 0, 0)))
    x += L + OH_GAP
items.append(item("オーバーハングテスト", ["overhang", "test print", "3dprint", "オーバーハング", "テスト"],
                  group("オーバーハングテスト", oh_shapes)))

# 3. ブリッジテスト: four pairs of pillars with a bridge of 20, 30, 40 and 50 mm span
br_shapes = [P("Box", "ベース", (0, 0, 1.5), (120, 64, 3), mat=GREY)]
for k, (y, span) in enumerate(zip((-21, -7, 7, 21), (20, 30, 40, 50))):
    xp = span / 2 + 2                              # pillar centres: the gap between them is the span
    for sgn, side in ((-1, "左"), (1, "右")):
        br_shapes.append(P("Box", f"はしら{k + 1}{side}", (sgn * xp, y, 10), (4, 6, 14), mat=GREY))
    br_shapes.append(P("Box", f"はし{span}mm", (0, y, 18), (span + 8, 6, 2), mat=ORANGE))
items.append(item("ブリッジテスト", ["bridge", "bridging", "test print", "3dprint", "ブリッジ", "テスト"],
                  group("ブリッジテスト", br_shapes)))

# 4. おんどタワー: four stacked 12 mm tiers that shrink upward; tier k has k+1 notches on its front
tw_shapes = []
for k, w in enumerate((34.0, 31.0, 28.0, 25.0)):
    z0 = 12.0 * k
    mat = ORANGE if k % 2 == 0 else GREY
    tw_shapes.append(P("Box", f"だん{k + 1}", (0, 0, z0 + 6), (w, w, 12), mat=mat))
    n = k + 1
    for i in range(n):
        nx = (i + 1) * w / (n + 1) - w / 2
        tw_shapes.append(Sub("Box", f"だん{k + 1}のきざみ{i + 1}", (nx, -w / 2, z0 + 6), (3, 6, 5)))
items.append(item("おんどタワー", ["temperature tower", "temp tower", "test print", "3dprint", "おんどタワー", "温度タワー"],
                  group("おんどタワー", tw_shapes)))

# 5. こうさテスト: a plate with six holes (4.0 to 6.0 mm, 0.4 mm steps) and six matching pegs in front
cl_shapes = [P("Box", "板", (0, 0, 2.5), (100, 26, 5), mat=ORANGE, round=0.8)]
cl_d = [4.0, 4.4, 4.8, 5.2, 5.6, 6.0]
cl_x = [-37.5 + 15 * i for i in range(6)]
for d, x in zip(cl_d, cl_x):
    cl_shapes.append(Sub("Cylinder", f"あな{d}", (x, 0, 2.5), (d / 2, 8, 1)))
for d, x in zip(cl_d, cl_x):
    cl_shapes.append(P("Cylinder", f"ペグ{d}", (x, -26, 5), (d / 2 - 0.2, 10, 1), mat=GREY))
items.append(item("こうさテスト", ["clearance", "fit test", "tolerance", "test print", "3dprint", "こうさ", "テスト"],
                  group("こうさテスト", cl_shapes)))

# 6. リトラクションテスト: two thin towers on a base, 42 mm apart, each with a flat cap
rt_shapes = [P("Box", "ベース", (0, 0, 1.5), (80, 30, 3), mat=GREY, round=0.6)]
for sgn, side in ((-1, "左"), (1, "右")):
    x = sgn * 25
    rt_shapes.append(P("Cylinder", f"{side}のね", (x, 0, 6), (7, 6, 1), mat=GREY))
    rt_shapes.append(P("Cylinder", f"{side}のとう", (x, 0, 29), (4, 40, 1), mat=ORANGE))
    rt_shapes.append(P("Cylinder", f"{side}のふた", (x, 0, 50), (6, 2, 1), mat=ORANGE))
items.append(item("リトラクションテスト", ["retraction", "stringing", "test print", "3dprint", "リトラクション", "テスト"],
                  group("リトラクションテスト", rt_shapes)))

# 7. スマホスタンド: a base, a back plate leaning 25 degrees back, and a front lip that holds the phone
ph_angle = 25.0
ph_plate_t, ph_plate_h = 6.0, 95.0
ph_bottom = (0.0, -17.0, 4.0)                      # middle of the plate's lower face
ph_shapes = [
    P("Box", "ベース", (0, 0, 2.5), (90, 72, 5), mat=GREY, round=1.0),
    tilted("背板", (80, ph_plate_t, ph_plate_h), ph_bottom, ph_angle, mat=ORANGE),
    P("Box", "ふちのだい", (0, -22, 8.5), (90, 10, 7), mat=ORANGE),
]
items.append(item("スマホスタンド", ["phone stand", "smartphone", "desk", "test print", "3dprint", "スマホ", "スタンド"],
                  group("スマホスタンド", ph_shapes)))

# 8. ケーブルクリップ: a snap clip for a 8 mm cable, with a slit in front and two screw holes in the foot
cc_shapes = [
    P("Box", "土台", (0, 0, 1.5), (30, 16, 3), mat=GREY),
    P("Tube", "わ", (0, 0, 9), (6.7, 2.5, 12), mat=ORANGE),
    Sub("Cylinder", "ケーブルの穴", (0, 0, 1.5), (4.2, 6, 1)),
    Sub("Box", "すきま", (0, -6.7, 10.5), (2.4, 6, 14)),
    Sub("Cylinder", "ねじ穴1", (-10.5, 0, 1.5), (1.8, 8, 1)),
    Sub("Cylinder", "ねじ穴2", (10.5, 0, 1.5), (1.8, 8, 1)),
]
items.append(item("ケーブルクリップ", ["cable clip", "cable holder", "screw mount", "test print", "3dprint", "ケーブル", "クリップ"],
                  group("ケーブルクリップ", cc_shapes)))

# 9. かべフック: a wall plate with two screw holes and a J hook that comes out toward the viewer
wh_shapes = [
    P("Box", "かべの板", (0, 8, 25), (30, 4, 50), mat=GREY, round=1.5),
    Sub("Cylinder", "ねじ穴1", (-8, 8, 12), (2.0, 10, 1), rot=(90, 0, 0)),
    Sub("Cylinder", "ねじ穴2", (8, 8, 12), (2.0, 10, 1), rot=(90, 0, 0)),
    P("Curve", "フック", (0, 0, 0), mat=ORANGE, blend=0.5,
      path=[[0, 6, 38, 3.5], [0, -2, 38, 3.5], [0, -10, 35, 3.5], [0, -14, 28, 3.5],
            [0, -10, 22, 3.5], [0, -5, 22, 3.5]]),
]
items.append(item("かべフック", ["wall hook", "hook", "screw holes", "test print", "3dprint", "フック"],
                  group("かべフック", wh_shapes)))

# 10. スペーサー: a hex standoff with a through bore and a round foot for bed adhesion
sp_shapes = [
    P("Cylinder", "ふみ台", (0, 0, 0.75), (7.5, 1.5, 1), mat=GREY),
    P("PrismN", "六角", (0, 0, 7.5), (5.8, 12, 1), mat=ORANGE, params=(6, 0, 0, 0)),
    Sub("Cylinder", "あな", (0, 0, 7.5), (2.1, 20, 1)),
]
items.append(item("スペーサー", ["spacer", "standoff", "hex", "bore", "test print", "3dprint", "スペーサー"],
                  group("スペーサー", sp_shapes)))

# 11. うすいかべテスト: eight walls from 0.4 to 2.0 mm thick, side by side, 10 mm apart
tw_thick = [0.4, 0.6, 0.8, 1.0, 1.2, 1.4, 1.6, 2.0]
tl_shapes = [P("Box", "ベース", (0, 0, 1.5), (84, 30, 3), mat=GREY)]
for i, (t, x) in enumerate(zip(tw_thick, [-35 + 10 * i for i in range(8)])):
    tl_shapes.append(P("Box", f"かべ{t}", (x, 0, 13), (t, 24, 20), mat=ORANGE if i % 2 else GREY))
items.append(item("うすいかべテスト", ["thin wall", "wall thickness", "test print", "3dprint", "うすいかべ", "テスト"],
                  group("うすいかべテスト", tl_shapes)))

# 12. ボックスとフタ: a small open box with its lid beside it; the plug on the lid fits the box
bx_w, bx_d, bx_h = 40.0, 30.0, 20.0
bl_shapes = [
    P("Box", "箱", (-26, 0, bx_h / 2), (bx_w, bx_d, bx_h), mat=GREY, round=1.5),
    Sub("Box", "中のくぼみ", (-26, 0, 12), (bx_w - 4, bx_d - 4, bx_h), round=0.0),
    P("Box", "フタ", (26, 0, 1.5), (bx_w, bx_d, 3), mat=ORANGE, round=1.0),
    P("Box", "はめこみ", (26, 0, 4.5), (bx_w - 4.4, bx_d - 4.4, 3), mat=ORANGE),
]
items.append(item("ボックスとフタ", ["box", "lid", "container", "test print", "3dprint", "はこ", "フタ"],
                  group("ボックスとフタ", bl_shapes)))

# 13. ペンスタンド: an open cup with a cross divider that splits it into four pen slots
pn_r, pn_w, pn_h = 26.0, 3.0, 80.0
pn_shapes = [
    P("Tube", "まわり", (0, 0, pn_h / 2), (pn_r, pn_w, pn_h), mat=ORANGE, round=1.0),
    P("Cylinder", "そこ", (0, 0, 1.5), (pn_r - 0.1, 3, 1), mat=ORANGE),
    P("Box", "しきり1", (0, 0, 33), (2.4, 50, 60), mat=GREY),
    P("Box", "しきり2", (0, 0, 33), (50, 2.4, 60), mat=GREY),
]
items.append(item("ペンスタンド", ["pen stand", "pen holder", "desk organiser", "test print", "3dprint", "ペン", "スタンド"],
                  group("ペンスタンド", pn_shapes)))

# 14. ネームプレート: a rounded plate with a raised frame, two raised name bars and a hanging hole
np_shapes = [
    P("Box", "板", (0, 0, 2), (90, 40, 4), mat=ORANGE, round=2.0),
    Sub("Cylinder", "つりさげ穴", (-41, 0, 2), (2.0, 10, 1)),
    P("Box", "ふち上", (0, 15.7, 5.5), (74, 2.6, 3), mat=GREY),
    P("Box", "ふち下", (0, -15.7, 5.5), (74, 2.6, 3), mat=GREY),
    P("Box", "ふち右", (35.7, 0, 5.5), (2.6, 34, 3), mat=GREY),
    P("Box", "ふち左", (-35.7, 0, 5.5), (2.6, 34, 3), mat=GREY),
    P("Box", "名前の棒1", (0, 5, 4.5), (56, 4.5, 3.5), mat=GREY),
    P("Box", "名前の棒2", (-11, -6, 4.5), (34, 4.5, 3.5), mat=GREY),
]
items.append(item("ネームプレート", ["name plate", "nameplate", "name tag", "test print", "3dprint", "ネームプレート", "名札"],
                  group("ネームプレート", np_shapes)))

assert len(items) == 14, len(items)
for it in items:
    n = len(it["group"]["shapes"])
    assert 3 <= n <= 24, (it["name"], n)

write_category("print.json", "3Dプリント", items)

if os.environ.get("YURACAD_BIN"):
    preview(items, outdir="/tmp/lib_preview_print")
