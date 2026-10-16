"""くらしのもの (everyday objects) -> ../tools_b.json

Run: MELTCAD_BIN=... python3 tools_b.py   (writes tools_b.json and renders previews)
"""
import math

from lib import *

FILE = "tools_b.json"
CAT = "くらしのもの"

WHITE = (0.94, 0.94, 0.92)
SILVER = (0.80, 0.81, 0.84)
GOLD = (0.88, 0.70, 0.30)
BLACK = (0.08, 0.08, 0.09)


def add3(a, b):
    return (a[0] + b[0], a[1] + b[1], a[2] + b[2])


def rot_x(v, deg):
    """Rotate a vector about the X axis (same sense as the shape's rot[0])."""
    a = math.radians(deg)
    c, s = math.cos(a), math.sin(a)
    x, y, z = v
    return (x, y * c - z * s, y * s + z * c)


# ---- small constructors (all sizes in mm, same meaning as model.rs size_labels) ----

def rev(name, pts, pos=(0, 0, 0), **kw):
    """Revolve: pts = [[radius, height], ...], height measured from pos.z.
    params[0] is the sweep angle; 0 would be a 1 degree wedge, so always 360."""
    kw.setdefault("params", (360, 0, 0, 0))
    kw.setdefault("blend", 0.5)
    return shape("Revolve", name, pos=pos, outline=pts, **kw)


def curve(name, nodes, **kw):
    """Tube through nodes [x, y, z, radius]. params: closed, smooth, flat ends, thickness."""
    kw.setdefault("params", (0, 0.6, 0, 1))
    kw.setdefault("blend", 1.0)
    return shape("Curve", name, path=nodes, **kw)


def box(name, size, pos, **kw):
    kw.setdefault("blend", 1.0)
    return shape("Box", name, pos=pos, size=size, **kw)


def cyl(name, r, h, pos, rot=(0, 0, 0), **kw):
    kw.setdefault("blend", 1.0)
    return shape("Cylinder", name, pos=pos, size=(r, h, 1), rot=rot, **kw)


def cone(name, r, h, pos, rot=(0, 0, 0), **kw):
    kw.setdefault("blend", 1.0)
    return shape("Cone", name, pos=pos, size=(r, h, 1), rot=rot, **kw)


def sph(name, r, pos, **kw):
    kw.setdefault("blend", 1.0)
    return shape("Sphere", name, pos=pos, size=(r, 1, 1), **kw)


def tor(name, big_r, small_r, pos, rot=(0, 0, 0), **kw):
    kw.setdefault("blend", 1.0)
    return shape("Torus", name, pos=pos, size=(big_r, small_r, 1), rot=rot, **kw)


def hexa(name, r, h, pos, rot=(0, 0, 0), **kw):
    """Six-sided prism: circumradius r, length h along its own z."""
    kw.setdefault("blend", 0.5)
    return shape("PrismN", name, pos=pos, size=(r, h, 1), rot=rot, params=(6, 0, 0, 0), **kw)


def extrude(name, outline, height, pos, **kw):
    """A flat outline in XY, pushed up through pos.z +- height/2."""
    kw.setdefault("blend", 0.5)
    return shape("Extrude", name, pos=pos, size=(height, 1, 1), outline=outline, **kw)


def loft(name, bottom, top, height, pos, **kw):
    kw.setdefault("blend", 1.0)
    return shape("Loft", name, pos=pos, size=(height, 1, 1), outline=bottom, outline2=top, **kw)


def rect(w, d):
    return [[-w / 2, -d / 2], [w / 2, -d / 2], [w / 2, d / 2], [-w / 2, d / 2]]


# ---- the objects ----

