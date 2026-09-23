#!/usr/bin/env python3
"""Compare two Kasane preview benchmark reports."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys


METRICS = (
    ("refresh_cpu_ms", "mean"),
    ("refresh_cpu_ms", "p50"),
    ("refresh_cpu_ms", "p95"),
    ("refresh_cpu_ms", "p99"),
    ("frame_ms", "mean"),
    ("frame_ms", "p50"),
    ("frame_ms", "p95"),
    ("frame_ms", "p99"),
)
RESOURCE_METRICS = (
    "gpu_texture_bytes",
    "gpu_video_bytes",
    "mask_viewports",
    "offscreen_groups",
    "offscreen_creations",
    "offscreen_resizes",
    "uploads",
)


def load(path: Path) -> dict:
    report = json.loads(path.read_text())
    if "median" not in report or not report.get("runs"):
        raise ValueError(f"not a Kasane benchmark report: {path}")
    return report


def delta_percent(baseline: float, current: float) -> float:
    if baseline == 0:
        return 0.0 if current == 0 else float("inf")
    return (current / baseline - 1.0) * 100.0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("current", type=Path)
    parser.add_argument("--max-regression-percent", type=float, default=5.0)
    parser.add_argument(
        "--include-p99",
        action="store_true",
        help="also gate p99; it is more sensitive to desktop scheduling noise",
    )
    args = parser.parse_args()

    try:
        baseline = load(args.baseline)
        current = load(args.current)
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(exc, file=sys.stderr)
        return 2

    limit = args.max_regression_percent
    failures = []
    print(f"baseline={baseline.get('label', 'baseline')} current={current.get('label', 'current')}")
    print("metric,baseline,current,delta_percent,status")
    for category, percentile in METRICS:
        before = float(baseline["median"][category][percentile])
        after = float(current["median"][category][percentile])
        delta = delta_percent(before, after)
        gated = percentile in ("mean", "p95") or (percentile == "p99" and args.include_p99)
        passed = delta <= limit
        status = "PASS" if passed else ("FAIL" if gated else "INFO")
        print(f"{category}.{percentile},{before:.6f},{after:.6f},{delta:.2f},{status}")
        if gated and not passed:
            failures.append(f"{category}.{percentile} regressed by {delta:.2f}%")

    before_stats = baseline["runs"][0].get("render_stats", {})
    after_stats = current["runs"][0].get("render_stats", {})
    for metric in RESOURCE_METRICS:
        if metric not in before_stats or metric not in after_stats:
            continue
        before = float(before_stats[metric])
        after = float(after_stats[metric])
        delta = delta_percent(before, after)
        passed = delta <= limit
        print(f"render_stats.{metric},{before:.0f},{after:.0f},{delta:.2f},{'PASS' if passed else 'FAIL'}")
        if not passed:
            failures.append(f"render_stats.{metric} regressed by {delta:.2f}%")

    if failures:
        print("FAIL: " + "; ".join(failures), file=sys.stderr)
        return 1
    print(f"PASS: no metric regressed by more than {limit:.2f}%")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
