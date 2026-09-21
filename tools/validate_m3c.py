#!/usr/bin/env python3
"""Unified M3C acceptance gates, baseline scanner, and multi-version test runner.

Implements M3C stages S0 through S7 per docs/M3C-implementation-plan.md:
- S0: Baseline, sample inventory, Hiyori failure reproduction, Core/GPU oracle capability
- S1: Version 3 layout safety, zero-triangle meshes, Rice / Hiyori / Mark
- S2: Version 4 / MOC 4.2 sections & BlendShapes
- S3: Repeat parameters & Project format v4
- S4: BlendShape Glue
- S5: Version 6 / MOC 5.3 data, evaluation & export
- S6: Offscreen & extended blend modes rendering
- S7: Full version matrix & release closure
"""
import argparse
import hashlib
import json
import math
import os
import platform
import re
import acceptance_evidence as evidence
import struct
import subprocess
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional

ROOT = Path(__file__).resolve().parents[1]


def run_cmd(cmd: List[str], cwd: Path = ROOT, text_input: Optional[str] = None) -> str:
    res = subprocess.run(
        list(map(str, cmd)),
        cwd=cwd,
        input=text_input,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=300,
    )
    if res.returncode != 0:
        raise RuntimeError(
            f"Command failed ({res.returncode}): {' '.join(map(str, cmd))}\n"
            f"Stderr: {res.stderr}\nStdout: {res.stdout}"
        )
    return res.stdout


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(65536):
            h.update(chunk)
    return h.hexdigest()


def get_git_info() -> Dict[str, Any]:
    try:
        rev = run_cmd(["git", "rev-parse", "HEAD"]).strip()
        status = run_cmd(["git", "status", "--porcelain"]).strip()
        dirty = bool(status)
        submodules = run_cmd(["git", "submodule", "status"]).strip()
        return {"revision": rev, "dirty": dirty, "submodules": submodules}
    except Exception as e:
        return {"error": str(e)}


def ensure_probes(build_dir: Path, sdk_dir: Path):
    build_dir.mkdir(parents=True, exist_ok=True)
    official_probe = build_dir / "kasane_document_official_probe"
    purism_probe = build_dir / "kasane_document_purism_probe"
    if official_probe.is_file() and purism_probe.is_file():
        return official_probe, purism_probe

    print(f"Building Core probes in {build_dir}...")
    run_cmd([
        "cmake",
        "-S", ROOT / "tools/probes",
        "-B", build_dir,
        "-DCMAKE_BUILD_TYPE=Release",
        f"-DKASANE_CUBISM_ROOT={sdk_dir.resolve()}",
    ])
    run_cmd([
        "cmake",
        "--build", build_dir,
        "--target", "kasane_document_official_probe", "kasane_document_purism_probe",
        "-j4",
    ])
    return official_probe, purism_probe


def inspect_moc3_file(moc3_path: Path) -> Dict[str, Any]:
    raw = moc3_path.read_bytes()
    if len(raw) < 64:
        return {"error": "Buffer too small"}
    if raw[:4] != b"MOC3":
        return {"error": "Invalid magic"}
    version = raw[4]
    endian = raw[5]
    num_offsets = 480 if version >= 6 else 160
    header_size = 64 + num_offsets * 4
    if len(raw) < header_size:
        return {"error": "Header too small for offsets"}
    offsets = struct.unpack_from(f"<{num_offsets}I", raw, 64)
    counts_off = offsets[0]
    count_ints = 64 if version >= 5 else 32
    if counts_off + count_ints * 4 > len(raw):
        return {"error": "Counts section out of bounds"}
    counts_raw = struct.unpack_from(f"<{count_ints}i", raw, counts_off)

    count_names = [
        "parts", "deformers", "warps", "rotations", "art_meshes", "parameters",
        "part_keyforms", "warp_keyforms", "rotation_keyforms", "art_mesh_keyforms",
        "keyform_pos", "key_table_idx", "bindings", "key_tables", "keys", "uvs",
        "idx", "masks", "draw_groups", "draw_items", "glues", "glue_info",
        "glue_keyforms", "keyform_mul_colors", "keyform_scr_colors",
        "blend_key_tables", "blend_bindings", "bs_warps", "bs_art_meshes",
        "bs_constraint_idx", "bs_constraints", "bs_constraint_vals",
        "bs_parts", "bs_rotations", "bs_glues", "offscreens", "offscreen_keyforms", "bs_offscreens"
    ]
    counts = {}
    for i, val in enumerate(counts_raw):
        name = count_names[i] if i < len(count_names) else f"count_{i}"
        counts[name] = val

    canvas_off = offsets[1]
    ppu, ox, oy, w, h = struct.unpack_from("<5f", raw, canvas_off)
    flag = raw[canvas_off + 20]

    num_meshes = counts.get("art_meshes", 0)
    zero_tri_meshes = []
    if num_meshes > 0 and len(offsets) > 48:
        id_off = offsets[33]
        vcount_off = offsets[43]
        idx_len_off = offsets[46]
        mask_len_off = offsets[48]
        key_len_off = offsets[36]
        parent_part_off = offsets[39]
        parent_def_off = offsets[40]
        for mi in range(num_meshes):
            mesh_id = raw[id_off + mi * 64 : id_off + (mi + 1) * 64].split(b"\x00", 1)[0].decode("latin1", errors="replace")
            vcount = struct.unpack_from("<i", raw, vcount_off + mi * 4)[0]
            idx_len = struct.unpack_from("<i", raw, idx_len_off + mi * 4)[0]
            mask_len = struct.unpack_from("<i", raw, mask_len_off + mi * 4)[0]
            key_len = struct.unpack_from("<i", raw, key_len_off + mi * 4)[0]
            parent_part = struct.unpack_from("<i", raw, parent_part_off + mi * 4)[0]
            parent_def = struct.unpack_from("<i", raw, parent_def_off + mi * 4)[0]
            if idx_len == 0:
                zero_tri_meshes.append({
                    "mesh_index": mi,
                    "id": mesh_id,
                    "vertex_count": vcount,
                    "idx_len": idx_len,
                    "key_len": key_len,
                    "mask_len": mask_len,
                    "parent_part": parent_part,
                    "parent_deformer": parent_def,
                })

    num_params = counts.get("parameters", 0)
    repeat_params = []
    if num_params > 0 and len(offsets) > 54:
        param_id_off = offsets[50]
        repeat_off = offsets[54]
        for pi in range(num_params):
            pid = raw[param_id_off + pi * 64 : param_id_off + (pi + 1) * 64].split(b"\x00", 1)[0].decode("latin1", errors="replace")
            rep = struct.unpack_from("<i", raw, repeat_off + pi * 4)[0]
            if rep != 0:
                repeat_params.append({"param_index": pi, "id": pid, "repeat": rep})

    return {
        "version": version,
        "endian": endian,
        "counts": counts,
        "canvas": {"ppu": ppu, "origin_x": ox, "origin_y": oy, "width": w, "height": h, "flag": flag},
        "zero_triangle_meshes": zero_tri_meshes,
        "repeat_parameters": repeat_params,
    }


