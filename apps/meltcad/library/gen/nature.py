"""Nature models (category しぜん) -> ../nature.json.

Run:  export MELTCAD_BIN=/path/to/meltcad; python3 nature.py
Preview PNGs:  preview(items, outdir="/tmp/lib_preview_nature")
"""
import math

from lib import *

# colours (0-1)
BARK = (0.42, 0.28, 0.17)
BARK_DARK = (0.32, 0.20, 0.12)
LEAF = (0.27, 0.62, 0.25)
LEAF_B = (0.34, 0.68, 0.28)
PINE = (0.11, 0.38, 0.21)
PINE_B = (0.15, 0.43, 0.23)
GREEN = (0.25, 0.58, 0.24)
STEM = (0.28, 0.58, 0.22)
PINK = (0.97, 0.45, 0.66)
YELLOW = (1.0, 0.80, 0.18)
WHITE = (0.97, 0.97, 0.95)
GREY = (0.50, 0.49, 0.47)
GREY_D = (0.40, 0.40, 0.42)
SNOW = (0.96, 0.97, 1.0)
CREAM = (0.97, 0.93, 0.84)
RED = (0.88, 0.14, 0.14)


def sph(name, pos, r, color, scale=(1, 1, 1), rot=(0, 0, 0), blend=6.0, op="Add", **kw):
    return shape("Sphere", name, pos=pos, size=(r, 1, 1), scale=scale, rot=rot, color=color,
                 blend=blend, op=op, **kw)


def cyl(name, pos, r, h, color, rot=(0, 0, 0), blend=6.0, **kw):
    return shape("Cylinder", name, pos=pos, size=(r, h, 1), rot=rot, color=color, blend=blend, **kw)


def cone(name, pos, r, h, color, blend=6.0, **kw):
    return shape("Cone", name, pos=pos, size=(r, h, 1), color=color, blend=blend, **kw)


def cap(name, pos, r, length, color, rot=(0, 0, 0), blend=6.0, **kw):
    """Capsule along its own Z; size = radius, straight length."""
    return shape("Capsule", name, pos=pos, size=(r, length, 1), rot=rot, color=color, blend=blend, **kw)


def curve(name, nodes, color, blend=3.0, smooth=0.6, flat=0, **kw):
    """Tube through (x, y, z, radius) nodes. params = closed, smooth, flat ends, thickness."""
    nodes = [[round(v, 2) for v in n] for n in nodes]
    return shape("Curve", name, pos=(0, 0, 0), path=nodes, params=(0, smooth, flat, 1),
                 color=color, blend=blend, **kw)


def revolve(name, prof, color, pos=(0, 0, 0), rot=(0, 0, 0), angle=360.0, blend=6.0, **kw):
    """Profile x = radius, y = height; turned about Z through `angle` degrees (params[0])."""
    return shape("Revolve", name, pos=pos, rot=rot, outline=prof, params=(angle, 0, 0, 0),
                 color=color, blend=blend, **kw)


def rad(deg):
    return math.radians(deg)


items = []

# ---------------------------------------------------------------- き (round tree)
items.append(item("き", ["tree", "round tree", "木", "nature"], group("き", [
    cyl("みき", (0, 0, 16), 4.5, 32, BARK, blend=2, roughness=0.8),
    sph("おおきなは", (0, 0, 52), 22, LEAF, blend=8, roughness=0.7),
    sph("はっぱ左", (-14, -4, 44), 15, LEAF_B, blend=8, roughness=0.7),
    sph("はっぱ右", (15, -3, 44), 16, LEAF, blend=8, roughness=0.7),
    sph("はっぱ上", (-6, -8, 64), 14, LEAF_B, blend=8, roughness=0.7),
    sph("はっぱ前", (9, -12, 34), 11, LEAF_B, blend=8, roughness=0.7),
])))

# ---------------------------------------------------------------- まつ (pine, stacked cones)
items.append(item("まつ", ["pine", "conifer", "松", "nature"], group("まつ", [
    cyl("みき", (0, 0, 11), 3.5, 22, BARK, blend=2, roughness=0.8),
    cone("した", (0, 0, 24), 26, 28, PINE, blend=4, roughness=0.75),
    cone("なか", (0, 0, 42), 20, 24, PINE_B, blend=4, roughness=0.75),
    cone("うえ", (0, 0, 60), 13, 20, PINE, blend=4, roughness=0.75),
])))

# ---------------------------------------------------------------- やし (palm, curved trunk)
PALM_TRUNK = [[-12, 0, 3.2, 3.2], [-10, 0, 20, 2.8], [-7, 0, 38, 2.5], [-3, 0, 52, 2.3], [0, 0, 60, 2.2]]
palm = [curve("みき", PALM_TRUNK, (0.55, 0.40, 0.25), blend=2, smooth=0.6, roughness=0.8)]
for k in range(8):
    a = rad(45 * k + 5)
    rs = [0, 10, 22, 34]
    zs = [0, 6, 1, -11]
    rr = [2.8, 2.4, 1.8, 1.0]
    nodes = [[r * math.cos(a), r * math.sin(a), 60 + z, q] for r, z, q in zip(rs, zs, rr)]
    palm.append(curve(f"はっぱ{k + 1}", nodes, (0.22, 0.60, 0.22), blend=3, smooth=0.6, roughness=0.6))
palm.append(sph("ココナッツ1", (-3, -3, 56), 3.6, (0.36, 0.22, 0.12), blend=2))
palm.append(sph("ココナッツ2", (3, -1, 56), 3.6, (0.36, 0.22, 0.12), blend=2))
palm.append(sph("ココナッツ3", (0, 2, 54), 3.4, (0.36, 0.22, 0.12), blend=2))
items.append(item("やし", ["palm", "coconut", "椰子", "ヤシ", "nature"], group("やし", palm)))

# ---------------------------------------------------------------- はな (flower)
flower = [
    curve("くき", [[0, 0, 1.6, 1.6], [1, 0, 20, 1.5], [-1, 0, 38, 1.5], [0, 0, 55, 1.5]],
          STEM, blend=3, smooth=0.6, roughness=0.6),
    sph("はっぱ右", (10, 0, 30), 7, LEAF, scale=(1.8, 0.45, 0.9), rot=(0, -30, 0), blend=3),
    sph("はっぱ左", (-10, 0, 36), 7, LEAF_B, scale=(1.8, 0.45, 0.9), rot=(0, -150, 0), blend=3),
]
HEAD = (0, -2, 56)
for k in range(5):
    a = 90 + 72 * k
    ar = rad(a)
    flower.append(sph(f"はなびら{k + 1}",
                      (HEAD[0] + 8 * math.cos(ar), HEAD[1], HEAD[2] + 8 * math.sin(ar)),
                      5, PINK, scale=(1.6, 0.45, 0.9), rot=(0, -a, 0), blend=2, coat=0.2, roughness=0.45))
flower.append(sph("まんなか", (HEAD[0], HEAD[1] - 1, HEAD[2]), 5.5, YELLOW, blend=2, roughness=0.5))
items.append(item("はな", ["flower", "blossom", "花", "nature"], group("はな", flower)))

# ---------------------------------------------------------------- ちゅうりっぷ (tulip)
TULIP_CUP = [[0, 0], [4, 0], [7, 8], [9, 15], [8.5, 21], [5, 17.5], [2.5, 21], [0, 22]]
tulip = [
    curve("くき", [[0, 0, 1.6, 1.4], [0, 0, 20, 1.4], [1, 0, 38, 1.4]], STEM, blend=3, smooth=0.5),
    sph("はっぱ右", (7, 0, 18), 8, LEAF, scale=(1.8, 0.45, 0.9), rot=(0, -30, 0), blend=3),
    sph("はっぱ左", (-7, 0, 22), 8, LEAF_B, scale=(1.8, 0.45, 0.9), rot=(0, -150, 0), blend=3),
    revolve("カップ", TULIP_CUP, RED, pos=(0, 0, 37), blend=3, roughness=0.35, coat=0.4),
]
items.append(item("ちゅうりっぷ", ["tulip", "flower", "チューリップ", "花", "nature"], group("ちゅうりっぷ", tulip)))

