"""Category デフォルメ: 14 chibi / toy-style characters, 50-90 mm, standing on z = 0, facing -Y.

Run from this folder:  python3 toon.py   (writes ../toon.json, renders previews to /tmp/lib_preview_toon)

Every figure is built from one `Frame` (proportions from its total height T): a huge round head
(diameter about half of T), a small squat body split into a lighter upper and a darker lower
shade, stubby arms and legs, and big glossy eyes (white ball + dark pupil + tiny highlight) with
pink blush. Hair, clothes and props are added per character. Fractions "zr" are in units of the
head radius R, measured up from the head centre.
"""
import math

from lib import *

# ---- colours -------------------------------------------------------------------------------
INK = (0.10, 0.08, 0.09)
WHITE = (1.0, 1.0, 1.0)
SKIN = (1.0, 0.87, 0.76)
BLUSH = (1.0, 0.50, 0.60)
MOUTH = (0.72, 0.20, 0.26)
GOLD = (1.0, 0.80, 0.25)
RED = (0.92, 0.16, 0.18)
SILVER = (0.80, 0.83, 0.90)
STEEL = (0.45, 0.49, 0.58)
BLACK = (0.13, 0.13, 0.16)
BROWN = (0.46, 0.28, 0.16)


# ---- primitives ----------------------------------------------------------------------------
def sph(name, c, r, color, scale=(1, 1, 1), blend=4.0, **kw):
    return shape("Sphere", name, pos=c, size=(r, 1, 1), scale=scale, color=color, blend=blend, **kw)


def cap(name, p0, p1, r, color, blend=4.0, **kw):
    """Capsule whose two ball centres are p0 and p1 (radius r)."""
    d = [p1[i] - p0[i] for i in range(3)]
    n = math.sqrt(sum(v * v for v in d))
    ux, uy, uz = (v / n for v in d)
    rx = math.degrees(math.asin(max(-1.0, min(1.0, -uy))))
    rz = math.degrees(math.atan2(ux, uz))
    mid = [(p0[i] + p1[i]) / 2 for i in range(3)]
    return shape("Capsule", name, pos=mid, size=(r, n, 1), rot=(rx, rz, 0), color=color, blend=blend, **kw)


def cyl(name, c, r, h, color, rot=(0, 0, 0), blend=1.0, **kw):
    return shape("Cylinder", name, pos=c, size=(r, h, 1), rot=rot, color=color, blend=blend, **kw)


def cone(name, c, r, h, color, rot=(0, 0, 0), blend=1.0, **kw):
    return shape("Cone", name, pos=c, size=(r, h, 1), rot=rot, color=color, blend=blend, **kw)


def box(name, c, size, color, rnd=1.0, blend=2.0, **kw):
    return shape("Box", name, pos=c, size=size, round=rnd, color=color, blend=blend, **kw)


def torus(name, c, major, minor, color, rot=(0, 0, 0), blend=1.0, **kw):
    """Lies flat (XY plane) unless rot tilts it."""
    return shape("Torus", name, pos=c, size=(major, minor, 1), rot=rot, color=color, blend=blend, **kw)


def star(name, c, r, h, color, rot=(0, 0, 0), blend=0.6, **kw):
    """A star prism: outer radius r, thickness h. rot (90, 0, 0) stands it up facing -Y."""
    return shape("Star", name, pos=c, size=(r, h, 1), rot=rot, color=color, blend=blend, **kw)


def revolve(name, c, profile, color, blend=3.0, **kw):
    """profile: [[radius, height], ...] turned about Z at c."""
    return shape("Revolve", name, pos=c, size=(1, 1, 1), outline=profile, params=(360, 0, 0, 0),
                 color=color, blend=blend, **kw)


def curve(name, pts, color, blend=2.0, **kw):
    """A tube through (x, y, z, radius) nodes: tails and other bendy things."""
    return shape("Curve", name, pos=(0, 0, 0), size=(1, 1, 1), path=[list(p) for p in pts],
                 color=color, blend=blend, roughness=0.6, **kw)


