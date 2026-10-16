"""Hands, a foot and a bent arm for YURACAD category "て" -> ../hands.json.

Run:  python3 hands.py          (writes hands.json, renders /tmp/lib_preview_hands/*.png)

Geometry is built in a hand-local frame: wrist base at the origin, fingers along +Z, palm facing -Y.
Each finger is a chain of Capsules; a joint bend bends the chain toward -Y (toward the viewer, the
way a fist curls). Hand frames can be tilted/moved so the same hand works in arm poses.
"""
import math

from lib import group, item, preview, shape, write_category

SKIN = (0.93, 0.76, 0.63)
SURF = dict(roughness=0.5, coat=0.12)


# ---------- small vector / rotation helpers ----------
def rot_of(d):
    """Euler (X, Y, 0) in degrees that turns the local +Z axis onto direction d (engine: Ry*Rx)."""
    n = math.sqrt(sum(x * x for x in d))
    dx, dy, dz = d[0] / n, d[1] / n, d[2] / n
    a = math.degrees(math.asin(max(-1.0, min(1.0, -dy))))
    b = math.degrees(math.atan2(dx, dz))
    return a, b


def mat(a, b):
    """Rotation Ry(b) * Rx(a): the same matrix as the engine's rot = (a, b, 0)."""
    ca, sa = math.cos(math.radians(a)), math.sin(math.radians(a))
    cb, sb = math.cos(math.radians(b)), math.sin(math.radians(b))
    rx = [[1, 0, 0], [0, ca, -sa], [0, sa, ca]]
    ry = [[cb, 0, sb], [0, 1, 0], [-sb, 0, cb]]
    return [[sum(ry[i][k] * rx[k][j] for k in range(3)) for j in range(3)] for i in range(3)]


def mv(m, v):
    return [sum(m[i][j] * v[j] for j in range(3)) for i in range(3)]


def add(a, b):
    return [a[i] + b[i] for i in range(3)]


def sub(a, b):
    return [a[i] - b[i] for i in range(3)]


def mul(a, s):
    return [x * s for x in a]


def dist(a, b):
    return math.sqrt(sum((a[i] - b[i]) ** 2 for i in range(3)))


class Frame:
    """Local frame: origin plus the orientation that maps local +Z onto `axis`."""

    def __init__(self, origin=(0, 0, 0), axis=(0, 0, 1)):
        self.o = list(origin)
        self.a, self.b = rot_of(axis)
        self.m = mat(self.a, self.b)

    def pt(self, p):
        return add(self.o, mv(self.m, p))

    def dr(self, d):
        return mv(self.m, d)


WORLD = Frame()


# ---------- shape helpers ----------
def cap(fr, p0, p1, r, blend, name, color=SKIN):
    """Capsule whose ball centres sit at p0 and p1 (frame-local); radius r."""
    P0, P1 = fr.pt(p0), fr.pt(p1)
    d = sub(P1, P0)
    L = math.sqrt(sum(x * x for x in d))
    a, b = rot_of(d)
    return shape("Capsule", name, pos=mul(add(P0, P1), 0.5), size=(r, L, 1), rot=(a, b, 0),
                 blend=blend, color=color, **SURF)


def box(fr, c, size, rnd, blend, name, color=SKIN):
    return shape("Box", name, pos=fr.pt(c), size=size, rot=(fr.a, fr.b, 0), round=rnd,
                 blend=blend, color=color, **SURF)


# ---------- fingers ----------
def segments(base, spread, bends, lengths):
    """Finger as a planar chain. spread: degrees from +Z toward +X; bends: per-joint flex toward -Y."""
    s = math.radians(spread)
    u = [math.sin(s), 0.0, math.cos(s)]
    v = [0.0, -1.0, 0.0]
    out, P, phi = [], list(base), 0.0
    for bd, L in zip(bends, lengths):
        phi += bd
        t = math.radians(phi)
        d = [math.cos(t) * u[i] + math.sin(t) * v[i] for i in range(3)]
        Q = add(P, mul(d, L))
        out.append((P, Q))
        P = Q
    return out


