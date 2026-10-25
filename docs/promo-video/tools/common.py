CHAR = "/home/claude/video/char.meltcad"
TAB = {"shape": (1021, 75), "material": (1098, 75), "melt": (1170, 75), "light": (1231, 75)}
def load_char(app):
    for _ in range(5):
        app.cmd({"cmd": "scene.new"}); app.wait(0.5)
        r = app.cmd({"cmd": "load", "path": CHAR})
        if r and r.get("shape_count") == 68: break
        app.wait(2)
    app.wait(1.5)
    app.click(862, 64, 1, 0.5); app.wait(8)
    for x in (700, 640, 700): app.move(x, 420, 0.4); app.wait(1.0)
def orbit(app, dx, dy=0, secs=2.0, cx=620, cy=420):
    app.drag(cx, cy, cx + dx, cy + dy, secs=secs, button=2)
def zoom(app, n, up=True):
    app.move(620, 420, 0.4); app.wheel(n, up)
