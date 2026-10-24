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


if __name__ == "__main__":
    write_category("nature.json", "しぜん", items)
    preview(items, outdir="/tmp/lib_preview_nature")
