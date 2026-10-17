#!/usr/bin/env python3
"""Audit all YuraCAD library models and optionally render actual four-view PNGs.

python3 apps/meltcad/library/gen/audit_models.py
python3 apps/meltcad/library/gen/audit_models.py --render --bin /path/to/meltcad
python3 apps/meltcad/library/gen/audit_models.py --regenerate

No third-party dependencies. Generated reports and PNGs go outside the repository.
"""
import argparse
import html
import json
import math
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
CATS = (
    "people", "hands", "animals", "tools_a", "tools_b", "furniture",
    "food", "vehicles", "buildings", "nature", "print", "pip",
    "realistic", "toon", "town",
)


def valid_vec(v, n, lo, hi):
    return isinstance(v, list) and len(v) == n and all(
        type(x) in (int, float) and math.isfinite(x) and lo <= x <= hi for x in v
    )


def audit_model(cat, model, findings):
    name = model.get("name", "")
    def flag(kind, message):
        findings.append({"category": cat, "name": name, "kind": kind, "message": message})
    tags = model.get("tags")
    group = model.get("group", {})
    shapes = group.get("shapes", [])
    if not isinstance(name, str) or not name.strip():
        flag("error", "Missing name")
    if not isinstance(tags, list) or not tags or any(not isinstance(t, str) or not t.strip() for t in tags):
        flag("error", "Missing/invalid tags")
    if group.get("name") != name or group.get("op") != "Add":
        flag("error", "Group name/operation mismatch")
    if not 1 <= len(shapes) <= 24:
        flag("error", f"Shape count: {len(shapes)} (expected 1..24)")
    if not any(s.get("op") == "Add" for s in shapes):
        flag("error", "No additive shapes")
    positions = []
    for i, s in enumerate(shapes):
        for field, lo, hi in (("pos", -5000, 5000), ("rot", -3600, 3600),
                              ("size", 0.01, 2000), ("scale", 0.01, 100)):
            if not valid_vec(s.get(field), 3, lo, hi):
                flag("error", f"Shape {i}: invalid {field}")
        pos = s.get("pos")
        if valid_vec(pos, 3, -5000, 5000):
            positions.append(pos)
        col = s.get("material", {}).get("color")
        if not valid_vec(col, 3, 0, 1):
            flag("error", f"Shape {i}: invalid color")
        if s.get("kind") in ("Curve", "Sweep") and len(s.get("path", [])) < 2:
            flag("error", f"Shape {i}: curve path too short")
        if s.get("kind") in ("Extrude", "Revolve", "Loft", "Sweep") and len(s.get("outline", [])) < 3:
            flag("error", f"Shape {i}: outline too short")
    if positions:
        avg = [sum(p[k] for p in positions) / len(positions) for k in (0, 1)]
        if abs(avg[0]) > 20 or abs(avg[1]) > 30:
            flag("review", f"Off-centre shape origins {avg}; may be intentional")
    if not model.get("source") or not model.get("license"):
        flag("provenance", "Source/licence are not established per model")
    return len(shapes)