def mug():
    return group("マグカップ", [
        rev("カップ", [[0, 0], [22, 0], [22, 46], [19.5, 46], [19.5, 3], [0, 3]],
            color=WHITE, roughness=0.22, coat=0.5, blend=0.5),
        cyl("コーヒー", 19.4, 3, (0, 0, 40.5), color=(0.32, 0.17, 0.08), roughness=0.2, coat=0.6, blend=0.3),
        curve("とって", [[20, 0, 40, 3.2], [30, 0, 41, 3.4], [36, 0, 33, 3.4], [33, 0, 18, 3.4],
                        [24, 0, 12, 3.2], [19, 0, 15, 3.2]],
              color=WHITE, roughness=0.22, coat=0.5, blend=2.0),
    ])


def glass_cup():
    glass_col = (0.86, 0.95, 1.0)
    return group("コップ", [
        rev("グラス", [[0, 0], [26, 0], [32, 95], [30, 95], [24, 5], [0, 5]],
            color=glass_col, glass=1.0, roughness=0.05, blend=0.5),
        cyl("水", 23.8, 60, (0, 0, 37), color=(0.55, 0.82, 1.0), glass=0.6, roughness=0.05, blend=0.3),
        curve("ストロー", [[6, 0, 15, 1.3], [7, 0, 50, 1.3], [10, 0, 90, 1.3], [14, 0, 106, 1.3]],
              color=(0.9, 0.2, 0.25), roughness=0.4, blend=0.5),
    ])


def plain_bottle():
    return group("ふつうのびん", [
        rev("びん", [[0, 0], [24, 0], [25, 4], [25, 52], [13, 62], [10, 70], [10, 80], [12, 84],
                     [12, 88], [0, 88]],
            color=(0.14, 0.4, 0.22), roughness=0.12, coat=0.7, translucency=0.2, blend=0.5),
        cyl("ラベル", 25.4, 26, (0, 0, 28), color=(0.96, 0.95, 0.9), roughness=0.6, blend=0.1),
        cyl("ふた", 12.6, 8, (0, 0, 84), color=SILVER, metallic=0.9, roughness=0.3, blend=0.3),
    ])


def wine_bottle():
    return group("ワインボトル", [
        rev("ボトル", [[0, 0], [16, 0], [17.5, 5], [17.5, 50], [14, 62], [8.5, 78], [8, 100],
                       [9.5, 103], [9.5, 106], [0, 106]],
            color=(0.05, 0.2, 0.1), roughness=0.08, coat=0.9, blend=0.5),
        cyl("ラベル", 17.6, 30, (0, 0, 22), color=(0.93, 0.9, 0.8), roughness=0.6, blend=0.1),
        cyl("ホイル", 9.2, 18, (0, 0, 95), color=GOLD, metallic=0.85, roughness=0.3, blend=0.3),
    ])


def vase():
    return group("はなびん", [
        rev("つぼ", [[0, 0], [16, 0], [20, 4], [27, 20], [26, 32], [18, 46], [8, 60], [7, 72],
                     [9, 76], [0, 76]],
            color=(0.2, 0.45, 0.8), roughness=0.15, coat=0.6, blend=0.5),
        rev("あな", [[0, 4], [14, 4], [24, 20], [23, 32], [16, 44], [7, 58], [5, 72], [5, 80], [0, 80]],
            op="Subtract", blend=0.2),
        curve("くき1", [[0, 0, 25, 1.0], [-2, 0, 50, 1.0], [-4, 0, 72, 1.0], [-5, 0, 92, 1.0]],
              color=(0.3, 0.6, 0.25), roughness=0.5, blend=1.0),
        curve("くき2", [[2, -2, 25, 1.0], [3, -2, 50, 1.0], [3, -2, 70, 1.0], [4, -2, 88, 1.0]],
              color=(0.3, 0.6, 0.25), roughness=0.5, blend=1.0),
        sph("はな1", 9, (-5, 0, 101), color=(1.0, 0.8, 0.1), roughness=0.5, blend=2.0),
        sph("はな2", 8, (4, -2, 95), color=(1.0, 0.42, 0.6), roughness=0.5, blend=2.0),
    ])


