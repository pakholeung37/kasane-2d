#!/usr/bin/env python3
"""M3B numerical and complete application acceptance gates.

Validates:
1. Mao 5.0 MOC3 full import:
   - Normal bindings
   - 33 BlendShape parameters, 124 BlendBindings, 33 BlendKeyTables, 7 Constraints
   - 7 Glues (161 vertex pairs)
   - 260 ArtMeshes, 175 Deformers, 31 Parts
2. Lossless Document and Project v3 persistence:
   - Saving .kasane (Project v3)
   - Complete detachment & reopening
3. 5.0 MOC3 re-export:
   - Preserves all Glue and BlendShape sections
   - Passes PurismCore csmHasMocConsistency == 1
4. Dual-core numerical parity:
   - Evaluated across Official Live2D Core and PurismCore
   - source_document, source_export, document_export comparisons
   - Strict < 0.05 px max position error and 1e-5 float tolerance
"""
import argparse
import hashlib
import json
import math
import platform
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def run(cmd, cwd=ROOT, text_input=None):
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


def ensure_probes(build_dir: Path, sdk_dir: Path):
    build_dir.mkdir(parents=True, exist_ok=True)
    official_probe = build_dir / "kasane_document_official_probe"
    purism_probe = build_dir / "kasane_document_purism_probe"

    print(f"Building Core probes in {build_dir}...")
    run([
        "cmake",
        "-S", ROOT / "tools/probes",
        "-B", build_dir,
        "-DCMAKE_BUILD_TYPE=Release",
        f"-DKASANE_CUBISM_ROOT={sdk_dir.resolve()}",
    ])
    run([
        "cmake",
        "--build", build_dir,
        "--target", "kasane_document_official_probe", "kasane_document_purism_probe",
        "-j4",
    ])
    return official_probe, purism_probe


def require_finite(value):
    if isinstance(value, float) and not math.isfinite(value):
        raise RuntimeError("Non-finite numeric value in conformance data")
    if isinstance(value, dict):
        for item in value.values():
            require_finite(item)
    elif isinstance(value, list):
        for item in value:
            require_finite(item)


def get_git_info():
    try:
        rev = run(["git", "rev-parse", "HEAD"]).strip()
        status = run(["git", "status", "--porcelain"]).strip()
        dirty = bool(status)
        submodules = run(["git", "submodule", "status"]).strip()
        return {"revision": rev, "dirty": dirty, "submodules": submodules}
    except Exception as e:
        return {"error": str(e)}


def compare_samples(expected, actual, ppu, label):
    try:
        from m3b_numeric import compare, np
    except ImportError:
        np = None
    if np is not None:
        return compare(expected, actual, ppu, label)
    return compare_samples_scalar(expected, actual, ppu, label)


def compare_samples_scalar(expected, actual, ppu, label):
    if len(expected) != len(actual):
        raise RuntimeError(f"{label}: sample count mismatch")
    metrics = {"max_pixel_error": 0.0, "max_uv_error": 0.0, "max_float_error": 0.0}

    def scalar(e, a, context, position=False, uv=False):
        if not math.isfinite(e) or not math.isfinite(a):
            raise RuntimeError(f"{context}: non-finite value")
        diff = abs(e - a)
        metrics["max_float_error"] = max(metrics["max_float_error"], diff)
        if position:
            metrics["max_pixel_error"] = max(metrics["max_pixel_error"], diff * ppu)
        if uv:
            metrics["max_uv_error"] = max(metrics["max_uv_error"], diff)
        if diff > 1e-5 + 1e-5 * max(abs(e), abs(a)) or (position and diff * ppu > 0.05):
            raise RuntimeError(
                f"{context}: expected={e}, actual={a}, abs_error={diff}, pixel_error={diff * ppu if position else None}"
            )

    for sample_index, (exp, act) in enumerate(zip(expected, actual)):
        exp_by_id = {d["runtime_id"]: d for d in exp}
        act_by_id = {d["runtime_id"]: d for d in act}
        if len(exp_by_id) != len(exp) or len(act_by_id) != len(act) or exp_by_id.keys() != act_by_id.keys():
            raise RuntimeError(f"{label}/sample {sample_index}: drawable IDs differ or are duplicated")
        for runtime_id, e in exp_by_id.items():
            a = act_by_id[runtime_id]
            context = f"{label}/sample {sample_index}/{runtime_id}"
            for attr in [
                "texture_slot",
                "double_sided",
                "inverted_mask",
                "blend_mode",
                "indices",
                "draw_order",
                "render_order",
                "visible",
            ]:
                if e[attr] != a[attr]:
                    raise RuntimeError(f"{context}/{attr}: expected={e[attr]}, actual={a[attr]}")
            exp_masks = list(dict.fromkeys([exp[i]["runtime_id"] for i in e["mask_indices"]]))
            act_masks = list(dict.fromkeys([act[i]["runtime_id"] for i in a["mask_indices"]]))
            if exp_masks != act_masks:
                raise RuntimeError(f"{context}/masks: expected={exp_masks}, actual={act_masks}")
            scalar(e["opacity"], a["opacity"], context + "/opacity")
            for attr in ["multiply_color", "screen_color", "uvs", "positions"]:
                if len(e[attr]) != len(a[attr]):
                    raise RuntimeError(f"{context}/{attr}: array length mismatch")
                for index, (ev, av) in enumerate(zip(e[attr], a[attr])):
                    if isinstance(ev, list):
                        if len(ev) != len(av):
                            raise RuntimeError(f"{context}/{attr}/{index}: component count mismatch")
                        for component, (ec, ac) in enumerate(zip(ev, av)):
                            scalar(
                                ec,
                                ac,
                                f"{context}/{attr}/{index}/{component}",
                                position=attr == "positions",
                                uv=attr == "uvs",
                            )
                    else:
                        scalar(ev, av, f"{context}/{attr}/{index}")
    return metrics


