"""Generator for ../people.json (category ひと): 12 soft, toy-like people.

Run:  python3 people.py     (writes ../people.json; renders previews if YURACAD_BIN is set)

Every figure stands on z = 0 and faces -Y. A `Body` lays out the common skeleton (head, torso,
arms, legs, hands, feet, face) from a few proportions; each figure adds its own hair, clothes
and props. Eyes, cheeks and mouth are small blended spheres sitting on the head's front face.
"""
import math

from lib import *

# ---- colours -------------------------------------------------------------------------------
INK = (0.12, 0.10, 0.10)
MOUTH = (0.62, 0.18, 0.20)
LIPS = (0.85, 0.15, 0.25)
BLUSH = (1.0, 0.56, 0.62)
WHITE = (0.96, 0.96, 0.97)
NAVY = (0.12, 0.20, 0.42)
BLACK = (0.08, 0.08, 0.09)
GOLD = (0.95, 0.75, 0.25)
RED = (0.85, 0.14, 0.14)
SILVER = (0.72, 0.74, 0.78)
DARK_GREY = (0.24, 0.25, 0.28)
ORANGE = (1.0, 0.5, 0.1)
BROWN = (0.36, 0.22, 0.12)
BROWN_HAIR = (0.22, 0.13, 0.07)
BLACK_HAIR = (0.08, 0.07, 0.07)
DARK_HAIR = (0.12, 0.07, 0.05)
WHITE_HAIR = (0.94, 0.94, 0.92)
GREY_HAIR = (0.78, 0.78, 0.80)
ORANGE_HAIR = (0.78, 0.38, 0.14)
YELLOW_HAIR = (0.98, 0.80, 0.35)
SKIN_LIGHT = (1.0, 0.85, 0.74)
SKIN_MID = (0.98, 0.80, 0.66)
SKIN_TAN = (0.90, 0.68, 0.50)
SKIN_OLD = (0.93, 0.74, 0.58)


# ---- primitives ----------------------------------------------------------------------------
def sph(name, c, r, color, scale=(1, 1, 1), blend=6.0, **kw):
    return shape("Sphere", name, pos=c, size=(r, 1, 1), scale=scale, color=color, blend=blend, **kw)


def cap(name, p0, p1, r, color, blend=6.0, **kw):
    """Capsule whose two ball centres are p0 and p1 (radius r). Its axis is its own Z."""
    d = [p1[i] - p0[i] for i in range(3)]
    n = math.sqrt(sum(v * v for v in d))
    ux, uy, uz = (v / n for v in d)
    rx = math.degrees(math.asin(max(-1.0, min(1.0, -uy))))
    rz = math.degrees(math.atan2(ux, uz))
    mid = [(p0[i] + p1[i]) / 2 for i in range(3)]
    return shape("Capsule", name, pos=mid, size=(r, n, 1), rot=(rx, rz, 0), color=color, blend=blend, **kw)


def cyl(name, c, r, h, color, rot=(0, 0, 0), blend=2.0, **kw):
    return shape("Cylinder", name, pos=c, size=(r, h, 1), rot=rot, color=color, blend=blend, **kw)


def box(name, c, size, color, rnd=1.0, blend=2.0, **kw):
    return shape("Box", name, pos=c, size=size, round=rnd, color=color, blend=blend, **kw)


def torus(name, c, major, minor, color, rot=(0, 0, 0), blend=2.0, **kw):
    """Lies flat (XY plane) unless rot tilts it; rot (90, 0, 0) stands it up facing -Y."""
    return shape("Torus", name, pos=c, size=(major, minor, 1), rot=rot, color=color, blend=blend, **kw)


def revolve(name, c, profile, color, blend=3.0, **kw):
    """profile: [[radius, height], ...] turned about the Z axis at c."""
    return shape("Revolve", name, pos=c, size=(1, 1, 1), outline=profile, params=(360, 0, 0, 0),
                 color=color, blend=blend, **kw)


