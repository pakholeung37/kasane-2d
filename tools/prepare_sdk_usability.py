#!/usr/bin/env python3
"""Prepare an isolated task-01 packet with a saved project and public docs."""

from __future__ import annotations

import argparse
import hashlib
from importlib.metadata import version
import json
from pathlib import Path
import runpy
import shutil
import sys
from tempfile import TemporaryDirectory

import kasane


ROOT = Path(__file__).resolve().parents[1]
RECIPE = ROOT / "examples/sdk/python_two_asset_recipe.py"
TASK = ROOT / "examples/sdk/usability/task-01.md"
API = ROOT / "docs/SDK-API.md"


def project_files(project: Path) -> dict[str, str]:
    result = {}
    for path in sorted(project.rglob("*")):
        if path.is_file() and path.name != ".kasane.lock":
            result[str(path.relative_to(project))] = hashlib.sha256(path.read_bytes()).hexdigest()
    return result


def prepare(destination: Path) -> Path:
    if not destination.is_absolute():
        raise ValueError("Destination must be absolute")
    if destination.exists():
        raise FileExistsError(f"Use a new run directory: {destination}")
    destination.mkdir(parents=True)
    with TemporaryDirectory(prefix="kasane-usability-fixture-") as temporary:
        temporary_root = Path(temporary)
        recipe = runpy.run_path(str(RECIPE))["run"]
        report_path = recipe(temporary_root / "fixture")
        report = json.loads(report_path.read_text(encoding="utf-8"))
        source_manifest = Path(report["project_manifest"]).resolve()
        source_project = (temporary_root / "fixture/project").resolve()
        if not source_manifest.is_relative_to(source_project):
            raise RuntimeError("Fixture manifest is outside the saved project")
        baseline_project = destination / "baseline"
        shutil.copytree(source_project, baseline_project)
        for lock in baseline_project.rglob(".kasane.lock"):
            lock.unlink()
        baseline_manifest = baseline_project / source_manifest.relative_to(source_project)
    reopened = kasane.open_project(baseline_manifest)
    if reopened.validate_structure() or reopened.diagnose_resources():
        raise RuntimeError("Copied fixture failed to reopen cleanly")
    shutil.copy2(API, destination / "SDK-API.md")
    output = destination / "output"
    output.mkdir()
    task = TASK.read_text(encoding="utf-8").format(
        baseline_manifest=baseline_manifest,
        output_directory=output,
        python_executable=sys.executable,
    )
    (destination / "TASK.md").write_text(task, encoding="utf-8")
    setup = {
        "baseline_manifest": str(baseline_manifest),
        "participant_directory": str(destination),
        "task": str(destination / "TASK.md"),
        "sdk_version": version("kasane"),
        "baseline_files_sha256": project_files(baseline_project),
    }
    (destination / "setup.json").write_text(
        json.dumps(setup, indent=2) + "\n", encoding="utf-8"
    )
    return destination / "TASK.md"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    print(prepare(args.destination))
    return 0


if __name__ == "__main__":
    sys.exit(main())
