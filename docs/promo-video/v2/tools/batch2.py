from t import *
def closeanim(app):
    app.click(1241, 738, 1, 0.3); app.wait(0.6)
def base(app, melt=1.2):
    newscene(app, melt); add(app, "Box", [0, 0, 14], [26, 26, 14], [1, .6, .3]); add(app, "Sphere", [0, 0, 36], [16, 16, 16], [.3, .85, .6]); home(app, 6); closeanim(app)
def T11(app):
    app.wait(0.5); app.click(*TAB["material"], 1, 0.4); app.wait(0.7)
    for x, y in [(1049, 378), (1141, 378), (1233, 378), (1096, 423), (1004, 469)]:
        app.click(x, y, 1, 0.4); app.wait(0.6)
    app.click(1073, 300, 1, 0.4); app.wait(0.6); app.click(1134, 300, 1, 0.4); app.wait(0.6)
    slide(app, 1100, 1190, 577, 0.8); app.wait(0.3); orbit(app, 150, 20, 1.6)
def T12(app):
    app.wait(0.5); app.click(*TAB["melt"], 1, 0.4); app.wait(0.7)
    for x, y in [(1008, 153), (1133, 153), (1210, 153), (1014, 189), (1089, 189), (1067, 153)]:
        app.click(x, y, 1, 0.4); app.wait(0.9)
    app.click(1218, 329, 1, 0.4); app.wait(0.8); app.click(1128, 329, 1, 0.4); app.wait(0.8); orbit(app, 120, 10, 1.4)
def T13(app):
    app.wait(0.5); app.click(*TAB["light"], 1, 0.4); app.wait(0.7)
    for x, y in [(1098, 154), (1173, 154), (1008, 187), (1082, 187), (1022, 154)]:
        app.click(x, y, 1, 0.4); app.wait(0.9)
    for x in (1082, 1111, 1169, 1198): app.click(x, 256, 1, 0.3); app.wait(0.6)
    slide(app, 1110, 1180, 581, 0.7); slide(app, 1180, 1100, 615, 0.7); orbit(app, 160, 20, 1.6)
def T14s(app):
    newscene(app, 1.0); closeanim(app)
def T14(app):
    import math
    pts = []
    for i in range(24):
        r = 30 if i % 2 == 0 else 22; a = i * math.pi / 12
        pts.append([round(r * math.cos(a), 1), round(r * math.sin(a), 1)])
    app.wait(0.6); app.cmd({"cmd": "sketch.create", "points": pts, "closed": True, "plane": "floor", "name": "gear"}); app.click(*HOME, 1, 0.4); app.wait(2.0)
    app.click(92, 197, 1, 0.5); app.wait(0.8)
    app.cmd({"cmd": "solid.extrude", "sketch": {"group": 0, "shape": 0}, "height": 14, "color": [1, .45, .6], "name": "gear"}); app.click(*HOME, 1, 0.4); app.wait(1.4)
    orbit(app, 170, 30, 1.8)
    for k in range(1, 7): set_(app, 0, 1, round=k); app.wait(0.35)
    app.cmd({"cmd": "solid.extrude", "sketch": {"group": 0, "shape": 0}, "height": 30, "color": [.35, .6, 1], "name": "gear2", "op": "Add"}); app.wait(1.5)
def T17s(app): base(app, 1.2); app.click(1226, 789, 1, 0.3); app.wait(2.0)
def T17(app):
    app.wait(0.4); app.click(143, 738, 1, 0.4); app.wait(0.8)
    for fx, pos in [(380, [-30, 0, 36]), (700, [28, 0, 52]), (1100, [0, 0, 36])]:
        app.click(fx, 768, 1, 0.5); app.wait(0.5); tween(app, 0, 1, [0, 0, 36], pos, 10, 0.05) if False else set_(app, 0, 1, pos=pos); app.wait(0.5)
        app.click(143, 738, 1, 0.4); app.wait(0.7)
    app.click(20, 768, 1, 0.5); app.wait(0.4); app.click(35, 738, 1, 0.4); app.wait(4.0)
def T18(app):
    app.wait(0.4); app.click(1197, 23, 1, 0.5); app.wait(1.4); app.click(1209, 159, 1, 0.5); app.wait(5.0)
    app.click(548, 234, 1, 0.5); app.wait(1.4); app.click(456, 234, 1, 0.5); app.wait(1.4); app.click(466, 517, 1, 0.5); app.wait(2.5)
    app.click(559, 640, 1, 0.5); app.wait(1.0)
    app.click(1197, 23, 1, 0.5); app.wait(1.2); app.click(1200, 57, 1, 0.5); app.wait(3.0)
    app.shot("/tmp/ex1.png"); app.wait(0.5)
    for (cx, cy) in [(559, 598), (557, 587), (559, 640)]: app.click(cx, cy, 1, 0.2); app.wait(0.3)
