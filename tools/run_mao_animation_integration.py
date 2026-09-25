#!/usr/bin/env python3
"""Exercise the local Mao model through the installed Python wheel and Framework.

Run from outside the source tree with the freshly built wheel, for example:
  cd /tmp
  uv run --no-project --no-cache --python 3.14 \
    --with /absolute/repo/target/python-wheels/kasane-*.whl \
    python /absolute/repo/tools/run_mao_animation_integration.py
"""
from __future__ import annotations

import json
import math
from pathlib import Path
import shutil
import subprocess
import tempfile
import zipfile

import kasane

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "models/local/mao/runtime/mao_pro.model3.json"
OUT = ROOT / "target/mao-animation-integration"
PROJECT = OUT / "project"
PACKAGE = OUT / "live2d-model"
MODEL3 = PACKAGE / "model.model3.json"
ARCHIVE = OUT / "mao-live2d-model.zip"
FRAMEWORK = ROOT / "target/animation-cpu-probe/kasane_framework_animation_cpu_probe"
CDI_FRAMEWORK = ROOT / "target/cdi-probe-build/kasane_framework_cdi_cpu_probe"


def check(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def probe(executable: Path, *args: Path | str) -> dict:
    result = subprocess.run(
        [str(executable), *(str(item) for item in args)],
        text=True,
        capture_output=True,
        check=True,
        timeout=120,
    )
    objects = [
        json.loads(line)
        for line in result.stdout.splitlines()
        if line.startswith("{")
    ]
    check(len(objects) == 1, f"{executable.name} returned no JSON result")
    return objects[0]


def refs(model3: dict) -> list[str]:
    files = model3["FileReferences"]
    paths = [files["Moc"], *files["Textures"]]
    paths += [files[key] for key in ("DisplayInfo", "Physics", "Pose") if key in files]
    paths += [entry["File"] for entry in files.get("Expressions", [])]
    for entries in files.get("Motions", {}).values():
        for entry in entries:
            paths.append(entry["File"])
            if "Sound" in entry:
                paths.append(entry["Sound"])
    if "UserData" in files:
        paths.append(files["UserData"])
    return paths


def finite_snapshot(snapshot) -> None:
    check(all(math.isfinite(value) for value in snapshot.parameters.values()),
          "preview produced a nonfinite parameter")
    check(all(math.isfinite(value) for value in snapshot.part_opacities.values()),
          "preview produced a nonfinite Part opacity")
    check(math.isfinite(snapshot.model_opacity), "preview produced nonfinite model opacity")


def semantic_equal(a, b, path="$") -> None:
    if isinstance(a, dict) and isinstance(b, dict):
        check(a.keys() == b.keys(), f"{path}: JSON keys changed")
        for key in a:
            semantic_equal(a[key], b[key], f"{path}.{key}")
    elif isinstance(a, list) and isinstance(b, list):
        check(len(a) == len(b), f"{path}: JSON list length changed")
        for index, (left, right) in enumerate(zip(a, b)):
            semantic_equal(left, right, f"{path}[{index}]")
    elif (isinstance(a, (int, float)) and not isinstance(a, bool)
          and isinstance(b, (int, float)) and not isinstance(b, bool)):
        check(math.isclose(a, b, rel_tol=1e-6, abs_tol=1e-5),
              f"{path}: {a} != {b}")
    else:
        check(a == b, f"{path}: {a!r} != {b!r}")


def main() -> None:
    check(SOURCE.is_file(), f"missing local sample: {SOURCE}")
    check(FRAMEWORK.is_file() and CDI_FRAMEWORK.is_file(),
          "build the official Framework CPU probes first")
    OUT.mkdir(parents=True, exist_ok=True)
    for path in (PROJECT, PACKAGE):
        if path.exists():
            shutil.rmtree(path)
    if ARCHIVE.exists():
        ARCHIVE.unlink()

    session = kasane.Session("00000000-0000-4000-8000-000000000001",
                             64, 64, (32, 32), 1)
    imported = session.import_model3(SOURCE)
    check(not session.validate_structure(), "imported document has structure issues")
    check(not session.diagnose_resources(), "imported document has resource issues")
    check(not session.missing_attachments(), "imported document has missing attachments")
    check(len(session.motion_ids()) == 7, "Mao should have 7 Motion clips")
    check(len(session.expression_ids()) == 8, "Mao should have 8 Expressions")
    check(len(session.mesh_ids()) == 260, "Mao MOC drawable count changed")
    groups = session.motion_groups()
    check({group["name"] for group in groups} == {"", "Idle"},
          "Motion registration groups changed")

    save = session.save(PROJECT)
    check(save.durable, "project save was not durable")
    reopened = kasane.open_project(PROJECT)
    check(len(reopened.motion_ids()) == 7 and len(reopened.expression_ids()) == 8,
          "saved project lost animation assets")
    check(reopened.motion_groups() == groups, "saved project changed registrations")

    playback: list[dict] = []
    for group in groups:
        for index, entry in enumerate(group["entries"]):
            preview = reopened.motion_preview()
            preview.schedule_motion_entry(group["name"], index, 0)
            preview.advance(0)
            frame = preview.advance(0.5)
            finite_snapshot(frame)
            check(entry["clip_id"] in frame.active_motions,
                  f"{group['name']}[{index}] was not active")
            check(len(preview.frame().drawables) == 260,
                  f"{group['name']}[{index}] lost drawables")
            playback.append({
                "group": group["name"],
                "index": index,
                "active": len(frame.active_motions),
                "coverage": frame.coverage,
            })

    expression_frames = 0
    for expression_id in reopened.expression_ids():
        preview = reopened.expression_preview()
        preview.schedule_expression(expression_id, 0)
        preview.advance(0)
        snapshot = preview.advance(0.5)
        check(all(math.isfinite(value) for value in snapshot.parameters.values()),
              "expression produced a nonfinite parameter")
        check(len(preview.frame().drawables) == 260,
              "expression preview lost drawables")
        expression_frames += 1

    combined = reopened.motion_preview()
    combined.schedule_motion_entry("Idle", 0, 0)
    combined.schedule_expression(reopened.expression_ids()[0], 0)
    first = combined.seek(0.5)
    finite_snapshot(first)
    check(first == combined.seek(0.5), "repeat seek changed the same animation frame")
    check(len(combined.frame().drawables) == 260,
          "combined animation preview lost drawables")
    check(not reopened.validate_structure(), "preview modified project structure")

    exported = reopened.export_package(PACKAGE)
    check(exported.published and exported.durable, "package was not durably published")
    source_model3 = json.loads(SOURCE.read_text())
    model3 = json.loads(MODEL3.read_text())
    check(model3["Version"] == 3, "model3 version changed")
    check(len(refs(model3)) == len(set(refs(model3))),
          "package has duplicate file references")
    for relative in refs(model3):
        path = Path(relative)
        check(not path.is_absolute() and ".." not in path.parts
              and (PACKAGE / path).is_file(),
              f"unsafe or missing package reference: {relative}")
    check(source_model3["Groups"] == model3["Groups"], "model3 Groups changed")
    check(source_model3["HitAreas"] == model3["HitAreas"],
          "model3 HitAreas changed")

    source_refs = source_model3["FileReferences"]
    output_refs = model3["FileReferences"]
    for key in ("DisplayInfo", "Physics", "Pose"):
        semantic_equal(
            json.loads((SOURCE.parent / source_refs[key]).read_text()),
            json.loads((PACKAGE / output_refs[key]).read_text()),
        )
    for old, new in zip(source_refs["Textures"], output_refs["Textures"]):
        check((SOURCE.parent / old).read_bytes() == (PACKAGE / new).read_bytes(),
              "texture bytes changed")
    for old, new in zip(source_refs["Expressions"], output_refs["Expressions"]):
        check(old["File"] == new["File"],
              f"expression filename changed: {old['File']} -> {new['File']}")
        semantic_equal(json.loads((SOURCE.parent / old["File"]).read_text()),
                       json.loads((PACKAGE / new["File"]).read_text()))
    for group, old_entries in source_refs["Motions"].items():
        for old, new in zip(old_entries, output_refs["Motions"][group]):
            check(old["File"] == new["File"],
                  f"motion filename changed: {old['File']} -> {new['File']}")
            source_motion = json.loads((SOURCE.parent / old["File"]).read_text())
            source_motion.setdefault("UserData", [])
            semantic_equal(source_motion,
                           json.loads((PACKAGE / new["File"]).read_text()))

    with tempfile.TemporaryDirectory(prefix="mao-detached-", dir=OUT) as temporary:
        detached = Path(temporary) / "model"
        shutil.copytree(PACKAGE, detached)
        detached_model3 = detached / "model.model3.json"
        detached_refs = json.loads(detached_model3.read_text())["FileReferences"]
        result = probe(FRAMEWORK, "--model3-check", detached_model3)
        check(result["expressions"] == 8 and result["motion_groups"] == 2,
              "Framework model3 loader lost animation registrations")
        cdi = probe(CDI_FRAMEWORK, "--load", detached / detached_refs["DisplayInfo"])
        check(cdi["accepted"], "Framework rejected CDI")
        moc = detached / detached_refs["Moc"]
        expression_checks = [
            probe(FRAMEWORK, "--expression-check", detached / item["File"])
            for item in detached_refs["Expressions"]
        ]
        motion_checks = []
        max_motion_error = 0.0
        first_parameter = reopened.parameter_ids()[0]
        for group in groups:
            for index, entry in enumerate(detached_refs["Motions"][group["name"]]):
                motion_path = detached / entry["File"]
                motion_checks.append(probe(FRAMEWORK, "--motion-json-check", motion_path))
                official = probe(FRAMEWORK, "--motion-loop", moc, motion_path)
                check(official["configs"][0]["behavior"] == 1,
                      "Framework did not use MotionBehavior V2")
                preview = reopened.motion_preview()
                preview.schedule_motion_entry(group["name"], index, 0)
                for delta, frame in zip(
                    (0.25, 0.25, 0.5, 0.25, 0.25, 0.25, 0.5),
                    official["configs"][0]["frames"],
                ):
                    actual = preview.advance(delta).parameters[first_parameter]
                    max_motion_error = max(
                        max_motion_error, abs(actual - frame["param_x"])
                    )
        check(all(item["consistent"] for item in motion_checks),
              "Framework rejected a Motion count")
        check(max_motion_error <= 1e-3,
              f"Mao Motion V2 drifted from Framework: {max_motion_error}")
        probe(FRAMEWORK, "--pose-check", moc, detached / detached_refs["Pose"])
        physics = probe(FRAMEWORK, "--physics-sequence",
                        moc, detached / detached_refs["Physics"])
        check(len(physics["frames"]) == 120,
              "Framework Physics sequence was incomplete")
        detached_session = kasane.Session(
            "00000000-0000-4000-8000-000000000002", 64, 64, (32, 32), 1
        )
        detached_session.import_model3(detached_model3)
        check(len(detached_session.motion_ids()) == 7
              and len(detached_session.expression_ids()) == 8,
              "detached package could not be reimported")

    with zipfile.ZipFile(ARCHIVE, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(PACKAGE.rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(PACKAGE))

    report = {
        "status": "passed",
        "source": str(SOURCE),
        "model3": str(MODEL3),
        "zip": str(ARCHIVE),
        "import_diagnostics": [
            {"code": item.code, "asset_id": item.asset_id, "message": item.message}
            for item in imported.diagnostics
        ],
        "motion_previews": playback,
        "expression_previews": expression_frames,
        "combined_seek": "passed",
        "project_reopen": "passed",
        "source_export_semantics": "passed",
        "detached_reimport": "passed",
        "framework_model3": result,
        "framework_cdi": cdi,
        "framework_expressions": len(expression_checks),
        "framework_motions": len(motion_checks),
        "framework_motion_v2_max_parameter_error": max_motion_error,
        "framework_pose": "passed",
        "framework_physics_frames": len(physics["frames"]),
        "viewer_check": "pending_user",
    }
    (OUT / "report.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    print(f"passed: {OUT / 'report.json'}")


if __name__ == "__main__":
    main()