def solve_bends(base, spread, lengths, target):
    """Find 3 joint bends (5 degree steps) that put the fingertip nearest to target."""
    best = None
    for b0 in range(0, 151, 5):
        for b1 in range(0, 151, 5):
            for b2 in range(0, 111, 5):
                tip = segments(base, spread, [b0, b1, b2], lengths)[-1][1]
                err = dist(tip, target) + 0.02 * (b0 + b1 + b2)
                if best is None or err < best[0]:
                    best = (err, [b0, b1, b2])
    return best[1]


# hand-local layout: knuckle (MCP) base, segment lengths, radius
FINGERS = {
    "index": ((12.5, 0, 43), [15, 10, 8.5], 3.4, "ひとさしゆび"),
    "middle": ((4.5, 0, 44), [16, 11, 9.5], 3.4, "なかゆび"),
    "ring": ((-3.5, 0, 43), [15, 10.5, 9], 3.3, "くすりゆび"),
    "pinky": ((-11.5, 0, 41), [11.5, 8, 7], 3.0, "こゆび"),
}
THUMB_BASE = (17, 0, 20)
THUMB_LEN = [12, 10]
THUMB_R = 4.0

OPEN = {"index": (6, [2, 4, 3]), "middle": (0, [2, 4, 3]), "ring": (-5, [2, 4, 3]),
        "pinky": (-13, [2, 5, 4]), "thumb": (55, [0, 0])}
CURL = (0, [85, 85, 35])


def hand(fr, pose):
    """All shapes of one hand in frame fr. pose: finger -> (spread, bends), thumb -> (spread, 2 bends)."""
    sh = [box(fr, (0, 0, 9), (22, 13, 18), 5, 6, "てくび"),
          box(fr, (0, 0, 32), (36, 15, 32), 6, 6, "てのひら")]
    for key, (base, lens, r, nm) in FINGERS.items():
        spread, bends = pose[key]
        for k, (P, Q) in enumerate(segments(base, spread, bends, lens)):
            sh.append(cap(fr, P, Q, r, 4, f"{nm}{k + 1}"))
    spread, bends = pose["thumb"]
    tb = pose.get("thumb_base", THUMB_BASE)
    for k, (P, Q) in enumerate(segments(tb, spread, bends, THUMB_LEN)):
        sh.append(cap(fr, P, Q, THUMB_R, 4, f"おやゆび{k + 1}"))
    return sh


def tip_of_thumb(spread, bends):
    return segments(THUMB_BASE, spread, bends, THUMB_LEN)[-1][1]


def touching_pose(thumb_spread, thumb_bends, others):
    """Index fingertip solved to meet the thumb tip; `others` gives the other three fingers."""
    base, lens, _, _ = FINGERS["index"]
    idx = solve_bends(base, 15, lens, tip_of_thumb(thumb_spread, thumb_bends))
    pose = dict(others)
    pose["index"] = (15, idx)
    pose["thumb"] = (thumb_spread, thumb_bends)
    return pose


def fist_pose(**over):
    pose = {"index": CURL, "middle": CURL, "ring": CURL, "pinky": CURL, "thumb": (25, [60, 50])}
    pose.update(over)
    return pose


# ---------- items ----------
items = []

# 1 open hand
items.append(item("ひらいたて", ["open hand", "hand", "palm", "てのひら", "wave"],
                  group("ひらいたて", hand(WORLD, OPEN))))

# 2 fist
items.append(item("グー", ["fist", "rock", "rock paper scissors", "こぶし"],
                  group("グー", hand(WORLD, fist_pose()))))

# 3 peace sign
peace = fist_pose(index=(14, [0, 0, 0]), middle=(-9, [0, 0, 0]), thumb=(45, [60, 50]))
items.append(item("ピース", ["peace", "victory", "v sign", "two", "ピースサイン"],
                  group("ピース", hand(WORLD, peace))))

# 4 thumbs up
thumbs_up = fist_pose(thumb=(4, [0, 0]), thumb_base=(19, 0, 31))
items.append(item("サムズアップ", ["thumbs up", "thumb", "good", "like", "いいね"],
                  group("サムズアップ", hand(WORLD, thumbs_up))))

# 5 pointing
pointing = fist_pose(index=(4, [0, 0, 0]), thumb=(50, [70, 40]))
items.append(item("ゆびさし", ["pointing", "point", "index finger", "指差し"],
                  group("ゆびさし", hand(WORLD, pointing))))

