"""Generator for ../realistic.json (category リアルなひと): 12 realistic-proportion human figures.

Run:  python3 realistic.py     (writes ../realistic.json, renders /tmp/lib_preview_real/*.png)

Every figure is laid out from a small skeleton, computed in mm from the pose angles: the pelvis,
spine, neck and head, shoulders, hips, knees, ankles, elbows and wrists. Limbs are Capsules
between joint points, so a pose is just a set of angles. Torso, head, hands and feet are scaled
Spheres and Boxes. Joint blends are 4-8 mm so limbs melt into the torso; eyes, nose and ears use
0.5-1.2 mm so they stay crisp. A figure is scaled so its lowest foot point sits on z = 0.

Engine conventions used here (src/sdf.rs): size is the full extent of a Box and of a Cylinder; a
Capsule has size (radius, length between the ball centres, 1); scale is applied before the
rotation, which is X then Y then Z.
"""
import math

from lib import *

# ---- colours -------------------------------------------------------------------------------
INK = (0.10, 0.08, 0.08)
SKIN = (0.92, 0.74, 0.60)
SKIN_D = (0.84, 0.64, 0.50)
HAIR_BROWN = (0.24, 0.14, 0.08)
HAIR_BLACK = (0.10, 0.08, 0.07)
HAIR_GREY = (0.80, 0.80, 0.82)
HAIR_ORANGE = (0.62, 0.30, 0.12)
MANNEQUIN = (0.62, 0.62, 0.64)
BLUE_J = (0.20, 0.30, 0.52)
GREY_P = (0.36, 0.37, 0.40)
DARK_P = (0.14, 0.14, 0.18)
WHITE = (0.94, 0.94, 0.95)
SHOE_BR = (0.30, 0.18, 0.10)
SHOE_BK = (0.10, 0.10, 0.11)
RED = (0.82, 0.14, 0.16)
ORANGE = (0.96, 0.52, 0.16)
TEAL = (0.14, 0.56, 0.56)
PURPLE = (0.44, 0.24, 0.62)
NAVY = (0.12, 0.18, 0.36)
BEIGE = (0.78, 0.70, 0.52)
GREEN_J = (0.22, 0.36, 0.26)
BLOUSE = (0.88, 0.40, 0.48)
CARDIGAN = (0.52, 0.36, 0.26)


# ---- small vector helpers (mm, degrees) ----------------------------------------------------
def vnorm(v):
    n = math.sqrt(sum(x * x for x in v))
    return [x / n for x in v]


def rad(d):
    return math.radians(d)


def Mab(a, b):
    """Rotation Ry(b) * Rx(a): the engine's matrix for rot = (a, b, 0), rows as in sdf.rs."""
    sa, ca = math.sin(rad(a)), math.cos(rad(a))
    sb, cb = math.sin(rad(b)), math.cos(rad(b))
    return [[cb, sb * sa, sb * ca], [0.0, ca, -sa], [-sb, cb * sa, cb * ca]]


def mv(m, v):
    return [sum(m[i][j] * v[j] for j in range(3)) for i in range(3)]


def orient(d):
    """Euler (X, Y, 0) that turns local +Z onto direction d."""
    n = vnorm(d)
    return (math.degrees(math.asin(max(-1.0, min(1.0, -n[1])))), math.degrees(math.atan2(n[0], n[2])), 0.0)


def spine_dir(lean, sway):
    """Unit axis of the spine: lean forward (toward -Y) by `lean` degrees, sway toward +X by `sway`."""
    return vnorm([math.sin(rad(sway)), -math.sin(rad(lean)), math.cos(rad(lean))])


def dirv(fwd, abd, s):
    """Limb direction: fwd degrees from straight down toward -Y (forward), abd degrees out to the side
    (s = +1 or -1 for the two sides)."""
    f, a = rad(fwd), rad(abd)
    return vnorm([s * math.sin(a), -math.sin(f) * math.cos(a), -math.cos(f) * math.cos(a)])


def addv(a, b, k=1.0):
    return [a[i] + k * b[i] for i in range(3)]


