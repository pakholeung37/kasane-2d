#!/usr/bin/env python3
"""Milestone 3 (M3) Acceptance Validation Script: MOC3 Import, Editing, and Re-export.

Validates:
1. M1 constructed model import-export roundtrip
2. External v33 model (3d8e869a678a1dac) import, evaluation & dual-core conformance
3. External v50 model (tests/fixtures/external_v50) import & dual-core conformance
4. Post-import editing (mesh keyforms, warp control points, rotation pose, bindings, draw attributes)
5. Structural editing (adding meshes/parameters, rebinding, deleting unbound objects)
6. Project detachment (import -> save .kasane -> delete source files -> reopen -> edit -> export MOC3)
7. Resource mapping (bare MOC3, model3.json, deterministic UUIDs, missing texture diagnostics)
8. Unsupported features rejection (BlendShape, Glue, Offscreen, cyclic parameters)
9. Malformed and corrupted file rejection
"""
import argparse
import json
import math
import hashlib
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
        timeout=180,
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
    if len(expected) != len(actual):
        raise RuntimeError(f"{label}: sample count mismatch")
    metrics = {"max_pixel_error": 0.0, "max_uv_error": 0.0, "max_float_error": 0.0}

    def scalar(e, a, context, position=False, uv=False):
        diff = abs(e - a)
        metrics["max_float_error"] = max(metrics["max_float_error"], diff)
        if position:
            metrics["max_pixel_error"] = max(metrics["max_pixel_error"], diff * ppu)
        if uv:
            metrics["max_uv_error"] = max(metrics["max_uv_error"], diff)
        if diff > 1e-5 + 1e-5 * max(abs(e), abs(a)) or (position and diff * ppu > 0.05):
            raise RuntimeError(f"{context}: expected={e}, actual={a}, abs_error={diff}, pixel_error={diff * ppu if position else None}")

    for sample_index, (exp, act) in enumerate(zip(expected, actual)):
        exp_by_id = {d["runtime_id"]: d for d in exp}
        act_by_id = {d["runtime_id"]: d for d in act}
        if len(exp_by_id) != len(exp) or len(act_by_id) != len(act) or exp_by_id.keys() != act_by_id.keys():
            raise RuntimeError(f"{label}/sample {sample_index}: drawable IDs differ or are duplicated")
        for runtime_id, e in exp_by_id.items():
            a = act_by_id[runtime_id]
            context = f"{label}/sample {sample_index}/{runtime_id}"
            for attr in ["texture_slot", "double_sided", "inverted_mask", "blend_mode", "indices", "draw_order", "render_order", "visible"]:
                if e[attr] != a[attr]:
                    raise RuntimeError(f"{context}/{attr}: expected={e[attr]}, actual={a[attr]}")
            exp_masks = [exp[i]["runtime_id"] for i in e["mask_indices"]]
            act_masks = [act[i]["runtime_id"] for i in a["mask_indices"]]
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
                            scalar(ec, ac, f"{context}/{attr}/{index}/{component}", position=attr == "positions", uv=attr == "uvs")
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
    parser.add_argument("--output-dir", type=Path, default=ROOT / "target/kasane/m3")
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    report_file = args.output_dir / "report.json"
    report = {"status": "failed", "milestone": "M3", "cases": [], "checks": [],
              "system": {"platform": platform.platform(), "machine": platform.machine(),
                         "python": platform.python_version(), "git": get_git_info()},
              "scope": "Source MOC3 / imported Document / re-export, official and Purism Core"}
    report_file.write_text(json.dumps(report, indent=2) + "\n")
    try:
        official, purism = ensure_probes(args.probe_dir.resolve(), args.sdk.resolve())
        for package, test in [("kasane-moc3", "import_tests"), ("kasane-project", "project_tests")]:
            log = run(["cargo", "test", "-p", package, "--test", test])
            (args.output_dir / f"{test}.log").write_text(log)
            report["checks"].append(f"{package}/{test}")
        fixtures = args.output_dir / "fixtures"
        print(run(["cargo", "run", "-p", "kasane-moc3", "--example", "export_m3_cases", "--", fixtures]))
        case_names = json.loads((fixtures / "cases.json").read_text())
        if not case_names:
            raise RuntimeError("No M3 conformance cases")
        for name in case_names:
            case_dir = fixtures / name
            samples = json.loads((case_dir / "samples.json").read_text())
            document = json.loads((case_dir / "document.json").read_text())
            ppu = float((case_dir / "ppu.txt").read_text())
            require_finite(samples)
            require_finite(document)
            require_finite(ppu)
            if ppu <= 0:
                raise RuntimeError(f"{name}: invalid pixels_per_unit")
            inputs = {kind: {"path": str(case_dir / f"{kind}.moc3"),
                            "sha256": hashlib.sha256((case_dir / f"{kind}.moc3").read_bytes()).hexdigest(),
                            "version": (case_dir / f"{kind}.moc3").read_bytes()[4]}
                      for kind in ["orig", "re_export"]}
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
                    comparisons[path] = compare_samples(expected, actual, ppu, label)
                    report["checks"].append(label)
                report["cases"].append({"case": name, "provider": provider, "core_version": hex(original["core_version"]),
                                        "samples_count": len(samples), "samples_path": str(case_dir / "samples.json"),
                                        "metadata_path": str(case_dir / "metadata.json"), "inputs": inputs, "ppu": ppu,
                                        "comparisons": comparisons, "status": "passed"})
                print(f"PASS {name}/{provider}: {len(samples)} samples, all three paths")
        report["status"] = "passed"
        report["metrics"] = {"total_checks": len(report["checks"]), "cases_verified": len(report["cases"]),
                             "max_pixel_error": max(m["max_pixel_error"] for c in report["cases"] for m in c["comparisons"].values())}
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        report_file.write_text(json.dumps(report, indent=2) + "\n")
    print(f"M3 validation passed: {report_file}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
