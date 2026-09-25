#!/usr/bin/env python3
"""Compare selected Motion/Pose frames against official Framework GPU pixels."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import subprocess
import sys

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
SDK = ROOT / "third_party/CubismSdkForNative-5-r.5"
OUT = ROOT / "target/animation-frame-gpu-probe"
GPU = ROOT / "target/animation-gpu-probe/kasane_framework_gpu_probe"
SHADERS = SDK / "Framework/src/Rendering/OpenGL/Shaders/Standard"
SOURCE = ROOT / "tests/fixtures/animation_cpu"
TEXTURE = ROOT / "tests/fixtures/external_v50/texture_00.png"
MOTION = SOURCE / "minimal.motion3.json"
POSE = SOURCE / "minimal.pose3.json"


def run(command: list[str]) -> str:
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=300)
    if result.returncode:
        raise RuntimeError(f"{' '.join(command)} failed: {result.stderr or result.stdout}")
    return result.stdout


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def numeric_state(output: str) -> tuple[list[str], list[str], float]:
    parameters: list[str] = []
    parts: list[str] = []
    model_opacity = 1.0
    for line in output.splitlines():
        words = line.split()
        if words and words[0] == "PARAM":
            if words[2] not in ("ParamX", "ParamY") or int(words[1]) != len(parameters) // 2:
                raise AssertionError(f"unexpected parameter map: {line}")
            parameters += [words[1], words[3]]
        elif words and words[0] == "PART":
            if words[2] not in ("Part0", "Part1") or int(words[1]) != len(parts) // 3:
                raise AssertionError(f"unexpected Part map: {line}")
            parts += ["--part-opacity", words[1], words[3]]
        elif words and words[0] == "MODEL_OPACITY":
            model_opacity = float(words[1])
    if len(parameters) != 4 or len(parts) != 6 or model_opacity >= 1:
        raise AssertionError(f"incomplete sampled frame: {output}")
    return parameters, parts, model_opacity


def compare(a_path: Path, b_path: Path) -> dict:
    a = Image.open(a_path).convert("RGBA")
    b = Image.open(b_path).convert("RGBA")
    if a.size != b.size:
        raise AssertionError(f"frame sizes differ: {a.size} vs {b.size}")
    differences = [tuple(abs(left - right) for left, right in zip(x, y))
                   for x, y in zip(a.get_flattened_data(), b.get_flattened_data())]
    max_error = max(max(row) for row in differences)
    if max_error > 1:
        raise AssertionError(f"GPU pixel error {max_error} exceeds 1 at {a_path}")
    return {"max_channel_error": max_error,
            "nonidentical_pixels": sum(any(value for value in row) for row in differences),
            "center_kasane": a.getpixel((64, 64)),
            "center_framework": b.getpixel((64, 64))}


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    report: dict = {"status": "failed", "frames": {}, "inputs": {},
                    "model_opacity_application": "not_run"}
    try:
        run(["cmake", "-S", "tools/probes", "-B", str(GPU.parent),
             f"-DKASANE_CUBISM_ROOT={SDK}", "-DKASANE_BUILD_GPU_PROBE=ON",
             "-DCMAKE_POLICY_VERSION_MINIMUM=3.5"])
        run(["cmake", "--build", str(GPU.parent), "--target", GPU.name, "-j4"])
        variants = {"plain": SOURCE / "model.moc3"}
        for name, flag in (("offscreen", []), ("nested_offscreen", ["--nested"])):
            moc = OUT / f"{name}.moc3"
            run(["cargo", "run", "-q", "-p", "kasane-moc3", "--example",
                 "create_animation_offscreen_fixture", "--",
                 str(variants["plain"]), str(moc), *flag])
            variants[name] = moc
        for variant, moc in variants.items():
            for time in (0.25, 0.5, 0.75):
                key = f"{variant}_{time:.2f}"
                kasane = OUT / f"{key}_kasane.png"
                official = OUT / f"{key}_framework.png"
                output = run(["cargo", "run", "-q", "-p", "kasane-sdk-observe",
                              "--example", "animation_gpu_frame", "--", str(moc),
                              str(TEXTURE), str(MOTION), str(POSE), str(kasane), str(time)])
                parameters, parts, model_opacity = numeric_state(output)
                framework_output = run([str(GPU), str(SHADERS), str(moc), str(TEXTURE),
                                        str(official), "128x128", "128", *parameters, *parts,
                                        "--model-opacity", str(model_opacity)])
                framework_details = [json.loads(line) for line in framework_output.splitlines()
                                     if line.startswith("{")]
                if len(framework_details) != 1:
                    raise AssertionError(f"missing Framework GPU metadata: {framework_output}")
                expected_offscreens = {"plain": 0, "offscreen": 1, "nested_offscreen": 2}[variant]
                if framework_details[0]["offscreen_count"] != expected_offscreens:
                    raise AssertionError(f"incorrect Offscreen topology: {framework_details}")
                report["frames"][key] = {**compare(kasane, official),
                                         "model_opacity": model_opacity,
                                         "offscreen_count": expected_offscreens}
                report["inputs"][str(kasane.relative_to(ROOT))] = digest(kasane)
                report["inputs"][str(official.relative_to(ROOT))] = digest(official)
                applied_key = f"{key}_model_applied"
                applied_kasane = OUT / f"{applied_key}_kasane.png"
                applied_official = OUT / f"{applied_key}_framework.png"
                run(["cargo", "run", "-q", "-p", "kasane-sdk-observe", "--example",
                     "animation_gpu_frame", "--", str(moc), str(TEXTURE), str(MOTION),
                     str(POSE), str(applied_kasane), str(time), "--apply-model-opacity"])
                run([str(GPU), str(SHADERS), str(moc), str(TEXTURE),
                     str(applied_official), "128x128", "128", *parameters, *parts,
                     "--model-opacity", str(model_opacity),
                     "--renderer-opacity", str(model_opacity)])
                report["frames"][applied_key] = {
                    **compare(applied_kasane, applied_official),
                    "model_opacity": model_opacity,
                    "offscreen_count": expected_offscreens,
                }
                report["inputs"][str(applied_kasane.relative_to(ROOT))] = digest(applied_kasane)
                report["inputs"][str(applied_official.relative_to(ROOT))] = digest(applied_official)
        for path in (MOTION, POSE, TEXTURE, GPU, SOURCE / "model.moc3",
                     ROOT / "modules/kasane-sdk-observe/examples/animation_gpu_frame.rs",
                     ROOT / "tools/probes/framework_gpu_probe.cpp"):
            report["inputs"][str(path.relative_to(ROOT))] = digest(path)
        report["model_opacity_application"] = "passed"
        report["status"] = "passed"
    except Exception as failure:
        report["error"] = str(failure)
    (OUT / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(f"{report['status']}: {OUT / 'report.json'}")
    if "error" in report:
        print(report["error"], file=sys.stderr)
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
