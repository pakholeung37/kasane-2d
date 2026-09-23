#!/usr/bin/env python3
"""Run the Ren default and parameter-change WGPU/Godot image gates."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path,
                        default=ROOT / "target/wgpu-real-model-suite")
    parser.add_argument("--godot", type=Path)
    args = parser.parse_args()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report_path = output / "report.json"
    report_path.unlink(missing_ok=True)
    report = {"status": "failed", "cases": {}}
    try:
        for name, parameter in (("default", None), ("angle-x-15", "ParamAngleX=15")):
            directory = output / name
            command = [sys.executable, str(ROOT / "tools/compare_wgpu_real_model.py"),
                       "--output-dir", str(directory)]
            if args.godot:
                command += ["--godot", str(args.godot)]
            if parameter:
                command += ["--parameter", parameter]
            with (output / f"{name}.log").open("w") as log:
                result = subprocess.run(command, cwd=ROOT, stdout=log,
                                        stderr=subprocess.STDOUT, timeout=600)
            case_report = json.loads((directory / "report.json").read_text())
            report["cases"][name] = {
                "status": case_report["status"],
                "report": str(directory / "report.json"),
                "wgpu_png_sha256": sha256(directory / "wgpu.png") if result.returncode == 0 else None,
                "godot_png_sha256": sha256(directory / "godot.png") if result.returncode == 0 else None,
            }
            if result.returncode or case_report["status"] != "passed":
                raise RuntimeError(f"{name} failed; see {output / (name + '.log')}")
        default = report["cases"]["default"]
        changed = report["cases"]["angle-x-15"]
        if default["wgpu_png_sha256"] == changed["wgpu_png_sha256"] or \
                default["godot_png_sha256"] == changed["godot_png_sha256"]:
            raise RuntimeError("ParamAngleX=15 did not change both renderer images")
        report["status"] = "passed"
    except Exception as exc:
        report["error"] = str(exc)
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    print(f"{report['status']}: {report_path}")
    if "error" in report:
        print(report["error"])
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