# ---------------------------------------------------------------- ひまわり (sunflower)
sunflower = [
    cyl("くき", (0, 0, 28), 2.2, 56, STEM, blend=3, roughness=0.6),
    sph("はっぱ右", (10, 0, 30), 8, GREEN, scale=(1.8, 0.4, 0.9), rot=(0, -30, 0), blend=3),
    sph("はっぱ左", (-10, 0, 24), 8, GREEN, scale=(1.8, 0.4, 0.9), rot=(0, -150, 0), blend=3),
    cyl("まんなか", (0, -3, 58), 11, 4, (0.30, 0.15, 0.05), rot=(90, 0, 0), blend=2, roughness=0.8),
]
HEAD_S = (0, -3, 58)
for k in range(16):
    a = 360.0 / 16 * k
    ar = rad(a)
    sunflower.append(sph(f"はなびら{k + 1}",
                         (HEAD_S[0] + 13 * math.cos(ar), HEAD_S[1], HEAD_S[2] + 13 * math.sin(ar)),
                         5, YELLOW, scale=(1.9, 0.35, 0.9), rot=(0, -a, 0), blend=1.0,
                         roughness=0.45, coat=0.1))
items.append(item("ひまわり", ["sunflower", "ひまわり", "向日葵", "flower", "nature"], group("ひまわり", sunflower)))

# ---------------------------------------------------------------- きのこ (mushroom)
def mushroom(prefix, ox, oy, s):
    """Cap (flat underside), stem and white spots. s scales everything."""
    cz = 30 * s
    cap_r, cap_h = 22 * s, 13.2 * s
    parts = [
        sph(f"{prefix}かさ", (ox, oy, cz), cap_r, RED, scale=(1, 1, 0.6), blend=6, roughness=0.4, coat=0.3),
        shape("Box", f"{prefix}したをけずる", pos=(ox, oy, 16 * s), size=(60 * s, 60 * s, 20 * s),
              op="Subtract", blend=0),
        shape("Revolve", f"{prefix}くき", pos=(ox, oy, 0),
              outline=[[0, 0], [8 * s, 0], [6 * s, 4 * s], [4.5 * s, 8 * s], [4.5 * s, 26 * s], [0, 26 * s]],
              params=(360, 0, 0, 0), color=CREAM, blend=3, roughness=0.7),
    ]
    for phi, th in [(35, 0), (35, 120), (35, 240), (62, 60), (62, 200)]:
        p, t = rad(phi), rad(th)
        x = cap_r * math.sin(p) * math.cos(t)
        y = cap_r * math.sin(p) * math.sin(t)
        z = cz + cap_h * math.cos(p)
        parts.append(sph(f"{prefix}ほし{phi}_{th}", (ox + x, oy + y, z), 4.2 * s, WHITE, blend=0.5,
                         roughness=0.6))
    return parts


items.append(item("きのこ", ["mushroom", "きのこ", "キノコ", "toadstool", "nature"],
                  group("きのこ", mushroom("", 0, 0, 1.0) + mushroom("ちいさい", 22, 8, 0.55))))

# ---------------------------------------------------------------- いわ (rock)
rock = [
    sph("おおきいいわ", (0, 0, 22.4), 28, GREY, scale=(1.25, 1.0, 0.8), blend=6, roughness=0.92),
    sph("いわ2", (-27, -6, 12.8), 16, GREY_D, scale=(1, 1, 0.8), blend=6, roughness=0.92),
    sph("いわ3", (25, -4, 10.4), 13, GREY, scale=(1, 1, 0.8), blend=6, roughness=0.92),
    sph("こいし", (-6, -27, 7), 7, GREY_D, blend=4, roughness=0.92),
    sph("こけ", (-2, -2, 40), 20, (0.30, 0.55, 0.25), scale=(1.3, 1.1, 0.32), blend=4, roughness=0.95),
]
items.append(item("いわ", ["rock", "stone", "岩", "nature"], group("いわ", rock)))

# ---------------------------------------------------------------- やま (mountain with snow cap)
mountain = [
    cone("やま", (0, 0, 31), 42, 62, (0.42, 0.46, 0.38), blend=6, roughness=0.9),
    cone("ちいさいやま", (-34, -4, 15), 22, 30, (0.36, 0.50, 0.38), blend=5, roughness=0.9),
    cone("ゆきのふもと", (0, 0, 51), 16, 22, SNOW, blend=3, roughness=0.5),
]
items.append(item("やま", ["mountain", "snow", "山", "nature"], group("やま", mountain)))

# ---------------------------------------------------------------- くも (cloud)
cloud = [
    sph("まんなか", (0, 0, 30), 20, WHITE, blend=6, roughness=0.85, translucency=0.15, glow=0.2),
    sph("ひだりの", (-23, -2, 14), 14, WHITE, blend=6, roughness=0.85, translucency=0.15, glow=0.2),
    sph("みぎの", (23, 0, 15), 15, WHITE, blend=6, roughness=0.85, translucency=0.15, glow=0.2),
    sph("うえの左", (-10, -6, 42), 13, WHITE, blend=6, roughness=0.85, translucency=0.15, glow=0.2),
    sph("うえの右", (12, -5, 40), 14, WHITE, blend=6, roughness=0.85, translucency=0.15, glow=0.2),
    sph("まえ", (0, -12, 19), 12, WHITE, blend=6, roughness=0.85, translucency=0.15, glow=0.2),
]
items.append(item("くも", ["cloud", "雲", "nature"], group("くも", cloud)))

# ---------------------------------------------------------------- たいよう (sun with glow)
SUN_CORE = 21.0
RAY_R, RAY_L = 3.4, 9.0
RAY_D = 29.0                              # centre distance of a ray capsule
RAY_HALF = RAY_L / 2 + RAY_R              # half extent of a ray
SUN_Z = RAY_D + RAY_HALF                  # so the lowest ray tip touches z = 0
sun = [sph("たいよう", (0, 0, SUN_Z), SUN_CORE, (1.0, 0.80, 0.22), blend=6, glow=2.0, roughness=0.4)]
for k in range(8):
    a = 45.0 * k
    ar = rad(a)
    d = (math.cos(ar), 0.0, math.sin(ar))
    sun.append(cap(f"ひかりA{k + 1}", (RAY_D * d[0], 0, SUN_Z + RAY_D * d[2]), RAY_R, RAY_L, (1.0, 0.70, 0.10),
                   rot=(0, 90 - a, 0), blend=5, glow=1.5, roughness=0.4))
for k in range(8):
    a = 22.5 + 45.0 * k
    ar = rad(a)
    d = (0.0, math.cos(ar), math.sin(ar))
    sun.append(cap(f"ひかりB{k + 1}", (0, RAY_D * d[1], SUN_Z + RAY_D * d[2]), RAY_R, RAY_L, (1.0, 0.70, 0.10),
                   rot=(a - 90, 0, 0), blend=5, glow=1.5, roughness=0.4))
items.append(item("たいよう", ["sun", "太陽", "nature", "glow"], group("たいよう", sun)))

