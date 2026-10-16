"""Ready-made town pieces and architecture (category まち). Writes ../town.json.

Run:  YURACAD_BIN=/path/to/yuracad python3 town.py
All models: mm, Z up, stand on z = 0, front toward -Y.
Box size = full extents; Cylinder/Cone size = (radius, height); Loft size[0] = height with
outline = bottom and outline2 = top; Extrude with rot (90,0,0) gives a flat shape in the XZ plane
(gables, arches). Windows: a Subtract hole through the front wall, then a recessed glass pane.
"""
import math

from lib import *

CREAM = (0.95, 0.90, 0.78)
WHITE = (0.95, 0.95, 0.93)
CONCRETE = (0.78, 0.80, 0.82)
STONE = (0.76, 0.73, 0.67)
WOOD = (0.62, 0.44, 0.28)
DARK_WOOD = (0.40, 0.27, 0.17)
TILE = (0.22, 0.24, 0.28)
ROOF_RED = (0.72, 0.28, 0.20)
GLASS = (0.55, 0.78, 0.95)
PALE_GLASS = (0.72, 0.86, 0.95)
GREY = (0.55, 0.56, 0.58)
DARK_GREY = (0.30, 0.31, 0.33)
ASPHALT = (0.30, 0.31, 0.34)
SIDEWALK = (0.80, 0.79, 0.76)
YARD = (0.86, 0.78, 0.60)
RED = (0.82, 0.18, 0.16)
GOLD = (1.00, 0.78, 0.35)
ORANGE = (0.95, 0.45, 0.14)
GREEN = (0.36, 0.60, 0.34)
SIGN_GREEN = (0.20, 0.62, 0.40)
PALE = (0.98, 0.96, 0.90)
INK = (0.12, 0.12, 0.14)


def box(name, pos, size, color, op="Add", blend=0.0, rough=0.7, metallic=0.0, glass=0.0, rot=(0, 0, 0),
        rnd=0.0, glow=0.0, translucency=0.0):
    return shape("Box", name, pos=pos, size=size, color=color, op=op, blend=blend, roughness=rough,
                 metallic=metallic, glass=glass, rot=rot, round=rnd, glow=glow, translucency=translucency)


def cyl(name, pos, r, h, color, rot=(0, 0, 0), rough=0.6, metallic=0.0):
    return shape("Cylinder", name, pos=pos, size=(r, h, 1), color=color, rot=rot, roughness=rough,
                 metallic=metallic, blend=0.0)


def cone(name, pos, r, h, color, rough=0.6):
    return shape("Cone", name, pos=pos, size=(r, h, 1), color=color, roughness=rough, blend=0.0)


def sphere(name, pos, r, color, rough=0.7, glow=0.0, glass=0.0):
    return shape("Sphere", name, pos=pos, size=(r, 1, 1), color=color, roughness=rough, glow=glow,
                 glass=glass, blend=0.0)


def torus(name, pos, big_r, small_r, color, rot=(0, 0, 0), metallic=0.0, rough=0.5):
    return shape("Torus", name, pos=pos, size=(big_r, small_r, 1), rot=rot, color=color,
                 metallic=metallic, roughness=rough, blend=0.0)


def loft(name, z, h, bottom, top, color, rough=0.6):
    """Frustum: rectangle `bottom` (half sizes) at z - h/2 up to rectangle `top` at z + h/2."""
    bx, by = bottom
    tx, ty = top
    return shape("Loft", name, pos=(0, 0, z), size=(h, 1, 1), color=color, roughness=rough, blend=0.0,
                 outline=[[-bx, -by], [bx, -by], [bx, by], [-bx, by]],
                 outline2=[[-tx, -ty], [tx, -ty], [tx, ty], [-tx, ty]])


def gable(name, x_half, height, base_z, depth, color, y=0.0, rough=0.6):
    """Triangular roof: ridge runs front to back (along Y), centred on y."""
    return shape("Extrude", name, pos=(0, y, base_z), size=(depth, 1, 1), rot=(90, 0, 0),
                 outline=[[-x_half, 0], [x_half, 0], [0, height]], color=color, roughness=rough, blend=0.0)


