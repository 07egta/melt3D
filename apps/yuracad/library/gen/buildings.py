"""Ready-made buildings and landmarks (category たてもの). Writes ../buildings.json.

Run:  YURACAD_BIN=/path/to/yuracad python3 buildings.py
All models: mm, Z up, stand on z = 0, front toward -Y.
Box size = full extents; Cylinder/Cone size = (radius, height); Extrude/Revolve use outline.
Windows: a Subtract hole through the wall, then a recessed glass pane (blend 0 keeps it crisp).
"""
import math

from lib import *

CREAM = (0.95, 0.90, 0.78)
BRICK = (0.72, 0.42, 0.32)
STONE = (0.80, 0.76, 0.68)
ROOF_RED = (0.72, 0.28, 0.20)
ROOF_BLUE = (0.25, 0.38, 0.62)
DARK_ROOF = (0.22, 0.22, 0.25)
WOOD = (0.52, 0.32, 0.18)
GLASS = (0.55, 0.78, 0.95)
CONCRETE = (0.78, 0.82, 0.86)
GREY = (0.55, 0.55, 0.57)
RED = (0.80, 0.22, 0.16)
WHITE = (0.96, 0.95, 0.92)
VERMILION = (0.82, 0.24, 0.16)
GOLD = (1.00, 0.78, 0.35)
ORANGE = (0.95, 0.48, 0.16)
SAND = (0.87, 0.76, 0.52)
GREEN = (0.36, 0.56, 0.32)


def box(name, pos, size, color, op="Add", blend=0.0, rough=0.7, metallic=0.0, glass=0.0, rot=(0, 0, 0), rnd=0.0):
    return shape("Box", name, pos=pos, size=size, color=color, op=op, blend=blend, roughness=rough,
                 metallic=metallic, glass=glass, rot=rot, round=rnd)


def cyl(name, pos, r, h, color, rot=(0, 0, 0), rough=0.6, metallic=0.0, glass=0.0, op="Add", blend=0.0):
    return shape("Cylinder", name, pos=pos, size=(r, h, 1), color=color, rot=rot, roughness=rough,
                 metallic=metallic, glass=glass, op=op, blend=blend)


def cone(name, pos, r, h, color, rough=0.6):
    return shape("Cone", name, pos=pos, size=(r, h, 1), color=color, roughness=rough, blend=0.0)


def gable(name, x_half, height, base_z, depth, color, y=0.0, rough=0.6):
    """Triangular roof/tent: a triangle in the XZ plane, extruded along Y (ridge runs front to back)."""
    return shape("Extrude", name, pos=(0, y, base_z), size=(depth, 1, 1), rot=(90, 0, 0),
                 outline=[[-x_half, 0], [x_half, 0], [0, height]], color=color, roughness=rough, blend=0.0)


def win_front(name, x, z, w, h, y_face, depth=6.0):
    """Window in a wall whose front face is at y = y_face (front = -Y): hole + recessed glass."""
    return [box(name + "あな", (x, y_face, z), (w, depth, h), (0, 0, 0), op="Subtract"),
            box(name + "ガラス", (x, y_face + 1.5, z), (w - 2, 1.0, h - 2), GLASS, glass=1.0, rough=0.05)]


def win_side(name, x_face, y, z, w, h, outward, depth=6.0):
    """Window on a side wall at x = x_face; outward = +1 (right wall) or -1 (left wall)."""
    return [box(name + "あな", (x_face, y, z), (depth, w, h), (0, 0, 0), op="Subtract"),
            box(name + "ガラス", (x_face - outward * 1.5, y, z), (1.0, w - 2, h - 2), GLASS, glass=1.0,
                rough=0.05)]


# ---------------------------------------------------------------- いえ
def house():
    s = [
        box("かべ", (0, 0, 20), (70, 56, 40), CREAM, rough=0.8),
        gable("やね", 40, 26, 40, 66, ROOF_RED),
        box("えんとつ", (20, 10, 50), (8, 8, 20), BRICK, rough=0.9),
        box("とびら穴", (0, -28, 11), (12, 6, 22), (0, 0, 0), op="Subtract"),
        box("とびら", (0, -26.5, 11), (10, 2, 20), WOOD, rough=0.6),
        box("ステップ", (0, -31, 1), (18, 6, 2), STONE, rough=0.9),
    ]
    s += win_front("まど左", -20, 30, 12, 12, -28)
    s += win_front("まど右", 20, 30, 12, 12, -28)
    return group("いえ", s)


