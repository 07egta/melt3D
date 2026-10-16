"""プリントインプレース (mechanisms printed already assembled) -> ../pip.json

Run:  YURACAD_BIN=/path/to/yuracad python3 pip.py
Previews go to /tmp/lib_preview_pip when YURACAD_BIN is set (the newest model only).

Rules used here:
- Moving parts sit 0.6 mm apart (GAP). Every part has blend 0, so a gap is never bridged.
- A Subtract only carves the shapes listed BEFORE it, so each cut is listed right after the
  part it should carve, and parts that must not be carved are listed after the cut.
- Box/Cylinder/Tube/Extrude/Torus are centred on pos. Cylinder and Tube size = (radius, height, _)
  and (outer radius, wall, height). rot (0, 90, 0) turns a Z-axis part along X, rot (90, 0, 0) along Y.
- Hemi: its flat face runs through local z = -r/2 and the dome goes up; rot 180 about X makes a bowl.
"""
import math
import os
from lib import *

# ---- materials (values follow src/materials.rs presets, as in tools_a.py) ----
STEEL = dict(color=(0.74, 0.76, 0.80), roughness=0.28, metallic=1.0)
CHROME = dict(color=(0.90, 0.92, 0.96), roughness=0.08, metallic=1.0)
BRASS = dict(color=(0.86, 0.66, 0.32), roughness=0.30, metallic=1.0)
BLACK = dict(color=(0.07, 0.07, 0.08), roughness=0.4)
RED = dict(color=(0.85, 0.18, 0.15), roughness=0.5)
ORANGE = dict(color=(0.96, 0.46, 0.13), roughness=0.45)
BLUE = dict(color=(0.20, 0.45, 0.85), roughness=0.45)
GREEN = dict(color=(0.30, 0.72, 0.38), roughness=0.5)
GREY = dict(color=(0.56, 0.58, 0.61), roughness=0.55)

GAP = 0.6  # clearance between moving parts (mm); the brief asks for 0.5-0.8


def P(kind, name, pos, size=None, mat=GREY, blend=0.0, **kw):
    """A solid (Add) part with hard edges."""
    return shape(kind, name, pos=pos, size=size, blend=blend, **mat, **kw)


def Sub(kind, name, pos, size=None, **kw):
    """A cut. It carves only the shapes listed before it in the group."""
    return shape(kind, name, pos=pos, size=size, blend=0.0, op="Subtract", **kw)


def turn_x(v, a):
    """Turn vector v about the X axis by a degrees, as the app does for rot=(a, 0, 0)."""
    c, s = math.cos(math.radians(a)), math.sin(math.radians(a))
    return (v[0], v[1] * c - v[2] * s, v[1] * s + v[2] * c)


def turn_about(pivot, pt, a):
    """Turn point pt=(x, y, z) about the X-parallel axis through pivot=(y, z) by a degrees."""
    d = turn_x((0.0, pt[1] - pivot[0], pt[2] - pivot[1]), a)
    return (pt[0], pivot[0] + d[1], pivot[1] + d[2])


def dome(name, flat_centre, r, a=0.0, down=False, mat=GREY, **kw):
    """A hemisphere whose flat face is centred on flat_centre (dome up, or down when down=True).
    The Hemi's pos is r/2 away from its flat face, so the offset is turned with the shape."""
    ang = a + (180.0 if down else 0.0)
    off = turn_x((0.0, 0.0, -r / 2), ang)
    pos = tuple(f - o for f, o in zip(flat_centre, off))
    return P("Hemi", name, pos, (r, 1, 1), mat=mat, rot=(ang, 0, 0), **kw)


def helix(r, z0, z1, turns, tube, n=32):
    """Thread path (x, y, z, radius) around the Z axis, at most 32 nodes."""
    pts = []
    for i in range(n):
        t = i / (n - 1)
        a = 2 * math.pi * turns * t
        pts.append([r * math.cos(a), r * math.sin(a), z0 + (z1 - z0) * t, tube])
    return pts


items = []


def save():
    write_category("pip.json", "プリントインプレース", items)
    if os.environ.get("YURACAD_BIN") and items:
        preview(items, outdir="/tmp/lib_preview_pip", size=360, only=[items[-1]["name"]])