# ---- the common human skeleton -------------------------------------------------------------
class Body:
    """Proportions of one person. Z of the head centre (hc), shoulders (sh), hips (hip), hands (hz)."""

    def __init__(self, head_r=12.0, head_c=68.0, sh_z=55.0, hip_z=36.0, torso_w=10.0,
                 leg_x=5.5, arm_out=18.0, hand_z=37.0, limb_r=4.2, hand_r=3.8, foot_r=4.6):
        self.r, self.hc, self.sh, self.hip = head_r, head_c, sh_z, hip_z
        self.tw, self.lx, self.ao, self.hz = torso_w, leg_x, arm_out, hand_z
        self.lr, self.hr, self.fr = limb_r, hand_r, foot_r
        self.zc = (hip_z + sh_z) / 2                 # torso centre
        self.zh = (sh_z - hip_z) / 2 + 3.0           # torso half height

    def face(self, x, zr, out=0.0):
        """A point on the front (-Y) of the head, x sideways and zr above the head centre.
        out > 0 moves it away from the head, out < 0 sinks it in."""
        y = math.sqrt(max(0.0, self.r * self.r - x * x - zr * zr))
        return (x, -(y + out), self.hc + zr)

    def hair(self, kind, col):
        r, hc = self.r, self.hc
        if kind == "tuft":
            return [sph("まえがみ", (0, r * 0.1, hc + r * 0.95), r * 0.28, col, blend=0.8)]
        out = []
        if kind in ("short", "long", "bun"):
            out.append(sph("かみ", (0, r * 0.12, hc + r * 0.3), r * 1.06, col, scale=(1, 1, 0.78), blend=0.8))
        if kind == "long":
            out.append(sph("かみ(うしろ)", (0, r * 0.45, hc - r * 0.55), r * 0.92, col,
                           scale=(1, 0.9, 1.4), blend=1.0))
        if kind == "bun":
            out.append(sph("おだんご", (0, r * 0.6, hc + r * 1.05), r * 0.36, col, blend=0.8))
        if kind == "bald":   # white tufts over the ears and round the back, bald on top
            for sx in (-1, 1):
                side = "右" if sx > 0 else "左"
                out.append(sph(f"{side}のしらが", (sx * r * 0.92, r * 0.08, hc - r * 0.1), r * 0.32, col,
                               scale=(0.7, 1.0, 1.2), blend=1.0))
            out.append(sph("うしろのしらが", (0, r * 0.84, hc - r * 0.12), r * 0.34, col,
                           scale=(1.6, 0.9, 1.1), blend=1.0))
        return out

    def base(self, skin, shirt, pants, shoe, hair=None, hair_color=BROWN_HAIR, blush=True,
             mouth=MOUTH):
        r = self.r
        out = [
            sph("からだ", (0, 0, self.zc), self.tw, shirt, scale=(1, 0.85, self.zh / self.tw), blend=6),
            sph("あたま", (0, 0, self.hc), r, skin, blend=3),
        ]
        if hair:
            out += self.hair(hair, hair_color)
        eye_r = r * 0.17
        for sx in (-1, 1):
            side = "右" if sx > 0 else "左"
            out.append(sph(f"{side}め", self.face(sx * r * 0.34, r * 0.07, out=-0.4 * eye_r),
                           eye_r, INK, blend=0.5))
            if blush:
                cr = r * 0.16
                out.append(sph(f"{side}ほっぺ", self.face(sx * r * 0.6, -r * 0.22, out=-0.3 * cr),
                               cr, BLUSH, blend=1.5, translucency=0.3, roughness=0.7))
        mr = r * 0.1
        out.append(sph("くち", self.face(0, -r * 0.32, out=-0.2 * mr), mr, mouth,
                       scale=(1.8, 0.45, 0.6), blend=0.4))
        for sx in (-1, 1):
            side = "右" if sx > 0 else "左"
            out.append(cap(f"{side}うで", (sx * (self.tw - 1), 0, self.sh - 2), (sx * self.ao, 0, self.hz + 1),
                           self.lr, shirt, blend=5))
            out.append(sph(f"{side}て", (sx * self.ao, 0, self.hz), self.hr, skin, blend=1.5))
            out.append(cap(f"{side}あし", (sx * self.lx, 0, self.hip), (sx * self.lx, 0, self.lr),
                           self.lr, pants, blend=6))
            out.append(sph(f"{side}くつ", (sx * self.lx, -2.0, self.fr * 0.75), self.fr, shoe,
                           scale=(1, 1.6, 0.75), blend=3))
        return out


def adult(**kw):
    return Body(**kw)


def kid(**kw):
    base = dict(head_r=13, head_c=58, sh_z=44, hip_z=28, torso_w=9, leg_x=4.8, arm_out=16,
                hand_z=31, limb_r=3.6, hand_r=3.4, foot_r=4.2)
    base.update(kw)
    return Body(**base)


