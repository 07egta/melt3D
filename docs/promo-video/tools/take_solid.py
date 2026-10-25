from common import *
def run(app):
    app.wait(0.5)
    app.click(1180, 52, 1, 0.3) if False else None
    xs = [205, 238, 270, 303, 335, 368, 400, 432, 465]
    for x in xs[:4]:                                   # step back through the history
        app.click(x, 772, 1, 0.35); app.wait(0.9)
    for x in xs[4:]:
        app.click(x, 772, 1, 0.35); app.wait(0.9)
    app.click(1181, 124, 1, 0.3) if False else None
    app.drag(640, 460, 560, 400, secs=1.8, button=2); app.wait(0.4)
    for y in (432, 458, 484, 510):                     # pick each sketch in the browser
        app.click(110, y, 1, 0.4); app.wait(0.8)
    app.click(168, 125, 1, 0.5); app.wait(1.2)         # hole tool dialog
    app.shot("/home/claude/video/work/h1.png")
    app.key("Escape"); app.wait(0.5)
    app.click(240, 150, 1, 0.5); app.wait(1.2)         # extrude dialog
    app.key("Escape"); app.wait(0.5)
    app.drag(640, 460, 760, 440, secs=2.0, button=2); app.wait(0.8)
