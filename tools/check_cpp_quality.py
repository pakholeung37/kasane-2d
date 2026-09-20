#!/usr/bin/env python3
"""Pinned clang-format and selected clang-tidy checks for owned code."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
VERSION = "22.1.8"


def executable(name, override):
    if override:
        path = Path(override)
    else:
        path = Path(shutil.which(name) or "")
        if not path.is_file():
            for prefix in ("/opt/homebrew/opt/llvm@22", "/usr/local/opt/llvm@22"):
                candidate = Path(prefix) / "bin" / name
                if candidate.is_file():
                    path = candidate
                    break
    if not path.is_file():
        raise RuntimeError(f"{name} not found; install LLVM {VERSION}")
    result = subprocess.run([str(path), "--version"], text=True, capture_output=True, check=True)
    if VERSION not in result.stdout:
        raise RuntimeError(f"Expected {name} {VERSION}, got: {result.stdout.strip()}")
    return path.resolve()


def owned_sources():
    paths = set()
    for base in (ROOT / "modules/kasane-core", ROOT / "modules/kasane-document", ROOT / "modules/gd-kasane/src"):
        for extension in ("*.cpp", "*.hpp"):
            paths.update(path for path in base.rglob(extension) if "vendor" not in path.parts)
    purism = ROOT / "modules/purism-core"
    tracked = subprocess.check_output(["git", "ls-files", "-z", "*.c", "*.h"], cwd=purism)
    for raw in tracked.split(b"\0"):
        if raw:
            path = purism / os.fsdecode(raw)
            if "vendor" not in path.parts:
                paths.add(path)
    return sorted(path for path in paths if path.is_file())


def check_format(clang_format, files):
    failed = []
    for path in files:
        relative = path.relative_to(ROOT).as_posix()
        result = subprocess.run([str(clang_format), "-style=file", "-output-replacements-xml", str(path)],
                                text=True, capture_output=True, check=True)
        count = len(ET.fromstring(result.stdout).findall("replacement"))
        if count:
            failed.append(f"{relative}: {count} format edits required")
    if failed:
        raise RuntimeError("Formatting required:\n" + "\n".join(failed))
    print(f"format: {len(files)} owned files checked; no formatting edits required")


def check_tidy(clang_tidy, build_dir, godot_build_dir):
    database = build_dir / "compile_commands.json"
    if not database.is_file():
        raise RuntimeError(f"Missing {database}; configure a core preset first")
    godot_database = godot_build_dir / "compile_commands.json"
    if not godot_database.is_file():
        raise RuntimeError(f"Missing {godot_database}; run SCons compiledb first")
    groups = [
        (build_dir, sorted((ROOT / "modules/kasane-core/src").glob("*.cpp")) +
         sorted((ROOT / "modules/kasane-core/tests").glob("*.cpp")) +
         sorted((ROOT / "modules/kasane-document/src").glob("*.cpp")) +
         sorted((ROOT / "modules/kasane-document/tests").glob("*.cpp"))),
        (godot_build_dir, sorted((ROOT / "modules/gd-kasane/src").glob("*.cpp"))),
    ]
    extra = []
    if sys.platform == "darwin":
        sdk = subprocess.check_output(["xcrun", "--show-sdk-path"], text=True).strip()
        extra = [f"--extra-arg=-isystem{sdk}/usr/include/c++/v1", f"--extra-arg=-isysroot{sdk}"]
    for database_dir, files in groups:
        for path in files:
            result = subprocess.run([str(clang_tidy), "-p", str(database_dir), str(path), "--quiet",
                                     "--warnings-as-errors=*", *extra], text=True, capture_output=True)
            if result.returncode or result.stdout.strip() or result.stderr.strip():
                raise RuntimeError(f"clang-tidy failed on {path.relative_to(ROOT)}:\n{result.stdout}{result.stderr}")
    print(f"tidy: {sum(len(files) for _, files in groups)} Kasane and Godot translation units passed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--format-only", action="store_true")
    parser.add_argument("--tidy-only", action="store_true")
    parser.add_argument("--compile-commands", type=Path, default=ROOT / "target/cmake/core-debug")
    parser.add_argument("--godot-compile-commands", type=Path, default=ROOT / "modules/gd-kasane")
    parser.add_argument("--clang-format", default=os.environ.get("CLANG_FORMAT"))
    parser.add_argument("--clang-tidy", default=os.environ.get("CLANG_TIDY"))
    args = parser.parse_args()
    try:
        if not args.tidy_only:
            check_format(executable("clang-format", args.clang_format), owned_sources())
        if not args.format_only:
            check_tidy(executable("clang-tidy", args.clang_tidy), args.compile_commands.resolve(),
                       args.godot_compile_commands.resolve())
    except (RuntimeError, subprocess.CalledProcessError, ET.ParseError) as exc:
        print(exc, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
