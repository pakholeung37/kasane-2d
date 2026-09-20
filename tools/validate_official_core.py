#!/usr/bin/env python3
"""Run official Live2D Cubism Core and Purism Core conformance checks on Rust MOC3 exports."""
import argparse
import json
import math
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def run(cmd, cwd=ROOT, text_input=None):
    res = subprocess.run(
        list(map(str, cmd)),
        cwd=cwd,
        input=text_input,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=120,
    )
    if res.returncode != 0:
        raise RuntimeError(f"Command failed ({res.returncode}): {' '.join(map(str, cmd))}\nStderr: {res.stderr}\nStdout: {res.stdout}")
    return res.stdout


def ensure_probes(build_dir: Path, sdk_dir: Path):
    build_dir.mkdir(parents=True, exist_ok=True)
    official_probe = build_dir / "kasane_document_official_probe"
    purism_probe = build_dir / "kasane_document_purism_probe"
    if official_probe.is_file() and purism_probe.is_file():
        return official_probe, purism_probe

    print(f"Building Core probes in {build_dir}...")
    run([
        "cmake",
        "-S", ROOT / "tools/probes",
        "-B", build_dir,
        "-DCMAKE_BUILD_TYPE=Release",
        f"-DKASANE_CUBISM_ROOT={sdk_dir.resolve()}",
    ])
    run(["cmake", "--build", build_dir, "--target", "kasane_document_official_probe", "kasane_document_purism_probe", "-j4"])
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


