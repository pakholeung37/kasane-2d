"""Host-only SDK operations. Executed with the experiment's installed Python.

The grader reads actual saved projects, never a participant's claimed metrics.
SDK conformance is still the responsibility of the separate SDK test suite.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
import shutil
import struct
import tempfile
from uuid import uuid4
import zlib

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


def state(session, parameter=None, values=(0, 0.5, 1)):
    result = {"document_id": session.document_id, "canvas": plain(session.canvas),
              "draw_order_groups": plain(session.draw_order_groups)}
    for group, reader in GROUPS.items():
        result[group] = {}
        for key in getattr(session, group + "_ids")():
            record = plain(getattr(session, reader)(key))
            if group == "asset":
                record.pop("source", None)  # Saving legitimately relocates resources.
            result[group][key] = record
    sample_parameter = parameter or ("Open" if session.parameter_ids() else None)
    result["samples"] = [plain(session.evaluate({sample_parameter: v}))
                         for v in values] if sample_parameter else []
    return result


def compose_state(session, values):
    result = state(session, "Expression", values)
    result["binding"] = {}
    for binding_id in session.binding_ids():
        binding = session.binding(binding_id)
        record = plain(binding)
        record.pop("id", None)  # Newly created binding IDs are participant choices.
        result["binding"][binding.mesh_id] = record
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


def png(path, width, height, color):
    def chunk(kind, payload):
        data = kind + payload
        return struct.pack(">I", len(payload)) + data + struct.pack(">I", zlib.crc32(data))
    rows = b"".join(b"\0" + bytes(color) * width for _ in range(height))
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
                     + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))


def advanced_fixture(task, packet):
    source = packet / "input"
    texture = packet / "input/texture.png"
    shutil.copy2(Path(__file__).parent / "texture.png", texture)
    session = kasane.Session(str(uuid4()), 100, 100, (50, 50), 10)
    a, p = str(uuid4()), str(uuid4())
    assets = [a]
    if task == "resource-recovery":
        second = str(uuid4())
        png(source / "second.png", 2, 2, (12, 80, 204, 255))
        assets.append(second)
    with session.edit("fixture") as edit:
        edit.add_png_asset(a, "primary", texture)
        if len(assets) > 1:
            edit.add_png_asset(assets[1], "secondary", source / "second.png")
        edit.create_rectangle(str(uuid4()), "left", a, (10, 40), (30, 60))
        edit.create_rectangle(str(uuid4()), "right", assets[-1], (40, 40), (60, 60))
        edit.create_parameter(p, "Open", 0, 1, 0)
    with session.edit("bindings") as edit:
        for mesh in session.mesh_ids():
            base = session.mesh(mesh).positions
            edit.create_mesh_binding(str(uuid4()), mesh, [kasane.Axis(p, [0, 1])], [
                kasane.MeshKeyform([0], base),
                kasane.MeshKeyform([1], [(x + 10, y) for x, y in base]),
            ])
    if task == "delivery-transfer":
        exported = source / "exported"
        result = session.export_package(exported)
        if not result.published or result.warnings:
            raise RuntimeError(f"fixture export failed: {result}")
        for child in exported.iterdir():
            shutil.move(str(child), source / child.name)
        exported.rmdir()
        texture.unlink()
        imported = kasane.Session(str(uuid4()), 100, 100, (50, 50), 10)
        imported.import_model3(source / "model.model3.json")
        target = next(m for m in imported.mesh_ids() if imported.mesh(m).positions[0][0] > 35)
        parameter = imported.parameter_ids()[0]
        request = {"mesh_runtime_id": imported.mesh_record(target).runtime_id,
                   "parameter_runtime_name": imported.parameter(parameter).name,
                   "key": 1, "space": "source canvas pixels", "delta": [7, 0]}
        (source / "request.json").write_text(json.dumps(request, indent=2) + "\n")
        baseline = state(imported, parameter)
        binding = imported.binding_for_mesh(target)
        form = next(f for f in binding.keyforms if f.keys == [1])
        with imported.edit("expected edit") as edit:
            edit.set_mesh_keyform(binding.id, form._replace(positions=[(x + 7, y) for x, y in form.positions]))
        return {"baseline": baseline, "expected": state(imported, parameter),
                "parameter_name": request["parameter_runtime_name"], "request": request}
    project = session.save(source / "project")
    expected = state(kasane.open_project(project.manifest), p)
    original = kasane.open_project(project.manifest)
    asset = original.asset(a)
    missing_path = source / "project" / asset.source
    candidates = source / "candidates"
    candidates.mkdir()
    shutil.copy2(missing_path, candidates / "tile_17.png")
    png(candidates / "tile_42.png", 2, 2, (255, 20, 30, 255))
    png(candidates / "tile_63.png", 3, 2, (255, 20, 30, 255))
    missing_path.unlink()
    texture.unlink()
    (source / "second.png").unlink()
    broken = kasane.open_project(project.manifest)
    if len(broken.diagnose_resources()) != 1:
        raise RuntimeError("resource fixture does not have exactly one missing asset")
    return {"expected": expected, "parameter_name": "Open", "missing_asset": a,
            "correct_candidate_sha256": asset.sha256}


def advanced_checks(task, manifest, package, oracle):
    checks = {}
    session = kasane.open_project(manifest)
    parameter = oracle["parameter_name"]
    checks["structure"] = not session.validate_structure()
    checks["resources"] = not session.diagnose_resources()
    checks["saved_state"] = close(state(session, parameter), oracle["expected"])
    checks["package_path"] = package.name == "model.model3.json" and package.is_file()
    package_dir = package.parent
    try:
        model = json.loads(package.read_text())
        references = model["FileReferences"]
        paths = [references["Moc"], *references["Textures"]]
        checks["package_refs"] = all(
            not Path(p).is_absolute() and ".." not in Path(p).parts
            and (package_dir / p).resolve().is_relative_to(package_dir.resolve())
            and (package_dir / p).is_file() for p in paths)
    except (OSError, KeyError, TypeError, ValueError):
        checks["package_refs"] = False
    with tempfile.TemporaryDirectory() as folder:
        receiver = Path(folder)
        shutil.copytree(manifest.parent, receiver / "project")
        shutil.copytree(package_dir, receiver / "package")
        moved = kasane.open_project(receiver / "project" / manifest.name)
        checks["moved_project"] = not moved.diagnose_resources() and close(state(moved, parameter), oracle["expected"])
        # An absolute asset reference to the original location is not a portable delivery.
        checks["internal_assets"] = all(not Path(moved.asset(a).source).is_absolute()
                                        and ".." not in Path(moved.asset(a).source).parts
                                        for a in moved.asset_ids())
        imported = kasane.Session(str(uuid4()), 100, 100, (50, 50), 10)
        imported.import_model3(receiver / "package/model.model3.json")
        checks["moved_package"] = not imported.diagnose_resources() and not imported.validate_structure()
        checks["package_semantics"] = close(semantic(imported, parameter), oracle["semantic"])
    return {"status": "passed" if all(checks.values()) else "failed", "checks": checks}


def semantic(session, parameter):
    if parameter not in (session.parameter(pid).name for pid in session.parameter_ids()):
        parameter = session.parameter_ids()[0]
    meshes = {}
    for mid in session.mesh_ids():
        m = session.mesh_record(mid)
        vertex_index = {vertex: index for index, vertex in enumerate(m.geometry.vertex_ids)}
        meshes[m.runtime_id] = {"positions": plain(m.geometry.positions), "uvs": plain(m.geometry.uvs),
                                "triangles": sorted(sorted(vertex_index[v] for v in t) for t in m.geometry.triangles),
                                "drawing": plain(m.drawing.appearance),
                                "texture_hash": session.asset(m.drawing.texture_asset_id).sha256}
    values = {}
    for v in (0, 0.5, 1):
        sample = session.evaluate({parameter: v})
        values[str(v)] = {session.mesh_record(d.id).runtime_id: plain(d.positions) for d in sample.drawables}
    params = sorted([p.minimum, p.maximum, p.default_value, p.repeat, p.kind]
                    for p in (session.parameter(pid) for pid in session.parameter_ids()))
    return {"canvas": plain(session.canvas), "meshes": meshes, "parameters": params, "values": values}


def advanced_solution(task, packet, oracle, variant="positive"):
    if task == "delivery-transfer":
        session = kasane.Session(str(uuid4()), 100, 100, (50, 50), 10)
        session.import_model3(packet / "input/model.model3.json")
        target = next(m for m in session.mesh_ids() if session.mesh_record(m).runtime_id == oracle["request"]["mesh_runtime_id"])
        if variant == "wrong_target":
            target = next(m for m in session.mesh_ids() if m != target)
        binding = session.binding_for_mesh(target)
        form = next(f for f in binding.keyforms if f.keys == [1])
        if variant != "no_edit":
            shift = 8 if variant == "wrong_delta" else 7
            with session.edit("control edit") as edit:
                edit.set_mesh_keyform(binding.id, form._replace(positions=[(x + shift, y) for x, y in form.positions]))
    else:
        session = kasane.open_project(packet / "input/project")
        candidate = "tile_42.png" if variant == "wrong_content" else "tile_17.png"
        if variant != "no_recovery":
            with session.edit("control recovery") as edit:
                if variant == "wrong_content":
                    asset = session.asset(oracle["missing_asset"])
                    edit.replace_png_asset(asset.id, asset.name, packet / "input/candidates" / candidate)
                else:
                    edit.relocate_png_asset(oracle["missing_asset"], packet / "input/candidates" / candidate)
    return session


def safe_advanced_grade(task, manifest, package, oracle):
    try:
        return advanced_checks(task, manifest, package, oracle)
    except Exception as exc:
        return {"status": "failed", "error": f"{type(exc).__name__}: {exc}"}


def prepare_advanced(task, packet, oracle_path):
    oracle = advanced_fixture(task, packet)
    correct = advanced_solution(task, packet, oracle)
    oracle["semantic"] = semantic(correct, oracle["parameter_name"])
    controls = {}
    with tempfile.TemporaryDirectory(dir=oracle_path.parent) as folder:
        root = Path(folder)
        for variant in (["positive", "no_edit", "wrong_target", "wrong_delta"] if task == "delivery-transfer"
                        else ["positive", "no_recovery", "wrong_content"]):
            try:
                session = advanced_solution(task, packet, oracle, variant)
                destination = root / variant
                manifest = session.save(destination / "project").manifest
                session.export_package(destination / "package")
                controls[variant] = safe_advanced_grade(task, manifest, destination / "package/model.model3.json", oracle)
            except Exception as exc:
                controls[variant] = {"status": "failed", "error": f"{type(exc).__name__}: {exc}"}
        good = root / "positive"
        missing = root / "missing_texture"
        shutil.copytree(good, missing)
        model = missing / "package/model.model3.json"
        ref = json.loads(model.read_text())["FileReferences"]["Textures"][0]
        (model.parent / ref).unlink()
        controls["missing_texture"] = safe_advanced_grade(task, missing / "project" / (good / "project/project.kasane.json").name,
                                                           model, oracle)
        # The exported package must be self-contained; an absolute external reference fails.
        external = root / "external_reference"
        shutil.copytree(good, external)
        model = external / "package/model.model3.json"
        raw = json.loads(model.read_text())
        raw["FileReferences"]["Textures"][0] = str(packet / "input/candidates/tile_17.png") if task == "resource-recovery" else str(packet / "input/textures/0.png")
        model.write_text(json.dumps(raw))
        controls["external_reference"] = safe_advanced_grade(task, external / "project/project.kasane.json", model, oracle)
    oracle["controls"] = controls
    oracle_path.write_text(json.dumps(oracle, indent=2, allow_nan=False) + "\n")
    if controls["positive"]["status"] != "passed" or any(v["status"] != "failed" for k, v in controls.items() if k != "positive"):
        raise RuntimeError(f"advanced grader controls failed: {controls}")
    return {"status": "passed", "controls": controls}


def visual_fixture(packet, parented=False):
    if not kasane.capabilities()["gpu_observation"]:
        raise RuntimeError("visual-locate requires the observe wheel")
    source = packet / "input"
    texture_dir = source / "fixture-textures"
    texture_dir.mkdir()
    colors = [(240, 70, 70, 255), (60, 200, 90, 255), (70, 100, 240, 255)]
    for index, color in enumerate(colors):
        png(texture_dir / f"{index}.png", 8, 8, color)
    session = kasane.Session(str(uuid4()), 100, 100, (50, 50), 10)
    parameter = str(uuid4())
    positions = [((12, 20), (30, 38)), ((42, 20), (60, 38)), ((70, 55), (88, 73))]
    meshes = []
    with session.edit("three part fixture") as edit:
        for index, (minimum, maximum) in enumerate(positions):
            asset, mesh = str(uuid4()), str(uuid4())
            edit.add_png_asset(asset, f"texture-{index}", texture_dir / f"{index}.png")
            edit.create_rectangle(mesh, f"segment-{index}", asset, minimum, maximum)
            meshes.append(mesh)
        edit.create_parameter(parameter, "Pose", 0, 1, 0)
        if parented:
            transform = str(uuid4())
            edit.create_rotation_transform(transform, "hinge", kasane.RotationData(
                0, kasane.RotationPose((51, 29), angle=35)))
            edit.set_deform_parent(meshes[1], transform)
    if parented:
        local_square = [(-0.9, 0.9), (0.9, 0.9), (0.9, -0.9), (-0.9, -0.9)]
        with session.edit("green parent-local geometry") as edit:
            edit.update_positions(meshes[1], session.mesh(meshes[1]).vertex_ids, local_square)
    offsets = [(5, 0), (0.4, -0.3) if parented else (0, 0), (0, -4)]
    with session.edit("endpoint motion") as edit:
        for mesh, (dx, dy) in zip(meshes, offsets):
            base = session.mesh(mesh).positions
            edit.create_mesh_binding(str(uuid4()), mesh, [kasane.Axis(parameter, [0, 1])], [
                kasane.MeshKeyform([0], base),
                kasane.MeshKeyform([1], [(x + dx, y + dy) for x, y in base]),
            ])
    oracle = {"parameter_name": "Pose", "expected": state(session, parameter),
              "target_mesh_id": meshes[1], "offset": [1.2 if parented else 12, 0],
              "parented": parented,
              "sample_values": [0, 0.5, 1],
              "observer": {"width": 256, "height": 256, "fit_long_side": 256}}
    with kasane.Observer(256, 256, 256) as observer:
        rgba = {}
        for value in (0, 0.5, 1):
            first = observer.observe(session, {parameter: value})
            second = observer.observe(session, {parameter: value})
            if first.rgba != second.rgba:
                raise RuntimeError("GPU reference render is not repeatable")
            rgba[str(value)] = hashlib.sha256(first.rgba).hexdigest()
            if value == 1:
                first.save_png(source / "reference-endpoint.png")
        oracle["rgba_sha256"] = rgba
    target = meshes[1]
    binding = session.binding_for_mesh(target)
    form = next(f for f in binding.keyforms if f.keys == [1])
    with session.edit("perturb endpoint") as edit:
        offset = oracle["offset"][0]
        edit.set_mesh_keyform(binding.id, form._replace(positions=[(x + offset, y) for x, y in form.positions]))
    session.save(source / "project")
    with kasane.Observer(256, 256, 256) as observer:
        wrong = observer.observe(session, {parameter: 1})
        if hashlib.sha256(wrong.rgba).hexdigest() == oracle["rgba_sha256"]["1"]:
            raise RuntimeError("visual fixture perturbation is invisible")
    shutil.rmtree(texture_dir)
    (source / "observation.json").write_text(json.dumps({"parameter": "Pose", "sample": 1,
        "observer": oracle["observer"]}, indent=2) + "\n")
    return oracle


def visual_checks(manifest, result, oracle):
    session = kasane.open_project(manifest)
    samples = oracle["sample_values"]
    snapshot = compose_state if oracle.get("state_kind") == "compose" else state
    checks = {"structure": not session.validate_structure(),
              "resources": not session.diagnose_resources(),
              "saved_state": close(snapshot(session, samples) if snapshot is compose_state else
                                   snapshot(session, oracle["parameter_name"], samples), oracle["expected"])}
    expected_pngs = []
    with kasane.Observer(**oracle["observer"]) as observer:
        for value in samples:
            frame = observer.observe(session, {oracle["parameter_name"]: value})
            checks[f"rgba_{value}"] = hashlib.sha256(frame.rgba).hexdigest() == oracle["rgba_sha256"][str(value)]
            expected_pngs.append(frame.png)
    if result is not None:
        output = result.parent.resolve()
        data = json.loads(result.read_text())
        paths = [data["observation_report"], data["contact_sheet"], *data["frames"]]
        resolved = [Path(p).resolve(strict=True) for p in paths]
        checks["artifacts_inside_output"] = len(data["frames"]) == len(samples) and all(p.is_relative_to(output) for p in resolved)
        checks["observation_report"] = (json.loads(resolved[0].read_text()).get("status") == "frames_complete")
        checks["images_present"] = all(p.read_bytes().startswith(b"\x89PNG\r\n\x1a\n") for p in resolved[1:])
        checks["frames_match_project"] = [p.read_bytes() for p in resolved[2:]] == expected_pngs
    with tempfile.TemporaryDirectory() as folder:
        moved_path = Path(folder) / "project"
        shutil.copytree(manifest.parent, moved_path)
        moved = kasane.open_project(moved_path / manifest.name)
        moved_state = snapshot(moved, samples) if snapshot is compose_state else snapshot(moved, oracle["parameter_name"], samples)
        checks["moved_project"] = not moved.diagnose_resources() and close(moved_state, oracle["expected"])
    return {"status": "passed" if all(checks.values()) else "failed", "checks": checks}


def safe_visual_grade(manifest, result, oracle):
    try:
        return visual_checks(manifest, result, oracle)
    except Exception as exc:
        return {"status": "failed", "error": f"{type(exc).__name__}: {exc}"}


def prepare_visual(packet, oracle_path, parented=False):
    oracle = visual_fixture(packet, parented)
    controls = {}
    with tempfile.TemporaryDirectory(dir=oracle_path.parent) as folder:
        root = Path(folder)
        for variant in ("positive", "no_edit", "wrong_target", "wrong_amount"):
            session = kasane.open_project(packet / "input/project")
            if variant != "no_edit":
                target = oracle["target_mesh_id"]
                if variant == "wrong_target":
                    target = next(m for m in session.mesh_ids() if m != target)
                binding = session.binding_for_mesh(target)
                form = next(f for f in binding.keyforms if f.keys == [1])
                shift = (1.1 if parented else 11) if variant == "wrong_amount" else oracle["offset"][0]
                with session.edit("control") as edit:
                    edit.set_mesh_keyform(binding.id, form._replace(positions=[(x - shift, y) for x, y in form.positions]))
            manifest = session.save(root / variant / "project").manifest
            run = None
            if variant == "positive":
                with kasane.Observer(256, 256, 256) as observer:
                    observed = observer.observe_run(session, [{"Pose": v} for v in (0, 0.5, 1)], root / variant / "observations")
                data = {"observation_report": str(observed.report), "contact_sheet": str(observed.contact_sheet),
                        "frames": [str(p) for p in observed.frames]}
                run = root / variant / "result.json"
                run.write_text(json.dumps(data))
            controls[variant] = safe_visual_grade(manifest, run, oracle)
    oracle["controls"] = controls
    oracle_path.write_text(json.dumps(oracle, indent=2, allow_nan=False) + "\n")
    if controls["positive"]["status"] != "passed" or any(v["status"] != "failed" for k, v in controls.items() if k != "positive"):
        raise RuntimeError(f"visual controls failed: {controls}")
    return {"status": "passed", "controls": controls}


def expression_bindings(session, included=("mouth", "left-mark", "right-mark"), mark_delta=-6):
    parameter = session.parameter_id("Expression")
    with session.edit("expression bindings") as edit:
        for name in included:
            mesh = session.require_unique_mesh(name)
            base = mesh.positions
            if name == "mouth":
                middle = (min(y for _, y in base) + max(y for _, y in base)) / 2
                endpoint = [(x, y - 5 if y < middle else y + 5) for x, y in base]
            else:
                endpoint = [(x, y + mark_delta) for x, y in base]
            edit.create_mesh_binding(str(uuid4()), mesh.id, [kasane.Axis(parameter, [0, 1])], [
                kasane.MeshKeyform([0], base), kasane.MeshKeyform([1], endpoint)])


def compose_fixture(packet):
    if not kasane.capabilities()["gpu_observation"]:
        raise RuntimeError("compose-expression requires the observe wheel")
    source = packet / "input"
    textures = source / "fixture-textures"
    textures.mkdir()
    colors = [(230, 80, 110, 255), (240, 200, 40, 255), (80, 190, 240, 255)]
    names = ["mouth", "left-mark", "right-mark"]
    boxes = [((40, 50), (60, 60)), ((20, 35), (28, 43)), ((72, 35), (80, 43))]
    session = kasane.Session(str(uuid4()), 100, 100, (50, 50), 10)
    with session.edit("expression base") as edit:
        for index, (name, (minimum, maximum)) in enumerate(zip(names, boxes)):
            texture = textures / f"{index}.png"
            png(texture, 8, 8, colors[index])
            asset = str(uuid4())
            edit.add_png_asset(asset, f"paint-{index}", texture)
            edit.create_rectangle(str(uuid4()), name, asset, minimum, maximum)
        edit.create_parameter(str(uuid4()), "Expression", 0, 1, 0)
    session.save(source / "project")
    shutil.rmtree(textures)
    truth = kasane.open_project(source / "project")
    expression_bindings(truth)
    samples = [0, 0.25, 0.5, 0.75, 1]
    oracle = {"parameter_name": "Expression", "sample_values": samples,
              "observer": {"width": 256, "height": 256, "fit_long_side": 256},
              "state_kind": "compose", "expected": compose_state(truth, samples)}
    with kasane.Observer(256, 256, 256) as observer:
        rgba = {}
        for value in samples:
            first = observer.observe(truth, {"Expression": value})
            if first.rgba != observer.observe(truth, {"Expression": value}).rgba:
                raise RuntimeError("expression reference render is not repeatable")
            rgba[str(value)] = hashlib.sha256(first.rgba).hexdigest()
            if value in (0, 1):
                first.save_png(source / f"reference-{value}.png")
        oracle["rgba_sha256"] = rgba
    (source / "request.json").write_text(json.dumps({"parameter": "Expression", "targets": names,
        "mouth_top_delta_y": -5, "mouth_bottom_delta_y": 5, "mark_delta_y": -6,
        "observer": oracle["observer"], "samples": samples}, indent=2) + "\n")
    return oracle


def prepare_compose(packet, oracle_path):
    oracle = compose_fixture(packet)
    controls = {}
    with tempfile.TemporaryDirectory(dir=oracle_path.parent) as folder:
        root = Path(folder)
        for variant in ("positive", "no_bindings", "mouth_only", "one_mark", "wrong_mark"):
            session = kasane.open_project(packet / "input/project")
            if variant == "positive":
                expression_bindings(session)
            elif variant == "mouth_only":
                expression_bindings(session, ("mouth",))
            elif variant == "one_mark":
                expression_bindings(session, ("mouth", "left-mark"))
            elif variant == "wrong_mark":
                expression_bindings(session, mark_delta=-5)
            manifest = session.save(root / variant / "project").manifest
            result_path = None
            if variant == "positive":
                with kasane.Observer(256, 256, 256) as observer:
                    run = observer.observe_run(session, [{"Expression": value} for value in oracle["sample_values"]], root / variant / "observations")
                result_path = root / variant / "result.json"
                result_path.write_text(json.dumps({"observation_report": str(run.report),
                    "contact_sheet": str(run.contact_sheet), "frames": [str(p) for p in run.frames]}))
            controls[variant] = safe_visual_grade(manifest, result_path, oracle)
    oracle["controls"] = controls
    oracle_path.write_text(json.dumps(oracle, indent=2, allow_nan=False) + "\n")
    if controls["positive"]["status"] != "passed" or any(v["status"] != "failed" for k, v in controls.items() if k != "positive"):
        raise RuntimeError(f"expression controls failed: {controls}")
    return {"status": "passed", "controls": controls}


def revise_handoff(session, included=("left-mark", "right-mark"), delta=-3):
    with session.edit("revise mark amplitude") as edit:
        for name in included:
            mesh = session.require_unique_mesh(name)
            binding = session.binding_for_mesh(mesh.id)
            base = next(f for f in binding.keyforms if f.keys == [0])
            endpoint = next(f for f in binding.keyforms if f.keys == [1])
            edit.set_mesh_keyform(binding.id, endpoint._replace(
                positions=[(x, y + delta) for x, y in base.positions]))


def handoff_fixture(packet):
    if not kasane.capabilities()["gpu_observation"]:
        raise RuntimeError("handoff-revision requires the observe wheel")
    source = packet / "input"
    session = kasane.open_project(source / "project")
    if session.validate_structure() or session.diagnose_resources():
        raise RuntimeError("handoff source is not a valid self-contained project")
    for name in ("mouth", "left-mark", "right-mark"):
        mesh = session.require_unique_mesh(name)
        binding = session.binding_for_mesh(mesh.id)
        if binding is None or len(binding.keyforms) != 2:
            raise RuntimeError("handoff source lacks complete expression bindings")
        if name != "mouth":
            base = next(f for f in binding.keyforms if f.keys == [0])
            endpoint = next(f for f in binding.keyforms if f.keys == [1])
            if not close(endpoint.positions, [(x, y - 6) for x, y in base.positions]):
                raise RuntimeError("handoff mark endpoint does not have expected prior amplitude")
    samples = [0, 0.25, 0.5, 0.75, 1]
    baseline = state(session, "Expression", samples)
    revise_handoff(session)
    oracle = {"parameter_name": "Expression", "sample_values": samples,
              "observer": {"width": 256, "height": 256, "fit_long_side": 256},
              "baseline": baseline, "expected": state(session, "Expression", samples)}
    with kasane.Observer(256, 256, 256) as observer:
        rgba = {}
        for value in samples:
            first = observer.observe(session, {"Expression": value})
            if first.rgba != observer.observe(session, {"Expression": value}).rgba:
                raise RuntimeError("handoff reference render is not repeatable")
            rgba[str(value)] = hashlib.sha256(first.rgba).hexdigest()
            if value in (0, 1):
                first.save_png(source / f"reference-{value}.png")
        oracle["rgba_sha256"] = rgba
    (source / "request.json").write_text(json.dumps({"parameter": "Expression",
        "preserve": ["mouth"], "revise": ["left-mark", "right-mark"],
        "new_mark_delta_y_from_base": -3, "samples": samples,
        "observer": oracle["observer"]}, indent=2) + "\n")
    return oracle


def prepare_handoff(packet, oracle_path):
    oracle = handoff_fixture(packet)
    controls = {}
    with tempfile.TemporaryDirectory(dir=oracle_path.parent) as folder:
        root = Path(folder)
        for variant in ("positive", "no_edit", "one_mark", "wrong_amount", "changed_mouth"):
            session = kasane.open_project(packet / "input/project")
            if variant == "positive":
                revise_handoff(session)
            elif variant == "one_mark":
                revise_handoff(session, ("left-mark",))
            elif variant == "wrong_amount":
                revise_handoff(session, delta=-4)
            elif variant == "changed_mouth":
                revise_handoff(session)
                mesh = session.require_unique_mesh("mouth")
                binding = session.binding_for_mesh(mesh.id)
                endpoint = next(f for f in binding.keyforms if f.keys == [1])
                with session.edit("negative mouth change") as edit:
                    edit.set_mesh_keyform(binding.id, endpoint._replace(
                        positions=[(x, y + 1) for x, y in endpoint.positions]))
            manifest = session.save(root / variant / "project").manifest
            result_path = None
            if variant == "positive":
                with kasane.Observer(256, 256, 256) as observer:
                    run = observer.observe_run(session, [{"Expression": value} for value in oracle["sample_values"]], root / variant / "observations")
                result_path = root / variant / "result.json"
                result_path.write_text(json.dumps({"observation_report": str(run.report),
                    "contact_sheet": str(run.contact_sheet), "frames": [str(p) for p in run.frames]}))
            controls[variant] = safe_visual_grade(manifest, result_path, oracle)
    oracle["controls"] = controls
    oracle_path.write_text(json.dumps(oracle, indent=2, allow_nan=False) + "\n")
    if controls["positive"]["status"] != "passed" or any(v["status"] != "failed" for k, v in controls.items() if k != "positive"):
        raise RuntimeError(f"handoff controls failed: {controls}")
    return {"status": "passed", "controls": controls}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("operation", choices=["prepare", "grade"])
    parser.add_argument("--task", choices=["create", "parameter", "edit", "delivery-transfer", "resource-recovery", "visual-locate", "visual-parent", "compose-expression", "handoff-revision"], required=True)
    parser.add_argument("--packet", type=Path, required=True)
    parser.add_argument("--oracle", type=Path, required=True)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--package", type=Path)
    parser.add_argument("--result", type=Path)
    args = parser.parse_args()
    if args.operation == "grade":
        try:
            oracle = json.loads(args.oracle.read_text())
            result = (safe_visual_grade(args.manifest, args.result, oracle)
                      if args.task in ("visual-locate", "visual-parent", "compose-expression", "handoff-revision") else
                      safe_advanced_grade(args.task, args.manifest, args.package, oracle)
                      if args.task in ("delivery-transfer", "resource-recovery")
                      else grade(args.task, args.manifest, oracle))
        except Exception as exc:
            result = {"status": "failed", "error": f"{type(exc).__name__}: {exc}"}
        print(json.dumps(result, allow_nan=False))
        return
    if args.task in ("delivery-transfer", "resource-recovery"):
        print(json.dumps(prepare_advanced(args.task, args.packet, args.oracle), allow_nan=False))
        return
    if args.task in ("visual-locate", "visual-parent"):
        print(json.dumps(prepare_visual(args.packet, args.oracle, args.task == "visual-parent"), allow_nan=False))
        return
    if args.task == "compose-expression":
        print(json.dumps(prepare_compose(args.packet, args.oracle), allow_nan=False))
        return
    if args.task == "handoff-revision":
        print(json.dumps(prepare_handoff(args.packet, args.oracle), allow_nan=False))
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