# ---- geometry ------------------------------------------------------------------------------
def unit(v):
    n = math.sqrt(sum(a * a for a in v))
    return tuple(a / n for a in v)


def surf(c, R, x, zr, dist):
    """A point `dist` from centre c, in the direction of sideways x and height zr (both from c)
    on the front (-Y) of a sphere of radius R. Sticks eyes, blush and mouths onto a face."""
    y = -math.sqrt(max(R * R - x * x - zr * zr, 1e-6))
    d = unit((x, y, zr))
    return (c[0] + d[0] * dist, c[1] + d[1] * dist, c[2] + d[2] * dist)


class Frame:
    """Chibi proportions from the total height T (mm, ground to top of the head)."""

    def __init__(self, T=66.0):
        self.T = T
        self.R = 0.24 * T              # head radius: the head is about half the height
        self.hc = T - self.R           # head centre
        self.c = (0.0, 0.0, self.hc)
        self.bw = 0.18 * T             # body half width (short and squat)
        self.zu = 0.42 * T             # upper torso centre
        self.zl = 0.27 * T             # lower torso centre
        self.lr = 0.07 * T             # limb radius (stubby)
        self.lx = 0.095 * T            # leg x
        self.sh = 0.40 * T             # shoulder height
        self.hz = 0.28 * T             # hand height
        self.ax = self.bw + 0.08 * T   # hand x


# ---- the common chibi parts ----------------------------------------------------------------
def head_ball(f, col, blend=3.0):
    return [sph("あたま", f.c, f.R, col, blend=blend)]


def torso(f, top, bot=None):
    """Upper body in the light shade; lower body in the dark shade (a soft gradient where they meet)."""
    S = [sph("からだ(うえ)", (0, 0, f.zu), f.bw, top, scale=(1, 0.9, 0.95), blend=5)]
    if bot is not None:
        S.append(sph("からだ(した)", (0, 0, f.zl), 0.95 * f.bw, bot, scale=(1, 0.9, 0.85), blend=5))
    return S


def limbs(f, arms, legs=None, shoes=None, hands=None):
    """Stubby arms and legs. legs=None leaves out legs and feet (a robe or skirt covers them)."""
    S = []
    for sx, side in ((1, "右"), (-1, "左")):
        S.append(cap(f"{side}うで", (sx * (f.bw - 0.02 * f.T), 0, f.sh), (sx * f.ax, -0.02 * f.T, f.hz),
                     f.lr * 0.95, arms, blend=3))
        if hands is not None:
            S.append(sph(f"{side}て", (sx * f.ax, -0.02 * f.T, f.hz), f.lr * 1.1, hands, blend=1.5))
        if legs is not None:
            S.append(cap(f"{side}あし", (sx * f.lx, 0, 0.2 * f.T), (sx * f.lx, 0, f.lr), f.lr, legs, blend=4))
            S.append(sph(f"{side}くつ", (sx * f.lx, -0.5 * f.lr, 0.83 * f.lr), 1.1 * f.lr, shoes or INK,
                         scale=(1, 1.35, 0.75), blend=2))
    return S


def base(f, skin, top, bot=None, arms=None, legs=None, shoes=None, hands=None):
    return head_ball(f, skin) + torso(f, top, bot) + limbs(f, arms or top, legs, shoes, hands)


def hair_cap(f, col, fringe=True, blend=1.0):
    """A round hair cap over the top of the head, with an optional bang over the forehead."""
    R, hc = f.R, f.hc
    S = [sph("かみ", (0, 0.10 * R, hc + 0.28 * R), 1.03 * R, col, scale=(1, 1, 0.82), blend=blend)]
    if fringe:
        S.append(sph("まえがみ", (0, -0.70 * R, hc + 0.70 * R), 0.42 * R, col, blend=0.8))
    return S