def validate_drawable_shape(drawable, expected=False):
    require_finite(drawable)
    positions, uvs = drawable["positions"], drawable["uvs"]
    if not positions or len(positions) != len(uvs):
        raise RuntimeError("Missing positions or mismatched vertex/UV count")
    for key in ("multiply_color", "screen_color"):
        if len(drawable[key]) != 4:
            raise RuntimeError(f"Invalid {key} component count")
    for point in positions + uvs:
        if expected:
            if not isinstance(point, dict) or set(point) != {"x", "y"}:
                raise RuntimeError("Invalid expected point")
        elif not isinstance(point, list) or len(point) != 2:
            raise RuntimeError("Invalid runtime point")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", type=Path, default=ROOT / "third_party/CubismSdkForNative-5-r.5")
    parser.add_argument("--probe-dir", type=Path, default=ROOT / "target/kasane/core-regression/build")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "target/kasane/official-conformance")
    args = parser.parse_args()

    args.output_dir.mkdir(parents=True, exist_ok=True)
    (args.output_dir / "report.json").write_text(json.dumps({"status": "failed", "error": "Validation incomplete"}) + "\n")
    fixtures_dir = args.output_dir / "fixtures"

    print("Step 1: Exporting Rust conformance fixtures...")
    run(["cargo", "run", "-p", "kasane-moc3", "--example", "export_conformance_cases", "--", fixtures_dir])

    print("Step 2: Checking/building official and Purism Core probes...")
    official_probe, purism_probe = ensure_probes(args.probe_dir.resolve(), args.sdk.resolve())

    cases = sorted([d for d in fixtures_dir.iterdir() if d.is_dir()])
    if not cases:
        raise RuntimeError("No conformance cases found!")

    report = {
        "status": "failed",
        "scope": "Rust MOC3 conformance against official Cubism Core and Purism Core",
        "cases": [],
        "checks": [],
        "metrics": {},
    }

    total_checks = 0
    max_position_error = 0.0
    max_pixel_error = 0.0

    for case_dir in cases:
        case_name = case_dir.name
        print(f"\n--- Validating case: {case_name} ---")
        moc3_path = case_dir / "model.moc3"
        samples_path = case_dir / "samples.json"
        if not moc3_path.is_file() or not samples_path.is_file():
            raise RuntimeError(f"{case_name}: missing moc3 or samples.json")

        samples = json.loads(samples_path.read_text())
        require_finite(samples)
        if not samples or any(not s["drawables"] for s in samples):
            raise RuntimeError("Empty conformance samples/drawables")
        input_data = f"{len(samples)}\n" + "\n".join(" ".join(map(str, s["parameters"])) for s in samples) + "\n"

        for provider, probe in [("official", official_probe), ("purism", purism_probe)]:
            stdout = run([probe, moc3_path], text_input=input_data)
            json_line = next(line for line in stdout.splitlines() if line.startswith('{"core_version"'))
            runtime = json.loads(json_line)
            require_finite(runtime)

            core_version = hex(runtime["core_version"])
            report["checks"].append(f"{case_name}/{provider}_revived_and_initialized")
            total_checks += 1

            if len(runtime["samples"]) != len(samples):
                raise RuntimeError(f"{case_name}/{provider}: sample count mismatch: {len(runtime['samples'])} vs {len(samples)}")

            for s_idx, (expected_sample, actual_drawables) in enumerate(zip(samples, runtime["samples"])):
                expected_drawables = expected_sample["drawables"]
                if len(actual_drawables) != len(expected_drawables):
                    raise RuntimeError(f"{case_name}/{provider}/sample_{s_idx}: drawable count mismatch")

                expected_ids = [d["id"] for d in expected_drawables]

                for d_idx, (exp, act) in enumerate(zip(expected_drawables, actual_drawables)):
                    validate_drawable_shape(exp, expected=True)
                    validate_drawable_shape(act)
                    for key in ("positions", "uvs", "multiply_color", "screen_color"):
                        if len(exp[key]) != len(act[key]):
                            raise RuntimeError(f"{case_name}/{provider}: {key} length mismatch")
                    # Check discrete properties
                    for key in ["runtime_id", "texture_slot", "render_order", "double_sided", "inverted_mask", "blend_mode"]:
                        if exp[key] != act[key]:
                            raise RuntimeError(f"{case_name}/{provider}/s{s_idx}/d{d_idx} ({exp['runtime_id']}): {key} mismatch: expected {exp[key]}, got {act[key]}")

                    # Check draw_order
                    if abs(exp["draw_order"] - act["draw_order"]) > 0.01:
                        raise RuntimeError(f"{case_name}/{provider}/s{s_idx}/d{d_idx} ({exp['runtime_id']}): draw_order mismatch: {exp['draw_order']} vs {act['draw_order']}")

                    # Check indices
                    if exp["indices"] != act["indices"]:
                        raise RuntimeError(f"{case_name}/{provider}/s{s_idx}/d{d_idx}: indices mismatch")

                    # Check masks
                    expected_mask_indices = [expected_ids.index(m) for m in exp["masks"]]
                    if expected_mask_indices != act["mask_indices"]:
                        raise RuntimeError(f"{case_name}/{provider}/s{s_idx}/d{d_idx}: mask indices mismatch: expected {expected_mask_indices}, got {act['mask_indices']}")

                    # Check opacity
                    op_err = abs(exp["opacity"] - act["opacity"])
                    if op_err > 1e-4:
                        raise RuntimeError(f"{case_name}/{provider}/s{s_idx}/d{d_idx}: opacity error {op_err} > 1e-4")

                    # Check multiply and screen colors
                    for color_key in ["multiply_color", "screen_color"]:
                        for c_exp, c_act in zip(exp[color_key], act[color_key]):
                            if abs(c_exp - c_act) > 1e-4:
                                raise RuntimeError(f"{case_name}/{provider}/s{s_idx}/d{d_idx}: {color_key} error: {c_exp} vs {c_act}")

                    # Check positions
                    for p_exp, p_act in zip(exp["positions"], act["positions"]):
                        ex, ey = p_exp["x"], p_exp["y"]
                        ax, ay = p_act[0], p_act[1]
                        err_x = abs(ex - ax)
                        err_y = abs(ey - ay)
                        pos_err = max(err_x, err_y)
                        pixel_err = pos_err * 100.0  # ppu = 100
                        max_position_error = max(max_position_error, pos_err)
                        max_pixel_error = max(max_pixel_error, pixel_err)

                        # Threshold: pixel error <= 0.05 px or relative error <= 1e-4
                        if any(abs(e - a) * 100.0 > 0.05 and abs(e - a) > 1e-4 + 1e-4 * max(abs(e), abs(a)) for e, a in ((ex, ax), (ey, ay))):
                            raise RuntimeError(f"{case_name}/{provider}/s{s_idx}/d{d_idx}: position error {pos_err} ({pixel_err} px): expected ({ex}, {ey}), got ({ax}, {ay})")

                    # Check UVs
                    for uv_exp, uv_act in zip(exp["uvs"], act["uvs"]):
                        ux, uy = uv_exp["x"], uv_exp["y"]
                        ax, ay = uv_act[0], uv_act[1]
                        uv_err = max(abs(ux - ax), abs(uy - ay))
                        if uv_err > 1e-4:
                            raise RuntimeError(f"{case_name}/{provider}/s{s_idx}/d{d_idx}: UV error {uv_err}: expected ({ux}, {uy}), got ({ax}, {ay})")

            report["cases"].append({
                "case": case_name,
                "provider": provider,
                "core_version": core_version,
                "samples_validated": len(samples),
                "status": "passed",
            })
            print(f"  [PASS] {provider} Core (version {core_version}): {len(samples)} samples verified successfully.")

    report["status"] = "passed"
    report["metrics"] = {
        "total_checks": total_checks,
        "max_position_error": max_position_error,
        "max_pixel_error": max_pixel_error,
    }

    report_file = args.output_dir / "report.json"
    report_file.write_text(json.dumps(report, indent=2) + "\n")
    print(f"\nOfficial and Purism Core conformance PASSED: {len(report['cases'])} cases, {total_checks} checks.")
    print(f"Max pixel error across all parameter samples: {max_pixel_error:.5f} px (per-axis absolute/relative tolerance).")
    print(f"Report saved to: {report_file}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
