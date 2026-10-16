"""Category どうぶつ: 14 cute toy-like animals, 30-70 mm, standing on z = 0 and facing -Y.

Run from this folder:  python3 animals.py   (writes ../animals.json, renders previews)
"""
import math

from lib import *

DARK = (0.10, 0.08, 0.07)
WHITE = (0.97, 0.96, 0.93)
PINK = (0.98, 0.66, 0.70)


def ball(name, pos, r, color, blend=6.0, scale=(1, 1, 1), rough=0.55, **kw):
    """A sphere of radius r. scale stretches it into an ellipsoid (bodies, ears, fins)."""
    return shape("Sphere", name, pos=pos, size=(r, 1, 1), scale=scale, color=color,
                 blend=blend, roughness=rough, **kw)


def eye(name, pos, r=1.4):
    """A tiny glossy dark eye with a hard join, so it stays crisp on the head."""
    return ball(name, pos, r, DARK, blend=0.6, rough=0.2, coat=0.6)


def tube(name, pts, color, blend=4.0, **kw):
    """A tube through (x, y, z, radius) nodes: tails, trunks, necks and bent limbs."""
    return shape("Curve", name, pos=(0, 0, 0), size=(1, 1, 1), path=[list(p) for p in pts],
                 color=color, blend=blend, roughness=0.6, **kw)


def stick(name, x, y, z0, z1, r, color, blend=4.0):
    """An upright leg: a capsule whose rounded ends sit exactly on z0 (ground) and z1."""
    length = max(z1 - z0 - 2 * r, 0.5)
    return shape("Capsule", name, pos=(x, y, (z0 + z1) / 2), size=(r, length, 1),
                 color=color, blend=blend, roughness=0.6)


def on_ellipsoid(c, radii, d, out=0.0):
    """A point on an ellipsoid (centre c, radii) in direction d. out > 0 pushes it outward."""
    k = 1.0 / math.sqrt(sum((d[i] / radii[i]) ** 2 for i in range(3)))
    return tuple(c[i] + d[i] * k * (1.0 + out) for i in range(3))


def front_of(c, radii, x, z, out=0.0):
    """A point on the front (-Y side) of an ellipsoid, at sideways x and height z (absolute mm)."""
    return on_ellipsoid(c, radii, (x - c[0], -1.0, z - c[2]), out)


def spot(name, c, radii, d, color, r):
    """A flat patch stuck on a body's surface (half sunk in, so it reads as a spot)."""
    return shape("Sphere", name, pos=on_ellipsoid(c, radii, d, 0.0), size=(r, 1, 1),
                 scale=(1, 1, 0.45), color=color, blend=0.8, roughness=0.6)


def both(fn):
    """A left and a right part: fn(sx, side) with sx = +1 / -1 and side = the name suffix."""
    return [fn(1, "右"), fn(-1, "左")]


# ---------------------------------------------------------------- the animals

def dog():
    c, ear, cream = (0.84, 0.60, 0.34), (0.42, 0.25, 0.12), (0.97, 0.88, 0.70)
    head, R = (0, -17, 31), 9.5
    snout, sr = (0, -24, 27), (5.0, 5.5, 4.25)
    s = [ball("からだ", (0, 4, 21), 11, c, scale=(1, 1.45, 0.92))]
    s += both(lambda sx, side: stick("まえあし" + side, 6 * sx, -8, 0, 24, 3.6, c))
    s += both(lambda sx, side: stick("うしろあし" + side, 6 * sx, 12, 0, 24, 3.6, c))
    s += [ball("あたま", head, R, c),
          ball("はな", snout, 5.0, cream, blend=3, scale=(1, 1.1, 0.85)),
          ball("はなさき", on_ellipsoid(snout, sr, (0, -1, 0.2)), 1.9, DARK, blend=0.8)]
    s += both(lambda sx, side: eye("め" + side, front_of(head, (R, R, R), 3.8 * sx, 33.5), 1.5))
    s += both(lambda sx, side: ball("みみ" + side, (8.6 * sx, -14, 30), 4.8, ear, blend=3,
                                    scale=(0.6, 1.1, 2.0)))
    s += [tube("しっぽ", [(0, 15, 23, 2.6), (2, 19, 28, 2.2), (4, 20.5, 35, 1.8)], c, blend=2.5)]
    return s


