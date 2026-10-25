from common import *
HOME = (862, 64)
def run(app):
    app.wait(0.5); app.cmd({"cmd": "scene.new"}); app.wait(0.8)
    app.cmd({"cmd": "shape.add", "kind": "Box", "pos": [0, 0, 24], "size": [36, 36, 24], "color": [1.0, 0.7, 0.25], "op": "Add", "blend": 2})
    app.click(*HOME, 1, 0.5); app.wait(1.0)
    app.cmd({"cmd": "shape.add", "kind": "Sphere", "pos": [0, 0, 80], "size": [26, 26, 26], "color": [0.5, 0.5, 0.5], "op": "Subtract", "blend": 2})
    for i in range(24):
        app.cmd({"cmd": "shape.set", "group": 0, "shape": 1, "pos": [0, 0, 80 - 54 * (i / 23)]}); app.wait(0.04)
    app.click(*HOME, 1, 0.4); app.wait(0.8)
    app.cmd({"cmd": "shape.add", "kind": "Cylinder", "pos": [0, -70, 24], "size": [9, 9, 90], "color": [0.5, 0.5, 0.5], "op": "Subtract", "blend": 1, "rot": [90, 0, 0]})
    for i in range(24):
        app.cmd({"cmd": "shape.set", "group": 0, "shape": 2, "pos": [0, -70 + 70 * (i / 23), 24]}); app.wait(0.04)
    app.wait(0.5)
    app.cmd({"cmd": "shape.add", "kind": "Sphere", "pos": [20, 0, 40], "size": [16, 16, 16], "color": [0.2, 0.7, 1.0], "op": "Paint", "blend": 1})
    app.wait(1.2)
    app.drag(560, 420, 820, 360, secs=2.4, button=2); app.wait(1.0)
