from common import *
HOME = (862, 64)
def setup(app):
    app.cmd({"cmd": "scene.new"}); app.wait(0.5)
    app.cmd({"cmd": "melt.set", "amount": 1.2})
    app.cmd({"cmd": "shape.add", "kind": "Box", "pos": [0, 0, 14], "size": [30, 30, 14], "color": [1.0, 0.7, 0.25], "op": "Add", "blend": 8})
    app.cmd({"cmd": "shape.add", "kind": "Sphere", "pos": [0, 0, 36], "size": [17, 17, 17], "color": [0.3, 0.8, 0.6], "op": "Add", "blend": 8})
    app.click(*HOME, 1, 0.5); app.wait(6)


def run(app):
    app.wait(0.8)
    app.click(*TAB["material"], 1, 0.6); app.wait(2.2)
    for x, y in [(570, 190), (610, 190), (585, 211)]:         # none here: material swatch clicks
        pass
    app.click(1115, 188, 1, 0.5); app.wait(1.6)               # material swatch
    app.click(1213, 211, 1, 0.5); app.wait(1.6)
    app.click(*TAB["melt"], 1, 0.6); app.wait(1.8)
    app.click(1067, 153, 1, 0.5); app.wait(1.6)               # Soft
    app.click(1133, 153, 1, 0.5); app.wait(1.6)               # Bouncy
    app.click(1210, 153, 1, 0.5); app.wait(1.6)               # Cheese
    app.click(1014, 189, 1, 0.5); app.wait(1.6)               # Honey
    app.click(*TAB["light"], 1, 0.6); app.wait(1.8)
    app.click(1153, 154, 1, 0.5); app.wait(1.6)               # Studio / Noon / Sunset
    app.click(1172, 154, 1, 0.5); app.wait(1.6)
    app.click(1207, 188, 1, 0.5); app.wait(1.6)
    app.click(1215, 128, 1, 0.5); app.wait(1.4)
    app.click(196, 73, 1, 0.6); app.wait(2.4)                 # Library tab
    app.drag(640, 420, 760, 380, secs=1.5, button=2); app.wait(0.8)
