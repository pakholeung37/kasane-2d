#!/usr/bin/env python3
"""Publish a combined package and load every supported attachment with Framework."""
from __future__ import annotations

import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "target/animation-package-probe"
PROBE = ROOT / "target/animation-cpu-probe/kasane_framework_animation_cpu_probe"
CDI_PROBE = ROOT / "target/cdi-probe-build/kasane_framework_cdi_cpu_probe"


def run(args: list[str]) -> str:
    result = subprocess.run(args, cwd=ROOT, text=True, capture_output=True, timeout=300)
    if result.returncode:
        raise RuntimeError(f"{' '.join(args)} failed: {result.stderr or result.stdout}")
    return result.stdout


def json_line(output: str) -> dict:
    lines = [json.loads(line) for line in output.splitlines() if line.startswith("{")]
    if len(lines) != 1:
        raise AssertionError("expected one Framework JSON result")
    return lines[0]


def references(model3: dict) -> list[str]:
    refs = model3["FileReferences"]
    paths = [refs["Moc"], *refs["Textures"], refs["DisplayInfo"], refs["Physics"], refs["Pose"], refs["UserData"]]
    paths.extend(entry["File"] for entry in refs["Expressions"])
    for entries in refs["Motions"].values():
        for entry in entries:
            paths.append(entry["File"])
            if "Sound" in entry:
                paths.append(entry["Sound"])
    return paths


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    package = OUT / "package"
    detached = OUT / "detached"
    report: dict = {"status": "failed", "checks": {}}
    try:
        run([sys.executable, str(ROOT / "tools/run_framework_animation_cpu_probe.py")])
        run([sys.executable, str(ROOT / "tools/run_framework_cdi_probe.py")])
        if package.exists(): shutil.rmtree(package)
        if detached.exists(): shutil.rmtree(detached)
        run(["cargo", "run", "-q", "-p", "kasane-project", "--example", "animation_package", "--", str(package)])
        shutil.copytree(package, detached)
        shutil.rmtree(package)
        model3_path = detached / "model.model3.json"
        model3 = json.loads(model3_path.read_text())
        paths = references(model3)
        if len(paths) != len(set(paths)):
            raise AssertionError("duplicate model3 file path")
        for relative in paths:
            path = Path(relative)
            if path.is_absolute() or ".." in path.parts or not (detached / path).is_file():
                raise AssertionError(f"missing or unsafe model3 reference: {relative}")
        report["checks"]["detached_references"] = len(paths)
        settings = json_line(run([str(PROBE), "--model3-check", str(model3_path)]))
        if settings != {"moc":"model.moc3", "textures":1, "expressions":1, "motion_groups":1, "hit_areas":1}:
            raise AssertionError(f"Framework model3 settings mismatch: {settings}")
        report["checks"]["model3_loader"] = "passed"
        refs = model3["FileReferences"]
        moc = detached / refs["Moc"]
        expression = detached / refs["Expressions"][0]["File"]
        motion = detached / refs["Motions"]["Idle"][0]["File"]
        physics = detached / refs["Physics"]
        pose = detached / refs["Pose"]
        cdi = json_line(run([str(CDI_PROBE), "--load", str(detached / refs["DisplayInfo"])]))
        if not cdi["accepted"]: raise AssertionError("Framework rejected CDI")
        run([str(PROBE), "--expression-check", str(expression)])
        motion_meta = json_line(run([str(PROBE), "--motion-json-check", str(motion)]))
        if not motion_meta["consistent"]: raise AssertionError("Framework rejected motion metadata")
        run([str(PROBE), "--motion-loop", str(moc), str(motion)])
        pose_result = json_line(run([str(PROBE), "--pose-check", str(moc), str(pose)]))
        if pose_result["parts"] < 1: raise AssertionError("Framework loaded no Parts")
        physics_trace = json_line(run([str(PROBE), "--physics-sequence", str(moc), str(physics)]))
        if len(physics_trace["frames"]) != 120: raise AssertionError("Framework physics trace incomplete")
        report["checks"]["attachment_loaders"] = "passed"
        rust_trace = json_line(run(["cargo", "run", "-q", "-p", "kasane-animation", "--example", "physics_sequence", "--", str(physics)]))
        max_error = max(abs(a["output"] - b["output"]) for a, b in zip(physics_trace["frames"], rust_trace["frames"]))
        if max_error > 0.0001: raise AssertionError(f"Physics drift: {max_error}")
        report["checks"]["physics_max_error"] = max_error
        report["status"] = "passed"
    except Exception as failure:
        report["error"] = str(failure)
    path = OUT / "report.json"
    path.write_text(json.dumps(report, indent=2) + "\n")
    print(f"{report['status']}: {path}")
    if "error" in report:
        print(report["error"], file=sys.stderr)
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
