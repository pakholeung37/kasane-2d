#!/usr/bin/env python3
"""Build and validate the independent official Framework CPU animation trace."""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import platform
import shutil
import struct
import subprocess
import sys
from copy import deepcopy
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests/fixtures/animation_cpu"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_sha256(root: Path) -> str:
    digest = hashlib.sha256()
    sources = sorted(p for p in root.rglob("*") if p.suffix in {".cpp", ".hpp", ".h"})
    if not sources:
        raise AssertionError(f"Framework sources missing: {root}")
    for path in sources:
        digest.update(str(path.relative_to(root)).encode())
        digest.update(bytes.fromhex(sha256(path)))
    return digest.hexdigest()


def run(command: list[str], log: Path, timeout: int = 120) -> subprocess.CompletedProcess[str]:
    try:
        result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        log.write_text("$ " + " ".join(command) + f"\ntimeout={timeout}s\n"
                       + str(error.stdout or "") + "\n" + str(error.stderr or ""))
        raise AssertionError(f"command timed out after {timeout}s: {command[0]}") from error
    except OSError as error:
        log.write_text("$ " + " ".join(command) + f"\nos_error={error}\n")
        raise AssertionError(f"cannot execute {command[0]}: {error}") from error
    log.write_text("$ " + " ".join(command) + "\nexit=" + str(result.returncode)
                   + "\n[stdout]\n" + result.stdout + "\n[stderr]\n" + result.stderr)
    return result


def finite_json(text: str) -> object:
    parsed = json.loads(text, parse_constant=lambda value: (_ for _ in ()).throw(
        AssertionError(f"non-finite JSON constant: {value}")))
    def check(value: object) -> None:
        if isinstance(value, float) and not math.isfinite(value):
            raise AssertionError("non-finite trace value")
        if isinstance(value, dict):
            for item in value.values():
                check(item)
        elif isinstance(value, list):
            for item in value:
                check(item)
    check(parsed)
    return parsed


def close(actual: float, expected: float) -> None:
    if type(actual) not in (int, float) or not math.isfinite(actual):
        raise AssertionError(f"trace value is not a finite number: {actual!r}")
    if not math.isclose(actual, expected, rel_tol=0, abs_tol=0.00002):
        raise AssertionError(f"reference drift: actual={actual}, expected={expected}")


def check_trace(trace: object, real_control: bool = False) -> None:
    if not isinstance(trace, dict):
        raise AssertionError("trace is not an object")
    if set(trace) != {"core_version", "motion_behavior", "real_parameter_count",
                      "real_part_count", "real_drawable_count", "real_parameter_ids",
                      "real_part_ids", "drawable_parent_parts", "frames"}:
        raise AssertionError("trace header schema changed")
    expected_header = {
        "motion_behavior": 1,
        "real_parameter_count": 2,
        "real_part_count": 2,
        "real_drawable_count": 2,
        "real_parameter_ids": ["ParamX", "Part0"] if real_control else ["ParamX", "ParamY"],
        "real_part_ids": ["Part0", "Part1"],
        "drawable_parent_parts": [0, 1],
    }
    for key, expected in expected_header.items():
        if type(trace.get(key)) is not type(expected) or trace.get(key) != expected:
            raise AssertionError(f"unexpected {key}: {trace.get(key)!r}")
        if key == "drawable_parent_parts" and any(type(item) is not int for item in trace[key]):
            raise AssertionError("drawable parent index has wrong type")
    if type(trace.get("core_version")) is not int or trace["core_version"] <= 0:
        raise AssertionError("missing official Core version")
    frames = trace.get("frames")
    if not isinstance(frames, list) or len(frames) != 4:
        raise AssertionError("expected exactly four frames")
    expected = [
        ("pose_reset", 0, 1, 0, [1, 0], 1,
         [0, 1] if real_control else [0, 0],
         [.95, 0] if real_control else [.875, 0]),
        ("motion_pose", .25, 0, 1, [.7, .5], .3, [0, 0], [.6125, .4375]),
        ("motion_pose", .5, 0, 1, [0, 1], .4, [.25, 0], [0, .8625]),
        ("motion_pose", .75, 0, 1, [0, 1], .5, [.5, 0], [0, .85]),
    ]
    for frame, want in zip(frames, expected):
        if not isinstance(frame, dict) or set(frame) != {"stage", "time", "part0_control",
                                                       "part1_control", "part_opacities",
                                                       "model_opacity", "parameters",
                                                       "drawable_opacities"}:
            raise AssertionError("frame schema changed")
        stage, time, part0, part1, opacities, model_opacity, parameters, drawables = want
        if frame.get("stage") != stage:
            raise AssertionError("unexpected frame stage")
        close(frame["time"], time)
        for key, expected_index, expected_value in (
            ("part0_control", 1 if real_control else 2, part0),
            ("part1_control", 2 if real_control else 3, part1)
        ):
            control = frame.get(key)
            if not isinstance(control, dict) or set(control) != {"index", "value"}:
                raise AssertionError("control slot schema changed")
            if type(control["index"]) is not int or control["index"] != expected_index:
                raise AssertionError(f"{key} has wrong parameter index")
            if key == "part0_control" and real_control:
                if expected_index >= trace["real_parameter_count"]:
                    raise AssertionError("Part0 control should be a real parameter")
            elif expected_index < trace["real_parameter_count"]:
                raise AssertionError(f"{key} should be a virtual parameter")
            close(control["value"], expected_value)
        for key, values in (("part_opacities", opacities), ("parameters", parameters),
                            ("drawable_opacities", drawables)):
            if not isinstance(frame[key], list) or len(frame[key]) != len(values):
                raise AssertionError(f"{key} has wrong length")
            for actual, expected_value in zip(frame[key], values):
                close(actual, expected_value)
        close(frame["model_opacity"], model_opacity)


