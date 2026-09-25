#!/usr/bin/env python3
"""Rebuild a self-made CDI3 and check it through the official Framework getters."""
from __future__ import annotations

import argparse
from copy import deepcopy
import hashlib
import json
import os
import platform
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SDK = ROOT / "third_party/CubismSdkForNative-5-r.5"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    sources = sorted(item for item in path.rglob("*") if item.suffix in {".cpp", ".hpp", ".h"})
    if not sources:
        raise AssertionError(f"Framework source tree missing: {path}")
    for item in sources:
        digest.update(str(item.relative_to(path)).encode())
        digest.update(bytes.fromhex(sha256(item)))
    return digest.hexdigest()


def write_report(path: Path, data: dict[str, object]) -> None:
    temporary = path.with_suffix(".json.tmp")
    temporary.write_text(json.dumps(data, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    os.replace(temporary, path)


def run(command: list[str], log: Path, timeout: int = 120) -> subprocess.CompletedProcess[str]:
    try:
        result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        log.write_text(f"$ {' '.join(command)}\ntimeout={timeout}s\n{error.stdout}\n{error.stderr}\n")
        raise AssertionError(f"timed out after {timeout}s: {command[0]}") from error
    except OSError as error:
        log.write_text(f"$ {' '.join(command)}\nos_error={error}\n")
        raise AssertionError(f"cannot execute {command[0]}: {error}") from error
    log.write_text(f"$ {' '.join(command)}\nexit={result.returncode}\n[stdout]\n{result.stdout}\n[stderr]\n{result.stderr}")
    return result


def strict_object(text: str) -> dict[str, object]:
    def no_duplicate(pairs: list[tuple[str, object]]) -> dict[str, object]:
        result: dict[str, object] = {}
        for key, value in pairs:
            if key in result:
                raise AssertionError(f"duplicate JSON member: {key}")
            result[key] = value
        return result

    def no_constant(value: str) -> object:
        raise AssertionError(f"non-finite JSON constant: {value}")

    parsed = json.loads(text, object_pairs_hook=no_duplicate, parse_constant=no_constant)
    if type(parsed) is not dict:
        raise AssertionError("expected JSON object")
    return parsed


def check_fixture(wire: dict[str, object]) -> dict[str, object]:
    if type(wire.get("Version")) is not int or wire["Version"] != 3:
        raise AssertionError("Rust writer did not produce CDI3 Version 3")
    if set(wire) != {"Version", "Parameters", "ParameterGroups", "Parts", "CombinedParameters", "Future"}:
        raise AssertionError("unexpected CDI root fields")
    parameters = wire["Parameters"]
    if type(parameters) is not list or len(parameters) != 2:
        raise AssertionError("self-made fixture lost parameter entries")
    expected_name = '角度\n"X"\\位置'
    if parameters != [
        {"Id": "ParamAngleX", "GroupId": "Face", "Name": expected_name, "Hint": "追加\t説明"},
        {"Id": "ParamAngleY", "GroupId": "Face", "Name": expected_name},
    ]:
        raise AssertionError("Rust CDI parameters changed")
    if wire["ParameterGroups"] != [{"Id": "Face", "GroupId": "", "Name": "顔"}]:
        raise AssertionError("Rust CDI groups changed")
    if wire["Parts"] != [{"Id": "PartHair", "Name": "头发"}]:
        raise AssertionError("Rust CDI parts changed")
    if wire["CombinedParameters"] != [["ParamAngleX", "ParamAngleY"]]:
        raise AssertionError("Rust CDI combination changed")
    if wire["Future"] != {"Label": "扩展", "Enabled": True}:
        raise AssertionError("Rust CDI unknown fields changed")
    return {
        "accepted": True,
        "version": 3,
        "parameters": [
            {"id": "ParamAngleX", "group_id": "Face", "name": expected_name},
            {"id": "ParamAngleY", "group_id": "Face", "name": expected_name},
        ],
        "parameter_groups": [{"id": "Face", "group_id": "", "name": "顔"}],
        "parts": [{"id": "PartHair", "name": "头发"}],
        "combined_parameters": [["ParamAngleX", "ParamAngleY"]],
        "parameter_hint": "追加\t説明",
        "future_label": "扩展",
        "future_enabled": True,
    }


def check_observed(observed: object, expected: object, path: str = "$") -> None:
    if type(observed) is not type(expected):
        raise AssertionError(f"wrong Framework trace type at {path}")
    if isinstance(expected, dict):
        if set(observed) != set(expected):
            raise AssertionError(f"wrong Framework trace keys at {path}")
        for key, value in expected.items():
            check_observed(observed[key], value, f"{path}.{key}")
    elif isinstance(expected, list):
        if len(observed) != len(expected):
            raise AssertionError(f"wrong Framework trace length at {path}")
        for index, value in enumerate(expected):
            check_observed(observed[index], value, f"{path}[{index}]")
    elif observed != expected:
        raise AssertionError(f"wrong Framework trace value at {path}: {observed!r}")


def check_validator_negative_controls(expected: dict[str, object]) -> None:
    variants = []
    for replacement in (False, "3", None):
        candidate = deepcopy(expected)
        candidate["version"] = replacement
        variants.append(candidate)
    wrong_id = deepcopy(expected)
    wrong_id["combined_parameters"][0][1] = "OtherParam"
    variants.append(wrong_id)
    truncated = deepcopy(expected)
    truncated["parameters"].pop()
    variants.append(truncated)
    missing = deepcopy(expected)
    del missing["future_label"]
    variants.append(missing)
    for candidate in variants:
        try:
            check_observed(candidate, expected)
        except AssertionError:
            continue
        raise AssertionError("strict result validator accepted a negative control")
    for invalid in ('{"accepted":true', '{"x":NaN}', '{"x":1,"x":2}'):
        try:
            strict_object(invalid)
        except (AssertionError, json.JSONDecodeError):
            continue
        raise AssertionError("strict result parser accepted a negative control")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", type=Path, default=ROOT / "target/cdi-probe-results")
    parser.add_argument("--build-dir", type=Path, default=ROOT / "target/cdi-probe-build")
    args = parser.parse_args()
    output = args.output_dir.resolve()
    build = args.build_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report_path = output / "report.json"
    report: dict[str, object] = {"status": "failed", "phase": "incomplete", "checks": {}}
    write_report(report_path, report)
    for name in ("configure.log", "build.log", "fixture.log", "official_read.log",
                 "official_control.log", "fixture.cdi3.json", "official_read.json",
                 "unsupported_control.cdi3.json"):
        (output / name).unlink(missing_ok=True)
    try:
        system = platform.system().lower()
        machine = platform.machine()
        if system not in {"darwin", "linux"}:
            raise AssertionError(f"unsupported official Core platform: {system}")
        core_platform = "macos" if system == "darwin" else "linux"
        core = SDK / "Core/lib" / core_platform / machine / "libLive2DCubismCore.a"
        if not core.is_file():
            raise AssertionError(f"official Core unavailable: {core}")
        report["provenance"] = {
            "framework_tree_sha256": source_sha256(SDK / "Framework/src"),
            "core_headers_sha256": source_sha256(SDK / "Core/include"),
            "core_path": str(core),
            "core_sha256": sha256(core),
            "cmake_sha256": sha256(ROOT / "tools/probes/CMakeLists.txt"),
            "probe_sha256": sha256(ROOT / "tools/probes/framework_cdi_cpu_probe.cpp"),
            "shim_sha256": sha256(ROOT / "tools/probes/framework_cpu_renderer_cleanup.cpp"),
            "runner_sha256": sha256(Path(__file__)),
            "crate_sha256": sha256(ROOT / "modules/kasane-live2d/src/cdi3.rs"),
            "fixture_source_sha256": sha256(ROOT / "modules/kasane-live2d/examples/cdi_fixture.rs"),
        }
        configure = run(["cmake", "-S", "tools/probes", "-B", str(build),
                         f"-DKASANE_CUBISM_ROOT={SDK}", "-DCMAKE_BUILD_TYPE=Release"],
                        output / "configure.log")
        if configure.returncode:
            raise AssertionError("CMake configure failed")
        compiled = run(["cmake", "--build", str(build), "--target",
                        "kasane_framework_cdi_cpu_probe", "-j", "4"], output / "build.log")
        if compiled.returncode:
            raise AssertionError("official CDI CPU probe build failed")
        probe = build / "kasane_framework_cdi_cpu_probe"
        report["provenance"]["probe_binary_sha256"] = sha256(probe)
        fixture = run(["cargo", "run", "--locked", "-q", "-p", "kasane-live2d",
                       "--example", "cdi_fixture"], output / "fixture.log")
        if fixture.returncode:
            raise AssertionError("Rust CDI fixture generation failed")
        wire = strict_object(fixture.stdout)
        expected = check_fixture(wire)
        check_validator_negative_controls(expected)
        fixture_path = output / "fixture.cdi3.json"
        fixture_path.write_text(fixture.stdout)
        report["fixture_sha256"] = sha256(fixture_path)
        observed_run = run([str(probe), str(fixture_path)], output / "official_read.log")
        if observed_run.returncode:
            raise AssertionError("official CDI read failed")
        observed = strict_object(observed_run.stdout)
        check_observed(observed, expected)
        (output / "official_read.json").write_text(json.dumps(observed, ensure_ascii=False, indent=2) + "\n")
        report["checks"] = {"rust_fixture": "passed", "framework_cdi_getters": "passed",
                            "utf8_and_supported_escapes": "passed", "unknown_text_and_bool": "passed"}

        # Valid standard JSON whose U+0001 escape is deliberately outside this
        # writer's verified subset. The official parser should reject it.
        control = dict(wire)
        control["Parts"] = [{"Id": "PartHair", "Name": "bad\u0001"}]
        control_path = output / "unsupported_control.cdi3.json"
        control_path.write_text(json.dumps(control, ensure_ascii=False, indent=2))
        strict_object(control_path.read_text())
        rejected_run = run([str(probe), str(control_path)], output / "official_control.log")
        if rejected_run.returncode:
            raise AssertionError("official Framework unexpectedly accepted U+0001 escape")
        check_observed(strict_object(rejected_run.stdout), {"accepted": False})
        report["checks"]["unsupported_control_rejected"] = "passed"
        report["status"] = "passed"
        report["phase"] = "complete"
    except Exception as error:
        report["error"] = f"{type(error).__name__}: {error}"
        write_report(report_path, report)
        print(report["error"], file=sys.stderr)
        return 1
    write_report(report_path, report)
    print(f"CDI3 official Framework check passed: {report_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
