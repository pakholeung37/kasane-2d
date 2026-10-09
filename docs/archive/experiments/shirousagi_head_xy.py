"""Add a model-wide Y warp and semantic Parts to the editable head-X project.

The delivered model is used only to render comparison images. Its parameters,
mesh geometry, bindings, and deformers are never read by this experiment.
"""

from __future__ import annotations

import argparse
from bisect import bisect_right
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
from uuid import NAMESPACE_URL, uuid4, uuid5

import kasane


ANGLE_KEYS = (-30.0, 0.0, 30.0)
SCREEN_ROWS = (0, 60, 140, 230, 310, 390, 470, 550, 640, 730, 766)
# Screen-pixel offsets at SCREEN_ROWS. Image comparisons establish the pose;
# the upward endpoint also keeps the head's local vertical scale above 0.8 so
# its forehead is not visibly flattened. No delivered tracks were read.
Y_OFFSETS = {
    -30.0: (26.68, 6.85, 13.17, 16.63, 16.29, 38.49, 21.47, 23.11, 33.34, 3.12, -11.42),
    0.0: (0.0,) * len(SCREEN_ROWS),
    30.0: (0.0, -1.0, -5.0, -9.0, -18.0, -33.0, -22.0, -12.0, 0.0, 0.0, 0.0),
}
MOUTH_Y_OFFSETS = {-30.0: 14.0, 0.0: 0.0, 30.0: -10.0}
MOUTH_BOUNDS = (1800.0, 2050.0, 2450.0, 2450.0)
EAR_SCREEN_ROWS = SCREEN_ROWS[:5]
# Ear tips lag the upward head movement while the roots remain close to the
# face outline. These are extra offsets on top of the shared model warp.
EAR_Y_OFFSETS = {
    -30.0: (0.0,) * len(EAR_SCREEN_ROWS),
    0.0: (0.0,) * len(EAR_SCREEN_ROWS),
    30.0: (5.0, 5.0, 4.0, 3.0, 2.0),
}

# Draw order and runtime ArtMesh IDs remain unchanged. These names describe
# what is visible in the PSD, including its disabled expression alternatives.
LAYERS = {
    "ArtMesh30": ("body", "身体轮廓"),
    "ArtMesh27": ("ear_left", "画面左耳"),
    "ArtMesh26": ("ear_right", "画面右耳"),
    "ArtMesh24": ("face", "脸部轮廓"),
    "ArtMesh23": ("cheeks", "画面右腮红"),
    "ArtMesh22": ("cheeks", "画面左腮红"),
    "ArtMesh18": ("mouth", "嘴部粉色内层"),
    "ArtMesh17": ("mouth", "嘴部上缘线"),
    "ArtMesh16": ("mouth", "微笑嘴形"),
    "ArtMesh15": ("eye_right", "画面右眼瞳"),
    "ArtMesh14": ("eye_left", "画面左眼瞳"),
    "ArtMesh13": ("face_detail", "鼻尖"),
    "ArtMesh12": ("eye_right", "画面右眼替换短线"),
    "ArtMesh11": ("eye_left", "画面左眼替换短线"),
    "ArtMesh10": ("brows", "画面右眉替换线"),
    "ArtMesh9": ("brows", "画面左眉替换线"),
    "ArtMesh8": ("eye_right", "画面右眼替换高光"),
    "ArtMesh7": ("eye_left", "画面左眼替换高光"),
    "ArtMesh6": ("eye_right", "画面右眼替换闭眼线"),
    "ArtMesh5": ("eye_left", "画面左眼替换闭眼线"),
    "ArtMesh4": ("expressions", "画面右侧表情替换"),
    "ArtMesh3": ("expressions", "画面左侧表情替换"),
    "ArtMesh2": ("forehead", "额头卷纹替换"),
    "ArtMesh": ("forehead", "额头饰线替换"),
}

PARTS = (
    ("root", "", "白兔｜整体", 0),
    ("body", "root", "身体", 0),
    ("head", "root", "头部", 1),
    ("ears", "head", "双耳", 1),
    ("ear_left", "ears", "画面左耳", 1),
    ("ear_right", "ears", "画面右耳", 2),
    ("face", "head", "脸部轮廓", 3),
    ("face_detail", "face", "鼻尖", 11),
    ("cheeks", "face", "腮红", 4),
    ("eyes", "face", "双眼", 9),
    ("eye_left", "eyes", "画面左眼及替换层", 10),
    ("eye_right", "eyes", "画面右眼及替换层", 9),
    ("brows", "face", "眉毛替换层", 14),
    ("mouth", "face", "嘴部", 6),
    ("expressions", "face", "两侧表情替换层", 20),
    ("forehead", "head", "额头装饰替换层", 22),
)