def item_of(name, tags, shapes):
    return item(name, tags, group(name, shapes))


items = []

# 1. おとこのこ -- a boy: spiky brown hair, blue shirt, navy trousers, red sneakers
b = kid()
S = b.base(skin=SKIN_LIGHT, shirt=(0.25, 0.55, 0.95), pants=(0.30, 0.32, 0.60),
           shoe=(0.90, 0.25, 0.20), hair="short", hair_color=BROWN_HAIR)
S.append(sph("ネクタイ", (0, -b.tw * 0.85 - 0.2, b.zc + 1), 1.6, ORANGE, blend=0.5))
items.append(item_of("おとこのこ", ["boy", "child", "kid", "男の子", "子ども"], S))

# 2. おんなのこ -- a girl: long orange hair with a bow, pink dress, purple tights
b = kid(limb_r=3.4, hand_r=3.3)
S = b.base(skin=SKIN_MID, shirt=(1.0, 0.55, 0.75), pants=(0.55, 0.40, 0.62),
           shoe=(0.30, 0.20, 0.22), hair="long", hair_color=ORANGE_HAIR)
S.append(revolve("スカート", (0, 0, 33), [[0, -9], [12, -9], [9, 8], [0, 8]], (1.0, 0.55, 0.75)))
PINK_RIBBON = (1.0, 0.35, 0.55)
S.append(sph("リボン(まん中)", (-10.2, -6.4, b.hc + 7.0), 1.5, PINK_RIBBON, blend=0.6))
for sx, nm in ((-1, "左"), (1, "右")):
    S.append(sph(f"リボン({nm}のわ)", (-10.2 + sx * 2.5, -6.4, b.hc + 7.0), 2.2, PINK_RIBBON,
                 scale=(0.9, 0.5, 0.8), blend=0.6))
items.append(item_of("おんなのこ", ["girl", "child", "kid", "女の子", "子ども"], S))

# 3. おとこのひと -- a man: short black hair, grey-blue jacket, red tie
b = adult()
S = b.base(skin=SKIN_TAN, shirt=(0.30, 0.34, 0.48), pants=(0.18, 0.18, 0.22),
           shoe=BLACK, hair="short", hair_color=BLACK_HAIR, blush=False)
S.append(cap("ネクタイ", (0, -9.0, b.zc + 5), (0, -9.0, b.zc - 6), 1.5, RED, blend=1.0))
items.append(item_of("おとこのひと", ["man", "adult", "male", "男の人", "おとこ"], S))

# 4. おんなのひと -- a woman: long dark hair, red dress, lipstick
b = adult()
S = b.base(skin=SKIN_MID, shirt=(0.92, 0.25, 0.38), pants=SKIN_MID, shoe=(0.45, 0.10, 0.15),
           hair="long", hair_color=DARK_HAIR, mouth=LIPS)
S.append(revolve("スカート", (0, 0, 36), [[0, -11], [15, -11], [10, 6], [0, 6]], (0.92, 0.25, 0.38)))
items.append(item_of("おんなのひと", ["woman", "adult", "female", "女の人", "おんな"], S))

# 5. あかちゃん -- a baby: big head, one yellow curl, blue onesie, pink booties
b = Body(head_r=16, head_c=46, sh_z=32, hip_z=19, torso_w=8.5, leg_x=4.5, arm_out=14,
         hand_z=23, limb_r=3.0, hand_r=3.2, foot_r=3.6)
S = b.base(skin=SKIN_LIGHT, shirt=(0.60, 0.85, 0.98), pants=(0.60, 0.85, 0.98),
           shoe=(1.0, 0.62, 0.72), hair="tuft", hair_color=YELLOW_HAIR)
items.append(item_of("あかちゃん", ["baby", "infant", "赤ちゃん", "あかんぼう"], S))

# 6. おじいさん -- an old man: bald with a white ring of hair, glasses, cane, brown cardigan
b = adult(head_c=66, sh_z=53, hip_z=35, torso_w=10.5, hand_z=36)
S = b.base(skin=SKIN_OLD, shirt=(0.55, 0.38, 0.24), pants=(0.50, 0.50, 0.55),
           shoe=(0.35, 0.22, 0.12), hair="bald", hair_color=WHITE_HAIR)
