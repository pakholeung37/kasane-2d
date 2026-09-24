#!/usr/bin/env python3
"""Build and run the Cubism Core provider/host benchmark matrix."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import sys


MATRIX_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = MATRIX_ROOT.parents[1]
sys.path.insert(0, str(REPO_ROOT / "tools"))

from stage_godot_addon import stage_addon


MATRIX_PATH = MATRIX_ROOT / "config" / "matrix.json"
WORKLOAD_PATH = MATRIX_ROOT / "config" / "mao-40.json"
BUILD_ROOT = REPO_ROOT / "target/cubism-matrix/build"
SDK_ROOT = REPO_ROOT / "third_party/CubismSdkForNative-5-r.5"
PURISM_ROOT = Path(
    os.environ.get("PURISM_CORE_ROOT", REPO_ROOT / "modules/purism-core")
).expanduser().resolve()
PURISM_BUILD_ROOT = REPO_ROOT / "target/cubism-matrix/core/purism-v6"
MAO_SOURCE = REPO_ROOT / "models/local/mao"
MAO_MOC3 = MAO_SOURCE / "runtime/mao_pro.moc3"


def read_json(path: Path) -> dict:
    with path.open(encoding="utf-8") as file:
        return json.load(file)


def load_configuration() -> tuple[dict, dict]:
    matrix = read_json(MATRIX_PATH)
    workload = read_json(WORKLOAD_PATH)
    providers = {"cubism", "purism"}
    hosts = {"cubism-framework-native", "gd-cubism", "core-only"}
    expected = {(provider, host) for provider in providers for host in hosts}
    cases = matrix.get("cases", [])
    actual = {(case.get("core"), case.get("host")) for case in cases}
    ids = [case.get("id") for case in cases]
    if actual != expected or len(ids) != len(expected) or len(set(ids)) != len(expected):
        raise ValueError("matrix.json must contain each Core/host combination exactly once")
    if workload.get("instances") != workload["layout"]["columns"] * workload["layout"]["rows"]:
        raise ValueError("workload layout must have exactly one grid cell per instance")
    if len(workload.get("viewport", [])) != 2:
        raise ValueError("workload viewport must contain width and height")
    return matrix, workload


def find_case(case_id: str) -> tuple[dict, dict]:
    matrix, workload = load_configuration()
    for case in matrix["cases"]:
        if case["id"] == case_id:
            return case, workload
    choices = ", ".join(case["id"] for case in matrix["cases"])
    raise ValueError(f"unknown case {case_id!r}; choose one of: {choices}")


def run(command: list[str], *, cwd: Path = REPO_ROOT, env: dict | None = None) -> None:
    print("+", " ".join(command), flush=True)
    subprocess.run(command, cwd=cwd, env=env, check=True)


def run_capture(command: list[str], *, cwd: Path = REPO_ROOT) -> str:
    print("+", " ".join(command), flush=True)
    completed = subprocess.run(
        command, cwd=cwd, check=True, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    print(completed.stdout, end="", flush=True)
    return completed.stdout


def replace_tree(source: Path, destination: Path) -> None:
    if not source.is_dir():
        raise FileNotFoundError(f"required directory does not exist: {source}")
    if destination.exists():
        shutil.rmtree(destination)
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source, destination)


def select_core_variant_for_editor(
    addon_root: Path, core_provider: str, platform: str, arch: str
) -> None:
    """Point the descriptor at one coexisting Core-specific release library.

    The benchmark runs through the Godot editor executable, which selects the
    debug GDExtension entry even when the native extension was built with
    target=template_release. Without this rewrite an old debug binary in the
    addon can silently defeat Core-provider isolation.
    """
    descriptor = addon_root / "gd_cubism.gdextension"
    text = descriptor.read_text(encoding="utf-8")
    suffix = "" if platform == "macos" else f".{arch}"
    debug_key = f"{platform}.debug{suffix}"
    release_key = f"{platform}.release{suffix}"
    lines = text.splitlines()
    default_release_value = next(
        (
            line.split("=", 1)[1].strip()
            for line in lines
            if line.split("=", 1)[0].strip() == release_key
        ),
        None,
    )
    if default_release_value is None:
        raise ValueError(f"missing {release_key} in {descriptor}")
    release_value = default_release_value.replace(
        ".cubism.", f".{core_provider}."
    )
    library_path = addon_root / release_value.strip('"')
    if not library_path.exists():
        raise FileNotFoundError(
            f"missing {core_provider} extension library: {library_path}"
        )
    replaced = set()
    for index, line in enumerate(lines):
        key = line.split("=", 1)[0].strip()
        if key in (debug_key, release_key):
            lines[index] = f"{key} = {release_value}"
            replaced.add(key)
    missing = {debug_key, release_key} - replaced
    if missing:
        raise ValueError(f"missing {', '.join(sorted(missing))} in {descriptor}")
    descriptor.write_text("\n".join(lines) + "\n", encoding="utf-8")


def prepare_model(model_source: Path | None = None) -> Path:
    destination = MATRIX_ROOT / "assets/live2d/mao"
    source = model_source or MAO_SOURCE
    if source.resolve() != destination.resolve():
        replace_tree(source, destination)
    model3 = destination / "runtime/mao_pro.model3.json"
    if not model3.is_file():
        raise FileNotFoundError(f"Mao model is missing: {model3}")
    return model3


def prepare_godot(addon_source: Path | None = None, model_source: Path | None = None) -> None:
    addon_source = addon_source or REPO_ROOT / "modules/gd-cubism/addons/gd_cubism"
    stage_addon(MATRIX_ROOT, addon_source)
    prepare_model(model_source)
    print(f"prepared isolated Godot project at {MATRIX_ROOT}")



def build_purism_core(jobs: int) -> Path:
    if not (PURISM_ROOT / "CMakeLists.txt").is_file():
        raise FileNotFoundError(
            f"PurismCore checkout not found at {PURISM_ROOT}; set PURISM_CORE_ROOT"
        )
    run(
        [
            "cmake",
            "-S",
            str(PURISM_ROOT),
            "-B",
            str(PURISM_BUILD_ROOT),
            "-DCMAKE_BUILD_TYPE=Release",
            "-DBUILD_SHARED_LIBS=OFF",
            "-DPURISM_CORE_ABI=v6",
            "-DPURISM_CORE_BUILD_TESTS=ON",
        ]
    )
    run(["cmake", "--build", str(PURISM_BUILD_ROOT), f"-j{jobs}"])
    archive = PURISM_BUILD_ROOT / "libPurismCore.a"
    if not archive.is_file():
        raise FileNotFoundError(f"CMake did not produce {archive}")
    return archive


def cmake_workload_arguments(workload: dict, model_hash: str) -> list[str]:
    width, height = workload["viewport"]
    timing = workload["timing"]
    layout = workload["layout"]
    return [
        f"-DBENCHMARK_MODEL_COUNT={workload['instances']}",
        f"-DBENCHMARK_WORKLOAD_ID={workload['id']}",
        f"-DBENCHMARK_MODEL_NAME={workload['model']}",
        f"-DBENCHMARK_MODEL_HASH={model_hash}",
        f"-DBENCHMARK_COLUMNS={layout['columns']}",
        f"-DBENCHMARK_ROWS={layout['rows']}",
        f"-DBENCHMARK_WIDTH={width}",
        f"-DBENCHMARK_HEIGHT={height}",
        f"-DBENCHMARK_WARMUP_SECONDS={timing['warmup_seconds']}",
        f"-DBENCHMARK_SAMPLE_SECONDS={timing['sample_seconds']}",
    ]


def build_native(case_id: str, jobs: int) -> Path:
    case, workload = find_case(case_id)
    if case["host"] != "cubism-framework-native":
        raise ValueError(f"{case_id} is not a Native case")
    third_party = SDK_ROOT / "Samples/OpenGL/thirdParty"
    missing = [path for path in (third_party / "glew/build/cmake", third_party / "glfw") if not path.is_dir()]
    if missing:
        setup = third_party / "scripts/setup_glew_glfw"
        raise FileNotFoundError(
            "Native OpenGL dependencies are not prepared; run:\n"
            f"cd {setup.parent} && ./setup_glew_glfw"
        )
    if case["core"] == "purism":
        build_purism_core(jobs)
    model_path = prepare_model()
    model_hash = hashlib.sha256(model_path.read_bytes()).hexdigest()
    build_dir = BUILD_ROOT / case_id / "native"
    source_dir = MATRIX_ROOT / "runners/native"
    command = [
        "cmake", "-S", str(source_dir), "-B", str(build_dir),
        "-DCMAKE_BUILD_TYPE=Release",
        "-DCMAKE_POLICY_VERSION_MINIMUM=3.5",
        "-DCSM_MINIMUM_DEMO=OFF",
        f"-DSDK_ROOT_PATH={SDK_ROOT}",
        f"-DCORE_PROVIDER={case['core']}",
        f"-DPURISM_CORE_LIBRARY={PURISM_BUILD_ROOT / 'libPurismCore.a'}",
        f"-DBENCHMARK_CASE_ID={case_id}",
        *cmake_workload_arguments(workload, model_hash),
    ]
    run(command)
    run(["cmake", "--build", str(build_dir), f"-j{jobs}"])
    executable = build_dir / "bin/Demo/Demo"
    if not executable.is_file():
        raise FileNotFoundError(f"Native build did not produce {executable}")
    return executable


def build_core(case_id: str, jobs: int) -> Path:
    case, workload = find_case(case_id)
    if case["host"] != "core-only":
        raise ValueError(f"{case_id} is not a Core-only case")
    if case["core"] == "purism":
        build_purism_core(jobs)
    build_dir = BUILD_ROOT / case_id / "core"
    source_dir = MATRIX_ROOT / "runners/core"
    command = [
        "cmake", "-S", str(source_dir), "-B", str(build_dir),
        "-DCMAKE_BUILD_TYPE=Release",
        f"-DSDK_ROOT_PATH={SDK_ROOT}",
        f"-DCORE_PROVIDER={case['core']}",
        f"-DPURISM_CORE_LIBRARY={PURISM_BUILD_ROOT / 'libPurismCore.a'}",
        f"-DBENCHMARK_CASE_ID={case_id}",
        f"-DBENCHMARK_MODEL_COUNT={workload['instances']}",
    ]
    run(command)
    run(["cmake", "--build", str(build_dir), f"-j{jobs}"])
    executable = build_dir / "cubism-core-benchmark"
    if not executable.is_file():
        raise FileNotFoundError(f"Core-only build did not produce {executable}")
    return executable


def build_godot(case_id: str, jobs: int, platform: str, arch: str) -> Path:
    case, _ = find_case(case_id)
    if case["host"] != "gd-cubism":
        raise ValueError(f"{case_id} is not a Godot case")
    extension_root = REPO_ROOT / "modules/gd-cubism"
    scons_python = extension_root / ".venv/bin/python"
    if not scons_python.is_file():
        raise FileNotFoundError(
            f"SCons environment does not exist: {scons_python.parent}"
        )
    environment = os.environ.copy()
    environment["CUBISM_SDK_ROOT"] = str(SDK_ROOT)
    environment["CUBISM_CORE_PROVIDER"] = case["core"]
    if case["core"] == "purism":
        environment["CUBISM_CORE_LIBRARY"] = str(build_purism_core(jobs))
    else:
        environment.pop("CUBISM_CORE_LIBRARY", None)
    run(
        [
            str(scons_python), "-m", "SCons",
            f"platform={platform}", f"arch={arch}",
            "target=template_release", f"-j{jobs}",
        ],
        cwd=extension_root,
        env=environment,
    )
    addon_source = extension_root / "addons/gd_cubism"
    artifact = BUILD_ROOT / case_id / "addons/gd_cubism"
    replace_tree(addon_source, artifact)
    select_core_variant_for_editor(artifact, case["core"], platform, arch)
    prepare_godot(artifact)
    return artifact


def run_case(case_id: str, godot_bin: str) -> None:
    case, _ = find_case(case_id)
    if case["host"] == "core-only":
        executable = BUILD_ROOT / case_id / "core/cubism-core-benchmark"
        if not executable.is_file():
            raise FileNotFoundError(f"build {case_id} before running it")
        run([str(executable), str(MAO_MOC3)], cwd=executable.parent)
        return
    if case["host"] == "cubism-framework-native":
        executable = BUILD_ROOT / case_id / "native/bin/Demo/Demo"
        if not executable.is_file():
            raise FileNotFoundError(f"build {case_id} before running it")
        run([str(executable)], cwd=executable.parent)
        return
    addon_artifact = BUILD_ROOT / case_id / "addons/gd_cubism"
    if not addon_artifact.is_dir():
        raise FileNotFoundError(f"build {case_id} before running it")
    prepare_godot(addon_artifact)
    run(
        [
            godot_bin, "--path", str(MATRIX_ROOT),
            "res://runners/godot/benchmark.tscn", "--",
            f"--case={case_id}", f"--core={case['core']}", "--profile=release",
        ]
    )


def parse_benchmark_result(output: str) -> dict:
    marker = "BENCHMARK_RESULT "
    position = output.rfind(marker)
    if position < 0:
        raise ValueError("benchmark output did not contain BENCHMARK_RESULT")
    return json.loads(output[position + len(marker):])


def benchmark_core(repeats: int, jobs: int) -> Path:
    if repeats < 1:
        raise ValueError("repeats must be at least 1")
    case_ids = ["cubism-core", "purism-core"]
    executables = {case_id: build_core(case_id, jobs) for case_id in case_ids}
    trials: dict[str, list[dict]] = {case_id: [] for case_id in case_ids}
    for repeat in range(repeats):
        order = case_ids if repeat % 2 == 0 else list(reversed(case_ids))
        for case_id in order:
            executable = executables[case_id]
            output = run_capture([str(executable), str(MAO_MOC3)], cwd=executable.parent)
            trials[case_id].append(parse_benchmark_result(output))

    phase_names = list(trials[case_ids[0]][0]["phases"])
    medians: dict[str, dict] = {}
    for case_id in case_ids:
        case_trials = trials[case_id]
        medians[case_id] = {
            "startup": {
                key: statistics.median(trial["startup"][key] for trial in case_trials)
                for key in ("copy_ns", "consistency_with_copy_ns", "revive_with_copy_ns", "initialize_ns")
            },
            "phases": {
                phase: statistics.median(
                    trial["phases"][phase]["ns_per_operation"] for trial in case_trials
                )
                for phase in phase_names
            },
            "working_set_ns_per_model": statistics.median(
                trial["working_set_ns_per_model"] for trial in case_trials
            ),
            "working_set_model_updates_per_second": statistics.median(
                trial["working_set_model_updates_per_second"] for trial in case_trials
            ),
        }

    report = {
        "schema_version": 1,
        "benchmark": "cubism-core-only",
        "model": "mao_pro",
        "repeats": repeats,
        "trials": trials,
        "medians": medians,
    }
    output_path = MATRIX_ROOT / "artifacts/results/latest-core-only.json"
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    baseline = medians["cubism-core"]["working_set_ns_per_model"]
    print("\nCore-only median summary (lower ns/model is better):")
    for case_id in case_ids:
        ns_per_model = medians[case_id]["working_set_ns_per_model"]
        print(
            f"  {case_id:13} {ns_per_model:10.1f} ns/model  "
            f"{baseline / ns_per_model:6.2f}x vs official"
        )
    print(f"saved {output_path}")
    return output_path


def validate(local: bool) -> None:
    matrix, workload = load_configuration()
    print(f"configuration valid: {len(matrix['cases'])} cases, workload={workload['id']}")
    if local:
        required = [
            SDK_ROOT,
            PURISM_ROOT / "CMakeLists.txt",
            MAO_SOURCE / "runtime/mao_pro.model3.json",
            SDK_ROOT / "Samples/OpenGL/thirdParty/glew/build/cmake",
            SDK_ROOT / "Samples/OpenGL/thirdParty/glfw",
        ]
        missing = [path for path in required if not path.exists()]
        if missing:
            raise FileNotFoundError("missing local prerequisites:\n" + "\n".join(map(str, missing)))
        print("local SDK and Mao fixture found")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    validate_parser = subparsers.add_parser("validate")
    validate_parser.add_argument("--local", action="store_true")
    prepare_parser = subparsers.add_parser("prepare-godot")
    prepare_parser.add_argument("--addon-source", type=Path)
    prepare_parser.add_argument("--model-source", type=Path)
    for name in ("build-native", "build-godot", "build-core"):
        build_parser = subparsers.add_parser(name)
        build_parser.add_argument("case")
        build_parser.add_argument("--jobs", type=int, default=os.cpu_count() or 4)
        if name == "build-godot":
            build_parser.add_argument("--platform", default="macos")
            build_parser.add_argument("--arch", default="arm64")
    run_parser = subparsers.add_parser("run")
    run_parser.add_argument("case")
    run_parser.add_argument(
        "--godot-bin",
        default=os.environ.get("GODOT_BIN", "/Applications/Godot_mono.app/Contents/MacOS/Godot"),
    )
    benchmark_core_parser = subparsers.add_parser("benchmark-core")
    benchmark_core_parser.add_argument("--repeats", type=int, default=3)
    benchmark_core_parser.add_argument("--jobs", type=int, default=os.cpu_count() or 4)
    args = parser.parse_args()
    try:
        if args.command == "validate":
            validate(args.local)
        elif args.command == "prepare-godot":
            prepare_godot(args.addon_source, args.model_source)
        elif args.command == "build-native":
            build_native(args.case, args.jobs)
        elif args.command == "build-godot":
            build_godot(args.case, args.jobs, args.platform, args.arch)
        elif args.command == "build-core":
            build_core(args.case, args.jobs)
        elif args.command == "run":
            run_case(args.case, args.godot_bin)
        elif args.command == "benchmark-core":
            benchmark_core(args.repeats, args.jobs)
    except (FileNotFoundError, ValueError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