def scan_all_samples(sdk_res: Path) -> Dict[str, Any]:
    samples = {}
    candidate_dirs = [
        sdk_res,
        ROOT / "demos/gd-cubism-demo/assets/live2d/mao/runtime",
        ROOT / "benchmarks/cubism-matrix/assets/live2d/mao/runtime",
        ROOT / "modules/purism-core/testdata/moc3",
        ROOT / "tests/fixtures",
    ]

    all_moc3 = []
    for cdir in candidate_dirs:
        if cdir.is_dir():
            all_moc3.extend(cdir.rglob("*.moc3"))

    unique_by_hash = {}
    for moc_path in sorted(all_moc3):
        # ignore target/
        if "target/" in str(moc_path):
            continue
        h = sha256_file(moc_path)
        rel = str(moc_path.relative_to(ROOT))
        if h not in unique_by_hash:
            info = inspect_moc3_file(moc_path)
            model_dir = moc_path.parent
            model3_files = [str(p.relative_to(ROOT)) for p in model_dir.glob("*.model3.json")]
            textures = [str(p.relative_to(ROOT)) for p in model_dir.glob("**/*.png")]
            unique_by_hash[h] = {
                "sha256": h,
                "primary_path": rel,
                "all_paths": [rel],
                "file_size": moc_path.stat().st_size,
                "model3_files": model3_files,
                "textures": textures,
                "inspection": info,
            }
        else:
            unique_by_hash[h]["all_paths"].append(rel)

    return unique_by_hash


def build_s0_baseline_report(sdk_dir: Path, probe_dir: Path, output_dir: Path) -> Dict[str, Any]:
    sdk_res = sdk_dir / "Samples/Resources"
    unique_samples = scan_all_samples(sdk_res)

    official_probe, purism_probe = ensure_probes(probe_dir, sdk_dir)

    # 1. Hiyori detailed failure evidence
    hiyori_moc3 = sdk_res / "Hiyori/Hiyori.moc3"
    hiyori_info = inspect_moc3_file(hiyori_moc3) if hiyori_moc3.is_file() else {}
    
    # Hiyori glue connections to zero-triangle meshes
    glue_connections = [
        {"glue_index": 7, "mesh_a": 110, "mesh_b": 111, "zero_mesh_id": "ArtMesh116"},
        {"glue_index": 13, "mesh_a": 117, "mesh_b": 118, "zero_mesh_id": "ArtMesh123"},
        {"glue_index": 19, "mesh_a": 124, "mesh_b": 125, "zero_mesh_id": "ArtMesh130"},
        {"glue_index": 25, "mesh_a": 131, "mesh_b": 132, "zero_mesh_id": "ArtMesh137"},
    ]

    core_zero_acceptance = {"status": "not_run", "reason": "No zero-vertex Core probe executed by this baseline run"}
    capability_table = {"official_probe": {"path": str(official_probe), "sha256": sha256_file(official_probe)},
                        "purism_probe": {"path": str(purism_probe), "sha256": sha256_file(purism_probe)}}

    # Missing assets inventory
    missing_assets = {
        "version_4_real_model": {
            "required_for_stage": "S2",
            "status": "missing",
            "impact": "S2 implementation can proceed with synthetic fixtures, real external acceptance stays not_run"
        },
        "repeat_parameter_real_model": {
            "required_for_stage": "S3",
            "status": "missing",
            "impact": "S3 implementation uses dual-core tested synthetic fixtures"
        },
        "blendshape_glue_real_model": {
            "required_for_stage": "S4",
            "status": "missing",
            "impact": "S4 implementation uses dual-core tested synthetic fixtures"
        },
        "extended_blend_modes_complete_coverage": {
            "required_for_stage": "S6",
            "status": "partial",
            "note": "Ren covers 5.3 Offscreen with blend mode 0; remaining blend modes tested via synthetic fixtures"
        }
    }

    # Known rejection evidence
    known_rejections = {
        "version_3_inspector": {
            "cause": "inspector.rs accepts only version 2 and 5; version 3 rejected with UNSUPPORTED_VERSION",
            "status": "historical_unverified"
        },
        "zero_triangle_mesh_geometry": {
            "cause": "geometry.rs validate_render_mesh enforces non-empty indices; doc.create_mesh and frame validation fail with INVALID_LENGTH",
            "status": "historical_unverified"
        },
        "blendshape_glue_inspector": {
            "cause": "inspector.rs explicitly rejects bs_glues > 0",
            "status": "historical_unverified"
        },
        "repeat_parameter_inspector": {
            "cause": "inspector.rs explicitly rejects repeat != 0",
            "status": "historical_unverified"
        },
        "offscreen_inspector": {
            "cause": "inspector.rs explicitly rejects offscreens > 0",
            "status": "historical_unverified"
        }
    }

    report = {
        "milestone": "M3C",
        "stage": "S0",
        "status": "passed",
        "system": {
            "platform": platform.platform(),
            "machine": platform.machine(),
            "python": platform.python_version(),
            "git": get_git_info(),
        },
        "manifest": unique_samples,
        "hiyori_evidence": {
            "moc3_path": "third_party/CubismSdkForNative-5-r.5/Samples/Resources/Hiyori/Hiyori.moc3",
            "version": 3,
            "zero_triangle_meshes": hiyori_info.get("zero_triangle_meshes", []),
            "glue_connections": glue_connections,
            "core_zero_acceptance": core_zero_acceptance,
        },
        "core_gpu_capabilities": capability_table,
        "missing_assets": missing_assets,
        "known_rejections": known_rejections,
        "checks": [evidence.missing("historical_rejections", "historical", "Baseline failure reproduction must be run against its original revision"),
                   evidence.missing("core_zero_vertex_probe", "numerical", "Not executed in this run")],
    }

    output_dir.mkdir(parents=True, exist_ok=True)
    report_path = output_dir / "s0_baseline_report.json"
    manifest_path = output_dir / "baseline_manifest.json"
    evidence.finalize(report)
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    manifest_path.write_text(json.dumps(unique_samples, indent=2) + "\n")
    print(f"S0 Baseline report written to {report_path}")
    print(f"S0 Manifest written to {manifest_path}")
    return report


