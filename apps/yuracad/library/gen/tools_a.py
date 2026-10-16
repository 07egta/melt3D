"""Tools and hardware (category どうぐ) -> ../tools_a.json.

Run:  YURACAD_BIN=/path/to/yuracad python3 tools_a.py
Previews go to /tmp/lib_preview_toolsa when YURACAD_BIN is set.

Conventions used here: Box/Cylinder/Tube/PrismN/Extrude are centred on pos in Z, so a part
standing on the ground uses pos z = height / 2. Revolve profiles run from z = 0 upwards.
rot (0, 90, 0) turns a part's Z axis into world +X (lying parts).
"""
import math
import os
from lib import *

# ---- materials (values follow src/materials.rs presets) ----
STEEL = dict(color=(0.74, 0.76, 0.80), roughness=0.28, metallic=1.0)
CHROME = dict(color=(0.90, 0.92, 0.96), roughness=0.08, metallic=1.0)
DARK = dict(color=(0.24, 0.25, 0.29), roughness=0.35, metallic=1.0)
BRASS = dict(color=(0.86, 0.66, 0.32), roughness=0.30, metallic=1.0)
RED = dict(color=(0.85, 0.18, 0.15), roughness=0.55)
WOOD = dict(color=(0.80, 0.58, 0.36), roughness=0.70)
YELLOW = dict(color=(0.98, 0.80, 0.18), roughness=0.35)


def S(kind, name, pos, size=None, mat=STEEL, blend=0.0, **kw):
    """A solid (Add) shape. Hard details use blend 0-1.5; soft grips use a larger blend."""
    return shape(kind, name, pos=pos, size=size, blend=blend, **mat, **kw)


def Sub(kind, name, pos, size=None, **kw):
    """A cut (Subtract). It only affects the shapes listed before it in the group."""
    return S(kind, name, pos, size, blend=0.0, op="Subtract", **kw)


def arc(cx, cy, r, a0, a1, n):
    """Points on a circle from angle a0 to a1 (radians, either direction)."""
    return [[cx + r * math.cos(a0 + (a1 - a0) * i / n), cy + r * math.sin(a0 + (a1 - a0) * i / n)]
            for i in range(n + 1)]


def bar(p0, p1, w0, w1):
    """Quad around the segment p0-p1, half width w0 at p0 and w1 at p1."""
    dx, dy = p1[0] - p0[0], p1[1] - p0[1]
    ln = math.hypot(dx, dy)
    nx, ny = -dy / ln, dx / ln
    return [[p0[0] + nx * w0, p0[1] + ny * w0], [p1[0] + nx * w1, p1[1] + ny * w1],
            [p1[0] - nx * w1, p1[1] - ny * w1], [p0[0] - nx * w0, p0[1] - ny * w0]]


def gear_outline(n, r_outer, r_root):
    """Gear with n trapezoid teeth; 4 points per tooth (4n <= 64)."""
    step = 2 * math.pi / n
    tip, root = step * 0.22, step * 0.36
    pts = []
    for i in range(n):
        a = i * step
        pts += [[r_root * math.cos(a - root), r_root * math.sin(a - root)],
                [r_outer * math.cos(a - tip), r_outer * math.sin(a - tip)],
                [r_outer * math.cos(a + tip), r_outer * math.sin(a + tip)],
                [r_root * math.cos(a + root), r_root * math.sin(a + root)]]
    return pts


items = []

def helix(r, z0, z1, turns, tube, n=32):
    """Thread path (x, y, z, radius) around the Z axis, at most 32 nodes."""
    pts = []
    for i in range(n):
        t = i / (n - 1)
        a = 2 * math.pi * turns * t
        pts.append([r * math.cos(a), r * math.sin(a), z0 + (z1 - z0) * t, tube])
    return pts