# ---------------------------------------------------------------- つき (moon crescent)
# The bite is a cylinder along Y (seen from the front it cuts a true crescent). Its circle passes
# through the moon's top and bottom at x = 0, so the two horns meet exactly there.
MOON_R, MOON_D = 30.0, 20.0
MOON_BITE_R = math.hypot(MOON_R, MOON_D)
moon = [
    sph("つき", (0, 0, MOON_R), MOON_R, (0.95, 0.93, 0.80), blend=6, roughness=0.7, glow=0.3),
    shape("Cylinder", "かけた部分", pos=(MOON_D, 0, MOON_R), size=(MOON_BITE_R, 100, 1), rot=(90, 0, 0),
          op="Subtract", blend=0.5),
    sph("くぼみ1", (-14, -26, 40), 4.5, (0.95, 0.93, 0.80), op="Subtract", blend=0.5),
    sph("くぼみ2", (-24, -8, 18), 4, (0.95, 0.93, 0.80), op="Subtract", blend=0.5),
    sph("くぼみ3", (-18, -18, 46), 3, (0.95, 0.93, 0.80), op="Subtract", blend=0.5),
]
items.append(item("つき", ["moon", "crescent", "月", "nature"], group("つき", moon)))

# ---------------------------------------------------------------- にじ (rainbow)
RAINBOW = [
    (0.93, 0.20, 0.20), (0.98, 0.55, 0.15), (0.98, 0.85, 0.20),
    (0.25, 0.75, 0.35), (0.20, 0.50, 0.95), (0.55, 0.30, 0.85),
]
rainbow = []
for i, col in enumerate(RAINBOW):
    rc = 46 - 8 * i      # band centre radius
    band = [[rc - 4, -4], [rc + 4, -4], [rc + 4, 4], [rc - 4, 4]]
    rainbow.append(revolve(f"にじ{i + 1}", band, col, rot=(90, 0, 0), angle=180, blend=0,
                           roughness=0.35, coat=0.2))
items.append(item("にじ", ["rainbow", "虹", "nature"], group("にじ", rainbow)))

# ---------------------------------------------------------------- ほし (star)
STAR_R = 40.0
star = [
    shape("Star", "ほし", pos=(0, 0, round(STAR_R * math.sin(rad(54)), 2)), size=(STAR_R, 16, 1),
          rot=(90, 0, 0), params=(5, 0.45, 0, 0), color=(1.0, 0.78, 0.15), blend=0,
          metallic=0.2, roughness=0.3, glow=0.4, coat=0.5),
    sph("まんなかのふくらみ", (0, 0, round(STAR_R * math.sin(rad(54)), 2)), 12, (1.0, 0.82, 0.25),
        blend=8, glow=0.4, roughness=0.35, coat=0.5),
]
items.append(item("ほし", ["star", "星", "nature"], group("ほし", star)))

# ---------------------------------------------------------------- たき (waterfall)
falls = [
    shape("Box", "がけ", pos=(0, 8, 31), size=(64, 22, 62), round=9, color=GREY, blend=0, roughness=0.92),
    shape("Box", "こけ", pos=(0, 8, 63), size=(62, 20, 4), round=2, color=(0.30, 0.55, 0.25), blend=0,
          roughness=0.95),
    shape("Curve", "みず", pos=(0, 0, 0),
          path=[[0, -4, 58, 6.0], [1, -5, 44, 6.0], [-1, -6, 28, 5.5], [0, -7, 14, 5.5], [0, -7, 6, 5.0]],
          params=(0, 0.5, 0, 1), color=(0.45, 0.75, 0.98), blend=2, translucency=0.5,
          roughness=0.1, coat=0.5),
    cyl("いけ", (0, -18, 2), 28, 4, (0.25, 0.60, 0.90), blend=3, roughness=0.1, coat=0.6,
        translucency=0.3),
    sph("あわ", (0, -10, 5), 7, WHITE, blend=3, roughness=0.8),
    sph("いわ左", (-30, -16, 6), 9, GREY_D, scale=(1, 1, 0.8), blend=3, roughness=0.92),
    sph("いわ右", (30, -20, 5), 8, GREY, scale=(1, 1, 0.8), blend=3, roughness=0.92),
]
items.append(item("たき", ["waterfall", "滝", "nature", "water"], group("たき", falls)))

# ---------------------------------------------------------------- サボテン (cactus)
OFFX = -3.0
cactus = [
    cap("みき", (0, 0, 34), 9, 50, (0.30, 0.60, 0.28), blend=6, roughness=0.6, translucency=0.1),
    cap("よこ右", (16 + OFFX, 0, 28), 7, 14, (0.30, 0.60, 0.28), rot=(0, 90, 0), blend=6, roughness=0.6),
    cap("たて右", (27 + OFFX, 0, 40), 7, 22, (0.30, 0.60, 0.28), blend=6, roughness=0.6),
    cap("よこ左", (-14 + OFFX, 0, 42), 6, 12, (0.30, 0.60, 0.28), rot=(0, 90, 0), blend=6, roughness=0.6),
    cap("たて左", (-22 + OFFX, 0, 52), 6, 18, (0.30, 0.60, 0.28), blend=6, roughness=0.6),
    sph("はな", (0, 0, 70), 4.5, PINK, blend=2, roughness=0.5),
]
items.append(item("サボテン", ["cactus", "サボテン", "nature", "desert"], group("サボテン", cactus)))


# ------------------------------------------------------------ 10 detailed nature additions
# Each variant has a distinctive silhouette from the original simple entries above.
# Generated geometry is made entirely of native MELTCAD SDF shapes (no images/mesh imports).

items.append(item("こうようじゅ（枝ぶり）", ["nature", "広葉樹", "こうようじゅ", "broadleaf", "deciduous tree", "tree", "木"], group("こうようじゅ（枝ぶり）", [
    shape("Curve", "ひろがるみき", blend=2.4, color=(0.43, 0.28, 0.16), roughness=0.85, path=[[0, 0, 5, 5], [0, 0, 25, 4.7], [2, 0, 46, 3.8], [1, 1, 65, 2.8], [4, 2, 74, 2]]),
    shape("Curve", "えだ左1", blend=2, color=(0.32, 0.19, 0.12), roughness=0.68, path=[[1, 0, 44, 3.4], [-10, 0, 55, 2.5], [-22, 2, 70, 1.4]]),
    shape("Curve", "えだ右1", blend=2, color=(0.43, 0.28, 0.16), roughness=0.68, path=[[1, 0, 49, 3.3], [14, 1, 61, 2.5], [26, 0, 76, 1.3]]),
    shape("Curve", "えだ奥", blend=2, color=(0.43, 0.28, 0.16), roughness=0.68, path=[[1, 0, 61, 2.7], [-4, 14, 71, 1.9], [-13, 19, 80, 1]]),
    shape("Curve", "えだ前", blend=2, color=(0.43, 0.28, 0.16), roughness=0.68, path=[[1, 0, 55, 2.6], [9, -11, 67, 1.8], [15, -17, 77, 1.1]]),
    shape("Sphere", "こずえ左", pos=(-22, 1, 73), size=(16, 1, 1), scale=(1.2, 0.93, 1.02), color=(0.25, 0.57, 0.26), roughness=0.84),
    shape("Sphere", "こずえ右", pos=(24, 1, 76), size=(17, 1, 1), scale=(1.12, 0.89, 0.95), color=(0.38, 0.69, 0.3), roughness=0.84),
    shape("Sphere", "こずえ高", pos=(3, 7, 95), size=(18, 1, 1), scale=(1, 0.95, 0.93), color=(0.25, 0.57, 0.26), roughness=0.84),
    shape("Sphere", "こずえ手前", pos=(-5, -16, 78), size=(16, 1, 1), scale=(1.04, 1.15, 0.85), color=(0.38, 0.69, 0.3), roughness=0.84),
    shape("Sphere", "こずえ奥", pos=(-10, 14, 86), size=(17, 1, 1), scale=(1, 0.94, 0.88), color=(0.14, 0.46, 0.24), roughness=0.84),
    shape("Sphere", "こずえ右高", pos=(16, -3, 93), size=(14, 1, 1), scale=(1.1, 0.95, 0.92), color=(0.25, 0.57, 0.26), roughness=0.84),
    shape("Sphere", "こずえ左高", pos=(-16, -5, 91), size=(14, 1, 1), scale=(1, 0.97, 0.91), color=(0.38, 0.69, 0.3), roughness=0.84),
])))