def build_s1_report(manifest_path: Path, output_dir: Path) -> Dict[str, Any]:
    print("=== Executing M3C Stage S1: Layout Safety, Zero-Triangle Meshes & Version 3 ===")

    # 1. Run Cargo Tests
    cargo_suites = [
        ("kasane-core", ["cargo", "test", "-p", "kasane-core", "--locked"]),
        ("kasane-godot", ["cargo", "test", "-p", "kasane-godot", "--locked"]),
        ("kasane-moc3", ["cargo", "test", "-p", "kasane-moc3", "--locked"]),
        ("kasane-project", ["cargo", "test", "-p", "kasane-project", "--locked"]),
    ]
    test_results = {}
    for suite_name, cmd in cargo_suites:
        print(f"Running {suite_name} tests...")
        try:
            test_results[suite_name] = test_evidence(cmd, output_dir / "s1", suite_name)
        except Exception as e:
            test_results[suite_name] = {"passed": False, "error": str(e)}
            raise RuntimeError(f"Cargo test suite {suite_name} failed: {e}")

    # 2. Verify Version 3 models from baseline manifest
    hiyori_moc3 = ROOT / "third_party/CubismSdkForNative-5-r.5/Samples/Resources/Hiyori/Hiyori.moc3"
    rice_moc3 = ROOT / "third_party/CubismSdkForNative-5-r.5/Samples/Resources/Rice/Rice.moc3"
    mark_moc3 = ROOT / "third_party/CubismSdkForNative-5-r.5/Samples/Resources/Mark/Mark.moc3"

    v3_models = {}
    for name, path in [("Hiyori", hiyori_moc3), ("Rice", rice_moc3), ("Mark", mark_moc3)]:
        if not path.is_file():
            raise RuntimeError(f"Required version 3 model {name} not found at {path}")
        info = inspect_moc3_file(path)
        v3_models[name] = {
            "path": str(path.relative_to(ROOT)),
            "sha256": sha256_file(path),
            "version": info.get("version"),
            "art_meshes": info.get("counts", {}).get("art_meshes"),
            "glues": info.get("counts", {}).get("glues"),
            "zero_triangle_meshes": info.get("zero_triangle_meshes", []),
        }

    zero_tri_meshes_hiyori = v3_models["Hiyori"]["zero_triangle_meshes"]
    assert len(zero_tri_meshes_hiyori) == 4, f"Expected 4 zero-triangle meshes in Hiyori, found {len(zero_tri_meshes_hiyori)}"

    # 3. Gate verification
    checks = stage_checks("S1", test_results, output_dir)

    report = {
        "milestone": "M3C",
        "stage": "S1",
        "status": "passed",
        "system": {
            "platform": platform.platform(),
            "machine": platform.machine(),
            "python": platform.python_version(),
            "git": get_git_info(),
        },
        "cargo_tests": test_results,
        "version_3_models": v3_models,
        "zero_triangle_handling": {
            "hiyori_zero_triangle_meshes": zero_tri_meshes_hiyori,
            "core_acceptance": "Core accepts vertex_count >= 0 with idx_len == 0",
            "kasane_handling": "geometry.rs decoupled validity from drawability; mesh_view updates bounds without surface",
            "serialization": "encoder.rs writes idx_len=0 and keeps mesh & glue topology intact",
        },
        "checks": checks,
    }

    output_dir.mkdir(parents=True, exist_ok=True)
    evidence.finalize(report)
    report_path = output_dir / "s1_report.json"
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    print(f"S1 Report written to {report_path}")
    return report


def finalize_required_gates(report):
    # Compatibility for callers constructing the legacy gate shape.
    if "checks" in report:
        return evidence.finalize(report)
    checks = [value for name, value in report["gate"].items() if name != "passed"]
    passed = bool(checks) and all(value is True or value == "passed" for value in checks)
    report["gate"]["passed"] = passed
    report["status"] = ("passed" if passed else
                        "failed" if any(value is False or value == "failed" for value in checks)
                        else "not_run")
    return report


def test_evidence(cmd, output_dir, suite):
    output_dir.mkdir(parents=True, exist_ok=True)
    output = run_cmd(cmd)
    log = output_dir / (suite + ".log")
    log.write_text(output)
    count = sum(int(n) for n in re.findall(r"test result: ok\. (\d+) passed", output))
    if count == 0:
        raise RuntimeError(f"{suite}: no passing tests were executed")
    return {"passed": True, "tests_passed": count, "command": cmd,
            "evidence": [evidence.artifact(log, "test_log")]}


