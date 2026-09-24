#!/usr/bin/env python3
"""Grade the first SDK usability task using only the installed public Python API.

The candidate must be a separately saved project. This checker never writes either
project and does not inspect the participant's script or implementation details.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
import sys

import kasane


COLLECTIONS = {
    "assets": ("asset_ids", "asset"),
    "meshes": ("mesh_ids", "mesh_record"),
    "parameters": ("parameter_ids", "parameter"),
    "bindings": ("binding_ids", "binding"),
    "parts": ("part_ids", "part"),
    "transforms": ("transform_ids", "transform"),
    "scene_bindings": ("scene_binding_ids", "scene_binding"),
    "blend_key_tables": ("blend_key_table_ids", "blend_key_table"),
    "blend_constraints": ("blend_constraint_ids", "blend_constraint"),
    "blend_bindings": ("blend_binding_ids", "blend_binding"),
    "glues": ("glue_ids", "glue"),
    "offscreens": ("offscreen_ids", "offscreen"),
}
SAMPLES = (0.0, 0.5, 1.0)
TOLERANCE = 1e-4


def project_files(project: Path) -> dict[str, str]:
    result = {}
    for path in sorted(project.rglob("*")):
        if path.is_file() and path.name != ".kasane.lock":
            result[str(path.relative_to(project))] = hashlib.sha256(path.read_bytes()).hexdigest()
    return result


def content(snapshot, *, asset: bool = False):
    """Remove version fields and the expected save-as resource relocation."""
    fields = snapshot._asdict()
    fields.pop("version", None)
    if asset:
        fields.pop("source", None)
    return fields


def close_points(actual, expected) -> bool:
    return len(actual) == len(expected) and all(
        math.isclose(a, e, rel_tol=0, abs_tol=TOLERANCE)
        for point_a, point_e in zip(actual, expected, strict=True)
        for a, e in zip(point_a, point_e, strict=True)
    )


def drawable_positions(session, parameter_id: str, mesh_id: str, value: float):
    frame = session.evaluate({parameter_id: value})
    return next(item.positions for item in frame.drawables if item.id == mesh_id)


def grade(baseline_path: Path, candidate_path: Path, setup_path: Path | None = None) -> dict:
    if not baseline_path.is_absolute() or not candidate_path.is_absolute():
        raise ValueError("Both manifest paths must be absolute")
    if baseline_path.resolve() == candidate_path.resolve():
        raise ValueError("Candidate must be saved separately from the baseline")
    baseline = kasane.open_project(baseline_path)
    candidate = kasane.open_project(candidate_path)
    checks: dict[str, dict] = {}

    def check(name: str, condition: bool, detail: str = "") -> None:
        checks[name] = {"status": "passed" if condition else "failed"}
        if not condition:
            checks[name]["detail"] = detail

    if setup_path is not None:
        setup = json.loads(setup_path.read_text(encoding="utf-8"))
        check("baseline_unchanged", project_files(baseline_path.parent) ==
              setup["baseline_files_sha256"], "Baseline project files changed")

    check("structure", not candidate.validate_structure(),
          "Candidate contains structural diagnostics")
    check("resources", not candidate.diagnose_resources(),
          "Candidate contains missing or invalid resources")
    check("document", baseline.document_id == candidate.document_id and
          baseline.canvas == candidate.canvas and
          baseline.draw_order_groups == candidate.draw_order_groups,
          "Document identity, canvas, or draw order changed")

    baseline_right = baseline.find_meshes_by_name("right")
    candidate_right = candidate.find_meshes_by_name("right")
    if len(baseline_right) != 1 or len(candidate_right) != 1:
        check("target_lookup", False, "Expected one mesh named right in each project")
        return {"status": "failed", "checks": checks}
    target_id = baseline_right[0].id
    check("target_lookup", candidate_right[0].id == target_id,
          "The target mesh ID changed")
    parameter_ids = baseline.parameter_ids()
    if len(parameter_ids) != 1:
        raise ValueError("Baseline fixture must have exactly one parameter")
    parameter_id = parameter_ids[0]

    baseline_binding = baseline.binding_for_mesh(target_id)
    candidate_binding = candidate.binding_for_mesh(target_id)
    if baseline_binding is None or candidate_binding is None:
        check("target_binding", False, "Target mesh binding is missing")
        return {"status": "failed", "checks": checks}
    check("binding_identity", baseline_binding.id == candidate_binding.id and
          baseline_binding.mesh_id == candidate_binding.mesh_id and
          baseline_binding.axes == candidate_binding.axes,
          "Binding identity or axes changed")
    baseline_forms = {tuple(form.keys): form for form in baseline_binding.keyforms}
    candidate_forms = {tuple(form.keys): form for form in candidate_binding.keyforms}
    check("keyform_keys", len(baseline_binding.keyforms) == 2 and
          len(candidate_binding.keyforms) == 2 and
          set(baseline_forms) == {(0.0,), (1.0,)} and
          set(candidate_forms) == set(baseline_forms),
          "Expected only the original 0 and 1 keyforms")
    if set(baseline_forms) == set(candidate_forms):
        base_zero, base_one = baseline_forms[(0.0,)], baseline_forms[(1.0,)]
        next_zero, next_one = candidate_forms[(0.0,)], candidate_forms[(1.0,)]
        check("zero_keyform", next_zero == base_zero,
              "The 0 keyform changed")
        expected_one = [(x + 0.1, y) for x, y in base_one.positions]
        check("one_keyform", close_points(next_one.positions, expected_one) and
              next_one.appearance == base_one.appearance and
              next_one.draw_order == base_one.draw_order,
              "The 1 keyform must move +0.1 in parent-local X only")

    for label, (id_method, item_method) in COLLECTIONS.items():
        original_ids = getattr(baseline, id_method)()
        final_ids = getattr(candidate, id_method)()
        check(f"{label}_ids", set(original_ids) == set(final_ids),
              f"{label} object IDs changed")
        if set(original_ids) != set(final_ids):
            continue
        unchanged = True
        for object_id in original_ids:
            if label == "bindings" and object_id == baseline_binding.id:
                continue
            original = getattr(baseline, item_method)(object_id)
            final = getattr(candidate, item_method)(object_id)
            if content(original, asset=label == "assets") != content(
                final, asset=label == "assets"
            ):
                unchanged = False
                break
        check(f"{label}_content", unchanged,
              f"An unrelated {label} object changed")

    if set(baseline.mesh_ids()) == set(candidate.mesh_ids()):
        zero_stable = close_points(
            drawable_positions(candidate, parameter_id, target_id, 0.0),
            drawable_positions(baseline, parameter_id, target_id, 0.0),
        )
        one_changed = not close_points(
            drawable_positions(candidate, parameter_id, target_id, 1.0),
            drawable_positions(baseline, parameter_id, target_id, 1.0),
        )
        other_stable = all(
            close_points(
                drawable_positions(candidate, parameter_id, mesh_id, value),
                drawable_positions(baseline, parameter_id, mesh_id, value),
            )
            for mesh_id in baseline.mesh_ids() if mesh_id != target_id
            for value in SAMPLES
        )
        check("evaluated_zero", zero_stable, "The target's initial pose changed")
        check("evaluated_one", one_changed, "The target's end pose did not move")
        check("other_mesh_samples", other_stable,
              "Another mesh changed at a sampled parameter value")

    return {
        "status": "passed" if all(
            item["status"] == "passed" for item in checks.values()
        ) else "failed",
        "baseline": str(baseline_path),
        "candidate": str(candidate_path),
        "checks": checks,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path, help="Baseline project manifest")
    parser.add_argument("candidate", type=Path, help="Saved candidate manifest")
    parser.add_argument("--setup", type=Path, help="Prepared setup.json with baseline hashes")
    parser.add_argument("--report", type=Path, help="Write a JSON grading report")
    args = parser.parse_args()
    try:
        result = grade(args.baseline, args.candidate, args.setup)
    except Exception as error:
        result = {"status": "error", "error": f"{type(error).__name__}: {error}"}
    data = json.dumps(result, indent=2, ensure_ascii=False, allow_nan=False) + "\n"
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(data, encoding="utf-8")
    print(data, end="")
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