items.append(item("まつのき（ひろがる枝）", ["nature", "松の木", "松", "pine", "japanese pine", "tree", "枝"], group("まつのき（ひろがる枝）", [
    shape("Curve", "まがったみき", blend=2.6, color=(0.32, 0.19, 0.12), roughness=0.9, path=[[0, 0, 4, 4], [1, 0, 21, 4], [6, 0, 40, 3.3], [4, 0, 57, 2.6], [7, 0, 75, 2]]),
    shape("Curve", "下左えだ", blend=2, color=(0.32, 0.19, 0.12), roughness=0.68, path=[[4, 0, 36, 2.3], [-12, 0, 40, 1.9], [-25, -2, 44, 1.2]]),
    shape("Curve", "中右えだ", blend=2, color=(0.32, 0.19, 0.12), roughness=0.68, path=[[5, 0, 48, 2.1], [18, 1, 54, 1.9], [31, 3, 57, 1.2]]),
    shape("Curve", "上左えだ", blend=2, color=(0.32, 0.19, 0.12), roughness=0.68, path=[[5, 0, 63, 2], [-10, 0, 71, 1.5], [-20, -2, 73, 1]]),
    shape("Curve", "奥えだ", blend=2, color=(0.32, 0.19, 0.12), roughness=0.68, path=[[4, 0, 57, 1.8], [2, 15, 65, 1.3], [1, 24, 68, 0.8]]),
    shape("Sphere", "ひだり低", pos=(-25, -2, 47), size=(12, 1, 1), scale=(1.5, 0.9, 0.48), blend=3.8, color=(0.11, 0.32, 0.2), roughness=0.95),
    shape("Sphere", "ひだり中", pos=(-12, 2, 51), size=(12, 1, 1), scale=(1.4, 0.8, 0.53), blend=3.8, color=(0.15, 0.42, 0.26), roughness=0.95),
    shape("Sphere", "みぎ中", pos=(29, 2, 59), size=(12, 1, 1), scale=(1.5, 0.85, 0.49), blend=3.8, color=(0.11, 0.32, 0.2), roughness=0.95),
    shape("Sphere", "みぎ上", pos=(16, 3, 62), size=(11, 1, 1), scale=(1.3, 0.9, 0.52), blend=3.8, color=(0.15, 0.42, 0.26), roughness=0.95),
    shape("Sphere", "ひだり上", pos=(-20, -2, 76), size=(10, 1, 1), scale=(1.5, 0.86, 0.53), blend=3.8, color=(0.11, 0.32, 0.2), roughness=0.95),
    shape("Sphere", "おく", pos=(1, 22, 70), size=(12, 1, 1), scale=(1.3, 1, 0.5), blend=3.8, color=(0.15, 0.42, 0.26), roughness=0.95),
    shape("Sphere", "てっぺん", pos=(7, 0, 82), size=(13, 1, 1), scale=(1.45, 1.05, 0.58), blend=3.8, color=(0.11, 0.32, 0.2), roughness=0.95),
])))

items.append(item("さくらのき（まんかい）", ["nature", "桜の木", "桜", "さくら", "cherry blossom", "sakura", "tree", "pink"], group("さくらのき（まんかい）", [
    shape("Curve", "さくらの幹", blend=2, color=(0.43, 0.28, 0.16), roughness=0.84, path=[[0, 0, 5, 5], [0, 0, 24, 4.4], [2, 0, 43, 3.7], [4, 1, 60, 2.6]]),
    shape("Curve", "左枝", blend=2, color=(0.32, 0.19, 0.12), roughness=0.68, path=[[2, 0, 39, 3], [-10, 0, 52, 2], [-22, 0, 68, 1.4]]),
    shape("Curve", "右枝", blend=2, color=(0.43, 0.28, 0.16), roughness=0.68, path=[[2, 0, 42, 3], [12, 0, 57, 2], [22, 3, 72, 1.3]]),
    shape("Curve", "前枝", blend=2, color=(0.43, 0.28, 0.16), roughness=0.68, path=[[2, 0, 48, 2.6], [2, -11, 65, 1.8], [6, -18, 76, 1.2]]),
    shape("Curve", "奥枝", blend=2, color=(0.43, 0.28, 0.16), roughness=0.68, path=[[2, 0, 53, 2.6], [-3, 10, 67, 1.8], [-9, 16, 80, 1.1]]),
    shape("Sphere", "はな左1", pos=(-27, 0, 72), size=(15, 1, 1), scale=(1.13, 0.97, 0.84), blend=5.5, color=(0.98, 0.62, 0.75), roughness=0.87),
    shape("Sphere", "はな左2", pos=(-16, -4, 83), size=(15, 1, 1), scale=(1.13, 0.97, 0.84), blend=5.5, color=(0.99, 0.77, 0.84), roughness=0.87),
    shape("Sphere", "はな右1", pos=(27, 2, 74), size=(14, 1, 1), scale=(1.13, 0.97, 0.84), blend=5.5, color=(0.98, 0.62, 0.75), roughness=0.87),
    shape("Sphere", "はな右2", pos=(16, -5, 88), size=(15, 1, 1), scale=(1.13, 0.97, 0.84), blend=5.5, color=(0.99, 0.77, 0.84), roughness=0.87),
    shape("Sphere", "はな中央", pos=(3, 4, 90), size=(18, 1, 1), scale=(1.13, 0.97, 0.84), blend=5.5, color=(0.98, 0.62, 0.75), roughness=0.87),
    shape("Sphere", "はな頂点", pos=(5, 8, 105), size=(13, 1, 1), scale=(1.13, 0.97, 0.84), blend=5.5, color=(0.99, 0.77, 0.84), roughness=0.87),
    shape("Sphere", "はな前", pos=(8, -18, 78), size=(14, 1, 1), scale=(1.13, 0.97, 0.84), blend=5.5, color=(0.91, 0.4, 0.6), roughness=0.87),
    shape("Sphere", "はな奥", pos=(-11, 17, 83), size=(15, 1, 1), scale=(1.13, 0.97, 0.84), blend=5.5, color=(0.98, 0.62, 0.75), roughness=0.87),
    shape("Sphere", "はな上左", pos=(-13, 5, 103), size=(12, 1, 1), scale=(1.13, 0.97, 0.84), blend=5.5, color=(0.99, 0.77, 0.84), roughness=0.87),
    shape("Sphere", "はな右前", pos=(19, -15, 78), size=(12, 1, 1), scale=(1.13, 0.97, 0.84), blend=5.5, color=(0.98, 0.62, 0.75), roughness=0.87),
])))

