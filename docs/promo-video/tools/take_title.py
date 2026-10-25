from common import *
def setup(app):
    load_char(app)


def run(app):
    app.wait(1.0)
    orbit(app, 200, -10, 2.6); app.wait(0.4)
    orbit(app, -380, 20, 3.4); app.wait(0.4)
    zoom(app, 2); app.wait(0.6)
    orbit(app, 240, 0, 2.4); app.wait(0.4)
    zoom(app, 2, up=False); app.wait(0.6)
    app.click(862, 64, 1, 0.5); app.wait(1.6)