# ---------------------------------------------------------------- ビル
def building():
    s = [
        box("ほんたい", (0, 0, 44), (50, 44, 88), CONCRETE, rough=0.35),
        box("さいじょうかい", (0, 0, 96), (36, 34, 16), CONCRETE, rough=0.35),
        cyl("アンテナ", (0, 0, 110), 0.8, 12, GREY, metallic=0.8, rough=0.3),
        box("ロビーあな", (0, -22, 6), (30, 6, 12), (0, 0, 0), op="Subtract"),
        box("ロビーガラス", (0, -20.5, 6), (28, 1.0, 10), GLASS, glass=1.0, rough=0.05),
    ]
    for z in (18, 36, 54, 72):
        s += [box("まどあな", (0, -22, z), (44, 6, 7), (0, 0, 0), op="Subtract"),
              box("まどガラス", (0, -20.5, z), (42, 1.0, 5), GLASS, glass=1.0, rough=0.05)]
    return group("ビル", s)


# ---------------------------------------------------------------- おしろ
def castle():
    s = [
        box("おしろ本丸", (0, 0, 20), (60, 40, 40), STONE, rough=0.9),
    ]
    for sx in (-30, 30):
        for sy in (-20, 20):
            s.append(cyl("とう", (sx, sy, 25), 9, 50, STONE, rough=0.9))
    for sx in (-30, 30):
        for sy in (-20, 20):
            s.append(cone("とうのやね", (sx, sy, 63), 11, 26, ROOF_BLUE))
    for x in (-24, -12, 0, 12, 24):
        s.append(box("ひめがき", (x, -17, 43), (6, 6, 6), STONE, rough=0.9))
    s.append(box("もん穴", (0, -20, 9), (14, 12, 18), (0, 0, 0), op="Subtract"))
    s.append(cyl("もんアーチ", (0, -20, 18), 7, 12, (0, 0, 0), rot=(90, 0, 0), op="Subtract"))
    s.append(cyl("はたざお", (0, 0, 52), 0.8, 24, GREY, metallic=0.6, rough=0.3))
    s.append(box("はた", (7, 0, 58), (12, 1, 7), RED, rough=0.6))
    return group("おしろ", s)


# ---------------------------------------------------------------- とう
def tower():
    s = [
        cyl("とうのみ", (0, 0, 39), 12, 78, (0.86, 0.83, 0.76), rough=0.85),
        cyl("てんぼうだい", (0, 0, 80), 16, 4, GREY, rough=0.5),
        cyl("へや", (0, 0, 89), 9, 14, (0.92, 0.88, 0.80), rough=0.8),
        cone("やね", (0, 0, 106), 10, 20, ROOF_RED),
        box("とびら穴", (0, -12, 5), (6, 6, 10), (0, 0, 0), op="Subtract"),
        shape("Sphere", "てっぺん", pos=(0, 0, 117.5), size=(2, 1, 1), color=GOLD, metallic=0.9, roughness=0.3,
              blend=0.0),
    ]
    for z in (30, 50, 66):
        s += win_front("まど", 0, z, 3, 8, -12)
    return group("とう", s)


# ---------------------------------------------------------------- こうば
def factory():
    s = [box("こうば", (0, 0, 16), (70, 46, 32), BRICK, rough=0.9)]
    for x in (-22, 0, 22):
        s.append(shape("Extrude", "のこぎりやね", pos=(x, 0, 32), size=(46, 1, 1), rot=(90, 0, 0),
                       outline=[[-11, 0], [11, 0], [11, 14]], color=DARK_ROOF, roughness=0.6, blend=0.0))
        s.append(box("まどガラス", (x + 11.6, 0, 42), (1.2, 40, 8), GLASS, glass=1.0, rough=0.05))
    s.append(cyl("えんとつ", (28, -4, 33), 5, 66, (0.62, 0.30, 0.25), rough=0.8))
    s.append(cyl("えんとつのふち", (28, -4, 66), 6, 4, (0.45, 0.22, 0.20), rough=0.8))
    s.append(shape("Sphere", "けむり1", pos=(31, -4, 77), size=(4.5, 1, 1), color=(0.80, 0.80, 0.82), roughness=1.0,
                   blend=0.0, translucency=0.5))
    s.append(shape("Sphere", "けむり2", pos=(35, -4, 86), size=(3.2, 1, 1), color=(0.80, 0.80, 0.82), roughness=1.0,
                   blend=0.0, translucency=0.5))
    s.append(box("しゃったー", (-12, -22.5, 9), (16, 2, 18), (0.55, 0.56, 0.58), rough=0.5))
    s += win_front("まど", 14, 22, 10, 8, -23)
    return group("こうば", s)