# 1. ネジ: a pan-head screw lying on its head, with a slot in the head and a helical thread
screw_profile = [[0, 0], [6, 0], [6, 3.5], [4.6, 5], [2.5, 5], [2.5, 44], [1.2, 46], [0, 46]]
items.append(item("ネジ", ["screw", "ねじ", "ネジ", "fastener", "木ネジ"], group("ネジ", [
    S("Revolve", "ねじ", (-23, 0, 6), outline=screw_profile, rot=(0, 90, 0), mat=STEEL),
    S("Curve", "ねじ山", (-23, 0, 6), (1, 1, 1), mat=STEEL, path=helix(2.4, 7, 42, 4, 0.7), params=(0, 0.7, 0, 1), rot=(0, 90, 0)),
    Sub("Box", "マイナスみぞ", (-18, 0, 6), (3, 12, 1.6)),
])))

# 2. ボルトとナット: a hex bolt standing up, a hex nut lying next to it
items.append(item("ボルトとナット", ["bolt", "nut", "ボルト", "ナット", "hex"], group("ボルトとナット", [
    S("PrismN", "ボルトの頭", (-18, 0, 3.5), (11, 7, 1), mat=STEEL, params=(6, 0, 0, 0)),
    S("Cylinder", "ボルトの軸", (-18, 0, 25), (4.6, 36, 1), mat=STEEL),
    S("Curve", "ねじ山", (-18, 0, 7), (1, 1, 1), mat=STEEL, path=helix(4.0, 0, 36, 5, 0.8), params=(0, 0.7, 0, 1)),
    S("PrismN", "ナット", (22, 0, 4), (9.5, 8, 1), mat=CHROME, params=(6, 0, 0, 0)),
    Sub("Cylinder", "ナットの穴", (22, 0, 4), (5, 12, 1)),
])))

# 3. ワッシャー: a small stack of flat rings
washer_stack = []
for k in range(4):
    washer_stack.append(S("Tube", f"ワッシャー{k + 1}", (k * 0.5, 0, 1.25 + k * 2.9), (15, 7, 2.5), mat=STEEL, round=0.6))
items.append(item("ワッシャー", ["washer", "ワッシャー", "ring", "座金"], group("ワッシャー", washer_stack)))

# 4. スパナ: double-ended open wrench, one flat outline, jaws cut into both heads
R_HEAD, C_HEAD, HALF_HANDLE = 13.0, 46.0, 4.5
join = math.asin(HALF_HANDLE / R_HEAD)
right_head = arc(C_HEAD, 0, R_HEAD, math.pi - join, -(math.pi - join), 20)
left_head = arc(-C_HEAD, 0, R_HEAD, -join, -(2 * math.pi - join), 20)
wrench_outline = right_head + left_head
items.append(item("スパナ", ["wrench", "spanner", "スパナ", "レンチ", "工具"], group("スパナ", [
    S("Extrude", "スパナ本体", (0, 0, 3), (6, 1, 1), mat=CHROME, outline=wrench_outline, round=1.0),
    Sub("Box", "右のあご", (56, 0, 3), (20, 8, 20)),
    Sub("Box", "左のあご", (-56, 0, 3), (20, 8, 20)),
])))

# 5. ハンマー: claw hammer lying down. Head across Y, handle along X, claw cut at -Y
items.append(item("ハンマー", ["hammer", "ハンマー", "金づち", "工具"], group("ハンマー", [
    S("Cylinder", "柄", (-6, 0, 7), (4.5, 90, 1), mat=WOOD, rot=(0, 90, 0), blend=1.0),
    S("Box", "頭", (36, 0, 7), (14, 44, 14), mat=DARK, round=1.0, blend=1.0),
    S("Capsule", "にぎり", (-36, 0, 7), (5.5, 34, 1), mat=RED, rot=(0, 90, 0), blend=4.0),
    Sub("Box", "つめのみぞ", (36, -22, 7), (8, 16, 20)),
])))