def book():
    cover = (0.14, 0.3, 0.62)
    return group("ほん", [
        box("うら表紙", (80, 54, 3), (0, 0, 1.5), round=1.5, color=cover, roughness=0.6, blend=0.5),
        box("ページ", (76, 50, 12), (0, 0, 9), round=0.5, color=(0.96, 0.93, 0.84), roughness=0.8, blend=0.3),
        box("おもて表紙", (80, 54, 2.5), (0, 0, 16.25), round=1.5, color=cover, roughness=0.6, blend=0.5),
        box("背", (3, 54, 17.5), (-38.5, 0, 8.75), round=1.0, color=cover, roughness=0.6, blend=0.5),
        box("ラベル", (30, 22, 0.6), (6, 0, 17.8), color=(0.95, 0.85, 0.4), roughness=0.4, blend=0.2),
        box("しおり", (3, 16, 1.2), (24, 33, 1.5), color=(0.8, 0.12, 0.12), roughness=0.5, blend=0.2),
    ])


def pencil():
    # lies along X: local z is turned onto world +x by rot Y 90
    lay = (0, 90, 0)
    return group("えんぴつ", [
        hexa("ぼう", 4.2, 100, (0, 0, 4.2), rot=lay, color=(0.97, 0.78, 0.12), roughness=0.4, blend=0.3),
        cone("さきの木", 3.7, 13, (56.5, 0, 4.2), rot=lay, color=(0.92, 0.78, 0.6), roughness=0.7, blend=0.3),
        sph("しん", 1.2, (62.5, 0, 4.2), color=(0.15, 0.15, 0.16), roughness=0.5, blend=0.3),
        cyl("けしごむ", 4.2, 6, (-53, 0, 4.2), rot=lay, color=(0.95, 0.6, 0.66), roughness=0.8, blend=0.3),
    ])


def scissors():
    ang = 9.46  # the two levers cross at the pivot, (8, 0)
    blade = (0.80, 0.82, 0.86)
    red = (0.85, 0.15, 0.15)
    return group("はさみ", [
        box("刃A", (96, 5, 3), (8, 0, 2.2), rot=(0, 0, -ang), round=1.0,
            color=blade, metallic=0.9, roughness=0.25, blend=0.3),
        box("刃B", (96, 5, 3), (8, 0, 2.2), rot=(0, 0, ang), round=1.0,
            color=blade, metallic=0.9, roughness=0.25, blend=0.3),
        tor("持ち手A", 9, 2.2, (-48.9, 9.5, 2.2), color=red, roughness=0.4, coat=0.3, blend=1.5),
        tor("持ち手B", 9, 2.2, (-48.9, -9.5, 2.2), color=red, roughness=0.4, coat=0.3, blend=1.5),
        cyl("ねじ", 2.4, 5, (8, 0, 2.5), color=SILVER, metallic=0.9, roughness=0.3, blend=0.2),
    ])


def smartphone():
    body = (0.12, 0.12, 0.14)
    return group("スマホ", [
        box("本体", (54, 6.5, 110), (0, 0, 55), round=4, color=body, metallic=0.5, roughness=0.25, blend=0.5),
        box("画面", (47, 0.8, 100), (0, -3.5, 55), round=3, color=(0.05, 0.2, 0.45),
            glow=0.35, roughness=0.05, coat=0.6, blend=0.2),
        cyl("前カメラ", 1.6, 0.8, (0, -3.8, 101), rot=(90, 0, 0), color=BLACK, roughness=0.3, blend=0.1),
        box("うしろカメラ", (18, 1.5, 18), (-12, 3.5, 92), round=3, color=(0.22, 0.22, 0.24),
            roughness=0.3, blend=0.3),
        cyl("レンズ1", 3.2, 1.2, (-16, 4.6, 96), rot=(90, 0, 0), color=BLACK, roughness=0.2, blend=0.1),
        cyl("レンズ2", 3.2, 1.2, (-8, 4.6, 96), rot=(90, 0, 0), color=BLACK, roughness=0.2, blend=0.1),
        cyl("レンズ3", 2.0, 1.2, (-12, 4.6, 87), rot=(90, 0, 0), color=(0.9, 0.85, 0.5), roughness=0.3, blend=0.1),
    ])


