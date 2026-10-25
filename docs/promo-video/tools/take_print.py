from common import *
HOME = (862, 64)
def setup(app):
    app.cmd({"cmd": "scene.new"}); app.wait(0.5)
    app.cmd({"cmd": "melt.set", "amount": 1.2})
    app.cmd({"cmd": "shape.add", "kind": "Box", "pos": [0, 0, 14], "size": [30, 30, 14], "color": [1.0, 0.7, 0.25], "op": "Add", "blend": 8})
    app.cmd({"cmd": "shape.add", "kind": "Sphere", "pos": [0, 0, 36], "size": [17, 17, 17], "color": [0.3, 0.8, 0.6], "op": "Add", "blend": 8})
    app.click(*HOME, 1, 0.5); app.wait(6)
    app.click(1021, 75, 1, 0.5); app.wait(3)


def run(app):
    app.wait(1.0)
    app.click(1197, 23, 1, 0.7); app.wait(3.0)
    app.shot("/home/claude/video/work/p1.png")
    app.click(1220, 158, 1, 0.6); app.wait(5.0)
    app.shot("/home/claude/video/work/p2.png")
    app.click(548, 234, 1, 0.6); app.wait(2.0)
    app.click(456, 234, 1, 0.6); app.wait(2.0)
    app.click(466, 517, 1, 0.6); app.wait(3.0)
    app.click(559, 640, 1, 0.6); app.wait(1.5)