# 6. ドライバー: rubber grip (Revolve), hex collar, round shaft, flat blade. Lying along X
grip_profile = [[0, 0], [6.5, 0], [8.5, 3], [9.5, 12], [9.5, 40], [8.5, 47], [7, 50], [0, 50]]
items.append(item("ドライバー", ["screwdriver", "ドライバー", "工具"], group("ドライバー", [
    S("Revolve", "にぎり", (-57, 0, 9.5), outline=grip_profile, rot=(0, 90, 0), mat=RED, blend=4.0),
    S("PrismN", "六角のつば", (-3, 0, 9.5), (5.5, 8, 1), mat=CHROME, rot=(0, 90, 0), params=(6, 0, 0, 0)),
    S("Cylinder", "軸", (25.5, 0, 9.5), (3.0, 49, 1), mat=CHROME, rot=(0, 90, 0)),
    S("Box", "刃", (54, 0, 9.5), (8, 7, 2.2), mat=CHROME),
])))

# 7. ペンチ: closed jaws at the top, handles spread below, both arms bend at the pivot
O = (0.0, 0.0)
jaw_a = bar((-3.5, 46), O, 3.5, 4.5)      # jaw arms meet at the pivot
jaw_b = bar((3.5, 46), O, 3.5, 4.5)
grip_a = bar(O, (16, -50), 4.5, 7.0)      # handles open out below the pivot
grip_b = bar(O, (-16, -50), 4.5, 7.0)
items.append(item("ペンチ", ["pliers", "ペンチ", "工具"], group("ペンチ", [
    S("Extrude", "あご1", (0, 0, 3), (6, 1, 1), mat=STEEL, outline=jaw_a, round=1.0),
    S("Extrude", "あご2", (0, 0, 3), (6, 1, 1), mat=STEEL, outline=jaw_b, round=1.0),
    S("Box", "かみ合わせ", (0, 43, 3), (10, 9, 6), mat=DARK, round=1.0),
    S("Extrude", "にぎり1", (0, 0, 3), (6, 1, 1), mat=RED, outline=grip_a, round=2.5, blend=2.0),
    S("Extrude", "にぎり2", (0, 0, 3), (6, 1, 1), mat=RED, outline=grip_b, round=2.5, blend=2.0),
    S("Cylinder", "かなめのピン", (0, 0, 5), (3.2, 10, 1), mat=DARK),
])))

# 8. ものさし: yellow plastic strip with a 10 mm tick every centimetre
ruler = [S("Box", "ものさし", (0, 0, 1.0), (120, 16, 2.0), mat=YELLOW, round=0.4)]
for i in range(13):
    x = -60 + 10 * i
    ruler.append(Sub("Box", f"目もり{i}", (x, 8 - 3.5, 2.0), (0.8, 7.0, 1.2)))
ruler.append(Sub("Cylinder", "つりさげ穴", (-52, 0, 1.5), (2.5, 4, 1)))
items.append(item("ものさし", ["ruler", "ものさし", "定規", "scale"], group("ものさし", ruler)))

# 9. ちょうつがい: two brass leaves, a knuckle with a pin, four screw holes (open flat)
hinge = [
    S("Box", "板1", (-21.5, 0, 0.8), (37, 30, 1.6), mat=BRASS, round=0.4),
    S("Box", "板2", (21.5, 0, 0.8), (37, 30, 1.6), mat=BRASS, round=0.4),
    S("Cylinder", "関節", (0, 0, 3.2), (3.2, 40, 1), mat=BRASS, rot=(90, 0, 0)),
    S("Cylinder", "軸", (0, 0, 3.2), (1.0, 46, 1), mat=CHROME, rot=(90, 0, 0)),
]
for x in (-31, -12, 12, 31):
    for y in (-8, 8):
        hinge.append(Sub("Cylinder", f"ねじ穴{x}{y}", (x, y, 0.8), (2.0, 4, 1)))