def laptop():
    ang = -20.0  # lid leans back this many degrees
    hinge = (0, 34, 4)
    lid_c = add3(hinge, rot_x((0, 0, 33), ang))
    disp_c = add3(lid_c, rot_x((0, -1.0, 0), ang))
    return group("ノートPC", [
        box("本体", (100, 68, 4), (0, 0, 2), round=1.5, color=(0.7, 0.72, 0.75), metallic=0.8,
            roughness=0.3, blend=0.5),
        box("キーボード", (86, 40, 0.8), (0, -8, 4.3), round=0.3, color=(0.15, 0.15, 0.17),
            roughness=0.6, blend=0.2),
        box("トラックパッド", (26, 16, 0.4), (0, 22, 4.2), color=(0.3, 0.3, 0.32), roughness=0.4, blend=0.1),
        box("ふた", (100, 2, 66), lid_c, rot=(ang, 0, 0), round=0.8, color=(0.78, 0.8, 0.84),
            metallic=0.7, roughness=0.3, blend=0.5),
        box("がめん", (94, 0.8, 58), disp_c, rot=(ang, 0, 0), round=0.5, color=(0.06, 0.12, 0.22),
            glow=0.3, roughness=0.1, blend=0.2),
    ])


def glasses():
    gold = (0.9, 0.72, 0.32)
    lz = 17.5  # lens centre height: the rims stand on the ground
    parts = []
    for sx in (-1, 1):
        cx = 17 * sx
        parts.append(tor("ふち", 15, 1.4, (cx, 0, lz), rot=(90, 0, 0), color=gold, metallic=0.9,
                         roughness=0.3, blend=0.5))
        parts.append(cyl("レンズ", 14.2, 0.5, (cx, 0, lz), rot=(90, 0, 0), color=(0.75, 0.9, 1.0),
                         glass=1.0, roughness=0.05, blend=0.2))
        parts.append(curve("つる", [[cx + sx * 15, 0, lz, 1.1], [cx + sx * 18, 12, lz, 1.1],
                                  [cx + sx * 19, 40, lz - 2, 1.1], [cx + sx * 20, 62, lz - 8, 1.1]],
                           color=gold, metallic=0.9, roughness=0.3, blend=1.0))
    parts.append(capsule_bridge(lz))
    return group("めがね", parts)


def capsule_bridge(lz):
    return shape("Capsule", "ブリッジ", pos=(0, 0, lz + 1.5), size=(1.2, 4.4, 1), rot=(0, 90, 0),
                 color=(0.9, 0.72, 0.32), metallic=0.9, roughness=0.3, blend=0.5)


def umbrella():
    # outer edge from the tip down to the rim, then back up the underside (2.5 mm thick)
    canopy = [[0, 90], [14, 84], [30, 73], [44, 60], [52, 48], [55, 45], [53, 45], [50, 47],
              [41, 57], [27, 69], [13, 80], [0, 87]]
    return group("かさ", [
        rev("てんがい", canopy, color=(0.85, 0.15, 0.2), roughness=0.6, coat=0.2, blend=0.5),
        cyl("じく", 1.5, 86, (0, 0, 43), color=(0.25, 0.25, 0.27), metallic=0.6, roughness=0.3, blend=0.5),
        curve("とって", [[0, 0, 16, 1.4], [-1, 0, 8, 1.4], [-5, 0, 4.5, 1.4], [-9, 0, 7, 1.4],
                        [-10, 0, 13, 1.4]],
              color=(0.55, 0.35, 0.2), roughness=0.5, blend=0.8),
    ])