# ---------------------------------------------------------------- ふうしゃごや
def windmill():
    s = [
        shape("Revolve", "ほんたい", pos=(0, 0, 0), size=(1, 1, 1), params=(360, 0, 0, 0), color=CREAM,
              roughness=0.8, blend=0.0, outline=[[0, 0], [18, 0], [14, 40], [11, 62], [0, 62]]),
        cone("やね", (0, 0, 70), 14, 16, ROOF_RED),
        cyl("ハブ", (0, -14, 54), 3, 6, WOOD, rot=(90, 0, 0), rough=0.5),
    ]
    for a in (45, 135, 225, 315):
        x = 16 * math.sin(math.radians(a))
        z = 54 + 16 * math.cos(math.radians(a))
        s.append(box("かざぐるまの はね", (x, -14, z), (9, 2, 26), (0.96, 0.92, 0.80), rot=(0, a, 0), rough=0.7))
    s.append(box("とびら穴", (0, -17.3, 7), (8, 8, 14), (0, 0, 0), op="Subtract"))
    s += win_front("まど", 0, 30, 5, 8, -15)
    return group("ふうしゃごや", s)


# ---------------------------------------------------------------- とりい
def torii():
    s = [
        cyl("はしら左", (-28, 0, 38), 3.5, 76, VERMILION, rough=0.5),
        cyl("はしら右", (28, 0, 38), 3.5, 76, VERMILION, rough=0.5),
        box("いしだん左", (-28, 0, 1.5), (12, 12, 3), STONE, rough=0.9),
        box("いしだん右", (28, 0, 1.5), (12, 12, 3), STONE, rough=0.9),
        box("ぬき", (0, 0, 62), (62, 5, 4.5), VERMILION, rough=0.5, rnd=1.0),
        box("かさぎ", (0, 0, 80), (92, 9, 3.5), DARK_ROOF, rough=0.4, rnd=1.0),
    ]
    return group("とりい", s)


# ---------------------------------------------------------------- ごじゅうのとう
def pagoda():
    s = []
    z = 0.0
    for i in range(5):
        rb = 10.5 - 1.5 * i       # body radius
        hb = 9.0 - 0.6 * i        # body height
        er = rb + 8.5 - 1.0 * i   # eave radius
        zr = z + hb
        s.append(shape("PrismN", "どう%d" % (i + 1), pos=(0, 0, z + hb / 2), size=(rb, hb, 1), params=(8, 0, 0, 0),
                       color=VERMILION, roughness=0.6, blend=0.0))
        # roof: flat underside, eave curling up, steep-ish top
        prof = [[0, 0], [er, 0], [er + 1.8, 2.0], [rb + 2.5, 3.6], [rb - 0.5, 5.2], [0, 6.4]]
        s.append(shape("Revolve", "やね%d" % (i + 1), pos=(0, 0, zr), size=(1, 1, 1), params=(360, 0, 0, 0),
                       outline=prof, color=DARK_ROOF, roughness=0.5, blend=0.0))
        z = zr + 6.4
    s.append(cyl("そうりん", (0, 0, z + 7), 1.0, 14, GOLD, metallic=1.0, rough=0.3))
    s.append(shape("Sphere", "ほうじゅ", pos=(0, 0, z + 15), size=(3, 1, 1), color=GOLD, metallic=1.0, roughness=0.3,
                   blend=0.0))
    return group("ごじゅうのとう", s)


# ---------------------------------------------------------------- きょうかい
def church():
    s = [
        box("ほんどう", (0, 0, 17), (44, 72, 34), CREAM, rough=0.8),
        box("とう", (0, -38, 36), (22, 22, 72), CREAM, rough=0.8),
        box("とびら穴", (0, -49, 8), (10, 6, 16), (0, 0, 0), op="Subtract"),
        box("まど穴", (0, -49, 30), (6, 6, 12), (0, 0, 0), op="Subtract"),
        box("つりがね穴", (0, -49, 62), (12, 6, 12), (0, 0, 0), op="Subtract"),
    ]
    for side, x in ((1, 22), (-1, -22)):
        for y in (-14, 14):
            s += win_side("まど", x, y, 20, 8, 12, side)
    s += [
        gable("やね", 25, 22, 34, 76, ROOF_RED),
        shape("Pyramid", "とうのとんがり", pos=(0, -38, 85), size=(11 / 0.70710678, 26, 1), params=(4, 0, 0, 0),
              rot=(0, 0, 45), color=DARK_ROOF, roughness=0.5, blend=0.0),
        box("じゅうじか1", (0, -38, 104), (1.6, 1.6, 12), GOLD, metallic=0.8, rough=0.3),
        box("じゅうじか2", (0, -38, 106), (7, 1.6, 1.6), GOLD, metallic=0.8, rough=0.3),
    ]
    return group("きょうかい", s)