def window(name, x, z, w, h, y_face, pane=GLASS, glass=1.0):
    """Hole through a front wall at y = y_face (front = -Y), with a glass pane set back inside it."""
    return [box(name + "あな", (x, y_face, z), (w, 6.0, h), (0, 0, 0), op="Subtract"),
            box(name + "ガラス", (x, y_face + 1.5, z), (w - 2, 1.0, h - 2), pane, glass=glass, rough=0.05)]


# ---------------------------------------------------------------- ひらや (Japanese single-storey house)
def hiraya():
    s = [
        box("いしのきそ", (0, 0, 2), (74, 54, 4), STONE, rough=0.9),
        box("かべ", (0, 0, 22), (70, 50, 36), CREAM, rough=0.8),                    # z 4..40
        loft("かわらやね", 52, 24, (44, 34), (18, 8), TILE, rough=0.55),              # hip roof, z 40..64
        box("むね", (0, 0, 65.5), (38, 6, 3), TILE, rough=0.5),
        box("のき", (0, 0, 39), (90, 70, 2), DARK_WOOD, rough=0.7),
        box("えんがわ", (0, -30, 1.5), (76, 10, 3), WOOD, rough=0.6),                # engawa deck
        box("まどあな", (0, -25, 15), (22, 6, 22), (0, 0, 0), op="Subtract"),          # sliding door
        box("しょうじ1", (-5.5, -23.5, 15), (10, 1, 20), PALE, rough=0.9, translucency=0.5),
        box("しょうじ2", (5.5, -23.5, 15), (10, 1, 20), PALE, rough=0.9, translucency=0.5),
        cyl("はしら左", (-33, -33, 21.5), 1.2, 37, DARK_WOOD, rough=0.6),
        cyl("はしら右", (33, -33, 21.5), 1.2, 37, DARK_WOOD, rough=0.6),
    ]
    s += window("まど左", -22, 26, 12, 10, -25)
    s += window("まど右", 22, 26, 12, 10, -25)
    return group("ひらや", s)


# ---------------------------------------------------------------- ビルのあつまり (three towers)
def office_cluster():
    towers = [  # name, x, width, depth, height, colour, glass band heights
        ("ビルA", -26, 28, 30, 130, (0.66, 0.70, 0.76), (26, 60, 94, 118)),
        ("ビルB", 4, 26, 30, 92, (0.82, 0.78, 0.72), (30, 62, 84)),
        ("ビルC", 30, 24, 30, 62, (0.62, 0.68, 0.74), (22, 42)),
    ]
    s = []
    for name, x, w, d, h, col, bands in towers:
        s.append(box(name + "たい", (x, 0, h / 2), (w, d, h), col, rough=0.35, metallic=0.1))
        s.append(box(name + "おくじょう", (x, 0, h + 3), (w - 4, d - 4, 6), GREY, rough=0.4))
        for bz in bands:
            s.append(box(name + "まどたい", (x, -d / 2 - 0.3, bz), (w - 4, 1.6, 6), GLASS, glass=1.0,
                         rough=0.05))
    s.append(cyl("アンテナ", (-26, 0, 147), 0.8, 22, GREY, metallic=0.8, rough=0.3))
    return group("ビルのあつまり", s)


# ---------------------------------------------------------------- タワーマンション
def tower_mansion():
    s = [
        box("ていしゃ", (0, 0, 5), (50, 40, 10), CONCRETE, rough=0.7),              # podium z 0..10
        box("ほんたい", (0, 0, 70), (28, 24, 120), (0.90, 0.88, 0.84), rough=0.5),  # z 10..130
        box("やね", (0, 0, 132), (30, 26, 4), GREY, rough=0.5),
        box("げんかんあな", (0, -20, 5), (12, 6, 8), (0, 0, 0), op="Subtract"),
        box("げんかんガラス", (0, -18.5, 5), (10, 1, 6), GLASS, glass=1.0, rough=0.05),
        box("ひさし", (0, -24, 11.5), (16, 8, 1.5), GREY, rough=0.5),
    ]
    for z0 in (18, 36, 54, 72, 90, 108):
        s.append(box("ベランダ", (0, -16, z0), (30, 8, 1.6), (0.72, 0.72, 0.74), rough=0.6))
        s.append(box("てすり", (0, -20.4, z0 + 2.8), (30, 0.8, 4), GLASS, glass=1.0, rough=0.05))
    return group("タワーマンション", s)