def bulb():
    return group("でんきゅう", [
        cyl("口金", 13, 20, (0, 0, 10), color=(0.75, 0.75, 0.78), metallic=0.9, roughness=0.3, blend=1.0),
        tor("みぞ1", 13.2, 1.0, (0, 0, 6), color=(0.6, 0.6, 0.63), metallic=0.9, roughness=0.3, blend=0.2),
        tor("みぞ2", 13.2, 1.0, (0, 0, 14), color=(0.6, 0.6, 0.63), metallic=0.9, roughness=0.3, blend=0.2),
        cyl("くび", 10, 8, (0, 0, 24), color=(0.75, 0.75, 0.78), metallic=0.9, roughness=0.3, blend=1.5),
        sph("ガラス球", 28, (0, 0, 56), shell=2.0, glass=0.4, glow=3.0, color=(1.0, 0.96, 0.8),
            roughness=0.1, translucency=0.6, blend=3.0),
        curve("フィラメント", [[-7, 0, 36, 0.6], [-7, 0, 50, 0.5], [-3, 0, 56, 0.5], [0, 0, 48, 0.5],
                              [3, 0, 56, 0.5], [7, 0, 50, 0.5], [7, 0, 36, 0.6]],
              params=(0, 0.0, 0, 1), color=(1.0, 0.6, 0.25), glow=4.0, blend=0.5),
    ])


def broom():
    straw = (0.92, 0.78, 0.4)
    return group("ほうき", [
        # bristles: a wide skirt on the floor that narrows up into the binding
        loft("ほうきの頭", rect(44, 12), rect(14, 8), 18, (0, 0, 9), color=straw, roughness=0.9, blend=1.0),
        box("毛の色", (46, 14, 4), (0, 0, 2), op="Paint", color=(0.7, 0.5, 0.2), roughness=0.9, blend=0.2),
        box("しばり", (16, 10, 3), (0, 0, 15), round=1.0, color=(0.55, 0.38, 0.2), roughness=0.7, blend=0.5),
        cyl("つけね", 2.8, 8, (0, 0, 22), color=SILVER, metallic=0.9, roughness=0.3, blend=1.0),
        cyl("え", 1.6, 80, (0, 0, 66), color=(0.7, 0.5, 0.3), roughness=0.5, blend=1.0),
    ])


def key():
    brass = (0.85, 0.65, 0.3)
    return group("かぎ", [
        tor("わ", 11, 2.6, (-24, 0, 2.6), color=brass, metallic=0.9, roughness=0.3, blend=0.5),
        box("ぼう", (64, 5, 2.4), (18, 0, 2.2), round=0.5, color=SILVER, metallic=0.9, roughness=0.3,
            blend=0.5),
        box("ぎざぎざ1", (3.6, 3.5, 2.4), (36, 4.5, 2.2), color=SILVER, metallic=0.9, roughness=0.3,
            blend=0.3),
        box("ぎざぎざ2", (3.6, 3.5, 2.4), (44, 4.5, 2.2), color=SILVER, metallic=0.9, roughness=0.3,
            blend=0.3),
    ])


