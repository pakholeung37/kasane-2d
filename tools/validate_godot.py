#!/usr/bin/env python3
"""Stage a disposable Godot project to test data/preview ownership boundaries.
Headless integration evidence only; does not replace GPU visual acceptance.
"""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--godot', type=Path, default=Path('/Applications/Godot_mono.app/Contents/MacOS/Godot'))
    p.add_argument('--library', type=Path, default=ROOT/'modules/gd-kasane/build/bin/libgd_kasane.macos.template_debug.arm64.dylib')
    p.add_argument('--output-dir', type=Path, default=ROOT/'target/kasane/godot-boundary')
    args = p.parse_args()
    project = args.output_dir.resolve()
    project.mkdir(parents=True, exist_ok=True)
    if not args.godot.is_file() or not args.library.is_file():
        print('Missing Godot executable or built gd-kasane library', file=sys.stderr)
        return 1
    library = project/args.library.name
    shutil.copyfile(args.library, library)
    shutil.copyfile(ROOT/'modules/gd-kasane/tests/document_boundary.gd', project/'test.gd')
    (project/'project.godot').write_text('config_version=5\n[application]\nconfig/name="Kasane Boundary Test"\n[rendering]\nrenderer/rendering_method="gl_compatibility"\n')
    (project/'kasane.gdextension').write_text('[configuration]\nentry_symbol="gd_kasane_library_init"\ncompatibility_minimum="4.3"\n[libraries]\nmacos.debug.arm64="res://'+library.name+'"\n')
    # All fixtures are built in memory. Register this one extension directly;
    # no editor import or filesystem resource scan is needed for this harness.
    # First-scan editor shutdown currently crashes on this host even with the
    # pre-refactor HEAD extension; see docs/editor/M1-CORE-REFACTOR.md.
    (project/'.godot').mkdir(exist_ok=True)
    (project/'.godot/extension_list.cfg').write_text('res://kasane.gdextension\n')
    data = tempfile.mkdtemp(prefix='files-', dir=project)
    for label, command in [('test', ['--headless', '--script', 'res://test.gd', '--', data])]:
        result = subprocess.run([str(args.godot), '--path', str(project), *command], text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=60)
        (project/f'{label}.log').write_text(result.stdout)
        if result.returncode or 'SCRIPT ERROR:' in result.stdout or 'ERROR:' in result.stdout:
            print(result.stdout)
            return 1
        if label == 'test':
            lines = [line for line in result.stdout.splitlines() if line.startswith('{')]
            if not lines:
                print('Missing test report')
                return 1
            report = json.loads(lines[-1])
            report['godot'] = result.stdout.splitlines()[0]
            report['library'] = str(args.library.resolve())
            report['extension_loading'] = 'pre_registered'
            report['fresh_editor_import'] = 'not_tested_by_this_harness; known baseline crash documented in M1-CORE-REFACTOR.md'
            (project/'report.json').write_text(json.dumps(report, indent=2)+'\n')
            if report['status'] != 'passed':
                return 1
            print(f'{report["checks"]} Godot boundary checks passed: {project / "report.json"}')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