# ---------------------------------------------------------------- コンビニ
def konbini():
    s = [
        box("じめん", (0, 0, 0.5), (110, 74, 1), ASPHALT, rough=0.9),
        box("みせ", (0, 0, 17), (60, 40, 34), WHITE, rough=0.6),                    # z 0..34
        box("やね", (0, 0, 35.5), (64, 44, 3), GREY, rough=0.5),
        box("かんばん", (0, -22.5, 40), (46, 2, 6), SIGN_GREEN, glow=0.5, rough=0.4),
        box("ショーウインドウあな", (0, -20, 12), (48, 6, 20), (0, 0, 0), op="Subtract"),
        box("ガラス", (0, -18.5, 12), (46, 1, 18), GLASS, glass=1.0, rough=0.05),
        box("たてぼう左", (-8, -19.5, 12), (1.5, 1.5, 20), WHITE, rough=0.5),
        box("たてぼう右", (8, -19.5, 12), (1.5, 1.5, 20), WHITE, rough=0.5),
        box("ひさし", (0, -25, 26), (56, 10, 1.5), SIGN_GREEN, rot=(15, 0, 0), rough=0.6),
        cyl("ポール", (32, -30, 20), 0.8, 40, GREY, metallic=0.6, rough=0.3),
        sphere("らいと", (32, -30, 41), 3, (1.0, 0.95, 0.7), glow=1.5),
    ]
    return group("コンビニ", s)


# ---------------------------------------------------------------- がっこう
def school():
    s = [
        box("こうてい", (0, 0, 0.5), (110, 60, 1), YARD, rough=0.9),
        box("ほんかん", (0, 0, 18), (100, 26, 36), (0.93, 0.90, 0.84), rough=0.8),   # z 0..36, front y -13
    ]
    for i, z in enumerate((11, 26)):
        for x in (-30, 30):
            s += window("まど%d_%d" % (i, 0 if x < 0 else 1), x, z, 32, 7, -13)
    s += [
        box("とう", (0, -7, 37), (26, 26, 74), (0.93, 0.90, 0.84), rough=0.8),      # clock tower, front y -20
        shape("Pyramid", "やね", pos=(0, -7, 85), size=(13 / 0.70710678, 22, 1), params=(4, 0, 0, 0),
              rot=(0, 0, 45), color=TILE, roughness=0.5, blend=0.0),
        cyl("とけいばん", (0, -20.6, 62), 7, 1.2, WHITE, rot=(90, 0, 0)),
        torus("とけいのわ", (0, -21.2, 62), 7.6, 0.7, GOLD, rot=(90, 0, 0), metallic=0.8, rough=0.3),
        box("はり長", (0, -21.5, 64.5), (1.0, 0.6, 5), INK),
        box("はり短", (1.8, -21.5, 62), (4.0, 0.6, 1.0), INK),
        box("げんかんあな", (0, -20, 10), (12, 8, 18), (0, 0, 0), op="Subtract"),
        box("げんかんガラス", (0, -19, 10), (10, 1, 16), GLASS, glass=1.0, rough=0.05),
        box("ひさし", (0, -24, 22), (18, 8, 1.5), GREY, rough=0.5),
        cyl("ポール", (0, -7, 104), 0.6, 16, GREY, metallic=0.6, rough=0.3),
        box("はた", (4.5, -7, 108), (8, 0.6, 5), RED),
    ]
    return group("がっこう", s)