def check_validator_negative_controls(trace: dict[str, object]) -> None:
    variants: list[dict[str, object]] = []
    wrong_id = deepcopy(trace)
    wrong_id["real_part_ids"][1] = "PartWrong"
    variants.append(wrong_id)
    short = deepcopy(trace)
    short["frames"].pop()
    variants.append(short)
    boolean = deepcopy(trace)
    boolean["frames"][0]["part_opacities"][0] = True
    variants.append(boolean)
    string = deepcopy(trace)
    string["frames"][0]["model_opacity"] = "1"
    variants.append(string)
    missing = deepcopy(trace)
    del missing["frames"][0]["time"]
    variants.append(missing)
    bad_parent_type = deepcopy(trace)
    bad_parent_type["drawable_parent_parts"][1] = True
    variants.append(bad_parent_type)
    for candidate in variants:
        try:
            check_trace(candidate)
        except (AssertionError, KeyError, TypeError):
            continue
        raise AssertionError("strict trace validator accepted a corrupted result")
    for bad in ('{"value":NaN}', '{"value":Infinity}', '{"value":-Infinity}'):
        try:
            finite_json(bad)
        except AssertionError:
            continue
        raise AssertionError("strict JSON loader accepted a non-finite constant")
    try:
        finite_json('{"frames":[')
    except json.JSONDecodeError:
        pass
    else:
        raise AssertionError("strict JSON loader accepted a truncated result")


def write_report(path: Path, report: dict[str, object]) -> None:
    temporary = path.with_suffix(".json.tmp")
    temporary.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    os.replace(temporary, path)


def trace_from_run(result: subprocess.CompletedProcess[str], label: str) -> dict[str, object]:
    if result.returncode:
        raise AssertionError(f"{label} Framework trace failed")
    records = [line for line in result.stdout.splitlines() if line.startswith("{")]
    if len(records) != 1:
        raise AssertionError(f"{label} emitted {len(records)} JSON records; expected one")
    data = finite_json(records[0])
    if not isinstance(data, dict):
        raise AssertionError(f"{label} did not emit an object")
    return data