def eyes(c, R, zr=0.0, xo=0.42, rs=None, pupil=INK):
    """Big glossy eyes: a white ball, a dark pupil in front of it, and a tiny highlight on the pupil.
    zr and xo are fractions of R."""
    rs = 0.30 * R if rs is None else rs
    S = []
    for sx, side in ((1, "右"), (-1, "左")):
        x = sx * xo * R
        w = surf(c, R, x, zr * R, R - 0.25 * rs)
        S.append(sph(f"{side}め(しろ)", w, rs, WHITE, blend=0.5))
        rp = 0.66 * rs
        p = surf(c, R, x, zr * R, R + 0.55 * rs)
        S.append(sph(f"{side}め(ひとみ)", p, rp, pupil, blend=0.25, roughness=0.15, coat=0.6))
        n = unit(tuple(p[i] - c[i] for i in range(3)))
        h = (p[0] + n[0] * 0.8 * rp - 0.3 * rp, p[1] + n[1] * 0.8 * rp, p[2] + n[2] * 0.8 * rp + 0.35 * rp)
        S.append(sph(f"{side}め(ハイライト)", h, 0.26 * rp, WHITE, blend=0.1, roughness=0.1))
    return S


def blush(c, R, zr=-0.28, xo=0.66, r=0.2):
    """Flat pink cheeks."""
    S = []
    for sx, side in ((1, "右"), (-1, "左")):
        p = surf(c, R, sx * xo * R, zr * R, R - 0.1 * r * R)
        S.append(sph(f"{side}ほっぺ", p, r * R, BLUSH, scale=(1, 0.5, 0.75), blend=1.0,
                     translucency=0.3, roughness=0.7))
    return S


def mouth(c, R, zr=-0.45, colour=MOUTH):
    p = surf(c, R, 0.0, zr * R, R - 0.02 * R)
    return sph("くち", p, 0.09 * R, colour, scale=(1.7, 0.45, 0.6), blend=0.4)


def face(c, R, zr=0.0, xo=0.42, eye_rs=None, cheeks=True, mouth_on=True, mouth_col=MOUTH):
    S = eyes(c, R, zr, xo, eye_rs)
    if cheeks:
        S += blush(c, R)
    if mouth_on:
        S.append(mouth(c, R, colour=mouth_col))
    return S


# ---------------------------------------------------------------- the characters

def boy():
    f = Frame(62)
    hair = (0.34, 0.20, 0.10)
    S = base(f, SKIN, top=(0.46, 0.76, 1.0), bot=(0.20, 0.42, 0.92), legs=(0.26, 0.28, 0.56),
             shoes=RED)
    S += hair_cap(f, hair)
    S.append(sph("ぴょこ", (0.20 * f.R, 0.20 * f.R, f.hc + 1.12 * f.R), 0.24 * f.R, hair, blend=0.8))
    S += face(f.c, f.R)
    return S


def girl():
    f = Frame(62)
    hair = (0.66, 0.36, 0.22)
    dress = (1.0, 0.74, 0.86)
    S = base(f, SKIN, top=dress, bot=None, arms=dress, legs=SKIN, shoes=(0.95, 0.32, 0.48))
    S.append(revolve("スカート", (0, 0, 0), [[0, 0.10 * f.T], [0.27 * f.T, 0.10 * f.T],
                                             [0.22 * f.T, 0.20 * f.T], [0.15 * f.T, 0.27 * f.T],
                                             [0, 0.27 * f.T]], (0.98, 0.50, 0.70), blend=3))
    S += hair_cap(f, hair)
    R, hc = f.R, f.hc
    S.append(sph("かみ(うしろ)", (0, 0.5 * R, hc - 0.5 * R), 0.95 * R, hair, scale=(1, 0.9, 1.45), blend=1.0))
    for sx, side in ((1, "右"), (-1, "左")):
        S.append(cap(f"{side}しっぽ", (sx * 0.95 * R, 0.2 * R, hc + 0.1 * R), (sx * 1.15 * R, 0.25 * R, hc - 1.1 * R),
                     0.40 * R, hair, blend=1.5))
    S.append(sph("リボン", (0.62 * R, -0.62 * R, hc + 0.85 * R), 0.30 * R, (1.0, 0.30, 0.50),
                 scale=(1.4, 0.6, 1.0), blend=0.6))
    S += face(f.c, f.R)
    return S


