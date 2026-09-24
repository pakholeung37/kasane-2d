"""Host-only SDK operations. Executed with the experiment's installed Python.

The grader reads actual saved projects, never a participant's claimed metrics.
SDK conformance is still the responsibility of the separate SDK test suite.
"""
from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
from uuid import uuid4

import kasane


GROUPS = {
    "asset": "asset", "mesh": "mesh_record", "parameter": "parameter",
    "binding": "binding", "part": "part", "transform": "transform",
    "scene_binding": "scene_binding", "offscreen": "offscreen", "glue": "glue",
    "blend_key_table": "blend_key_table", "blend_constraint": "blend_constraint",
    "blend_binding": "blend_binding",
}


def plain(value):
    if hasattr(value, "_asdict"):
        return {k: plain(v) for k, v in value._asdict().items() if k != "version"}
    if isinstance(value, (tuple, list)):
        return [plain(v) for v in value]
    if isinstance(value, dict):
        return {k: plain(v) for k, v in value.items()}
    if isinstance(value, Path):
        return str(value)
    return value


def state(session):
    result = {"document_id": session.document_id, "canvas": plain(session.canvas),
              "draw_order_groups": plain(session.draw_order_groups)}
    for group, reader in GROUPS.items():
        result[group] = {}
        for key in getattr(session, group + "_ids")():
            record = plain(getattr(session, reader)(key))
            if group == "asset":
                record.pop("source", None)  # Saving legitimately relocates resources.
            result[group][key] = record
    result["samples"] = [plain(session.evaluate({"Open": v}))
                         for v in (0, 0.5, 1)] if session.parameter_ids() else []
    return result


def close(left, right):
    if isinstance(left, bool) or isinstance(right, bool):
        return type(left) is type(right) and left == right
    if isinstance(left, (int, float)) and isinstance(right, (int, float)):
        return math.isfinite(left) and math.isfinite(right) and math.isclose(
            left, right, rel_tol=0, abs_tol=1e-6)
    if isinstance(left, dict) and isinstance(right, dict):
        return left.keys() == right.keys() and all(close(left[k], right[k]) for k in left)
    if isinstance(left, list) and isinstance(right, list):
        return len(left) == len(right) and all(close(a, b) for a, b in zip(left, right))
    return type(left) is type(right) and left == right


def make_project(texture, task):
    session = kasane.Session(str(uuid4()), 100, 100, (50, 50), 10)
    asset, parameter = str(uuid4()), str(uuid4())
    with session.edit("fixture") as edit:
        edit.add_png_asset(asset, "texture", texture)
        for name, lo, hi in ([("left", (10, 40), (30, 60)),
                              ("right", (40, 40), (60, 60))] if task == "edit"
                             else [("face", (40, 40), (60, 60))]):
            edit.create_rectangle(str(uuid4()), name, asset, lo, hi)
        if task != "create":
            edit.create_parameter(parameter, "Open", 0, 1, 0)
    if task != "create":
        with session.edit("bindings") as edit:
            for mesh in session.mesh_ids():
                base = session.mesh(mesh).positions
                edit.create_mesh_binding(str(uuid4()), mesh, [kasane.Axis(parameter, [0, 1])], [
                    kasane.MeshKeyform([0], base),
                    kasane.MeshKeyform([1], [(x + 10, y) for x, y in base]),
                ])
    return session


def repair(session):
    mesh = session.require_unique_mesh("right")
    binding = session.binding_for_mesh(mesh.id)
    form = next(f for f in binding.keyforms if f.keys == [1])
    with session.edit("target change") as edit:
        edit.set_mesh_keyform(binding.id, form._replace(
            positions=[(x + 10, y) for x, y in form.positions]))