# ---------------------------------------------------------------- びょういん
def hospital():
    s = [
        box("ほんかん", (0, 0, 50), (50, 36, 100), WHITE, rough=0.6),               # tower z 0..100
        box("よくかん", (0, -2, 15), (96, 34, 30), WHITE, rough=0.6),                # wing z 0..30
        cyl("ヘリポート", (36, 0, 30.4), 9, 0.8, (0.30, 0.45, 0.65), rough=0.5),
        torus("ヘリポートのわ", (36, 0, 31), 8.5, 0.6, WHITE),
        box("じゅうじ縦", (0, -18.6, 72), (8, 2, 28), RED),                         # red cross
        box("じゅうじ横", (0, -18.6, 72), (28, 2, 8), RED),
    ]
    for z in (38, 50, 90):
        s += window("まど%d" % z, 0, z, 36, 5, -18)
    s += [
        box("げんかんあな", (0, -19, 8), (14, 8, 16), (0, 0, 0), op="Subtract"),
        box("げんかんガラス", (0, -18, 8), (12, 1, 14), GLASS, glass=1.0, rough=0.05),
        box("ひさし", (0, -23, 18), (24, 8, 2), GREY, rough=0.5),
    ]
    return group("びょういん", s)


# ---------------------------------------------------------------- えき
def station():
    s = [
        box("えきしゃ", (0, 0, 11), (110, 28, 22), (0.86, 0.82, 0.76), rough=0.8),   # z 0..22, front y -14
        box("やね", (0, 0, 23), (116, 32, 2), GREY, rough=0.5),
    ]
    s += window("まどひだり", -34, 10, 16, 9, -14)
    s += window("まどみぎ", 34, 10, 16, 9, -14)
    s += [
        box("ちょうとう", (0, -2, 20), (26, 30, 40), (0.86, 0.82, 0.76), rough=0.8),   # front y -17
        loft("ちょうとうやね", 44, 8, (15, 16), (7, 7), TILE, rough=0.5),
        cone("とげ", (0, -2, 54), 3, 12, GREY),
        cyl("とけい", (0, -17.6, 30), 6, 1.2, WHITE, rot=(90, 0, 0)),
        torus("とけいのわ", (0, -18.2, 30), 6.6, 0.6, GOLD, rot=(90, 0, 0), metallic=0.8, rough=0.3),
        box("ホーム", (0, -30, 2), (124, 26, 4), CONCRETE, rough=0.8),
        box("ひさし", (0, -29, 24), (120, 28, 1.5), GLASS, glass=0.8, rough=0.1),
        box("せんろ", (0, -62, 0.5), (130, 14, 1), ASPHALT, rough=0.9),
        box("でんしゃ", (0, -62, 8), (120, 11, 13), (0.25, 0.60, 0.45), rough=0.4, rnd=1.5),
        box("でんしゃのまど", (0, -67.6, 9), (116, 1, 4), GLASS, glass=1.0, rough=0.05),
    ]
    for x in (-50, 50):
        s.append(cyl("はしら", (x, -18, 12), 1.4, 24, WHITE))
    for x in (-50, 0, 50):
        s.append(cyl("はしら前", (x, -41, 12), 1.4, 24, WHITE))
    return group("えき", s)


# ---------------------------------------------------------------- はし (arch bridge)
def arch_bridge():
    big_r, small_r, base = 62.0, 52.0, 12.0
    outer = [[big_r * math.cos(math.radians(a)), big_r * math.sin(math.radians(a))] for a in range(0, 181, 15)]
    inner = [[small_r * math.cos(math.radians(a)), small_r * math.sin(math.radians(a))]
             for a in range(180, -1, -15)]
    s = [
        shape("Extrude", "アーチ", pos=(0, 0, base), size=(18, 1, 1), rot=(90, 0, 0), outline=outer + inner,
              color=CONCRETE, roughness=0.6, blend=0.0),
        box("はしげた", (0, 0, 14), (140, 18, 4), CONCRETE, rough=0.6),
        box("はしら左", (-62, 0, 6), (12, 16, 12), STONE, rough=0.9),
        box("はしら右", (62, 0, 6), (12, 16, 12), STONE, rough=0.9),
    ]
    for x in (-44, -24, 24, 44):
        zc = base + math.sqrt(57.0 ** 2 - x * x)
        s.append(cyl("つり", (x, 0, (16 + zc) / 2), 0.8, zc - 16, GREY, metallic=0.5, rough=0.4))
    s.append(box("てすり前", (0, -8.6, 18), (136, 1.2, 4), GREY, rough=0.4))
    s.append(box("てすり後", (0, 8.6, 18), (136, 1.2, 4), GREY, rough=0.4))
    return group("はし", s)