def cat():
    c, white, ear = (0.95, 0.60, 0.28), (0.98, 0.94, 0.88), (0.90, 0.48, 0.30)
    head, R = (0, -14, 30), 9.0
    muzzle, mr = (0, -21.5, 27), (4.5, 2.9, 2.9)
    s = [ball("からだ", (0, 4, 19), 10, c, scale=(1, 1.35, 0.95))]
    s += both(lambda sx, side: stick("まえあし" + side, 5 * sx, -6, 0, 17, 3.0, c))
    s += both(lambda sx, side: stick("うしろあし" + side, 5 * sx, 11, 0, 17, 3.0, c))
    s += [ball("あたま", head, R, c),
          ball("はな", muzzle, 3.6, white, blend=2, scale=(1.25, 0.8, 0.8)),
          ball("はなさき", on_ellipsoid(muzzle, mr, (0, -1, 0.3)), 1.2, PINK, blend=0.5)]
    s += both(lambda sx, side: eye("め" + side, front_of(head, (R, R, R), 3.5 * sx, 32.5), 1.5))
    s += both(lambda sx, side: ball("みみ" + side, (5.8 * sx, -12, 38.5), 3.8, ear, blend=2,
                                    scale=(0.7, 0.55, 1.25)))
    s += [tube("しっぽ", [(0, 12, 12, 1.8), (3, 17, 11, 1.6), (5, 19, 17, 1.5), (6, 19.5, 23, 1.4)],
               c, blend=2.0)]
    return s


def rabbit():
    c, pink = WHITE, PINK
    head, R = (0, -6, 30), 9.0
    s = [ball("からだ", (0, 4, 14), 11.5, c, scale=(0.95, 1.1, 1.0))]
    s += both(lambda sx, side: ball("あし" + side, (5.5 * sx, -8, 2.4), 4, c, blend=2,
                                    scale=(0.9, 1.6, 0.6)))
    s += [ball("あたま", head, R, c)]
    s += both(lambda sx, side: tube("みみ" + side, [(3.4 * sx, -2, 36, 3.0), (4.4 * sx, -1.5, 50, 2.7),
                                                   (5.2 * sx, 0.5, 61, 2.3)], c, blend=3))
    s += both(lambda sx, side: tube("みみの中" + side, [(3.4 * sx, -4.0, 38, 1.3), (4.2 * sx, -3.2, 50, 1.2),
                                                       (5.0 * sx, -1.8, 59, 1.0)], pink, blend=0.8))
    s += both(lambda sx, side: eye("め" + side, front_of(head, (R, R, R), 3.6 * sx, 31.5), 1.4))
    s += [ball("はな", front_of(head, (R, R, R), 0, 27.5), 1.1, pink, blend=0.6),
          ball("しっぽ", (0, 15, 9), 4, c, blend=2)]
    return s


def bear():
    c, muzzle, nose = (0.52, 0.33, 0.18), (0.90, 0.74, 0.56), DARK
    head, R = (0, -6, 40), 11.0
    mc, mr = (0, -14.5, 34.5), (6.05, 4.4, 4.4)
    s = [ball("からだ", (0, 4, 22), 13, c)]
    s += both(lambda sx, side: ball("まえあし" + side, (7.5 * sx, -6, 3.75), 5, c, blend=3,
                                    scale=(1, 1.2, 0.75)))
    s += both(lambda sx, side: ball("うしろあし" + side, (7.5 * sx, 8, 3.75), 5, c, blend=3,
                                    scale=(1, 1.2, 0.75)))
    s += both(lambda sx, side: tube("うで" + side, [(10.5 * sx, -3, 26, 4.2), (13.5 * sx, -5, 18, 4.0),
                                                   (12.5 * sx, -6, 11, 4.0)], c, blend=4))
    s += [ball("あたま", head, R, c)]
    s += both(lambda sx, side: ball("みみ" + side, (8.0 * sx, -6, 49), 4.2, c, blend=2.5))
    s += [ball("はな", mc, 5.5, muzzle, blend=2, scale=(1.1, 0.8, 0.8)),
          ball("はなさき", on_ellipsoid(mc, mr, (0, -1, 0.25)), 2.0, nose, blend=0.8)]
    s += both(lambda sx, side: eye("め" + side, front_of(head, (R, R, R), 4.6 * sx, 44.5), 1.6))
    return s