# ---------------------------------------------------------------- ライトハウス
def lighthouse():
    def rad(z):
        return 13.0 - 5.0 * (z - 4.0) / 76.0

    s = [shape("Revolve", "きそ", pos=(0, 0, 0), size=(1, 1, 1), params=(360, 0, 0, 0), color=GREY,
               roughness=0.9, blend=0.0, outline=[[0, 0], [16, 0], [16, 4], [0, 4]])]
    bands = [(4, 22, WHITE), (22, 34, RED), (34, 46, WHITE), (46, 58, RED), (58, 80, WHITE)]
    for z0, z1, col in bands:
        s.append(shape("Revolve", "とうたい", pos=(0, 0, 0), size=(1, 1, 1), params=(360, 0, 0, 0), color=col,
                       roughness=0.5, blend=0.0,
                       outline=[[0, z0], [rad(z0), z0], [rad(z1), z1], [0, z1]]))
    s += [
        cyl("がいろう", (0, 0, 81), 11, 2, GREY, rough=0.5),
        shape("Torus", "てすり", pos=(0, 0, 84), size=(11, 0.8, 1), color=GREY, metallic=0.4, roughness=0.3,
              blend=0.0),
        cyl("ランタン", (0, 0, 88), 8, 12, GLASS, glass=1.0, rough=0.05),
        shape("Sphere", "ひかり", pos=(0, 0, 88), size=(4.5, 1, 1), color=(1.0, 0.9, 0.5), glow=1.8, roughness=0.4,
              blend=0.0),
        cone("やね", (0, 0, 98), 8.5, 8, RED),
        shape("Sphere", "てっぺん", pos=(0, 0, 103), size=(1.5, 1, 1), color=GOLD, metallic=1.0, roughness=0.3,
              blend=0.0),
        box("とびら穴", (0, -13, 5), (6, 8, 10), (0, 0, 0), op="Subtract"),
    ]
    return group("ライトハウス", s)


# ---------------------------------------------------------------- テント
def tent():
    s = [
        box("じめん", (0, 0, 1), (92, 70, 2), GREEN, rough=0.95),
        gable("テント", 42, 44, 2, 60, ORANGE, rough=0.9),
        box("いりぐち穴", (0, -30, 15), (22, 10, 26), (0, 0, 0), op="Subtract"),
        shape("Curve", "ロープ左", pos=(0, 0, 0), size=(1, 1, 1), path=[[0, -30, 44, 0.9], [-26, -40, 22, 0.9],
                                                                      [-52, -50, 2, 0.9]],
              color=(0.35, 0.33, 0.30), roughness=0.8, blend=0.0),
        shape("Curve", "ロープ右", pos=(0, 0, 0), size=(1, 1, 1), path=[[0, -30, 44, 0.9], [26, -40, 22, 0.9],
                                                                      [52, -50, 2, 0.9]],
              color=(0.35, 0.33, 0.30), roughness=0.8, blend=0.0),
    ]
    return group("テント", s)


# ---------------------------------------------------------------- ピラミッド
def pyramid():
    s = [
        box("だい", (0, 0, 1), (150, 150, 2), SAND, rough=0.95),
        shape("Pyramid", "ピラミッド", pos=(0, 0, 40), size=(60 / 0.70710678, 80, 1), params=(4, 0, 0, 0),
              rot=(0, 0, 45), color=SAND, roughness=0.9, blend=0.0),
        shape("Pyramid", "きんのいただき", pos=(0, 0, 84), size=(6 / 0.70710678, 8, 1), params=(4, 0, 0, 0),
              rot=(0, 0, 45), color=GOLD, metallic=1.0, roughness=0.3, blend=0.0),
        box("いりぐち穴", (0, -55.5, 8), (14, 14, 16), (0, 0, 0), op="Subtract"),
    ]
    return group("ピラミッド", s)


# ---------------------------------------------------------------- items
items = [
    item("いえ", ["house", "home", "家", "building"], house()),
    item("ビル", ["building", "office", "skyscraper", "高層ビル"], building()),
    item("おしろ", ["castle", "城", "keep"], castle()),
    item("とう", ["tower", "塔", "watchtower"], tower()),
    item("こうば", ["factory", "工場", "chimney", "煙突"], factory()),
    item("ふうしゃごや", ["windmill", "mill", "風車小屋", "風車"], windmill()),
    item("とりい", ["torii", "gate", "鳥居"], torii()),
    item("ごじゅうのとう", ["pagoda", "五重塔", "temple"], pagoda()),
    item("きょうかい", ["church", "chapel", "教会"], church()),
    item("ライトハウス", ["lighthouse", "灯台", "beacon"], lighthouse()),
    item("テント", ["tent", "camp", "camping"], tent()),
    item("ピラミッド", ["pyramid", "egypt", "金字塔"], pyramid()),
]

if __name__ == "__main__":
    for it in items:
        assert len(it["group"]["shapes"]) <= 24, it["name"]
    write_category("buildings.json", "たてもの", items)
    preview(items, outdir="/tmp/lib_preview_bld")