def cat():
    f = Frame(66)
    R, hc = f.R, f.hc
    fur = (0.60, 0.57, 0.68)
    top, bot = (1.0, 0.86, 0.50), (0.96, 0.66, 0.26)
    S = base(f, SKIN, top=top, bot=bot, arms=top, legs=SKIN, shoes=(0.97, 0.96, 0.98))
    S += hair_cap(f, fur, fringe=False)
    for sx, side in ((1, "右"), (-1, "左")):
        S.append(cone(f"{side}みみ", (sx * 0.55 * R, 0, hc + 0.78 * R), 0.42 * R, 0.95 * R, fur,
                      rot=(0, sx * 14, 0), blend=1.5))
        S.append(cone(f"{side}みみの中", (sx * 0.55 * R, -0.22 * R, hc + 0.72 * R), 0.22 * R, 0.6 * R,
                      (1.0, 0.60, 0.68), rot=(0, sx * 14, 0), blend=0.6))
    S.append(curve("しっぽ", [(0, 9, 0.22 * f.T, 2.2), (2, 14, 0.20 * f.T, 2.0), (5, 17, 0.30 * f.T, 1.8),
                             (5, 16, 0.42 * f.T, 1.5)], fur, blend=2.0))
    S += face(f.c, f.R)
    return S


def bunny():
    f = Frame(60)
    R, hc = f.R, f.hc
    white = (0.97, 0.95, 1.0)
    top, bot = (0.70, 0.95, 0.86), (0.44, 0.82, 0.72)
    S = base(f, SKIN, top=top, bot=bot, arms=top, legs=white, shoes=(1.0, 0.62, 0.72))
    S += hair_cap(f, white, fringe=False)
    for sx, side in ((1, "右"), (-1, "左")):
        S.append(cap(f"{side}みみ", (sx * 0.32 * R, 0.05 * R, hc + 0.6 * R), (sx * 0.42 * R, 0.02 * R, hc + 2.0 * R),
                     0.24 * R, white, blend=2.5))
        S.append(cap(f"{side}みみの中", (sx * 0.32 * R, -0.2 * R, hc + 0.7 * R), (sx * 0.42 * R, -0.2 * R, hc + 1.9 * R),
                     0.12 * R, (1.0, 0.62, 0.70), blend=0.6))
    S.append(sph("しっぽ", (0, 0.95 * f.bw, 0.26 * f.T), 0.10 * f.T, white, blend=2.0))
    S += face(f.c, f.R)
    return S


def knight():
    f = Frame(76)
    R, hc = f.R, f.hc
    S = base(f, SKIN, top=SILVER, bot=STEEL, arms=SILVER, legs=STEEL, shoes=(0.50, 0.30, 0.16))
    S.append(sph("かぶと", (0, 0.1 * R, hc + 0.4 * R), 1.04 * R, SILVER, scale=(1, 1, 0.8), blend=1.0,
                 metallic=0.7, roughness=0.3))
    S.append(cap("はね", (0, 0.2 * R, hc + 0.95 * R), (0.35 * R, 0.6 * R, hc + 1.75 * R), 0.22 * R, RED, blend=1.0))
    S.append(box("けん", (f.ax + 0.5, -1.5, f.hz + 13), (2.0, 1.0, 24), (0.90, 0.93, 0.98), rnd=0.3, blend=1.0,
                 metallic=0.9, roughness=0.2))
    S.append(box("つば", (f.ax, -1.5, f.hz + 2.5), (7.0, 2.0, 1.6), GOLD, rnd=0.5, blend=1.0,
                 metallic=0.8, roughness=0.3))
    S.append(cyl("たて", (-(f.ax + 1.5), -4.0, 0.30 * f.T), 0.13 * f.T, 1.6, RED, rot=(90, 0, 0), blend=1.0))
    S += face(f.c, f.R)
    return S


