"""Audit every public Rust SDK entry point against the Python binding inventory."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[3]
SDK = ROOT / "modules/kasane-sdk/src/lib.rs"
MANIFEST = Path(__file__).resolve().parents[1] / "python-coverage.json"
TESTS = Path(__file__).resolve().parents[1] / "tests/test_cpu.py"


def public_api() -> set[str]:
    source = SDK.read_text(encoding="utf-8")
    sections = {
        "Session": source.split("impl AuthoringSession {", 1)[1].split(
            "pub struct EditSession", 1
        )[0],
        "Edit": source.split("impl EditSession<'_> {", 1)[1].split("fn merge_kind", 1)[0],
        "ObjectHandle": source.split("impl ObjectHandle {", 1)[1].split(
            "pub struct MeshProperties", 1
        )[0],
        "free": source.split("fn merge_kind", 1)[1],
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
    for api, entry in sorted(entries.items()):
        if entry.get("status") == "bound":
            bound += 1
            if not entry.get("python") or not entry.get("test"):
                bad.append(f"{api}: missing python method or test")
            elif f"def {entry['test']}(" not in test_source:
                bad.append(f"{api}: unknown test {entry['test']}")
        elif entry.get("status") != "pending":
            bad.append(f"{api}: invalid status")
    if bad:
        print("\n".join(bad))
        return 1
    pending = len(entries) - bound
    print(f"Rust SDK API: {len(entries)}; Python bound: {bound}; pending: {pending}")
    return int(args.require_complete and pending > 0)


if __name__ == "__main__":
    raise SystemExit(main())
