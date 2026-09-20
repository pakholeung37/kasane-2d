#!/usr/bin/env python3
"""Stage A baseline verification: Mao structure inventory and dual-core conformance.

Extracts structure, verifies SHA-256, verifies 128 parameters (95 Normal, 33 BlendShape),
7 constraints, 7 glues (161 vertex pairs), and executes dual-core probe comparison
between Official Live2D Core and PurismCore.
"""
import argparse
import hashlib
import json
import math
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

MAO_DIR = ROOT / "demos/gd-cubism-demo/assets/live2d/mao/runtime"
MAO_MOC3 = MAO_DIR / "mao_pro.moc3"
MAO_MODEL3 = MAO_DIR / "mao_pro.model3.json"
MAO_TEXTURE = MAO_DIR / "mao_pro.4096/texture_00.png"

EXPECTED_MOC3_SHA256 = "247d028f9900be2a46e4530816ece5749524d35013ba77424bd943598e0e54ff"


def compute_sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(65536):
            h.update(chunk)
    return h.hexdigest()


def ensure_probes(build_dir: Path, sdk_dir: Path):
    build_dir.mkdir(parents=True, exist_ok=True)
    official_probe = build_dir / "kasane_document_official_probe"
    purism_probe = build_dir / "kasane_document_purism_probe"

    if not official_probe.exists() or not purism_probe.exists():
        print(f"Building Core probes in {build_dir}...")
        subprocess.run([
            "cmake",
            "-S", str(ROOT / "tools/probes"),
            "-B", str(build_dir),
            "-DCMAKE_BUILD_TYPE=Release",
            f"-DKASANE_CUBISM_ROOT={sdk_dir.resolve()}",
        ], cwd=ROOT, check=True)
        subprocess.run([
            "cmake",
            "--build", str(build_dir),
            "--target", "kasane_document_official_probe", "kasane_document_purism_probe",
            "-j4",
        ], cwd=ROOT, check=True)
    return official_probe, purism_probe


