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
