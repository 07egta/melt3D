from common import *
def run(app):
    app.wait(0.5); app.cmd({"cmd": "scene.new"}); app.wait(0.8)
    star = [[0,-30],[9,-11],[30,-9],[14,5],[18,26],[0,15],[-18,26],[-14,5],[-30,-9],[-9,-11]]
    app.cmd({"cmd": "sketch.create", "points": star, "closed": True, "plane": "floor", "name": "star sketch"})
    app.click(862, 64, 1, 0.5); app.wait(2.0)
    r = app.cmd({"cmd": "solid.extrude", "sketch": {"group": 0, "shape": 0}, "height": 34, "color": [1.0, 0.45, 0.6], "name": "star"})
    app.click(862, 64, 1, 0.4); app.wait(1.2)
    app.drag(620, 420, 780, 360, secs=2.2, button=2); app.wait(0.4)
    for k in range(1, 9):
        app.cmd({"cmd": "shape.set", "group": 0, "shape": 1, "round": k}); app.wait(0.25)
    app.wait(1.2)