def T19s(app): newscene(app, 1.4); closeanim(app)
def T19(app):
    app.wait(0.6)
    for k, p, s, c in [("Sphere", [-24, 0, 24], [18] * 3, [.3, .85, .6]), ("Sphere", [24, 0, 24], [18] * 3, [1, .45, .6]), ("Torus", [0, 0, 8], [30, 7, 7], [.35, .6, 1]), ("Cone", [0, 0, 56], [12, 12, 14], [1, .8, .3])]:
        add(app, k, p, s, c, blend=12); app.wait(0.9); 
    app.click(*HOME, 1, 0.3); app.wait(1.5)
    for i in range(22): set_(app, 0, 0, pos=[-24 + 12 * i / 21, 0, 24]); set_(app, 0, 1, pos=[24 - 12 * i / 21, 0, 24]); app.wait(0.04)
    app.cmd({"cmd": "melt.set", "amount": 2.2, "drip": 6}); app.wait(1.0); orbit(app, 170, 25, 2.0)
def T20s(app):
    newscene(app, 1.5); closeanim(app)
    add(app, "Sphere", [0, 0, 18], [20] * 3, [.95, .45, .6], blend=14); add(app, "Sphere", [0, 0, 44], [14] * 3, [1, .85, .4], blend=14); add(app, "Sphere", [0, 0, 62], [9] * 3, [.35, .75, 1], blend=14)
    add(app, "Torus", [0, 0, 6], [32, 6, 6], [.3, .85, .6], blend=14); home(app, 6)
def T20(app):
    app.wait(0.4); app.click(*TAB["light"], 1, 0.4); app.wait(0.5); app.click(1173, 154, 1, 0.4); app.wait(0.6)
    orbit(app, 200, 20, 2.4); app.click(1082, 256, 1, 0.3); app.wait(0.4); orbit(app, -200, -10, 2.4); app.wait(0.5)
# --- SolidCraft takes
def T02s(app): sc_enter(app)
def T02(app):
    app.wait(0.3)
    for x in (215, 305, 376, 429, 533, 585, 137): app.click(x, 92, 1, 0.3); app.wait(0.45)
    app.click(140, 135, 1, 0.4); app.wait(1.5); app.click(695, 437, 1, 0.5); app.wait(2.0)
    app.click(130, 125, 1, 0.4)
    for p in [(700, 330), (800, 305), (870, 380), (840, 470), (740, 480), (700, 330)]:
        app.click(p[0], p[1], 1, 0.35); app.wait(0.25)
    app.wait(0.6); sc_finish_extrude(app, "24"); app.click(815, 724, 1, 0.4); app.wait(1.5); app.click(215, 92, 1, 0.3); app.wait(0.8)
def T15s(app): sc_enter(app)
def T15(app):
    app.wait(0.3); sc_sketch_rect(app); sc_dim(app, 780, 331, 780, 295, "60"); app.wait(0.3)
    sc_dim(app, 861, 400, 905, 400, "40")
    app.click(243, 125, 1, 0.4); app.click(900, 360, 1, 0.4); app.click(930, 360, 1, 0.4); app.wait(0.6)
    sc_finish_extrude(app, "12"); app.click(815, 724, 1, 0.4); app.wait(1.5)
def T16s(app):
    sc_enter(app); sc_sketch_rect(app, 700, 330, 820, 440); sc_finish_extrude(app, "20")
    app.click(815, 724, 1, 0.4); app.wait(1.5)
def T16(app):
    app.wait(0.4)
    for x in (87, 113, 113, 139, 139, 165): app.click(x, 772, 1, 0.4); app.wait(0.7)
    for x in (207, 237): app.click(x, 772, 1, 0.4); app.wait(0.9)
    app.drag(640, 420, 760, 400, secs=1.5, button=1); app.wait(0.5); app.click(165, 772, 1, 0.4); app.wait(1.0)
    app.click(168, 125, 1, 0.5) if False else None
def Tend(app): sc_leave(app)
BATCH = [("T11", base, T11), ("T12", base, T12), ("T13", base, T13), ("T14", T14s, T14), ("T17", T17s, T17), ("T18", base, T18), ("T19", T19s, T19), ("T20", T20s, T20),
         ("T02", T02s, T02), ("T15", lambda a: (sc_leave(a), sc_enter(a)), T15), ("T16", lambda a: (sc_leave(a), T16s(a)), T16)]