def generated_matches(cat, committed, findings):
    with tempfile.TemporaryDirectory(prefix="yuracad-library-") as folder:
        env = os.environ.copy()
        env["MELTCAD_OUT_DIR"] = folder
        env["MELTCAD_SKIP_PREVIEW"] = "1"
        proc = subprocess.run([sys.executable, str(ROOT / "gen" / (cat + ".py"))],
                              cwd=ROOT / "gen", env=env, capture_output=True,
                              text=True, timeout=120, check=False)
        file = Path(folder) / (cat + ".json")
        if proc.returncode or not file.is_file():
            findings.append({"category": cat, "name": "", "kind": "error",
                             "message": "Generation failed: " + proc.stderr[-300:]})
        elif json.loads(file.read_text(encoding="utf-8")) != committed:
            findings.append({"category": cat, "name": "", "kind": "error",
                             "message": "Committed JSON differs from generator output"})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=Path(tempfile.gettempdir()) / "yuracad-library-audit")
    parser.add_argument("--render", action="store_true")
    parser.add_argument("--regenerate", action="store_true",
                        help="Check reproducibility without modifying committed JSON")
    parser.add_argument("--bin", type=Path, default=None)
    parser.add_argument("--size", type=int, default=160)
    args = parser.parse_args()
    if not 40 <= args.size <= 400:
        parser.error("--size must be 40..400")
    binary = args.bin or (Path(os.environ["MELTCAD_BIN"]) if os.environ.get("MELTCAD_BIN") else None)
    if args.render and (binary is None or not binary.is_file()):
        parser.error("--render requires --bin /path/to/meltcad or MELTCAD_BIN")
    args.out.mkdir(parents=True, exist_ok=True)
    findings, cards = [], []
    total = shapes_total = renders = 0
    with tempfile.TemporaryDirectory(prefix="yuracad-scenes-") as folder:
        for cat in CATS:
            data = json.loads((ROOT / (cat + ".json")).read_text(encoding="utf-8"))
            if args.regenerate:
                generated_matches(cat, data, findings)
            seen = set()
            for index, item in enumerate(data["items"]):
                name = item["name"]
                if name in seen:
                    findings.append({"category": cat, "name": name, "kind": "error",
                                     "message": "Duplicate name within category"})
                seen.add(name)
                shapes_total += audit_model(cat, item, findings)
                total += 1
                if not args.render:
                    continue
                scene = Path(folder) / "scene.yura"
                scene.write_text(json.dumps({"groups": [item["group"]]}, ensure_ascii=False),
                                 encoding="utf-8")
                png = args.out / f"{cat}-{index:03d}.png"
                try:
                    proc = subprocess.run([str(binary), "--render", str(scene), str(png),
                                           str(args.size)], capture_output=True, timeout=120)
                    head = png.read_bytes()[:24] if png.is_file() else b""
                    dims = struct.unpack(">II", head[16:24]) if len(head) == 24 and head[:8] == b"\\x89PNG\\r\\n\\x1a\\n" else None
                    if proc.returncode or dims != (2 * args.size, 2 * args.size):
                        raise ValueError("No valid four-view PNG")
                    renders += 1
                    cards.append(f'<figure><img loading="lazy" src="{png.name}"><figcaption>'
                                 f'{html.escape(name)} · {html.escape(data["category"])}</figcaption></figure>')
                except (OSError, ValueError, subprocess.TimeoutExpired) as exc:
                    findings.append({"category": cat, "name": name, "kind": "error",
                                     "message": "Render: " + str(exc)})
    report = {"categories": len(CATS), "models": total, "shapes": shapes_total,
              "four_view_renders": renders, "findings": findings,
              "limitation": "Semantic recognisability and ground contact require visual review."}
    (args.out / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n",
                                          encoding="utf-8")
    if args.render:
        gallery = ('<!doctype html><meta charset="utf-8"><title>YuraCAD previews</title>'
                   '<style>body{font:16px system-ui;background:#24272c;color:#eee}'
                   'main{display:grid;grid-template-columns:repeat(auto-fit,minmax(230px,1fr))}'
                   'img{width:100%}figure{background:#353b42;padding:8px}</style>'
                   '<h1>Four-view model audit</h1>'
                   '<p>Top-left: oblique; top-right: front; bottom-left: right; bottom-right: top.</p>'
                   '<main>' + "\n".join(cards) + '</main>')
        (args.out / "index.html").write_text(gallery, encoding="utf-8")
    errors = sum(x["kind"] == "error" for x in findings)
    print(f"{total} models, {shapes_total} shapes, {renders} renders, {errors} errors; {args.out}")
    sys.exit(1 if errors else 0)


if __name__ == "__main__":
    main()