# ---- primitives ----------------------------------------------------------------------------
def ell(name, c, rad3, color, blend=5.0, rot=(0, 0, 0), **kw):
    """Ellipsoid with radii rad3 = (x, y, z) in its own frame, rotated by rot."""
    r = rad3[2]
    return shape("Sphere", name, pos=c, size=(r, 1, 1), scale=(rad3[0] / r, rad3[1] / r, 1.0),
                 rot=rot, color=color, blend=blend, **kw)


def cap(name, p0, p1, r, color, blend=5.0, **kw):
    """Capsule whose two ball centres are p0 and p1 (radius r)."""
    d = [p1[i] - p0[i] for i in range(3)]
    n = math.sqrt(sum(v * v for v in d))
    rot = orient(d)
    mid = [(p0[i] + p1[i]) / 2 for i in range(3)]
    return shape("Capsule", name, pos=mid, size=(r, n, 1), rot=rot, color=color, blend=blend, **kw)


def bx(name, c, size, color, rot=(0, 0, 0), rnd=1.5, blend=2.0, **kw):
    return shape("Box", name, pos=c, size=size, rot=rot, round=rnd, color=color, blend=blend, **kw)


# ---- the figure ----------------------------------------------------------------------------
LEG = dict(th=0.0, ab=0.0, kn=0.0, pt=0.0)          # thigh forward, out, knee bend, foot pitch
ARM = dict(fw=0.0, ab=8.0, el=10.0, wr=0.0)          # upper arm forward, out, elbow bend, wrist