def stage_checks(stage, tests, output_dir, probe=None):
    checks = [evidence.check("rust_" + name, "constructed_tests", "passed" if result["passed"] else "failed", result["evidence"])
              for name, result in tests.items()]
    if probe:
        model, source_kind, probe_input, p_json, o_json, purism, official = probe
        path = output_dir / (stage.lower() + "-numerical-evidence.json")
        path.write_text(json.dumps({"input": probe_input, "purism": p_json, "official": o_json}, indent=2))
        checks.append(evidence.check("dual_core_numerical", source_kind, "passed", [
            evidence.artifact(path, "numerical_comparison"), evidence.artifact(model, source_kind),
            evidence.artifact(purism, "reference_binary"), evidence.artifact(official, "reference_binary")]))
    if stage in ("S2", "S3", "S4"):
        checks.append(evidence.missing("real_asset_acceptance", "real_model", "Only constructed fixtures were run; external feature asset acceptance is missing"))
    if stage in ("S1", "S2"):
        checks.append(evidence.missing("real_model_editor_gpu", "gpu", "This runner has not executed the required real-model Editor/GPU workflow"))
    return checks


def execute_stage(stage, builder, output_dir, *args):
    output_dir.mkdir(parents=True, exist_ok=True)
    path = output_dir / ("s0_baseline_report.json" if stage == "S0" else stage.lower() + "_report.json")
    report = {"stage": stage, "status": "not_run", "checks": [], "gate": {"passed": False}}
    path.write_text(json.dumps(report, indent=2))  # Invalidate stale success before any command.
    try:
        before = evidence.source_identity(ROOT)
        report = builder(*args)
        report['provenance'] = before
        if evidence.source_identity(ROOT) != before:
            report.setdefault('checks', []).append(evidence.check('stable_source', 'provenance', 'failed', reason='Source changed during run'))
        evidence.finalize(report)
    except Exception as exc:
        report.update(status='failed', gate={'passed': False}, error=str(exc))
    path.write_text(json.dumps(report, indent=2) + "\n")
    return report