# ---------------------------------------------------------------- つりばし (suspension bridge)
def suspension_bridge():
    def cable_z(x):
        ax = abs(x)
        if ax <= 50:  # main span: parabola from tower tops (z 92) down to the middle (z 26)
            return 26.0 + 66.0 / 2500.0 * x * x
        u = (100.0 - ax) / 50.0  # side span: 0 at the anchor, 1 at the tower top
        return 14.0 + 78.0 * u * u

    xs = [-100, -87.5, -75, -62.5, -50, -37.5, -25, -12.5, 0, 12.5, 25, 37.5, 50, 62.5, 75, 87.5, 100]
    path = [[float(x), 0.0, cable_z(x), 1.0] for x in xs]
    s = [
        box("はし", (0, 0, 14), (200, 16, 4), CONCRETE, rough=0.6),
        box("とう左", (-50, 0, 46), (8, 12, 92), CONCRETE, rough=0.6),
        box("とう右", (50, 0, 46), (8, 12, 92), CONCRETE, rough=0.6),
        box("アンカー左", (-106, 0, 6), (12, 20, 12), STONE, rough=0.9),
        box("アンカー右", (106, 0, 6), (12, 20, 12), STONE, rough=0.9),
    ]
    for y in (-7.0, 7.0):
        s.append(shape("Curve", "ケーブル", pos=(0, 0, 0), size=(1, 1, 1), path=[[p[0], y, p[2], p[3]] for p in path],
                       color=GREY, metallic=0.6, roughness=0.3, blend=0.0))
    for x in (-40, -30, -20, -10, 0, 10, 20, 30, 40):
        zc = cable_z(x)
        s.append(box("つりひも", (x, 0, (16 + zc) / 2), (0.8, 14, zc - 16), GREY, metallic=0.4, rough=0.4))
    return group("つりばし", s)


# ---------------------------------------------------------------- ドームきゅうじょう
def dome_stadium():
    dome_r = 56.0
    dome_profile = [[0.0, 0.0]] + [[dome_r * math.cos(math.radians(a)), dome_r * math.sin(math.radians(a))]
                                   for a in range(0, 91, 10)]
    s = [
        shape("Revolve", "きそ", pos=(0, 0, 0), size=(1, 1, 1), color=CONCRETE, roughness=0.8, blend=0.0,
              outline=[[0, 0], [64, 0], [64, 6], [60, 8], [60, 14], [0, 14]]),
        shape("Revolve", "ドーム", pos=(0, 0, 14), size=(1, 1, 1), color=(0.90, 0.92, 0.95), roughness=0.3,
              metallic=0.3, blend=0.0, outline=dome_profile),
        torus("ふち", (0, 0, 14), 60, 2.5, WHITE, rough=0.5),
        box("げーと", (0, -60, 6), (18, 14, 12), (0, 0, 0), op="Subtract"),
        cyl("ポール", (0, 0, 80), 0.6, 20, GREY, metallic=0.6, rough=0.3),
        box("はた", (4, 0, 86), (8, 0.5, 5), RED),
    ]
    for x, y in ((-50, -50), (50, -50)):
        s.append(cyl("ライトポール", (x, y, 22), 1.2, 44, GREY, metallic=0.5, rough=0.4))
        s.append(box("ライト", (x, y, 46), (8, 3, 3), (1.0, 0.95, 0.80), glow=1.5))
    return group("ドームきゅうじょう", s)


