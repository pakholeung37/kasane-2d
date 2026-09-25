"""Build tiny, self-authored Observe inspection projects with the public SDK.

Run from the repository root with `uv run --locked python
tests/fixtures/observe_inspection/build_fixtures.py`. The script creates PNGs
with the Python standard library and writes three authoring projects. It
refuses to overwrite existing projects; pass --replace to regenerate them.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import zlib

import kasane

ROOT = Path(__file__).resolve().parent
TEXTURES = ROOT / "textures"
PROJECTS = ROOT / "projects"
UUID = "10000000-0000-4000-8000-"


def ident(number: int) -> str:
    return UUID + f"{number:012x}"


def png_rgba(width: int, height: int, pixels: bytes) -> bytes:
    assert len(pixels) == width * height * 4

    def chunk(kind: bytes, data: bytes) -> bytes:
        return (struct.pack(">I", len(data)) + kind + data +
                struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff))

    scanlines = b"".join(
        b"\0" + pixels[y * width * 4:(y + 1) * width * 4]
        for y in range(height)
    )
    return (b"\x89PNG\r\n\x1a\n" +
            chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)) +
            chunk(b"IDAT", zlib.compress(scanlines, level=9)) +
            chunk(b"IEND", b""))


def make_textures() -> dict[str, Path]:
    TEXTURES.mkdir(parents=True, exist_ok=True)
    rgba = bytearray()
    for y in range(16):
        for x in range(16):
            inside = 4 <= x < 12 and 4 <= y < 12
            rgba.extend((230, 30, 80, 255 if inside else 0))
    padding = TEXTURES / "transparent-padding.png"
    padding.write_bytes(png_rgba(16, 16, rgba))

    rgba = bytearray()
    for y in range(32):
        for x in range(32):
            rgba.extend((20, 110, 250, 255 if x == y else 0))
    line = TEXTURES / "thin-line.png"
    line.write_bytes(png_rgba(32, 32, rgba))

    rgba = bytearray()
    for y in range(32):
        for x in range(32):
            inside = (x - 15.5) ** 2 + (y - 15.5) ** 2 < 10 ** 2
            rgba.extend((255, 255, 255, 255 if inside else 0))
    mask = TEXTURES / "round-mask.png"
    mask.write_bytes(png_rgba(32, 32, rgba))
    return {"padding": padding, "line": line, "mask": mask}


def new_session(number: int) -> kasane.Session:
    return kasane.Session(ident(number), 100, 100, (37, 62), 7.5)


def save(model: kasane.Session, name: str) -> Path:
    PROJECTS.mkdir(parents=True, exist_ok=True)
    destination = PROJECTS / f"{name}.kasane.json"
    receipt = model.save(destination)
    assert receipt.manifest == destination
    assert model.validate_structure() == []
    return destination


def pixel_project(textures: dict[str, Path]) -> Path:
    model = new_session(1)
    with model.edit("pixel and binding controls") as edit:
        edit.add_png_asset(ident(11), "padding", textures["padding"])
        edit.add_png_asset(ident(12), "one pixel line", textures["line"])
        edit.create_rectangle(ident(21), "same name", ident(11), (20, 20), (60, 60))
        edit.create_rectangle(ident(22), "same name", ident(12), (62, 24), (82, 44))
        edit.create_parameter(ident(31), "Shift", 0, 1, 0)
        edit.create_mesh_binding(
            ident(41), ident(21), [kasane.Axis(ident(31), [0, 1])], [
                kasane.MeshKeyform([0], [(20, 20), (60, 20), (60, 60), (20, 60)]),
                kasane.MeshKeyform([1], [(24, 20), (64, 20), (64, 60), (24, 60)]),
            ],
        )
    return save(model, "pixel-binding")


def hierarchy_project(textures: dict[str, Path]) -> Path:
    model = new_session(2)
    # Deformers use runtime units, while create_rectangle uses canvas pixels.
    left, right = (20 - 37) / 7.5, (60 - 37) / 7.5
    bottom, top = (62 - 60) / 7.5, (62 - 20) / 7.5
    quad = [(left, top), (right, top), (left, bottom), (right, bottom)]
    with model.edit("rotation and compressed warp") as edit:
        edit.add_png_asset(ident(13), "line", textures["line"])
        edit.create_rectangle(ident(23), "warped mesh", ident(13), (20, 20), (60, 60))
        edit.create_part(ident(51), "head")
        edit.create_rotation_transform(
            ident(61), "rotate", kasane.RotationData(
                0, kasane.RotationPose((37, 62), angle=25)
            ), ident(51),
        )
        edit.create_warp_transform(
            ident(62), "compressed warp",
            kasane.WarpData(1, 1, True, quad), ident(51), ident(61),
        )
        edit.set_deform_parent(ident(23), ident(62))
        edit.set_mesh_part(ident(23), ident(51))
        edit.update_positions(
            ident(23), [0, 1, 2, 3],
            [(0, 0), (1, 0), (1, 1), (0, 1)],
        )
        edit.update_warp_points(
            ident(62), [(left, top), (right, top),
                        (left + 0.2, top * 0.45), (right - 0.2, top * 0.45)]
        )
    return save(model, "rotated-warp")


def composition_project(textures: dict[str, Path]) -> Path:
    model = new_session(3)
    with model.edit("overlap, mask, and offscreen") as edit:
        edit.add_png_asset(ident(14), "padding", textures["padding"])
        edit.add_png_asset(ident(15), "mask", textures["mask"])
        edit.create_part(ident(52), "offscreen group")
        edit.create_rectangle(ident(24), "rear", ident(14), (20, 20), (80, 80))
        edit.create_rectangle(ident(25), "mask source", ident(15), (25, 25), (75, 75))
        edit.create_rectangle(ident(26), "masked target", ident(14), (30, 30), (70, 70))
        edit.create_rectangle(ident(27), "front cover", ident(14), (40, 40), (65, 65))
        edit.set_mesh_part(ident(26), ident(52))
        edit.create_offscreen(kasane.OffscreenSpec(
            ident(71), "half-opacity layer", ident(52),
            keyforms=[kasane.OffscreenKeyform(0.5)],
        ))
    properties = model.mesh_properties(ident(26))
    with model.edit("attach mask") as edit:
        edit.update_mesh_properties(ident(26), kasane.MeshProperties(
            properties.texture_asset_id, properties.appearance,
            properties.draw_order, properties.blend_mode, properties.enabled,
            properties.double_sided, properties.inverted_mask, [ident(25)],
        ))
    return save(model, "masked-offscreen")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--replace", action="store_true")
    parser.add_argument("--verify", action="store_true")
    options = parser.parse_args()
    if options.verify:
        if options.replace:
            parser.error("--verify and --replace are mutually exclusive")
        manifest = json.loads((ROOT / "manifest.json").read_text(encoding="utf-8"))
        for relative, expected in manifest["files"].items():
            path = (ROOT / relative).resolve()
            if not path.is_relative_to(ROOT) or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
                raise RuntimeError(f"Fixture checksum mismatch: {relative}")
        for relative in manifest["projects"]:
            model = kasane.open_project((ROOT / relative).resolve())
            if model.validate_structure():
                raise RuntimeError(f"Fixture structure invalid: {relative}")
        print(f"Verified {len(manifest['files'])} files and {len(manifest['projects'])} projects")
        return
    if PROJECTS.exists():
        if not options.replace:
            parser.error("projects already exist; pass --replace to regenerate")
        shutil.rmtree(PROJECTS)
    textures = make_textures()
    projects = [pixel_project(textures), hierarchy_project(textures),
                composition_project(textures)]
    (PROJECTS / ".kasane.lock").unlink(missing_ok=True)
    files = sorted([*textures.values(), *PROJECTS.rglob("*")])
    manifest = {
        "schema_version": 1,
        "license": "CC0-1.0; generated locally by build_fixtures.py",
        "projects": [str(path.relative_to(ROOT)) for path in projects],
        "files": {
            str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in files if path.is_file() and path.name != ".kasane.lock"
        },
    }
    (ROOT / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


if __name__ == "__main__":
    main()