def figure(F):
    """Shapes of one person. F keys (defaults in brackets):
    H total height [170], hh head height [22.7], build width factor [1.0], fem [False],
    lean / sway / nod forward lean, sideways sway, head nod (deg) [0],
    pz pelvis height [0.535 H], legs {1: LEG, -1: LEG}, arms {1: ARM, -1: ARM},
    face [True], hair None|short|long|bun|grey, block [False] (seat block under the hips),
    skirt [False], c: colours dict (skin, top, sleeve, fore, bottom, thigh, calf, shoe, hair_col)."""
    H, hh = float(F.get("H", 170)), float(F.get("hh", 22.7))
    k = H / 170.0
    bd = F.get("build", 1.0)
    kW = k * bd
    kD = k * (0.5 + 0.5 * bd)
    fem = F.get("fem", False)
    wf = 0.92 if fem else 1.0
    hipf = 1.06 if fem else 1.0
    mus = F.get("mus", 1.0)       # extra limb thickness (muscle)
    shw = 14.0 * kW * wf          # shoulder joint offset from the middle
    hw = 8.5 * kW * hipf          # hip joint offset
    C = F["c"]
    lean, sway, nod = F.get("lean", 0.0), F.get("sway", 0.0), F.get("nod", 0.0)
    u = spine_dir(lean, sway)
    P0z = F.get("pz", 0.535 * H)
    P0 = [0.0, 0.0, P0z]
    Lsp = H - 1.37 * hh - 0.535 * H  # pelvis centre to neck base: the torso keeps its length

    def spine(h):
        """A point h mm above the pelvis centre along the spine."""
        return [P0[i] + h * u[i] / u[2] for i in range(3)]

    neck = spine(Lsp)
    uh = spine_dir(lean + nod, sway)
    HC = addv(neck, uh, 0.87 * hh)     # head centre

    # ---- legs (raw coordinates, before grounding) ----
    legs_raw = {}
    for s in (1, -1):
        L_ = dict(LEG, **F.get("legs", {}).get(s, {}))
        HJ = [s * hw, 0.0, P0z - 3.0 * k]
        T1 = dirv(L_["th"], L_["ab"], s)
        K = addv(HJ, T1, 40.5 * k)
        sf = L_["sf"] if "sf" in L_ else L_["th"] - L_["kn"]   # shin forward angle (sf overrides kn)
        T2 = dirv(sf, L_.get("sab", L_["ab"]), s)
        A = addv(K, T2, 40.5 * k)
        M = Mab(L_["pt"], 0.0)
        FC = addv(A, mv(M, [0.0, -4.5 * k, -3.4 * k]))
        legs_raw[s] = dict(HJ=HJ, K=K, A=A, FC=FC, M=M, pt=L_["pt"])
    fw_, fl_, fh_ = 9.0 * kW, 24.0 * k, 6.4 * k
    zs = []
    for s in (1, -1):
        L_ = legs_raw[s]
        for dx in (-fw_ / 2, fw_ / 2):
            for dy in (-fl_ / 2, fl_ / 2):
                for dz_ in (-fh_ / 2, fh_ / 2):
                    zs.append(L_["FC"][2] + mv(L_["M"], [dx, dy, dz_])[2])
    dz = -min(zs) + F.get("lift", 0.0)

    def T(p):
        return [p[0], p[1], p[2] + dz]

    S = []
    # ---- torso: chest, waist and pelvis as three scaled spheres ----
    S.append(ell("むね", T(spine(0.77 * Lsp)), (14.5 * kW * wf, 10.0 * kD, 11.5 * k), C["top"], blend=6,
                 rot=orient(u)))
    S.append(ell("こし", T(spine(0.40 * Lsp)), (10.5 * kD * wf, 8.5 * kD, 10.5 * k), C["top"], blend=6,
                 rot=orient(u)))
    S.append(ell("こしまわり", T(P0), (13.0 * kD * hipf, 9.0 * kD, 8.5 * k), C["bottom"], blend=6,
                 rot=orient(u)))
    S.append(cap("くび", T(spine(Lsp - 2.5 * k)), T(HC), 4.8 * kW, C["skin"], blend=4))

    # ---- head: an egg, with features on its front (-Y) ----
    hr = (0.33 * hh, 0.42 * hh, 0.5 * hh)
    S.append(ell("あたま", T(HC), hr, C["skin"], blend=3, rot=orient(uh)))
    Mh = Mab(*orient(uh)[:2])

    def hl(off):
        return T(addv(HC, mv(Mh, off)))

    if F.get("face", True):
        for s in (1, -1):
            side = "右" if s > 0 else "左"
            S.append(ell(f"{side}め", hl((s * 0.135 * hh, -0.37 * hh, 0.03 * hh)),
                         (0.05 * hh, 0.035 * hh, 0.05 * hh), INK, blend=0.6, rot=orient(uh)))
            S.append(ell(f"{side}みみ", hl((s * 0.35 * hh, 0.02 * hh, -0.02 * hh)),
                         (0.06 * hh, 0.07 * hh, 0.10 * hh), C["skin"], blend=1.2, rot=orient(uh)))
        S.append(ell("はな", hl((0, -0.41 * hh, -0.07 * hh)), (0.065 * hh, 0.10 * hh, 0.10 * hh),
                     C["skin"], blend=1.2, rot=orient(uh)))
    hair = F.get("hair")
    hc = C.get("hair_col", HAIR_BROWN)
    if hair in ("short", "grey", "bun"):
        S.append(ell("かみ", hl((0, 0.10 * hh, 0.05 * hh)), (0.36 * hh, 0.43 * hh, 0.49 * hh), hc,
                     blend=0.8, rot=orient(uh)))
        if hair == "bun":
            S.append(ell("おだんご", hl((0, 0.45 * hh, 0.55 * hh)), (0.22 * hh, 0.22 * hh, 0.22 * hh), hc,
                         blend=0.8))
    elif hair == "long":
        # one long ellipsoid: covers the crown and falls down the back to the shoulders
        S.append(ell("かみ", hl((0, 0.16 * hh, -0.04 * hh)), (0.42 * hh, 0.47 * hh, 0.62 * hh), hc,
                     blend=1.0, rot=orient(uh)))

    # ---- arms ----
    for s in (1, -1):
        side = "右" if s > 0 else "左"
        A_ = dict(ARM, **F.get("arms", {}).get(s, {}))
        SH = addv(spine(Lsp - 4.0 * k), [1, 0, 0], s * shw)
        Ua = dirv(A_["fw"], A_["ab"], s)
        E = addv(SH, Ua, 29.0 * k)
        fab = A_.get("fab", A_["ab"])     # forearm side angle (flexed arms)
        Fa = dirv(A_["fw"] + A_["el"], fab, s)
        Wr = addv(E, Fa, 25.0 * k)
        Hd = dirv(A_["fw"] + A_["el"] + A_["wr"], fab, s)
        Hc = addv(Wr, Hd, 9.0 * k)
        up_c = C.get("sleeve", C["top"])
        fo_c = C.get("fore") or (C.get("sleeve") if C.get("long") else None) or C["skin"]
        S.append(cap(f"{side}うで", T(SH), T(E), 4.3 * kW * mus, up_c, blend=6))
        S.append(cap(f"{side}ひじ下", T(E), T(Wr), 3.2 * kW * mus, fo_c, blend=4))
        S.append(ell(f"{side}て", T(Hc), (3.6 * kW, 2.2 * k, 9.0 * k), C["skin"], blend=2.5,
                     rot=orient(Hd)))

    # ---- legs ----
    for s in (1, -1):
        side = "右" if s > 0 else "左"
        L_ = legs_raw[s]
        S.append(cap(f"{side}ふともも", T(L_["HJ"]), T(L_["K"]), 6.6 * kW * mus, C.get("thigh", C.get("bottom", C["skin"])),
                     blend=6))
        S.append(cap(f"{side}すね", T(L_["K"]), T(L_["A"]), 4.7 * kW * mus, C.get("calf", C.get("bottom", C["skin"])), blend=5))
        S.append(bx(f"{side}あし", T(L_["FC"]), (fw_, fl_, fh_), C["shoe"], rot=(L_["pt"], 0, 0),
                    rnd=1.8 * k, blend=2.0))

    # ---- optional: seat block under the hips, a skirt ----
    if F.get("block"):
        seat = legs_raw[1]["HJ"][2] + dz - 6.0 * kW
        S.append(bx("いす", [0.0, 3.0 * k, seat / 2], (34.0 * kW, 30.0 * kD, seat), C["block"],
                    rnd=2.0, blend=2.0))
    if F.get("skirt"):
        S.append(shape("Revolve", "スカート", pos=T(addv(P0, [0, 0, -6.0 * k])), size=(1, 1, 1),
                       outline=[[0, -32 * k], [19 * k, -32 * k], [12 * k, 6 * k], [0, 6 * k]],
                       params=(360, 0, 0, 0), color=C["skirt"], blend=4.0))
    return S


