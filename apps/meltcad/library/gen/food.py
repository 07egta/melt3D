"""Generates ../food.json (category たべもの). Run from this folder:

    export MELTCAD_BIN=/path/to/meltcad
    python3 food.py      # writes ../food.json and renders previews
"""
import math

from lib import *

# lib.shape() fills params with zeros; these kinds need real defaults.
PARAMS = {"Revolve": (360, 0, 0, 0), "Star": (5, 0.45, 0, 0), "Curve": (0, 0.7, 0, 1)}


def sh(kind, name, **kw):
    kw.setdefault("params", PARAMS.get(kind, (0, 0, 0, 0)))
    kw.setdefault("blend", 3.0)
    return shape(kind, name, **kw)


# ---- geometry helpers -------------------------------------------------------

def hull(points):
    """Convex hull (monotone chain), counter-clockwise."""
    pts = sorted(set((round(x, 3), round(y, 3)) for x, y in points))

    def cross(o, a, b):
        return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])

    lower, upper = [], []
    for p in pts:
        while len(lower) >= 2 and cross(lower[-2], lower[-1], p) <= 0:
            lower.pop()
        lower.append(p)
    for p in reversed(pts):
        while len(upper) >= 2 and cross(upper[-2], upper[-1], p) <= 0:
            upper.pop()
        upper.append(p)
    return [list(p) for p in lower[:-1] + upper[:-1]]


def rounded_outline(corners, r, n=12):
    """Convex polygon with every corner rounded by radius r (hull of circles)."""
    pts = []
    for cx, cy in corners:
        for k in range(n):
            a = 2 * math.pi * k / n
            pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return hull(pts)


def half_disc(R, n=30):
    return [[R * math.cos(math.pi * i / n), R * math.sin(math.pi * i / n)] for i in range(n + 1)]


def outer_r(profile, z):
    """Outer radius of a Revolve profile (x = radius, y = height) at height z."""
    best = 0.0
    for (r1, z1), (r2, z2) in zip(profile, profile[1:] + profile[:1]):
        if z1 != z2 and (z1 - z) * (z2 - z) <= 0:
            t = (z - z1) / (z2 - z1)
            best = max(best, r1 + (r2 - r1) * t)
    return best


def top_z(profile, r):
    """Highest profile height at radius r (for domes)."""
    best = 0.0
    for (r1, z1), (r2, z2) in zip(profile, profile[1:] + profile[:1]):
        if r1 != r2 and (r1 - r) * (r2 - r) <= 0:
            t = (r - r1) / (r2 - r1)
            best = max(best, z1 + (z2 - z1) * t)
    return best


def polar(r, deg, z):
    a = math.radians(deg)
    return (r * math.cos(a), r * math.sin(a), z)


# ---- the foods --------------------------------------------------------------

def apple():
    return item("りんご", ["apple", "fruit", "リンゴ", "red"], group("りんご", [
        sh("Sphere", "からだ", pos=(0, 0, 22.4), size=(26, 1, 1), scale=(1, 1, 0.86),
           color=(0.92, 0.1, 0.12), roughness=0.3, coat=0.6, blend=6),
        sh("Sphere", "くぼみ上", pos=(0, 0, 46), size=(7, 1, 1), op="Subtract", blend=2),
        sh("Sphere", "くぼみ下", pos=(0, 0, 0), size=(5, 1, 1), op="Subtract", blend=2),
        sh("Curve", "へた", path=[(0, 0, 36, 2.0), (0.5, 0, 46, 1.6), (2, 0, 54, 1.3), (4, 0, 58, 1.1)],
           color=(0.4, 0.26, 0.12), blend=1),
        sh("Sphere", "は", pos=(8, 0, 55), size=(7, 1, 1), scale=(1.8, 0.7, 0.25),
           color=(0.3, 0.72, 0.2), blend=1.5, coat=0.3),
    ]))


