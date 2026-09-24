"""Build an editable head-X prototype from Shirousagi's layered PSD.

The delivered model is sampled only as a geometric reference.  The output
project starts from Session.import_psd and contains newly authored mesh grids
and ParamAngleX keyforms; no imported model rig is copied into the PSD project.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
from typing import NamedTuple, Sequence
from uuid import uuid4

import kasane


ANGLES = (-30.0, 0.0, 30.0)
SAMPLE_ANGLES = (-30.0, -15.0, 0.0, 15.0, 30.0)
BODY = "ArtMesh30"


def new_session() -> kasane.Session:
    return kasane.Session(str(uuid4()), 1, 1, (0, 0), 1)


def source_positions(session: kasane.Session, angle: float) -> dict[str, list[tuple[float, float]]]:
    canvas = session.canvas
    names = {mesh_id: session.mesh_record(mesh_id).name for mesh_id in session.mesh_ids()}
    result = {}
    for drawable in session.evaluate({"ParamAngleX": angle}).drawables:
        result[names[drawable.id]] = [canvas.runtime_to_source(point) for point in drawable.positions]
    return result


def grid_size(name: str, width: float, height: float) -> tuple[int, int]:
    if name == "ArtMesh24":
        return 20, 18
    if name in {"ArtMesh26", "ArtMesh27"}:
        return 12, 16
    return max(3, min(10, round(width / 80))), max(3, min(10, round(height / 80)))


def grid_geometry(mesh: kasane.MeshRecordSnapshot) -> kasane.MeshGeometryData:
    source = mesh.geometry
    left, top = source.positions[0]
    right, bottom = source.positions[2]
    nx, ny = grid_size(mesh.name, right - left, bottom - top)
    return kasane.rectangle_grid_geometry(source, nx, ny)


class Projection(NamedTuple):
    positions: list[tuple[float, float]]
    outside_points: int


def project_mesh_displacement(
    reference: kasane.MeshGeometryData,
    neutral: Sequence[tuple[float, float]],
    posed: Sequence[tuple[float, float]],
    target_positions: Sequence[tuple[float, float]],
) -> Projection:
    """Experiment-specific transfer from a delivered mesh to its PSD layer."""
    index = {vertex_id: i for i, vertex_id in enumerate(reference.vertex_ids)}
    triangles = [tuple(index[vertex_id] for vertex_id in triangle)
                 for triangle in reference.triangles]
    result = []
    outside_points = 0
    for x, y in target_positions:
        weights = None
        for a, b, c in triangles:
            ax, ay = neutral[a]
            bx, by = neutral[b]
            cx, cy = neutral[c]
            denominator = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy)
            if abs(denominator) < 1e-8:
                continue
            wa = ((by - cy) * (x - cx) + (cx - bx) * (y - cy)) / denominator
            wb = ((cy - ay) * (x - cx) + (ax - cx) * (y - cy)) / denominator
            wc = 1 - wa - wb
            if min(wa, wb, wc) >= -1e-5:
                weights = ((a, wa), (b, wb), (c, wc))
                break
        if weights is None:
            outside_points += 1
            closest = min(range(len(neutral)), key=lambda i: (neutral[i][0] - x) ** 2 + (neutral[i][1] - y) ** 2)
            weights = ((closest, 1.0),)
        result.append((
            x + sum(weight * (posed[i][0] - neutral[i][0]) for i, weight in weights),
            y + sum(weight * (posed[i][1] - neutral[i][1]) for i, weight in weights),
        ))
    return Projection(result, outside_points)


def deformation_forms(
    original: kasane.MeshRecordSnapshot,
    grid: kasane.MeshGeometryData,
    sampled: dict[float, dict[str, list[tuple[float, float]]]],
) -> tuple[list[kasane.MeshKeyform], int]:
    neutral = sampled[0.0][original.name]
    result = []
    outside_points = 0
    for angle in ANGLES:
        target = sampled[angle][original.name]
        projection = project_mesh_displacement(
            original.geometry, neutral, target, grid.positions,
        )
        outside_points = projection.outside_points
        result.append(kasane.MeshKeyform([angle], projection.positions))
    return result, outside_points


def image_distance(first: bytes, second: bytes, width: int, *, head_only: bool = True) -> dict[str, float | int]:
    # At 768 px the head occupies roughly rows 25..510.  The same fixed
    # region is used for every pose and for the static baseline.
    y0, y1 = (25, 510) if head_only else (540, 768)
    total, changed, count = 0, 0, 0
    for y in range(y0, y1):
        for x in range(width):
            at = 4 * (y * width + x)
            diffs = [abs(first[at + channel] - second[at + channel]) for channel in range(4)]
            total += sum(diffs)
            changed += max(diffs) > 2
            count += 1
    return {"mean_channel_error": round(total / (4 * count), 4), "changed_pixels_gt2": changed}


def image_delta(first: bytes, second: bytes) -> dict[str, int]:
    return {
        "changed_pixels": sum(any(first[i + c] != second[i + c] for c in range(4))
                              for i in range(0, len(first), 4)),
        "max_channel_delta": max(abs(a - b) for a, b in zip(first, second, strict=True)),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=Path("models/local/Shirousagi"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--keep-debug", action="store_true",
                        help="retain the PSD import and all reference/intermediate frames")
    args = parser.parse_args()
    source, output = args.source.resolve(strict=True), args.output.resolve()
    if output.exists():
        parser.error(f"output directory already exists: {output}")
    output.mkdir(parents=True)

    baseline = new_session()
    imported = baseline.import_psd(source / "Shirousagi.psd", output / "psd-import")
    original = new_session()
    original_import = original.import_model3(source / "Shirousagi.model3.json")
    sampled = {angle: source_positions(original, angle) for angle in ANGLES}
    original_meshes = {original.mesh_record(i).name: original.mesh_record(i) for i in original.mesh_ids()}
    target_meshes = [baseline.mesh_record(i) for i in baseline.mesh_ids()]
    source_drawing = {mesh.name: mesh.drawing for mesh in target_meshes}
    projection_outside = {}
    parameter_id = str(uuid4())
    with baseline.edit("Bind PSD head to ParamAngleX") as edit:
        edit.create_parameter(parameter_id, "ParamAngleX", -30, 30, 0, runtime_id="ParamAngleX")
        for mesh in target_meshes:
            if mesh.name == BODY:
                continue
            grid = grid_geometry(mesh)
            forms, projection_outside[mesh.name] = deformation_forms(
                original_meshes[mesh.name], grid, sampled,
            )
            edit.replace_mesh(mesh._replace(geometry=grid))
            edit.create_mesh_binding(
                str(uuid4()), mesh.id, [kasane.Axis(parameter_id, list(ANGLES))],
                forms,
            )

    saved = baseline.save(output / "project")
    published = baseline.export_package(output / "package")
    reopened = kasane.open_project(saved.manifest)
    package = new_session()
    package_import = package.import_model3(output / "package" / "model.model3.json")
    report = {
        "sources_sha256": {
            filename: hashlib.sha256((source / filename).read_bytes()).hexdigest()
            for filename in ("Shirousagi.psd", "Shirousagi.moc3", "Shirousagi.model3.json")
        },
        "psd_import": {"layers": imported.raster_layers, "warnings": imported.warnings},
        "reference_import": {"moc_version": original_import.moc_version, "warnings": original_import.warnings},
        "counts": {"meshes": len(baseline.mesh_ids()), "mesh_bindings": len(baseline.binding_ids()),
                   "parameters": len(baseline.parameter_ids()),
                   "unchanged_drawing_records": sum(
                       baseline.mesh_record(i).drawing == source_drawing[baseline.mesh_record(i).name]
                       for i in baseline.mesh_ids()
                   )},
        "projection_outside_points": projection_outside,
        "diagnostics": {"structure": [str(x) for x in baseline.validate_structure()],
                        "resources": [str(x) for x in baseline.diagnose_resources()],
                        "geometry": [str(x) for x in baseline.diagnose_geometry()]},
        "package": {"published": published.published, "warnings": published.warnings,
                    "reimport_warnings": package_import.warnings,
                    "parameter_runtime_ids": [package.parameter(i).runtime_id for i in package.parameter_ids()]},
        "file_bytes": {"editable_manifest": saved.manifest.stat().st_size,
                       "exported_moc3": (output / "package" / "model.moc3").stat().st_size,
                       "reference_moc3": (source / "Shirousagi.moc3").stat().st_size},
        "poses": {},
    }
    with tempfile.TemporaryDirectory(prefix="shirousagi-head-x-") as temporary:
        relocated = Path(temporary)
        shutil.copytree(saved.manifest.parent, relocated / "project")
        shutil.copytree(output / "package", relocated / "package")
        moved_project = kasane.open_project(relocated / "project")
        moved_package = new_session()
        moved_package.import_model3(relocated / "package" / "model.model3.json")
        with kasane.Observer(768, 768, 768) as observer:
            static_frame = observer.observe(kasane.open_project(imported.manifest), {})
            if args.keep_debug:
                static_frame.save_png(output / "psd-default.png")
            for angle in SAMPLE_ANGLES:
                values = {"ParamAngleX": angle}
                reference = observer.observe(original, values)
                candidate = observer.observe(baseline, values)
                reopened_frame = observer.observe(reopened, values)
                package_frame = observer.observe(package, values)
                moved_project_frame = observer.observe(moved_project, values)
                moved_package_frame = observer.observe(moved_package, values)
                tag = f"{'m' if angle < 0 else 'p'}{abs(int(angle))}"
                if args.keep_debug:
                    reference.save_png(output / f"reference-{tag}.png")
                if args.keep_debug or angle in ANGLES:
                    candidate.save_png(output / f"candidate-{tag}.png")
                report["poses"][str(int(angle))] = {
                    "static_to_reference_head": image_distance(static_frame.rgba, reference.rgba, 768),
                    "candidate_to_reference_head": image_distance(candidate.rgba, reference.rgba, 768),
                    "candidate_to_static_head": image_distance(candidate.rgba, static_frame.rgba, 768),
                    "candidate_to_static_body": image_distance(candidate.rgba, static_frame.rgba, 768, head_only=False),
                    "project_reopen_pixel_equal": candidate.rgba == reopened_frame.rgba,
                    "relocated_project_pixel_equal": candidate.rgba == moved_project_frame.rgba,
                    "package_reimport_delta": image_delta(candidate.rgba, package_frame.rgba),
                    "relocated_package_delta": image_delta(candidate.rgba, moved_package_frame.rgba),
                }
    (output / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    if not args.keep_debug:
        shutil.rmtree(output / "psd-import")
    print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
