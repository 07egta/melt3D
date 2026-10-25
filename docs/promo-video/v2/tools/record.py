import json, sys, os, subprocess, time, importlib
sys.path.insert(0, "/home/claude/video/rec"); sys.path.insert(0, "/home/claude/video2/rec")
import driver
mod = importlib.import_module(sys.argv[1]); want = sys.argv[2:]
app = driver.App()
for name, su, run in mod.BATCH:
    if want and name not in want: continue
    try:
        su(app)
    except Exception as e: print("setup err", name, e)
    out = f"/home/claude/video2/clips/{name}.mp4"
    rec = subprocess.Popen(["ffmpeg", "-y", "-loglevel", "error", "-f", "x11grab", "-framerate", "30", "-video_size", "1280x800", "-i", ":99", "-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-pix_fmt", "yuv420p", out], stdin=subprocess.PIPE, env=dict(os.environ, DISPLAY=":99"))
    t0 = time.time(); import t as _t; _t.MARKS.clear(); _t.MARKS_T0[0] = t0
    try: run(app)
    except Exception as e: print("run err", name, e)
    json.dump(_t.MARKS, open(out.replace('.mp4', '.marks.json'), 'w'))
    time.sleep(0.4); rec.communicate(b"q"); print("rec", name, round(time.time() - t0, 1), flush=True)