# 1. ちょうつがいのはこ: an open box whose lid is lifted 40 degrees on a pin held by two end ears.
#    The lid's sleeve turns around the pin with 0.6 mm clearance; the ears stop at 0.6 mm from it.
def model_box():
    w, d, h = 60.0, 40.0, 24.0
    pv = (24.5, 24.6)                     # hinge axis (y, z), just behind the back wall
    a = -40.0                             # lid angle (negative lifts the front edge)
    sh = [
        P("Box", "はこ", (0, 0, h / 2), (w, d, h), mat=ORANGE, round=1.0),
        Sub("Box", "中のくぼみ", (0, 0, 14.5), (55, 35, 24)),
        P("Box", "はこの耳L", (-15.8, 21.25, 21), (8.4, 6.5, 6), mat=ORANGE),
        P("Box", "はこの耳R", (15.8, 21.25, 21), (8.4, 6.5, 6), mat=ORANGE),
        P("Cylinder", "ピン", (0, pv[0], pv[1]), (2.0, 40, 1), mat=CHROME, rot=(0, 90, 0)),
        P("Box", "ふた", turn_about(pv, (0, 0, 26.1), a), (w, d, 3), mat=BLUE, round=1.0, rot=(a, 0, 0)),
        P("Box", "ふたのうで", turn_about(pv, (0, 21.75, 26.1), a), (22, 5.5, 3), mat=BLUE, rot=(a, 0, 0)),
        P("Tube", "ふたのそで", (0, pv[0], pv[1]), (3.5, 0.9, 22), mat=BLUE, rot=(0, 90, 0)),
    ]
    return item("ちょうつがいのはこ", ["box", "hinge", "hinged lid", "print in place", "はこ", "ちょうつがい", "ふた"],
                group("ちょうつがいのはこ", sh))


items.append(model_box())
save()


# ---- hinge builder (ヒンジ and ドアのちょうつがい) ----
def hinge_shapes(mode, u0, L, leaf, mat_a, mat_b, R=4.0, rp=1.5, t=2.0):
    """Two leaves with five interleaved knuckles (A B A B A) on a pin.
    mode 'flat': the axis runs along X, both leaves lie flat on the bed, knuckles at z = R.
    mode 'door': the axis runs along Z, both leaves stand on the bed, the axis at x = y = 0.
    A knuckles are solid and carry the pin; B knuckles are tubes around it (clearance GAP)."""
    flat = mode == "flat"

    def W(u, v, w):                       # (along axis, along leaf, normal) -> world
        return (u, v, w) if flat else (v, w, u)

    wa = R if flat else 0.0               # height of the knuckle axis in the normal direction
    wl = 0.0 if flat else -t / 2          # leaf bottom face (flat) or centred (door)
    rot_axis = (0, 90, 0) if flat else (0, 0, 0)

    def box(name, u_a, u_b, v_a, v_b, mat, **kw):
        size = (u_b - u_a, v_b - v_a, t) if flat else (v_b - v_a, t, u_b - u_a)
        c = W((u_a + u_b) / 2, (v_a + v_b) / 2, wl + t / 2)
        return P("Box", name, c, size, mat=mat, **kw)

    def axial(kind, name, uc, length, rad, mat, wall=None):
        size = (rad, length, 1) if kind == "Cylinder" else (rad, wall, length)
        return P(kind, name, W(uc, 0, wa), size, mat=mat, rot=rot_axis)

    def hole(u, v):
        c = W(u, v, wl + t / 2)
        if flat:
            return Sub("Cylinder", "ねじ穴", c, (2.0, t + 4, 1))
        return Sub("Cylinder", "ねじ穴", c, (2.0, t + 4, 1), rot=(90, 0, 0))

    n = 5
    ws = (L - (n - 1) * GAP) / n
    segs, u = [], u0
    for i in range(n):
        segs.append(("A" if i % 2 == 0 else "B", u, u + ws))
        u += ws + GAP

    sh = [box("はねA", u0, u0 + L, -leaf, -(R + GAP), mat_a),
          box("はねB", u0, u0 + L, R + GAP, leaf, mat_b)]
    for u in (u0 + L * 0.25, u0 + L * 0.75):          # screw holes in the leaves
        for v in (-(leaf + R + GAP) / 2, (leaf + R + GAP) / 2):
            sh.append(hole(u, v))
    for owner, a, b in segs:                          # knuckles, pin and the tongues that join them
        uc = (a + b) / 2
        if owner == "A":
            sh.append(axial("Cylinder", "くびれA", uc, b - a, R, mat_a))
            sh.append(box("つなぎA", a, b, -(R + 1), -(R - 1.5), mat_a))
        else:
            sh.append(axial("Tube", "くびれB", uc, b - a, R, mat_b, wall=R - (rp + GAP)))
            sh.append(box("つなぎB", a, b, R - 1.5, R + 1, mat_b))
    sh.append(axial("Cylinder", "ピン", u0 + L / 2, L, rp, CHROME))
    return sh