eye_r = b.r * 0.17
for sx in (-1, 1):
    ex, ey, ez = b.face(sx * b.r * 0.34, b.r * 0.07, out=-0.4 * eye_r)
    S.append(torus("めがね", (ex, ey - 0.9, ez), eye_r * 1.6, 0.45, DARK_GREY, rot=(90, 0, 0),
                   blend=1.0, metallic=0.6, roughness=0.3))
S.append(cap("つえ", (b.ao, -2, 1.2), (b.ao, -2, 42), 1.1, (0.45, 0.28, 0.14), blend=1.5, roughness=0.4))
items.append(item_of("おじいさん", ["grandfather", "old man", "elder", "おじいさん", "ろうじん"], S))

# 7. おばあさん -- an old woman: grey bun, glasses, teal skirt, handbag
b = adult(head_c=66, sh_z=53, hip_z=36, torso_w=10, arm_out=17, hand_z=38)
S = b.base(skin=SKIN_OLD, shirt=(0.95, 0.60, 0.70), pants=(0.30, 0.70, 0.65),
           shoe=BLACK, hair="bun", hair_color=GREY_HAIR)
S.append(revolve("スカート", (0, 0, 36), [[0, -22], [17, -22], [10, 8], [0, 8]], (0.30, 0.70, 0.65)))
S.append(box("バッグ", (b.ao + 3, -2, b.hz - 8), (10, 5, 8), BROWN, rnd=1.5, blend=3))
items.append(item_of("おばあさん", ["grandmother", "old woman", "elder", "おばあさん", "ろうじん"], S))

# 8. ロボットのひと -- a boxy silver robot: glowing eyes, antenna, grille
hc = 69.0
S = [
    box("あたま", (0, 0, hc), (22, 20, 18), SILVER, rnd=5, metallic=0.7, roughness=0.3),
    box("からだ", (0, 0, 46), (20, 14, 24), SILVER, rnd=4, metallic=0.7, roughness=0.3),
    box("くち", (0, -10.2, 64), (10, 1.2, 2.4), DARK_GREY, rnd=0.6, blend=0.5),
    cap("アンテナ", (0, 0, 78), (0, 0, 84), 1.0, DARK_GREY, blend=2),
    sph("アンテナのたま", (0, 0, 85.5), 2.2, RED, glow=2.0, blend=1),
    sph("むね", (0, -7.6, 47), 2.6, ORANGE, glow=2.0, blend=0.8),
]
for sx in (-1, 1):
    side = "右" if sx > 0 else "左"
    S.append(sph(f"{side}め", (sx * 5, -10.3, hc + 1), 2.4, (0.3, 0.9, 1.0), glow=2.5, blend=0.5))
    S.append(cyl(f"{side}みみ", (sx * 11.5, 0, hc), 3.5, 2, SILVER, rot=(0, 90, 0), blend=1.5,
                 metallic=0.7, roughness=0.3))
    S.append(cap(f"{side}うで", (sx * 11, 0, 52), (sx * 17, 0, 34), 3.0, DARK_GREY, blend=4,
                 metallic=0.9, roughness=0.35))
    S.append(sph(f"{side}て", (sx * 17, 0, 33), 3.6, SILVER, blend=2, metallic=0.7, roughness=0.3))
    S.append(cap(f"{side}あし", (sx * 5, 0, 34), (sx * 5, 0, 4), 3.4, DARK_GREY, blend=5,
                 metallic=0.9, roughness=0.35))
    S.append(box(f"{side}あし先", (sx * 5, -2, 2.5), (9, 13, 5), SILVER, rnd=2.0, blend=2,
                 metallic=0.7, roughness=0.3))
items.append(item_of("ロボットのひと", ["robot", "android", "ロボット"], S))

# 9. ぼうけんしゃ -- an adventurer: brown wide-brim hat, green shirt, tan trousers, backpack, red scarf
b = adult()
hc = b.hc
S = b.base(skin=SKIN_TAN, shirt=(0.32, 0.58, 0.30), pants=(0.76, 0.60, 0.38),
           shoe=BROWN, hair=None, blush=True)
S.append(cyl("ぼうしのつば", (0, 0, hc + 9.0), 15, 1.6, (0.50, 0.33, 0.18), blend=1.2, roughness=0.7))
S.append(cyl("ぼうし", (0, 0, hc + 13.2), 9.5, 8, (0.50, 0.33, 0.18), blend=1.0, roughness=0.7))
S.append(cyl("ぼうしのおび", (0, 0, hc + 10.2), 9.8, 1.6, RED, blend=0.5, roughness=0.6))
S.append(box("リュック", (0, 10.0, 46), (13, 5, 17), (0.60, 0.32, 0.14), rnd=2.0, blend=3))
S.append(torus("マフラー", (0, 0, 56), 7.5, 2.4, RED, blend=2))
items.append(item_of("ぼうけんしゃ", ["adventurer", "explorer", "hat", "冒険者", "ぼうけん"], S))