def item_of(name, tags, F):
    shapes = figure(F)
    assert len(shapes) <= 24, (name, len(shapes))
    return item(name, tags, group(name, shapes))


# ---- standard poses ------------------------------------------------------------------------
def legs_std(th=0.0, kn=0.0, pt=0.0, ab=0.0):
    return {s: dict(th=th, ab=ab, kn=kn, pt=pt) for s in (1, -1)}


def arms_std(fw=0.0, ab=8.0, el=10.0):
    return {s: dict(fw=fw, ab=ab, el=el, wr=0.0) for s in (1, -1)}


items = []

# 1. だんせい -- a man: short brown hair, blue-grey shirt, dark trousers, brown shoes
items.append(item_of("だんせい", ["man", "male", "adult", "realistic", "男性", "おとこ"], dict(
    H=170, hh=22.7, build=1.0,
    legs=legs_std(ab=1.5), arms=arms_std(ab=7, el=12),
    face=True, hair="short",
    c=dict(skin=SKIN, top=BLUE_J, bottom=GREY_P, shoe=SHOE_BR, hair_col=HAIR_BROWN),
)))

# 2. じょせい -- a woman: long brown hair, blouse, navy knee-length skirt, red shoes
items.append(item_of("じょせい", ["woman", "female", "adult", "realistic", "女性", "おんな"], dict(
    H=168, hh=22.5, build=0.94, fem=True,
    legs=legs_std(ab=2.0), arms=arms_std(ab=8, el=10),
    face=True, hair="long", skirt=True,
    c=dict(skin=SKIN, top=BLOUSE, sleeve=BLOUSE, bottom=NAVY, thigh=SKIN, calf=SKIN, skirt=NAVY,
           shoe=RED, hair_col=HAIR_BROWN),
)))