def clock():
    body = (0.85, 0.2, 0.2)
    face_y = -14.5
    ang = 60.0
    hdir = (math.sin(math.radians(ang)), 0, math.cos(math.radians(ang)))
    minute_c = (hdir[0] * 8.5, face_y - 1.5, 33 + hdir[2] * 8.5)
    return group("とけい", [
        box("ほんたい", (70, 28, 62), (0, 0, 31), round=10, color=body, roughness=0.4, coat=0.3, blend=1.0),
        cyl("文字板", 23, 2, (0, face_y, 33), rot=(90, 0, 0), color=(0.97, 0.97, 0.94), roughness=0.3,
            blend=0.2),
        box("短針", (1.8, 1.0, 12), (0, face_y - 1.5, 39), color=BLACK, roughness=0.4, blend=0.2),
        shape("Box", "長針", pos=minute_c, size=(1.4, 1.0, 17), rot=(0, ang, 0), color=BLACK,
              roughness=0.4, blend=0.2),
        cyl("中心", 1.8, 1.2, (0, face_y - 1.5, 33), rot=(90, 0, 0), color=BLACK, roughness=0.4, blend=0.1),
        box("目盛り12", (1.2, 0.8, 3), (0, face_y - 1.5, 53), color=BLACK, blend=0.1),
        box("目盛り6", (1.2, 0.8, 3), (0, face_y - 1.5, 13), color=BLACK, blend=0.1),
        box("目盛り3", (3, 0.8, 1.2), (20, face_y - 1.5, 33), color=BLACK, blend=0.1),
        box("目盛り9", (3, 0.8, 1.2), (-20, face_y - 1.5, 33), color=BLACK, blend=0.1),
        sph("ベル左", 8, (-21, 0, 62), color=SILVER, metallic=0.9, roughness=0.3, blend=3.0),
        sph("ベル右", 8, (21, 0, 62), color=SILVER, metallic=0.9, roughness=0.3, blend=3.0),
    ])


def pen_holder():
    teal = (0.3, 0.55, 0.6)
    return group("ペン立て", [
        rev("入れもの", [[0, 0], [22, 0], [22, 60], [20, 60], [20, 3], [0, 3]],
            color=teal, roughness=0.5, coat=0.2, blend=0.5),
        cyl("赤ペン", 2.0, 80, (-8, -6, 45), color=(0.9, 0.2, 0.2), roughness=0.3, coat=0.4, blend=0.3),
        cyl("青ペン", 2.0, 70, (6, 4, 40), color=(0.2, 0.4, 0.9), roughness=0.3, coat=0.4, blend=0.3),
        cyl("黒ペン", 2.0, 88, (2, -8, 49), color=BLACK, roughness=0.3, coat=0.4, blend=0.3),
        cyl("黄ペン", 2.0, 76, (-4, 8, 43), color=(0.95, 0.8, 0.15), roughness=0.3, coat=0.4, blend=0.3),
    ])


def build_items():
    return [
        item("マグカップ", ["mug", "coffee cup", "マグ", "コーヒー"], mug()),
        item("コップ", ["glass", "tumbler", "cup", "グラス", "硝子"], glass_cup()),
        item("ふつうのびん", ["bottle", "glass bottle", "瓶", "びん"], plain_bottle()),
        item("ワインボトル", ["wine", "wine bottle", "葡萄酒"], wine_bottle()),
        item("はなびん", ["vase", "flower vase", "花瓶", "花"], vase()),
        item("ほん", ["book", "本"], book()),
        item("えんぴつ", ["pencil", "鉛筆"], pencil()),
        item("はさみ", ["scissors", "shears", "鋏"], scissors()),
        item("スマホ", ["smartphone", "phone", "スマートフォン", "携帯"], smartphone()),
        item("ノートPC", ["laptop", "notebook", "パソコン", "PC"], laptop()),
        item("めがね", ["glasses", "spectacles", "眼鏡"], glasses()),
        item("かさ", ["umbrella", "parasol", "傘"], umbrella()),
        item("でんきゅう", ["light bulb", "lamp", "電球", "ランプ"], bulb()),
        item("ほうき", ["broom", "箒"], broom()),
        item("かぎ", ["key", "鍵"], key()),
        item("とけい", ["clock", "alarm clock", "時計"], clock()),
        item("ペン立て", ["pen holder", "pen stand", "筆立て", "ペンたて"], pen_holder()),
    ]


if __name__ == "__main__":
    items = build_items()
    write_category(FILE, CAT, items)
    preview(items, outdir="/tmp/lib_preview_toolsb")