def elephant():
    c, ear, ivory = (0.62, 0.65, 0.72), (0.74, 0.60, 0.64), (0.97, 0.94, 0.86)
    head, R = (0, -14, 40), 12.0
    body = (0, 6, 27)
    s = [ball("からだ", body, 14, c, scale=(1, 1.3, 0.95))]
    s += both(lambda sx, side: stick("あし" + side, 8 * sx, -7, 0, 18, 5.0, c))
    s += both(lambda sx, side: stick("うしろあし" + side, 8 * sx, 14, 0, 18, 5.0, c))
    s += [ball("あたま", head, R, c, blend=8)]
    s += both(lambda sx, side: ball("みみ" + side, (10.5 * sx, -9, 40), 10, ear, blend=2,
                                    scale=(0.22, 1.0, 1.1)))
    s += [tube("はな", [(0, -21, 38, 6.0), (0, -28, 31, 5.2), (0, -31, 23, 4.4), (0, -29.5, 15, 3.8)],
               c, blend=4)]
    s += both(lambda sx, side: tube("きば" + side, [(4.8 * sx, -23, 35, 1.2), (6.8 * sx, -30, 31, 0.6)],
                                    ivory, blend=0.8))
    s += both(lambda sx, side: eye("め" + side, front_of(head, (R, R, R), 5.5 * sx, 45), 1.3))
    s += [tube("しっぽ", [(0, 15, 26, 1.5), (1.2, 19, 20, 1.3), (2, 20, 12, 1.0)], c, blend=1.5)]
    return s


def giraffe():
    c, spotc, horn, muzzle = (0.97, 0.79, 0.37), (0.62, 0.37, 0.15), (0.35, 0.22, 0.12), (0.98, 0.92, 0.75)
    body, br = (0, 2, 28), (10.45, 16.5, 9.35)
    head, hr = (0, -14, 61), (6.4, 8.6, 6.0)
    mc, mr = (0, -20.5, 56.5), (3.8, 3.3, 3.1)
    s = [ball("からだ", body, 11, c, scale=(0.95, 1.5, 0.85))]
    s += both(lambda sx, side: stick("あし" + side, 5 * sx, -7, 0, 21, 2.2, c, blend=3))
    s += both(lambda sx, side: stick("うしろあし" + side, 5 * sx, 10, 0, 21, 2.2, c, blend=3))
    s += [tube("くび", [(0, -4, 32, 5.0), (0, -7, 43, 4.6), (0, -10, 52, 4.4), (0, -12, 58, 4.2)], c, blend=4),
          ball("あたま", head, 6.4, c, blend=4, scale=(1, 1.35, 0.95))]
    for i, d in enumerate([(0.9, -0.2, 0.4), (-0.8, 0.5, 0.45), (0.3, 0.9, 0.8), (-0.4, -0.7, 0.7)]):
        s.append(spot("もよう%d" % (i + 1), body, br, d, spotc, 3.0))
    s += both(lambda sx, side: tube("つの" + side, [(2.4 * sx, -14, 66, 0.9), (2.8 * sx, -14.5, 69, 0.9)],
                                    horn, blend=0.6))
    s += both(lambda sx, side: ball("みみ" + side, (6.6 * sx, -12, 62), 2.4, c, blend=1.5,
                                    scale=(1.6, 0.6, 0.8)))
    s += [ball("はな", mc, 3.2, muzzle, blend=2, scale=(1.1, 0.95, 0.9)),
          ball("はなさき", on_ellipsoid(mc, mr, (0, -1, 0.1)), 1.4, DARK, blend=0.5)]
    s += both(lambda sx, side: eye("め" + side, front_of(head, hr, 3.4 * sx, 63), 1.2))
    s += [tube("しっぽ", [(0, 13, 28, 1.0), (0, 16, 22, 0.9), (0, 17, 16, 0.8)], c, blend=1.0),
          ball("しっぽの房", (0, 17, 14), 1.6, horn, blend=0.5)]
    return s


