"""Helpers for writing YURACAD library models. Pure Python 3, no dependencies.

Usage in a generator script (library/gen/<file>.py):

    from lib import *
    items = []
    items.append(item("ねこ", ["cat", "animal"], group("ねこ", [
        shape("Sphere", "からだ", pos=(0, 0, 20), size=(18, 1, 1), color=(0.9, 0.7, 0.4)),
        ...
    ])))
    write_category("animals.json", "どうぶつ", items)       # writes ../animals.json
    preview(items)                                          # renders PNG sheets into /tmp/lib_preview/

Set MELTCAD_BIN to the prebuilt meltcad binary (see README.md).
"""
import json, os, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
OUT_DIR = os.path.dirname(HERE)  # apps/meltcad/library

SIZE_DEFAULT = {
    "Sphere": (20, 1, 1), "Box": (30, 30, 30), "Cylinder": (15, 40, 1), "Torus": (20, 7, 1),
    "Cone": (18, 40, 1), "Extrude": (20, 1, 1), "Prism3": (22, 40, 1), "PrismN": (20, 40, 1),
    "Octa": (25, 1, 1), "Tetra": (25, 1, 1), "Pyramid": (22, 40, 1), "Capsule": (10, 30, 1),
    "Hemi": (22, 1, 1), "Tube": (18, 4, 40), "Wedge": (30, 30, 30), "Star": (24, 14, 1),
    "Revolve": (1, 1, 1), "Loft": (40, 1, 1), "Curve": (1, 1, 1), "Sweep": (1, 1, 1),
}


def shape(kind, name, pos=(0, 0, 0), size=None, rot=(0, 0, 0), scale=(1, 1, 1), op="Add",
          blend=6.0, color=(0.9, 0.6, 0.5), roughness=0.5, metallic=0.0, glow=0.0, glass=0.0,
          coat=0.0, translucency=0.0, iridescence=0.0, round=0.0, outline=None, outline2=None,
          path=None, params=None, shell=0.0):
    """One shape. mm, Z up, degrees. op: Add | Subtract | Paint | Intersect.
    outline: [[x,y],...] for Extrude/Revolve/Loft/Sweep; path: [[x,y,z,radius],...] for Curve/Sweep."""
    if params is None:  # sensible defaults: a full turn for Revolve, 5 points for Star, 6 sides for PrismN, 4 for Pyramid
        params = {"Revolve": (360, 0, 0, 0), "Star": (5, 0.45, 0, 0), "PrismN": (6, 0, 0, 0), "Pyramid": (4, 0, 0, 0),
                  "Curve": (0, 0.7, 0, 1)}.get(kind, (0, 0, 0, 0))
    s = {
        "name": name, "kind": kind, "size": list(size if size is not None else SIZE_DEFAULT[kind]),
        "round": round, "pos": list(pos), "rot": list(rot), "scale": list(scale), "op": op,
        "blend": blend,
        "material": {"color": list(color), "roughness": roughness, "metallic": metallic, "glow": glow,
                     "glass": glass, "iridescence": iridescence, "coat": coat, "translucency": translucency},
        "visible": True,
        "outline": [list(p) for p in (outline or [[-15, -15], [15, -15], [15, 15], [-15, 15]])],
        "params": list(params), "path": [list(p) for p in (path or [])],
        "outline2": [list(p) for p in (outline2 or [])], "shell": shell,
    }
    return s


def group(name, shapes, blend=0.0):
    return {"name": name, "op": "Add", "blend": blend, "visible": True, "shapes": shapes}


def item(name, tags, grp):
    return {"name": name, "tags": list(tags), "group": grp}


def write_category(filename, category, items):
    with open(os.path.join(OUT_DIR, filename), "w", encoding="utf-8") as f:
        json.dump({"category": category, "items": items}, f, ensure_ascii=False, indent=1)
    print(f"wrote {filename}: {len(items)} items")


def scene_of(it):
    return {"groups": [it["group"]]}


def preview(items, outdir="/tmp/lib_preview", size=220, only=None):
    """Render each item to <outdir>/<n>_<name>.png (4 views: 3/4, front, right, top)."""
    binp = os.environ.get("MELTCAD_BIN")
    if not binp or not os.path.exists(binp):
        sys.exit("set MELTCAD_BIN to the meltcad binary")
    os.makedirs(outdir, exist_ok=True)
    for n, it in enumerate(items):
        if only and it["name"] not in only:
            continue
        yura = os.path.join(outdir, f"{n:03d}.yura")
        with open(yura, "w", encoding="utf-8") as f:
            json.dump(scene_of(it), f, ensure_ascii=False)
        png = os.path.join(outdir, f"{n:03d}.png")
        subprocess.run([binp, "--render", yura, png, str(size)], check=False)
        print(png, it["name"])
