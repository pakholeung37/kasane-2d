#!/usr/bin/env python3
"""Run the Kasane preview benchmark against one built GDExtension."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import statistics
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
FIXTURE_ROOT = ROOT / "tests/fixtures/gpu"
BENCHMARK_SCRIPT = ROOT / "benchmarks/kasane-preview/benchmark.gd"
OFFSCREEN_BENCHMARK_SCRIPT = ROOT / "benchmarks/kasane-preview/offscreen_benchmark.gd"
REN_MODEL3 = (
    ROOT
    / "third_party/CubismSdkForNative-5-r.5/Samples/Resources/Ren/Ren.model3.json"
)


def default_library() -> Path:
    release = ROOT / "target/release/libkasane_godot.dylib"
    if release.is_file():
        return release
    return ROOT / "target/debug/libkasane_godot.dylib"


def stage_project(project: Path, library: Path, workload: str) -> None:
    project.mkdir(parents=True, exist_ok=True)
    if workload == "gpu-fixture":
        fixture = json.loads((FIXTURE_ROOT / "gpu-source.json").read_text())
        fixture["format"] = "kasane-directory-project"
        fixture["format_version"] = 1
        assets_dir = project / "assets"
        assets_dir.mkdir(parents=True, exist_ok=True)
        for asset in fixture["document"]["assets"]:
            source = FIXTURE_ROOT / "gpu-package" / asset["source"]
            data = source.read_bytes()
            digest = hashlib.sha256(data).hexdigest()
            asset["sha256"] = digest
            asset["source"] = f"assets/{digest}.png"
            (project / asset["source"]).write_bytes(data)
        (project / "gpu-source.json").write_text(json.dumps(fixture) + "\n")
        shutil.copyfile(BENCHMARK_SCRIPT, project / "benchmark.gd")
    elif workload == "ren-offscreen":
        shutil.copyfile(OFFSCREEN_BENCHMARK_SCRIPT, project / "benchmark.gd")
    else:
        raise ValueError(f"unknown benchmark workload: {workload}")
    shutil.copyfile(library, project / library.name)

    (project / "project.godot").write_text(
        "config_version=5\n"
        "[application]\n"
        "config/name=\"Kasane Preview Benchmark\"\n"
        "[display]\n"
        "window/size/viewport_width=640\n"
        "window/size/viewport_height=480\n"
        "[rendering]\n"
        "renderer/rendering_method=\"gl_compatibility\"\n"
        "textures/default_filters/use_nearest_mipmap_filter=false\n"
    )
    (project / ".godot").mkdir(exist_ok=True)
    (project / "kasane.gdextension").write_text(
        "[configuration]\n"
        "entry_symbol=\"kasane_gd_library_init\"\n"
        "compatibility_minimum=\"4.3\"\n"
        "[libraries]\n"
        f"macos.debug.arm64=\"res://{library.name}\"\n"
    )
    (project / ".godot/extension_list.cfg").write_text("res://kasane.gdextension\n")


def run_once(
    godot: Path,
    project: Path,
    workload: str,
    model3: Path | None,
    warmup_frames: int,
    sample_frames: int,
    repeat: int,
) -> dict:
    log_path = project / f"benchmark-{repeat}.log"
    user_args = [
        str(project),
        f"--warmup-frames={warmup_frames}",
        f"--sample-frames={sample_frames}",
    ]
    if workload == "ren-offscreen":
        assert model3 is not None
        user_args.append(f"--model3={model3}")
    command = [
        str(godot),
        "--path",
        str(project),
        "--rendering-method",
        "gl_compatibility",
        "--resolution",
        "640x480",
        "--script",
        "res://benchmark.gd",
        "--",
        *user_args,
    ]
    result = subprocess.run(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        timeout=180,
    )
    log_path.write_text(result.stdout)
    lines = [line for line in result.stdout.splitlines() if line.startswith("BENCHMARK_RESULT ")]
    if result.returncode != 0 or not lines:
        raise RuntimeError(f"benchmark run {repeat} failed; see {log_path}\n{result.stdout[-4000:]}")
    return json.loads(lines[-1].removeprefix("BENCHMARK_RESULT "))


def median_summary(runs: list[dict]) -> dict:
    summary = {}
    for metric in ("refresh_cpu_ms", "frame_ms"):
        summary[metric] = {}
        for percentile in ("mean", "p50", "p95", "p99"):
            summary[metric][percentile] = statistics.median(
                run[metric][percentile] for run in runs
            )
    return summary


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--godot",
        type=Path,
        default=Path("/Applications/Godot_mono.app/Contents/MacOS/Godot"),
    )
    parser.add_argument("--library", type=Path, default=default_library())
    parser.add_argument(
        "--output-dir", type=Path, default=ROOT / "target/kasane-preview-benchmark"
    )
    parser.add_argument(
        "--workload",
        choices=("gpu-fixture", "ren-offscreen"),
        default="gpu-fixture",
        help="real Godot workload to measure",
    )
    parser.add_argument("--model3", type=Path, default=REN_MODEL3)
    parser.add_argument("--label", default="current")
    parser.add_argument("--warmup-frames", type=int, default=60)
    parser.add_argument("--sample-frames", type=int, default=300)
    parser.add_argument("--repeats", type=int, default=3)
    args = parser.parse_args()

    godot = args.godot.resolve()
    library = args.library.resolve()
    model3 = args.model3.resolve()
    project = args.output_dir.resolve()
    if not godot.is_file():
        print(f"Missing Godot executable: {godot}", file=sys.stderr)
        return 2
    if not library.is_file():
        print(f"Missing GDExtension library: {library}", file=sys.stderr)
        return 2
    if args.workload == "gpu-fixture" and not (FIXTURE_ROOT / "gpu-source.json").is_file():
        print(f"Missing benchmark fixture: {FIXTURE_ROOT}", file=sys.stderr)
        return 2
    if args.workload == "ren-offscreen" and not model3.is_file():
        print(f"Missing Ren model3 fixture: {model3}", file=sys.stderr)
        return 2

    stage_project(project, library, args.workload)
    runs = [
        run_once(
            godot,
            project,
            args.workload,
            model3 if args.workload == "ren-offscreen" else None,
            max(args.warmup_frames, 1),
            max(args.sample_frames, 1),
            repeat,
        )
        for repeat in range(max(args.repeats, 1))
    ]
    report = {
        "schema_version": 1,
        "benchmark": "kasane-preview",
        "workload": args.workload,
        "label": args.label,
        "library": str(library),
        "godot": str(godot),
        "warmup_frames": max(args.warmup_frames, 1),
        "sample_frames": max(args.sample_frames, 1),
        "repeats": len(runs),
        "model3": str(model3) if args.workload == "ren-offscreen" else None,
        "runs": runs,
        "median": median_summary(runs),
    }
    report_path = project / f"{args.label}.json"
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    (project / "latest.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