# 3. こども -- a child, about 120 mm tall (bigger head ratio): short hair, orange T-shirt, blue trousers
items.append(item_of("こども", ["child", "kid", "boy", "girl", "realistic", "子ども", "こども"], dict(
    H=120, hh=22.0, build=0.9,
    legs=legs_std(ab=2.0), arms=arms_std(ab=10, el=14),
    face=True, hair="short",
    c=dict(skin=SKIN, top=ORANGE, sleeve=ORANGE, bottom=BLUE_J,
           shoe=WHITE, hair_col=HAIR_BROWN),
)))

# 4. ろうじん -- an elderly man, a little stooped: grey hair, green cardigan, grey trousers
items.append(item_of("ろうじん", ["elderly", "old man", "grandfather", "ろうじん", "おじいさん", "老人"], dict(
    H=166, hh=22.7, build=0.96,
    lean=9, nod=10,
    legs=legs_std(th=3, kn=6, ab=2.0), arms=arms_std(fw=8, ab=6, el=22),
    face=True, hair="grey",
    c=dict(skin=SKIN, top=GREEN_J, sleeve=GREEN_J, fore=GREEN_J, bottom=GREY_P,
           shoe=SHOE_BK, hair_col=HAIR_GREY),
)))

# 5. せんしゅ -- a runner mid-stride: red vest, black shorts, white trainers, leaning slightly forward
items.append(item_of("せんしゅ", ["runner", "running", "athlete", "sprint", "realistic", "せんしゅ", "ランナー"], dict(
    H=172, hh=22.7, build=0.92,
    lean=6,
    legs={1: dict(th=28, ab=0.0, kn=20, pt=-14),      # front leg, heel strike
          -1: dict(th=-22, ab=0.0, kn=100, pt=35)},   # back leg, toe-off and heel kicked up
    arms={1: dict(fw=-30, ab=6, el=85, wr=0.0),       # right arm back
          -1: dict(fw=48, ab=6, el=96, wr=0.0)},      # left arm forward, elbow bent
    face=True, hair="short",
    c=dict(skin=SKIN, top=RED, sleeve=RED, bottom=DARK_P, thigh=SKIN, calf=SKIN,
           shoe=WHITE, hair_col=HAIR_BLACK),
)))

# 6. すわるひと -- a seated person on a wooden block: teal shirt, blue jeans, hands on the knees
items.append(item_of("すわるひと", ["sitting", "seated", "sit", "chair", "realistic", "すわるひと", "すわる"], dict(
    H=170, hh=22.7, build=1.0,
    lean=3, pz=54.0,
    legs=legs_std(th=88, kn=88, ab=3.0),
    arms=arms_std(fw=16, ab=8, el=62),
    face=True, hair="short", block=True,
    c=dict(skin=SKIN, top=TEAL, sleeve=TEAL, bottom=BLUE_J, shoe=SHOE_BK, hair_col=HAIR_BLACK,
           block=BEIGE),
)))

# 7. あるくひと -- a person walking at a relaxed pace: grey jumper, beige trousers, arms swinging
items.append(item_of("あるくひと", ["walking", "walker", "walk", "pedestrian", "realistic", "あるくひと", "あるく"], dict(
    H=170, hh=22.7, build=1.0,
    lean=2,
    legs={1: dict(th=16, ab=2.0, kn=6, pt=-8),        # front leg, heel down
          -1: dict(th=-12, ab=2.0, kn=12, pt=18)},    # back leg, toe pushing off
    arms={1: dict(fw=-16, ab=7, el=18, wr=0.0),
          -1: dict(fw=16, ab=7, el=18, wr=0.0)},
    face=True, hair="short",
    c=dict(skin=SKIN, top=(0.58, 0.62, 0.66), sleeve=(0.58, 0.62, 0.66), bottom=BEIGE,
           shoe=SHOE_BR, hair_col=HAIR_BLACK),
)))

# 8. ボディビルダー -- a muscular bodybuilder in a red tank top and shorts, flexing both arms
items.append(item_of("ボディビルダー", ["bodybuilder", "muscle", "muscular", "fitness", "realistic", "ボディビルダー", "きんにく"], dict(
    H=172, hh=22.7, build=1.25, mus=1.3,
    legs=legs_std(ab=3.0), arms={s_: dict(fw=0, ab=38, el=130, fab=10, wr=0) for s_ in (1, -1)},
    face=True, hair="short",
    c=dict(skin=SKIN, top=WHITE, sleeve=SKIN, bottom=RED, thigh=SKIN, calf=SKIN,
           shoe=WHITE, hair_col=HAIR_BLACK),
)))

