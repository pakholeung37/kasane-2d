#!/usr/bin/env python3
"""Capture one real model through Godot and WGPU, then compare the results."""
import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import sys

if importlib.util.find_spec("numpy") is None or importlib.util.find_spec("PIL") is None:
    bundled = Path.home() / ".cache/codex-runtimes/codex-primary-runtime/dependencies/python/bin/python3"
    if bundled.is_file() and Path(sys.executable).resolve() != bundled.resolve():
        os.execv(str(bundled), [str(bundled), *sys.argv])
    raise SystemExit("Image comparison requires NumPy and Pillow")

from validate_s6 import compare_images


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MODEL = ROOT / "third_party/CubismSdkForNative-5-r.5/Samples/Resources/Ren/Ren.model3.json"


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run(command, log):
    with log.open("w") as stream:
        result = subprocess.run(list(map(str, command)), cwd=ROOT, stdout=stream,
                                stderr=subprocess.STDOUT, timeout=600)
    if result.returncode:
        raise RuntimeError(f"{command[0]} failed ({result.returncode}): {log}\n{log.read_text()[-3500:]}")


def compare_summaries(godot, wgpu):
    fields = ("drawable_ids", "offscreen_ids", "render_commands", "texture_ids")
    mismatches = [key for key in fields if godot[key] != wgpu[key]]
    for key in ("width", "height", "pixels_per_unit"):
        if abs(godot["canvas"][key] - wgpu["canvas"][key]) > 1e-5:
            mismatches.append("canvas." + key)
    for index, (a, b) in enumerate(zip(godot["canvas"]["origin"], wgpu["canvas"]["origin"])):
        if abs(a - b) > 1e-5:
            mismatches.append(f"canvas.origin[{index}]")
    if len(godot["parameters"]) != len(wgpu["parameters"]):
        mismatches.append("parameters.length")
    else:
        for index, (a, b) in enumerate(zip(godot["parameters"], wgpu["parameters"])):
            if a["id"] != b["id"] or abs(a["value"] - b["value"]) > 1e-5:
                mismatches.append(f"parameters[{index}]")
    return mismatches


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model3", type=Path, default=DEFAULT_MODEL)
    parser.add_argument("--output-dir", type=Path, default=ROOT / "target/wgpu-real-model")
    parser.add_argument("--godot", type=Path, default=Path(
        shutil.which("godot") or "/Applications/Godot_mono.app/Contents/MacOS/Godot"))
    parser.add_argument("--width", type=int, default=2048)
    parser.add_argument("--height", type=int, default=2048)
    parser.add_argument("--fit-long-side", type=float, default=1800)
    parser.add_argument("--texture-profile", choices=("linear_mipmap", "linear_no_mipmap"),
                        default="linear_mipmap")
    parser.add_argument("--parameter", action="append", default=[], metavar="RUNTIME_ID=VALUE",
                        help="Set a model parameter by runtime ID; repeat for multiple values")
    args = parser.parse_args()
    parameters = {}
    for assignment in args.parameter:
        runtime_id, separator, value = assignment.partition("=")
        if not separator or not runtime_id or runtime_id in parameters:
            parser.error(f"invalid or duplicate parameter: {assignment}")
        try:
            parameters[runtime_id] = float(value)
        except ValueError:
            parser.error(f"invalid parameter value: {assignment}")
        if not math.isfinite(parameters[runtime_id]):
            parser.error(f"non-finite parameter value: {assignment}")
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    model3 = args.model3.resolve()
    case = {
        "model3": str(model3),
        "width": args.width,
        "height": args.height,
        "fit_long_side": args.fit_long_side,
        "parameters": parameters,
        "texture_profile": args.texture_profile,
    }
    case_path = output / "case.json"
    case_path.write_text(json.dumps(case, indent=2) + "\n")
    report_path = output / "report.json"
    for name in ("report.json", "godot-report.json", "wgpu-report.json",
                 "godot.png", "wgpu.png", "difference.png"):
        (output / name).unlink(missing_ok=True)
    report = {"status": "failed", "case": str(case_path), "texture_profile": case["texture_profile"]}
    try:
        if not model3.is_file():
            raise FileNotFoundError(model3)
        model = json.loads(model3.read_text())
        moc = model3.parent / model["FileReferences"]["Moc"]
        textures = [model3.parent / path for path in model["FileReferences"]["Textures"]]
        report["inputs"] = {
            "model3_sha256": sha256(model3), "moc_sha256": sha256(moc),
            "texture_sha256": [sha256(texture) for texture in textures],
        }
        report["git_revision"] = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
        report["submodules"] = subprocess.check_output(
            ["git", "submodule", "status"], cwd=ROOT, text=True).strip()
        report["godot_version"] = subprocess.check_output(
            [str(args.godot), "--version"], cwd=ROOT, text=True).strip()
        sources = (
            ROOT / "Cargo.lock",
            ROOT / "apps/wgpu-validate/src/lib.rs",
            ROOT / "apps/wgpu-validate/src/main.rs",
            ROOT / "apps/wgpu-viewer/src/main.rs",
            ROOT / "tests/wgpu_real_model_capture.gd",
            ROOT / "tools/compare_wgpu_real_model.py",
            *sorted((ROOT / "modules/kasane-render-wgpu/src").glob("*.rs")),
            *sorted((ROOT / "modules/kasane-render-wgpu/shaders").glob("*.wgsl")),
        )
        report["source_sha256"] = {str(path.relative_to(ROOT)): sha256(path) for path in sources}
        run(["cargo", "build", "-p", "kasane-godot", "--locked"], output / "godot-build.log")
        run(["cargo", "run", "-p", "kasane-wgpu-validate", "--locked", "--",
             case_path, output], output / "wgpu.log")
        run([args.godot, "--path", ROOT / "tests", "--rendering-method", "gl_compatibility",
             "--script", "res://wgpu_real_model_capture.gd", "--", case_path, output],
            output / "godot.log")
        if report["inputs"] != {
            "model3_sha256": sha256(model3), "moc_sha256": sha256(moc),
            "texture_sha256": [sha256(texture) for texture in textures],
        } or report["source_sha256"] != {
            str(path.relative_to(ROOT)): sha256(path) for path in sources
        }:
            raise RuntimeError("Inputs or renderer sources changed during capture")
        godot = json.loads((output / "godot-report.json").read_text())
        wgpu = json.loads((output / "wgpu-report.json").read_text())
        mismatches = compare_summaries(godot["frame_summary"], wgpu["frame_summary"])
        mip_mismatches = [texture for texture in set(godot["mip_sha256"]) | set(wgpu["mip_sha256"])
                          if godot["mip_sha256"].get(texture) != wgpu["mip_sha256"].get(texture)]
        view_errors = [key for key in ("scale",) if abs(godot["view"][key] - wgpu["view"][key]) > 1e-4]
        view_errors += [f"offset[{i}]" for i, (a, b) in enumerate(zip(
            godot["view"]["offset"], wgpu["view"]["offset"])) if abs(a - b) > 1e-3]
        regions = [(region["name"], region["rect"]) for region in godot["object_regions"]]
        image = compare_images(output / "godot.png", output / "wgpu.png",
                               output / "difference.png", object_regions=regions)
        report.update({
            "status": "passed" if not mismatches and not mip_mismatches and not view_errors and image["status"] == "passed" else "failed",
            "frame_mismatches": mismatches,
            "mip_mismatches": mip_mismatches,
            "view_mismatches": view_errors,
            "image_comparison": image,
            "object_regions": regions,
            "godot_report": str(output / "godot-report.json"),
            "wgpu_report": str(output / "wgpu-report.json"),
            "godot_image": str(output / "godot.png"),
            "wgpu_image": str(output / "wgpu.png"),
        })
    except Exception as exc:
        report["error"] = str(exc)
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    print(f"{report['status']}: {report_path}")
    if "error" in report:
        print(report["error"])
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
