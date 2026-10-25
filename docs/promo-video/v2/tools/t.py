import sys, os
sys.path.insert(0, "/home/claude/video/rec")
HOME = (862, 64)
TAB = {"shape": (1021, 75), "material": (1098, 75), "melt": (1170, 75), "light": (1231, 75)}
def add(app, k, pos, size, col, op="Add", blend=10, **kw):
    return app.cmd({"cmd": "shape.add", "kind": k, "pos": pos, "size": size, "color": col, "op": op, "blend": blend, **kw})
MARKS = []
def mark(name):
    import time
    MARKS.append([name, round(time.time() - MARKS_T0[0], 2)])
MARKS_T0 = [0]
def newscene(app, melt=1.2):
    r = app.cmd({"cmd": "ui.precise", "on": False})
    if r and r.get('importing'): app.wait(9)
    for i in range(6):
        r = app.cmd({"cmd": "scene.new"})
        if r is not None: break
        app.key("Escape"); app.wait(0.4)
        for (cx, cy) in [(559, 598), (557, 587), (559, 640), (557, 610)]:
            app.click(cx, cy, 1, 0.2); app.wait(0.3)
        app.wait(0.6)
    app.wait(0.6); app.cmd({"cmd": "melt.set", "amount": melt})
def home(app, w=6): app.click(*HOME, 1, 0.3); app.wait(w)
def orbit(app, dx, dy=0, secs=1.6, cx=620, cy=430): app.drag(cx, cy, cx + dx, cy + dy, secs=secs, button=2)
def slide(app, x0, x1, y, secs=1.2): app.drag(x0, y, x1, y, secs=secs, button=1)
def set_(app, g, s, **kw): app.cmd({"cmd": "shape.set", "group": g, "shape": s, **kw})
def tween(app, g, s, a, b, n=22, dt=0.04):
    for i in range(n):
        t = i / (n - 1); t = t * t * (3 - 2 * t)
        set_(app, g, s, pos=[a[j] + (b[j] - a[j]) * t for j in range(3)]); app.wait(dt)
def sc_enter(app):
    app.cmd({"cmd": "ui.precise", "on": True}); app.wait(7)
    app.click(472, 371, 1, 0.2); app.wait(0.5)   # discard the recovery prompt if shown
    app.click(185, 53, 1, 0.3); app.wait(1.0); app.click(262, 56, 1, 0.3); app.wait(2.0)   # new design
    app.click(472, 371, 1, 0.2) if False else None
def sc_leave(app):
    app.key("Escape"); app.key("Escape")
    app.cmd({"cmd": "ui.precise", "on": False}); app.wait(8)
    app.cmd({"cmd": "scene.new"}); app.wait(1)
def sc_sketch_rect(app, x0=700, y0=330, x1=860, y1=470):
    app.click(120, 92, 1, 0.3); app.wait(0.5)
    app.click(140, 135, 1, 0.4); app.wait(1.5); app.click(695, 437, 1, 0.5); app.wait(2.0)
    app.click(184, 125, 1, 0.4); app.click(x0, y0, 1, 0.4); app.click(x1, y1, 1, 0.5); app.wait(0.8)
def sc_dim(app, ex, ey, px, py, val):
    app.click(752, 125, 1, 0.3); app.click(ex, ey, 1, 0.4); app.click(px, py, 1, 0.4); app.wait(0.5)
    app.type(val, gap=0.12); app.key("Return"); app.wait(1.2)
def sc_finish_extrude(app, val="18"):
    app.key("Escape"); app.wait(0.3); app.key("Escape")
    app.click(1143, 135, 1, 0.4); app.wait(2.0)
    app.click(130, 92, 1, 0.3); app.click(220, 135, 1, 0.4); app.wait(2.0)
    app.type(val, gap=0.12); app.key("Return"); app.wait(0.8)
    app.click(1222, 677, 1, 0.4); app.wait(2.5)
