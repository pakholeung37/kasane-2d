"""One-shot script runner with a machine-readable result report."""

from __future__ import annotations

import argparse
from contextlib import redirect_stderr, redirect_stdout
import io
import json
from pathlib import Path
import sys
import traceback

from . import _sessions


def _snapshot_sessions() -> list[dict]:
    return [
        {
            "version": list(session.version),
            "project_path": str(session.project_path) if session.project_path else None,
            "modified": session.modified,
        }
        for session in list(_sessions)
    ]


def run(script: Path, report: Path) -> int:
    script = script.resolve()
    report = report.resolve()
    output = io.StringIO()
    errors = io.StringIO()
    failure = None
    namespace = {"__name__": "__main__", "__file__": str(script), "__package__": None}
    saved_argv = sys.argv
    saved_path = sys.path[:]
    try:
        sys.argv = [str(script)]
        sys.path.insert(0, str(script.parent))
        with redirect_stdout(output), redirect_stderr(errors):
            try:
                code = compile(script.read_bytes(), str(script), "exec")
                exec(code, namespace)
            except BaseException as exception:
                frames = traceback.extract_tb(exception.__traceback__)
                failure = {
                    "type": type(exception).__name__,
                    "message": str(exception),
                    "line": next(
                        (frame.lineno for frame in reversed(frames) if frame.filename == str(script)),
                        None,
                    ),
                    "traceback": "".join(traceback.format_exception(exception)),
                }
    finally:
        sys.argv = saved_argv
        sys.path[:] = saved_path
    payload = {
        "schema": "kasane.run.v0",
        "script": str(script),
        "status": "failed" if failure else "passed",
        "stdout": output.getvalue(),
        "stderr": errors.getvalue(),
        "exception": failure,
        "sessions": _snapshot_sessions(),
        "observation_artifacts": [],
    }
    report.parent.mkdir(parents=True, exist_ok=True)
    report.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    if failure:
        print(failure["traceback"], file=sys.stderr, end="")
        return 1
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(prog="python -m kasane")
    commands = parser.add_subparsers(dest="command", required=True)
    run_parser = commands.add_parser("run", help="Run a Python authoring script once")
    run_parser.add_argument("script", type=Path)
    run_parser.add_argument("--report", type=Path, required=True)
    arguments = parser.parse_args()
    if arguments.command == "run":
        return run(arguments.script, arguments.report)
    parser.error("Unknown command")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