items.append(item("やしのき（ふたご）", ["nature", "ヤシの木", "ヤシ", "palm trees", "twin palm", "tree", "南国"], group("やしのき（ふたご）", [
    shape("Curve", "左みき", blend=2, color=(0.43, 0.28, 0.16), roughness=0.85, path=[[-8.95, 0, 3.8, 3.8], [-11.2, 0, 26.180000000000003, 3.1], [-15.07, 0, 51.59, 2.5], [-13, 0, 77, 2]]),
    shape("Curve", "左はっぱ1", blend=2.2, color=(0.14, 0.46, 0.24), roughness=0.62, path=[[-13, 0, 77, 2.8], [-4.087587381325866, -1.252557908640589, 82, 2.5], [8.568038537191406, -3.0311901389102256, 78, 1.7], [19.678846268471823, -4.59271233168216, 65, 0.75]]),
    shape("Curve", "左はっぱ2", blend=2.2, color=(0.38, 0.69, 0.3), roughness=0.62, path=[[-13, 0, 77, 2.8], [-9.054659678898304, 8.089146416692504, 82, 2.5], [-3.4522764229338936, 19.575734328395857, 78, 1.7], [1.4662478440395557, 29.66020352787251, 65, 0.75]]),
    shape("Curve", "左はっぱ3", blend=2.2, color=(0.14, 0.46, 0.24), roughness=0.62, path=[[-13, 0, 77, 2.8], [-19.47405820304786, 6.251925334130974, 82, 2.5], [-28.667220851375824, 15.129659308596958, 78, 1.7], [-36.738213411175494, 22.923726225146904, 65, 0.75]]),
    shape("Curve", "左はっぱ4", blend=2.2, color=(0.38, 0.69, 0.3), roughness=0.62, path=[[-13, 0, 77, 2.8], [-20.946528335730342, -4.225244065073018, 82, 2.5], [-32.23059857246743, -10.225090637476704, 78, 1.7], [-42.13727056434459, -15.492561571934399, 65, 0.75]]),
    shape("Curve", "左はっぱ5", blend=2.2, color=(0.14, 0.46, 0.24), roughness=0.62, path=[[-13, 0, 77, 2.8], [-11.43716640099763, -8.863269777109874, 82, 2.5], [-9.217942690414265, -21.449112860605894, 78, 1.7], [-7.269610136991311, -32.498655849402866, 65, 0.75]]),
    shape("Curve", "右みき", blend=2, color=(0.43, 0.28, 0.16), roughness=0.85, path=[[12.85, 0, 3.8, 3.8], [14.6, 0, 20.740000000000002, 3.1], [17.61, 0, 40.870000000000005, 2.5], [16, 0, 61, 2]]),
    shape("Curve", "右はっぱ1", blend=2.2, color=(0.14, 0.46, 0.24), roughness=0.62, path=[[16, 0, 61, 2.8], [24.559508646656383, 2.7811529493745266, 66, 2.5], [32.32012981962484, 5.302731623474098, 62, 1.7], [40.727469423673995, 8.034441853748632, 49, 0.75]]),
    shape("Curve", "右はっぱ2", blend=2.2, color=(0.38, 0.69, 0.3), roughness=0.62, path=[[16, 0, 61, 2.8], [16, 9, 66, 2.5], [16, 17.16, 62, 1.7], [16, 26, 49, 0.75]]),
    shape("Curve", "右はっぱ3", blend=2.2, color=(0.14, 0.46, 0.24), roughness=0.62, path=[[16, 0, 61, 2.8], [7.440491353343619, 2.7811529493745275, 66, 2.5], [-0.3201298196248352, 5.3027316234740995, 62, 1.7], [-8.727469423673991, 8.034441853748636, 49, 0.75]]),
    shape("Curve", "右はっぱ4", blend=2.2, color=(0.38, 0.69, 0.3), roughness=0.62, path=[[16, 0, 61, 2.8], [10.70993272936774, -7.281152949374526, 66, 2.5], [5.913605070661159, -13.882731623474097, 62, 1.7], [0.7175834403956962, -21.034441853748632, 49, 0.75]]),
    shape("Curve", "右はっぱ5", blend=2.2, color=(0.14, 0.46, 0.24), roughness=0.62, path=[[16, 0, 61, 2.8], [21.290067270632257, -7.281152949374528, 66, 2.5], [26.086394929338837, -13.8827316234741, 62, 1.7], [31.282416559604297, -21.034441853748636, 49, 0.75]]),
    shape("Sphere", "み左", pos=(-14, -3, 72), size=(3.1, 1, 1), blend=1, color=(0.32, 0.19, 0.12), roughness=0.9),
    shape("Sphere", "み右", pos=(17, -2, 56), size=(2.8, 1, 1), blend=1, color=(0.32, 0.19, 0.12), roughness=0.9),
])))

items.append(item("たまサボテン（はな）", ["nature", "サボテン", "たまサボテン", "cactus", "barrel cactus", "多肉植物"], group("たまサボテン（はな）", [
    shape("Sphere", "まるい本体", pos=(0, 0, 21), size=(21, 1, 1), scale=(0.96, 0.96, 1), blend=3, color=(0.14, 0.46, 0.24), roughness=0.74),
    shape("Curve", "たてのひだ1", blend=1.3, color=(0.38, 0.69, 0.3), roughness=0.76, path=[[11, 0, 4, 1.05], [17, 0, 10, 1.05], [20.2, 0, 21, 1.05], [17, 0, 33, 1.05], [11, 0, 39, 1.05]]),
    shape("Curve", "たてのひだ2", blend=1.3, color=(0.25, 0.57, 0.26), roughness=0.76, path=[[8.43, 7.07, 4, 1.05], [13.02, 10.93, 10, 1.05], [15.47, 12.98, 21, 1.05], [13.02, 10.93, 33, 1.05], [8.43, 7.07, 39, 1.05]]),
    shape("Curve", "たてのひだ3", blend=1.3, color=(0.38, 0.69, 0.3), roughness=0.76, path=[[1.91, 10.83, 4, 1.05], [2.95, 16.74, 10, 1.05], [3.51, 19.89, 21, 1.05], [2.95, 16.74, 33, 1.05], [1.91, 10.83, 39, 1.05]]),
    shape("Curve", "たてのひだ4", blend=1.3, color=(0.25, 0.57, 0.26), roughness=0.76, path=[[-5.5, 9.53, 4, 1.05], [-8.5, 14.72, 10, 1.05], [-10.1, 17.49, 21, 1.05], [-8.5, 14.72, 33, 1.05], [-5.5, 9.53, 39, 1.05]]),
    shape("Curve", "たてのひだ5", blend=1.3, color=(0.38, 0.69, 0.3), roughness=0.76, path=[[-10.34, 3.76, 4, 1.05], [-15.97, 5.81, 10, 1.05], [-18.98, 6.91, 21, 1.05], [-15.97, 5.81, 33, 1.05], [-10.34, 3.76, 39, 1.05]]),
    shape("Curve", "たてのひだ6", blend=1.3, color=(0.25, 0.57, 0.26), roughness=0.76, path=[[-10.34, -3.76, 4, 1.05], [-15.97, -5.81, 10, 1.05], [-18.98, -6.91, 21, 1.05], [-15.97, -5.81, 33, 1.05], [-10.34, -3.76, 39, 1.05]]),
    shape("Curve", "たてのひだ7", blend=1.3, color=(0.38, 0.69, 0.3), roughness=0.76, path=[[-5.5, -9.53, 4, 1.05], [-8.5, -14.72, 10, 1.05], [-10.1, -17.49, 21, 1.05], [-8.5, -14.72, 33, 1.05], [-5.5, -9.53, 39, 1.05]]),
    shape("Curve", "たてのひだ8", blend=1.3, color=(0.25, 0.57, 0.26), roughness=0.76, path=[[1.91, -10.83, 4, 1.05], [2.95, -16.74, 10, 1.05], [3.51, -19.89, 21, 1.05], [2.95, -16.74, 33, 1.05], [1.91, -10.83, 39, 1.05]]),
    shape("Curve", "たてのひだ9", blend=1.3, color=(0.38, 0.69, 0.3), roughness=0.76, path=[[8.43, -7.07, 4, 1.05], [13.02, -10.93, 10, 1.05], [15.47, -12.98, 21, 1.05], [13.02, -10.93, 33, 1.05], [8.43, -7.07, 39, 1.05]]),
    shape("Sphere", "はな中心", pos=(0, 0, 43.4), size=(3.1, 1, 1), blend=1, color=(1, 0.78, 0.18)),
    shape("Sphere", "はなびら1", pos=(4.5, 0, 43), size=(3.7, 1, 1), scale=(0.95, 0.74, 0.38), blend=1, color=(0.91, 0.4, 0.6), roughness=0.48, coat=0.2),
    shape("Sphere", "はなびら2", pos=(2.25, 3.9, 43), size=(3.7, 1, 1), scale=(0.95, 0.74, 0.38), blend=1, color=(0.91, 0.4, 0.6), roughness=0.48, coat=0.2),
    shape("Sphere", "はなびら3", pos=(-2.25, 3.9, 43), size=(3.7, 1, 1), scale=(0.95, 0.74, 0.38), blend=1, color=(0.91, 0.4, 0.6), roughness=0.48, coat=0.2),
    shape("Sphere", "はなびら4", pos=(-4.5, 0, 43), size=(3.7, 1, 1), scale=(0.95, 0.74, 0.38), blend=1, color=(0.91, 0.4, 0.6), roughness=0.48, coat=0.2),
    shape("Sphere", "はなびら5", pos=(-2.25, -3.9, 43), size=(3.7, 1, 1), scale=(0.95, 0.74, 0.38), blend=1, color=(0.91, 0.4, 0.6), roughness=0.48, coat=0.2),
    shape("Sphere", "はなびら6", pos=(2.25, -3.9, 43), size=(3.7, 1, 1), scale=(0.95, 0.74, 0.38), blend=1, color=(0.91, 0.4, 0.6), roughness=0.48, coat=0.2),
])))

