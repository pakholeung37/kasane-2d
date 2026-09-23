"""S5 flow 1: create, sample, save, reopen, and export a two-texture model.

Run with an installed wheel from outside the repository:
    python /absolute/path/to/python_two_asset_recipe.py /absolute/output/directory
"""

from __future__ import annotations

import hashlib
import json
import math
from pathlib import Path
import sys

import kasane


DOCUMENT = "10000000-0000-4000-8000-000000000001"
ASSET_A = "10000000-0000-4000-8000-000000000002"
ASSET_B = "10000000-0000-4000-8000-000000000003"
MESH_A = "10000000-0000-4000-8000-000000000004"
MESH_B = "10000000-0000-4000-8000-000000000005"
PART = "10000000-0000-4000-8000-000000000006"
ROTATION = "10000000-0000-4000-8000-000000000007"
WARP = "10000000-0000-4000-8000-000000000008"
PARAMETER = "10000000-0000-4000-8000-000000000009"
BINDING_A = "10000000-0000-4000-8000-00000000000a"
BINDING_B = "10000000-0000-4000-8000-00000000000b"


def digest(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def frame_positions(session: kasane.Session, value: float) -> dict[str, list[list[float]]]:
    evaluation = session.evaluate({PARAMETER: value})
    actual = next(sample.value for sample in evaluation.parameters if sample.id == PARAMETER)
    assert math.isclose(actual, value, abs_tol=1e-6)
    return {
        drawable.id: [list(position) for position in drawable.positions]
        for drawable in evaluation.drawables if drawable.id in (MESH_A, MESH_B)
    }


def run(output: Path) -> Path:
    if not output.is_absolute():
        raise ValueError("Output path must be absolute")
    output.mkdir(parents=True, exist_ok=True)
    source_a = Path(__file__).with_name("asymmetric-2x2.png").resolve()
    source_b = Path(__file__).resolve().parents[2] / "tests/fixtures/external_v50/texture_00.png"
    session = kasane.Session(DOCUMENT, 120, 100, (60, 50), 10)
    with session.edit("two materials and hierarchy") as edit:
        edit.add_png_asset(ASSET_A, "asymmetric 2x2", source_a)
        edit.add_png_asset(ASSET_B, "external 4x4", source_b)
        edit.create_part(PART, "face parts")
        edit.create_rectangle(MESH_A, "left", ASSET_A, (15, 20), (45, 55))
        edit.create_rectangle(MESH_B, "right", ASSET_B, (70, 35), (110, 90))
        edit.set_mesh_part(MESH_A, PART)
        edit.set_mesh_part(MESH_B, PART)
        edit.create_rotation_transform(
            ROTATION, "root rotation",
            kasane.RotationData(0, kasane.RotationPose((60, 50), angle=10)), PART,
        )
        edit.create_warp_transform(
            WARP, "child warp",
            kasane.WarpData(1, 1, True, [(0, 0), (1, 0), (0, 1), (1, 1)]), PART, ROTATION,
        )
        edit.set_deform_parent(MESH_A, ROTATION)
        edit.set_deform_parent(MESH_B, WARP)
        edit.update_positions(
            MESH_A, [0, 1, 2, 3],
            [(-4.5, 3), (-1.5, 3), (-1.5, -0.5), (-4.5, -0.5)],
        )
        edit.update_positions(
            MESH_B, [0, 1, 2, 3],
            [(0.2, 0.2), (0.8, 0.2), (0.8, 0.8), (0.2, 0.8)],
        )
    positions_a = session.mesh(MESH_A).positions
    positions_b = session.mesh(MESH_B).positions
    with session.edit("two animated meshes") as edit:
        edit.create_parameter(PARAMETER, "expression", 0, 1, 0)
        edit.create_mesh_binding(
            BINDING_A, MESH_A, [kasane.Axis(PARAMETER, [0, 1])],
            [kasane.MeshKeyform([0], positions_a),
             kasane.MeshKeyform([1], [(x + 0.6, y + 0.2) for x, y in positions_a])],
        )
        edit.create_mesh_binding(
            BINDING_B, MESH_B, [kasane.Axis(PARAMETER, [0, 1])],
            [kasane.MeshKeyform([0], positions_b),
             kasane.MeshKeyform([1], [(x - 0.1, y + 0.05) for x, y in positions_b])],
        )
    assert session.validate_structure() == []
    assert session.diagnose_resources() == []
    samples = {str(value): frame_positions(session, value) for value in (0.0, 0.5, 1.0)}
    assert set(samples["0.5"]) == {MESH_A, MESH_B}
    errors = []
    for mesh_id in (MESH_A, MESH_B):
        for minimum, middle, maximum in zip(
            samples["0.0"][mesh_id], samples["0.5"][mesh_id],
            samples["1.0"][mesh_id], strict=True,
        ):
            errors.extend(abs(middle[axis] - (minimum[axis] + maximum[axis]) / 2) * 10
                          for axis in (0, 1))
    max_pixel_error = max(errors)
    assert max_pixel_error <= 0.05, max_pixel_error
    saved = session.save(output / "project")
    assert saved.durable
    reopened = kasane.open_project(saved.manifest)
    assert reopened.validate_structure() == []
    assert reopened.diagnose_resources() == []
    for value in (0.0, 0.5, 1.0):
        assert frame_positions(reopened, value) == samples[str(value)]
    exported = reopened.export_package(output / "export")
    assert exported.published and exported.durable
    model_moc3 = output / "export/model.moc3"
    assert model_moc3.is_file()
    assets = [reopened.asset(asset_id) for asset_id in (ASSET_A, ASSET_B)]
    report = {
        "status": "passed", "document_id": DOCUMENT,
        "asset_ids": [ASSET_A, ASSET_B], "mesh_ids": [MESH_A, MESH_B],
        "transform_ids": [ROTATION, WARP], "parameter_id": PARAMETER,
        "assets": [{"id": asset.id, "width": asset.width, "height": asset.height,
                    "sha256": asset.sha256} for asset in assets],
        "sources": {source.name: digest(source) for source in (source_a, source_b)},
        "samples": samples, "maximum_midpoint_pixel_error": max_pixel_error,
        "project_manifest": str(saved.manifest),
        "export_moc3": str(model_moc3), "export_sha256": digest(model_moc3),
        "export_warnings": exported.warnings,
    }
    destination = output / "report.json"
    destination.write_text(json.dumps(report, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    return destination


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: python_two_asset_recipe.py /absolute/output/directory")
    print(run(Path(sys.argv[1])))