items.append(item("ちょうつがい", ["hinge", "ちょうつがい", "蝶番", "金物"], group("ちょうつがい", hinge)))

# 10. くぎ: a board with three nails standing in it at different depths
nail_profile = [[0, 0], [2.0, 9], [2.0, 16], [4.6, 17], [4.6, 19], [0, 19]]
nails = [S("Box", "板", (0, 0, 5), (90, 40, 10), mat=WOOD, round=0.8)]
for k, x in enumerate((-26, 0, 26)):
    nails.append(S("Revolve", f"くぎ{k + 1}", (x, 0, 1 + k * 0.5), outline=nail_profile, mat=STEEL))
items.append(item("くぎ", ["nail", "くぎ", "釘", "hardware"], group("くぎ", nails)))

# 11. ギア: a big gear (12 teeth) meshing with a small one (6 teeth), both with holes
pitch_big = (28.0 + 23.0) / 2                       # pitch radius of the big gear
pitch_small = 6 * (2 * math.pi * pitch_big / 12) / (2 * math.pi)  # same tooth pitch, 6 teeth
big_x = -12.7
small_x = big_x + pitch_big + pitch_small           # centres a pitch-radius sum apart, so the teeth mesh
big_gear = [
    S("Extrude", "大歯車", (big_x, 0, 4), (8, 1, 1), mat=BRASS, outline=gear_outline(12, 28.0, 23.0), round=0.5),
    S("Cylinder", "大歯車のこしき", (big_x, 0, 13), (10, 10, 1), mat=BRASS, blend=0.0),
    Sub("Cylinder", "大歯車の軸穴", (big_x, 0, 8), (5.0, 40, 1)),
]
for k in range(5):
    a = math.radians(90 + 72 * k)
    big_gear.append(Sub("Cylinder", f"かるめの穴{k + 1}", (big_x + 16 * math.cos(a), 16 * math.sin(a), 4), (4.0, 20, 1)))
small_gear = [
    S("Extrude", "小歯車", (small_x, 0, 4), (8, 1, 1), mat=STEEL,
      outline=gear_outline(6, pitch_small + 2.5, pitch_small - 2.5), round=0.5),
    S("Cylinder", "小歯車のこしき", (small_x, 0, 13), (6, 10, 1), mat=STEEL),
    Sub("Cylinder", "小歯車の軸穴", (small_x, 0, 8), (2.8, 40, 1)),
]
items.append(item("ギア", ["gear", "ギア", "歯車", "cog"], group("ギア", big_gear + small_gear)))

# 12. ブラケット: L bracket from a base plate, an upright, a gusset and holes
items.append(item("ブラケット", ["bracket", "ブラケット", "L字金具", "steel"], group("ブラケット", [
    S("Box", "底板", (0, 0, 2), (60, 40, 4), mat=STEEL, round=0.4),
    S("Box", "立ち板", (-28, 0, 25), (4, 40, 50), mat=STEEL, round=0.4),
    S("Wedge", "補強", (-18, 0, 12), (16, 4, 16), mat=STEEL, blend=1.5),
    Sub("Cylinder", "底の穴1", (14, -10, 2), (3.5, 10, 1)),
    Sub("Cylinder", "底の穴2", (14, 10, 2), (3.5, 10, 1)),
    Sub("Cylinder", "立ちの穴1", (-28, -10, 38), (3.5, 10, 1), rot=(0, 90, 0)),
    Sub("Cylinder", "立ちの穴2", (-28, 10, 38), (3.5, 10, 1), rot=(0, 90, 0)),
])))

assert len(items) == 12, len(items)
for it in items:
    n = len(it["group"]["shapes"])
    assert 3 <= n <= 24, (it["name"], n)

write_category("tools_a.json", "どうぐ", items)

if os.environ.get("YURACAD_BIN"):
    preview(items, outdir="/tmp/lib_preview_toolsa")