items.append(item("かんようしょくぶつ（はち）", ["nature", "観葉植物", "鉢植え", "houseplant", "potted plant", "interior", "植物"], group("かんようしょくぶつ（はち）", [
    shape("Revolve", "うえがひらいた植木ばち", blend=0.5, color=(0.75, 0.49, 0.32), roughness=0.76, outline=[[0, 0], [11.3, 0], [12, 1.5], [13, 19], [13.5, 22], [11.6, 22], [10.8, 20], [10, 4], [0, 4]]),
    shape("Torus", "うえのふち", pos=(0, 0, 21.7), size=(12.3, 1.35, 1), blend=0.8, color=(0.63, 0.36, 0.23), roughness=0.72),
    shape("Cylinder", "土", pos=(0, 0, 17.9), size=(10, 2.2, 1), blend=1, color=(0.29, 0.2, 0.14), roughness=0.94),
    shape("Curve", "くき1", blend=1.5, color=(0.26, 0.53, 0.19), roughness=0.73, path=[[0, 0, 18, 1.5], [3, 0, 35, 1.25], [-12, -2, 53, 1]]),
    shape("Sphere", "おおきなは1", pos=(-13.919999999999998, -2.32, 57), size=(8.7, 1, 1), rot=(-12, -27, 8), scale=(1.25, 0.42, 1.75), blend=1.8, color=(0.25, 0.57, 0.26), roughness=0.42, coat=0.32),
    shape("Curve", "くき2", blend=1.5, color=(0.26, 0.53, 0.19), roughness=0.73, path=[[0, 0, 19, 1.5], [5, 1, 35, 1.2], [16, 0, 54, 1]]),
    shape("Sphere", "おおきなは2", pos=(18.56, 0, 58), size=(8.7, 1, 1), rot=(-12, 24, -12), scale=(1.25, 0.42, 1.75), blend=1.8, color=(0.14, 0.46, 0.24), roughness=0.42, coat=0.32),
    shape("Curve", "くき3", blend=1.5, color=(0.26, 0.53, 0.19), roughness=0.73, path=[[0, 0, 18, 1.2], [0, -3, 39, 1], [2, -12, 67, 0.9]]),
    shape("Sphere", "おおきなは3", pos=(2.32, -13.919999999999998, 71), size=(8.7, 1, 1), rot=(-12, 0, 5), scale=(1.25, 0.42, 1.75), blend=1.8, color=(0.38, 0.69, 0.3), roughness=0.42, coat=0.32),
    shape("Curve", "くき4", blend=1.5, color=(0.26, 0.53, 0.19), roughness=0.73, path=[[1, 0, 18, 1.4], [-5, 4, 37, 1.1], [-17, 10, 56, 0.9]]),
    shape("Sphere", "おおきなは4", pos=(-19.72, 11.6, 60), size=(8.7, 1, 1), rot=(-12, -34, 12), scale=(1.25, 0.42, 1.75), blend=1.8, color=(0.25, 0.57, 0.26), roughness=0.42, coat=0.32),
    shape("Curve", "くき5", blend=1.5, color=(0.26, 0.53, 0.19), roughness=0.73, path=[[0, 0, 18, 1.2], [3, 4, 40, 1], [12, 15, 66, 0.9]]),
    shape("Sphere", "おおきなは5", pos=(13.919999999999998, 17.4, 70), size=(8.7, 1, 1), rot=(-12, 27, -10), scale=(1.25, 0.42, 1.75), blend=1.8, color=(0.38, 0.69, 0.3), roughness=0.42, coat=0.32),
    shape("Curve", "くき6", blend=1.5, color=(0.26, 0.53, 0.19), roughness=0.73, path=[[0, 0, 19, 1.1], [-3, -2, 35, 0.9], [-5, -14, 47, 0.9]]),
    shape("Sphere", "おおきなは6", pos=(-5.8, -16.24, 51), size=(8.7, 1, 1), rot=(-12, -8, 6), scale=(1.25, 0.42, 1.75), blend=1.8, color=(0.14, 0.46, 0.24), roughness=0.42, coat=0.32),
])))