# 2. ヒンジ: a flat butterfly hinge printed open at 180 degrees, leaves on the bed.
def model_hinge():
    return item("ヒンジ", ["hinge", "butterfly hinge", "print in place", "ヒンジ", "ちょうつがい", "つなぎ"],
                group("ヒンジ", hinge_shapes("flat", -20, 40, 30, STEEL, BRASS)))


items.append(model_hinge())
save()


# 3. くさり: five interlocked links. Ring k lies in XZ (even k) or YZ (odd k), its centre 14 mm above
#    the last one, so each ring passes through the hole of its neighbour (centre lines 6 mm apart).
def model_chain():
    R, r, pitch = 10.0, 2.2, 14.0
    sh = []
    for k in range(5):
        z = R + r + pitch * k
        even = k % 2 == 0
        sh.append(P("Torus", f"くさりの輪{k + 1}", (0, 0, z), (R, r, 1),
                    mat=CHROME if even else BRASS, rot=(90, 0, 0) if even else (0, 90, 0)))
    return item("くさり", ["chain", "links", "interlocked", "print in place", "くさり", "チェーン", "輪"],
                group("くさり", sh))


items.append(model_chain())
save()


# 4. ボールジョイント: a ball on a stem sits in a socket hollowed with a 0.6 mm gap. The socket's top
#    opening (r 5) is smaller than the ball (r 8), so the ball is kept in; the stem (r 3.5) passes out.
def model_ball_joint():
    cz = 15.0
    sh = [
        P("Box", "うけざら", (0, 0, 15), (30, 30, 30), mat=ORANGE, round=1.0),
        Sub("Sphere", "玉のくぼみ", (0, 0, cz), (8.6, 1, 1)),      # radius 8 + 0.6
        Sub("Cylinder", "玉の口", (0, 0, 26), (5.0, 22, 1)),       # opening through the top
        Sub("Cylinder", "前の窓", (0, -15, cz), (6.0, 20, 1), rot=(90, 0, 0)),  # window smaller than the ball
        P("Sphere", "玉", (0, 0, cz), (8.0, 1, 1), mat=CHROME),
        P("Cylinder", "棒", (0, 0, 26), (3.5, 22, 1), mat=CHROME),
        P("Box", "うで", (0, 0, 39.5), (30, 6, 6), mat=BLUE, round=1.0),
        P("Sphere", "うでの先L", (-15, 0, 39.5), (4.0, 1, 1), mat=BLUE),
        P("Sphere", "うでの先R", (15, 0, 39.5), (4.0, 1, 1), mat=BLUE),
    ]
    return item("ボールジョイント", ["ball joint", "socket", "ball and socket", "print in place", "ボール", "ジョイント", "玉"],
                group("ボールジョイント", sh))


items.append(model_ball_joint())
save()


# 5. ギアのくみあわせ: two 12-tooth gears (module 2) with 24.6 mm between centres, each on a shaft
#    through its bore (0.6 mm clearance) and held by a cap; both sit 0.6 mm above the base plate.
def gear_outline(n, r_out, r_root, offset_deg):
    step = 2 * math.pi / n
    tip, root = step * 0.16, step * 0.25
    pts = []
    for i in range(n):
        a = math.radians(offset_deg) + i * step
        pts += [[r_root * math.cos(a - root), r_root * math.sin(a - root)],
                [r_out * math.cos(a - tip), r_out * math.sin(a - tip)],
                [r_out * math.cos(a + tip), r_out * math.sin(a + tip)],
                [r_root * math.cos(a + root), r_root * math.sin(a + root)]]
    return pts


def model_gears():
    xa, xb, zg = -12.3, 12.3, 6.6                       # gear centres and mid-height (3.6 .. 9.6)
    sh = [
        P("Extrude", "歯車A", (xa, 0, zg), (6, 1, 1), mat=RED, outline=gear_outline(12, 13.0, 10.5, 0)),
        Sub("Cylinder", "軸あなA", (xa, 0, zg), (3.1, 10, 1)),
        P("Extrude", "歯車B", (xb, 0, zg), (6, 1, 1), mat=BLUE, outline=gear_outline(12, 13.0, 10.5, 15)),
        Sub("Cylinder", "軸あなB", (xb, 0, zg), (3.1, 10, 1)),
        P("Box", "だい", (0, 0, 1.5), (80, 40, 3), mat=GREY, round=1.0),
    ]
    for x, nm in ((xa, "A"), (xb, "B")):
        sh.append(P("Cylinder", f"軸{nm}", (x, 0, 6.1), (2.5, 12.2, 1), mat=CHROME))
        sh.append(P("Cylinder", f"ふた{nm}", (x, 0, 11.2), (4.5, 2.0, 1), mat=CHROME))
    return item("ギアのくみあわせ", ["gears", "gear pair", "meshing gears", "print in place", "ギア", "歯車", "くみあわせ"],
                group("ギアのくみあわせ", sh))