# 6 OK sign: index meets thumb, other three open
ok_pose = touching_pose(70, [20, 20], {"middle": (0, [2, 3, 2]), "ring": (-5, [2, 3, 2]),
                                       "pinky": (-13, [2, 4, 3])})
items.append(item("OK", ["ok", "okay", "circle sign", "OKサイン"],
                  group("OK", hand(WORLD, ok_pose))))

# 7 rock sign (horns)
rock = fist_pose(index=(8, [0, 0, 0]), pinky=(-14, [0, 0, 0]), thumb=(50, [60, 50]))
items.append(item("ロックサイン", ["rock sign", "horns", "metal", "ロック"],
                  group("ロックサイン", hand(WORLD, rock))))

# 8 wave: open hand tilted sideways
wave = Frame(axis=(0.4, 0, 0.92))
# keep the tilted wrist block standing on z = 0
wave.o[2] -= min(wave.pt([x, y, z])[2] for x in (-11, 11) for y in (-6.5, 6.5) for z in (0, 18))
items.append(item("バイバイ", ["wave", "bye bye", "おいでおいで", "come here", "手を振る"],
                  group("バイバイ", hand(wave, OPEN))))

# 9 pinch: thumb and index tip meet, other fingers curled
pinch = touching_pose(25, [45, 30], {"middle": (-2, [30, 40, 20]), "ring": (-6, [35, 45, 25]),
                                     "pinky": (-12, [40, 50, 25])})
items.append(item("つまむて", ["pinch", "pinching", "fingers", "つまむ"],
                  group("つまむて", hand(WORLD, pinch))))

# 10 raised fist on a forearm, elbow on the ground
ELBOW_UP, WRIST_UP = (-8, 0, 8), (2, 0, 52)
fist_up_frame = Frame(origin=WRIST_UP, axis=(0.2, 0, 0.98))
fist_up = hand(fist_up_frame, fist_pose())
fist_up.append(cap(WORLD, ELBOW_UP, WRIST_UP, 8, 6, "うでのほね"))
items.append(item("こぶしあげ", ["raised fist", "fist up", "fight", "power", "ガッツ"],
                  group("こぶしあげ", fist_up)))

# 11 foot: sole on the ground, toes toward -Y
foot = [
    box(WORLD, (0, 32, 18), (40, 42, 36), 15, 6, "かかと"),
    box(WORLD, (0, -2, 14), (44, 40, 28), 12, 6, "土ふまず"),
    box(WORLD, (0, -36, 11), (50, 32, 22), 11, 6, "まえあし"),
    shape("Sphere", "甲", pos=(0, -6, 22), size=(18, 1, 1), scale=(1.0, 1.2, 0.6),
          blend=6, color=SKIN, **SURF),
    cap(WORLD, (0, 22, 34), (0, 22, 50), 13, 6, "足首"),
]
for name, x, L, r in [("おおゆび", 16, 14, 6.2), ("ひとゆび", 9, 12, 5.0),
                      ("なかゆび", 2.5, 10.5, 4.6), ("くすりゆび", -4, 9.5, 4.3), ("こゆび", -10, 8, 3.8)]:
    foot.append(cap(WORLD, (x, -46, r), (x, -46 - L, r), r, 5, name))
items.append(item("あし", ["foot", "feet", "ashi", "toes", "sole"],
                  group("あし", foot)))

# 12 bent arm: elbow low, upper arm up to the shoulder, forearm up to an open hand
ELBOW, SHOULDER, WRIST = (-6, 0, 9.5), (-30, 0, 42), (14, 0, 44)
arm_frame = Frame(origin=WRIST, axis=sub(WRIST, ELBOW))
arm = hand(arm_frame, OPEN)
arm.append(cap(WORLD, ELBOW, SHOULDER, 9.5, 6, "上うで"))
arm.append(cap(WORLD, ELBOW, WRIST, 8, 6, "ひじから先"))
items.append(item("うで", ["arm", "bent arm", "elbow", "flex", "うでまげ"],
                  group("うで", arm)))

if __name__ == "__main__":
    write_category("hands.json", "て", items)
    preview(items, outdir="/tmp/lib_preview_hands")
