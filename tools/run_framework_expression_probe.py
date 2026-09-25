#!/usr/bin/env python3
"""Re-encode exp3 fixtures and check their decoded values in the local Framework."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests/fixtures/animation_cpu"
OUTPUT = ROOT / "target/expression-cpu-probe"
PROBE = ROOT / "target/animation-cpu-probe/kasane_framework_animation_cpu_probe"


def run(args: list[str], timeout: int = 300) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(args, cwd=ROOT, text=True, capture_output=True, timeout=timeout)
    if result.returncode:
        raise RuntimeError(f"{' '.join(args)} failed: {result.stderr or result.stdout}")
    return result


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def trace(path: Path) -> dict:
    result = run([str(PROBE), "--expression-check", str(path)])
    lines = [line for line in result.stdout.splitlines() if line.startswith("{")]
    if len(lines) != 1:
        raise AssertionError(f"expected one Framework trace for {path}")
    return json.loads(lines[0])


def main() -> int:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    report = {"status": "failed", "checks": {}, "inputs": {}}
    try:
        run([sys.executable, str(ROOT / "tools/run_framework_animation_cpu_probe.py")])
        expected = {
            "minimal": {
                "fade_in": 0.25,
                "fade_out": 0,
                "parameters": [
                    {"is_param_x": True, "blend": 2, "value": 0.75},
                    {"is_param_x": False, "blend": 1, "value": 0.5},
                ],
            },
            "default": {
                "fade_in": 1,
                "fade_out": 1,
                "parameters": [{"is_param_x": True, "blend": 0, "value": 0.125}],
            },
        }
        for name, wanted in expected.items():
            source = FIXTURES / f"{name}.exp3.json"
            encoded = OUTPUT / f"{name}.exp3.json"
            run(["cargo", "run", "-q", "-p", "kasane-live2d", "--example",
                 "reencode_exp3", "--", str(source), str(encoded)])
            source_trace = trace(source)
            encoded_trace = trace(encoded)
            if source_trace != wanted or encoded_trace != wanted:
                raise AssertionError(f"Framework expression drift for {name}: {source_trace}, {encoded_trace}")
            report["checks"][name] = "passed"
            report["inputs"][str(source.relative_to(ROOT))] = digest(source)
            report["inputs"][str(encoded.relative_to(ROOT))] = digest(encoded)
        mixed = run([str(PROBE), "--expression-mix", str(FIXTURES / "model.moc3"),
                     str(OUTPUT / "minimal.exp3.json"), str(OUTPUT / "default.exp3.json")])
        traces = [json.loads(line) for line in mixed.stdout.splitlines() if line.startswith("{")]
        if len(traces) != 1 or len(traces[0].get("frames", [])) != 7:
            raise AssertionError(f"unexpected Framework expression mix trace: {traces}")
        expected_x = [0, 0.375, 0.75, 0, 0.00475753099, 0.0183058269, 0.0385822877]
        expected_y = [0.5, 0.375, 0.25, 0.25, 0.259515047, 0.286611676, 0.32716459]
        for index, (frame, wanted) in enumerate(zip(traces[0]["frames"], expected_x)):
            if (abs(frame["param_x"] - wanted) > 0.00002 or
                    abs(frame["param_y"] - expected_y[index]) > 0.00002):
                raise AssertionError(f"Framework expression queue drift at frame {index}: {frame}")
        report["checks"]["expression_queue"] = "passed"
        report["expression_queue"] = traces[0]
        for blend, amount, wanted in [("Add", 4.0, 0.75),
                                      ("Overwrite", -4.0, -0.25),
                                      ("Multiply", 4.0, 0.75)]:
            source = OUTPUT / f"clamp-{blend}-source.exp3.json"
            encoded = OUTPUT / f"clamp-{blend}.exp3.json"
            source.write_text(json.dumps({
                "Type": "Live2D Expression", "FadeInTime": 0.25,
                "Parameters": [{"Id": "ParamY", "Value": amount, "Blend": blend}],
            }, indent=2))
            run(["cargo", "run", "-q", "-p", "kasane-live2d", "--example",
                 "reencode_exp3", "--", str(source), str(encoded)])
            result = run([str(PROBE), "--expression-mix", str(FIXTURES / "model.moc3"),
                          str(encoded), str(OUTPUT / "default.exp3.json")])
            clamp_trace = json.loads(next(line for line in result.stdout.splitlines() if line.startswith("{")))
            actual = clamp_trace["frames"][1]["param_y"]
            if abs(actual - wanted) > 0.00002:
                raise AssertionError(f"Framework clamp-before-weight drift for {blend}: {actual}")
            report["checks"][f"clamp_before_weight_{blend}"] = "passed"
            report["inputs"][str(encoded.relative_to(ROOT))] = digest(encoded)
        report["inputs"]["probe_binary"] = digest(PROBE)
        report["status"] = "passed"
    except Exception as failure:
        report["error"] = str(failure)
    path = OUTPUT / "report.json"
    path.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    print(f"{report['status']}: {path}")
    if report.get("error"):
        print(report["error"], file=sys.stderr)
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
