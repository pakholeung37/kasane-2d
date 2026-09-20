#!/usr/bin/env python3
"""Stage a disposable Godot project to test data/preview ownership boundaries,
lifecycle safety, and end-to-end authoring workflows using the Rust GDExtension.
"""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def get_default_library():
    release_rust = ROOT / 'target/release/libkasane_godot.dylib'
    if release_rust.is_file():
        return release_rust
    debug_rust = ROOT / 'target/debug/libkasane_godot.dylib'
    if debug_rust.is_file():
        return debug_rust
    return release_rust


def validate_workflow_package(data_dir, samples, probe):
    from validate_official_core import run, require_finite, validate_drawable_shape
    package = Path(data_dir) / "e2e_export"
    model = json.loads((package / "model.model3.json").read_text())
    references = model["FileReferences"]
    if not references.get("Textures"):
        raise RuntimeError("Exported package has no textures")
    for texture in references["Textures"]:
        if not (package / texture).is_file():
            raise RuntimeError(f"Missing exported texture: {texture}")
    if not samples:
        raise RuntimeError("Missing workflow parameter samples")
    require_finite(samples)
    input_data = str(len(samples)) + "\n" + "\n".join(str(s["parameter"]) for s in samples) + "\n"
    stdout = run([probe, package / references["Moc"]], text_input=input_data)
    runtime = json.loads(next(line for line in stdout.splitlines() if line.startswith('{"core_version"')))
    require_finite(runtime)
    if len(runtime["samples"]) != len(samples):
        raise RuntimeError("Workflow sample count mismatch")
    for expected, drawables in zip(samples, runtime["samples"]):
        if len(drawables) != 1 or drawables[0]["runtime_id"] != "ArtMeshE2E":
            raise RuntimeError("Workflow drawable mismatch")
        actual = drawables[0]
        validate_drawable_shape(actual)
        if len(actual["positions"]) != len(expected["positions"]) or len(expected["positions"]) != 4:
            raise RuntimeError("Workflow vertex count mismatch")
        for ep, ap in zip(expected["positions"], actual["positions"]):
            if len(ep) != 2:
                raise RuntimeError("Invalid workflow expected point")
            for e, a in zip(ep, ap):
                if abs(e - a) > max(0.0005, 1e-4 + 1e-4 * max(abs(e), abs(a))):
                    raise RuntimeError("Workflow evaluated position differs from official Core")
    return {"status": "passed", "samples": len(samples), "core_version": runtime["core_version"]}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--godot', type=Path, default=Path('/Applications/Godot_mono.app/Contents/MacOS/Godot'))
    p.add_argument('--library', type=Path, default=get_default_library())
    p.add_argument('--output-dir', type=Path, default=ROOT / 'target/kasane/godot-boundary')
    p.add_argument('--suite', choices=['all', 'boundary', 'lifecycle', 'workflow'], default='all')
    p.add_argument('--official-probe', type=Path, default=ROOT / 'target/kasane/core-regression/build/kasane_document_official_probe')
    args = p.parse_args()

    project = args.output_dir.resolve()
    project.mkdir(parents=True, exist_ok=True)

    if not args.godot.is_file() or not args.library.is_file():
        print(f'Missing Godot executable ({args.godot}) or library ({args.library})', file=sys.stderr)
        return 1

    library = project / args.library.name
    shutil.copyfile(args.library, library)

    # Copy gd_cubism addon if available so runtime player tests can run
    addon_src = ROOT / 'modules/gd-cubism/addons/gd_cubism'
    addon_dst = project / 'addons/gd_cubism'
    has_cubism = False
    if addon_src.exists():
        shutil.copytree(addon_src / 'res', addon_dst / 'res', dirs_exist_ok=True)
        framework = 'libgd_cubism.cubism.macos.release.framework'
        bin_framework = addon_src / 'bin' / framework
        if bin_framework.exists():
            shutil.copytree(bin_framework, addon_dst / 'bin' / framework, dirs_exist_ok=True)
            (addon_dst / 'gd_cubism.gdextension').write_text(
                f'[configuration]\nentry_symbol="gd_cubism_library_init"\ncompatibility_minimum="4.3"\n[libraries]\nmacos.debug.arm64="res://addons/gd_cubism/bin/{framework}"\n'
            )
            has_cubism = True

    (project / 'project.godot').write_text(
        'config_version=5\n[application]\nconfig/name="Kasane Godot Test"\n[rendering]\nrenderer/rendering_method="gl_compatibility"\n'
    )
    (project / 'kasane.gdextension').write_text(
        f'[configuration]\nentry_symbol="kasane_gd_library_init"\ncompatibility_minimum="4.3"\n[libraries]\nmacos.debug.arm64="res://{library.name}"\n'
    )
    (project / '.godot').mkdir(exist_ok=True)
    ext_list = 'res://kasane.gdextension\n'
    if has_cubism:
        ext_list += 'res://addons/gd_cubism/gd_cubism.gdextension\n'
    (project / '.godot/extension_list.cfg').write_text(ext_list)

    suites = []
    if args.suite in ['all', 'boundary']:
        suites.append(('boundary', ROOT / 'modules/kasane-godot/tests/document_boundary.gd'))
    if args.suite in ['all', 'lifecycle']:
        suites.append(('lifecycle', ROOT / 'modules/kasane-godot/tests/lifecycle_boundary.gd'))
    if args.suite in ['all', 'workflow']:
        suites.append(('workflow', ROOT / 'modules/kasane-godot/tests/full_workflow_e2e.gd'))

    overall_report = {
        'status': 'passed',
        'library': str(args.library.resolve()),
        'suites': {},
        'total_checks': 0,
    }

    for name, script_path in suites:
        print(f'\n--- Running Godot suite: {name} ({script_path.name}) ---')
        shutil.copyfile(script_path, project / f'{name}.gd')
        data_dir = tempfile.mkdtemp(prefix=f'data-{name}-', dir=project)

        cmd = [str(args.godot), '--headless', '--path', str(project), '--script', f'res://{name}.gd', '--', data_dir]
        result = subprocess.run(cmd, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=90)
        (project / f'{name}.log').write_text(result.stdout)

        if result.returncode != 0 or 'SCRIPT ERROR:' in result.stdout or 'ERROR:' in result.stdout:
            print(result.stdout)
            print(f'Suite {name} FAILED with returncode {result.returncode}')
            overall_report['status'] = 'failed'
            overall_report['suites'][name] = {'status': 'failed', 'log': str(project / f'{name}.log')}
            break

        lines = [line for line in result.stdout.splitlines() if line.startswith('{')]
        if not lines:
            print(f'Suite {name} returned no JSON report. Log:\n{result.stdout[-2000:]}')
            overall_report['status'] = 'failed'
            overall_report['suites'][name] = {'status': 'missing_report'}
            break

        suite_report = json.loads(lines[-1])
        suite_checks = suite_report.get('checks', 0)
        overall_report['total_checks'] += suite_checks
        overall_report['suites'][name] = suite_report

        if suite_checks <= 0 or suite_report.get('status') != 'passed':
            print(f'Suite {name} reported failures: {suite_report.get("failures")}')
            overall_report['status'] = 'failed'
            break

        if name == 'workflow':
            try:
                suite_report['official_runtime'] = validate_workflow_package(data_dir, suite_report['runtime_samples'], args.official_probe.resolve())
            except Exception as exc:
                suite_report['status'] = 'failed'
                suite_report['failures'].append(str(exc))
                overall_report['status'] = 'failed'
                print(f'Workflow official runtime validation FAILED: {exc}')
                break
        print(f'  [PASS] {name}: {suite_checks} checks passed.')

    report_path = project / 'report.json'
    report_path.write_text(json.dumps(overall_report, indent=2) + '\n')

    if overall_report['status'] == 'passed':
        print(f'\nAll Godot integration suites PASSED ({overall_report["total_checks"]} total checks): {report_path}')
        return 0
    else:
        print(f'\nGodot integration FAILED: {report_path}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
