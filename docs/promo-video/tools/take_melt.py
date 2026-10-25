"""Real window: shapes slide together and melt; melt amount sweeps; orbit."""
def run(app):
    import sys; sys.path.insert(0, "/home/claude/video/rec")
    app.wait(1.5)
    app.cmd({"cmd": "scene.new"})
    app.cmd({"cmd": "melt.set", "amount": 1.0})
    def add(k, pos, size, col, **kw):
        app.cmd({"cmd": "shape.add", "kind": k, "pos": pos, "size": size, "color": col, "op": "Add", "blend": 12, **kw})
    add("Sphere", [-46, 0, 30], [18, 18, 18], [0.25, 0.8, 0.55])
    app.wait(0.5)
    add("Sphere", [46, 0, 30], [18, 18, 18], [1.0, 0.45, 0.6])
    app.wait(0.5)
    add("Torus", [0, 0, 12], [30, 7, 7], [0.3, 0.6, 1.0])
    app.wait(0.8)
    for i in range(40):  # slide the two balls together
        t = i / 39
        x = 46 - 36 * t
        app.cmd({"cmd": "shape.set", "group": 0, "shape": 0, "pos": [-x, 0, 30 + 6 * t]})
        app.cmd({"cmd": "shape.set", "group": 0, "shape": 1, "pos": [x, 0, 30 + 6 * t]})
        app.wait(0.04)
    for a in [0.2, 1.6, 0.5, 2.2, 1.0]:  # melt amount sweep
        for j in range(10):
            app.cmd({"cmd": "melt.set", "amount": a * (j + 1) / 10 + 0.0})
            app.wait(0.05)
        app.wait(0.4)
    app.cmd({"cmd": "melt.set", "drip": 8, "ripple": 0.8})
    app.drag(560, 380, 760, 340, secs=2.2, button=2)
    app.wait(1.0)