def mage():
    f = Frame(70)
    R, hc, T = f.R, f.hc, f.T
    lilac, robe, hat = (0.64, 0.44, 0.94), (0.40, 0.22, 0.70), (0.30, 0.16, 0.55)
    S = base(f, SKIN, top=lilac, bot=None, arms=lilac, hands=SKIN)
    S.append(revolve("ローブ", (0, 0, 0), [[0, 0], [0.26 * T, 0], [0.22 * T, 0.24 * T],
                                          [0.17 * T, 0.36 * T], [0, 0.36 * T]], robe, blend=4))
    brim_top = hc + 0.5 * R + 0.9
    S.append(cyl("つば", (0, 0, hc + 0.5 * R), 1.5 * R, 1.8, hat, blend=0.8))
    S.append(cone("とんがりぼうし", (0, 0, brim_top + 11), 0.85 * R, 22, hat, blend=1.0))
    S.append(cyl("ぼうしのおび", (0, 0, brim_top + 1.3), 0.87 * R, 2.6, GOLD, blend=0.4))
    S.append(star("ほし", (0, -0.54 * R - 0.6, brim_top + 8), 3.2, 1.0, GOLD, rot=(90, 0, 0)))
    for sx, side in ((1, "右"), (-1, "左")):
        S.append(cap(f"{side}かみ", (sx * 0.95 * R, 0.1 * R, hc - 0.1 * R), (sx * 1.05 * R, 0.1 * R, hc - 1.1 * R),
                     0.32 * R, (0.92, 0.86, 0.96), blend=1.5))
    S.append(cap("つえ", (f.ax, -2, 0.035 * T * 1.1), (f.ax, -2, 0.88 * T), 0.035 * T, BROWN, blend=1.0))
    S.append(sph("すいしょう", (f.ax, -2, 0.92 * T), 0.07 * T, (0.50, 0.95, 1.0), glow=1.2, blend=0.6))
    S += face(f.c, f.R)
    return S


def ninja():
    f = Frame(64)
    R, hc, T = f.R, f.hc, f.T
    red = (0.90, 0.12, 0.15)
    mask = (0.26, 0.28, 0.36)
    S = base(f, BLACK, top=(0.20, 0.20, 0.25), bot=(0.06, 0.06, 0.08), arms=(0.14, 0.14, 0.18),
             legs=(0.10, 0.10, 0.12), shoes=(0.05, 0.05, 0.06))
    S.append(sph("くちマスク", surf(f.c, R, 0, -0.42 * R, R - 0.1 * R), 0.4 * R, mask,
                 scale=(1.4, 0.5, 0.85), blend=0.5))
    S.append(torus("はちまき", (0, 0, hc + 0.45 * R), 1.03 * R, 0.15 * R, red, blend=0.6))
    for sx, side in ((1, "右"), (-1, "左")):
        S.append(cap(f"{side}はちまきのはし", (sx * 0.3 * R, 0.95 * R, hc + 0.45 * R),
                     (sx * 0.55 * R, 1.5 * R, hc - 0.25 * R), 0.12 * R, red, blend=1.0))
    S.append(torus("マフラー", (0, 0, 0.50 * T), 0.17 * T, 0.035 * T, red, blend=1.0))
    S.append(cap("かたなのさや", (0.09 * T, 0.13 * T, 0.22 * T), (-0.09 * T, 0.13 * T, 0.56 * T), 0.028 * T,
                 (0.22, 0.13, 0.08), blend=1.0))
    S += face(f.c, R, zr=0.10, cheeks=False, mouth_on=False)
    return S