def check_repeat(trace: dict[str, object]) -> None:
    if set(trace) != {"parameter_id", "parameter_index", "moc_repeat", "minimum", "maximum", "configs"}:
        raise AssertionError("repeat trace schema changed")
    if trace["parameter_id"] != "ParamY" or type(trace["parameter_index"]) is not int or trace["parameter_index"] != 1 or trace["moc_repeat"] is not True:
        raise AssertionError("repeat fixture did not expose the real ParamY MOC bit")
    close(trace["minimum"], -1)
    close(trace["maximum"], 1)
    configs = trace["configs"]
    if not isinstance(configs, list) or len(configs) != 2:
        raise AssertionError("repeat configs missing")
    inputs = [-3, -1.25, -1, -.75, 0, .75, 1, 1.25, 3]
    expected_values = [[-1, -1, -1, -.75, 0, .75, 1, 1, 1],
                       [1, .75, -1, -.75, 0, .75, 1, -.75, -1]]
    expected_geometry = [
        [-1.5, -1.5, -1.5, -1.475, -1.4, -1.325, -1.5, -1.5, -1.5],
        [-1.5, -1.325, -1.5, -1.475, -1.4, -1.325, -1.5, -1.475, -1.5],
    ]
    for config, override, effective, values, geometry in zip(configs, (True, False), (False, True), expected_values, expected_geometry):
        if config.get("model_override") is not override or config.get("effective_repeat") is not effective:
            raise AssertionError("repeat override configuration mismatch")
        samples = config.get("samples")
        if not isinstance(samples, list) or len(samples) != len(inputs):
            raise AssertionError("repeat sample count mismatch")
        for sample, input_value, result, vertex_y in zip(samples, inputs, values, geometry):
            if not isinstance(sample, dict) or set(sample) != {"input", "parameter", "drawable0_vertex0_y"}:
                raise AssertionError("repeat sample schema changed")
            close(sample["input"], input_value)
            close(sample["parameter"], result)
            close(sample["drawable0_vertex0_y"], vertex_y)
    # The Framework stores the exact maximum, while Core geometry evaluates it
    # at the periodic start for this fixture. Keep both observations visible.
    repeating = configs[1]["samples"]
    if repeating[6]["parameter"] != 1 or not math.isclose(
        repeating[6]["drawable0_vertex0_y"], repeating[2]["drawable0_vertex0_y"], abs_tol=.00002):
        raise AssertionError("repeat maximum/geometry edge changed")


def check_loop(trace: dict[str, object]) -> None:
    if trace.get("meta_loop") is not True or trace.get("set_loop_called") is not True:
        raise AssertionError("loop activation is not explicit")
    configs = trace.get("configs")
    if not isinstance(configs, list) or len(configs) != 2:
        raise AssertionError("V1/V2 loop traces missing")
    times = [.25, .5, 1, 1.25, 1.5, 1.75, 2.25]
    values = [[0, .25, .75, 1, .5, 1, .5],
              [0, .25, .75, 1, .25, .5, 1]]
    for config, behavior, expected in zip(configs, (1, 0), values):
        if type(config.get("default_behavior")) is not int or config["default_behavior"] != 1 or type(config.get("behavior")) is not int or config["behavior"] != behavior:
            raise AssertionError("motion behavior version mismatch")
        frames = config.get("frames")
        if not isinstance(frames, list) or len(frames) != len(times):
            raise AssertionError("loop frame count mismatch")
        for frame, time, result in zip(frames, times, expected):
            if not isinstance(frame, dict) or set(frame) != {"time", "param_x"}:
                raise AssertionError("loop frame schema changed")
            close(frame["time"], time)
            close(frame["param_x"], result)


def check_physics(trace: dict[str, object]) -> None:
    configs = trace.get("configs")
    if not isinstance(configs, list) or len(configs) != 3:
        raise AssertionError("physics fps configs missing")
    expected = [
        [0, 0, -.654304147, -.307027221, .951560616, -.46348238],
        [0, 0, -.654304147, -.307027221, .951560616, -.46348238],
        [0, 0, -.212036729, -.424073458, .786318183, -.315223336],
    ]
    dts = [1 / 60, 1 / 60, 1 / 60, 1 / 60, 1 / 30, .1]
    inputs = [0, 1, 1, -1, -1, 0]
    for config, name, fps, outputs in zip(configs, ("missing", "zero", "thirty"), (0, 0, 30), expected):
        if config.get("config") != name:
            raise AssertionError("physics config identity mismatch")
        close(config["parsed_fps"], fps)
        frames = config.get("frames")
        if not isinstance(frames, list) or len(frames) != len(dts):
            raise AssertionError("physics frame count mismatch")
        elapsed = 0.0
        for frame, dt, input_value, output in zip(frames, dts, inputs, outputs):
            if not isinstance(frame, dict) or set(frame) != {"time", "dt", "input", "output"}:
                raise AssertionError("physics frame schema changed")
            elapsed += dt
            close(frame["time"], elapsed)
            close(frame["dt"], dt)
            close(frame["input"], input_value)
            close(frame["output"], output)
    if configs[0]["frames"] != configs[1]["frames"]:
        raise AssertionError("missing fps and zero fps diverged")
    if abs(configs[0]["frames"][2]["output"] - configs[2]["frames"][2]["output"]) < .1:
        raise AssertionError("fixed-fps substep path was not distinguished")


