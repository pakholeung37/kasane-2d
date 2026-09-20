#!/usr/bin/env python3
"""C++/Rust directory-project v1 roundtrips and real cross-process lock exclusion."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cpp-tool', type=Path, required=True)
    parser.add_argument('--rust-tool', type=Path, default=ROOT / 'target/debug/examples/project_roundtrip')
    parser.add_argument('--output-dir', type=Path, default=ROOT / 'target/rust-migration')
    args = parser.parse_args()
    cpp, rust = args.cpp_tool.resolve(), args.rust_tool.resolve()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    base = Path(tempfile.mkdtemp(prefix='run-', dir=args.output_dir.resolve()))
    report = {'status': 'failed', 'checks': [], 'scope': 'format and lock compatibility', 'directory': str(base)}

    def run(command, label, expected_error=None):
        result = subprocess.run(list(map(str, command)), capture_output=True, text=True, timeout=60)
        log = result.stdout + result.stderr
        (base / (label + '.log')).write_text(log)
        if expected_error:
            if result.returncode == 0 or expected_error not in log:
                raise RuntimeError(f'{label}: expected {expected_error}: {log}')
        elif result.returncode:
            raise RuntimeError(f'{label}: {log}')
        report['checks'].append(label)

    try:
        sample = ROOT / 'samples/m2-complete'
        run([cpp, 'save', sample, base / 'cpp-first'], 'cpp-source')
        run([rust, base / 'cpp-first', base / 'rust-first'], 'cpp-to-rust')
        run([cpp, 'save', base / 'rust-first', base / 'cpp-second'], 'rust-to-cpp')
        run([rust, base / 'cpp-second', base / 'rust-second'], 'cpp-back-to-rust')
        # Compare C++'s independently decoded source after both roundtrips.
        run([cpp, 'inspect', base / 'cpp-first', base / 'before'], 'inspect-before')
        run([cpp, 'inspect', base / 'rust-second', base / 'after'], 'inspect-after')
        for filename in ['source.json', 'samples.json', 'runtime/model.moc3', 'runtime/model.model3.json']:
            if (base / 'before' / filename).read_bytes() != (base / 'after' / filename).read_bytes():
                raise RuntimeError(f'Roundtrip changed {filename}')
            report['checks'].append('identical-' + filename)
        ready = base / 'lock-ready.json'
        holder = subprocess.Popen([str(cpp), 'hold-lock', str(base / 'rust-first'), str(ready)],
                                  stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            deadline = time.monotonic() + 10
            while not ready.exists():
                if holder.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError('C++ lock holder did not become ready')
                time.sleep(0.02)
            run([rust, base / 'rust-first', base / 'rust-first'], 'cpp-lock-blocks-rust', 'PROJECT_BUSY')
        finally:
            holder.communicate('\n', timeout=10)
        run([rust, base / 'rust-first', base / 'rust-first'], 'lock-release')
        report['status'] = 'passed'
    finally:
        (args.output_dir / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(f"{len(report['checks'])} migration checks passed: {args.output_dir / 'report.json'}")


if __name__ == '__main__':
    main()