def stable_id(name: str) -> str:
    return str(uuid5(NAMESPACE_URL, f"kasane/shirousagi/head-xy/{name}"))


def new_session() -> kasane.Session:
    return kasane.Session(str(uuid4()), 1, 1, (0, 0), 1)


def vertical_scales(angle: float) -> tuple[float, ...]:
    offsets = Y_OFFSETS[angle]
    return tuple(
        1.0 + (offsets[i + 1] - offsets[i]) / (SCREEN_ROWS[i + 1] - SCREEN_ROWS[i])
        for i in range(len(SCREEN_ROWS) - 1)
    )


def image_distance(first: bytes, second: bytes, width: int, height: int) -> dict[str, float | int]:
    if len(first) != len(second) or len(first) != width * height * 4:
        raise ValueError("RGBA frame size mismatch")
    total = sum(abs(a - b) for a, b in zip(first, second, strict=True))
    changed = sum(
        any(abs(first[i + c] - second[i + c]) > 2 for c in range(4))
        for i in range(0, len(first), 4)
    )
    return {"mean_channel_error": round(total / len(first), 4), "changed_pixels_gt2": changed}


def image_delta(first: bytes, second: bytes) -> dict[str, int]:
    return {
        "changed_pixels": sum(
            any(first[i + c] != second[i + c] for c in range(4))
            for i in range(0, len(first), 4)
        ),
        "max_channel_delta": max(abs(a - b) for a, b in zip(first, second, strict=True)),
    }


def pose_values(session: kasane.Session, x: int, y: int, *, include_y: bool = True) -> dict[str, int]:
    ids = {session.parameter(pid).runtime_id: pid for pid in session.parameter_ids()}
    values = {ids["ParamAngleX"]: x}
    if include_y:
        values[ids["ParamAngleY"]] = y
    return values