def pig():
    c, snout_c, ear = (0.98, 0.72, 0.74), (0.92, 0.52, 0.56), (0.92, 0.56, 0.60)
    head, R = (0, -14, 25), 10.0
    snout, sr = (0, -22.5, 23), (7.3, 3.9, 5.2)
    s = [ball("からだ", (0, 4, 17), 12, c, scale=(1.1, 1.25, 0.95))]
    s += both(lambda sx, side: stick("まえあし" + side, 7 * sx, -7, 0, 16, 3.8, c))
    s += both(lambda sx, side: stick("うしろあし" + side, 7 * sx, 10, 0, 16, 3.8, c))
    s += [ball("あたま", head, R, c, blend=6),
          ball("はな", snout, 5.2, snout_c, blend=3, scale=(1.4, 0.75, 1.0))]
    s += both(lambda sx, side: ball("はなあな" + side, on_ellipsoid(snout, sr, (0.35 * sx, -1, 0), 0.05),
                                    0.9, DARK, blend=0.4))
    s += both(lambda sx, side: eye("め" + side, front_of(head, (R, R, R), 4.2 * sx, 29.5), 1.3))
    s += both(lambda sx, side: ball("みみ" + side, (8.2 * sx, -12, 34), 4, ear, blend=2,
                                    scale=(0.5, 0.9, 1.2)))
    s += [tube("しっぽ", [(0, 12, 20, 1.1), (3, 15, 21, 1.0), (4.5, 16.5, 24, 0.9),
                          (3.5, 16.5, 26.5, 0.9), (1.8, 15.5, 25.5, 0.9)], c, blend=1.5)]
    return s


def chick():
    c, beak, feet, blush = (1.0, 0.85, 0.22), (1.0, 0.55, 0.12), (0.98, 0.50, 0.10), (1.0, 0.55, 0.55)
    head, R = (0, -2, 33), 11.0
    s = [ball("からだ", (0, 0, 14), 14.5, c, blend=6, scale=(1, 1, 0.95))]
    s += [ball("あたま", head, R, c, blend=7),
          tube("あほげ", [(0, 0, 43, 2.6), (1.5, 1, 47, 1.5), (3, 1.5, 47.5, 0.8)], c, blend=1.5)]
    s += both(lambda sx, side: ball("つばさ" + side, (14.5 * sx, 1, 15), 5.5, c, blend=3,
                                    scale=(0.4, 1.0, 1.4)))
    s += [ball("くちばし", (0, -14.3, 30), 3.0, beak, blend=1.2, scale=(1.0, 1.3, 0.6))]
    s += both(lambda sx, side: eye("め" + side, front_of(head, (R, R, R), 4.5 * sx, 35), 1.5))
    s += both(lambda sx, side: ball("ほっぺ" + side, front_of(head, (R, R, R), 6.5 * sx, 28, 0.1), 2.2,
                                    blush, blend=1.0, scale=(1, 0.6, 0.6)))
    s += both(lambda sx, side: ball("あし" + side, (4.5 * sx, -4, 1.26), 2.8, feet, blend=1.5,
                                    scale=(1.2, 1.8, 0.45)))
    return s


def penguin():
    c, white, orange = (0.14, 0.16, 0.22), WHITE, (1.0, 0.60, 0.15)
    head, R = (0, -1, 46), 9.5
    s = [ball("からだ", (0, 0, 22), 14, c, scale=(0.9, 0.95, 1.4)),
         ball("おなか", (0, -9, 20), 10, white, blend=1.5, scale=(0.75, 0.55, 1.2))]
    s += both(lambda sx, side: ball("あし" + side, (5 * sx, -4, 1.4), 3.5, orange, blend=1.5,
                                    scale=(1, 1.6, 0.4)))
    s += [ball("あたま", head, R, c, blend=6),
          ball("くちばし", (0, -10.5, 43.5), 3.0, orange, blend=1.0, scale=(0.7, 1.5, 0.55))]
    s += both(lambda sx, side: ball("め のまわり" + side, front_of(head, (R, R, R), 3.8 * sx, 48, 0.05),
                                    2.6, white, blend=0.8, scale=(1, 0.5, 1)))
    s += both(lambda sx, side: eye("め" + side, front_of(head, (R, R, R), 3.8 * sx, 48.5, 0.25), 1.2))
    s += both(lambda sx, side: tube("はね" + side, [(11 * sx, 0, 31, 3.6), (14.5 * sx, 1, 20, 2.6),
                                                   (13.5 * sx, 1, 12, 2.0)], c, blend=3))
    return s


