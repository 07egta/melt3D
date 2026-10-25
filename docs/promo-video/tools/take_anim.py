import copy
from common import *
HOME = (862, 64)
def run(app):
    app.wait(0.5); load_char(app)
    sc = app.cmd({"cmd": "scene.get"})
    def posed(base, dz, sz, melt):
        s = copy.deepcopy(base)
        for g in s["groups"]:
            for sh in g["shapes"]:
                sh["pos"][2] += dz
                sh["scale"][2] *= sz
                sh["scale"][0] *= (2 - sz) ** 0.5
        s["melt"]["amount"] = melt
        return s
    app.cmd({"cmd": "anim.set", "fps": 12, "length": 36, "clay": True})
    A = posed(sc, -1.0, 0.95, 1.0)
    app.cmd({"cmd": "scene.set", "scene": A})
    app.cmd({"cmd": "anim.set", "fps": 12, "length": 36, "clay": True})
    app.cmd({"cmd": "anim.key", "frame": 0})
    cur = app.cmd({"cmd": "scene.get"})
    B = posed(sc, 9.0, 1.06, 2.2); B["anim"] = cur["anim"]
    app.cmd({"cmd": "scene.set", "scene": B})
    app.cmd({"cmd": "anim.key", "frame": 12})
    cur = app.cmd({"cmd": "scene.get"})
    A2 = copy.deepcopy(A); A2["anim"] = cur["anim"]
    app.cmd({"cmd": "scene.set", "scene": A2})
    app.cmd({"cmd": "anim.key", "frame": 24})
    app.click(*HOME, 1, 0.5); app.wait(1.2)
    for rep in range(4):
        for f in range(0, 25):
            app.cmd({"cmd": "anim.at", "frame": f}); app.wait(0.045)
        if rep == 1:
            app.drag(620, 420, 740, 400, secs=1.0, button=2)
    app.wait(0.4)