items.append(item("チューリップ（3りん）", ["nature", "チューリップ", "tulip", "bouquet", "tulip flowers", "花"], group("チューリップ（3りん）", [
    shape("Curve", "くき1", blend=2.1, color=(0.26, 0.53, 0.19), roughness=0.68, path=[[-12, -1, 1.7, 1.7], [-11, -1, 19, 1.5], [-13, -1, 36, 1.35], [-12, -1, 43, 1.3]]),
    shape("Sphere", "はっぱ1", pos=(-6, -1, 19.78), size=(7.3, 1, 1), rot=(-9, 38, 0), scale=(1.6, 0.32, 1.05), blend=2, color=(0.25, 0.57, 0.26), roughness=0.7),
    shape("Revolve", "はなカップ1", pos=(-12, -1, 41), blend=2, color=(0.88, 0.15, 0.24), coat=0.27, outline=[[0, 0], [3.5, 0], [6, 4], [7, 9], [6.8, 14], [5.3, 12], [0, 11.5]]),
    shape("Sphere", "はなびら1-1", pos=(-7.32, 1.7, 51.3), size=(5.5, 1, 1), rot=(16, 0, 0), scale=(0.85, 0.46, 1.45), blend=1.1, color=(0.88, 0.15, 0.24), roughness=0.46, coat=0.22),
    shape("Sphere", "はなびら1-2", pos=(-16.68, 1.7, 51.3), size=(5.5, 1, 1), rot=(16, 0, 120), scale=(0.85, 0.46, 1.45), blend=1.1, color=(0.88, 0.15, 0.24), roughness=0.46, coat=0.22),
    shape("Sphere", "はなびら1-3", pos=(-12, -6.4, 51.3), size=(5.5, 1, 1), rot=(16, 0, 240), scale=(0.85, 0.46, 1.45), blend=1.1, color=(0.88, 0.15, 0.24), roughness=0.46, coat=0.22),
    shape("Curve", "くき2", blend=2.1, color=(0.26, 0.53, 0.19), roughness=0.68, path=[[0, 4, 1.7, 1.7], [1, 4, 19, 1.5], [-1, 4, 51, 1.35], [0, 4, 58, 1.3]]),
    shape("Sphere", "はっぱ2", pos=(-5, 4, 26.68), size=(7.3, 1, 1), rot=(-9, -42, 0), scale=(1.6, 0.32, 1.05), blend=2, color=(0.38, 0.69, 0.3), roughness=0.7),
    shape("Revolve", "はなカップ2", pos=(0, 4, 56), blend=2, color=(1, 0.78, 0.18), coat=0.27, outline=[[0, 0], [3.5, 0], [6, 4], [7, 9], [6.8, 14], [5.3, 12], [0, 11.5]]),
    shape("Sphere", "はなびら2-1", pos=(4.68, 6.7, 66.3), size=(5.5, 1, 1), rot=(16, 0, 0), scale=(0.85, 0.46, 1.45), blend=1.1, color=(1, 0.78, 0.18), roughness=0.46, coat=0.22),
    shape("Sphere", "はなびら2-2", pos=(-4.68, 6.7, 66.3), size=(5.5, 1, 1), rot=(16, 0, 120), scale=(0.85, 0.46, 1.45), blend=1.1, color=(1, 0.78, 0.18), roughness=0.46, coat=0.22),
    shape("Sphere", "はなびら2-3", pos=(0, -1.4, 66.3), size=(5.5, 1, 1), rot=(16, 0, 240), scale=(0.85, 0.46, 1.45), blend=1.1, color=(1, 0.78, 0.18), roughness=0.46, coat=0.22),
    shape("Curve", "くき3", blend=2.1, color=(0.26, 0.53, 0.19), roughness=0.68, path=[[13, -3, 1.7, 1.7], [14, -3, 19, 1.5], [12, -3, 42, 1.35], [13, -3, 49, 1.3]]),
    shape("Sphere", "はっぱ3", pos=(19, -3, 22.540000000000003), size=(7.3, 1, 1), rot=(-9, 38, 0), scale=(1.6, 0.32, 1.05), blend=2, color=(0.25, 0.57, 0.26), roughness=0.7),
    shape("Revolve", "はなカップ3", pos=(13, -3, 47), blend=2, color=(0.71, 0.3, 0.73), coat=0.27, outline=[[0, 0], [3.5, 0], [6, 4], [7, 9], [6.8, 14], [5.3, 12], [0, 11.5]]),
    shape("Sphere", "はなびら3-1", pos=(17.68, -0.3, 57.3), size=(5.5, 1, 1), rot=(16, 0, 0), scale=(0.85, 0.46, 1.45), blend=1.1, color=(0.71, 0.3, 0.73), roughness=0.46, coat=0.22),
    shape("Sphere", "はなびら3-2", pos=(8.32, -0.3, 57.3), size=(5.5, 1, 1), rot=(16, 0, 120), scale=(0.85, 0.46, 1.45), blend=1.1, color=(0.71, 0.3, 0.73), roughness=0.46, coat=0.22),
    shape("Sphere", "はなびら3-3", pos=(13, -8.4, 57.3), size=(5.5, 1, 1), rot=(16, 0, 240), scale=(0.85, 0.46, 1.45), blend=1.1, color=(0.71, 0.3, 0.73), roughness=0.46, coat=0.22),
])))

items.append(item("バラ（かいか）", ["nature", "バラ", "薔薇", "rose", "rose flower", "flower", "花"], group("バラ（かいか）", [
    shape("Curve", "バラのくき", blend=1.7, color=(0.26, 0.53, 0.19), roughness=0.68, path=[[0, 0, 1.6, 1.6], [1, 0, 25, 1.55], [-1, 0, 46, 1.5], [0, 0, 63, 1.45]]),
    shape("Sphere", "左のは", pos=(-8, 0, 27), size=(7.8, 1, 1), rot=(0, -30, 0), scale=(1.7, 0.35, 0.65), blend=1.5, color=(0.25, 0.57, 0.26), roughness=0.72),
    shape("Sphere", "右のは", pos=(9, 1, 35), size=(8, 1, 1), rot=(0, 30, 0), scale=(1.7, 0.35, 0.65), blend=1.5, color=(0.38, 0.69, 0.3), roughness=0.72),
    shape("Sphere", "うしろのは", pos=(0, 8, 43), size=(6.5, 1, 1), rot=(25, 0, 0), scale=(0.85, 1.7, 0.47), blend=1.5, color=(0.14, 0.46, 0.24), roughness=0.72),
    shape("Cone", "とげ左", pos=(-1, 0, 19), size=(2.2, 4.5, 1), rot=(0, -62, 0), blend=0.5, color=(0.14, 0.46, 0.24), roughness=0.68),
    shape("Cone", "とげ右", pos=(1, 0, 32), size=(2, 4.5, 1), rot=(0, 64, 0), blend=0.5, color=(0.14, 0.46, 0.24), roughness=0.68),
    shape("Sphere", "はなの芯", pos=(0, 0, 65), size=(8.3, 1, 1), scale=(0.9, 0.9, 1.1), blend=2.2, color=(0.64, 0.08, 0.18), roughness=0.48),
    shape("Sphere", "そとの花びら1", pos=(9, 0, 63.7), size=(6.7, 1, 1), rot=(-12, 14, 0), scale=(0.9, 0.45, 1.42), blend=1.4, color=(0.88, 0.15, 0.24), roughness=0.46, coat=0.22),
    shape("Sphere", "そとの花びら2", pos=(6.36, 6.36, 65.5), size=(6.7, 1, 1), rot=(-12, 14, 45), scale=(0.9, 0.45, 1.42), blend=1.4, color=(0.98, 0.29, 0.42), roughness=0.46, coat=0.22),
    shape("Sphere", "そとの花びら3", pos=(0, 9, 63.7), size=(6.7, 1, 1), rot=(-12, 14, 90), scale=(0.9, 0.45, 1.42), blend=1.4, color=(0.88, 0.15, 0.24), roughness=0.46, coat=0.22),
    shape("Sphere", "そとの花びら4", pos=(-6.36, 6.36, 65.5), size=(6.7, 1, 1), rot=(-12, 14, 135), scale=(0.9, 0.45, 1.42), blend=1.4, color=(0.98, 0.29, 0.42), roughness=0.46, coat=0.22),
    shape("Sphere", "そとの花びら5", pos=(-9, 0, 63.7), size=(6.7, 1, 1), rot=(-12, 14, 180), scale=(0.9, 0.45, 1.42), blend=1.4, color=(0.88, 0.15, 0.24), roughness=0.46, coat=0.22),
    shape("Sphere", "そとの花びら6", pos=(-6.36, -6.36, 65.5), size=(6.7, 1, 1), rot=(-12, 14, 225), scale=(0.9, 0.45, 1.42), blend=1.4, color=(0.98, 0.29, 0.42), roughness=0.46, coat=0.22),
    shape("Sphere", "そとの花びら7", pos=(0, -9, 63.7), size=(6.7, 1, 1), rot=(-12, 14, 270), scale=(0.9, 0.45, 1.42), blend=1.4, color=(0.88, 0.15, 0.24), roughness=0.46, coat=0.22),
    shape("Sphere", "そとの花びら8", pos=(6.36, -6.36, 65.5), size=(6.7, 1, 1), rot=(-12, 14, 315), scale=(0.9, 0.45, 1.42), blend=1.4, color=(0.98, 0.29, 0.42), roughness=0.46, coat=0.22),
    shape("Sphere", "うちの花びら1", pos=(3.39, 3.39, 69.7), size=(5.3, 1, 1), rot=(-10, 0, 45), scale=(0.8, 0.38, 1.25), blend=1.4, color=(0.64, 0.08, 0.18), roughness=0.46, coat=0.22),
    shape("Sphere", "うちの花びら2", pos=(-3.39, 3.39, 69.7), size=(5.3, 1, 1), rot=(-10, 0, 135), scale=(0.8, 0.38, 1.25), blend=1.4, color=(0.64, 0.08, 0.18), roughness=0.46, coat=0.22),
    shape("Sphere", "うちの花びら3", pos=(-3.39, -3.39, 69.7), size=(5.3, 1, 1), rot=(-10, 0, 225), scale=(0.8, 0.38, 1.25), blend=1.4, color=(0.64, 0.08, 0.18), roughness=0.46, coat=0.22),
    shape("Sphere", "うちの花びら4", pos=(3.39, -3.39, 69.7), size=(5.3, 1, 1), rot=(-10, 0, 315), scale=(0.8, 0.38, 1.25), blend=1.4, color=(0.64, 0.08, 0.18), roughness=0.46, coat=0.22),
])))

