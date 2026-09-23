#!/usr/bin/env python3
"""Compare the standalone WGPU host with the existing Godot blend shader.

The Godot process is a reference renderer only. No application integration is
needed. Both hosts render two source representations for all 18 × 5 modes.
"""

import argparse
import json
from pathlib import Path
import shutil
import subprocess

import numpy as np
from PIL import Image


ROOT = Path(__file__).resolve().parents[1]


def samples():
    inputs = [
        ([51, 153, 230, 128], [204, 77, 26, 102], 1.0, False, 1.0, False),
        ([230, 51, 153, 204], [26, 179, 77, 255], 0.4, True, 1.0, False),
        ([51, 153, 230, 128], [204, 77, 26, 102], 0.7, False, 128 / 255, False),
        ([230, 51, 153, 204], [26, 179, 77, 255], 0.4, True, 128 / 255, True),
        ([0, 0, 0, 255], [255, 255, 255, 255], 1.0, False, 1.0, False),
        ([255, 255, 255, 255], [0, 0, 0, 255], 1.0, True, 1.0, False),
        ([240, 80, 160, 0], [80, 240, 160, 128], 1.0, False, 1.0, False),
        ([240, 80, 160, 128], [0, 0, 0, 1], 0.4, True, 1.0, False),
    ]
    result = []
    for source, destination, opacity, premultiplied, mask, inverted in inputs:
        result.append({
            "source": [
                round(channel * source[3] / 255) / 255 if premultiplied else channel / 255
                for channel in source[:3]
            ] + [source[3] / 255],
            "destination": [
                round(channel * destination[3] / 255) / 255
                for channel in destination[:3]
            ] + [destination[3] / 255],
            "opacity": opacity,
            "mask": mask,
            "premultiplied": premultiplied,
            "inverted": inverted,
        })
    return result


def compare(wgpu_pixels, reference_path, difference_path):
    reference_pixels = np.asarray(Image.open(reference_path).convert("RGBA"), dtype=np.int16)
    if wgpu_pixels.shape != reference_pixels.shape:
        raise ValueError(f"Image shapes differ: {wgpu_pixels.shape} vs {reference_pixels.shape}")
    delta = np.abs(wgpu_pixels - reference_pixels)
    Image.fromarray(delta.astype(np.uint8), mode="RGBA").save(difference_path)
    results = []
    for color in range(18):
        for alpha in range(5):
            for sample in range(8):
                tile = delta[(color * 5 + alpha) * 8:(color * 5 + alpha + 1) * 8,
                             sample * 8:(sample + 1) * 8]
                maximum = int(tile.max())
                mean = float(tile.mean() / 255)
                bad = float((tile.max(axis=2) > 13).mean())
                results.append({
                    "color_mode": color,
                    "alpha_mode": alpha,
                    "source": "offscreen" if sample % 2 else "mesh",
                    "masked": sample in (2, 3),
                    "maximum_byte_error": maximum,
                    "mean_absolute_error": mean,
                    "bad_pixel_fraction": bad,
                    "passed": maximum <= 2 and mean <= 0.005 and bad <= 0.01,
                })
    return {
        "passed": all(item["passed"] for item in results),
        "reference": str(reference_path),
        "difference": str(difference_path),
        "maximum_byte_error": int(delta.max()),
        "mean_absolute_error": float(delta.mean() / 255),
        "cases": results,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, default=ROOT / "target/wgpu-blend-comparison")
    parser.add_argument("--godot", type=Path, default=Path(
        shutil.which("godot") or "/Applications/Godot_mono.app/Contents/MacOS/Godot"))
    parser.add_argument("--reference", type=Path, help="Existing Godot PNG; skip running Godot")
    parser.add_argument("--official-probe", type=Path,
                        help="Built kasane_framework_gpu_probe for direct SDK comparison")
    parser.add_argument("--sdk", type=Path,
                        default=ROOT / "third_party/CubismSdkForNative-5-r.5")
    args = parser.parse_args()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    cases = output / "cases.json"
    fixtures = samples()
    cases.write_text(json.dumps(fixtures, indent=2) + "\n")
    wgpu_image = output / "wgpu.png"
    godot_image = args.reference.resolve() if args.reference else output / "godot.png"
    with (output / "wgpu.log").open("w") as log:
        subprocess.run(
            ["cargo", "run", "-p", "kasane-render-wgpu", "--example", "blend_matrix",
             "--locked", "--", str(wgpu_image)],
            cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, check=True,
        )
    if args.reference is None:
        with (output / "godot.log").open("w") as log:
            subprocess.run(
                [str(args.godot), "--path", str(ROOT / "tests"),
                 "--rendering-method", "gl_compatibility", "--script",
                 "res://m3c_blend_matrix.gd", "--", str(cases), str(godot_image)],
                cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, check=True,
            )
    wgpu_pixels = np.asarray(Image.open(wgpu_image).convert("RGBA"), dtype=np.int16)
    godot = compare(wgpu_pixels, godot_image, output / "difference.png")
    report = {
        "passed": godot["passed"],
        "wgpu": str(wgpu_image),
        "godot": godot,
    }
    if args.official_probe:
        official_input = output / "official-cases.txt"
        lines = [str(len(fixtures))]
        for fixture in fixtures:
            effective_mask = 1 - fixture["mask"] if fixture["inverted"] else fixture["mask"]
            values = fixture["source"] + fixture["destination"] + [
                fixture["opacity"], effective_mask, int(fixture["premultiplied"])]
            lines.append(" ".join(map(str, values)))
        official_input.write_text("\n".join(lines) + "\n")
        official_image = output / "official.png"
        with (output / "official.log").open("w") as log:
            subprocess.run(
                [str(args.official_probe.resolve()),
                 str(args.sdk.resolve() / "Framework/src/Rendering/OpenGL/Shaders/Standard"),
                 "--matrix", str(official_input), str(official_image)],
                cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, check=True,
            )
        report["official"] = compare(
            wgpu_pixels, official_image, output / "official-difference.png")
        report["passed"] = report["passed"] and report["official"]["passed"]
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    for name in ("godot", "official"):
        if name in report:
            result = report[name]
            print(f"{name}: {sum(item['passed'] for item in result['cases'])}/"
                  f"{len(result['cases'])} cases passed; "
                  f"max byte error {result['maximum_byte_error']}")
    print(f"report: {output / 'report.json'}")
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