def banana():
    R, cy = 34.0, -30.0
    ts = [-55, -35, -18, 0, 18, 35, 55]
    rads = [3.2, 5.5, 6.6, 7.0, 6.6, 5.5, 3.2]
    path = []
    for t, r in zip(ts, rads):
        a = math.radians(t)
        path.append((R * math.sin(a), cy + R * math.cos(a), r, r))
    tips = [sh("Sphere", "はし%d" % i, pos=(x, y, 3.2), size=(2.4, 1, 1), color=(0.3, 0.2, 0.08),
               blend=1) for i, (x, y, _, _) in enumerate([path[0], path[-1]])]
    return item("バナナ", ["banana", "fruit", "バナナ", "yellow"], group("バナナ", [
        sh("Curve", "からだ", path=path, params=(0, 0.7, 0, 1), color=(1.0, 0.85, 0.2),
           roughness=0.35, coat=0.3, blend=1),
    ] + tips))


def strawberry():
    prof = [(0, 0), (6, 4), (12, 12), (16, 22), (15, 30), (10, 34), (0, 35)]
    shapes = [
        sh("Revolve", "からだ", outline=[list(p) for p in prof], color=(0.9, 0.08, 0.14),
           roughness=0.45, coat=0.3, blend=3),
        sh("Star", "へた", pos=(0, 0, 35.5), size=(11, 2.5, 1), color=(0.2, 0.6, 0.15), blend=1),
        sh("Curve", "くき", path=[(0, 0, 33, 1.2), (2, 0, 40, 1.0), (5, 0, 44, 0.9)],
           color=(0.25, 0.6, 0.2), blend=1),
    ]
    # seeds: rings of small yellow dots on the outside
    for z, n, off in [(8, 3, 0), (15, 4, 30), (22, 4, 15), (29, 3, 45)]:
        r = outer_r(prof, z) * 0.95
        for k in range(n):
            x, y, zz = polar(r, off + 360.0 * k / n - 90, z)
            shapes.append(sh("Sphere", "つぶ", pos=(x, y, zz), size=(1.2, 1, 1), scale=(1, 1, 1.3),
                             color=(1.0, 0.9, 0.4), roughness=0.4, blend=0.3))
    return item("いちご", ["strawberry", "fruit", "いちご", "red"], group("いちご", shapes))


def clip_below(poly, zmax):
    """Sutherland-Hodgman clip of a convex polygon [x, y] to y <= zmax."""
    out = []
    n = len(poly)
    for i in range(n):
        p, q = poly[i], poly[(i + 1) % n]
        pin, qin = p[1] <= zmax, q[1] <= zmax
        if pin:
            out.append(p)
        if pin != qin:
            t = (zmax - p[1]) / (q[1] - p[1])
            out.append([p[0] + (q[0] - p[0]) * t, zmax])
    return out


def onigiri():
    tri = rounded_outline([(0, 30), (-19, 9), (19, 9)], 9)
    band = clip_below(tri, 15)  # the nori covers the bottom of the triangle
    return item("おにぎり", ["onigiri", "rice ball", "おにぎり", "rice", "nori"], group("おにぎり", [
        sh("Extrude", "ごはん", outline=tri, size=(22, 1, 1), rot=(90, 0, 0), round=4,
           color=(0.98, 0.98, 0.95), roughness=0.6, blend=6),
        sh("Extrude", "のり前", pos=(0, -11.6, 0), size=(1.6, 1, 1), rot=(90, 0, 0), outline=band,
           color=(0.05, 0.2, 0.07), roughness=0.6, blend=0),
        sh("Extrude", "のり後", pos=(0, 11.6, 0), size=(1.6, 1, 1), rot=(90, 0, 0), outline=band,
           color=(0.05, 0.2, 0.07), roughness=0.6, blend=0),
    ]))