items.append(model_gears())
save()


# 6. ベアリング: outer race (r 16 inside), inner race (r 10 outside) and eight 2.5 mm balls on r 13.
#    The balls run in pockets of a cage ring, 0.5 mm clear of the balls, so nothing touches.
def model_bearing():
    zc, rb = 3.5, 13.0
    sh = [P("Tube", "保持器", (0, 0, zc), (14.5, 3, 5), mat=ORANGE)]
    balls = []
    for k in range(8):
        a = math.radians(45 * k)
        c = (rb * math.cos(a), rb * math.sin(a), zc)
        sh.append(Sub("Sphere", f"ボールのあな{k + 1}", c, (3.0, 1, 1)))   # cut into the cage only
        balls.append(P("Sphere", f"ボール{k + 1}", c, (2.5, 1, 1), mat=CHROME))
    sh += balls
    sh.append(P("Tube", "外のわ", (0, 0, zc), (20, 4, 7), mat=STEEL))
    sh.append(Sub("Box", "のぞき窓", (0, -20.7, zc), (10, 10, 5)))   # slot in the front of the outer race
    sh.append(P("Tube", "内のわ", (0, 0, zc), (10, 3, 7), mat=BRASS))
    return item("ベアリング", ["bearing", "ball bearing", "race", "print in place", "ベアリング", "玉軸受", "ボール"],
                group("ベアリング", sh))


items.append(model_bearing())
save()


# 7. スライダー: a block with a 0.6 mm bore slides along a rail held by two posts on a base.
def model_slider():
    sh = [
        P("Box", "スライダー", (0, 0, 16), (24, 16, 16), mat=ORANGE, round=1.0),
        Sub("Cylinder", "レールのあな", (0, 0, 16), (4.6, 40, 1), rot=(0, 90, 0)),   # bore radius 4 + 0.6
        P("Box", "つまみ", (0, 0, 27), (10, 10, 6), mat=BLUE, round=1.0),
        P("Box", "だい", (0, 0, 1.5), (90, 30, 3), mat=GREY, round=0.5),
        P("Box", "はしらL", (-30, 0, 9.5), (6, 12, 13), mat=GREY),
        P("Box", "はしらR", (30, 0, 9.5), (6, 12, 13), mat=GREY),
        P("Cylinder", "レール", (0, 0, 16), (4.0, 62, 1), mat=CHROME, rot=(0, 90, 0)),
    ]
    return item("スライダー", ["slider", "rail", "sliding block", "print in place", "スライダー", "レール", "すべり"],
                group("スライダー", sh))


items.append(model_slider())
save()


# 8. スピナー (fidget spinner): three weighted lobes on a hub that turns around a post. The hub
#    sits 0.6 mm above the base and under a cap on the post; its bore is 0.6 mm wider than the post.
def model_spinner():
    zb = 7.4                                          # hub mid-height (hub 3.6 .. 11.2)
    sh = [P("Tube", "ハブ", (0, 0, zb), (7, 2.4, 7.6), mat=RED)]
    for i, ang in enumerate((0, 120, 240)):
        a = math.radians(ang)
        sh.append(P("Box", f"うで{i + 1}", (12 * math.cos(a), 12 * math.sin(a), zb), (14, 6, 7.6),
                    mat=RED, round=1.0, rot=(0, 0, ang)))
    for i, ang in enumerate((0, 120, 240)):
        a = math.radians(ang)
        c = (18 * math.cos(a), 18 * math.sin(a), zb)
        sh.append(P("Cylinder", f"おもり{i + 1}", c, (8, 7.6, 1), mat=RED))
    for i, ang in enumerate((0, 120, 240)):          # weight holes: carve the lobe and arm only
        a = math.radians(ang)
        sh.append(Sub("Cylinder", f"おもりのあな{i + 1}", (18 * math.cos(a), 18 * math.sin(a), zb), (3, 10, 1)))
    sh += [
        P("Cylinder", "台", (0, 0, 1.5), (34, 3, 1), mat=GREY),
        P("Cylinder", "くい", (0, 0, 7), (4, 14, 1), mat=GREY),
        P("Cylinder", "ぼうし", (0, 0, 12.9), (6, 2.2, 1), mat=GREY),
    ]
    return item("スピナー", ["spinner", "fidget spinner", "fidget", "print in place", "スピナー", "ファイドゲット", "こま"],
                group("スピナー", sh))


items.append(model_spinner())
save()