items.append(item("しいたけ（茶色のキノコ）", ["nature", "キノコ", "椎茸", "しいたけ", "mushroom", "shiitake", "fungus"], group("しいたけ（茶色のキノコ）", [
    shape("Revolve", "ふとい軸", blend=2, color=(0.9, 0.8, 0.63), roughness=0.79, outline=[[0, 0], [6.8, 0], [6, 3], [5, 12], [5.4, 22], [0, 22]]),
    shape("Revolve", "茶色いかさ", blend=1, color=(0.47, 0.27, 0.15), roughness=0.84, outline=[[0, 20], [7, 20], [16, 19], [23, 19], [25, 21], [23, 26], [17, 30], [9, 33], [0, 33]]),
    shape("Torus", "ひだのふち", pos=(0, 0, 20), size=(19.5, 1.2, 1), blend=1, color=(0.8, 0.65, 0.43), roughness=0.83),
    shape("Curve", "うらのひだ1", blend=0.55, color=(0.8, 0.65, 0.43), roughness=0.89, path=[[7, 0, 19.5, 0.8], [14, 0, 19.5, 0.75], [21, 0, 20.3, 0.55]]),
    shape("Curve", "うらのひだ2", blend=0.55, color=(0.8, 0.65, 0.43), roughness=0.89, path=[[4.949747468305833, 4.949747468305832, 19.5, 0.8], [9.899494936611665, 9.899494936611664, 19.5, 0.75], [14.849242404917499, 14.849242404917497, 20.3, 0.55]]),
    shape("Curve", "うらのひだ3", blend=0.55, color=(0.8, 0.65, 0.43), roughness=0.89, path=[[4.286263797015736e-16, 7, 19.5, 0.8], [8.572527594031472e-16, 14, 19.5, 0.75], [1.2858791391047208e-15, 21, 20.3, 0.55]]),
    shape("Curve", "うらのひだ4", blend=0.55, color=(0.8, 0.65, 0.43), roughness=0.89, path=[[-4.949747468305832, 4.949747468305833, 19.5, 0.8], [-9.899494936611664, 9.899494936611665, 19.5, 0.75], [-14.849242404917497, 14.849242404917499, 20.3, 0.55]]),
    shape("Curve", "うらのひだ5", blend=0.55, color=(0.8, 0.65, 0.43), roughness=0.89, path=[[-7, 8.572527594031472e-16, 19.5, 0.8], [-14, 1.7145055188062944e-15, 19.5, 0.75], [-21, 2.5717582782094417e-15, 20.3, 0.55]]),
    shape("Curve", "うらのひだ6", blend=0.55, color=(0.8, 0.65, 0.43), roughness=0.89, path=[[-4.949747468305834, -4.949747468305832, 19.5, 0.8], [-9.899494936611667, -9.899494936611664, 19.5, 0.75], [-14.8492424049175, -14.849242404917497, 20.3, 0.55]]),
    shape("Curve", "うらのひだ7", blend=0.55, color=(0.8, 0.65, 0.43), roughness=0.89, path=[[-1.2858791391047208e-15, -7, 19.5, 0.8], [-2.5717582782094417e-15, -14, 19.5, 0.75], [-3.857637417314163e-15, -21, 20.3, 0.55]]),
    shape("Curve", "うらのひだ8", blend=0.55, color=(0.8, 0.65, 0.43), roughness=0.89, path=[[4.949747468305832, -4.949747468305834, 19.5, 0.8], [9.899494936611664, -9.899494936611667, 19.5, 0.75], [14.849242404917494, -14.8492424049175, 20.3, 0.55]]),
    shape("Sphere", "かさのもよう1", pos=(-7, -5, 30), size=(2.5, 1, 1), scale=(1.5, 0.9, 0.35), blend=0.8, color=(0.59, 0.38, 0.22), roughness=0.91),
    shape("Sphere", "かさのもよう2", pos=(10, 3, 30), size=(2.3, 1, 1), scale=(1.3, 0.9, 0.35), blend=0.8, color=(0.59, 0.38, 0.22), roughness=0.91),
])))

items.append(item("ごつごついわ（がんせき）", ["nature", "岩", "岩石", "rock", "boulder", "angular rock", "stone"], group("ごつごついわ（がんせき）", [
    shape("Box", "安定した底", pos=(0, 0, 5), size=(56, 42, 10), blend=2, color=(0.37, 0.41, 0.45), roughness=0.96, round=1.8),
    shape("Octa", "おおきい岩", pos=(0, 1, 30), size=(31, 1, 1), rot=(3, 0, 18), scale=(1.2, 1, 0.97), blend=2.3, color=(0.49, 0.51, 0.53), roughness=0.96),
    shape("Octa", "左にかたむく岩", pos=(-20, -4, 22), size=(20, 1, 1), rot=(8, 7, -16), scale=(1.18, 1.1, 0.87), blend=2.2, color=(0.37, 0.41, 0.45), roughness=0.96),
    shape("Octa", "右の岩", pos=(22, 3, 20), size=(20, 1, 1), rot=(5, -8, 19), scale=(1.03, 0.88, 0.95), blend=2.3, color=(0.63, 0.62, 0.6), roughness=0.96),
    shape("Octa", "うしろの岩", pos=(-2, 14, 28), size=(18, 1, 1), rot=(-14, 7, 10), scale=(1, 1.15, 0.83), blend=2, color=(0.37, 0.41, 0.45), roughness=0.96),
    shape("Octa", "手前の岩", pos=(7, -14, 14), size=(13, 1, 1), rot=(8, 0, -11), scale=(1.15, 1.05, 0.96), blend=1.8, color=(0.49, 0.51, 0.53), roughness=0.96),
    shape("Octa", "小石ひだり", pos=(-28, -12, 8), size=(8, 1, 1), scale=(1.3, 0.8, 1), blend=1.5, color=(0.63, 0.62, 0.6), roughness=0.96),
    shape("Curve", "岩のすじ", blend=0.55, color=(0.37, 0.41, 0.45), roughness=0.99, path=[[-13, -19, 20, 0.8], [-2, -22, 16, 0.7], [12, -19, 10, 0.6]]),
])))

if __name__ == "__main__":
    write_category("nature.json", "しぜん", items)
    preview(items, outdir="/tmp/lib_preview_nature")