def donut():
    sprinkle_cols = [(0.3, 0.8, 1.0), (0.5, 1.0, 0.4), (1.0, 1.0, 0.2), (0.6, 0.4, 1.0), (1.0, 1.0, 1.0)]
    shapes = [
        sh("Torus", "ドーナツ", pos=(0, 0, 8), size=(22, 8, 1), color=(0.85, 0.6, 0.32),
           roughness=0.7, blend=6),
        sh("Torus", "アイシング", pos=(0, 0, 12.2), size=(22, 8.5, 1), scale=(1, 1, 0.5),
           color=(1.0, 0.45, 0.7), roughness=0.25, coat=0.5, blend=2),
    ]
    for i in range(8):
        a = 360.0 * i / 8 + 10
        x, y, _ = polar(22, a, 0)
        shapes.append(sh("Capsule", "すぷりんくる", pos=(x, y, 16.8), size=(0.9, 4, 1),
                         rot=(90, 0, a + 90), color=sprinkle_cols[i % len(sprinkle_cols)],
                         roughness=0.4, blend=0.3))
    return item("ドーナツ", ["donut", "doughnut", "ドーナツ", "sweets"], group("ドーナツ", shapes))


def hamburger():
    bun_low = [(0, 0), (24, 0), (30, 2), (32, 6), (31, 10), (26, 13), (0, 14)]
    bun_top = [(0, 0), (30, 0), (31, 2), (33, 6), (32, 11), (28, 16), (20, 19.5), (10, 21), (0, 21.5)]
    shapes = [
        sh("Revolve", "したのパン", outline=[list(p) for p in bun_low], color=(0.9, 0.6, 0.25),
           roughness=0.6, blend=6),
        sh("Cylinder", "パティ", pos=(0, 0, 18), size=(27, 8, 1), color=(0.35, 0.18, 0.08),
           roughness=0.8, blend=4),
        sh("Box", "チーズ", pos=(0, 0, 23.5), size=(46, 46, 2.5), rot=(0, 0, 45), round=3,
           color=(1.0, 0.78, 0.1), roughness=0.3, blend=5),
        sh("Cylinder", "レタス", pos=(0, 0, 26.5), size=(31, 3.5, 1), color=(0.35, 0.75, 0.2),
           roughness=0.7, blend=6),
        sh("Cylinder", "トマト", pos=(0, 0, 30.5), size=(26, 3, 1), color=(0.95, 0.2, 0.15),
           roughness=0.4, blend=5),
        sh("Revolve", "うえのパン", pos=(0, 0, 31), outline=[list(p) for p in bun_top],
           color=(0.9, 0.6, 0.25), roughness=0.6, blend=6),
    ]
    for i in range(7):
        a = 360.0 * i / 7 + 20
        r = 18
        x, y, _ = polar(r, a, 0)
        z = 31 + top_z(bun_top, r) - 1.0
        shapes.append(sh("Sphere", "ごま", pos=(x, y, z), size=(1.1, 1, 1), scale=(1.4, 1, 0.6),
                         color=(1, 0.97, 0.85), roughness=0.5, blend=0.3))
    return item("ハンバーガー", ["hamburger", "burger", "ハンバーガー", "fast food"], group("ハンバーガー", shapes))


def cake():
    cream = [(0, 0), (30, 0), (31, 3), (29, 7), (22, 10), (12, 12.5), (0, 13.5)]
    shapes = [
        sh("Cylinder", "スポンジ", pos=(0, 0, 12), size=(30, 24, 1), color=(1.0, 0.9, 0.6),
           roughness=0.8, blend=3),
        sh("Revolve", "クリーム", pos=(0, 0, 23), outline=[list(p) for p in cream],
           color=(1.0, 0.97, 0.95), roughness=0.3, coat=0.3, blend=3),
        sh("Sphere", "まんなかのいちご", pos=(0, 0, 36.5), size=(7, 1, 1), scale=(1, 1, 1.1),
           color=(0.9, 0.08, 0.14), roughness=0.4, coat=0.3, blend=2),
    ]
    for i in range(6):
        a = 60.0 * i
        x, y, _ = polar(20, a, 0)
        shapes.append(sh("Sphere", "いちご", pos=(x, y, 33), size=(4.5, 1, 1), scale=(1, 1, 1.2),
                         color=(0.9, 0.08, 0.14), roughness=0.4, coat=0.3, blend=1))
    for i in range(4):
        x, y, _ = polar(22, 30 + 90.0 * i, 0)
        shapes.append(sh("Sphere", "クリームのかたまり", pos=(x, y, 33.5), size=(3, 1, 1),
                         color=(1, 1, 1), roughness=0.3, blend=2))
    return item("ケーキ", ["cake", "shortcake", "ケーキ", "strawberry", "sweets"], group("ケーキ", shapes))