# 10. コック -- a chef: tall white toque, moustache, red apron
b = adult()
hc = b.hc
S = b.base(skin=SKIN_MID, shirt=WHITE, pants=DARK_GREY, shoe=BLACK, hair=None, blush=True)
S.append(cyl("コックぼうしのまき", (0, 0, hc + 8.5), 10.5, 6, WHITE, blend=1.0))
S.append(sph("コックぼうし", (0, 0, hc + 12.5), 10, WHITE, scale=(1, 1, 0.6), blend=2))
S.append(cap("ひげ", b.face(-3.5, -2.0, out=-0.2), b.face(3.5, -2.0, out=-0.2), 1.0,
             (0.25, 0.15, 0.08), blend=0.5))
S.append(box("エプロン", (0, -9.0, b.zc - 4), (13, 1.6, 19), (0.85, 0.30, 0.25), rnd=1.0, blend=1.5))
items.append(item_of("コック", ["chef", "cook", "コック", "りょうり"], S))

# 11. おまわりさん -- a policeman: navy uniform, peaked cap, belt, baton
b = adult()
hc = b.hc
S = b.base(skin=SKIN_TAN, shirt=NAVY, pants=(0.10, 0.14, 0.30), shoe=BLACK, hair=None, blush=False)
S.append(cyl("ぼうし", (0, 0, hc + 9.5), 9.8, 6, NAVY, blend=1.5))
S.append(cyl("ぼうしのおび", (0, 0, hc + 6.6), 10.1, 1.6, BLACK, blend=0.5))
S.append(box("ひさし", (0, -11.0, hc + 6.8), (13, 6, 1.0), BLACK, rnd=0.5, blend=1.0))
S.append(sph("ぼうしのバッジ", (0, -10.2, hc + 9.0), 1.8, GOLD, blend=0.5, metallic=0.9, roughness=0.25))
S.append(cyl("ベルト", (0, 0, b.zc - 6.5), 10.8, 2.6, BLACK, blend=1.0))
S.append(sph("むねのバッジ", (0, -8.1, 50), 1.5, GOLD, scale=(1, 0.3, 1), blend=0.5, metallic=0.9, roughness=0.25))
S.append(cap("けいぼう", (18.5, 0, 40), (19.5, -1, 24), 1.1, BLACK, blend=1.5))
items.append(item_of("おまわりさん", ["policeman", "police", "おまわりさん", "けいさつ"], S))

# 12. うちゅうひこうし -- an astronaut: white suit, big helmet with gold visor, backpack
b = adult(head_c=66, sh_z=55, hip_z=36, torso_w=10, arm_out=18, hand_z=37, limb_r=4.4,
          hand_r=4.0, foot_r=4.6)
hc = b.hc
S = b.base(skin=WHITE, shirt=WHITE, pants=WHITE, shoe=(0.55, 0.55, 0.60), hair=None, blush=False)
S.append(sph("ヘルメット", (0, 0, hc + 0.5), 13.5, WHITE, blend=4, roughness=0.3, coat=0.3))
S.append(sph("バイザー", (0, -9.6, hc + 0.5), 9.3, (0.28, 0.34, 0.42), scale=(1.1, 0.7, 0.95),
             blend=0.6, metallic=0.6, roughness=0.12, coat=0.8))
S.append(box("バックパック", (0, 10.5, 46), (15, 6, 18), WHITE, rnd=2.5, blend=3))
S.append(box("ぱねる", (0, -9.2, 46), (6, 1.6, 4.5), ORANGE, rnd=0.6, blend=0.5))
items.append(item_of("うちゅうひこうし", ["astronaut", "space", "helmet", "うちゅうひこうし", "うちゅう"], S))


if __name__ == "__main__":
    assert len(items) == 12, len(items)
    for it in items:
        assert len(it["group"]["shapes"]) <= 24, (it["name"], len(it["group"]["shapes"]))
    write_category("people.json", "ひと", items)
    preview(items, outdir="/tmp/lib_preview_people")
