#!/usr/bin/env python3
"""Compare 120-frame Rust Physics traces with the official Framework."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests/fixtures/animation_cpu"
PROBE = ROOT / "target/animation-cpu-probe/kasane_framework_animation_cpu_probe"
OUTPUT = ROOT / "target/physics-sequence-probe"


def run(command: list[str]) -> str:
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, timeout=300)
    if result.returncode:
        raise RuntimeError(f"{' '.join(command)} failed: {result.stderr or result.stdout}")
    return result.stdout


def trace(output: str) -> list[dict[str, float]]:
    lines = [json.loads(line) for line in output.splitlines() if line.startswith("{")]
    if len(lines) != 1 or len(lines[0]["frames"]) != 120:
        raise AssertionError("unexpected Physics trace shape")
    return lines[0]["frames"]


def main() -> int:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    report: dict = {"status": "failed", "cases": {}}
    try:
        run([sys.executable, str(ROOT / "tools/run_framework_animation_cpu_probe.py")])
        for name in ("missing", "zero", "thirty", "multi"):
            source = FIXTURES / f"{name}.physics3.json"
            official = trace(run([str(PROBE), "--physics-sequence", str(FIXTURES / "model.moc3"), str(source)]))
            rust = trace(run(["cargo", "run", "-q", "-p", "kasane-animation", "--example",
                              "physics_sequence", "--", str(source)]))
            maximum = 0.0
            for index, (expected, actual) in enumerate(zip(official, rust)):
                for key in ("dt", "input", "output"):
                    error = abs(expected[key] - actual[key])
                    maximum = max(maximum, error)
                    if error > 0.0001:
                        raise AssertionError(f"{name} frame {index} {key}: {actual[key]} vs {expected[key]}")
            report["cases"][name] = {
                "frames": len(official), "max_error": maximum,
                "fixture_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            }
            official_stable = json.loads([line for line in run(
                [str(PROBE), "--physics-stabilize", str(FIXTURES / "model.moc3"), str(source)]
            ).splitlines() if line.startswith("{")][0])
            rust_stable = json.loads(run(["cargo", "run", "-q", "-p", "kasane-animation", "--example",
                                          "physics_sequence", "--", str(source), "--stabilize"]))
            stable_errors = [abs(official_stable["stabilized"] - rust_stable["stabilized"])]
            stable_errors.extend(abs(left - right) for left, right in zip(
                official_stable["frames"], rust_stable["frames"]))
            if len(official_stable["frames"]) != 20 or len(rust_stable["frames"]) != 20 or max(stable_errors) > 0.0001:
                raise AssertionError(f"{name} stabilization drift: {max(stable_errors)}")
            report["cases"][name]["stabilization_max_error"] = max(stable_errors)
        report["status"] = "passed"
    except Exception as failure:
        report["error"] = str(failure)
    path = OUTPUT / "report.json"
    path.write_text(json.dumps(report, indent=2) + "\n")
    print(f"{report['status']}: {path}")
    if "error" in report:
        print(report["error"], file=sys.stderr)
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