def ice_cream():
    shapes = [
        sh("Revolve", "コーン", outline=[[0, 0], [18, 32], [0, 32]], color=(0.9, 0.6, 0.3),
           roughness=0.8, blend=3),
        sh("Sphere", "ピンク", pos=(0, 0, 40), size=(18, 1, 1), scale=(1, 1, 0.85),
           color=(1.0, 0.6, 0.75), roughness=0.3, coat=0.4, blend=4),
        sh("Sphere", "ミント", pos=(0, 0, 57), size=(15, 1, 1), scale=(1, 1, 0.85),
           color=(0.6, 0.95, 0.8), roughness=0.3, coat=0.4, blend=4),
        sh("Sphere", "さくらんぼ", pos=(0, 0, 73), size=(3.5, 1, 1), color=(0.9, 0.05, 0.1),
           roughness=0.3, coat=0.5, blend=1.5),
    ]
    for a, e in [(-60, 25), (20, 40), (120, 20), (200, 35)]:
        x, y, _ = polar(18 * math.cos(math.radians(e)), a, 0)
        z = 40 + 15.3 * math.sin(math.radians(e))
        shapes.append(sh("Sphere", "チョコ", pos=(x, y, z), size=(1.3, 1, 1),
                         color=(0.25, 0.12, 0.05), roughness=0.4, blend=0.5))
    return item("アイスクリーム", ["ice cream", "cone", "アイスクリーム", "sweets"], group("アイスクリーム", shapes))


def pizza():
    slice_a = dict(params=(60, 0, 0, 0), rot=(0, 0, -120))
    shapes = [
        sh("Revolve", "生地", outline=[[0, 0], [35, 0], [37, 3], [38, 8], [37, 12], [35, 12], [35, 7], [0, 7]],
           color=(0.9, 0.6, 0.25), roughness=0.6, blend=3, **slice_a),
        sh("Revolve", "チーズ", outline=[[0, 6.5], [33, 6.5], [34, 8.5], [33, 10.5], [0, 10.5]],
           color=(1.0, 0.85, 0.3), roughness=0.35, blend=6, **slice_a),
    ]
    for r, a in [(20, -100), (24, -80), (10, -90)]:
        x, y, _ = polar(r, a, 0)
        shapes.append(sh("Cylinder", "ペパロニ", pos=(x, y, 11.4), size=(5, 1.8, 1),
                         color=(0.8, 0.12, 0.08), roughness=0.4, blend=0.5))
    for x, y in [(-6, -14), (7, -28)]:
        shapes.append(sh("Sphere", "バジル", pos=(x, y, 10.6), size=(3, 1, 1), scale=(1.6, 0.8, 0.2),
                         color=(0.25, 0.7, 0.2), roughness=0.5, blend=0.5))
    return item("ピザ", ["pizza", "slice", "ピザ", "food"], group("ピザ", shapes))


def fried_egg():
    return item("めだまやき", ["fried egg", "egg", "たまごやき", "めだまやき", "breakfast"], group("めだまやき", [
        sh("Cylinder", "おさら", pos=(0, 0, 1.5), size=(38, 3, 1), color=(0.95, 0.95, 0.97),
           roughness=0.2, coat=0.5, blend=0),
        sh("Sphere", "しろみ", pos=(0, 0, 7.6), size=(22, 1, 1), scale=(1.15, 1, 0.22),
           color=(1, 1, 0.98), roughness=0.25, translucency=0.1, blend=6),
        sh("Sphere", "きみ", pos=(3, 0, 14.5), size=(10, 1, 1), scale=(1, 1, 0.55),
           color=(1.0, 0.62, 0.05), roughness=0.2, coat=0.5, blend=1.5),
    ]))


