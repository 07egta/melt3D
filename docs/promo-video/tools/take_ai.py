from common import *
def run(app):
    app.wait(0.5); app.cmd({"cmd": "scene.new"}); app.wait(1.0)
    app.cmd({"cmd": "melt.set", "amount": 1.4}); app.wait(0.8)
    app.cmd({"cmd": "shape.add", "kind": "Sphere", "pos": [-22, 0, 24], "size": [18, 18, 18], "color": [0.25, 0.8, 0.55], "op": "Add", "blend": 12}); app.click(862, 64, 1, 0.3); app.wait(1.2)
    app.cmd({"cmd": "shape.add", "kind": "Sphere", "pos": [22, 0, 24], "size": [18, 18, 18], "color": [1.0, 0.45, 0.6], "op": "Add", "blend": 12}); app.wait(1.4)
    for i in range(20):
        app.cmd({"cmd": "shape.set", "group": 0, "shape": 0, "pos": [-22 + 12 * i / 19, 0, 24]})
        app.cmd({"cmd": "shape.set", "group": 0, "shape": 1, "pos": [22 - 12 * i / 19, 0, 24]}); app.wait(0.05)
    app.cmd({"cmd": "shape.add", "kind": "Torus", "pos": [0, 0, 10], "size": [30, 7, 7], "color": [0.3, 0.6, 1.0], "op": "Add", "blend": 12}); app.wait(1.2)
    app.click(862, 64, 1, 0.3); app.wait(0.8)
    app.drag(560, 380, 760, 340, secs=2.0, button=2); app.wait(0.6)