def build_s2_report(sdk_dir: Path, probe_dir: Path, output_dir: Path) -> Dict[str, Any]:
    print("=== Executing M3C Stage S2: Version 4 / MOC 4.2 Compatibility ===")

    # 1. Ensure external v42 fixture is built
    fixture_script = ROOT / "tools/create_v42_external_fixture.py"
    if not fixture_script.is_file():
        raise RuntimeError(f"Fixture generator not found: {fixture_script}")
    run_cmd(["python3", str(fixture_script)])

    v42_moc3 = ROOT / "tests/fixtures/external_v42/model.moc3"
    assert v42_moc3.is_file(), f"V42 fixture not found at {v42_moc3}"

    # 2. Dual-core probe verification on V42 fixture across sample values
    official_probe, purism_probe = ensure_probes(probe_dir, sdk_dir)
    probe_input = "3 0.0 0.0 0.0 0.5 -0.5 1.0 -0.5 0.5 -1.0"

    p_proc = subprocess.run([str(purism_probe), str(v42_moc3)], input=probe_input, capture_output=True, text=True, check=True)
    o_proc = subprocess.run([str(official_probe), str(v42_moc3)], input=probe_input, capture_output=True, text=True, check=True)

    p_json = json.loads([l for l in p_proc.stdout.splitlines() if l.startswith("{")][0])
    o_json = json.loads([l for l in o_proc.stdout.splitlines() if l.startswith("{")][0])

    assert len(p_json["samples"]) == len(o_json["samples"]) == 3
    max_pos_err = 0.0
    for s_idx in range(3):
        p_samp = p_json["samples"][s_idx]
        o_samp = o_json["samples"][s_idx]
        assert len(p_samp) == len(o_samp)
        for d_idx in range(len(p_samp)):
            pd = p_samp[d_idx]
            od = o_samp[d_idx]
            assert pd["runtime_id"] == od["runtime_id"]
            assert abs(pd["opacity"] - od["opacity"]) < 1e-4
            assert pd["draw_order"] == od["draw_order"]
            for c in range(4):
                assert abs(pd["multiply_color"][c] - od["multiply_color"][c]) < 1e-4
                assert abs(pd["screen_color"][c] - od["screen_color"][c]) < 1e-4
            for p1, p2 in zip(pd["positions"], od["positions"]):
                dx = abs(p1[0] - p2[0])
                dy = abs(p1[1] - p2[1])
                max_pos_err = max(max_pos_err, dx, dy)
    assert max_pos_err < 1e-3, f"Dual core probe pos err too large: {max_pos_err}"

    # 3. Cargo tests
    cargo_suites = [
        ("kasane-core", ["cargo", "test", "-p", "kasane-core", "--locked"]),
        ("kasane-godot", ["cargo", "test", "-p", "kasane-godot", "--locked"]),
        ("kasane-moc3", ["cargo", "test", "-p", "kasane-moc3", "--locked"]),
        ("kasane-project", ["cargo", "test", "-p", "kasane-project", "--locked"]),
    ]
    test_results = {}
    for suite_name, cmd in cargo_suites:
        print(f"Running {suite_name} tests...")
        test_results[suite_name] = test_evidence(cmd, output_dir / "s2", suite_name)

    # 4. Field mapping specification for PSM__SECTIONS_V42 (sections 0..136)
    field_mapping_report = {
        "format_version": 4,
        "section_count": 137,
        "count_info_ints": 32,
        "sections": {
            "0..1": {
                "category": "header_and_canvas",
                "sections": ["0: count_info (32 ints)", "1: canvas_info (ppu, origin, width, height, flag)"],
                "document_mapping": "Document.canvas",
                "evaluator": "Sets ppu and canvas origin coordinate transformation",
                "encoder_50": "Written directly to section 0 (64 ints in v5) and section 1",
            },
            "2..9": {
                "category": "parts",
                "sections": ["2: id_runtime", "3: id", "4: binding_idx", "5: keyform_off", "6: key_len", "7: visible", "8: enable", "9: parent_part_idx"],
                "document_mapping": "Document.parts / Part hierarchy",
                "evaluator": "Part opacity and hierarchy inheritance",
                "encoder_50": "Written to sections 2..9",
            },
            "10..28": {
                "category": "deformers",
                "sections": ["10..18: deformer_src", "19..24: warp_src", "25..28: rotation_src", "101: quad_transform"],
                "document_mapping": "Document.transforms (Warp and Rotation)",
                "evaluator": "Warp grid bicubic/bilinear deformation; Rotation polar matrix transform",
                "encoder_50": "Written to sections 10..28, 101",
            },
            "29..48": {
                "category": "art_meshes",
                "sections": ["29..33: id/runtime", "34..42: binding/parent/blend_mode", "43..48: vertex_count/uv/idx/mask"],
                "document_mapping": "Document.meshes (vertex_ids, uvs, triangles, masks, blend_mode, appearance)",
                "evaluator": "Binds to parent deformers, evaluates deformed vertex positions and draw order",
                "encoder_50": "Written to sections 29..48",
            },
            "49..57": {
                "category": "parameters_base",
                "sections": ["49..50: id/runtime", "51..55: max/min/default/repeat/decimal_places", "56..57: key_table_off/len"],
                "document_mapping": "Document.parameters",
                "evaluator": "Clamps value to [min, max], looks up normalized key values",
                "encoder_50": "Written to sections 49..57",
            },
            "58..77": {
                "category": "keyforms_and_geometry_pools",
                "sections": ["58: part_key_src", "59..60: warp_key_src", "61..67: rotation_key_src", "68..70: art_mesh_key_src", "71: key_pos_src.xy", "72..77: key_tables and keys"],
                "document_mapping": "MeshBinding, Transform bindings, Keyforms (positions, opacities)",
                "evaluator": "Multi-dimensional keyform interpolation",
                "encoder_50": "Written to sections 58..77",
            },
            "78..88": {
                "category": "mesh_data_and_draw_hierarchy",
                "sections": ["78: uv_src.xy", "79: idx_src.idx", "80: mask_src", "81..85: draw_group_src", "86..88: draw_group_obj_src"],
                "document_mapping": "Mesh.uvs, Mesh.triangles, DrawOrderGroups",
                "evaluator": "UV mapping, triangle rasterization indices, draw order tie-breaking",
                "encoder_50": "Written to sections 78..88",
            },
            "105..113": {
                "category": "normal_colors",
                "sections": [
                    "105: warp_src.key_color_off",
                    "106: rotation_src.key_color_off",
                    "107: art_mesh_src.key_color_off",
                    "108..110: keyform_mul_color_src (r, g, b)",
                    "111..113: keyform_scr_color_src (r, g, b)"
                ],
                "document_mapping": "Appearance.multiply [r, g, b], Appearance.screen [r, g, b] on Keyforms and base objects",
                "evaluator": "child.multiply * parent.multiply; child.screen + parent.screen - child.screen * parent.screen",
                "encoder_50": "Exported to 5.0 sections 105..113 with color pools",
            },
            "114..116": {
                "category": "parameter_extensions",
                "sections": ["114: param_src.type (0=normal, 1=blendshape)", "115..116: blend_key_table_off/len"],
                "document_mapping": "Parameter.kind (Normal vs BlendShape), Parameter.id mapping",
                "evaluator": "BlendShape parameters drive BlendShapeKeyTable and constraints",
                "encoder_50": "Written to sections 114..116",
            },
            "117..124": {
                "category": "blend_key_tables_and_bindings",
                "sections": ["117..119: blend_key_table_src (keys_off, keys_len, base_key_idx)", "120..124: blend_binding_src (key_table_idx, key_bs_off, key_bs_len, bs_constraint_idx_off, bs_constraint_idx_len)"],
                "document_mapping": "Document.blend_key_tables, Document.blend_bindings",
                "evaluator": "Computes delta weight relative to base_key_idx keyform; evaluates shared constraints",
                "encoder_50": "Written to sections 117..124",
            },
            "125..130": {
                "category": "blendshape_targets_warp_and_mesh",
                "sections": ["125..127: bs_warp_src (target_idx, bs_binding_off, bs_binding_len)", "128..130: bs_art_mesh_src (target_idx, bs_binding_off, bs_binding_len)"],
                "document_mapping": "BlendShapeBinding (Target: Warp / Mesh, DeltaKeyforms)",
                "evaluator": "Additive delta position / opacity offset applied to base keyform before parent transform",
                "encoder_50": "Written to sections 125..130; missing V50 delta colors (137..142) written as neutral -1",
            },
            "131..136": {
                "category": "blendshape_constraints",
                "sections": ["131: blend_constraint_idx_src", "132..134: blend_constraint_src (parameter_idx, value_off, value_len)", "135..136: blend_constraint_val_src (key, weight)"],
                "document_mapping": "Document.blend_constraints (keys, weights, parameter_id)",
                "evaluator": "Constraint piecewise linear interpolation scaling delta blend weight",
                "encoder_50": "Written to sections 131..136",
            },
        },
    }

    checks = stage_checks("S2", test_results, output_dir, (v42_moc3, "constructed_model", probe_input, p_json, o_json, purism_probe, official_probe))

    report = {
        "milestone": "M3C",
        "stage": "S2",
        "status": "passed",
        "system": {
            "platform": platform.platform(),
            "machine": platform.machine(),
            "python": platform.python_version(),
            "git": get_git_info(),
        },
        "cargo_tests": test_results,
        "fixture_dual_core_verification": {
            "fixture": "tests/fixtures/external_v42/model.moc3",
            "samples_tested": 3,
            "max_position_error": max_pos_err,
            "official_core_status": "passed",
            "purism_core_status": "passed",
        },
        "field_mapping": field_mapping_report,
        "checks": checks,
    }

    output_dir.mkdir(parents=True, exist_ok=True)
    evidence.finalize(report)
    report_path = output_dir / "s2_report.json"
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    print(f"S2 Report written to {report_path}")
    return report


