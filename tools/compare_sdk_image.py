#!/usr/bin/env python3
"""Compare a clean SDK RGBA8 PNG to an independent reference and retain diff pixels."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import struct
import zlib


def digest(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def paeth(a: int, b: int, c: int) -> int:
    prediction = a + b - c
    distances = (abs(prediction - a), abs(prediction - b), abs(prediction - c))
    return (a, b, c)[distances.index(min(distances))]


def read_png(path: Path) -> tuple[int, int, bytes]:
    data = path.read_bytes()
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        raise ValueError(f"Not a PNG: {path}")
    offset = 8
    width = height = 0
    compressed = bytearray()
    while offset < len(data):
        size = struct.unpack_from(">I", data, offset)[0]
        kind = data[offset + 4:offset + 8]
        payload = data[offset + 8:offset + 8 + size]
        if kind == b"IHDR":
            width, height, depth, color, compression, filtering, interlace = struct.unpack(
                ">IIBBBBB", payload,
            )
            if (depth, color, compression, filtering, interlace) != (8, 6, 0, 0, 0):
                raise ValueError("Only noninterlaced RGBA8 PNG is supported")
        elif kind == b"IDAT":
            compressed.extend(payload)
        elif kind == b"IEND":
            break
        offset += size + 12
    stride = width * 4
    scanlines = zlib.decompress(compressed)
    if len(scanlines) != height * (stride + 1):
        raise ValueError("PNG scanline size mismatch")
    pixels = bytearray()
    previous = bytearray(stride)
    for row in range(height):
        start = row * (stride + 1)
        filter_kind = scanlines[start]
        current = bytearray(scanlines[start + 1:start + 1 + stride])
        for column in range(stride):
            left = current[column - 4] if column >= 4 else 0
            above = previous[column]
            upper_left = previous[column - 4] if column >= 4 else 0
            if filter_kind == 0:
                predictor = 0
            elif filter_kind == 1:
                predictor = left
            elif filter_kind == 2:
                predictor = above
            elif filter_kind == 3:
                predictor = (left + above) // 2
            elif filter_kind == 4:
                predictor = paeth(left, above, upper_left)
            else:
                raise ValueError(f"Unsupported PNG filter: {filter_kind}")
            current[column] = (current[column] + predictor) & 255
        pixels.extend(current)
        previous = current
    return width, height, bytes(pixels)


def write_png(path: Path, width: int, height: int, rgba: bytes) -> None:
    def chunk(kind: bytes, payload: bytes) -> bytes:
        return (struct.pack(">I", len(payload)) + kind + payload +
                struct.pack(">I", zlib.crc32(kind + payload) & 0xffffffff))
    stride = width * 4
    scanlines = b"".join(b"\0" + rgba[y * stride:(y + 1) * stride]
                         for y in range(height))
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n" +
        chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)) +
        chunk(b"IDAT", zlib.compress(scanlines)) + chunk(b"IEND", b""),
    )


def metrics(name: str, difference: bytes, width: int,
            bounds: tuple[int, int, int, int]) -> dict:
    x0, y0, x1, y1 = bounds
    if x0 < 0 or y0 < 0 or x1 > width or x1 <= x0 or y1 <= y0:
        raise ValueError(f"Invalid comparison bounds: {bounds}")
    row_stride = width * 4
    total = 0
    maximum = 0
    bad = 0
    for y in range(y0, y1):
        for x in range(x0, x1):
            base = y * row_stride + x * 4
            channels = difference[base:base + 4]
            total += sum(channels)
            maximum = max(maximum, *channels)
            bad += max(channels) > 0.05 * 255
    pixels = (x1 - x0) * (y1 - y0)
    mean = total / (pixels * 4 * 255)
    fraction = bad / pixels
    passed = mean <= 0.005 and fraction <= 0.01
    if name == "focus_crop":
        passed = passed and maximum <= 2
    return {
        "name": name, "bounds": bounds,
        "expected": {"mean_absolute_error_max": 0.005,
                     "bad_pixel_fraction_max": 0.01,
                     "focus_max_channel_error": 2 / 255 if name == "focus_crop" else None},
        "actual": {"mean_absolute_error": mean,
                   "bad_pixel_fraction": fraction,
                   "max_channel_error": maximum / 255},
        "status": "passed" if passed else "failed",
    }


def compare(reference: Path, actual: Path, output: Path,
            crop: tuple[int, int, int, int]) -> Path:
    expected_width, expected_height, expected = read_png(reference)
    width, height, observed = read_png(actual)
    if (width, height) != (expected_width, expected_height):
        raise ValueError("Reference and actual dimensions differ")
    if crop[3] > height:
        raise ValueError("Crop exceeds image height")
    difference = bytes(abs(a - b) for a, b in zip(expected, observed, strict=True))
    output.mkdir(parents=True, exist_ok=True)
    difference_path = output / "difference.png"
    write_png(difference_path, width, height, difference)
    checks = [metrics("whole", difference, width, (0, 0, width, height)),
              metrics("focus_crop", difference, width, crop)]
    report = {
        "status": "passed" if all(item["status"] == "passed" for item in checks) else "failed",
        "reference": str(reference), "reference_sha256": digest(reference),
        "actual": str(actual), "actual_sha256": digest(actual),
        "difference": str(difference_path), "difference_sha256": digest(difference_path),
        "checks": checks,
    }
    destination = output / "comparison.json"
    destination.write_text(json.dumps(report, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    return destination


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    parser.add_argument("actual", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--crop", required=True, type=int, nargs=4)
    args = parser.parse_args()
    result = compare(args.reference, args.actual, args.output, tuple(args.crop))
    print(result)
    raise SystemExit(0 if json.loads(result.read_text())["status"] == "passed" else 1)
