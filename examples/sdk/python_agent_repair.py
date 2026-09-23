"""S5 flow 3, script two: inspect a crop, diagnose size, edit the existing warp."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import struct
import sys
import zlib

import kasane


def digest(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def read_rgba_png(path: Path) -> tuple[int, int, bytes]:
    data = path.read_bytes()
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        raise ValueError(f"Not a PNG: {path}")
    offset = 8
    compressed = bytearray()
    width = height = 0
    while offset < len(data):
        size = struct.unpack_from(">I", data, offset)[0]
        kind = data[offset + 4:offset + 8]
        payload = data[offset + 8:offset + 8 + size]
        if kind == b"IHDR":
            width, height, bit_depth, color, compression, filtering, interlace = struct.unpack(
                ">IIBBBBB", payload,
            )
            if (bit_depth, color, compression, filtering, interlace) != (8, 6, 0, 0, 0):
                raise ValueError("Expected noninterlaced RGBA8 PNG")
        elif kind == b"IDAT":
            compressed.extend(payload)
        elif kind == b"IEND":
            break
        offset += 12 + size
    scanlines = zlib.decompress(compressed)
    stride = width * 4
    if len(scanlines) != height * (stride + 1):
        raise ValueError("PNG row size mismatch")
    rgba = bytearray()
    for row in range(height):
        start = row * (stride + 1)
        if scanlines[start] != 0:
            raise ValueError("Expected unfiltered crop PNG")
        rgba.extend(scanlines[start + 1:start + 1 + stride])
    return width, height, bytes(rgba)


def visible_width(path: Path) -> tuple[int, int]:
    width, height, rgba = read_rgba_png(path)
    occupied = [x for y in range(height) for x in range(width)
                if rgba[(y * width + x) * 4 + 3] > 8]
    if not occupied:
        raise RuntimeError("Target crop contains no visible pixels")
    return max(occupied) - min(occupied) + 1, len(occupied)


def target_crop(report_path: Path, mesh_id: str) -> Path:
    report = json.loads(report_path.read_text(encoding="utf-8"))
    if report["status"] != "frames_complete" or len(report["frames"]) != 1:
        raise RuntimeError("Expected one complete observation frame")
    frame = report["frames"][0]
    full = report_path.parent / frame["path"]
    if digest(full) != frame["sha256"]:
        raise RuntimeError("Full frame hash mismatch")
    crop = next(item for item in frame["crops"] if item["object_id"] == mesh_id)
    image = report_path.parent / crop["path"]
    if digest(image) != crop["sha256"]:
        raise RuntimeError("Focus crop hash mismatch")
    return image


def run(handoff_path: Path, output: Path) -> Path:
    if not handoff_path.is_absolute() or not output.is_absolute():
        raise ValueError("Input and output paths must be absolute")
    output.mkdir(parents=True, exist_ok=True)
    handoff = json.loads(handoff_path.read_text(encoding="utf-8"))
    creation = json.loads(Path(handoff["creation_report"]).read_text(encoding="utf-8"))
    mesh_id = handoff["target_mesh_id"]
    warp_id = handoff["warp_id"]
    if mesh_id not in creation["mesh_ids"] or warp_id not in creation["transform_ids"]:
        raise RuntimeError("Handoff IDs are absent from the created model")
    before_crop = target_crop(Path(handoff["observation_report"]), mesh_id)
    before_width, before_pixels = visible_width(before_crop)
    minimum_visible_width = 30
    if before_width >= minimum_visible_width:
        raise RuntimeError("No undersized target detected; existing model was not edited")
    session = kasane.open_project(Path(handoff["project_manifest"]))
    source_mesh_ids = session.mesh_ids()
    source_asset_hashes = {id: session.asset(id).sha256 for id in session.asset_ids()}
    source_other_mesh = session.mesh_record(next(id for id in source_mesh_ids if id != mesh_id))
    source_warp = session.transform(warp_id)
    if source_warp.kind != "warp" or len(source_warp.warp.points) != 4:
        raise RuntimeError("Expected an editable 1x1 warp")
    new_points = [(0, 0), (3, 0), (0, 3), (3, 3)]
    with session.edit("expand visually undersized target") as edit:
        edit.update_warp_points(warp_id, new_points)
    saved = session.save(output / "repaired-project")
    assert saved.durable
    reopened = kasane.open_project(saved.manifest)
    assert reopened.mesh_ids() == source_mesh_ids
    assert {id: reopened.asset(id).sha256 for id in reopened.asset_ids()} == source_asset_hashes
    assert reopened.mesh_record(source_other_mesh.id)._replace(version=source_other_mesh.version) == source_other_mesh
    assert reopened.transform(warp_id).warp.points == new_points
    assert reopened.validate_structure() == [] and reopened.diagnose_resources() == []
    with kasane.Observer(256, 256, 256) as observer:
        after_observation = observer.observe_run(
            reopened, [{handoff["parameter_id"]: 0.5}],
            output / "repaired-observation", focus=[mesh_id],
        )
    after_crop = target_crop(after_observation.report, mesh_id)
    after_width, after_pixels = visible_width(after_crop)
    if after_width < minimum_visible_width or after_width <= before_width:
        raise RuntimeError("Warp edit did not resolve the measured size problem")
    report = {
        "status": "passed",
        "problem": {
            "object_id": mesh_id, "code": "TARGET_TOO_SMALL",
            "before_visible_width": before_width,
            "minimum_visible_width": minimum_visible_width,
            "before_opaque_pixels": before_pixels,
            "evidence_crop": str(before_crop),
        },
        "correction": {
            "existing_transform_id": warp_id, "new_points": new_points,
            "after_visible_width": after_width,
            "after_opaque_pixels": after_pixels,
            "project_manifest": str(saved.manifest),
            "evidence_crop": str(after_crop),
            "observation_report": str(after_observation.report),
        },
        "preserved_mesh_ids": source_mesh_ids,
        "preserved_asset_hashes": source_asset_hashes,
    }
    destination = output / "repair-report.json"
    destination.write_text(json.dumps(report, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    return destination


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: python_agent_repair.py /absolute/handoff.json /absolute/output")
    print(run(Path(sys.argv[1]), Path(sys.argv[2])))