# 9. モデルたい -- a slim fashion model in contrapposto: weight on one leg, long brown hair, black top, white trousers
items.append(item_of("モデルたい", ["model", "fashion", "slim", "catwalk", "contrapposto", "realistic", "モデル"], dict(
    H=176, hh=22.4, build=0.85, fem=True,
    lean=0, sway=-4,                                   # torso shifts over the standing leg (+X side)
    legs={1: dict(th=0, ab=1.0, kn=0, pt=0),           # standing leg, straight
          -1: dict(th=8, ab=1.5, kn=12, pt=10)},       # relaxed free leg, knee soft, toe down
    arms={1: dict(fw=-4, ab=7, el=12, wr=0),
          -1: dict(fw=6, ab=10, el=22, wr=0)},
    face=True, hair="long",
    c=dict(skin=SKIN, top=(0.10, 0.10, 0.12), sleeve=(0.10, 0.10, 0.12), bottom=WHITE,
           shoe=SHOE_BK, hair_col=HAIR_BROWN),
)))

# 10. ヨガのポーズ -- tree pose: standing on the left leg, the right foot on the inner thigh, palms together overhead
items.append(item_of("ヨガのポーズ", ["yoga", "tree pose", "vrksasana", "balance", "realistic", "ヨガ", "ポーズ"], dict(
    H=170, hh=22.7, build=0.92, fem=True,
    legs={1: dict(th=0, ab=1.0, kn=0, pt=0),           # standing leg
          -1: dict(th=0, ab=45, sf=114, sab=-80, pt=0)},  # bent leg: knee out, foot on the inner thigh
    arms={1: dict(fw=166, ab=-12, el=4, wr=0),         # arms up, hands together overhead
          -1: dict(fw=166, ab=-12, el=4, wr=0)},
    face=True, hair="long",
    c=dict(skin=SKIN, top=TEAL, sleeve=TEAL, bottom=PURPLE, shoe=SKIN, hair_col=HAIR_BROWN),
)))

# 11. マネキン -- a plain grey display mannequin: no face, no hair, no clothes, a neutral stance
items.append(item_of("マネキン", ["mannequin", "dummy", "display", "figure", "realistic", "マネキン", "ボディ"], dict(
    H=172, hh=22.7, build=0.96, fem=True,
    legs=legs_std(ab=2.0), arms=arms_std(ab=12, el=8),
    face=False, hair=None,
    c=dict(skin=MANNEQUIN, top=MANNEQUIN, sleeve=MANNEQUIN, bottom=MANNEQUIN, shoe=MANNEQUIN),
)))

# 12. ダンサー -- a ballet dancer in arabesque: red leotard, black tights, hair in a bun, one leg back
items.append(item_of("ダンサー", ["dancer", "ballet", "arabesque", "dance", "realistic", "ダンサー", "バレエ"], dict(
    H=170, hh=22.7, build=0.9, fem=True,
    lean=30, nod=-12,                                   # torso reaches forward over the standing leg
    legs={1: dict(th=0, ab=1.0, kn=0, pt=0),            # standing leg
          -1: dict(th=-84, ab=0.0, kn=0, pt=0)},        # extended leg, straight and horizontal behind
    arms={1: dict(fw=70, ab=18, el=12, wr=0),           # front arm forward and a little out
          -1: dict(fw=-4, ab=70, el=8, wr=0)},          # back arm out to the side
    face=True, hair="bun",
    c=dict(skin=SKIN, top=(0.80, 0.20, 0.36), sleeve=SKIN, bottom=DARK_P, thigh=DARK_P, calf=DARK_P,
           shoe=SKIN, hair_col=SHOE_BK),
)))

if __name__ == "__main__":
    assert len(items) == 12, len(items)
    write_category("realistic.json", "リアルなひと", items)
    preview(items, outdir="/tmp/lib_preview_real", size=400)