# ---------------------------------------------------------------- ガソリンスタンド
def gas_station():
    s = [
        box("じめん", (0, 0, 0.5), (130, 100, 1), ASPHALT, rough=0.9),
        box("ひさし", (0, -4, 60), (68, 40, 3), WHITE, rough=0.4),
        box("あかり", (0, -4, 58.2), (60, 36, 0.6), (1.0, 0.97, 0.85), glow=1.0),
        box("とう", (0, -2, 3), (44, 6, 4), CONCRETE, rough=0.8),                      # pump island
        box("ポンプ左", (-10, -2, 11), (4, 4, 14), RED, rough=0.5),
        box("ポンプ右", (10, -2, 11), (4, 4, 14), RED, rough=0.5),
    ]
    for x in (-28, 28):
        for y in (-14, 10):
            s.append(cyl("はしら", (x, y, 29), 2, 58, WHITE, rough=0.5))
    s += [
        box("こや", (-50, 5, 8), (24, 18, 16), WHITE, rough=0.6),
        box("こややね", (-50, 5, 17.5), (26, 20, 2), ROOF_RED, rough=0.5),
    ]
    s += window("こやまど", -50, 9, 12, 8, -4)
    s += [
        cyl("かんばんポール", (44, 28, 23), 1.0, 46, GREY, metallic=0.5, rough=0.4),
        box("かんばん", (44, 28, 53), (16, 2, 14), (0.98, 0.82, 0.25), glow=0.4, rough=0.4),
    ]
    return group("ガソリンスタンド", s)


# ---------------------------------------------------------------- しろ (Japanese castle)
def japanese_castle():
    s = [
        loft("いしがき", 11, 22, (60, 60), (52, 52), STONE, rough=0.9),                  # stone base
        box("もんあな", (0, -60, 7), (16, 16, 14), (0, 0, 0), op="Subtract"),
        box("おしろ1かい", (0, 0, 32), (84, 84, 20), WHITE, rough=0.5),                 # z 22..42
        box("まど1あな", (0, -42, 33), (12, 6, 8), (0, 0, 0), op="Subtract"),
        box("まど1", (0, -40.5, 33), (10, 1, 6), INK, rough=0.3),
        loft("やね1", 48, 12, (54, 54), (42, 42), TILE, rough=0.5),                     # z 42..54
        box("おしろ2かい", (0, 0, 63), (56, 56, 18), WHITE, rough=0.5),                 # z 54..72
        box("まど2あな", (0, -28, 63), (10, 6, 8), (0, 0, 0), op="Subtract"),
        box("まど2", (0, -26.5, 63), (8, 1, 6), INK, rough=0.3),
        loft("やね2", 77, 10, (34, 34), (24, 24), TILE, rough=0.5),                     # z 72..82
        box("おしろ3かい", (0, 0, 89), (32, 32, 14), WHITE, rough=0.5),                 # z 82..96
        box("まど3あな", (0, -16, 89), (8, 6, 6), (0, 0, 0), op="Subtract"),
        box("まど3", (0, -14.5, 89), (6, 1, 4), INK, rough=0.3),
        loft("やね3", 101, 10, (22, 22), (14, 14), TILE, rough=0.5),                    # z 96..106
        cyl("ひとつ", (0, 0, 111), 1.6, 10, GOLD, metallic=0.9, rough=0.3),
        sphere("てっぺん", (0, 0, 117), 3, GOLD, rough=0.3),
    ]
    return group("しろ", s)


# ---------------------------------------------------------------- タワー (lattice tower)
def lattice_tower():
    top = 118.0

    def hw(z):  # half width of the tapered tower at height z
        return 34.0 - 29.0 * z / top

    s = []
    for z, col in ((14, ORANGE), (34, WHITE), (56, ORANGE), (78, WHITE), (98, ORANGE)):
        w = hw(z)
        s.append(box("わく", (0, 0, z), (2 * w, 2 * w, 1.4), col, rough=0.5))
        s.append(box("あな", (0, 0, z), (2 * w - 5, 2 * w - 5, 3.0), (0, 0, 0), op="Subtract"))
    s.append(cyl("てんぼうだい", (0, 0, 58), 12, 4, WHITE, rough=0.5))
    s.append(cyl("てんぼうだい2", (0, 0, 100), 8, 3, WHITE, rough=0.5))
    for sx in (-1, 1):
        for sy in (-1, 1):
            path = [[sx * hw(z), sy * hw(z), z, r] for z, r in ((0, 2.2), (56, 1.7), (top, 1.2))]
            s.append(shape("Curve", "あし", pos=(0, 0, 0), size=(1, 1, 1), path=path, color=ORANGE,
                           roughness=0.5, blend=0.0))
    s.append(cone("てっぺん", (0, 0, top + 8), 5, 16, ORANGE))
    s.append(cyl("アンテナ", (0, 0, 141), 0.6, 14, GREY, metallic=0.6, rough=0.3))
    return group("タワー", s)