def build_s3_report(sdk_dir: Path, probe_dir: Path, output_dir: Path) -> Dict[str, Any]:
    print("=== Building Stage S3 Report: Cyclic Parameters & Project Format v4 ===")
    official_probe, purism_probe = ensure_probes(probe_dir, sdk_dir)

    # 1. Ensure external cyclic fixture exists
    cyclic_moc3 = ROOT / "tests/fixtures/external_cyclic/model.moc3"
    if not cyclic_moc3.is_file():
        print("Generating external cyclic parameter fixture...")
        run_cmd(["python3", ROOT / "tools/create_cyclic_fixture.py"])

    # 2. Probe verification on cyclic fixture
    probe_input = "6\n0.5 0.0 0.0\n2.5 0.0 2.0\n-1.5 0.0 -2.0\n1.0 0.0 0.0\n4.5 0.0 0.0\n-3.5 0.0 0.0\n"
    p_proc = subprocess.run([str(purism_probe), str(cyclic_moc3)], input=probe_input, capture_output=True, text=True, check=True)
    o_proc = subprocess.run([str(official_probe), str(cyclic_moc3)], input=probe_input, capture_output=True, text=True, check=True)

    p_json = json.loads([l for l in p_proc.stdout.splitlines() if l.startswith("{")][0])
    o_json = json.loads([l for l in o_proc.stdout.splitlines() if l.startswith("{")][0])

    assert len(p_json["samples"]) == len(o_json["samples"]) == 6
    max_pos_err = 0.0
    for s_idx in range(6):
        p_samp = p_json["samples"][s_idx]
        o_samp = o_json["samples"][s_idx]
        assert len(p_samp) == len(o_samp)
        for d_idx in range(len(p_samp)):
            pd = p_samp[d_idx]
            od = o_samp[d_idx]
            assert pd["runtime_id"] == od["runtime_id"]
            assert abs(pd["opacity"] - od["opacity"]) < 1e-4
            assert pd["draw_order"] == od["draw_order"]
            for p1, p2 in zip(pd["positions"], od["positions"]):
                dx = abs(p1[0] - p2[0])
                dy = abs(p1[1] - p2[1])
                max_pos_err = max(max_pos_err, dx, dy)
    assert max_pos_err < 1e-3, f"Dual core probe pos err too large: {max_pos_err}"

    # Verify periodicity: samples 0, 1, 2, 4, 5 yield identical positions
    periodicity_err = 0.0
    for s in [1, 2, 4, 5]:
        for p1, p2 in zip(p_json["samples"][0][0]["positions"], p_json["samples"][s][0]["positions"]):
            periodicity_err = max(periodicity_err, abs(p1[0] - p2[0]), abs(p1[1] - p2[1]))
    assert periodicity_err < 1e-4, f"Periodicity mismatch: {periodicity_err}"

    # 3. Cargo tests
    cargo_suites = [
        ("kasane-core", ["cargo", "test", "-p", "kasane-core", "--locked"]),
        ("kasane-godot", ["cargo", "test", "-p", "kasane-godot", "--locked"]),
        ("kasane-moc3", ["cargo", "test", "-p", "kasane-moc3", "--locked"]),
        ("kasane-project", ["cargo", "test", "-p", "kasane-project", "--locked"]),
    ]
    test_results = {}
    for suite_name, cmd in cargo_suites:
        print(f"Running {suite_name} tests...")
        test_results[suite_name] = test_evidence(cmd, output_dir / "s3", suite_name)

    checks = stage_checks("S3", test_results, output_dir, (cyclic_moc3, "constructed_model", probe_input, p_json, o_json, purism_probe, official_probe))

    report = {
        "milestone": "M3C",
        "stage": "S3",
        "status": "passed",
        "system": {
            "platform": platform.platform(),
            "machine": platform.machine(),
            "python": platform.python_version(),
            "git": get_git_info(),
        },
        "cargo_tests": test_results,
        "cyclic_dual_core_verification": {
            "fixture": "tests/fixtures/external_cyclic/model.moc3",
            "samples_tested": 6,
            "max_dual_core_position_error": max_pos_err,
            "periodicity_max_error": periodicity_err,
            "official_core_status": "passed",
            "purism_core_status": "passed",
        },
        "checks": checks,
    }

    output_dir.mkdir(parents=True, exist_ok=True)
    evidence.finalize(report)
    report_path = output_dir / "s3_report.json"
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    print(f"S3 Report written to {report_path}")
    return report


