#!/usr/bin/env python3
"""Compare the WGPU blend matrix with a pinned official Framework capture."""

import argparse
import json
from pathlib import Path
import subprocess

from compare_sdk_image import digest, read_png, write_png


ROOT = Path(__file__).resolve().parents[1]
REFERENCE = ROOT / "tests/fixtures/render_reference/blend_matrix_official.png"
METADATA = REFERENCE.with_suffix(".json")


def compare(actual: Path, output: Path) -> dict:
    metadata = json.loads(METADATA.read_text(encoding="utf-8"))
    if digest(REFERENCE) != metadata["png_sha256"]:
        raise RuntimeError("Pinned official blend reference changed")
    width, height, expected = read_png(REFERENCE)
    actual_width, actual_height, observed = read_png(actual)
    if (width, height) != (metadata["width"], metadata["height"]):
        raise RuntimeError("Pinned official blend reference has the wrong dimensions")
    if (actual_width, actual_height) != (width, height):
        raise RuntimeError("WGPU blend output has the wrong dimensions")
    difference = bytes(abs(a - b) for a, b in zip(expected, observed, strict=True))
    write_png(output / "difference.png", width, height, difference)
    cases = []
    count = metadata["color_modes"] * metadata["alpha_modes"] * metadata["samples_per_mode"]
    for index in range(count):
        x0 = index % metadata["samples_per_mode"] * 8
        y0 = index // metadata["samples_per_mode"] * 8
        channels = [difference[(y * width + x) * 4 + channel]
                    for y in range(y0, y0 + 8)
                    for x in range(x0, x0 + 8)
                    for channel in range(4)]
        maximum = max(channels)
        mean = sum(channels) / (len(channels) * 255)
        bad = sum(max(channels[i:i + 4]) > 13
                  for i in range(0, len(channels), 4)) / 64
        cases.append({
            "color_mode": index // (metadata["alpha_modes"] * metadata["samples_per_mode"]),
            "alpha_mode": index // metadata["samples_per_mode"] % metadata["alpha_modes"],
            "sample": index % metadata["samples_per_mode"],
            "maximum_byte_error": maximum,
            "mean_absolute_error": mean,
            "bad_pixel_fraction": bad,
            "passed": maximum <= metadata["maximum_byte_error"]
            and mean <= metadata["maximum_mean_absolute_error"]
            and bad <= metadata["maximum_bad_pixel_fraction"],
        })
    return {
        "status": "passed" if all(case["passed"] for case in cases) else "failed",
        "reference_sha256": metadata["png_sha256"],
        "actual_sha256": digest(actual),
        "cases": cases,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path,
                        default=ROOT / "target/wgpu-blend-comparison")
    args = parser.parse_args()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report_path = output / "report.json"
    report_path.unlink(missing_ok=True)
    image = output / "wgpu.png"
    try:
        with (output / "wgpu.log").open("w") as log:
            result = subprocess.run(
                ["cargo", "run", "-p", "kasane-render-wgpu", "--example", "blend_matrix",
                 "--locked", "--", str(image)],
                cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
            )
        if result.returncode:
            raise RuntimeError(f"WGPU blend capture failed; see {output / 'wgpu.log'}")
        report = compare(image, output)
    except Exception as error:
        report = {"status": "failed", "error": str(error)}
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    if "cases" in report:
        print(f"{sum(case['passed'] for case in report['cases'])}/{len(report['cases'])} "
              f"cases passed: {report_path}")
    else:
        print(f"failed: {report_path}: {report['error']}")
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