def fish():
    c, fin, mouth = (0.98, 0.45, 0.18), (1.0, 0.72, 0.22), (0.85, 0.20, 0.22)
    body, br = (0, 0, 13), (11.7, 26.0, 13.0)
    s = [ball("からだ", body, 13, c, blend=6, scale=(0.9, 2.0, 1.0)),
         ball("しっぽ", (0, 30, 13), 10, fin, blend=3, scale=(0.2, 0.9, 1.3)),
         ball("せびれ", (0, 2, 28), 7, fin, blend=3, scale=(0.25, 1.2, 0.9))]
    s += both(lambda sx, side: ball("むなびれ" + side, (11 * sx, -8, 8), 5, fin, blend=3,
                                    scale=(0.4, 0.9, 0.8)))
    s += both(lambda sx, side: ball("め しろめ" + side, on_ellipsoid(body, br, (sx, -2.5, 0.4), 0.15),
                                    3.6, WHITE, blend=0.6))
    s += both(lambda sx, side: eye("め" + side, on_ellipsoid(body, br, (sx, -2.5, 0.4), 0.35), 1.6))
    s += [ball("くち", (0, -26, 11), 2.8, mouth, blend=1.0, scale=(1.2, 0.6, 0.7))]
    return s


def turtle():
    c, plate, skin = (0.32, 0.56, 0.28), (0.20, 0.38, 0.18), (0.62, 0.78, 0.42)
    shell_c, shell_r = (0, 0, 15), (19.8, 25.2, 10.8)
    head, hr = (0, -27, 9), (6.6, 7.6, 6.2)
    s = [ball("こうら", shell_c, 18, c, blend=6, scale=(1.1, 1.4, 0.6))]
    for i, d in enumerate([(0, 0, 1), (-0.55, -0.45, 0.7), (0.55, -0.45, 0.7),
                           (-0.5, 0.5, 0.7), (0.5, 0.5, 0.7)]):
        s.append(spot("もよう%d" % (i + 1), shell_c, shell_r, d, plate, 4.5))
    s += both(lambda sx, side: stick("あし" + side, 14 * sx, -11, 0, 14, 4.6, skin))
    s += both(lambda sx, side: stick("うしろあし" + side, 14 * sx, 12, 0, 14, 4.6, skin))
    s += [ball("あたま", head, 6.8, skin, blend=4, scale=(0.95, 1.1, 0.9))]
    s += both(lambda sx, side: eye("め" + side, on_ellipsoid(head, hr, (3.4 * sx, -1, 0.6), 0.05), 1.3))
    s += [tube("しっぽ", [(0, 22, 14, 1.6), (0, 26, 12, 1.3), (0, 28, 10, 1.0)], skin, blend=1.5)]
    return s


def frog():
    c, belly = (0.32, 0.70, 0.26), (0.80, 0.88, 0.52)
    head, hr = (0, -6, 15), (10.0, 8.0, 6.4)
    s = [ball("からだ", (0, 2, 10), 13, c, blend=6, scale=(1.2, 1.1, 0.7)),
         ball("あたま", head, 8, c, blend=6, scale=(1.25, 1.0, 0.8))]
    s += both(lambda sx, side: ball("め ふくらみ" + side, (7.5 * sx, -9.5, 20.5), 4.2, c, blend=1.5))
    s += both(lambda sx, side: ball("め しろ" + side, (7.5 * sx, -13.2, 21), 2.8, WHITE, blend=0.8))
    s += both(lambda sx, side: eye("め" + side, (7.5 * sx, -15.3, 21.3), 1.5))
    s += both(lambda sx, side: tube("うしろあし" + side, [(10 * sx, 4, 8, 3.2), (17 * sx, -1, 3.5, 2.6),
                                                       (12 * sx, -7, 2.2, 1.8)], c, blend=3))
    s += both(lambda sx, side: tube("まえあし" + side, [(10 * sx, -8, 8, 1.6), (13 * sx, -13, 2.3, 1.3)],
                                    c, blend=2))
    s += [tube("くち", [(-6, -13.2, 11.5, 0.5), (0, -14, 10.3, 0.6), (6, -13.2, 11.5, 0.5)], DARK, blend=0.4)]
    return s