# ---------------------------------------------------------------- ふうりんのあるおみせ
def wind_chime_shop():
    s = [
        box("いえ", (0, 0, 17), (56, 38, 34), WOOD, rough=0.8),                          # z 0..34
        loft("やね", 42, 16, (35, 26), (17, 10), TILE, rough=0.55),                     # z 34..50
        box("いりぐちあな", (0, -19, 13), (38, 6, 22), (0, 0, 0), op="Subtract"),
        box("ガラス左", (-9.5, -17.5, 13), (17, 1, 20), GLASS, glass=1.0, rough=0.05),
        box("ガラス右", (9.5, -17.5, 13), (17, 1, 20), GLASS, glass=1.0, rough=0.05),
        box("たてぐみ", (0, -18, 13), (2, 2, 22), DARK_WOOD, rough=0.6),
        box("のれん左", (-9.5, -21, 26), (16, 1, 5), (0.18, 0.25, 0.45), rough=0.9),
        box("のれん右", (9.5, -21, 26), (16, 1, 5), (0.18, 0.25, 0.45), rough=0.9),
        cyl("ひも", (16, -24, 27), 0.3, 14, GREY, rough=0.4),
        sphere("ふうりん", (16, -24, 17), 3.6, PALE_GLASS, rough=0.05, glass=1.0),
        box("たんざく", (16, -24, 8.5), (3, 0.4, 9), (0.90, 0.30, 0.25), rough=0.8),
        cyl("はち", (-30, -26, 4), 4, 8, (0.72, 0.38, 0.22), rough=0.8),
        sphere("しげみ", (-30, -26, 13), 6, GREEN, rough=0.8),
    ]
    return group("ふうりんのあるおみせ", s)


# ---------------------------------------------------------------- モダンハウス
def modern_house():
    s = [
        box("ちかい", (-12, 0, 14), (72, 52, 28), (0.92, 0.92, 0.90), rough=0.6),       # ground floor
        box("うわのかい", (10, 0, 39), (90, 50, 22), (0.70, 0.56, 0.40), rough=0.7),    # cantilever, z 28..50
        box("やね", (10, 0, 51), (96, 54, 2), DARK_GREY, rough=0.5),
        box("ペントハウス", (-20, 0, 57), (30, 30, 10), WHITE, rough=0.6),
        cyl("はしら", (46, -18, 14), 1.6, 28, GREY, rough=0.5),
        box("まどあな1", (4, -26, 14), (40, 6, 14), (0, 0, 0), op="Subtract"),
        box("ガラス1", (4, -25, 14), (38, 1, 12), GLASS, glass=1.0, rough=0.05),
        box("まどあな2", (10, -25, 39), (80, 6, 12), (0, 0, 0), op="Subtract"),
        box("ガラス2", (10, -24.5, 39), (78, 1, 10), GLASS, glass=1.0, rough=0.05),
        box("どああな", (-38, -26, 11), (12, 6, 22), (0, 0, 0), op="Subtract"),
        box("とびら", (-38, -25, 11), (10, 1.2, 20), DARK_WOOD, rough=0.6),
        box("プール", (10, -46, 1), (62, 14, 2), (0.30, 0.68, 0.88), glass=0.6, rough=0.05),
        cyl("き", (-50, -30, 6), 1.5, 12, WOOD, rough=0.8),
        sphere("こずえ", (-50, -30, 18), 7, GREEN, rough=0.8),
    ]
    return group("モダンハウス", s)


