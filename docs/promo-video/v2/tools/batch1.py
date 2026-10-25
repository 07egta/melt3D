from t import *
# ---- each take: (name, setup, run)
def T01s(app): newscene(app, 1.0)
def T01(app):
    app.wait(0.8)
    for k, p, c in [("Sphere", [-30, 0, 26], [0.3, .85, .6]), ("Box", [26, 0, 18], [1, .6, .3]), ("Torus", [0, 0, 8], [.35, .6, 1])]:
        add(app, k, p, [16, 16, 16] if k != "Torus" else [30, 6, 6], c); app.wait(0.5); app.click(*HOME, 1, 0.3); app.wait(1.0)
    tween(app, 0, 0, [-30, 0, 26], [-8, 0, 28]); tween(app, 0, 1, [26, 0, 18], [8, 0, 20])
    app.click(*TAB["melt"], 1, 0.4); app.wait(0.5); app.click(1210, 153, 1, 0.4); app.wait(1.2)
    orbit(app, 190, 30, 2.0); app.click(1067, 153, 1, 0.4); app.wait(1.0)
def T03s(app):
    newscene(app, 1.4); add(app, "Sphere", [-8, 0, 22], [17, 17, 17], [.3, .85, .6]); add(app, "Box", [14, 0, 14], [14, 14, 14], [1, .6, .3], blend=14); home(app, 6)
def T03(app):
    app.wait(0.6); app.click(*TAB["shape"], 1, 0.4); app.wait(1.0)
    for x in (1103, 1139, 1175, 1067): app.click(x, 222, 1, 0.45); app.wait(1.1)
    app.click(1031, 222, 1, 0.4); app.wait(0.8); orbit(app, 160, 20, 2.0)
def T04s(app): newscene(app, 1.0)
def T04(app):
    app.wait(0.8)
    for k, p, c in [("Sphere", [-40, 0, 20], [.95, .45, .6]), ("Cone", [0, 0, 20], [.4, .75, 1]), ("Cylinder", [40, 0, 20], [1, .8, .3])]:
        add(app, k, p, [16, 16, 16], c, blend=6); app.wait(0.4); app.click(*HOME, 1, 0.3); app.wait(0.9)
    app.click(92, 197, 1, 0.5); app.wait(0.6)
    app.drag(1070, 330, 1104, 330, secs=0.6); 
    tween(app, 0, 1, [0, 0, 20], [0, -28, 36]); tween(app, 0, 2, [40, 0, 20], [40, 22, 20]); orbit(app, -140, 20, 1.8)
def T05s(app): newscene(app, 1.2); add(app, "Sphere", [-46, 0, 30], [18, 18, 18], [.3, .85, .6]); add(app, "Sphere", [46, 0, 30], [18, 18, 18], [1, .45, .6]); home(app, 6)
def T05(app):
    app.wait(0.6); app.click(92, 229, 1, 0.5); app.wait(0.8)
    tween(app, 0, 1, [46, 0, 30], [14, 0, 30], 34, 0.05); app.wait(0.3); tween(app, 0, 0, [-46, 0, 30], [-14, 0, 30], 28, 0.05)
    orbit(app, 150, 20, 1.8); tween(app, 0, 1, [14, 0, 30], [30, 0, 34], 18, 0.05); tween(app, 0, 1, [30, 0, 34], [10, 0, 30], 18, 0.05)
def T06s(app): newscene(app, 1.6); add(app, "Cylinder", [0, 0, 14], [16, 16, 14], [.35, .6, 1]); add(app, "Sphere", [0, 0, 40], [17, 17, 17], [1, .55, .75], blend=14); home(app, 6)
def T06(app):
    app.wait(0.6); app.click(*TAB["melt"], 1, 0.4); app.wait(0.8)
    slide(app, 1105, 1190, 366, 1.0); app.wait(0.5); slide(app, 1190, 1085, 366, 1.0); app.wait(0.5); slide(app, 1085, 1170, 366, 0.9)
    app.app_zoom = None; app.move(620, 430, 0.3); app.wheel(5, True); app.wait(0.4); orbit(app, 160, -10, 1.8); app.wheel(5, False)
def T07s(app): newscene(app, 1.0); add(app, "Sphere", [-14, 0, 20], [17, 17, 17], [.3, .85, .6]); add(app, "Sphere", [14, 0, 20], [17, 17, 17], [.4, .6, 1]); home(app, 6)
def T07(app):
    app.wait(0.5); app.click(*TAB["melt"], 1, 0.4); app.wait(0.7)
    for (x, y) in [(1128, 329), (1218, 329), (1035, 329)]:
        app.click(x, y, 1, 0.4); app.wait(0.6); slide(app, 1090, 1180, 366, 0.7); app.wait(0.4); slide(app, 1180, 1100, 366, 0.7)
    slide(app, 1077, 1160, 400, 0.8); app.wait(1.0)
def T08s(app): newscene(app, 1.2); add(app, "Box", [0, 0, 14], [20, 20, 14], [1, .6, .3]); add(app, "Sphere", [0, 0, 38], [14, 14, 14], [.3, .85, .6]); home(app, 6)
def T08(app):
    app.wait(0.5); app.click(*TAB["shape"], 1, 0.4); app.click(83, 229, 1, 0.4) if False else None; app.wait(0.5)
    for v in ["30", "10", "44", "20"]:
        app.click(1103, 330, 2, 0.3) if False else None
        app.drag(1103, 330, 1103, 330, 0.1)
        app.key("a", ctrl=True); app.type(v, gap=0.1); app.key("Return"); app.wait(0.9)
    orbit(app, 140, 10, 1.6)
def T09s(app): newscene(app, 1.0); add(app, "Box", [0, 0, 22], [38, 28, 22], [1, .6, .3], blend=3); home(app, 6)
def T09(app):
    app.wait(0.5)
    add(app, "Sphere", [0, 0, 90], [24, 24, 24], [.5, .5, .5], op="Subtract", blend=3); app.wait(0.5)
    tween(app, 0, 1, [0, 0, 90], [0, 0, 40], 30, 0.05); app.wait(0.3); tween(app, 0, 1, [0, 0, 40], [14, 0, 40], 14, 0.05)
    orbit(app, 190, 30, 2.0)
def T10s(app): newscene(app, 1.0); add(app, "Torus", [0, 0, 20], [38, 14, 14], [.95, .45, .6], blend=2); home(app, 6)
def T10(app):
    app.wait(0.4); add(app, "Cylinder", [-22, -80, 20], [7, 7, 90], [.5, .5, .5], op="Subtract", blend=2, rot=[90, 0, 0]); app.wait(0.4)
    for x in (-22, 0, 22, 0):
        tween(app, 0, 1, [0 if False else x, -0, 20], [x, 0, 20], 2)
        set_(app, 0, 1, pos=[x, 0, 20]); app.wait(0.7)
    orbit(app, 170, 25, 2.0)
BATCH = [("T01", T01s, T01), ("T03", T03s, T03), ("T04", T04s, T04), ("T05", T05s, T05), ("T06", T06s, T06), ("T07", T07s, T07), ("T08", T08s, T08), ("T09", T09s, T09), ("T10", T10s, T10)]