def princess():
    f = Frame(72)
    R, hc, T = f.R, f.hc, f.T
    gold_hair = (0.99, 0.84, 0.42)
    dress = (1.0, 0.80, 0.92)
    S = base(f, SKIN, top=dress, bot=None, arms=dress)
    S.append(revolve("スカート", (0, 0, 0), [[0, 0], [0.30 * T, 0], [0.27 * T, 0.12 * T],
                                             [0.20 * T, 0.22 * T], [0.14 * T, 0.27 * T], [0, 0.27 * T]],
                     (0.98, 0.50, 0.74), blend=3))
    S += hair_cap(f, gold_hair, fringe=False)
    S.append(sph("かみ(うしろ)", (0, 0.5 * R, hc - 0.6 * R), 0.95 * R, gold_hair, scale=(1, 0.85, 1.6), blend=1.0))
    band = hc + 0.62 * R
    S.append(cyl("ティアラ", (0, 0, band), 1.06 * R, 3.2, GOLD, blend=0.4, metallic=0.8, roughness=0.3))
    zc = band + 1.6 + 0.3 * R
    for x, y in ((0, -1.06), (-0.75, -0.75), (0.75, -0.75)):
        S.append(cone("ティアラのとがり", (x * R, y * R, zc), 0.22 * R, 0.6 * R, GOLD, blend=0.4,
                      metallic=0.8, roughness=0.3))
    S.append(sph("ほうせき", (0, -1.08 * R, band), 0.12 * R, (1.0, 0.35, 0.65), blend=0.4, glow=0.4))
    S.append(sph("リボン", (0, -0.95 * f.bw, 0.42 * T), 0.035 * T, (1.0, 0.35, 0.55), scale=(1.8, 0.6, 1.0),
                 blend=0.6))
    S += face(f.c, R)
    return S


def hero():
    f = Frame(68)
    R, hc = f.R, f.hc
    blue = (0.20, 0.42, 0.92)
    S = base(f, SKIN, top=(0.36, 0.82, 0.48), bot=(0.22, 0.58, 0.32), arms=(0.36, 0.82, 0.48),
             legs=(0.52, 0.36, 0.22), shoes=(0.36, 0.22, 0.14))
    S.append(sph("かみ", (0, 0.1 * R, hc + 0.3 * R), 1.02 * R, blue, scale=(1, 1, 0.8), blend=1.0))
    for x, y, rot in ((0, -0.4, (20, 0, 0)), (-0.6, 0.0, (0, -28, 0)), (0.6, 0.0, (0, 28, 0))):
        S.append(cone("とげ", (x * R, y * R, hc + 1.05 * R), 0.28 * R, 0.9 * R, blue, rot=rot, blend=0.8))
    S.append(box("けん", (f.ax, -1.5, f.hz + 13), (2.0, 1.0, 24), (0.90, 0.93, 0.98), rnd=0.3, blend=1.0,
                 metallic=0.9, roughness=0.2))
    S.append(box("つば", (f.ax, -1.5, f.hz + 2.5), (7.0, 2.0, 1.6), GOLD, rnd=0.5, blend=1.0))
    S += face(f.c, R)
    return S


def superhero():
    f = Frame(72)
    R, hc, T = f.R, f.hc, f.T
    suit, shorts = (0.20, 0.42, 0.95), (0.92, 0.16, 0.20)
    black = (0.10, 0.09, 0.12)
    S = base(f, SKIN, top=suit, bot=shorts, arms=suit, legs=shorts, shoes=(0.12, 0.14, 0.30))
    S += hair_cap(f, black, fringe=False)
    S.append(sph("まえがみ", (0, -0.30 * R, hc + 0.90 * R), 0.55 * R, black, scale=(1, 0.8, 0.75), blend=1.0))
    S.append(box("マント", (0, 0.22 * T, 0.36 * T), (0.46 * T, 1.5, 0.56 * T), (0.85, 0.10, 0.14), rnd=2.0,
                 blend=3.0))
    S.append(star("マーク", (0, -f.bw - 0.5, f.zu + 0.02 * T), 3.4, 1.0, GOLD, rot=(90, 0, 0),
                  metallic=0.6, roughness=0.3))
    S.append(cyl("ベルト", (0, 0, 0.30 * T), 0.17 * T, 3.0, GOLD, blend=0.6))
    S += face(f.c, R)
    return S


