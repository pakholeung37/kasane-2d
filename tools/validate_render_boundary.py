#!/usr/bin/env python3
"""Run targeted Godot render-boundary contracts in a disposable GPU project."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

from benchmark_kasane import ROOT, default_library, stage_project


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--library", type=Path, default=default_library())
    parser.add_argument("--godot", type=Path,
                        default=Path("/Applications/Godot_mono.app/Contents/MacOS/Godot"))
    parser.add_argument("--output-dir", type=Path, default=ROOT / "target/render-boundary-review")
    args = parser.parse_args()
    project = args.output_dir.resolve()
    library = args.library.resolve()
    # Reuse the existing GL project staging; no Ren model is loaded by these tests.
    stage_project(project, library, "ren-offscreen")
    scripts = ("preview_cache_regression", "m3c_surface_lifecycle", "render_boundary_contract",
               "m3c_viewport_chain_probe")
    for script in scripts:
        source = (ROOT / "tests" / f"{script}.gd").read_text()
        if script == "m3c_viewport_chain_probe":
            source = source.replace("res://../modules/kasane-render-godot/shaders/", "res://shaders/")
        (project / f"{script}.gd").write_text(source)
    shutil.copytree(ROOT / "modules/kasane-render-godot/shaders", project / "shaders",
                    dirs_exist_ok=True)
    report = {"status": "running", "library_sha256": hashlib.sha256(library.read_bytes()).hexdigest(),
              "suites": {}}
    report_path = project / "report.json"
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    try:
        for script in scripts:
            data = Path(tempfile.mkdtemp(prefix=f"data-{script}-", dir=project))
            result = subprocess.run(
                [str(args.godot), "--path", str(project), "--rendering-method", "gl_compatibility",
                 "--script", f"res://{script}.gd", "--", str(data)],
                text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=120)
            (project / f"{script}.log").write_text(result.stdout)
            if result.returncode or "SCRIPT ERROR:" in result.stdout or "ERROR:" in result.stdout:
                raise RuntimeError(f"{script} failed:\n{result.stdout[-4000:]}")
            if (data / "report.json").exists():
                suite = json.loads((data / "report.json").read_text())
                suite.pop("samples", None)
            elif script == "m3c_viewport_chain_probe":
                if "destination-copy regression: passed" not in result.stdout:
                    raise RuntimeError(f"{script}: missing success marker")
                suite = {"status": "passed", "negative_control": "passed"}
            else:
                lines = [line for line in result.stdout.splitlines() if line.startswith("{")]
                suite = json.loads(lines[-1])
            if suite.get("status") != "passed":
                raise RuntimeError(f"{script}: {suite}")
            report["suites"][script] = suite
            suite["data_dir"] = str(data)
            print(f"PASS {script}: {suite.get('checks', 'GPU positive/negative control')}", flush=True)
        report["status"] = "passed"
    except Exception as error:
        report["status"] = "failed"
        report["error"] = str(error)
        print(error, flush=True)
    finally:
        report_path.write_text(json.dumps(report, indent=2) + "\n")
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