def build_s4_report(sdk_dir: Path, probe_dir: Path, output_dir: Path) -> Dict[str, Any]:
    print("=== Building Stage S4 Report: BlendShape Glue & Version 5 Parity ===")
    official_probe, purism_probe = ensure_probes(probe_dir, sdk_dir)

    # 1. Ensure external v50 bs glue fixture exists
    bs_glue_moc3 = ROOT / "tests/fixtures/external_v50_bs_glue/model.moc3"
    if not bs_glue_moc3.is_file():
        print("Generating external BlendShape Glue fixture via cargo run...")
        run_cmd(["cargo", "run", "-p", "kasane-moc3", "--example", "export_v50_bs_glue"])

    assert bs_glue_moc3.is_file(), f"BS Glue fixture not found at {bs_glue_moc3}"

    # 2. Inspect fixture
    info = inspect_moc3_file(bs_glue_moc3)
    assert info.get("version") == 5, f"Expected MOC3 version 5, got {info.get('version')}"
    counts = info.get("counts", {})
    assert counts.get("bs_glues") == 1, f"Expected 1 bs_glue, got {counts.get('bs_glues')}"
    assert counts.get("glues") == 1, f"Expected 1 glue, got {counts.get('glues')}"

    # 3. Dual-core probe verification on BS Glue fixture
    probe_input = "3\n0.0 0.0\n0.0 0.5\n0.5 1.0\n"
    p_proc = subprocess.run([str(purism_probe), str(bs_glue_moc3)], input=probe_input, capture_output=True, text=True, check=True)
    o_proc = subprocess.run([str(official_probe), str(bs_glue_moc3)], input=probe_input, capture_output=True, text=True, check=True)

    p_json = json.loads([l for l in p_proc.stdout.splitlines() if l.startswith("{")][0])
    o_json = json.loads([l for l in o_proc.stdout.splitlines() if l.startswith("{")][0])

    assert len(p_json["samples"]) == len(o_json["samples"]) == 3
    max_pos_err = 0.0
    for s_idx in range(3):
        p_samp = p_json["samples"][s_idx]
        o_samp = o_json["samples"][s_idx]
        assert len(p_samp) == len(o_samp)
        for d_idx in range(len(p_samp)):
            pd = p_samp[d_idx]
            od = o_samp[d_idx]
            assert pd["runtime_id"] == od["runtime_id"]
            assert abs(pd["opacity"] - od["opacity"]) < 1e-4
            assert pd["draw_order"] == od["draw_order"]
            for c in range(4):
                assert abs(pd["multiply_color"][c] - od["multiply_color"][c]) < 1e-4
                assert abs(pd["screen_color"][c] - od["screen_color"][c]) < 1e-4
            for p1, p2 in zip(pd["positions"], od["positions"]):
                dx = abs(p1[0] - p2[0])
                dy = abs(p1[1] - p2[1])
                max_pos_err = max(max_pos_err, dx, dy)
    assert max_pos_err < 1e-4, f"Dual core probe pos err too large: {max_pos_err}"

    # 4. Cargo tests
    cargo_suites = [
        ("kasane-core", ["cargo", "test", "-p", "kasane-core", "--locked"]),
        ("kasane-godot", ["cargo", "test", "-p", "kasane-godot", "--locked"]),
        ("kasane-moc3", ["cargo", "test", "-p", "kasane-moc3", "--locked"]),
        ("kasane-project", ["cargo", "test", "-p", "kasane-project", "--locked"]),
    ]
    test_results = {}
    for suite_name, cmd in cargo_suites:
        print(f"Running {suite_name} tests...")
        test_results[suite_name] = test_evidence(cmd, output_dir / "s4", suite_name)

    checks = stage_checks("S4", test_results, output_dir, (bs_glue_moc3, "constructed_model", probe_input, p_json, o_json, purism_probe, official_probe))

    report = {
        "milestone": "M3C",
        "stage": "S4",
        "status": "passed",
        "system": {
            "platform": platform.platform(),
            "machine": platform.machine(),
            "python": platform.python_version(),
            "git": get_git_info(),
        },
        "cargo_tests": test_results,
        "bs_glue_dual_core_verification": {
            "fixture": "tests/fixtures/external_v50_bs_glue/model.moc3",
            "samples_tested": 3,
            "max_dual_core_position_error": max_pos_err,
            "official_core_status": "passed",
            "purism_core_status": "passed",
        },
        "checks": checks,
    }

    output_dir.mkdir(parents=True, exist_ok=True)
    evidence.finalize(report)
    report_path = output_dir / "s4_report.json"
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    print(f"S4 Report written to {report_path}")
    return report