# ---------------------------------------------------------------- たてものブロックのまち
def block_city():
    s = [
        box("じどうどう", (0, 0, 1), (130, 130, 2), SIDEWALK, rough=0.9),               # z 0..2
        box("どうろ", (0, 0, 2.4), (130, 22, 0.8), ASPHALT, rough=0.8),                 # road z 2..3
        box("ラインぶん", (0, 0, 2.9), (120, 1.0, 0.3), WHITE, rough=0.6),
    ]
    # front row (y -38): office, tower, shop with gable roof
    s += [box("オフィス", (-40, -38, 22), (30, 24, 40), (0.80, 0.84, 0.88), rough=0.35)]
    s += window("オフィス", -40, 22, 20, 24, -50)
    s += [box("タワーのたて", (0, -38, 35), (22, 22, 66), (0.66, 0.70, 0.76), rough=0.35)]
    s += window("タワー", 0, 40, 12, 48, -49)
    s += [box("みせ", (40, -38, 11), (26, 24, 18), (0.92, 0.86, 0.76), rough=0.7)]
    s += window("みせ", 40, 8, 18, 10, -50)
    s += [gable("みせのやね", 14, 12, 20, 24, ROOF_RED, y=-38)]
    # back row (y +38): low house and a tall block
    s += [box("まんなか", (-20, 38, 17), (28, 22, 30), (0.84, 0.74, 0.62), rough=0.7)]
    s += window("まんなか", -20, 18, 18, 14, 27)
    s += [box("のっぽ", (30, 38, 27), (24, 22, 50), (0.74, 0.78, 0.84), rough=0.35)]
    s += window("のっぽ", 30, 26, 16, 36, 27)
    # trees
    s += [cyl("き1", (19, -40, 6), 1.2, 8, WOOD, rough=0.8), sphere("こずえ1", (19, -40, 14), 6, GREEN, rough=0.8),
          cyl("き2", (-54, 38, 6), 1.2, 8, WOOD, rough=0.8), sphere("こずえ2", (-54, 38, 14), 6, GREEN, rough=0.8)]
    return group("たてものブロックのまち", s)


# ---------------------------------------------------------------- items
items = [
    item("ひらや", ["japanese house", "tiled roof", "engawa", "平屋", "いえ"], hiraya()),
    item("ビルのあつまり", ["skyscrapers", "office", "city", "ビル", "高層ビル"], office_cluster()),
    item("タワーマンション", ["tower mansion", "apartment", "condo", "balcony", "マンション"], tower_mansion()),
    item("コンビニ", ["convenience store", "shop", "store", "店", "コンビニエンスストア"], konbini()),
    item("がっこう", ["school", "clock tower", "学校"], school()),
    item("びょういん", ["hospital", "clinic", "red cross", "病院"], hospital()),
    item("えき", ["station", "train station", "platform", "駅"], station()),
    item("はし", ["bridge", "arch bridge", "橋"], arch_bridge()),
    item("つりばし", ["suspension bridge", "cable", "吊り橋"], suspension_bridge()),
    item("ドームきゅうじょう", ["stadium", "dome", "arena", "ドーム球場"], dome_stadium()),
    item("ガソリンスタンド", ["gas station", "petrol station", "fuel", "給油所"], gas_station()),
    item("しろ", ["castle", "japanese castle", "城"], japanese_castle()),
    item("タワー", ["tower", "lattice tower", "observation tower", "鉄塔", "展望台"], lattice_tower()),
    item("ふうりんのあるおみせ", ["wind chime shop", "wind bell", "shop", "風鈴", "お店"], wind_chime_shop()),
    item("モダンハウス", ["modern house", "flat roof", "villa", "モダン"], modern_house()),
    item("たてものブロックのまち", ["city block", "town", "street", "まち", "街"], block_city()),
]

if __name__ == "__main__":
    for it in items:
        assert len(it["group"]["shapes"]) <= 24, (it["name"], len(it["group"]["shapes"]))
    write_category("town.json", "まち", items)
    preview(items, outdir="/tmp/lib_preview_town")
