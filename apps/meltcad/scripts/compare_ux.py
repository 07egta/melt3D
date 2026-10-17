#!/usr/bin/env python3
"""Compare human YuraCAD UX trials; refuse mismatched conditions or modes.

Usage:
 python apps/meltcad/scripts/compare_ux.py /path/to/ux-json-directory
Outputs medians for the six predefined workflows. Missing metrics stay unmeasured.
"""
import json
import statistics
import sys
from collections import defaultdict
from pathlib import Path

FIELDS = (
    ("actions", "少ない操作数"),
    ("completion_ms", "短い完了時間 (ms)"),
    ("unwanted_selections", "少ない誤選択"),
    ("unwanted_cancels", "少ない意図しない取消"),
    ("ui_frame_interval_p95_ms", "短いUIフレーム間隔p95 (ms)"),
    ("ui_cpu_frame_p95_ms", "短いUI CPU時間p95 (ms)"),
    ("ui_frame_fps", "高いUIフレームレート"),
    ("input_to_screen_latency_ms", "短い実表示遅延 (ms)"),
    ("display_fps", "高い実描画FPS"),
)
LOWER = set(k for k, _ in FIELDS if k not in {"ui_frame_fps", "display_fps"})


def aggregate(reports):
    groups = defaultdict(list)
    for r in reports:
        if r.get("schema_version") != 1:
            continue
        c = r["conditions"]
        # Never compare synthetic measurements with real people. Do not mix display scale,
        # render quality, machine or test instructions.
        condition_key = (
            c.get("machine"), c.get("os"), tuple(c.get("physical_window_px", [])),
            c.get("pixels_per_point"), c.get("quality"), c.get("notes"),
        )
        groups[(r["scenario"], r["mode"], condition_key, r["stage"])].append(r)
    return groups


def compare(reports):
    g = aggregate(reports)
    bases = {(s, m, c) for s, m, c, _ in g}
    rows = []
    for scenario, mode, conditions in sorted(bases, key=str):
        before = [r for r in g.get((scenario, mode, conditions, "before"), []) if r.get("passed")]
        after = [r for r in g.get((scenario, mode, conditions, "after"), []) if r.get("passed")]
        if not before or not after:
            rows.append((scenario, mode, None, "比較不可：同条件の成功した改善前・後記録が不足"))
            continue
        for name, label in FIELDS:
            bv = [r["metrics"].get(name) for r in before]
            av = [r["metrics"].get(name) for r in after]
            if not bv or not av or any(v is None for v in bv + av):
                rows.append((scenario, mode, label, "未測定"))
                continue
            b, a = statistics.median(bv), statistics.median(av)
            delta = a - b
            improved = delta < 0 if name in LOWER else delta > 0
            state = "改善" if improved else ("同等" if delta == 0 else "悪化")
            rows.append((scenario, mode, label, f"{b:.2f} → {a:.2f} (差 {delta:+.2f}; {state}; n={len(before)}/{len(after)})"))
    return rows


def main():
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    source = Path(sys.argv[1])
    paths = sorted(source.glob("*.json")) if source.is_dir() else [source]
    reports = []
    for path in paths:
        try:
            reports.append(json.loads(path.read_text(encoding="utf-8")))
        except (OSError, ValueError, KeyError) as exc:
            print(f"SKIP {path}: {exc}", file=sys.stderr)
    if not reports:
        print("測定データなし")
        return 1
    for scenario, mode, label, value in compare(reports):
        print(f"{scenario} [{mode}] {label or ''}: {value}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
