#!/usr/bin/env python3
"""Derive a two-Part, two-drawable MOC3 from this repo's synthetic v5 fixture.

The input was constructed by create_v50_external_fixture.py, not supplied by
Live2D. This script only changes its structural arrays; no vendor asset is
copied into the animation probe.
"""
from __future__ import annotations

import hashlib
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "tests/fixtures/external_v50/model.moc3"
DEST = ROOT / "tests/fixtures/animation_cpu/model.moc3"
DEST_REAL_CONTROL = ROOT / "tests/fixtures/animation_cpu/model_real_part_control.moc3"
SOURCE_SHA256 = "ceb05a329481f6b510fd2458c8e3cb0923febe6c8a3b0d1695e0c110eec3c084"


def pack(fmt: str, *items: object) -> bytearray:
    return bytearray(struct.pack("<" + fmt, *items))


def generate() -> bytes:
    source = SOURCE.read_bytes()
    if hashlib.sha256(source).hexdigest() != SOURCE_SHA256:
        raise ValueError("synthetic source MOC3 changed; inspect layout before regenerating")
    offsets = [struct.unpack_from("<I", source, 64 + i * 4)[0] for i in range(152)]
    sections = [bytearray(source[offsets[i]:offsets[i + 1] if i < 151 else len(source)]) for i in range(152)]

    counts = list(struct.unpack_from("<64i", sections[0]))
    for index, value in {0: 2, 4: 2, 6: 2, 9: 12, 10: 128, 15: 16,
                         16: 12, 18: 3, 19: 4, 23: 12, 24: 12}.items():
        counts[index] = value
    sections[0] = pack("64i", *counts)

    # Two actual Parts; neither has a same-named MOC parameter.
    sections[2] = bytearray(16)
    sections[3] = bytearray(b"Part0".ljust(64, b"\0") + b"Part1".ljust(64, b"\0"))
    for index, values in {
        4: (0, 0), 5: (0, 1), 6: (1, 1), 7: (1, 1),
        8: (1, 1), 9: (-1, -1),
    }.items():
        sections[index] = pack("2i", *values)
    sections[58] = pack("2f", 0.0, 0.0)
    sections[54] = pack("2i", 0, 1)  # ParamY repeats across [-1, 1].

    # Two distinct drawables. The first retains its warp/rotation ancestry;
    # the second belongs directly to Part1 and has its own vertex/keyform span.
    for index in (29, 30, 31, 32):
        sections[index] = bytearray(16)
    sections[33] = bytearray(b"MeshQuad0".ljust(64, b"\0") + b"MeshQuad1".ljust(64, b"\0"))
    mesh_arrays = {
        34: (1, 1), 35: (0, 6), 36: (6, 6), 37: (1, 1),
        38: (1, 1), 39: (0, 1), 40: (1, -1), 41: (0, 0),
        43: (4, 4), 44: (0, 8), 45: (0, 6), 46: (6, 6),
        47: (0, 0), 48: (0, 0), 107: (0, 6),
    }
    for index, values in mesh_arrays.items():
        sections[index] = pack("2i", *values)
    sections[42] = bytearray((0, 0))
    sections[68] = pack("12f", *[1.0 - (k % 6) * 0.05 for k in range(12)])
    sections[69] = pack("12f", *[10.0 + k % 6 for k in range(12)])
    sections[70] = pack("12i", *[32 + k * 8 for k in range(12)])
    # Original section 71 contains 32 floats for the warp and 48 for mesh 0.
    original_positions = sections[71][:80 * 4]
    sections[71] = original_positions + original_positions[32 * 4:]
    sections[78] = sections[78][:8 * 4] * 2
    sections[79] = sections[79][:6 * 2] * 2
    sections[141] = pack("12i", *range(12))
    sections[142] = pack("12i", *range(12))
    for index in range(108, 114):
        sections[index] = sections[index][:6 * 4] * 2

    # Root group owns both Parts; each Part owns exactly one drawable.
    for index, values in {
        81: (0, 2, 3), 82: (2, 1, 1), 83: (2, 1, 1),
        84: (20, 20, 20), 85: (0, 0, 0),
    }.items():
        sections[index] = pack("3i", *values)
    sections[86] = pack("4i", 1, 1, 0, 0)
    sections[87] = pack("4i", 0, 1, 0, 1)
    sections[88] = pack("4i", 1, 2, -1, -1)

    output = bytearray(source[:1984])
    for i, section in enumerate(sections):
        output.extend(b"\0" * (-len(output) % 64))
        struct.pack_into("<I", output, 64 + i * 4, len(output))
        output.extend(section)
    output.extend(b"\0" * (-len(output) % 64))
    return bytes(output)


if __name__ == "__main__":
    data = generate()
    DEST.parent.mkdir(parents=True, exist_ok=True)
    DEST.write_bytes(data)
    print(f"{DEST}: {len(data)} bytes, sha256={hashlib.sha256(data).hexdigest()}")
    # Keep ParamX as a real motion/physics parameter while making Part0's
    # control ID a real MOC parameter. Part1 remains a virtual control slot.
    real_control = bytearray(data)
    parameter_ids_offset = struct.unpack_from("<I", real_control, 64 + 50 * 4)[0]
    real_control[parameter_ids_offset + 64:parameter_ids_offset + 128] = b"Part0".ljust(64, b"\0")
    DEST_REAL_CONTROL.write_bytes(real_control)
    print(f"{DEST_REAL_CONTROL}: {len(real_control)} bytes, sha256={hashlib.sha256(real_control).hexdigest()}")
