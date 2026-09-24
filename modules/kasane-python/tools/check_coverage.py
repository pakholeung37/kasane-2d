"""Audit every public Rust SDK entry point against the Python binding inventory."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[3]
SDK = ROOT / "modules/kasane-sdk/src"
MANIFEST = Path(__file__).resolve().parents[1] / "python-coverage.json"
TESTS = Path(__file__).resolve().parents[1] / "tests/test_cpu.py"


def public_api() -> set[str]:
    sources = {
        name: (SDK / f"{name}.rs").read_text(encoding="utf-8")
        for name in ("session", "project_io", "diagnostics", "edit", "types", "assets", "geometry")
    }
    geometry_free, geometry_session = sources["geometry"].split("impl AuthoringSession", 1)
    sections = {
        "Session": sources["session"] + sources["project_io"] + sources["diagnostics"] + geometry_session,
        "Edit": sources["edit"],
        "ObjectHandle": sources["types"],
        "free": sources["assets"] + geometry_free,
    }
    result = set()
    for owner, body in sections.items():
        for name in re.findall(r"(?m)^\s*pub fn (\w+)(?:<[^>]+>)?\s*\(", body):
            result.add(f"{owner}.{name}")
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--init", action="store_true", help="Add newly discovered API as pending")
    parser.add_argument("--require-complete", action="store_true")
    args = parser.parse_args()
    actual = public_api()
    entries = json.loads(MANIFEST.read_text(encoding="utf-8")) if MANIFEST.exists() else {}
    if args.init:
        for name in sorted(actual - entries.keys()):
            entries[name] = {"status": "pending"}
        MANIFEST.write_text(json.dumps(dict(sorted(entries.items())), indent=2) + "\n")
    missing = actual - entries.keys()
    removed = entries.keys() - actual
    if missing or removed:
        print(f"Missing: {sorted(missing)}")
        print(f"Removed: {sorted(removed)}")
        return 1
    test_source = TESTS.read_text(encoding="utf-8")
    bad = []
    bound = 0
    rust_only = 0
    for api, entry in sorted(entries.items()):
        if entry.get("status") == "bound":
            bound += 1
            if not entry.get("python") or not entry.get("test"):
                bad.append(f"{api}: missing python method or test")
            elif f"def {entry['test']}(" not in test_source:
                bad.append(f"{api}: unknown test {entry['test']}")
        elif entry.get("status") == "rust_only":
            rust_only += 1
            path = ROOT / entry.get("rust_test_path", "")
            if not entry.get("reason") or not entry.get("rust_test") or not path.is_file():
                bad.append(f"{api}: missing reason or Rust test")
            elif f"fn {entry['rust_test']}(" not in path.read_text(encoding="utf-8"):
                bad.append(f"{api}: unknown Rust test {entry['rust_test']}")
        elif entry.get("status") != "pending":
            bad.append(f"{api}: invalid status")
    if bad:
        print("\n".join(bad))
        return 1
    pending = len(entries) - bound - rust_only
    print(f"Rust SDK API: {len(entries)}; Python bound: {bound}; Rust only: {rust_only}; pending: {pending}")
    return int(args.require_complete and pending > 0)


if __name__ == "__main__":
    raise SystemExit(main())
