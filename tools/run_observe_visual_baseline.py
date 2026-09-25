"""Measure the shipped raw Observer path on self-authored inspection fixtures.

This runs against the installed wheel, so build/install the intended wheel
beforehand. The JSON output is a baseline record, not a new pixel policy.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from importlib.metadata import version
from pathlib import Path
import platform
import resource
from statistics import median
from time import perf_counter

import kasane
from kasane import _native

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests/fixtures/observe_inspection"


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def frame_record(frame: kasane.ObservedFrame, elapsed_ms: float) -> dict:
    pixels = [frame.rgba[index:index + 4]
              for index in range(0, len(frame.rgba), 4)]
    nonzero = [pixel for pixel in pixels if pixel[3] != 0]
    partial = [pixel for pixel in pixels if 0 < pixel[3] < 255]
    middle = ((frame.height // 2) * frame.width + frame.width // 2) * 4
    centre = frame.rgba[middle:middle + 4]

    def source_over_opaque(background: tuple[int, int, int]) -> list[int]:
        return [min(255, centre[i] +
                    (background[i] * (255 - centre[3]) + 127) // 255)
                for i in range(3)] + [255]

    return {
        "elapsed_ms": round(elapsed_ms, 3),
        "raw_sha256": sha(frame.rgba),
        "png_sha256": sha(frame.png),
        "input_sha256": frame.input_sha256,
        "adapter": {"name": frame.adapter_name, "backend": frame.adapter_backend},
        "dimensions": [frame.width, frame.height],
        "view": {"scale": frame.view_scale, "offset": frame.view_offset},
        "nonzero_alpha_pixels": len(nonzero),
        "partial_alpha_pixels": len(partial),
        "nonzero_rgb_zero_alpha_pixels": sum(
            pixel[3] == 0 and any(pixel[:3]) for pixel in pixels
        ),
        "centre_raw_rgba": list(centre),
        "derived_centre_source_over": {
            "white_255": source_over_opaque((255, 255, 255)),
            "dark_32": source_over_opaque((32, 32, 32)),
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    manifest_bytes = (FIXTURES / "manifest.json").read_bytes()
    cases = [
        ("pixel-neutral", "pixel-binding", {}),
        ("pixel-quarter", "pixel-binding", {"Shift": 0.25}),
        ("pixel-full", "pixel-binding", {"Shift": 1.0}),
        ("rotated-warp", "rotated-warp", {}),
        ("masked-offscreen", "masked-offscreen", {}),
    ]
    result = {
        "schema_version": 1,
        "kasane_version": version("kasane"),
        "native_binary_sha256": sha(Path(_native.__file__).read_bytes()),
        "platform": platform.platform(),
        "fixture_manifest_sha256": sha(manifest_bytes),
        "color_policy": "linear_unorm_no_gamma_conversion",
        "alpha_policy": "premultiplied_no_post_conversion",
        "background": "transparent",
        "cases": {},
    }
    with kasane.Observer(128, 128, 128) as observer:
        for name, project, values in cases:
            session = kasane.open_project(
                (FIXTURES / "projects" / f"{project}.kasane.json").resolve()
            )
            times = []
            frame = None
            for _ in range(3):
                start = perf_counter()
                frame = observer.observe(session, values)
                times.append((perf_counter() - start) * 1000)
            assert frame is not None
            result["cases"][name] = frame_record(frame, median(times))
            result["cases"][name]["warm_runs_ms"] = [round(t, 3) for t in times]
    result["peak_process_rss_bytes"] = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    content = json.dumps(result, indent=2, sort_keys=True, allow_nan=False) + "\n"
    if args.output:
        if not args.output.is_absolute():
            parser.error("--output must be absolute")
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(content, encoding="utf-8")
    else:
        print(content, end="")


if __name__ == "__main__":
    main()