def dragon():
    f = Frame(64)
    R, hc, T = f.R, f.hc, f.T
    green = (0.36, 0.80, 0.44)
    cream = (0.98, 0.92, 0.62)
    S = base(f, green, top=(0.52, 0.90, 0.52), bot=(0.30, 0.68, 0.38), arms=(0.52, 0.90, 0.52),
             legs=(0.32, 0.70, 0.40), shoes=cream)
    S.append(sph("おなか", (0, -0.9 * f.bw, 0.38 * T), 0.11 * T, cream, scale=(1, 0.45, 1.3), blend=1.0))
    for sx, side in ((1, "右"), (-1, "左")):
        S.append(cone(f"{side}つの", (sx * 0.5 * R, 0.15 * R, hc + 1.05 * R), 0.2 * R, 0.7 * R, cream,
                      rot=(0, sx * 15, 0), blend=0.6))
        S.append(sph(f"{side}つばさ", (sx * 0.9 * f.bw, 0.5 * f.bw, 0.46 * T), 0.14 * T, (0.46, 0.86, 0.52),
                     scale=(0.25, 0.9, 1.25), blend=2.0))
    S.append(curve("しっぽ", [(0, 9, 0.22 * T, 2.4), (2, 16, 0.18 * T, 2.0), (3, 22, 0.24 * T, 1.6),
                             (4, 26, 0.34 * T, 1.2)], green, blend=2.0))
    S += face(f.c, R, mouth_col=(0.60, 0.16, 0.20))
    return S


def slime():
    T = 70.0
    R = 0.30 * T
    head_c = (0, 0, 0.52 * T)
    green, light = (0.36, 0.86, 0.46), (0.62, 0.98, 0.66)
    S = [
        sph("からだ", (0, 0, 0.288 * T), 0.36 * T, green, scale=(1, 1, 0.8), blend=6,
            translucency=0.45, roughness=0.3, coat=0.4),
        sph("あたま", head_c, R, light, scale=(1, 1, 0.95), blend=8, translucency=0.45, roughness=0.3, coat=0.4),
        sph("みずたまり", (0, 0, 0.8), 0.5 * T, (0.30, 0.74, 0.40), scale=(1, 1, 0.08), blend=2.0),
        sph("したたり", (-0.18 * T, -0.26 * T, 0.2 * T), 0.07 * T, green, scale=(1, 1, 1.6), blend=5),
        sph("したたり2", (0.22 * T, -0.24 * T, 0.2 * T), 0.06 * T, green, scale=(1, 1, 1.6), blend=5),
        sph("ハイライト", surf(head_c, R, -0.5 * R, 0.55 * R, 0.95 * R), 0.14 * R, WHITE, blend=0.2, roughness=0.1),
        sph("ハイライト2", surf(head_c, R, -0.25 * R, 0.8 * R, 0.95 * R), 0.07 * R, WHITE, blend=0.2, roughness=0.1),
    ]
    S += face(head_c, R, mouth_col=(0.45, 0.12, 0.20))
    return S


def robot():
    f = Frame(68)
    R, hc, T = f.R, f.hc, f.T
    silver, grey = (0.78, 0.82, 0.88), (0.42, 0.46, 0.54)
    S = [
        box("あたま", f.c, (1.9 * R, 1.9 * R, 1.9 * R), silver, rnd=0.42 * R, blend=3, metallic=0.5, roughness=0.3),
        cap("アンテナ", (0, 0, hc + 0.95 * R), (0, 0, hc + 0.95 * R + 0.16 * T), 0.02 * T, grey, blend=2),
        sph("アンテナのたま", (0, 0, hc + 0.95 * R + 0.16 * T), 0.04 * T, (1.0, 0.25, 0.25), glow=1.5, blend=1),
        box("からだ(うえ)", (0, 0, f.zu), (1.9 * f.bw, 1.6 * f.bw, 0.42 * T), (0.62, 0.88, 1.0),
            rnd=0.4 * f.bw, blend=5, metallic=0.4, roughness=0.3),
        box("からだ(した)", (0, 0, f.zl), (1.8 * f.bw, 1.6 * f.bw, 0.20 * T), (0.36, 0.56, 0.80),
            rnd=0.4 * f.bw, blend=5, metallic=0.4, roughness=0.3),
        sph("むねのランプ", (0, -0.82 * f.bw, 0.40 * T), 0.05 * T, (0.30, 1.0, 1.0), glow=1.5, blend=0.6),
    ]
    S += limbs(f, arms=grey, legs=grey, shoes=(0.30, 0.34, 0.42))
    S += face(f.c, 0.95 * R, cheeks=True, mouth_col=(0.25, 0.28, 0.32))
    return S