def author(output: Path, input_project: Path, source: Path) -> None:
    if min(vertical_scales(30.0)) < 0.8:
        raise ValueError("Upward warp compresses a row interval below 80%")
    project = kasane.open_project(input_project)
    meshes = [project.mesh_record(mesh_id) for mesh_id in project.mesh_ids()]
    if {mesh.runtime_id for mesh in meshes} != set(LAYERS):
        raise ValueError("Input project does not have the expected 24 PSD layer identities")
    if project.part_ids() or project.transform_ids() or len(project.parameter_ids()) != 1:
        raise ValueError("Expected the ungrouped head-X project as input")
    if [project.parameter(pid).runtime_id for pid in project.parameter_ids()] != ["ParamAngleX"]:
        raise ValueError("Input project must contain the authored ParamAngleX parameter")

    canvas = project.canvas
    bindings = {mesh.id: project.binding_for_mesh(mesh.id) for mesh in meshes}
    with kasane.Observer(768, 768, 768) as observer:
        view = observer.observe(project, {})
    source_rows = tuple((row - view.view_offset[1]) / view.view_scale for row in SCREEN_ROWS)
    rows = len(source_rows) - 1
    ear_source_rows = source_rows[:len(EAR_SCREEN_ROWS)]
    ear_rows = len(ear_source_rows) - 1

    def parent_local(point: tuple[float, float]) -> tuple[float, float]:
        x, y = point
        index = max(0, min(rows - 1, bisect_right(source_rows, y) - 1))
        y_fraction = (y - source_rows[index]) / (source_rows[index + 1] - source_rows[index])
        return x / canvas.width, (index + y_fraction) / rows

    def warp_points(angle: float) -> list[tuple[float, float]]:
        return [
            (x, y + offset / view.view_scale)
            for y, offset in zip(source_rows, Y_OFFSETS[angle], strict=True)
            for x in (0.0, canvas.width)
        ]

    def mouth_local(point: tuple[float, float]) -> tuple[float, float]:
        left, top, right, bottom = MOUTH_BOUNDS
        return (point[0] - left) / (right - left), (point[1] - top) / (bottom - top)

    def mouth_points(angle: float) -> list[tuple[float, float]]:
        left, top, right, bottom = MOUTH_BOUNDS
        delta = MOUTH_Y_OFFSETS[angle] / view.view_scale
        return [parent_local((x, y + delta)) for y in (top, bottom) for x in (left, right)]

    def ear_local(point: tuple[float, float]) -> tuple[float, float]:
        x, y = point
        index = max(0, min(ear_rows - 1, bisect_right(ear_source_rows, y) - 1))
        fraction = (y - ear_source_rows[index]) / (ear_source_rows[index + 1] - ear_source_rows[index])
        return x / canvas.width, (index + fraction) / ear_rows

    def ear_points(angle: float) -> list[tuple[float, float]]:
        return [
            parent_local((x, y + offset / view.view_scale))
            for y, offset in zip(ear_source_rows, EAR_Y_OFFSETS[angle], strict=True)
            for x in (0.0, canvas.width)
        ]

    part_ids = {key: stable_id(f"part/{key}") for key, _, _, _ in PARTS}
    warp_id = stable_id("transform/model-y")
    mouth_warp_id = stable_id("transform/mouth-y")
    ear_warp_id = stable_id("transform/ears-y")
    y_id = stable_id("parameter/angle-y")
    with project.edit("Semantic Parts and model-wide Y warp") as edit:
        for key, parent, title, order in PARTS:
            edit.create_part(part_ids[key], title, part_ids[parent] if parent else "", draw_order=order)
        edit.create_parameter(y_id, "角度 Y｜整模", -30, 30, 0, runtime_id="ParamAngleY")
        edit.create_warp_transform(
            warp_id, "整模 Y 轴共享变形", kasane.WarpData(rows, 1, True, warp_points(0.0)),
            part_id=part_ids["root"],
        )
        edit.create_scene_binding(
            stable_id("binding/model-y"), "warp", warp_id,
            [kasane.Axis(y_id, list(ANGLE_KEYS))],
            [kasane.SceneWarpKeyform([angle], warp_points(angle)) for angle in ANGLE_KEYS],
        )
        edit.create_warp_transform(
            mouth_warp_id, "嘴部 Y 轴协同修形", kasane.WarpData(1, 1, True, mouth_points(0.0)),
            part_id=part_ids["mouth"], parent_id=warp_id,
        )
        edit.create_scene_binding(
            stable_id("binding/mouth-y"), "warp", mouth_warp_id,
            [kasane.Axis(y_id, list(ANGLE_KEYS))],
            [kasane.SceneWarpKeyform([angle], mouth_points(angle)) for angle in ANGLE_KEYS],
        )
        edit.create_warp_transform(
            ear_warp_id, "双耳 Y 轴相对位移", kasane.WarpData(ear_rows, 1, True, ear_points(0.0)),
            part_id=part_ids["ears"], parent_id=warp_id,
        )
        edit.create_scene_binding(
            stable_id("binding/ears-y"), "warp", ear_warp_id,
            [kasane.Axis(y_id, list(ANGLE_KEYS))],
            [kasane.SceneWarpKeyform([angle], ear_points(angle)) for angle in ANGLE_KEYS],
        )
        for mesh in meshes:
            part, label = LAYERS[mesh.runtime_id]
            is_ear = part in ("ear_left", "ear_right")
            to_local = ear_local if is_ear else mouth_local if part == "mouth" else parent_local
            edit.replace_mesh(mesh._replace(
                name=label,
                part_id=part_ids[part],
                deformer_id=ear_warp_id if is_ear else mouth_warp_id if part == "mouth" else warp_id,
                geometry=mesh.geometry._replace(
                    positions=[to_local(point) for point in mesh.geometry.positions],
                ),
            ))
            binding = bindings[mesh.id]
            if binding is not None:
                edit.replace_mesh_binding(
                    binding.id, mesh.id, binding.axes,
                    [form._replace(positions=[to_local(point) for point in form.positions])
                     for form in binding.keyforms],
                )

    saved = project.save(output / "project")
    published = project.export_package(output / "package")
    reopened = kasane.open_project(saved.manifest)
    package = new_session()
    package_import = package.import_model3(output / "package" / "model.model3.json")
    reference = new_session()
    reference.import_model3(source / "Shirousagi.model3.json")
    before = kasane.open_project(input_project)

    checks = [(-30, 0), (-15, 0), (0, 0), (15, 0), (30, 0),
              (0, -30), (0, -15), (0, 15), (0, 30),
              (-30, -30), (-30, 30), (-15, 15), (15, -15), (30, 30)]
    report = {
        "input_sha256": hashlib.sha256(
            (input_project / "project.kasane.json" if input_project.is_dir() else input_project).read_bytes()
        ).hexdigest(),
        "method": "image-only Y visual fitting with upward head-scale constraint; semantic Parts",
        "screen_rows": SCREEN_ROWS,
        "y_offsets_screen_px": Y_OFFSETS,
        "upward_interval_vertical_scales": vertical_scales(30.0),
        "mouth_y_offsets_screen_px": MOUTH_Y_OFFSETS,
        "ears_y_offsets_screen_px": EAR_Y_OFFSETS,
        "parts": {key: {"name": title, "parent": parent, "draw_order": order}
                  for key, parent, title, order in PARTS},
        "layers": {runtime_id: {"part": part, "name": label}
                   for runtime_id, (part, label) in LAYERS.items()},
        "counts": {"meshes": len(project.mesh_ids()), "mesh_bindings": len(project.binding_ids()),
                   "parts": len(project.part_ids()), "transforms": len(project.transform_ids()),
                   "scene_bindings": len(project.scene_binding_ids()),
                   "parameters": len(project.parameter_ids())},
        "diagnostics": {
            "structure": [str(item) for item in project.validate_structure()],
            "resources": [str(item) for item in project.diagnose_resources()],
            "geometry": [str(item) for item in project.diagnose_geometry()],
        },
        "package": {"published": published.published, "warnings": published.warnings,
                    "reimport_warnings": package_import.warnings,
                    "parameter_runtime_ids": [package.parameter(pid).runtime_id
                                              for pid in package.parameter_ids()]},
        "poses": {},
    }
    with tempfile.TemporaryDirectory(prefix="shirousagi-head-xy-") as temporary:
        relocated = Path(temporary)
        shutil.copytree(saved.manifest.parent, relocated / "project")
        shutil.copytree(output / "package", relocated / "package")
        moved_project = kasane.open_project(relocated / "project")
        moved_package = new_session()
        moved_package.import_model3(relocated / "package" / "model.model3.json")
        with kasane.Observer(768, 768, 768) as observer:
            for x, y in checks:
                target = observer.observe(reference, {"ParamAngleX": x, "ParamAngleY": y})
                old = observer.observe(before, pose_values(before, x, y, include_y=False))
                candidate = observer.observe(project, pose_values(project, x, y))
                open_frame = observer.observe(reopened, pose_values(reopened, x, y))
                package_frame = observer.observe(package, pose_values(package, x, y))
                moved_frame = observer.observe(moved_project, pose_values(moved_project, x, y))
                moved_package_frame = observer.observe(moved_package, pose_values(moved_package, x, y))
                label = f"x{'m' if x < 0 else 'p'}{abs(x)}-y{'m' if y < 0 else 'p'}{abs(y)}"
                if (x, y) in [(0, -30), (0, 0), (0, 30), (-30, -30), (30, 30)]:
                    candidate.save_png(output / f"candidate-{label}.png")
                    target.save_png(output / f"reference-{label}.png")
                report["poses"][f"{x},{y}"] = {
                    "before_to_reference": image_distance(old.rgba, target.rgba, 768, 768),
                    "candidate_to_reference": image_distance(candidate.rgba, target.rgba, 768, 768),
                    "candidate_to_before": image_distance(candidate.rgba, old.rgba, 768, 768),
                    "project_reopen_pixel_equal": candidate.rgba == open_frame.rgba,
                    "relocated_project_pixel_equal": candidate.rgba == moved_frame.rgba,
                    "package_reimport_delta": image_delta(candidate.rgba, package_frame.rgba),
                    "relocated_package_delta": image_delta(candidate.rgba, moved_package_frame.rgba),
                }
    (output / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"output": str(output), "counts": report["counts"],
                      "diagnostics": report["diagnostics"],
                      "y_minus_30": report["poses"]["0,-30"],
                      "y_plus_30": report["poses"]["0,30"]}, ensure_ascii=False, indent=2))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input-project", type=Path,
                        default=Path("output/head-x-experiment/result/project"))
    parser.add_argument("--source", type=Path, default=Path("models/local/Shirousagi"))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    input_project = args.input_project.resolve(strict=True)
    source = args.source.resolve(strict=True)
    output = args.output.resolve()
    if output.exists():
        parser.error(f"output directory already exists: {output}")
    output.mkdir(parents=True)
    author(output, input_project, source)


if __name__ == "__main__":
    main()
