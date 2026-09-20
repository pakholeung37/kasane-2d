#!/usr/bin/env python3
"""Stage the editor and its native library; optionally launch or export it."""
import argparse
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--godot', type=Path, default=Path('/Applications/Godot_mono.app/Contents/MacOS/Godot'))
    parser.add_argument('--output-dir', type=Path, default=ROOT / 'target/editor-app')
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--run', action='store_true')
    parser.add_argument('--prepare-source', action='store_true', help='Prepare native dependencies in apps/editor for opening project.godot in Godot')
    parser.add_argument('--editor', action='store_true', help='Open the prepared project in the Godot editor')
    parser.add_argument('--test', action='store_true', help='Run application workspace integration checks')
    parser.add_argument('--export', dest='export_path', type=Path)
    parser.add_argument('--template', type=Path, help='Custom platform export template (macOS: macos.zip)')
    args = parser.parse_args()
    if args.prepare_source and (args.export_path or args.output_dir != ROOT / 'target/editor-app'):
        parser.error('--prepare-source cannot be combined with --export or --output-dir')
    systems = {'Darwin': ('libkasane_godot.dylib', 'macos', 'macOS'),
               'Linux': ('libkasane_godot.so', 'linux', 'Linux'),
               'Windows': ('kasane_godot.dll', 'windows', 'Windows Desktop')}
    library, system, preset = systems[platform.system()]
    architecture = 'arm64' if platform.machine().lower() in ('arm64', 'aarch64') else 'x86_64'
    if not args.skip_build:
        subprocess.run(['cargo', 'build', '--release', '-p', 'kasane-godot'], cwd=ROOT, check=True)
    destination = ROOT / 'apps/editor' if args.prepare_source else args.output_dir.resolve()
    if destination == ROOT / 'apps/editor' and not args.prepare_source:
        raise ValueError('Use a staging directory, not the application source directory')
    if args.export_path:
        destination = Path(tempfile.mkdtemp(prefix='editor-export-', dir=ROOT / 'target'))
    if not args.prepare_source:
        shutil.copytree(ROOT / 'apps/editor', destination, dirs_exist_ok=True,
                        ignore=shutil.ignore_patterns('.godot', 'export_credentials.cfg', 'native', 'kasane.gdextension'))
    (destination / 'native').mkdir(exist_ok=True)
    temporary_library = destination / 'native' / (library + '.tmp')
    shutil.copy2(ROOT / 'target/release' / library, temporary_library)
    temporary_library.replace(destination / 'native' / library)
    (destination / 'kasane.gdextension').write_text(
        '[configuration]\nentry_symbol="kasane_gd_library_init"\ncompatibility_minimum="4.3"\n[libraries]\n'
        + ''.join(f'{system}.{mode}.{architecture}="res://native/{library}"\n' for mode in ['debug', 'release']))
    presets = destination / 'export_presets.cfg'
    preset_text = presets.read_text()
    if system == 'macos':
        preset_text = preset_text.replace('[preset.0.options]', '[preset.0.options]\nbinary_format/architecture="' + architecture + '"')
    if args.template and system == 'macos':
        # Official macOS templates contain universal binaries. Prepare the selected
        # architecture explicitly so the application matches its native extension.
        template_path = args.template.resolve()
        with zipfile.ZipFile(template_path) as archive:
            if any(n.endswith('godot_macos_release.universal') for n in archive.namelist()):
                thin_template = destination / '.godot' / 'macos-template.zip'
                thin_template.parent.mkdir(exist_ok=True)
                with zipfile.ZipFile(thin_template, 'w', zipfile.ZIP_DEFLATED) as output_zip:
                    for info in archive.infolist():
                        data = archive.read(info.filename)
                        if info.filename.endswith(('.universal')) and '/MacOS/godot_macos_' in info.filename:
                            binary = thin_template.parent / 'universal-binary'
                            thin = thin_template.parent / 'thin-binary'
                            binary.write_bytes(data)
                            subprocess.run(['lipo', str(binary), '-thin', architecture, '-output', str(thin)], check=True)
                            data = thin.read_bytes()
                            info.filename = info.filename.replace('.universal', '.' + architecture)
                            binary.unlink()
                            thin.unlink()
                        output_zip.writestr(info, data)
                args.template = thin_template
    if args.template:
        index = {'macos': 0, 'linux': 1, 'windows': 2}[system]
        preset_text = preset_text.replace(f'[preset.{index}.options]',
            f'[preset.{index}.options]\ncustom_template/debug="{args.template.resolve()}"\ncustom_template/release="{args.template.resolve()}"')
    if not args.prepare_source:
        presets.write_text(preset_text)
    (destination / '.godot').mkdir(exist_ok=True)
    (destination / '.godot/extension_list.cfg').write_text('res://kasane.gdextension\n')
    # Godot may exit 0 even when a script fails to parse. Validate startup output
    # before producing a distributable, and before tests can hang on missing UI.
    startup = subprocess.run([str(args.godot), '--headless', '--path', str(destination), '--quit-after', '2'],
                             text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=30)
    if startup.returncode or 'ERROR:' in startup.stdout:
        raise RuntimeError('Application startup failed:\n' + startup.stdout)
    if args.test:
        test_script = destination / 'editor_workspace.gd'
        shutil.copy2(ROOT / 'tests/editor_workspace.gd', test_script)
        data_dir = Path(tempfile.mkdtemp(prefix='workspace-test-', dir=ROOT / 'target'))
        try:
            result = subprocess.run([str(args.godot), '--headless', '--path', str(destination),
                                     '--script', 'res://editor_workspace.gd', '--', str(data_dir)],
                                    text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=60)
            (data_dir / 'editor-workspace.log').write_text(result.stdout)
            print(result.stdout)
            result.check_returncode()
            if 'ERROR:' in result.stdout:
                raise RuntimeError('Godot reported errors; see editor-workspace.log')
        finally:
            test_script.unlink(missing_ok=True)
    if args.export_path:
        output = args.export_path.resolve()
        output.parent.mkdir(parents=True, exist_ok=True)
        exported = subprocess.run([str(args.godot), '--headless', '--path', str(destination), '--export-debug', preset, str(output)],
                                  text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=180)
        print(exported.stdout)
        if exported.returncode or 'ERROR:' in exported.stdout or not output.is_file():
            raise RuntimeError('Application export failed')
        if output.suffix.lower() == '.zip':
            with zipfile.ZipFile(output, 'a', zipfile.ZIP_DEFLATED) as archive:
                archive.write(ROOT / 'apps/editor/API.md', 'API.md')
                archive.write(ROOT / 'apps/editor/README.md', 'README.md')
    if args.editor:
        subprocess.run([str(args.godot), '--editor', '--path', str(destination)], check=True)
    elif args.run:
        subprocess.run([str(args.godot), '--path', str(destination)], check=True)
    print(destination)


if __name__ == '__main__':
    main()
