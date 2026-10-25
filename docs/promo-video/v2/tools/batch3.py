from t import *
import batch2
from batch2 import closeanim, base, T14s, T14, T17s, T17, T18, T19s, T19, T20s, T20
def prep(app):
    sc_leave(app); app.wait(1)
def feat_m_setup(app):
    newscene(app, 1.3); closeanim(app)
    add(app, "Sphere", [0, 0, 36], [16, 16, 16], [.3, .85, .6]); add(app, "Box", [0, 0, 14], [26, 26, 14], [1, .6, .3]); home(app, 6)
    app.click(92, 197, 1, 0.3); app.wait(0.5); app.click(*TAB["shape"], 1, 0.3); app.wait(0.5)
def FEATM(app):
    app.wait(0.3)
    mark("mirror"); app.click(1113, 723, 1, 0.4); app.wait(1.1)
    mark("kind_star"); app.click(1035, 294, 1, 0.4); app.wait(0.5); app.click(1071, 294, 1, 0.3); app.wait(0.5)
    mark("kind_more"); app.click(1215, 258, 1, 0.3); app.wait(0.3); app.click(1143, 258, 1, 0.3); app.wait(0.5)
    app.click(1031, 222, 1, 0.3); app.wait(0.4)
    mark("gizmo_move"); app.drag(670, 306, 760, 292, secs=0.8); app.wait(0.2); app.drag(620, 255, 620, 190, secs=0.6); app.wait(0.2)
    mark("gizmo_rot"); app.drag(665, 228, 705, 262, secs=0.8); app.wait(0.3)
    mark("gizmo_scale"); app.drag(516, 277, 480, 250, secs=0.8); app.wait(0.3)
    mark("subtract"); app.click(1080, 183, 1, 0.3); app.wait(0.9); app.click(1010, 183, 1, 0.3)
    mark("paint"); app.click(1141, 183, 1, 0.3); app.wait(0.9); app.click(1224, 183, 1, 0.3); app.wait(0.6); app.click(1010, 183, 1, 0.3)
    mark("shell"); slide(app, 1077, 1160, 550, 0.8); app.wait(0.3); slide(app, 1160, 1077, 550, 0.4)
    mark("meltface"); app.click(1209, 609, 1, 0.3); app.wait(0.5); app.click(1060, 609, 1, 0.3); app.wait(0.4); app.click(1012, 609, 1, 0.3)
    mark("ripple"); app.click(1008, 646, 1, 0.3); app.wait(1.0)
    mark("group"); app.click(222, 117, 1, 0.4); app.wait(1.0)
    mark("library"); app.click(196, 73, 1, 0.4); app.wait(1.2); app.click(100, 240, 1, 0.4); app.wait(0.8)
    app.click(73, 73, 1, 0.3); app.wait(0.5)
    mark("undo"); 
    for _ in range(3): app.click(313, 23, 1, 0.2); app.wait(0.25)
    for _ in range(3): app.click(351, 23, 1, 0.2); app.wait(0.25)
    mark("quality"); app.click(948, 23, 1, 0.3); app.wait(1.0); app.key("Escape"); app.wait(0.3)
    mark("viewmenu"); app.click(250, 23, 1, 0.3); app.wait(1.0); app.key("Escape"); app.wait(0.3)
    mark("eye"); app.click(243, 229, 1, 0.3); app.wait(0.5); app.click(243, 229, 1, 0.3); app.wait(0.5)
    mark("trackpad"); app.click(1114, 23, 1, 0.3); app.wait(0.5); app.click(1038, 23, 1, 0.3); app.wait(0.5)
    mark("end")
def feat_s_setup(app):
    sc_enter(app); sc_sketch_rect(app, 700, 330, 820, 440); sc_finish_extrude(app, "20"); app.click(815, 724, 1, 0.4); app.wait(1.5)
def FEATS(app):
    app.wait(0.3)
    for name, x, y in [("sc_fillet", 407, 135), ("sc_chamfer", 470, 135), ("sc_shell", 528, 135), ("sc_press", 600, 135), ("sc_move", 668, 135), ("sc_mirror", 725, 135), ("sc_pattern", 783, 135), ("sc_param", 856, 135), ("sc_offset", 1017, 135), ("sc_hole", 337, 135), ("sc_revolve", 279, 135)]:
        mark(name); app.click(x, y, 1, 0.3); app.wait(1.1); app.key("Escape"); app.wait(0.3)
    for name, x in [("sc_inspect", 429), ("sc_view", 533), ("sc_sheet", 585), ("sc_insert", 376)]:
        mark(name); app.click(x, 92, 1, 0.3); app.wait(0.9)
    mark("end")
BATCH = [("T14", T14s, T14), ("T17", T17s, T17), ("T18", base, T18), ("T19", T19s, T19), ("T20", T20s, T20),
         ("FEATM", feat_m_setup, FEATM), ("FEATS", lambda a: (sc_leave(a), feat_s_setup(a)), FEATS)]
