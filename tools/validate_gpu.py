#!/usr/bin/env python3
"""GPU comparison of live Document preview with the existing gd-cubism player."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]

def run(command, log):
    result = subprocess.run(list(map(str, command)), stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=120)
    log.write_text(result.stdout)
    if result.returncode or 'SCRIPT ERROR:' in result.stdout or 'ERROR:' in result.stdout:
        raise RuntimeError(f'{log}:\n{result.stdout[-5000:]}')
    return result.stdout

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--godot', type=Path, default=Path('/Applications/Godot_mono.app/Contents/MacOS/Godot'))
    parser.add_argument('--core-build', type=Path, default=ROOT/'target/kasane/core-regression/build')
    parser.add_argument('--output-dir', type=Path, default=ROOT/'target/kasane/gpu-regression')
    args = parser.parse_args()
    project = args.output_dir.resolve()
    project.mkdir(parents=True, exist_ok=True)
    try:
        run([args.core_build.resolve()/'kasane_moc3_official_tests', project/'fixtures'], project/'fixtures.log')
        shutil.copytree(project/'fixtures/publication/gpu-package', project/'package', dirs_exist_ok=True)
        fixture = json.loads((project/'fixtures/publication/gpu-source.json').read_text())
        fixture.update(format='kasane-directory-project', format_version=1)
        (project/'assets').mkdir(exist_ok=True)
        for asset in fixture['document']['assets']:
            source = project / 'package' / asset['source'].removeprefix('res://')
            data = source.read_bytes()
            asset['sha256'] = hashlib.sha256(data).hexdigest()
            asset['source'] = 'assets/' + asset['sha256'] + '.png'
            (project/asset['source']).write_bytes(data)
        (project/'gpu-source.json').write_text(json.dumps(fixture))
        (project/'roundtrip.json').unlink(missing_ok=True)
        shutil.copyfile(ROOT/'modules/gd-kasane/tests/gpu_regression.gd', project/'test.gd')
        addon = project/'addons/gd_cubism'
        shutil.copytree(ROOT/'modules/gd-cubism/addons/gd_cubism/res', addon/'res', dirs_exist_ok=True)
        framework = 'libgd_cubism.cubism.macos.release.framework'
        shutil.copytree(ROOT/'modules/gd-cubism/addons/gd_cubism/bin'/framework, addon/'bin'/framework, dirs_exist_ok=True)
        (addon/'gd_cubism.gdextension').write_text('[configuration]\nentry_symbol="gd_cubism_library_init"\ncompatibility_minimum="4.3"\n[libraries]\nmacos.debug.arm64="res://addons/gd_cubism/bin/'+framework+'"\n')
        lib = 'libgd_kasane.macos.template_debug.arm64.dylib'
        shutil.copyfile(ROOT/'modules/gd-kasane/build/bin'/lib, project/lib)
        (project/'kasane.gdextension').write_text('[configuration]\nentry_symbol="gd_kasane_library_init"\ncompatibility_minimum="4.3"\n[libraries]\nmacos.debug.arm64="res://'+lib+'"\n')
        (project/'project.godot').write_text('config_version=5\n[application]\nconfig/name="Kasane GPU Regression"\n[display]\nwindow/size/viewport_width=640\nwindow/size/viewport_height=480\n[rendering]\nrenderer/rendering_method="gl_compatibility"\ntextures/default_filters/use_nearest_mipmap_filter=false\n')
        (project/'.godot').mkdir(exist_ok=True)
        (project/'.godot/extension_list.cfg').write_text('res://kasane.gdextension\nres://addons/gd_cubism/gd_cubism.gdextension\n')
        run([args.godot, '--headless', '--path', project, '--editor', '--import'], project/'import.log')
        run([args.godot, '--path', project, '--rendering-method', 'gl_compatibility', '--resolution', '640x480', '--script', 'res://test.gd', '--', project], project/'run.log')
        from compare_gpu_images import compare
        report = compare(project)
        print(f'{len(report["checks"])} GPU checks passed: {project / "report.json"}')
        return 0
    except Exception as exc:
        report_path = project/'report.json'
        report = json.loads(report_path.read_text()) if report_path.exists() else {}
        report.update(status='failed', error=str(exc))
        report_path.write_text(json.dumps(report,indent=2))
        print(exc)
        return 1

if __name__ == '__main__':
    raise SystemExit(main())
