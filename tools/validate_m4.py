#!/usr/bin/env python3
"""Run M4's Godot lifecycle and real-GPU acceptance gates."""

import argparse
import hashlib
import json
import platform
import re
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def git(*args):
    result = subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True)
    return result.stdout.strip() if result.returncode == 0 else None


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run_gate(name, command, directory):
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
    (directory / f"{name}.log").write_text(result.stdout + result.stderr)
    report_path = directory / name / "report.json"
    try:
        report = json.loads(report_path.read_text())
    except (OSError, json.JSONDecodeError):
        report = {}
    return {
        "status": "passed" if result.returncode == 0 and report.get("status") == "passed" else "failed",
        "exit_code": result.returncode,
        "report": str(report_path),
        "log": str(directory / f"{name}.log"),
        "checks": len(report.get("checks", [])) if name == "gpu" else report.get("total_checks"),
        "details": report,
    }


def run_rust_gate(directory):
    command = ["cargo", "test", "-p", "kasane-godot", "--lib", "--locked"]
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
    log = directory / "rust.log"
    log.write_text(result.stdout + result.stderr)
    summary = re.search(r"test result: ok\. (\d+) passed; 0 failed", result.stdout)
    checks = int(summary.group(1)) if summary else 0
    return {
        "status": "passed" if result.returncode == 0 and checks > 0 else "failed",
        "exit_code": result.returncode,
        "command": command,
        "checks": checks,
        "log": str(log),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--library", type=Path, default=ROOT / "target/release/libkasane_godot.dylib")
    parser.add_argument("--godot", type=Path, default=Path("/Applications/Godot_mono.app/Contents/MacOS/Godot"))
    parser.add_argument("--output-dir", type=Path, default=ROOT / "target/kasane/m4")
    args = parser.parse_args()
    directory = args.output_dir.resolve()
    directory.mkdir(parents=True, exist_ok=True)
    library = args.library.resolve()
    report = {
        "milestone": "M4",
        "status": "failed",
        "git_revision": git("rev-parse", "HEAD"),
        "git_status": git("status", "--short"),
        "git_submodules": git("submodule", "status"),
        "platform": platform.platform(),
        "build_config": "cargo build --release -p kasane-godot --locked",
        "library": str(library),
        "library_sha256": sha256(library) if library.is_file() else None,
        "godot_executable": str(args.godot.resolve()),
        "gates": {},
    }
    if library.is_file() and args.godot.is_file():
        report["gates"]["rust"] = run_rust_gate(directory)
        gates = [
            ("gpu", ROOT / "tools/validate_gpu.py", ["--library", str(library), "--godot", str(args.godot), "--output-dir", str(directory / "gpu")]),
            ("godot", ROOT / "tools/validate_godot.py", ["--library", str(library), "--godot", str(args.godot), "--output-dir", str(directory / "godot"), "--suite", "all"]),
        ]
        for name, script, options in gates:
            report["gates"][name] = run_gate(name, [sys.executable, str(script), *options], directory)
        report["status"] = "passed" if all(g["status"] == "passed" for g in report["gates"].values()) else "failed"
        gpu = report["gates"]["gpu"]["details"]
        report["godot_version"] = gpu.get("godot")
        report["renderer"] = gpu.get("renderer")
        report["adapter"] = gpu.get("adapter")
        fixture = directory / "gpu/fixtures/publication/gpu-source.json"
        package = directory / "gpu/package"
        inputs = ([fixture] if fixture.is_file() else []) + sorted(
            p for p in package.rglob("*") if p.is_file() and p.suffix.lower() in {".moc3", ".png", ".json"}
        )
        report["input_files"] = [{"path": str(p), "sha256": sha256(p)} for p in inputs]
        moc3 = next((p for p in inputs if p.suffix.lower() == ".moc3"), None)
        report["moc3_file_version"] = moc3.read_bytes()[4] if moc3 and moc3.read_bytes()[:4] == b"MOC3" else None
    else:
        report["error"] = "Missing Godot executable or built kasane-godot library"
    (directory / "report.json").write_text(json.dumps(report, indent=2))
    print(f'M4 {report["status"]}: {directory / "report.json"}')
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