def read_probe(probe, moc, samples, snapshot):
    inp = f"{len(samples)}\n" + "\n".join(" ".join(map(str, s)) for s in samples) + "\n"
    output = run([probe, moc], text_input=inp)
    result = json.loads(next(line for line in output.splitlines() if line.startswith('{"core_version"')))
    require_finite(result)
    snapshot.write_text(json.dumps(result) + "\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", type=Path, default=ROOT / "third_party/CubismSdkForNative-5-r.5")
    parser.add_argument("--probe-dir", type=Path, default=ROOT / "target/probes")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "target/kasane/m3b")
    parser.add_argument("--numerical-only", action="store_true",
                        help="Return success for numerical checks only; milestone status remains incomplete")
    parser.add_argument("--application", type=Path, default=ROOT / "dist/editor/Kasane-Editor.zip")
    parser.add_argument("--godot", type=Path, default=ROOT / "target/godot-tools/standard/Godot.app/Contents/MacOS/Godot")
    parser.add_argument("--collect-acceptance", action="store_true",
                        help="Combine existing reports in output-dir without rerunning their checks")
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    report_file = args.output_dir / "report.json"
    if args.collect_acceptance:
        from m3b_acceptance import collect
        report = collect(json.loads(report_file.read_text()), args.output_dir / "baseline.json",
                         args.output_dir / "editor/report.json", args.output_dir / "gpu/report.json")
        report_file.write_text(json.dumps(report, indent=2) + "\n")
        print(f"M3B acceptance {report['status']}: {report_file}")
        return 0 if report["status"] == "passed" else 1
    report = {
        "status": "failed",
        "milestone": "M3B",
        "cases": [],
        "checks": [],
        "numerical_failures": [],
        "required_acceptance": {
            "gpu_comparison": {"status": "not_run", "reason": "No GPU comparison in this runner"},
            "packaged_editor_workflow": {"status": "not_run", "reason": "No application workflow in this runner"},
            "detached_texture_project": {"status": "not_run", "reason": "JSON codec roundtrip does not verify asset detachment"},
            "new_feature_edit_roundtrips": {"status": "not_run", "reason": "Unmodified Mao roundtrip does not verify all new edit operations"},
        },
        "system": {
            "platform": platform.platform(),
            "machine": platform.machine(),
            "python": platform.python_version(),
            "git": get_git_info(),
        },
        "scope": "Mao 5.0 editable import, Project v3 persistence, 5.0 re-export, dual-core numerical parity",
    }
    report_file.write_text(json.dumps(report, indent=2) + "\n")
    try:
        official, purism = ensure_probes(args.probe_dir.resolve(), args.sdk.resolve())

        # Step 1: Rust test suites
        print("Running workspace cargo tests...")
        cargo_checks = [
            (["cargo", "test", "-p", "kasane-core", "-p", "kasane-moc3", "-p", "kasane-project", "--locked"], "cargo_test_core_moc3_project"),
            (["cargo", "test", "-p", "kasane-godot", "--lib", "--locked"], "cargo_test_godot_lib"),
            (["cargo", "check", "--workspace", "--locked"], "cargo_check_workspace"),
        ]
        for cmd, label in cargo_checks:
            log = run(cmd)
            (args.output_dir / f"{label}.log").write_text(log)
            report["checks"].append(label)

        # Step 2: Export conformance cases
        fixtures = args.output_dir / "fixtures"
        print(run(["cargo", "run", "--release", "-p", "kasane-moc3", "--example", "export_m3b_cases", "--", fixtures]))
        case_names = json.loads((fixtures / "cases.json").read_text())
        if not case_names:
            raise RuntimeError("No M3B conformance cases")

        # Step 3: Dual-core probe verification
        for name in case_names:
            case_dir = fixtures / name
            samples = json.loads((case_dir / "samples.json").read_text())
            document = json.loads((case_dir / "document.json").read_text())
            ppu = float((case_dir / "ppu.txt").read_text())
            metadata = json.loads((case_dir / "metadata.json").read_text())
            require_finite(samples)
            require_finite(document)
            require_finite(ppu)
            if ppu <= 0:
                raise RuntimeError(f"{name}: invalid pixels_per_unit")

            inputs = {
                kind: {
                    "path": str(case_dir / f"{kind}.moc3"),
                    "sha256": hashlib.sha256((case_dir / f"{kind}.moc3").read_bytes()).hexdigest(),
                    "version": (case_dir / f"{kind}.moc3").read_bytes()[4],
                }
                for kind in ["orig", "re_export"]
            }

            for provider, probe in [("official", official), ("purism", purism)]:
                original = read_probe(probe, case_dir / "orig.moc3", samples, case_dir / f"{provider}_original.json")
                exported = read_probe(probe, case_dir / "re_export.moc3", samples, case_dir / f"{provider}_exported.json")
                comparisons = {}
                for path, expected, actual in [
                    ("source_document", original["samples"], document["samples"]),
                    ("source_export", original["samples"], exported["samples"]),
                    ("document_export", document["samples"], exported["samples"]),
                ]:
                    label = f"{name}/{provider}/{path}"
                    try:
                        comparisons[path] = compare_samples(expected, actual, ppu, label)
                        report["checks"].append(label)
                    except RuntimeError as error:
                        comparisons[path] = {"status": "failed", "error": str(error)}
                        report["numerical_failures"].append(str(error))
                report["cases"].append({
                    "case": name,
                    "provider": provider,
                    "core_version": hex(original["core_version"]),
                    "samples_count": len(samples),
                    "samples_path": str(case_dir / "samples.json"),
                    "metadata_path": str(case_dir / "metadata.json"),
                    "metadata_summary": {
                        "meshes": metadata.get("meshes_count"),
                        "parts": metadata.get("parts_count"),
                        "glues": metadata.get("glues_count"),
                        "blend_bindings": metadata.get("blend_bindings_count"),
                        "blend_key_tables": metadata.get("blend_key_tables_count"),
                        "blend_constraints": metadata.get("blend_constraints_count"),
                    },
                    "inputs": inputs,
                    "ppu": ppu,
                    "comparisons": comparisons,
                    "status": "failed" if any(m.get("status") == "failed" for m in comparisons.values()) else "passed",
                })
                print(f"{report['cases'][-1]['status'].upper()} {name}/{provider}: {len(samples)} samples", flush=True)

        report["numerical_status"] = "failed" if report["numerical_failures"] else "passed"
        report["status"] = "failed" if report["numerical_failures"] else "incomplete"
        report["metrics"] = {
            "total_checks": len(report["checks"]),
            "cases_verified": len(report["cases"]),
            "max_pixel_error_passed_comparisons": max(
                (m["max_pixel_error"] for c in report["cases"] for m in c["comparisons"].values() if "max_pixel_error" in m), default=None
            ),
            "max_float_error_passed_comparisons": max(
                (m["max_float_error"] for c in report["cases"] for m in c["comparisons"].values() if "max_float_error" in m), default=None
            ),
        }
        if not args.numerical_only:
            from m3b_acceptance import collect
            acceptance_commands = [
                [sys.executable, ROOT / "tools/m3b_baseline.py", "--output-dir", args.output_dir],
                [sys.executable, ROOT / "tools/validate_m3b_editor.py", "--application", args.application,
                 "--godot", args.godot, "--output-dir", args.output_dir / "editor"],
                [sys.executable, ROOT / "tools/validate_gpu.py", "--godot", args.godot,
                 "--library", ROOT / "target/release/libkasane_godot.dylib", "--output-dir", args.output_dir / "gpu"],
            ]
            acceptance_reports = [args.output_dir / "baseline.json", args.output_dir / "editor/report.json",
                                  args.output_dir / "gpu/report.json"]
            for index, command in enumerate(acceptance_commands):
                # A failed process must never reuse a previous successful report.
                acceptance_reports[index].unlink(missing_ok=True)
                result = subprocess.run(list(map(str, command)), cwd=ROOT, text=True,
                                        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=600)
                (args.output_dir / f"acceptance-{index}.log").write_text(result.stdout)
                if result.returncode:
                    acceptance_reports[index].write_text(json.dumps({
                        "status": "failed", "returncode": result.returncode,
                        "log": str(args.output_dir / f"acceptance-{index}.log"),
                    }) + "\n")
            report = collect(report, args.output_dir / "baseline.json", args.output_dir / "editor/report.json", args.output_dir / "gpu/report.json")
    except Exception as error:
        report["status"] = "failed"
        report["error"] = str(error)
        raise
    finally:
        report_file.write_text(json.dumps(report, indent=2) + "\n")
    print(f"M3B numerical checks {report['numerical_status']}; milestone acceptance is {report['status']}: {report_file}")
    return 0 if report["status"] == "passed" or (args.numerical_only and report["numerical_status"] == "passed") else 1


if __name__ == "__main__":
    sys.exit(main())