def sushi():
    def nigiri(x, topping, topcol):
        return [
            sh("Box", "ごはん", pos=(x, 0, 10), size=(34, 22, 14), round=6,
               color=(0.98, 0.98, 0.95), roughness=0.6, blend=3),
            sh("Box", "ネタ", pos=(x, 0, 19.5), size=(36, 24, 5), round=3,
               color=topcol, roughness=0.3, coat=0.4, translucency=0.15, blend=1.5),
            sh("Box", "のり", pos=(x, 0, 10), size=(8, 24, 15), round=1,
               color=(0.06, 0.2, 0.08), roughness=0.6, blend=0.5),
        ]
    return item("すし", ["sushi", "nigiri", "すし", "japanese"], group("すし", nigiri(-14, "salmon", (1.0, 0.5, 0.35))
                                                                      + nigiri(20, "tuna", (0.85, 0.1, 0.2))))


def ramen():
    bowl = [(0, 0), (16, 0), (20, 4), (28, 12), (34, 22), (37, 30), (34, 30), (33, 27), (27, 18), (16, 9), (0, 8)]
    noodles1 = [(-26, -8, 27, 1.1), (-16, 4, 30, 1.1), (-4, -6, 31, 1.1), (8, 6, 29, 1.1),
                (20, -4, 30, 1.1), (28, 6, 27, 1.1)]
    noodles2 = [(-22, 6, 28, 1.1), (-8, -10, 30, 1.1), (6, 4, 31, 1.1), (18, -8, 29, 1.1), (26, 2, 28, 1.1)]
    return item("ラーメン", ["ramen", "noodles", "ラーメン", "japanese"], group("ラーメン", [
        sh("Revolve", "どんぶり", outline=[list(p) for p in bowl], color=(0.97, 0.96, 0.93),
           roughness=0.25, coat=0.5, blend=2),
        sh("Cylinder", "スープ", pos=(0, 0, 22), size=(33, 8, 1), color=(0.85, 0.45, 0.1),
           roughness=0.1, coat=0.8, translucency=0.35, blend=0),
        sh("Curve", "めん1", path=noodles1, color=(1.0, 0.85, 0.45), roughness=0.5, coat=0.2, blend=0.8),
        sh("Curve", "めん2", path=noodles2, color=(1.0, 0.85, 0.45), roughness=0.5, coat=0.2, blend=0.8),
        sh("Cylinder", "チャーシュー1", pos=(-10, 6, 30), size=(9, 3, 1), color=(0.92, 0.6, 0.45),
           roughness=0.5, blend=2),
        sh("Cylinder", "チャーシュー2", pos=(12, -6, 30), size=(9, 3, 1), color=(0.92, 0.6, 0.45),
           roughness=0.5, blend=2),
        sh("Sphere", "たまご白", pos=(14, 4, 30), size=(9, 1, 1), scale=(1, 1, 0.6),
           color=(1, 0.98, 0.95), roughness=0.3, blend=1.5),
        sh("Sphere", "たまご黄", pos=(14, 4, 33), size=(5, 1, 1), scale=(1, 1, 0.5),
           color=(1, 0.55, 0.1), roughness=0.2, blend=0.5),
        sh("Box", "のり", pos=(-16, 20, 38), size=(12, 1.2, 18), rot=(0, 0, -20),
           color=(0.05, 0.2, 0.08), roughness=0.5, blend=0.5),
        sh("Sphere", "ねぎ1", pos=(-6, -10, 33), size=(1.8, 1, 1), color=(0.35, 0.75, 0.25), blend=0.5),
        sh("Sphere", "ねぎ2", pos=(4, 2, 34), size=(1.8, 1, 1), color=(0.35, 0.75, 0.25), blend=0.5),
        sh("Sphere", "ねぎ3", pos=(18, 0, 33), size=(1.8, 1, 1), color=(0.35, 0.75, 0.25), blend=0.5),
    ]))


def bread():
    slice_outline = [[-14, 0], [14, 0], [14, 18], [10, 24], [0, 27], [-10, 24], [-14, 18]]
    shapes = [
        sh("Box", "食パン", pos=(0, 6, 15), size=(62, 30, 30), round=10,
           color=(0.85, 0.55, 0.25), roughness=0.6, blend=5),
    ]
    for x in (-18, 0, 18):
        shapes.append(sh("Box", "切れ目", pos=(x, 6, 31), size=(1.8, 30, 4), op="Subtract", blend=0.5))
    shapes.append(sh("Extrude", "スライス", outline=slice_outline, size=(6, 1, 1), rot=(90, 0, 0),
                     pos=(0, -26, 0), color=(1, 0.94, 0.8), roughness=0.7, blend=2))
    return item("パン", ["bread", "loaf", "パン", "bakery"], group("パン", shapes))