def numeric_relative_error(actual: object, expected_f32: float, limit: float) -> float:
    if type(actual) not in (int, float) or not math.isfinite(actual):
        raise AssertionError("numeric parser returned a non-finite or nonnumeric value")
    if expected_f32 == 0:
        if actual != 0:
            raise AssertionError("numeric parser changed zero to a nonzero value")
        return 0.0
    error = abs(actual - expected_f32) / abs(expected_f32)
    if error > limit:
        raise AssertionError(f"numeric parser relative error {error} exceeds {limit}")
    return error


def check_numeric_cases(probe: Path, output: Path) -> dict[str, object]:
    cases = {
        "zero_newline": ('{"Value":0\n}', True, 0.0),
        "negative": ('{"Value":-1\n}', True, -1.0),
        "decimal": ('{"Value":0.5\n}', True, .5),
        "tiny_decimal": ('{"Value":0.' + '0' * 37 + '1\n}', True, 1e-38),
        "large_decimal": ('{"Value":340282300000000000000000000000000000000\n}', True, 3.402823e38),
        "small_exponent": ('{"Value":1e-7\n}', False, None),
        "large_exponent": ('{"Value":1e7\n}', False, None),
        "object_close": ('{"Value":0}', False, None),
        "array_close": ('{"Value":[0]}', False, None),
        "space_before_newline": ('{"Value":0 \n}', False, None),
        "comma": ('{"Value":0,"Tail":1\n}', True, 0.0),
        "string_value": ('{"Value":"1"}', True, None),
    }
    folder = output / "numeric_cases"
    folder.mkdir(exist_ok=True)
    observations: dict[str, object] = {}
    for name, (source, accepted, expected_number) in cases.items():
        finite_json(source)  # All cases, even Framework rejections, are valid JSON.
        path = folder / f"{name}.json"
        path.write_text(source)
        result = run([str(probe), "--json-check", str(path)], folder / f"{name}.log")
        observed = trace_from_run(result, f"numeric {name}")
        if observed.get("accepted") is not accepted:
            raise AssertionError(f"numeric parser acceptance changed for {name}")
        if not accepted and set(observed) != {"accepted"}:
            raise AssertionError(f"rejected numeric case emitted unexpected fields: {name}")
        if accepted and expected_number is not None and set(observed) != {"accepted", "numeric", "finite", "value"}:
            raise AssertionError(f"accepted numeric case emitted wrong schema: {name}")
        if expected_number is not None:
            if observed.get("numeric") is not True or observed.get("finite") is not True:
                raise AssertionError(f"numeric parser lost finite number for {name}")
            actual = observed.get("value")
            expected_f32 = struct.unpack("<f", struct.pack("<f", expected_number))[0]
            # Extremes have a measured parser drift (see numeric.json); this
            # tolerance only locks the observation, not an exporter-safe range.
            limit = 2e-6 if name in {"tiny_decimal", "large_decimal"} else 3e-7
            relative_error = numeric_relative_error(actual, expected_f32, limit)
        elif name == "string_value" and observed.get("numeric") is not False:
            raise AssertionError("nonnumeric JSON was mistaken for numeric")
        if name == "string_value" and set(observed) != {"accepted", "numeric"}:
            raise AssertionError("nonnumeric JSON case emitted wrong schema")
        observations[name] = {"source_sha256": sha256(path), "result": observed}
        if expected_number is not None:
            observations[name]["float32_reference"] = expected_f32
            observations[name]["relative_error"] = relative_error
    # Mutation check for the zero branch: nonzero output must never pass on a
    # vacuous relative error calculation.
    try:
        numeric_relative_error(1.0, 0.0, 3e-7)
    except AssertionError:
        pass
    else:
        raise AssertionError("zero-value validator negative control failed")
    return observations


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", type=Path,
                        default=ROOT / "third_party/CubismSdkForNative-5-r.5")
    parser.add_argument("--build-dir", type=Path, default=ROOT / "target/animation-cpu-probe")
    args = parser.parse_args()
    sdk = args.sdk.resolve()
    build = args.build_dir.resolve()
    output = build / "results"
    output.mkdir(parents=True, exist_ok=True)
    report_path = output / "report.json"
    report: dict[str, object] = {"status": "failed", "phase": "incomplete",
                                 "checks": {}, "logs": {}, "inputs": {}}
    write_report(report_path, report)
    try:
        for stale in output.glob("*.log"):
            stale.unlink()
        for stale in (output / "reference.json", output / "real_control.json", output / "repeat.json",
                      output / "motion_loop.json", output / "physics_fps.json",
                      output / "numeric.json"):
            stale.unlink(missing_ok=True)
        shutil.rmtree(output / "numeric_cases", ignore_errors=True)
        stale_control = output / "stale_control.json"
        write_report(stale_control, {"status": "passed"})
        write_report(stale_control, {"status": "failed", "phase": "incomplete"})
        if json.loads(stale_control.read_text()) != {"status": "failed", "phase": "incomplete"}:
            raise AssertionError("old passed report was not invalidated")
        stale_control.unlink()
        report["checks"]["stale_report_negative_control"] = "passed"
        platforms = {("Darwin", "arm64"): ("macos", "arm64"),
                     ("Darwin", "x86_64"): ("macos", "x86_64"),
                     ("Linux", "x86_64"): ("linux", "x86_64")}
        machine = platforms.get((platform.system(), platform.machine()))
        if machine is None:
            raise AssertionError("unsupported official Core platform/architecture")
        core = sdk / "Core/lib" / machine[0] / machine[1] / "libLive2DCubismCore.a"
        framework = sdk / "Framework/src"
        if not core.is_file() or not framework.is_dir():
            raise AssertionError("official SDK Core or Framework missing")
        generated = run([sys.executable, str(ROOT / "tools/create_animation_cpu_moc3.py")],
                        output / "generate.log")
        if generated.returncode:
            raise AssertionError("synthetic MOC3 generation failed")
        physics_generated = run([sys.executable, str(ROOT / "tools/create_animation_cpu_physics.py")],
                                output / "generate_physics.log")
        if physics_generated.returncode:
            raise AssertionError("synthetic physics3 generation failed")
        report["inputs"] = {str(path.relative_to(ROOT)): sha256(path) for path in (
            FIXTURES / "model.moc3", FIXTURES / "model_real_part_control.moc3",
            FIXTURES / "minimal.motion3.json",
            FIXTURES / "minimal.pose3.json", FIXTURES / "inline_numbers.motion3.json",
            FIXTURES / "loop.motion3.json", FIXTURES / "missing.physics3.json",
            FIXTURES / "zero.physics3.json", FIXTURES / "thirty.physics3.json",
            ROOT / "tools/create_animation_cpu_moc3.py",
            ROOT / "tools/create_animation_cpu_physics.py",
            ROOT / "tools/probes/framework_animation_cpu_probe.cpp",
            ROOT / "tools/probes/framework_cpu_renderer_cleanup.cpp",
            ROOT / "tools/probes/CMakeLists.txt",
            ROOT / "tools/run_framework_animation_cpu_probe.py",
        )}
        report["framework_source_sha256"] = source_sha256(framework)
        report["core_headers_sha256"] = source_sha256(sdk / "Core/include")
        report["core_library_sha256"] = sha256(core)
        configure = run(["cmake", "-S", str(ROOT / "tools/probes"), "-B", str(build),
                         f"-DKASANE_CUBISM_ROOT={sdk}", "-DCMAKE_BUILD_TYPE=Release"],
                        output / "configure.log")
        if configure.returncode:
            raise AssertionError("CMake configure failed")
        built = run(["cmake", "--build", str(build), "--target",
                     "kasane_framework_animation_cpu_probe", "-j4"], output / "build.log", timeout=240)
        if built.returncode:
            raise AssertionError("CPU probe build failed")
        probe = build / "kasane_framework_animation_cpu_probe"
        report["probe_binary_sha256"] = sha256(probe)
        command = [str(probe), str(FIXTURES / "model.moc3"),
                   str(FIXTURES / "minimal.motion3.json"),
                   str(FIXTURES / "minimal.pose3.json")]
        positive = run(command, output / "positive.log")
        trace = trace_from_run(positive, "PartOpacity/Pose")
        check_trace(trace)
        check_validator_negative_controls(trace)
        (output / "reference.json").write_text(json.dumps(trace, indent=2) + "\n")
        report["checks"]["part_opacity_pose"] = "passed"
        report["checks"]["model_opacity_cpu"] = "passed"
        report["checks"]["model_opacity_pixels"] = "not_run"
        report["checks"]["offscreen_opacity"] = "not_run"
        real_control = trace_from_run(run([str(probe), "--real-control",
                                           str(FIXTURES / "model_real_part_control.moc3"),
                                           command[2], command[3]],
                                          output / "real_control.log"), "real Part control")
        check_trace(real_control, real_control=True)
        (output / "real_control.json").write_text(json.dumps(real_control, indent=2) + "\n")
        report["checks"]["part_opacity_real_control"] = "passed"
        negative = FIXTURES / "inline_numbers.motion3.json"
        finite_json(negative.read_text())  # The rejected input is valid standard JSON.
        rejected = run([str(probe), command[1], str(negative), command[3]],
                       output / "inline_negative.log")
        if rejected.returncode == 0 or "Invalid Json document" not in rejected.stderr:
            raise AssertionError("Framework parser no longer rejects inline numbers")
        report["checks"]["inline_numeric_negative"] = "passed"
        report["checks"]["strict_validator_negative_controls"] = "passed"
        repeat = trace_from_run(run([str(probe), "--repeat-check", command[1]],
                                    output / "repeat.log"), "repeat")
        check_repeat(repeat)
        (output / "repeat.json").write_text(json.dumps(repeat, indent=2) + "\n")
        report["checks"]["repeat_maximum"] = "passed"
        motion_loop = trace_from_run(run([str(probe), "--motion-loop", command[1],
                                          str(FIXTURES / "loop.motion3.json")],
                                         output / "motion_loop.log"), "motion V2 loop")
        check_loop(motion_loop)
        (output / "motion_loop.json").write_text(json.dumps(motion_loop, indent=2) + "\n")
        report["checks"]["motion_v2"] = "passed"
        physics = trace_from_run(run([str(probe), "--physics-fps", command[1],
                                      str(FIXTURES / "missing.physics3.json"),
                                      str(FIXTURES / "zero.physics3.json"),
                                      str(FIXTURES / "thirty.physics3.json")],
                                     output / "physics_fps.log"), "physics fps")
        check_physics(physics)
        (output / "physics_fps.json").write_text(json.dumps(physics, indent=2) + "\n")
        report["checks"]["physics_fps"] = "passed"
        numeric = check_numeric_cases(probe, output)
        (output / "numeric.json").write_text(json.dumps(numeric, indent=2) + "\n")
        report["checks"]["numeric_parser"] = "passed"
        report["status"] = "partial"
        report["phase"] = "complete"
    except Exception as error:
        report["error"] = str(error)
    report["logs"] = {path.name: str(path) for path in sorted(output.glob("*.log"))}
    if (output / "numeric_cases").is_dir():
        report["logs"]["numeric_cases"] = str(output / "numeric_cases")
    write_report(report_path, report)
    print(f"{report['status']}: {report_path}")
    if report.get("error"):
        print(report["error"], file=sys.stderr)
    return 0 if report["status"] in {"passed", "partial"} else 1


if __name__ == "__main__":
    raise SystemExit(main())