def build_s5_report(sdk_dir: Path, probe_dir: Path, output_dir: Path) -> Dict[str, Any]:
    print("=== Building Stage S5 Report: version 6 Data, Evaluation, and 5.3 Export ===")
    official_probe, purism_probe = ensure_probes(probe_dir, sdk_dir)

    ren_moc3 = sdk_dir / "Samples/Resources/Ren/Ren.moc3"
    assert ren_moc3.is_file(), f"Ren.moc3 not found at {ren_moc3}"

    # 1. Inspect Ren.moc3
    info = inspect_moc3_file(ren_moc3)
    assert info.get("version") == 6, f"Expected MOC3 version 6, got {info.get('version')}"
    counts = info.get("counts", {})
    assert counts.get("offscreens") == 24, f"Expected 24 offscreens, got {counts.get('offscreens')}"
    assert counts.get("parts") == 51, f"Expected 51 parts, got {counts.get('parts')}"
    assert counts.get("art_meshes") == 198, f"Expected 198 art meshes, got {counts.get('art_meshes')}"

    # 2. Dual-core probe verification on Ren.moc3
    param_count = counts.get("parameters", 73)
    sample_values = [
        [0.0] * param_count,
        [0.5] * param_count,
        [-0.5] * param_count,
    ]
    probe_input = f"{len(sample_values)}\n" + "\n".join(" ".join(str(v) for v in s) for s in sample_values) + "\n"

    p_proc = subprocess.run([str(purism_probe), str(ren_moc3)], input=probe_input, capture_output=True, text=True, check=True)
    o_proc = subprocess.run([str(official_probe), str(ren_moc3)], input=probe_input, capture_output=True, text=True, check=True)

    p_json = json.loads([l for l in p_proc.stdout.splitlines() if l.startswith("{")][0])
    o_json = json.loads([l for l in o_proc.stdout.splitlines() if l.startswith("{")][0])

    assert p_json.get("offscreen_count") == o_json.get("offscreen_count") == 24
    assert p_json.get("part_offscreen_indices") == o_json.get("part_offscreen_indices")

    max_pos_err = 0.0
    max_op_err = 0.0
    for s_idx in range(len(sample_values)):
        # Offscreens verification
        p_os = p_json["offscreen_samples"][s_idx]
        o_os = o_json["offscreen_samples"][s_idx]
        assert len(p_os) == len(o_os) == 24
        for i in range(24):
            po = p_os[i]
            oo = o_os[i]
            assert po["owner_index"] == oo["owner_index"]
            assert po["blend_mode"] == oo["blend_mode"]
            assert po["flags"] == oo["flags"]
            assert po["mask_indices"] == oo["mask_indices"]
            op_err = abs(po["opacity"] - oo["opacity"])
            max_op_err = max(max_op_err, op_err)
            assert op_err < 1e-4, f"Offscreen {i} opacity mismatch: {po['opacity']} vs {oo['opacity']}"

        # Drawables verification
        p_samp = p_json["samples"][s_idx]
        o_samp = o_json["samples"][s_idx]
        assert len(p_samp) == len(o_samp) == 198
        for d_idx in range(len(p_samp)):
            pd = p_samp[d_idx]
            od = o_samp[d_idx]
            assert pd["runtime_id"] == od["runtime_id"]
            assert abs(pd["opacity"] - od["opacity"]) < 1e-4
            assert pd["draw_order"] == od["draw_order"]
            for c in range(4):
                assert abs(pd["multiply_color"][c] - od["multiply_color"][c]) < 1e-4
                assert abs(pd["screen_color"][c] - od["screen_color"][c]) < 1e-4
            for p1, p2 in zip(pd["positions"], od["positions"]):
                dx = abs(p1[0] - p2[0])
                dy = abs(p1[1] - p2[1])
                max_pos_err = max(max_pos_err, dx, dy)
    assert max_pos_err < 1e-4, f"Dual core probe pos err too large: {max_pos_err}"

    # 3. Cargo tests
    cargo_suites = [
        ("kasane-core", ["cargo", "test", "-p", "kasane-core", "--locked"]),
        ("kasane-godot", ["cargo", "test", "-p", "kasane-godot", "--locked"]),
        ("kasane-moc3", ["cargo", "test", "-p", "kasane-moc3", "--locked"]),
        ("kasane-project", ["cargo", "test", "-p", "kasane-project", "--locked"]),
    ]
    test_results = {}
    for suite_name, cmd in cargo_suites:
        print(f"Running {suite_name} tests...")
        test_results[suite_name] = test_evidence(cmd, output_dir / "s5", suite_name)

    checks = stage_checks("S5", test_results, output_dir, (ren_moc3, "real_model", probe_input, p_json, o_json, purism_probe, official_probe))

    report = {
        "milestone": "M3C",
        "stage": "S5",
        "status": "passed",
        "system": {
            "platform": platform.platform(),
            "machine": platform.machine(),
            "python": platform.python_version(),
            "git": get_git_info(),
        },
        "cargo_tests": test_results,
        "ren_v6_dual_core_verification": {
            "fixture": "third_party/CubismSdkForNative-5-r.5/Samples/Resources/Ren/Ren.moc3",
            "samples_tested": len(sample_values),
            "offscreen_count": 24,
            "max_dual_core_position_error": max_pos_err,
            "max_dual_core_offscreen_opacity_error": max_op_err,
            "official_core_status": "passed",
            "purism_core_status": "passed",
        },
        "checks": checks,
    }

    output_dir.mkdir(parents=True, exist_ok=True)
    evidence.finalize(report)
    report_path = output_dir / "s5_report.json"
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    print(f"S5 Report written to {report_path}")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=ROOT / "target/kasane/m3c/baseline_manifest.json")
    parser.add_argument("--stage", type=str, default="S0", choices=["S0", "S1", "S2", "S3", "S4", "S5", "S6", "S7", "all"])
    parser.add_argument("--output", type=Path, default=ROOT / "target/kasane/m3c")
    parser.add_argument("--sdk", type=Path, default=ROOT / "third_party/CubismSdkForNative-5-r.5")
    parser.add_argument("--probe-dir", type=Path, default=ROOT / "target/probes")
    args = parser.parse_args()
    if not __debug__:
        parser.error("Acceptance comparisons require Python assertions; do not run with -O")

    args.output.mkdir(parents=True, exist_ok=True)
    incomplete_stages = []

    if args.stage in ("S0", "all"):
        print("=== Executing M3C Stage S0: Baseline, Manifest & Oracle Setup ===")
        s0_report = execute_stage("S0", build_s0_baseline_report, args.output, args.sdk, args.probe_dir, args.output)
        if not s0_report["gate"]["passed"]:
            incomplete_stages.append("S0")
        if args.stage == "S0":
            return 0 if s0_report["gate"]["passed"] else 1

    if args.stage in ("S1", "all"):
        s1_report = execute_stage("S1", build_s1_report, args.output, args.manifest, args.output)
        if not s1_report["gate"]["passed"]:
            incomplete_stages.append("S1")
        if args.stage == "S1":
            return 0 if s1_report["gate"]["passed"] else 1

    if args.stage in ("S2", "all"):
        s2_report = execute_stage("S2", build_s2_report, args.output, args.sdk, args.probe_dir, args.output)
        if not s2_report["gate"]["passed"]:
            incomplete_stages.append("S2")
        if args.stage == "S2":
            return 0 if s2_report["gate"]["passed"] else 1

    if args.stage in ("S3", "all"):
        s3_report = execute_stage("S3", build_s3_report, args.output, args.sdk, args.probe_dir, args.output)
        if not s3_report["gate"]["passed"]:
            incomplete_stages.append("S3")
        if args.stage == "S3":
            return 0 if s3_report["gate"]["passed"] else 1

    if args.stage in ("S4", "all"):
        s4_report = execute_stage("S4", build_s4_report, args.output, args.sdk, args.probe_dir, args.output)
        if not s4_report["gate"]["passed"]:
            incomplete_stages.append("S4")
        if args.stage == "S4":
            return 0 if s4_report["gate"]["passed"] else 1

    if args.stage in ("S5", "all"):
        s5_report = execute_stage("S5", build_s5_report, args.output, args.sdk, args.probe_dir, args.output)
        if not s5_report["gate"]["passed"]:
            incomplete_stages.append("S5")
        if args.stage == "S5":
            return 0 if s5_report["gate"]["passed"] else 1

    if args.stage in ("S6", "all"):
        from validate_s6 import acceptance, GODOT
        report_path = args.output / "s6_report.json"
        report_path.write_text(json.dumps({"stage": "S6", "status": "not_run"}) + "\n")
        report = acceptance((args.output / "s6").resolve(), args.sdk.resolve(), GODOT)
        report_path.write_text(json.dumps(report, indent=2) + "\n")
        if report["status"] != "passed" or args.stage == "S6":
            print(f"S6 {report['status']}: {report_path}")
            return 0 if report["status"] == "passed" else 1

    # Release closure remains a separate S7 gate.
    stages = ["S7"] if args.stage == "all" else [args.stage]
    for st in stages:
        stage_report_file = args.output / f"{st.lower()}_report.json"
        # Check if already completed by a dedicated run
        if stage_report_file.is_file():
            data = json.loads(stage_report_file.read_text())
            if not evidence.reusable(data, ROOT):
                incomplete_stages.append(st)
        else:
            incomplete_stages.append(st)
            stage_report_file.write_text(json.dumps({
                "stage": st,
                "status": "not_run",
                "reason": f"Stage {st} implementation has not yet been executed"
            }, indent=2) + "\n")

    if incomplete_stages:
        print(f"Stages not complete: {', '.join(incomplete_stages)}")
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
