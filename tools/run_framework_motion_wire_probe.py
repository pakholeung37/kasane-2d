#!/usr/bin/env python3
"""Check re-encoded motion3 files against the official Framework CPU trace."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests/fixtures/animation_cpu"
OUTPUT = ROOT / "target/motion-wire-probe"
PROBE = ROOT / "target/animation-cpu-probe/kasane_framework_animation_cpu_probe"


def run(command: list[str]) -> str:
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, timeout=300)
    if result.returncode:
        raise RuntimeError(f"{' '.join(command)} failed: {result.stderr or result.stdout}")
    return result.stdout


def trace(path: Path) -> dict:
    output = run([str(PROBE), "--motion-loop", str(FIXTURES / "model.moc3"), str(path)])
    lines = [json.loads(line) for line in output.splitlines() if line.startswith("{")]
    if len(lines) != 1:
        raise AssertionError(f"expected one Framework trace for {path}")
    return lines[0]


def curve_trace(path: Path) -> dict:
    output = run([str(PROBE), "--motion-curve", str(FIXTURES / "model.moc3"), str(path)])
    lines = [json.loads(line) for line in output.splitlines() if line.startswith("{")]
    if len(lines) != 1:
        raise AssertionError(f"expected one Framework curve trace for {path}")
    return lines[0]


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    report: dict = {"status": "failed", "checks": {}, "inputs": {}}
    try:
        run([sys.executable, str(ROOT / "tools/run_framework_animation_cpu_probe.py")])
        for name in ("loop", "minimal"):
            source = FIXTURES / f"{name}.motion3.json"
            encoded = OUTPUT / f"{name}.motion3.json"
            run(["cargo", "run", "-q", "-p", "kasane-live2d", "--example",
                 "reencode_motion3", "--", str(source), str(encoded)])
            if trace(source) != trace(encoded):
                raise AssertionError(f"Framework motion trace drift for {name}")
            report["checks"][name] = "passed"
            report["inputs"][str(source.relative_to(ROOT))] = digest(source)
            report["inputs"][str(encoded.relative_to(ROOT))] = digest(encoded)
        source = FIXTURES / "typed.motion3.json"
        encoded = OUTPUT / "typed.motion3.json"
        run(["cargo", "run", "-q", "-p", "kasane-live2d", "--example",
             "reencode_motion3", "--", str(source), str(encoded)])
        parsed = run([str(PROBE), "--motion-json-check", str(encoded)])
        lines = [json.loads(line) for line in parsed.splitlines() if line.startswith("{")]
        expected = {"consistent": True, "curves": 1, "segments": 4, "points": 7,
                    "events": 1, "event_bytes": 6, "first_event_actual_bytes": 6,
                    "first_event_time": 0.75}
        if lines != [expected]:
            raise AssertionError(f"Framework typed motion metadata drift: {lines}")
        report["checks"]["typed_segments_and_utf8_event"] = "passed"
        report["inputs"][str(source.relative_to(ROOT))] = digest(source)
        report["inputs"][str(encoded.relative_to(ROOT))] = digest(encoded)
        bezier_expected = {
            "bezier_restricted": [0, 0.223437503, 0.325000018, 0.3515625,
                                  0.350000024, 0.3671875, 0.450000048,
                                  0.645312488, 1],
            "bezier_unrestricted": [0.000000710705251, 0.257220954, 0.333164304,
                                    0.351470441, 0.350415289, 0.354238868,
                                    0.389742494, 0.509106219, 1],
        }
        for name, expected_values in bezier_expected.items():
            source = FIXTURES / f"{name}.motion3.json"
            encoded = OUTPUT / f"{name}.motion3.json"
            run(["cargo", "run", "-q", "-p", "kasane-live2d", "--example",
                 "reencode_motion3", "--", str(source), str(encoded)])
            original = curve_trace(source)
            rewritten = curve_trace(encoded)
            if len(original["samples"]) != len(expected_values):
                raise AssertionError(f"unexpected Framework curve sample count for {name}")
            for index, wanted in enumerate(expected_values):
                if (abs(original["samples"][index]["param_x"] - wanted) > 0.00002 or
                        abs(rewritten["samples"][index]["param_x"] - wanted) > 0.00002):
                    raise AssertionError(f"Framework curve drift for {name} at frame {index}")
            report["checks"][name] = "passed"
            report["inputs"][str(source.relative_to(ROOT))] = digest(source)
            report["inputs"][str(encoded.relative_to(ROOT))] = digest(encoded)
        report["inputs"]["probe_binary"] = digest(PROBE)
        report["status"] = "passed"
    except Exception as failure:
        report["error"] = str(failure)
    path = OUTPUT / "report.json"
    path.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(f"{report['status']}: {path}")
    if "error" in report:
        print(report["error"], file=sys.stderr)
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