def carrot():
    body = [(0, 0), (4, 4), (9, 14), (12, 26), (13, 38), (12, 48), (10, 53), (0, 55)]
    shapes = [
        sh("Revolve", "にんじん", outline=[list(p) for p in body], color=(1.0, 0.5, 0.1),
           roughness=0.5, blend=2),
    ]
    for z in (20, 32, 44):  # thin grooves around the carrot
        shapes.append(sh("Torus", "すじ", pos=(0, 0, z), size=(outer_r(body, z), 0.8, 1), op="Subtract",
                         blend=0))
    shapes.append(sh("Curve", "くき", path=[(0, 0, 50, 1.8), (2, 1, 60, 1.6), (4, 2, 68, 1.3)],
                     color=(0.25, 0.65, 0.2), blend=1.5))
    for x, ay, az in [(-5, -35, 0), (5, 35, 0), (0, 0, 0)]:
        shapes.append(sh("Capsule", "は", pos=(x, 0, 64), size=(2.2, 12, 1), rot=(0, ay, az),
                         color=(0.25, 0.65, 0.2), roughness=0.5, blend=1.5))
    return item("にんじん", ["carrot", "vegetable", "にんじん", "orange"], group("にんじん", shapes))


def watermelon():
    rind = half_disc(40) + list(reversed(half_disc(36)))  # green band, 4 mm wide
    shapes = [
        sh("Extrude", "みどりのかわ", outline=rind, size=(6, 1, 1), rot=(90, 0, 0),
           color=(0.15, 0.55, 0.2), roughness=0.5, blend=0),
        sh("Extrude", "あか", outline=half_disc(36), size=(5.2, 1, 1), rot=(90, 0, 0),
           color=(1.0, 0.18, 0.25), roughness=0.35, blend=0),
    ]
    for x, z in [(-18, 8), (-6, 14), (8, 12), (18, 6), (-12, 24), (4, 26), (16, 22), (-22, 18), (22, 18), (0, 16)]:
        shapes.append(sh("Sphere", "たね", pos=(x, -3.6, z), size=(1.3, 1, 1), scale=(0.7, 0.35, 1.1),
                         color=(0.1, 0.08, 0.06), roughness=0.4, blend=0.2))
    return item("スイカ", ["watermelon", "fruit", "スイカ", "summer"], group("スイカ", shapes))


def soft_serve():
    # a tower of swirl rings: the profile bulges out at each ring and curls over at the top
    swirl = [(0, 28), (15, 28), (16, 33), (13, 38), (15, 43), (12, 48), (14, 53), (10, 58), (11, 62),
             (7, 66), (0, 67)]
    shapes = [
        sh("Revolve", "コーン", outline=[[0, 0], [15, 30], [0, 30]], color=(0.9, 0.6, 0.3),
           roughness=0.8, blend=3),
        sh("Revolve", "うずまき", outline=[list(p) for p in swirl], color=(1, 0.98, 0.92),
           roughness=0.35, coat=0.2, blend=1.5),
        sh("Box", "チョコのスティック", pos=(8, 0, 58), size=(2, 14, 10), rot=(0, 0, 30), round=1,
           color=(0.45, 0.22, 0.08), roughness=0.5, blend=1),
    ]
    return item("ソフトクリーム", ["soft serve", "soft ice cream", "ソフトクリーム", "sweets"],
                group("ソフトクリーム", shapes))


ITEMS = [apple(), banana(), strawberry(), onigiri(), donut(), hamburger(), cake(), ice_cream(),
         pizza(), fried_egg(), sushi(), ramen(), bread(), carrot(), watermelon(), soft_serve()]

if __name__ == "__main__":
    write_category("food.json", "たべもの", ITEMS)
    preview(ITEMS, outdir="/tmp/lib_preview_food")