def ghost():
    T = 64.0
    white, lilac = (0.97, 0.97, 1.0), (0.82, 0.80, 0.96)
    head_c = (0, 0, 0.62 * T)
    R = 0.32 * T
    S = [
        sph("あたま", head_c, R, white, blend=6, translucency=0.3, roughness=0.4),
        sph("からだ", (0, 0, 0.40 * T), 0.30 * T, lilac, blend=6, translucency=0.3, roughness=0.4),
        sph("ぴょん", (0.06 * T, 0, 0.94 * T), 0.10 * T, white, blend=3, translucency=0.3),
    ]
    for i, x in enumerate((-0.28, -0.14, 0.0, 0.14, 0.28)):
        S.append(sph(f"すそ{i + 1}", (x * T, 0, 0.14 * T), 0.14 * T, lilac, blend=6, translucency=0.3,
                     roughness=0.4))
    for sx, side in ((1, "右"), (-1, "左")):
        S.append(cap(f"{side}て", (sx * 0.24 * T, 0, 0.42 * T), (sx * 0.40 * T, -0.03 * T, 0.30 * T), 0.06 * T,
                     white, blend=3, translucency=0.3))
    S += face(head_c, R, mouth_col=INK)
    return S


# ---------------------------------------------------------------- the library items
ITEMS_SPEC = [
    ("チビおとこのこ", ["boy", "child", "kid", "chibi", "デフォルメ", "男の子", "ちび"], boy),
    ("チビおんなのこ", ["girl", "child", "kid", "chibi", "デフォルメ", "女の子", "ちび"], girl),
    ("ねこみみのこ", ["cat ears", "cat girl", "kid", "neko", "デフォルメ", "猫", "ねこ耳"], cat),
    ("うさみみのこ", ["bunny", "rabbit ears", "kid", "usagi", "デフォルメ", "兎", "うさぎ"], bunny),
    ("ナイトきし", ["knight", "armour", "sword", "helmet", "デフォルメ", "騎士", "鎧"], knight),
    ("まほうつかい", ["mage", "wizard", "witch", "staff", "hat", "デフォルメ", "魔法使い"], mage),
    ("ニンジャ", ["ninja", "black", "mask", "headband", "デフォルメ", "忍者"], ninja),
    ("ひめさま", ["princess", "dress", "crown", "デフォルメ", "姫", "お姫様"], princess),
    ("ゆうしゃ", ["hero", "brave", "sword", "spiky hair", "デフォルメ", "勇者"], hero),
    ("ヒーロー", ["superhero", "cape", "hero", "デフォルメ", "ヒーロー"], superhero),
    ("ドラゴンのこ", ["dragon", "costume", "kid", "デフォルメ", "ドラゴン", "恐竜"], dragon),
    ("スライム", ["slime", "blob", "jelly", "デフォルメ", "スライム"], slime),
    ("ロボのこ", ["robot", "kid", "android", "デフォルメ", "ロボット"], robot),
    ("おばけ", ["ghost", "boo", "spooky", "デフォルメ", "おばけ", "お化け"], ghost),
]

items = [item(name, tags, group(name, make())) for name, tags, make in ITEMS_SPEC]


if __name__ == "__main__":
    assert len(items) == 14, len(items)
    for it in items:
        n = len(it["group"]["shapes"])
        print(f"{n:2d} shapes  {it['name']}")
        assert n <= 24, (it["name"], n)
    write_category("toon.json", "デフォルメ", items)
    preview(items, outdir="/tmp/lib_preview_toon")
