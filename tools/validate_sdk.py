#!/usr/bin/env python3
"""Run the shipped SDK wheel outside the source tree and retain CPU/GPU evidence.

This is the S3/S4 gate. S5 import and agent-edit flows are tracked separately.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
from tempfile import TemporaryDirectory
from uuid import uuid4


ROOT = Path(__file__).resolve().parents[1]
CPU_TEST = ROOT / "modules/kasane-python/tests/test_cpu.py"
GPU_TEST = ROOT / "modules/kasane-python/tests/test_observe.py"
RECIPE = ROOT / "examples/sdk/python_observe_recipe.py"
TEXTURE = ROOT / "examples/sdk/asymmetric-2x2.png"


def sha256(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def git(*args: str) -> str:
    result = subprocess.run(
        ["git", *args], cwd=ROOT, capture_output=True, text=True, check=True,
    )
    return result.stdout.strip()


def test_count(output: str, name: str) -> int:
    match = re.search(r"Ran (\d+) tests? in ", output)
    if match is None:
        raise RuntimeError(f"{name}: test count missing from output")
    return int(match.group(1))


def command(
    name: str, args: list[str], *, cwd: Path, env: dict[str, str], logs: Path,
) -> str:
    try:
        result = subprocess.run(
            args, cwd=cwd, env=env, capture_output=True, text=True, timeout=180,
        )
    except subprocess.TimeoutExpired as failure:
        (logs / f"{name}.log").write_text(
            f"Timed out after {failure.timeout}s\n", encoding="utf-8",
        )
        raise RuntimeError(f"{name}: timed out") from failure
    output = result.stdout + result.stderr
    (logs / f"{name}.log").write_text(output, encoding="utf-8")
    if result.returncode:
        raise RuntimeError(f"{name}: exit {result.returncode}; see logs/{name}.log")
    return result.stdout.strip()


def validate_observation(path: Path) -> dict:
    report = json.loads(path.read_text(encoding="utf-8"))
    if report["status"] != "frames_complete":
        raise RuntimeError("GPU recipe did not complete all frames")
    frames = report["frames"]
    if len(frames) != 3 or len(report["samples"]) != 3:
        raise RuntimeError("GPU recipe did not produce three parameter samples")
    if len({entry["sha256"] for entry in frames}) != 3:
        raise RuntimeError("GPU recipe frames did not change across samples")
    if len({entry["input_sha256"] for entry in frames}) != 3:
        raise RuntimeError("GPU recipe input fingerprints did not change")
    directory = path.parent
    crops = 0
    for entry in frames:
        image = directory / entry["path"]
        if not image.is_file() or sha256(image) != entry["sha256"]:
            raise RuntimeError(f"GPU frame missing or hash mismatch: {image}")
        if not image.read_bytes().startswith(b"\x89PNG\r\n\x1a\n"):
            raise RuntimeError(f"GPU frame is not PNG: {image}")
        if entry["alpha_convention"] != "premultiplied_no_post_conversion":
            raise RuntimeError("GPU report has unexpected alpha convention")
        if not entry["adapter"]["name"]:
            raise RuntimeError("GPU adapter was not recorded")
        for crop in entry["crops"]:
            artifact = directory / crop["path"]
            if not artifact.is_file() or sha256(artifact) != crop["sha256"]:
                raise RuntimeError(f"GPU crop missing or hash mismatch: {artifact}")
            crops += 1
    sheet = directory / report["contact_sheet"]["path"]
    if not sheet.is_file() or sha256(sheet) != report["contact_sheet"]["sha256"]:
        raise RuntimeError("GPU contact sheet missing or hash mismatch")
    if crops != 3:
        raise RuntimeError(f"GPU recipe expected three focus crops, got {crops}")
    return {
        "status": "passed", "report": str(path.relative_to(path.parents[2])),
        "frames": len(frames), "crops": crops,
        "adapter": frames[0]["adapter"],
        "sdk_binary_sha256": report["sdk_binary_sha256"],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wheel", required=True, type=Path)
    parser.add_argument("--python", type=Path, default=Path(sys.executable))
    parser.add_argument("--output", type=Path, default=ROOT / "target/sdk-acceptance")
    parser.add_argument("--require-gpu", action="store_true")
    args = parser.parse_args()
    wheel = args.wheel.resolve(strict=True)
    python = args.python.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    run = output / uuid4().hex
    run.mkdir()
    logs = run / "logs"
    logs.mkdir()
    report_path = run / "report.json"
    report = {
        "schema_version": 1,
        "status": "running",
        "source_revision": git("rev-parse", "HEAD"),
        "workspace_dirty": bool(git("status", "--porcelain")),
        "platform": platform.platform(),
        "wheel": {"path": str(wheel), "sha256": sha256(wheel)},
        "inputs": {
            str(path.relative_to(ROOT)): sha256(path)
            for path in (CPU_TEST, GPU_TEST, RECIPE, TEXTURE)
        },
        "checks": {},
    }
    try:
        with TemporaryDirectory(prefix="kasane-sdk-validate-") as temporary:
            outside = Path(temporary)
            environment = os.environ.copy()
            environment.pop("PYTHONPATH", None)
            command("venv", [str(python), "-m", "venv", str(outside / "venv")],
                    cwd=outside, env=environment, logs=logs)
            installed = outside / "venv/bin/python"
            command("install", [str(installed), "-m", "pip", "install", "--no-index",
                                "--no-deps", str(wheel)],
                    cwd=outside, env=environment, logs=logs)
            command("cpu-tests", [str(installed), str(CPU_TEST), "-q"],
                    cwd=outside, env=environment, logs=logs)
            report["checks"]["cpu"] = {
                "status": "passed",
                "tests": test_count((logs / "cpu-tests.log").read_text(), "cpu-tests"),
            }
            capabilities = json.loads(command(
                "capabilities", [str(installed), "-c",
                                 "import json, kasane; print(json.dumps(kasane.capabilities()))"],
                cwd=outside, env=environment, logs=logs,
            ))
            report["capabilities"] = capabilities
            if capabilities.get("gpu_observation"):
                command("gpu-tests", [str(installed), str(GPU_TEST), "-q"],
                        cwd=outside, env=environment, logs=logs)
                recipe_output = command(
                    "gpu-recipe", [str(installed), str(RECIPE), str(run / "observation")],
                    cwd=outside, env=environment, logs=logs,
                )
                recipe_report = Path(recipe_output.splitlines()[-1]).resolve(strict=True)
                if not recipe_report.is_relative_to(run / "observation"):
                    raise RuntimeError("GPU recipe wrote outside the evidence directory")
                report["checks"]["gpu"] = validate_observation(recipe_report)
                report["checks"]["gpu"]["tests"] = test_count(
                    (logs / "gpu-tests.log").read_text(), "gpu-tests",
                )
            else:
                report["checks"]["gpu"] = {"status": "not_run", "reason": "GPU observation unavailable"}
                if args.require_gpu:
                    raise RuntimeError("GPU observation is required but unavailable")
        report["status"] = "passed" if report["checks"]["gpu"]["status"] == "passed" else "partial"
    except Exception as failure:
        report["status"] = "failed"
        report["failure"] = {"type": type(failure).__name__, "message": str(failure)}
    report_path.write_text(json.dumps(report, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    print(report_path)
    return 1 if report["status"] == "failed" else 0


if __name__ == "__main__":
    raise SystemExit(main())
