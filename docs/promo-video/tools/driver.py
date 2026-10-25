#!/usr/bin/env python3
"""Drive the real Melt 3D window on a virtual screen and record it.

  python3 driver.py <take-name> <script.py>

The script file defines `run(app)` and may use:
  app.cmd({...})            a scene command sent to the open window (--control)
  app.move(x, y, secs)      move the real mouse pointer smoothly
  app.click(x, y, button)   real click   (button 1 left, 2 middle, 3 right)
  app.drag(x0, y0, x1, y1, secs, button)
  app.key(name)             real key press (e.g. 'f', 'Tab')
  app.wait(secs)
The take is written to ../clips/<take-name>.mp4 (1280x720 crop of the window area).
"""
import json, os, socket, subprocess, sys, time, glob, importlib.util
from Xlib import X, display, XK
from Xlib.ext import xtest

HERE = os.path.dirname(os.path.abspath(__file__))
BIN = "/home/claude/07egta/melt3d/target/release/meltcad"
DISP = ":99"
SW, SH = 1280, 800
PORT = 7788


class App:
    def __init__(self):
        self.procs = []   # the app and the display are started once by serve.sh and stay up
        env = dict(os.environ, DISPLAY=DISP)
        self.d = display.Display(DISP)
        self.sock = None
        for _ in range(120):
            try:
                self.sock = socket.create_connection(("127.0.0.1", PORT), timeout=60)
                break
            except OSError:
                time.sleep(0.5)
        if not self.sock:
            raise SystemExit("app did not start: " + open("/tmp/app.out").read()[-800:])
        self.f = self.sock.makefile("rw", encoding="utf-8")
        tf = glob.glob(os.path.expanduser(f"~/.local/state/Melt3D/control/melt3d-{PORT}.token"))
        self.token = open(tf[0]).read().strip() if tf else "x"
        self.n = 0
        self.cmd({"cmd": "commands"})
        self.px, self.py = SW // 2, SH // 2

    def cmd(self, c):
        self.n += 1
        msg = {"id": self.n, **c}
        if self.n == 1:
            msg["token"] = self.token
        self.f.write(json.dumps(msg) + "\n")
        self.f.flush()
        r = json.loads(self.f.readline())
        if not r.get("ok"):
            print("cmd error:", c.get("cmd"), r.get("error"))
        return r.get("result")

    def wait(self, s):
        time.sleep(s)

    def _ptr(self, x, y):
        xtest.fake_input(self.d, X.MotionNotify, x=int(x), y=int(y))
        self.d.sync()
        self.px, self.py = x, y

    def move(self, x, y, secs=0.6):
        x0, y0, steps = self.px, self.py, max(2, int(secs * 60))
        for i in range(1, steps + 1):
            t = i / steps
            t = t * t * (3 - 2 * t)
            self._ptr(x0 + (x - x0) * t, y0 + (y - y0) * t)
            time.sleep(secs / steps)

    def click(self, x, y, button=1, secs=0.5):
        self.move(x, y, secs)
        xtest.fake_input(self.d, X.ButtonPress, button)
        self.d.sync()
        time.sleep(0.07)
        xtest.fake_input(self.d, X.ButtonRelease, button)
        self.d.sync()
        time.sleep(0.2)

    def drag(self, x0, y0, x1, y1, secs=1.2, button=1):
        self.move(x0, y0, 0.5)
        xtest.fake_input(self.d, X.ButtonPress, button)
        self.d.sync()
        time.sleep(0.1)
        self.move(x1, y1, secs)
        xtest.fake_input(self.d, X.ButtonRelease, button)
        self.d.sync()
        time.sleep(0.2)

    def key(self, name, ctrl=False, shift=False, alt=False):
        mods = []
        if ctrl: mods.append("Control_L")
        if shift: mods.append("Shift_L")
        if alt: mods.append("Alt_L")
        for m in mods:
            xtest.fake_input(self.d, X.KeyPress, self.d.keysym_to_keycode(XK.string_to_keysym(m)))
        code = self.d.keysym_to_keycode(XK.string_to_keysym(name))
        xtest.fake_input(self.d, X.KeyPress, code)
        self.d.sync()
        time.sleep(0.06)
        xtest.fake_input(self.d, X.KeyRelease, code)
        for m in reversed(mods):
            xtest.fake_input(self.d, X.KeyRelease, self.d.keysym_to_keycode(XK.string_to_keysym(m)))
        self.d.sync()
        time.sleep(0.15)

    def type(self, text, gap=0.09):
        for ch in text:
            self.key({" ": "space"}.get(ch, ch), shift=ch.isupper())
            time.sleep(gap)

    def wheel(self, n=1, up=True):
        for _ in range(n):
            xtest.fake_input(self.d, X.ButtonPress, 4 if up else 5)
            xtest.fake_input(self.d, X.ButtonRelease, 4 if up else 5)
            self.d.sync()
            time.sleep(0.12)

    def shot(self, path):
        subprocess.run(["import", "-window", "root", path], env=dict(os.environ, DISPLAY=DISP))

    def close(self):
        for p in reversed(self.procs):
            p.terminate()


def main():
    take, script = sys.argv[1], sys.argv[2]
    spec = importlib.util.spec_from_file_location("take", script)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    app = App()
    if hasattr(mod, 'setup'): mod.setup(app)
    out = os.path.join(HERE, "..", "clips", take + ".mp4")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    rec = subprocess.Popen(["ffmpeg", "-y", "-loglevel", "error", "-f", "x11grab", "-framerate", "30", "-video_size", f"{SW}x{SH}",
                            "-i", DISP, "-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-pix_fmt", "yuv420p", out],
                           stdin=subprocess.PIPE, env=dict(os.environ, DISPLAY=DISP))
    try:
        mod.run(app)
    finally:
        time.sleep(0.5)
        rec.communicate(b"q")
        app.close()
    print("recorded", out)


if __name__ == "__main__":
    main()
