#!/usr/bin/env python3
"""Validate the shipped SDK wheel outside the source tree and retain evidence.

The gate covers S3/S4, all three S5 recipes, optional dual-Core numerical
parity, and an optional comparison with a pinned external GPU reference.
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
INSPECTION_TESTS = tuple(
    ROOT / f"modules/kasane-python/tests/test_observe_o{stage}.py"
    for stage in range(3, 8)
)
RECIPE = ROOT / "examples/sdk/python_observe_recipe.py"
TWO_ASSET_RECIPE = ROOT / "examples/sdk/python_two_asset_recipe.py"
IMPORT_EDIT_RECIPE = ROOT / "examples/sdk/python_import_edit_recipe.py"
AGENT_DRAFT = ROOT / "examples/sdk/python_agent_draft.py"
AGENT_REPAIR = ROOT / "examples/sdk/python_agent_repair.py"
REFERENCE_CAPTURE = ROOT / "examples/sdk/python_reference_capture.py"
IMAGE_COMPARATOR = ROOT / "tools/compare_sdk_image.py"
IMAGE_REFERENCE = ROOT / "tests/fixtures/render_reference/external_v50_default.png"
IMAGE_REFERENCE_METADATA = IMAGE_REFERENCE.with_suffix(".json")
TEXTURE = ROOT / "modules/kasane-python/tests/fixtures/asymmetric-2x2.png"
SECOND_TEXTURE = ROOT / "modules/kasane-python/tests/fixtures/texture_00.png"
EXTERNAL_MODEL3 = ROOT / "tests/fixtures/external_v50/model.model3.json"
EXTERNAL_MOC3 = ROOT / "tests/fixtures/external_v50/model.moc3"


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
    input_text: str | None = None, timeout: int = 180,
) -> str:
    try:
        result = subprocess.run(
            args, cwd=cwd, env=env, capture_output=True, text=True, timeout=timeout,
            input=input_text,
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


def validate_two_asset(path: Path, run: Path) -> dict:
    recipe = json.loads(path.read_text(encoding="utf-8"))
    if recipe["status"] != "passed" or len(recipe["assets"]) != 2:
        raise RuntimeError("Two-asset creation recipe did not pass")
    if {(asset["width"], asset["height"]) for asset in recipe["assets"]} != {(2, 2), (4, 4)}:
        raise RuntimeError("Two-asset recipe did not retain both source sizes")
    if recipe["maximum_midpoint_pixel_error"] > 0.05:
        raise RuntimeError("Two-asset midpoint exceeds pixel tolerance")
    if len(recipe["samples"]) != 3 or len(recipe["transform_ids"]) != 2:
        raise RuntimeError("Two-asset recipe lacks required samples or transforms")
    manifest = Path(recipe["project_manifest"])
    exported = Path(recipe["export_moc3"])
    if not manifest.is_relative_to(run) or not exported.is_relative_to(run):
        raise RuntimeError("Two-asset recipe wrote outside the evidence directory")
    if not manifest.is_file() or not exported.is_file() or sha256(exported) != recipe["export_sha256"]:
        raise RuntimeError("Two-asset project or exported MOC3 is missing")
    return {
        "status": "passed", "report": str(path.relative_to(run)),
        "maximum_midpoint_pixel_error": recipe["maximum_midpoint_pixel_error"],
        "export_sha256": recipe["export_sha256"],
    }


def validate_import_edit(path: Path, run: Path) -> dict:
    recipe = json.loads(path.read_text(encoding="utf-8"))
    if recipe["status"] != "passed" or not all(recipe["preserved"].values()):
        raise RuntimeError("External import edit did not preserve untouched content")
    if len(recipe["original_ids"]["mesh"]) != 36:
        raise RuntimeError("External import mesh ID was not recorded")
    if recipe["before_export_sha256"] == recipe["after_export_sha256"]:
        raise RuntimeError("External import edit did not change the export")
    expected_inputs = {
        "model3_sha256": sha256(EXTERNAL_MODEL3),
        "moc3_sha256": sha256(EXTERNAL_MOC3),
        "texture_sha256": sha256(SECOND_TEXTURE),
    }
    if recipe["fixture"] != expected_inputs:
        raise RuntimeError("External import fixture hash mismatch")
    for key in ("before_manifest", "after_manifest", "before_export", "after_export"):
        artifact = Path(recipe[key])
        if not artifact.is_relative_to(run) or not artifact.is_file():
            raise RuntimeError(f"External import artifact missing: {key}")
    if sha256(Path(recipe["before_export"])) != recipe["before_export_sha256"]:
        raise RuntimeError("External import before export hash mismatch")
    if sha256(Path(recipe["after_export"])) != recipe["after_export_sha256"]:
        raise RuntimeError("External import after export hash mismatch")
    return {
        "status": "passed", "report": str(path.relative_to(run)),
        "mesh_id": recipe["original_ids"]["mesh"],
        "before_export_sha256": recipe["before_export_sha256"],
        "after_export_sha256": recipe["after_export_sha256"],
    }


def validate_agent_repair(path: Path, run: Path) -> dict:
    result = json.loads(path.read_text(encoding="utf-8"))
    problem = result["problem"]
    correction = result["correction"]
    if result["status"] != "passed" or problem["code"] != "TARGET_TOO_SMALL":
        raise RuntimeError("Agent self-check did not identify a concrete visual problem")
    if not (problem["before_visible_width"] < problem["minimum_visible_width"]
            <= correction["after_visible_width"]):
        raise RuntimeError("Agent local repair did not pass the visible-width threshold")
    if correction["after_opaque_pixels"] <= problem["before_opaque_pixels"]:
        raise RuntimeError("Agent repair did not increase visible crop pixels")
    for key in (problem["evidence_crop"], correction["evidence_crop"],
                correction["project_manifest"], correction["observation_report"]):
        artifact = Path(key)
        if not artifact.is_relative_to(run) or not artifact.is_file():
            raise RuntimeError(f"Agent repair evidence missing: {artifact}")
    return {
        "status": "passed", "report": str(path.relative_to(run)),
        "object_id": problem["object_id"],
        "existing_transform_id": correction["existing_transform_id"],
        "before_visible_width": problem["before_visible_width"],
        "after_visible_width": correction["after_visible_width"],
    }


def validate_official_core(
    probe: Path, creation_report: Path, run: Path, outside: Path,
    environment: dict[str, str], logs: Path, provider: str,
) -> dict:
    recipe = json.loads(creation_report.read_text(encoding="utf-8"))
    moc3 = Path(recipe["export_moc3"])
    output = command(
        f"{provider}-core", [str(probe), str(moc3)], cwd=outside,
        env=environment, logs=logs, input_text="3\n0\n0.5\n1\n",
    )
    core = json.loads(next(line for line in output.splitlines()
                           if line.startswith('{"core_version"')))
    if len(core["samples"]) != 3:
        raise RuntimeError("Official Core probe returned the wrong sample count")
    comparisons = []
    maximum_pixel_error = 0.0
    for index, value in enumerate((0.0, 0.5, 1.0)):
        expected = recipe["samples"][str(value)]
        actual = {drawable["runtime_id"]: drawable
                  for drawable in core["samples"][index]}
        if set(expected) != set(actual):
            raise RuntimeError(f"Official Core drawable IDs differ at sample {value}")
        for mesh_id, positions in expected.items():
            observed = actual[mesh_id]["positions"]
            if len(positions) != len(observed):
                raise RuntimeError(f"Official Core vertex count differs for {mesh_id}")
            for vertex, (sdk_point, core_point) in enumerate(zip(positions, observed, strict=True)):
                for axis, (sdk_value, core_value) in enumerate(zip(sdk_point, core_point, strict=True)):
                    difference = abs(sdk_value - core_value)
                    pixel_error = difference * 10
                    maximum_pixel_error = max(maximum_pixel_error, pixel_error)
                    tolerance = 1e-5 + 1e-5 * max(abs(sdk_value), abs(core_value))
                    comparisons.append({
                        "sample": value, "mesh_id": mesh_id, "vertex": vertex,
                        "axis": axis, "expected_sdk": sdk_value,
                        "actual_official": core_value,
                        "absolute_error": difference, "pixel_error": pixel_error,
                        "status": "passed" if difference <= tolerance and pixel_error <= 0.05
                                  else "failed",
                    })
    destination = run / f"{provider}-core-comparison.json"
    destination.write_text(json.dumps({
        "core_version": core["core_version"], "source_moc3_sha256": sha256(moc3),
        "checks": comparisons,
    }, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    if any(item["status"] != "passed" for item in comparisons):
        raise RuntimeError("Official Core positions differ from SDK evaluation")
    return {
        "status": "passed", "probe_sha256": sha256(probe),
        "comparison": destination.name, "coordinates": len(comparisons),
        "maximum_pixel_error": maximum_pixel_error,
        "core_version": core["core_version"],
    }


def validate_official_import_edit(
    probe: Path, import_report: Path, run: Path, outside: Path,
    environment: dict[str, str], logs: Path, provider: str,
) -> dict:
    recipe = json.loads(import_report.read_text(encoding="utf-8"))
    mesh_id = recipe["original_ids"]["mesh"]
    runtime_id = recipe["original_ids"]["mesh_runtime_id"]
    comparisons = []
    maximum_pixel_error = 0.0
    for stage in ("before", "after"):
        moc3 = Path(recipe[f"{stage}_export"])
        output = command(
            f"{provider}-import-{stage}", [str(probe), str(moc3)], cwd=outside,
            env=environment, logs=logs, input_text="2\n0 0\n0.5 0.5\n",
        )
        core = json.loads(next(line for line in output.splitlines()
                               if line.startswith('{"core_version"')))
        if len(core["samples"]) != 2:
            raise RuntimeError(f"Official Core import {stage} returned wrong sample count")
        for sample_index, expected in enumerate(recipe[f"{stage}_samples"]):
            drawables = {item["runtime_id"]: item for item in core["samples"][sample_index]}
            if runtime_id not in drawables:
                raise RuntimeError(f"Official Core import {stage} lost original runtime ID")
            actual = drawables[runtime_id]["positions"]
            if len(actual) != len(expected):
                raise RuntimeError(f"Official Core import {stage} vertex count changed")
            for vertex, (sdk_point, core_point) in enumerate(zip(expected, actual, strict=True)):
                for axis, (sdk_value, core_value) in enumerate(zip(sdk_point, core_point, strict=True)):
                    difference = abs(sdk_value - core_value)
                    pixel_error = difference * 100
                    maximum_pixel_error = max(maximum_pixel_error, pixel_error)
                    tolerance = 1e-5 + 1e-5 * max(abs(sdk_value), abs(core_value))
                    comparisons.append({
                        "stage": stage, "sample_index": sample_index,
                        "mesh_id": mesh_id, "vertex": vertex, "axis": axis,
                        "expected_sdk": sdk_value, "actual_official": core_value,
                        "absolute_error": difference, "pixel_error": pixel_error,
                        "status": "passed" if difference <= tolerance and pixel_error <= 0.05
                                  else "failed",
                    })
    destination = run / f"{provider}-import-comparison.json"
    destination.write_text(json.dumps({
        "fixture_moc3_sha256": recipe["fixture"]["moc3_sha256"],
        "before_export_sha256": recipe["before_export_sha256"],
        "after_export_sha256": recipe["after_export_sha256"],
        "checks": comparisons,
    }, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    if any(item["status"] != "passed" for item in comparisons):
        raise RuntimeError("Official Core import positions differ from SDK evaluation")
    return {
        "status": "passed", "comparison": destination.name,
        "coordinates": len(comparisons), "maximum_pixel_error": maximum_pixel_error,
    }


def load_image_reference() -> dict:
    reference = json.loads(IMAGE_REFERENCE_METADATA.read_text(encoding="utf-8"))
    expected = {
        "model3_sha256": sha256(EXTERNAL_MODEL3),
        "moc_sha256": sha256(EXTERNAL_MOC3),
        "texture_sha256": sha256(SECOND_TEXTURE),
        "png_sha256": sha256(IMAGE_REFERENCE),
    }
    if any(reference.get(key) != value for key, value in expected.items()):
        raise RuntimeError("Pinned image reference or model inputs changed")
    if reference.get("texture_profile") != "linear_no_mipmap":
        raise RuntimeError("Pinned image reference has an unexpected texture profile")
    return reference


def validate_image_reference(
    installed: Path, run: Path, outside: Path,
    environment: dict[str, str], logs: Path,
) -> dict:
    reference = load_image_reference()
    observation_output = command(
        "sdk-reference-capture",
        [str(installed), str(REFERENCE_CAPTURE), str(EXTERNAL_MODEL3),
         str(run / "sdk-reference-observation")],
        cwd=outside, env=environment, logs=logs,
    )
    observation_report_path = Path(observation_output.splitlines()[-1]).resolve(strict=True)
    if not observation_report_path.is_relative_to(run / "sdk-reference-observation"):
        raise RuntimeError("SDK reference capture wrote outside the evidence directory")
    observation = json.loads(observation_report_path.read_text(encoding="utf-8"))
    if observation["status"] != "frames_complete" or len(observation["frames"]) != 1:
        raise RuntimeError("SDK reference capture is incomplete")
    frame = observation["frames"][0]
    view = reference["view"]
    if len(frame["crops"]) != 1 or any(
        frame["view"][key] != view[key] for key in ("width", "height")
    ):
        raise RuntimeError("SDK reference focus/view is incomplete")
    actual = observation_report_path.parent / frame["path"]
    if sha256(actual) != frame["sha256"]:
        raise RuntimeError("SDK reference image hash mismatch")
    if abs(view["scale"] - frame["view"]["scale"]) > 1e-5:
        raise RuntimeError("SDK and reference view scale differ")
    if any(abs(a - b) > 1e-3 for a, b in zip(
        view["offset"], frame["view"]["offset"], strict=True,
    )):
        raise RuntimeError("SDK and reference view offsets differ")
    bounds = frame["crops"][0]["bounds"]
    comparison_output = command(
        "sdk-image-comparison",
        [sys.executable, str(IMAGE_COMPARATOR), str(IMAGE_REFERENCE), str(actual),
         "--crop", *map(str, bounds), "--output", str(run / "sdk-image-comparison")],
        cwd=outside, env=environment, logs=logs,
    )
    comparison_path = Path(comparison_output.splitlines()[-1]).resolve(strict=True)
    comparison = json.loads(comparison_path.read_text(encoding="utf-8"))
    if comparison["status"] != "passed":
        raise RuntimeError("SDK image differs from the pinned external reference")
    return {
        "status": "passed", "reference": str(IMAGE_REFERENCE.relative_to(ROOT)),
        "reference_sha256": reference["png_sha256"],
        "observation_report": str(observation_report_path.relative_to(run)),
        "comparison": str(comparison_path.relative_to(run)),
        "adapter": frame["adapter"],
        "checks": comparison["checks"],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wheel", required=True, type=Path)
    parser.add_argument("--python", type=Path, default=Path(sys.executable))
    parser.add_argument("--uv", default="uv", help="uv executable used to manage the isolated environment")
    parser.add_argument("--output", type=Path, default=ROOT / "target/sdk-acceptance")
    parser.add_argument("--require-gpu", action="store_true")
    parser.add_argument("--official-probe", type=Path)
    parser.add_argument("--require-official-core", action="store_true")
    parser.add_argument("--purism-probe", type=Path)
    parser.add_argument("--require-purism-core", action="store_true")
    parser.add_argument("--require-image-reference", action="store_true")
    parser.add_argument("--inspection", action="store_true",
                        help="Install the inspection extra and run the Observe O3–O7 wheel suite")
    parser.add_argument("--require-inspection", action="store_true",
                        help="Require a GPU wheel and the Observe O3–O7 inspection suite")
    parser.add_argument("--full", action="store_true",
                        help="Require GPU, both Core probes, and image reference")
    args = parser.parse_args()
    if args.full:
        args.require_gpu = True
        args.require_official_core = True
        args.require_purism_core = True
        args.require_image_reference = True
    if args.require_inspection:
        args.inspection = True
        args.require_gpu = True
    wheel = args.wheel.resolve(strict=True)
    python = args.python.resolve(strict=True)
    official_probe = args.official_probe.resolve(strict=True) if args.official_probe else None
    purism_probe = args.purism_probe.resolve(strict=True) if args.purism_probe else None
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    run = output / uuid4().hex
    run.mkdir()
    logs = run / "logs"
    logs.mkdir()
    report_path = run / "report.json"
    inspection_only = args.inspection and not args.full
    input_paths = ((CPU_TEST, GPU_TEST, *INSPECTION_TESTS,
                    ROOT / "tests/fixtures/observe_inspection/manifest.json")
                   if inspection_only else
                   (CPU_TEST, GPU_TEST, *INSPECTION_TESTS, RECIPE, TWO_ASSET_RECIPE,
                    IMPORT_EDIT_RECIPE, AGENT_DRAFT, AGENT_REPAIR,
                    REFERENCE_CAPTURE, IMAGE_REFERENCE, IMAGE_REFERENCE_METADATA,
                    IMAGE_COMPARATOR, TEXTURE, SECOND_TEXTURE,
                    EXTERNAL_MODEL3, EXTERNAL_MOC3))
    report = {
        "schema_version": 1,
        "profile": "full" if args.full else "inspection" if inspection_only else "custom",
        "status": "running",
        "source_revision": git("rev-parse", "HEAD"),
        "workspace_dirty": bool(git("status", "--porcelain")),
        "platform": platform.platform(),
        "wheel": {"path": str(wheel), "sha256": sha256(wheel)},
        "inputs": {
            str(path.relative_to(ROOT)): sha256(path)
            for path in input_paths
        },
        "checks": {},
    }
    if inspection_only:
        try:
            with TemporaryDirectory(prefix="kasane-inspection-validate-") as temporary:
                outside = Path(temporary)
                environment = os.environ.copy()
                for name in ("PYTHONPATH", "PYTHONHOME", "VIRTUAL_ENV", "UV_PROJECT_ENVIRONMENT"):
                    environment.pop(name, None)
                report["uv_version"] = command(
                    "uv-version", [args.uv, "--version"], cwd=outside,
                    env=environment, logs=logs,
                )
                command("venv", [args.uv, "venv", "--python", str(python),
                                 str(outside / "venv")],
                        cwd=outside, env=environment, logs=logs)
                installed = outside / "venv" / (
                    "Scripts/python.exe" if os.name == "nt" else "bin/python")
                command("install-inspection", [args.uv, "pip", "install", "--python",
                                               str(installed), f"{wheel}[inspection]"],
                        cwd=outside, env=environment, logs=logs)
                capabilities = json.loads(command(
                    "capabilities", [str(installed), "-c",
                                     "import json, kasane; print(json.dumps(kasane.capabilities()))"],
                    cwd=outside, env=environment, logs=logs,
                ))
                report["capabilities"] = capabilities
                if not capabilities.get("gpu_observation"):
                    report["checks"]["gpu"] = {
                        "status": "not_run", "reason": "GPU observation unavailable",
                    }
                    report["checks"]["inspection"] = {
                        "status": "not_run", "reason": "GPU observation unavailable",
                    }
                    if args.require_inspection:
                        raise RuntimeError("Inspection release requires a GPU Observe wheel")
                else:
                    for name, command_args in (
                        ("gpu-tests", [str(installed), str(GPU_TEST), "-q"]),
                        ("inspection-tests", [str(installed), "-m", "unittest", "discover",
                                              "-s", str(GPU_TEST.parent), "-p",
                                              "test_observe_o*.py", "-q"]),
                    ):
                        command(name, command_args, cwd=outside, env=environment,
                                logs=logs, timeout=300)
                    report["checks"]["gpu"] = {
                        "status": "passed",
                        "tests": test_count((logs / "gpu-tests.log").read_text(), "gpu-tests"),
                    }
                    report["checks"]["inspection"] = {
                        "status": "passed",
                        "tests": test_count((logs / "inspection-tests.log").read_text(),
                                            "inspection-tests"),
                        "extra": "Pillow==12.3.0",
                    }
                command("cpu-tests", [str(installed), str(CPU_TEST), "-q"],
                        cwd=outside, env=environment, logs=logs)
                report["checks"]["cpu"] = {
                    "status": "passed",
                    "tests": test_count((logs / "cpu-tests.log").read_text(), "cpu-tests"),
                }
            report["status"] = ("passed" if report["checks"]["inspection"]["status"] ==
                                "passed" else "partial")
        except Exception as failure:
            report["status"] = "failed"
            report["failure"] = {"type": type(failure).__name__, "message": str(failure)}
        report_path.write_text(json.dumps(report, indent=2, allow_nan=False) + "\n",
                               encoding="utf-8")
        print(report_path)
        return 1 if report["status"] == "failed" else 0
    try:
        with TemporaryDirectory(prefix="kasane-sdk-validate-") as temporary:
            outside = Path(temporary)
            environment = os.environ.copy()
            environment.pop("PYTHONPATH", None)
            environment.pop("PYTHONHOME", None)
            environment.pop("VIRTUAL_ENV", None)
            environment.pop("UV_PROJECT_ENVIRONMENT", None)
            report["uv_version"] = command("uv-version", [args.uv, "--version"],
                    cwd=outside, env=environment, logs=logs)
            command("venv", [args.uv, "venv", "--python", str(python), str(outside / "venv")],
                    cwd=outside, env=environment, logs=logs)
            installed = outside / "venv" / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
            command("install", [args.uv, "pip", "install", "--python", str(installed), "--no-index",
                                "--no-deps", str(wheel)],
                    cwd=outside, env=environment, logs=logs)
            if args.inspection:
                command("install-inspection-extra", [args.uv, "pip", "install", "--python",
                                                    str(installed), "Pillow==12.3.0"],
                        cwd=outside, env=environment, logs=logs)
            command("cpu-tests", [str(installed), str(CPU_TEST), "-q"],
                    cwd=outside, env=environment, logs=logs)
            report["checks"]["cpu"] = {
                "status": "passed",
                "tests": test_count((logs / "cpu-tests.log").read_text(), "cpu-tests"),
            }
            creation_output = command(
                "two-asset-recipe",
                [str(installed), str(TWO_ASSET_RECIPE), str(run / "two-asset")],
                cwd=outside, env=environment, logs=logs,
            )
            creation_report = Path(creation_output.splitlines()[-1]).resolve(strict=True)
            report["checks"]["creation_export"] = validate_two_asset(creation_report, run)
            for provider, probe, required in (
                ("official", official_probe, args.require_official_core),
                ("purism", purism_probe, args.require_purism_core),
            ):
                key = f"{provider}_core"
                if probe:
                    report["checks"][key] = validate_official_core(
                        probe, creation_report, run, outside, environment, logs, provider,
                    )
                else:
                    report["checks"][key] = {
                        "status": "not_run", "reason": f"{provider} Core probe not supplied",
                    }
                    if required:
                        raise RuntimeError(f"{provider} Core comparison is required but no probe was supplied")
            import_output = command(
                "import-edit-recipe",
                [str(installed), str(IMPORT_EDIT_RECIPE), str(run / "import-edit")],
                cwd=outside, env=environment, logs=logs,
            )
            import_report = Path(import_output.splitlines()[-1]).resolve(strict=True)
            report["checks"]["import_edit"] = validate_import_edit(import_report, run)
            for provider, probe in (("official", official_probe), ("purism", purism_probe)):
                key = f"{provider}_core_import"
                if probe:
                    report["checks"][key] = validate_official_import_edit(
                        probe, import_report, run, outside, environment, logs, provider,
                    )
                else:
                    report["checks"][key] = {
                        "status": "not_run", "reason": f"{provider} Core probe not supplied",
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
                if args.inspection:
                    command(
                        "inspection-tests",
                        [str(installed), "-m", "unittest", "discover", "-s",
                         str(GPU_TEST.parent), "-p", "test_observe_o*.py", "-q"],
                        cwd=outside, env=environment, logs=logs, timeout=300,
                    )
                    report["checks"]["inspection"] = {
                        "status": "passed",
                        "tests": test_count((logs / "inspection-tests.log").read_text(),
                                            "inspection-tests"),
                        "extra": "Pillow==12.3.0",
                    }
                else:
                    report["checks"]["inspection"] = {
                        "status": "not_run", "reason": "Inspection suite not requested",
                    }
                handoff_output = command(
                    "agent-draft", [str(installed), str(AGENT_DRAFT), str(run / "agent")],
                    cwd=outside, env=environment, logs=logs,
                )
                handoff = Path(handoff_output.splitlines()[-1]).resolve(strict=True)
                if not handoff.is_relative_to(run / "agent"):
                    raise RuntimeError("Agent draft wrote outside the evidence directory")
                repair_output = command(
                    "agent-repair", [str(installed), str(AGENT_REPAIR), str(handoff),
                                     str(run / "agent/repair")],
                    cwd=outside, env=environment, logs=logs,
                )
                repair_report = Path(repair_output.splitlines()[-1]).resolve(strict=True)
                report["checks"]["agent_repair"] = validate_agent_repair(repair_report, run)
                if args.require_image_reference:
                    report["checks"]["image_reference"] = validate_image_reference(
                        installed, run, outside, environment, logs,
                    )
                else:
                    report["checks"]["image_reference"] = {
                        "status": "not_run", "reason": "Image reference comparison not requested",
                    }
            else:
                report["checks"]["gpu"] = {"status": "not_run", "reason": "GPU observation unavailable"}
                report["checks"]["inspection"] = {
                    "status": "not_run", "reason": "GPU observation unavailable",
                }
                report["checks"]["agent_repair"] = {
                    "status": "not_run", "reason": "GPU observation unavailable",
                }
                report["checks"]["image_reference"] = {
                    "status": "not_run", "reason": "GPU observation unavailable",
                }
                if args.require_image_reference:
                    raise RuntimeError("Image reference requires GPU observation")
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
