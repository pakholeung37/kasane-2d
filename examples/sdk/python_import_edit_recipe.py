"""S5 flow 2: import an external model3, edit existing IDs, and preserve the rest.

Run with an installed wheel from outside the repository:
    python /absolute/path/to/python_import_edit_recipe.py /absolute/output/directory
"""

from __future__ import annotations

import hashlib
import json
import math
from pathlib import Path
import sys

import kasane


DOCUMENT = "20000000-0000-4000-8000-000000000001"


def digest(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def positions(session: kasane.Session, values: dict[str, float], mesh_id: str) -> list[list[float]]:
    frame = session.evaluate(values)
    return [list(point) for drawable in frame.drawables if drawable.id == mesh_id
            for point in drawable.positions]


def equivalent_points(left: list[list[float]], right: list[list[float]]) -> bool:
    return len(left) == len(right) and all(
        math.isclose(a, b, rel_tol=1e-5, abs_tol=1e-5)
        for point_a, point_b in zip(left, right, strict=True)
        for a, b in zip(point_a, point_b, strict=True)
    )


def run(output: Path) -> Path:
    if not output.is_absolute():
        raise ValueError("Output path must be absolute")
    output.mkdir(parents=True, exist_ok=True)
    fixture = Path(__file__).resolve().parents[2] / "tests/fixtures/external_v50"
    model3 = fixture / "model.model3.json"
    moc3 = fixture / "model.moc3"
    texture = fixture / "texture_00.png"
    session = kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)
    imported = session.import_model3(model3)
    assert imported.moc_version == 5 and not imported.diagnostics
    assert len(session.mesh_ids()) == 1
    assert len(session.transform_ids()) == 2
    assert len(session.parameter_ids()) == 2
    mesh_id = session.mesh_ids()[0]
    binding_id = session.binding_ids()[0]
    mesh = session.mesh_record(mesh_id)
    binding = session.binding(binding_id)
    appearance = session.mesh_properties(mesh_id)
    assets = {asset_id: session.asset(asset_id) for asset_id in session.asset_ids()}
    parameters = {parameter_id: session.parameter(parameter_id)
                  for parameter_id in session.parameter_ids()}
    transforms = {transform_id: session.transform(transform_id)
                  for transform_id in session.transform_ids()}
    parts = {part_id: session.part(part_id) for part_id in session.part_ids()}
    warp_id = next(id for id, transform in transforms.items() if transform.kind == "warp")
    assert mesh.deformer_id != warp_id
    values = [{}, {parameter.id: 0.5 for parameter in parameters.values()}]
    before_samples = [positions(session, sample, mesh_id) for sample in values]
    assert session.validate_structure() == [] and session.diagnose_resources() == []
    before_save = session.save(output / "before")
    assert before_save.durable
    before_export = session.export_package(output / "before-export")
    assert before_export.published and before_export.durable

    offset = 0.05
    with session.edit("local modification of imported mesh") as edit:
        edit.update_positions(
            mesh_id, mesh.geometry.vertex_ids,
            [(x + offset, y) for x, y in mesh.geometry.positions],
        )
        edit.set_deform_parent(mesh_id, warp_id)
        edit.replace_mesh_binding(
            binding_id, mesh_id, binding.axes,
            [kasane.MeshKeyform(
                keyform.keys,
                [(x + offset, y) for x, y in keyform.positions],
                keyform.appearance, keyform.draw_order,
            ) for keyform in binding.keyforms],
        )
        edit.update_mesh_properties(
            mesh_id, kasane.MeshProperties(
                appearance.texture_asset_id,
                kasane.Appearance(0.9, appearance.appearance.multiply,
                                  appearance.appearance.screen),
                appearance.draw_order + 1, appearance.blend_mode,
                appearance.enabled, appearance.double_sided,
                appearance.inverted_mask, appearance.masks,
            ),
        )
    assert session.mesh_ids() == [mesh_id]
    assert session.binding_ids() == [binding_id]
    assert session.mesh_record(mesh_id).deformer_id == warp_id
    assert math.isclose(session.mesh_properties(mesh_id).appearance.opacity, 0.9,
                        abs_tol=1e-5)
    assert session.validate_structure() == [] and session.diagnose_resources() == []
    after_samples = [positions(session, sample, mesh_id) for sample in values]
    assert all(not equivalent_points(before, after)
               for before, after in zip(before_samples, after_samples, strict=True))
    after_save = session.save(output / "after")
    assert after_save.durable
    reopened = kasane.open_project(after_save.manifest)
    assert reopened.mesh_ids() == [mesh_id]
    assert reopened.binding_ids() == [binding_id]
    assert reopened.validate_structure() == [] and reopened.diagnose_resources() == []
    assert all(equivalent_points(after, positions(reopened, sample, mesh_id))
               for after, sample in zip(after_samples, values, strict=True))
    preserved = {
        "asset_ids": reopened.asset_ids() == list(assets),
        "parameter_ids": reopened.parameter_ids() == list(parameters),
        "transform_ids": reopened.transform_ids() == list(transforms),
        "part_ids": reopened.part_ids() == list(parts),
        "assets": all(
            (reopened.asset(id).name, reopened.asset(id).width,
             reopened.asset(id).height, reopened.asset(id).sha256)
            == (asset.name, asset.width, asset.height, asset.sha256)
            for id, asset in assets.items()
        ),
        "parameters": all(
            reopened.parameter(id)._replace(version=parameter.version) == parameter
            for id, parameter in parameters.items()
        ),
        "transforms": all(
            reopened.transform(id)._replace(version=transform.version) == transform
            for id, transform in transforms.items()
        ),
        "parts": all(
            reopened.part(id)._replace(version=part.version) == part
            for id, part in parts.items()
        ),
    }
    assert all(preserved.values()), preserved
    after_export = reopened.export_package(output / "after-export")
    assert after_export.published and after_export.durable
    before_moc3 = output / "before-export/model.moc3"
    after_moc3 = output / "after-export/model.moc3"
    assert before_moc3.is_file() and after_moc3.is_file()
    assert digest(before_moc3) != digest(after_moc3)
    report = {
        "status": "passed", "fixture": {
            "model3_sha256": digest(model3), "moc3_sha256": digest(moc3),
            "texture_sha256": digest(texture),
        },
        "original_ids": {
            "mesh": mesh_id, "binding": binding_id,
            "assets": list(assets), "parameters": list(parameters),
            "transforms": list(transforms), "parts": list(parts),
        },
        "changed": {
            "geometry_offset": offset, "deformer_id": warp_id,
            "binding_id": binding_id, "opacity": 0.9,
            "draw_order": appearance.draw_order + 1,
        },
        "preserved": preserved,
        "before_samples": before_samples, "after_samples": after_samples,
        "before_manifest": str(before_save.manifest),
        "after_manifest": str(after_save.manifest),
        "before_export_sha256": digest(before_moc3),
        "after_export_sha256": digest(after_moc3),
        "before_export": str(before_moc3), "after_export": str(after_moc3),
    }
    destination = output / "report.json"
    destination.write_text(json.dumps(report, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    return destination


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: python_import_edit_recipe.py /absolute/output/directory")
    print(run(Path(sys.argv[1])))