def creation_checks(session, task, texture_hash):
    checks = {}
    checks["canvas"] = close(plain(session.canvas), dict(
        width=100.0, height=100.0, origin_x=50.0, origin_y=50.0, pixels_per_unit=10.0))
    checks["object_counts"] = all(len(getattr(session, group + "_ids")()) == (
        1 if group in ("asset", "mesh") or (task == "parameter" and group in ("parameter", "binding"))
        else 0) for group in GROUPS)
    checks["draw_order_groups"] = session.draw_order_groups is None
    checks["texture"] = len(session.asset_ids()) == 1 and (
        session.asset(session.asset_ids()[0]).sha256 == texture_hash)
    checks["mesh"] = False
    if len(session.mesh_ids()) != 1:
        return checks
    mesh = session.mesh_record(session.mesh_ids()[0])
    geometry = mesh.geometry
    corners = [(40, 40), (60, 40), (60, 60), (40, 60)]
    checks["mesh"] = mesh.name == "face" and not mesh.part_id and not mesh.deformer_id
    checks["positions"] = close(sorted(plain(geometry.positions)), sorted(plain(corners)))
    expected_uv = {(40, 40): (0, 0), (60, 40): (1, 0), (60, 60): (1, 1), (40, 60): (0, 1)}
    checks["uvs"] = len(geometry.uvs) == len(geometry.positions) == 4 and all(
        close(plain(uv), plain(expected_uv.get(tuple(p)))) for p, uv in zip(geometry.positions, geometry.uvs))
    # Accept either diagonal and arbitrary vertex IDs/winding; reject duplicate triangles.
    vertices = dict(zip(geometry.vertex_ids, geometry.positions))
    triangles = [set(t) for t in geometry.triangles]
    checks["triangles"] = (len(vertices) == 4 and len(triangles) == 2
                           and all(len(t) == 3 and t <= vertices.keys() for t in triangles)
                           and len(triangles[0] | triangles[1]) == 4)
    if checks["triangles"]:
        shared = triangles[0] & triangles[1]
        a, b = [vertices[i] for i in shared]
        checks["triangles"] = abs(a[0] - b[0]) == 20 and abs(a[1] - b[1]) == 20
    checks["drawing"] = close(plain(mesh.drawing), plain(kasane.MeshDrawingData(
        session.asset_ids()[0] if session.asset_ids() else "")))
    if task == "parameter":
        checks["parameter"] = False
        checks["binding"] = False
        if len(session.parameter_ids()) == 1:
            p = session.parameter(session.parameter_ids()[0])
            checks["parameter"] = close(plain(p), dict(id=p.id, name="Open", minimum=0,
                maximum=1, default_value=0, repeat=False, kind="normal"))
            binding = session.binding_for_mesh(mesh.id)
            if binding:
                checks["binding"] = close(plain(binding.axes), [dict(parameter_id=p.id, keys=[0, 1])])
                checks["forms"] = len(binding.keyforms) == 2
                for value in (0, 1):
                    forms = [f for f in binding.keyforms if f.keys == [value]]
                    checks[f"form_{value}"] = len(forms) == 1 and close(
                        sorted(plain(forms[0].positions)), sorted([[x + 10 * value, y] for x, y in corners])) and (
                            forms[0].appearance == kasane.Appearance() and forms[0].draw_order is None)
            for value in (0, 0.5, 1):
                drawables = session.evaluate({p.id: value}).drawables
                checks[f"sample_{value}"] = len(drawables) == 1 and close(
                    sorted(plain(drawables[0].positions)),
                    sorted([[(x - 50) / 10 + value, (50 - y) / 10] for x, y in corners]))
    return checks


def grade(task, manifest, oracle):
    session = kasane.open_project(manifest)
    checks = {"structure": not session.validate_structure(),
              "resources": not session.diagnose_resources()}
    if task == "edit":
        actual = state(session)
        checks.update({key: close(actual.get(key), value) for key, value in oracle["expected"].items()})
    else:
        checks.update(creation_checks(session, task, oracle["texture_hash"]))
    return {"status": "passed" if all(checks.values()) else "failed", "checks": checks}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("operation", choices=["prepare", "grade"])
    parser.add_argument("--task", choices=["create", "parameter", "edit"], required=True)
    parser.add_argument("--packet", type=Path, required=True)
    parser.add_argument("--oracle", type=Path, required=True)
    parser.add_argument("--manifest", type=Path)
    args = parser.parse_args()
    if args.operation == "grade":
        try:
            result = grade(args.task, args.manifest, json.loads(args.oracle.read_text()))
        except Exception as exc:
            result = {"status": "failed", "error": f"{type(exc).__name__}: {exc}"}
        print(json.dumps(result, allow_nan=False))
        return
    import hashlib
    import tempfile
    texture = args.packet / "input/texture.png"
    session = make_project(texture, args.task)
    if args.task == "edit":
        source = session.save(args.packet / "input/project")
        session = kasane.open_project(source.manifest)
        repair(session)
    oracle = {"texture_hash": hashlib.sha256(texture.read_bytes()).hexdigest()}
    with tempfile.TemporaryDirectory(dir=args.oracle.parent) as folder:
        destination = Path(folder)
        manifest = session.save(destination / "positive").manifest
        oracle["expected"] = state(kasane.open_project(manifest))
        positive = grade(args.task, manifest, oracle)
        with session.edit("negative: wrong name") as edit:
            edit.rename_mesh(session.mesh_ids()[0], "incorrect")
        negative = grade(args.task, session.save(destination / "negative").manifest, oracle)
        # A second negative tests geometry rather than just metadata.
        session = kasane.open_project(manifest)
        mesh = session.mesh(session.mesh_ids()[0])
        with session.edit("negative: geometry") as edit:
            edit.update_positions(mesh.id, mesh.vertex_ids, [(x + 1, y) for x, y in mesh.positions])
        geometry = grade(args.task, session.save(destination / "geometry").manifest, oracle)
        extra = {}
        if args.task != "create":
            session = kasane.open_project(manifest)
            mesh = session.mesh(session.mesh_ids()[0])
            binding = session.binding_for_mesh(mesh.id)
            form = next(f for f in binding.keyforms if f.keys == [1])
            with session.edit("negative: keyform") as edit:
                edit.set_mesh_keyform(binding.id, form._replace(
                    positions=[(x + 1, y) for x, y in form.positions]))
            extra["wrong_keyform"] = grade(args.task, session.save(destination / "keyform").manifest, oracle)
        if args.task == "edit":
            extra["no_edit"] = grade(args.task, args.packet / "input/project", oracle)
    oracle["controls"] = {"positive": positive, "wrong_name": negative, "wrong_geometry": geometry}
    oracle["controls"].update(extra)
    args.oracle.write_text(json.dumps(oracle, indent=2, allow_nan=False) + "\n")
    if positive["status"] != "passed" or any(
            r["status"] != "failed" for name, r in oracle["controls"].items() if name != "positive"):
        raise RuntimeError("grader controls failed; trial must not be released")
    print(json.dumps({"status": "passed", "controls": oracle["controls"]}))


if __name__ == "__main__":
    main()