def lion():
    c, mane, muzzle, nose = (0.92, 0.66, 0.30), (0.56, 0.32, 0.12), (0.98, 0.88, 0.65), (0.25, 0.12, 0.10)
    head, R = (0, -14, 38), 9.0
    s = [ball("からだ", (0, 5, 21), 12, c, scale=(1, 1.3, 0.9))]
    s += both(lambda sx, side: stick("まえあし" + side, 6 * sx, -7, 0, 20, 3.4, c, blend=4))
    s += both(lambda sx, side: stick("うしろあし" + side, 6 * sx, 13, 0, 20, 3.4, c, blend=4))
    # the mane: a ring of puffs around the head, sitting behind the face
    for i in range(8):
        a = math.radians(45 * i)
        s.append(ball("たてがみ%d" % (i + 1), (10.5 * math.cos(a), -10, 38 + 10.5 * math.sin(a)), 5.5,
                      mane, blend=3))
    s += [ball("あたま", head, R, c, blend=3),
          ball("はな", (0, -22.5, 33), 5, muzzle, blend=1.5, scale=(1.1, 0.9, 0.8))]
    s += [ball("はなさき", on_ellipsoid((0, -22.5, 33), (5.5, 4.5, 4.0), (0, -1, 0.3)), 1.8, nose, blend=0.6)]
    s += both(lambda sx, side: eye("め" + side, front_of(head, (R, R, R), 3.6 * sx, 41), 1.4))
    s += [tube("しっぽ", [(0, 14, 24, 1.6), (2, 19, 20, 1.4), (3, 22, 13, 1.2), (3, 22, 8, 1.0)], c, blend=2),
          ball("しっぽの房", (3, 22, 6.0), 2.4, mane, blend=1.0)]
    return s


def sheep():
    wool, face = (0.97, 0.96, 0.92), (0.26, 0.22, 0.22)
    head, hr = (0, -14, 30), (7.1, 8.25, 7.5)
    s = [ball("からだ", (0, 4, 22), 11, wool, blend=6, scale=(1, 1.2, 0.9)),
         ball("もこもこ1", (-7, -4, 27), 6.5, wool, blend=6),
         ball("もこもこ2", (7, -4, 27), 6.5, wool, blend=6),
         ball("もこもこ3", (-7, 9, 27), 6.5, wool, blend=6),
         ball("もこもこ4", (7, 9, 27), 6.5, wool, blend=6),
         ball("もこもこ5", (0, 2, 33), 6.5, wool, blend=6)]
    s += both(lambda sx, side: stick("あし" + side, 5 * sx, -7, 0, 20, 2.0, face, blend=3))
    s += both(lambda sx, side: stick("うしろあし" + side, 5 * sx, 10, 0, 20, 2.0, face, blend=3))
    s += [ball("あたま", head, 7.5, face, blend=5, scale=(0.95, 1.1, 1.0)),
          ball("まえがみ", (0, -18.5, 36.5), 3.6, wool, blend=2)]
    s += both(lambda sx, side: ball("みみ" + side, (8.5 * sx, -12, 31), 3.2, face, blend=2,
                                    scale=(1.8, 0.6, 0.8)))
    s += both(lambda sx, side: eye("め" + side, on_ellipsoid(head, hr, (3.4 * sx, -1, 2.5), 0.05), 1.2))
    return s


ANIMALS = [
    ("いぬ", ["dog", "犬", "イヌ", "pet", "animal"], dog),
    ("ねこ", ["cat", "猫", "ネコ", "pet", "animal"], cat),
    ("うさぎ", ["rabbit", "bunny", "兎", "ウサギ", "animal"], rabbit),
    ("くま", ["bear", "熊", "クマ", "animal"], bear),
    ("ぞう", ["elephant", "象", "ゾウ", "animal"], elephant),
    ("きりん", ["giraffe", "麒麟", "キリン", "animal"], giraffe),
    ("ぶた", ["pig", "豚", "ブタ", "animal"], pig),
    ("ひよこ", ["chick", "chicken", "雛", "ヒヨコ", "bird", "animal"], chick),
    ("ペンギン", ["penguin", "企鵝", "bird", "animal"], penguin),
    ("さかな", ["fish", "魚", "サカナ", "animal"], fish),
    ("かめ", ["turtle", "tortoise", "亀", "カメ", "animal"], turtle),
    ("カエル", ["frog", "蛙", "animal"], frog),
    ("ライオン", ["lion", "獅子", "ライオン", "animal"], lion),
    ("ひつじ", ["sheep", "羊", "ヒツジ", "animal"], sheep),
]

items = [item(name, tags, group(name, make())) for name, tags, make in ANIMALS]

if __name__ == "__main__":
    for it in items:
        print(f"{len(it['group']['shapes']):2d} shapes  {it['name']}")
    write_category("animals.json", "どうぶつ", items)
    preview(items, outdir="/tmp/lib_preview_animals")
