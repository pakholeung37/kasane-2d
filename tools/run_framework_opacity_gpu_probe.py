#!/usr/bin/env python3
"""Official Framework OpenGL opacity pixels for the synthetic two-Part model."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import subprocess
import sys

from PIL import Image, ImageChops

ROOT = Path(__file__).resolve().parents[1]
SDK = ROOT / "third_party/CubismSdkForNative-5-r.5"
BUILD = ROOT / "target/animation-gpu-probe"
OUTPUT = ROOT / "target/animation-opacity-gpu-probe"
PROBE = BUILD / "kasane_framework_gpu_probe"
SHADERS = SDK / "Framework/src/Rendering/OpenGL/Shaders/Standard"
MOC = ROOT / "tests/fixtures/animation_cpu/model.moc3"
TEXTURE = ROOT / "tests/fixtures/external_v50/texture_00.png"


def run(command: list[str], timeout: int = 300) -> str:
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, timeout=timeout)
    if result.returncode:
        raise RuntimeError(f"{' '.join(command)} failed: {result.stderr or result.stdout}")
    return result.stdout


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    report: dict = {"status": "failed", "checks": {}, "inputs": {}, "offscreen": "not_run"}
    try:
        run(["cmake", "-S", "tools/probes", "-B", str(BUILD),
             f"-DKASANE_CUBISM_ROOT={SDK}", "-DKASANE_BUILD_GPU_PROBE=ON",
             "-DCMAKE_POLICY_VERSION_MINIMUM=3.5"])
        run(["cmake", "--build", str(BUILD), "--target", "kasane_framework_gpu_probe", "-j4"])
        scenarios = {"base": [], "model_half": ["--model-opacity", "0.5"],
                     "renderer_half": ["--renderer-opacity", "0.5"],
                     "part_half": ["--part-opacity", "0", "0.5"]}
        pixels = {}
        for name, args in scenarios.items():
            path = OUTPUT / f"{name}.png"
            run([str(PROBE), str(SHADERS), str(MOC), str(TEXTURE), str(path),
                 "128x128", "128", *args])
            pixels[name] = Image.open(path).convert("RGBA")
            report["inputs"][str(path.relative_to(ROOT))] = digest(path)
        base = pixels["base"]
        if ImageChops.difference(base, pixels["model_half"]).getbbox() is not None:
            raise AssertionError("SetModelOpacity unexpectedly changed Framework renderer pixels")
        report["checks"]["model_opacity_is_not_automatic_renderer_alpha"] = "passed"
        base_center = base.getpixel((64, 64))
        renderer_center = pixels["renderer_half"].getpixel((64, 64))
        if not (base_center[3] > 0 and 0 < renderer_center[3] < base_center[3]):
            raise AssertionError(f"renderer alpha did not affect the visible center: {base_center}, {renderer_center}")
        report["checks"]["renderer_model_color_alpha"] = "passed"
        part_diff = ImageChops.difference(base, pixels["part_half"]).getbbox()
        if part_diff is None or part_diff[2] > 64 or part_diff[0] >= 64:
            raise AssertionError(f"Part0 opacity did not stay within its left region: {part_diff}")
        if pixels["part_half"].getpixel((64, 64)) != base_center:
            raise AssertionError("Part0 opacity changed the other Part's center pixel")
        report["checks"]["part_opacity_spatial_scope"] = "passed"
        offscreen_moc = OUTPUT / "model_offscreen.moc3"
        run(["cargo", "run", "-q", "-p", "kasane-moc3", "--example",
             "create_animation_offscreen_fixture", "--", str(MOC), str(offscreen_moc)])
        offscreen_scenarios = {
            "offscreen_base": [],
            "offscreen_model_half": ["--model-opacity", "0.5"],
            "offscreen_part_half": ["--part-opacity", "0", "0.5"],
            "offscreen_renderer_half": ["--renderer-opacity", "0.5"],
        }
        for name, args in offscreen_scenarios.items():
            path = OUTPUT / f"{name}.png"
            output = run([str(PROBE), str(SHADERS), str(offscreen_moc), str(TEXTURE),
                          str(path), "128x128", "128", *args])
            lines = [json.loads(line) for line in output.splitlines() if line.startswith("{")]
            if len(lines) != 1 or lines[0]["offscreen_count"] != 1:
                raise AssertionError(f"official renderer did not load one Offscreen: {lines}")
            pixels[name] = Image.open(path).convert("RGBA")
            report["inputs"][str(path.relative_to(ROOT))] = digest(path)
        if ImageChops.difference(pixels["part_half"], pixels["offscreen_base"]).getbbox() is not None:
            raise AssertionError("Offscreen opacity 0.5 differs from equivalent Part opacity 0.5")
        if ImageChops.difference(pixels["offscreen_base"], pixels["offscreen_model_half"]).getbbox() is not None:
            raise AssertionError("Model opacity changed Offscreen pixels without host application")
        left = pixels["offscreen_base"].getpixel((32, 92))
        combined_left = pixels["offscreen_part_half"].getpixel((32, 92))
        if not (left[3] > combined_left[3] > 0 and
                abs(combined_left[3] - left[3] / 2) <= 1):
            raise AssertionError(f"Part and Offscreen alpha failed to combine once each: {left}, {combined_left}")
        if pixels["offscreen_part_half"].getpixel((64, 64)) != base_center:
            raise AssertionError("Offscreen/Part composition changed the other Part")
        report["checks"]["offscreen_part_opacity_composition"] = "passed"
        report["offscreen"] = "passed"
        report["samples"] = {name: image.getpixel((64, 64)) for name, image in pixels.items()}
        report["left_samples"] = {name: image.getpixel((32, 92)) for name, image in pixels.items()}
        report["part_difference_bounds"] = part_diff
        for source in (MOC, TEXTURE, PROBE, offscreen_moc,
                       ROOT / "tools/probes/framework_gpu_probe.cpp",
                       ROOT / "modules/kasane-moc3/examples/create_animation_offscreen_fixture.rs"):
            report["inputs"][str(source.relative_to(ROOT))] = digest(source)
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