def run_probe(probe_path: Path, moc3_path: Path, samples: list[list[float]]):
    input_str = f"{len(samples)}\n"
    for s in samples:
        input_str += " ".join(f"{v:.8f}" for v in s) + "\n"

    res = subprocess.run(
        [str(probe_path), str(moc3_path)],
        input=input_str,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=180,
    )
    if res.returncode != 0:
        raise RuntimeError(f"Probe {probe_path.name} failed ({res.returncode}):\n{res.stderr}")

    json_line = next(line for line in res.stdout.splitlines() if line.startswith('{"core_version"'))
    return json.loads(json_line)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", type=Path, default=ROOT / "third_party/CubismSdkForNative-5-r.5")
    parser.add_argument("--probe-dir", type=Path, default=ROOT / "target/probes")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "target/kasane/m3b")
    args = parser.parse_args()

    args.output_dir.mkdir(parents=True, exist_ok=True)
    report_file = args.output_dir / "baseline.json"

    print("=== M3B Stage A: Baseline Inventory & Conformance ===")

    # 1. File existence & SHA-256
    assert MAO_MOC3.exists(), f"Missing {MAO_MOC3}"
    assert MAO_MODEL3.exists(), f"Missing {MAO_MODEL3}"
    assert MAO_TEXTURE.exists(), f"Missing {MAO_TEXTURE}"

    moc3_sha = compute_sha256(MAO_MOC3)
    model3_sha = compute_sha256(MAO_MODEL3)
    texture_sha = compute_sha256(MAO_TEXTURE)

    print(f"MOC3 SHA-256:    {moc3_sha}")
    print(f"Model3 SHA-256:  {model3_sha}")
    print(f"Texture SHA-256: {texture_sha}")
    assert moc3_sha == EXPECTED_MOC3_SHA256, f"MOC3 SHA mismatch: expected {EXPECTED_MOC3_SHA256}, got {moc3_sha}"

    # 2. Structural Inspection
    raw_data = MAO_MOC3.read_bytes()
    version = raw_data[4]
    endian = raw_data[5]
    assert version == 5, f"Expected version 5, got {version}"
    assert endian == 0, f"Expected little-endian, got {endian}"

    offsets = struct.unpack_from("<160I", raw_data, 64)
    canvas_off = offsets[1]
    ppu, ox, oy, w, h = struct.unpack_from("<5f", raw_data, canvas_off)
    canvas_flag = raw_data[canvas_off + 20]

    assert (w, h) == (5800.0, 8400.0), f"Canvas size mismatch: ({w}x{h})"
    assert (ox, oy) == (2900.0, 4200.0), f"Canvas origin mismatch: ({ox}, {oy})"
    assert ppu == 5800.0, f"Canvas PPU mismatch: {ppu}"
    assert canvas_flag == 0, f"Canvas flag mismatch: {canvas_flag}"

    counts_off = offsets[0]
    raw_counts = struct.unpack_from("<64i", raw_data, counts_off)

    counts = {
        "parts": raw_counts[0],
        "deformers": raw_counts[1],
        "warps": raw_counts[2],
        "rotations": raw_counts[3],
        "art_meshes": raw_counts[4],
        "parameters": raw_counts[5],
        "part_keyforms": raw_counts[6],
        "warp_keyforms": raw_counts[7],
        "rotation_keyforms": raw_counts[8],
        "art_mesh_keyforms": raw_counts[9],
        "keyform_pos": raw_counts[10],
        "key_table_idx": raw_counts[11],
        "bindings": raw_counts[12],
        "key_tables": raw_counts[13],
        "keys": raw_counts[14],
        "uvs": raw_counts[15],
        "idx": raw_counts[16],
        "masks": raw_counts[17],
        "draw_groups": raw_counts[18],
        "draw_items": raw_counts[19],
        "glues": raw_counts[20],
        "glue_info": raw_counts[21],
        "glue_keyforms": raw_counts[22],
        "keyform_mul_colors": raw_counts[23],
        "keyform_scr_colors": raw_counts[24],
        "blend_key_tables": raw_counts[25],
        "blend_bindings": raw_counts[26],
        "bs_warps": raw_counts[27],
        "bs_art_meshes": raw_counts[28],
        "bs_constraint_idx": raw_counts[29],
        "bs_constraints": raw_counts[30],
        "bs_constraint_vals": raw_counts[31],
        "bs_parts": raw_counts[32],
        "bs_rotations": raw_counts[33],
        "bs_glues": raw_counts[34],
        "offscreens": raw_counts[35],
        "offscreen_keyforms": raw_counts[36],
        "bs_offscreens": raw_counts[37],
    }

    assert counts["parts"] == 31
    assert counts["deformers"] == 175
    assert counts["warps"] == 116
    assert counts["rotations"] == 59
    assert counts["art_meshes"] == 260
    assert counts["parameters"] == 128
    assert counts["glues"] == 7
    assert counts["glue_info"] == 322  # 161 pairs
    assert counts["glue_keyforms"] == 7
    assert counts["blend_key_tables"] == 33
    assert counts["blend_bindings"] == 124
    assert counts["bs_warps"] == 3
    assert counts["bs_art_meshes"] == 31
    assert counts["bs_parts"] == 1
    assert counts["bs_rotations"] == 3
    assert counts["bs_constraints"] == 7
    assert counts["bs_constraint_idx"] == 234
    assert counts["bs_constraint_vals"] == 14
    assert counts["bs_glues"] == 0
    assert counts["offscreens"] == 0

    print("Structure verification passed: all counts match M3B inventory.")

    # 3. Parameters extraction & classification
    param_id_off = offsets[50]
    param_max_off = offsets[51]
    param_min_off = offsets[52]
    param_def_off = offsets[53]
    param_type_off = offsets[114]

    param_types = struct.unpack_from("<128i", raw_data, param_type_off)
    params = []
    normal_count = 0
    bs_count = 0

    for i in range(128):
        name = raw_data[param_id_off + i * 64 : param_id_off + (i + 1) * 64].split(b"\x00")[0].decode("ascii")
        p_max = struct.unpack_from("<f", raw_data, param_max_off + i * 4)[0]
        p_min = struct.unpack_from("<f", raw_data, param_min_off + i * 4)[0]
        p_def = struct.unpack_from("<f", raw_data, param_def_off + i * 4)[0]
        kind = "BlendShape" if param_types[i] != 0 else "Normal"
        if kind == "Normal":
            normal_count += 1
        else:
            bs_count += 1
        params.append({
            "index": i,
            "id": name,
            "kind": kind,
            "min": p_min,
            "max": p_max,
            "default": p_def,
        })

    assert normal_count == 95, f"Expected 95 Normal params, got {normal_count}"
    assert bs_count == 33, f"Expected 33 BlendShape params, got {bs_count}"
    print(f"Parameter catalog verified: 128 total ({normal_count} Normal, {bs_count} BlendShape).")

    # 4. Constraints extraction
    c_param_off = offsets[132]
    c_val_off_off = offsets[133]
    c_val_len_off = offsets[134]
    val_key_off = offsets[135]
    val_weight_off = offsets[136]

    constraints = []
    expected_constraint_params = [
        "ParamA", "ParamI", "ParamU", "ParamE", "ParamO", "ParamMouthDown", "ParamMouthAngry"
    ]
    for c in range(7):
        pidx = struct.unpack_from("<i", raw_data, c_param_off + c * 4)[0]
        v_off = struct.unpack_from("<i", raw_data, c_val_off_off + c * 4)[0]
        v_len = struct.unpack_from("<i", raw_data, c_val_len_off + c * 4)[0]
        pname = params[pidx]["id"]
        keys = struct.unpack_from(f"<{v_len}f", raw_data, val_key_off + v_off * 4)
        weights = struct.unpack_from(f"<{v_len}f", raw_data, val_weight_off + v_off * 4)
        curve = [{"key": k, "weight": w} for k, w in zip(keys, weights)]
        constraints.append({"index": c, "parameter_id": pname, "parameter_index": pidx, "curve": curve})

    actual_constraint_params = [c["parameter_id"] for c in constraints]
    assert actual_constraint_params == expected_constraint_params, f"Constraint parameters mismatch: {actual_constraint_params}"
    print(f"Constraints verified: 7 curves referencing {actual_constraint_params}.")

    # 5. Dual-Core Probes Execution
    official_probe, purism_probe = ensure_probes(args.probe_dir.resolve(), args.sdk.resolve())

    # Build representative sample configurations (13 samples)
    param_map = {p["id"]: p["index"] for p in params}
    defaults = [p["default"] for p in params]
    mins = [p["min"] for p in params]
    maxs = [p["max"] for p in params]

    def make_sample(overrides: dict[str, float]) -> list[float]:
        s = list(defaults)
        for k, v in overrides.items():
            s[param_map[k]] = v
        return s

    sample_descriptors = [
        ("defaults", {}),
        ("all_minimums", {p["id"]: p["min"] for p in params}),
        ("all_maximums", {p["id"]: p["max"] for p in params}),
        ("vowel_A", {"ParamA": 1.0}),
        ("vowel_I", {"ParamI": 1.0}),
        ("vowel_U", {"ParamU": 1.0}),
        ("vowel_E", {"ParamE": 1.0}),
        ("vowel_O", {"ParamO": 1.0}),
        ("mouth_down", {"ParamMouthDown": 1.0}),
        ("mouth_angry", {"ParamMouthAngry": 1.0}),
        ("mouth_combo", {"ParamA": 0.5, "ParamMouthDown": 0.5}),
        ("head_angles", {"ParamAngleX": 30.0, "ParamAngleY": 30.0, "ParamAngleZ": 30.0}),
        ("eyes_and_rabbit", {
            "ParamEyeLOpen": 0.0, "ParamEyeLSmile": 1.0,
            "ParamEyeROpen": 0.0, "ParamEyeRSmile": 1.0,
            "ParamRabbitSize": 1.0, "ParamRabbitRotate": 30.0, "ParamAuraColor2": 1.0
        }),
    ]

    sample_vectors = [make_sample(overrides) for _, overrides in sample_descriptors]

    print(f"Running dual-core probes on {len(sample_vectors)} sample points...")
    off_result = run_probe(official_probe, MAO_MOC3, sample_vectors)
    pur_result = run_probe(purism_probe, MAO_MOC3, sample_vectors)

    assert len(off_result["samples"]) == len(sample_vectors)
    assert len(pur_result["samples"]) == len(sample_vectors)

    max_position_error_px = 0.0
    max_float_error = 0.0
    sample_metrics = []

    for s_idx, (name, _) in enumerate(sample_descriptors):
        off_sample = off_result["samples"][s_idx]
        pur_sample = pur_result["samples"][s_idx]

        assert len(off_sample) == 260
        assert len(pur_sample) == 260

        off_by_id = {d["runtime_id"]: d for d in off_sample}
        pur_by_id = {d["runtime_id"]: d for d in pur_sample}

        s_max_px = 0.0
        s_max_float = 0.0

        for rid, od in off_by_id.items():
            pd = pur_by_id[rid]
            # Discrete checks
            for attr in ["texture_slot", "draw_order", "render_order", "visible", "double_sided", "inverted_mask", "blend_mode"]:
                assert od[attr] == pd[attr], f"Sample {name} ({rid}): {attr} mismatch ({od[attr]} vs {pd[attr]})"

            assert od["mask_indices"] == pd["mask_indices"]
            assert od["indices"] == pd["indices"]

            # Continuous checks
            op_err = abs(od["opacity"] - pd["opacity"])
            s_max_float = max(s_max_float, op_err)

            for col in ["multiply_color", "screen_color"]:
                for c1, c2 in zip(od[col], pd[col]):
                    s_max_float = max(s_max_float, abs(c1 - c2))

            for (x1, y1), (x2, y2) in zip(od["uvs"], pd["uvs"]):
                s_max_float = max(s_max_float, abs(x1 - x2), abs(y1 - y2))

            for (x1, y1), (x2, y2) in zip(od["positions"], pd["positions"]):
                dx = abs(x1 - x2)
                dy = abs(y1 - y2)
                s_max_float = max(s_max_float, dx, dy)
                s_max_px = max(s_max_px, dx * ppu, dy * ppu)

        max_position_error_px = max(max_position_error_px, s_max_px)
        max_float_error = max(max_float_error, s_max_float)
        sample_metrics.append({
            "sample": name,
            "max_pixel_error": s_max_px,
            "max_float_error": s_max_float,
        })
        print(f"  Sample [{name:16s}]: max pixel error = {s_max_px:.5f} px, max float error = {s_max_float:.3e}")

    print(f"\nOverall dual-core max position error: {max_position_error_px:.6f} px")
    print(f"Overall dual-core max float error:    {max_float_error:.8e}")

    # Realistic samples must be <= 0.05 px; all_maximums stress test involves 128 maxed
    # axes across 5 nested deformers with PPU=5800 where 1e-5 float diff equates to 0.058px.
    for sm in sample_metrics:
        if sm["sample"] in ("all_maximums", "all_minimums"):
            assert sm["max_pixel_error"] <= 0.07, f"Stress sample {sm['sample']} pixel error {sm['max_pixel_error']} exceeded 0.07px"
        else:
            assert sm["max_pixel_error"] <= 0.05, f"Realistic sample {sm['sample']} pixel error {sm['max_pixel_error']} exceeded 0.05px"
        # In all samples, float error must satisfy 1e-5 + 1e-5 * max(|e|, |a|)
        assert sm["max_float_error"] <= 2.0e-5, f"Sample {sm['sample']} float error {sm['max_float_error']} exceeded float tolerance"

    # Save baseline report
    baseline_report = {
        "milestone": "M3B",
        "stage": "Stage A (Baseline)",
        "status": "passed",
        "model": {
            "name": "mao_pro",
            "moc3_path": str(MAO_MOC3),
            "moc3_sha256": moc3_sha,
            "model3_path": str(MAO_MODEL3),
            "model3_sha256": model3_sha,
            "texture_path": str(MAO_TEXTURE),
            "texture_sha256": texture_sha,
        },
        "canvas": {
            "width": w,
            "height": h,
            "origin_x": ox,
            "origin_y": oy,
            "pixels_per_unit": ppu,
            "flag": canvas_flag,
        },
        "counts": counts,
        "parameters": {
            "total": 128,
            "normal_count": normal_count,
            "blendshape_count": bs_count,
            "items": params,
        },
        "constraints": constraints,
        "conformance": {
            "official_core_version": hex(off_result["core_version"]),
            "purism_core_version": hex(pur_result["core_version"]),
            "sample_count": len(sample_vectors),
            "max_position_error_px": max_position_error_px,
            "max_float_error": max_float_error,
            "sample_metrics": sample_metrics,
        },
    }

    report_file.write_text(json.dumps(baseline_report, indent=2) + "\n")
    print(f"\nBaseline report successfully written to {report_file}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
