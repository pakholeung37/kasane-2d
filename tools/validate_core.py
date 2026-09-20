#!/usr/bin/env python3
"""Run source, codec, and dual-Core regression checks."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import struct
import subprocess
import tempfile
import zlib

ROOT = Path(__file__).resolve().parents[1]


def run(argv, log):
    result = subprocess.run([str(x) for x in argv], cwd=ROOT, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    log.write_text(result.stdout)
    if result.returncode:
        raise RuntimeError(f"Command failed ({result.returncode}); see {log}:\n{result.stdout[-4000:]}")
    return result.stdout


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT, text=True).strip()


def source_fingerprint():
    digest = hashlib.sha256()
    for repository, paths in ((ROOT, ('CMakeLists.txt', 'CMakePresets.json', '.github/workflows',
                                      'modules/kasane-core', 'modules/kasane-document', 'modules/kasane-gd', 'samples', 'tools')),
                              (ROOT/'modules/purism-core', ('.',))):
        names = subprocess.check_output(['git', 'ls-files', '-co', '--exclude-standard', '-z', '--', *paths],
                                        cwd=repository).split(b'\0')
        for raw in sorted(name for name in names if name):
            path = repository / raw.decode()
            if path.is_file():
                digest.update(str(path.relative_to(ROOT)).encode() + b'\0')
                digest.update(hashlib.sha256(path.read_bytes()).digest())
    return digest.hexdigest()


def schema():
    source = (ROOT/'modules/purism-core/src/moc3.h').read_text()
    fields = re.search(r'struct psm__count_info \{(.*?)\n\};', source, re.S)[1]
    counts = re.findall(r'psm__i32 (\w+);', fields)
    expected = []
    for version in ('30', '33', '42', '50'):
        block = source.split('#define PSM__SECTIONS_V'+version+'(S, D)')[1].split('\n\n')[0]
        for kind, typ, member, count in re.findall(r'([SD])\(([^,]+), ([^,]+), ([^)]+)\)', block):
            width = (256 if member == 'count_info' else 24 if member == 'canvas_info'
                     else 8 if '*' in typ else {'struct psm__id': 64, 'psm__i32': 4,
                                               'psm__f32': 4, 'psm__u16': 2, 'psm__u8': 1}[typ])
            expected.append((member, width, -1 if kind == 'S' else counts.index(count)))
    actual = [(m, int(w), int(c)) for m, w, c in re.findall(
        r'SECTION\("([^"]+)", (\d+), (-?\d+)\)',
        (ROOT/'modules/kasane-core/src/moc3_sections.inc').read_text())]
    if expected != actual:
        raise RuntimeError('MOC3 schema differs from pinned PurismCore; review the wire mapping')
    return actual


def inspect_layout(path, fields):
    data = path.read_bytes()
    if data[:8] != b'MOC3\x05\0\0\0':
        raise RuntimeError('Wrong MOC3 header/version/endian')
    offsets = struct.unpack_from('<160I', data, 64)
    counts = struct.unpack_from('<64I', data, offsets[0])
    cursor = 1984
    result = []
    for i, (name, width, count_index) in enumerate(fields):
        count = 1 if count_index < 0 else counts[count_index]
        offset = offsets[i]
        size = count * width
        if offset % 64 or offset < cursor or offset+size > len(data):
            raise RuntimeError(f'Invalid section layout: {name}')
        result.append(dict(section=i, field=name, width=width, count=count, offset=offset, size=size))
        cursor = offset+size
    if any(offsets[len(fields):]):
        raise RuntimeError('Reserved offsets must be zero')
    return result


def png(slot):
    """Asymmetric RGBA fixture, no external image dependencies or assets."""
    def chunk(name, body):
        return struct.pack('>I', len(body))+name+body+struct.pack('>I', zlib.crc32(name+body))
    rows = bytearray()
    for y in range(8):
        rows.append(0)
        for x in range(8):
            rows.extend((x*31, y*31, 240 if slot == 0 else 30, 255))
    return (b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR', struct.pack('>2I5B', 8, 8, 8, 6, 0, 0, 0))
            +chunk(b'IDAT', zlib.compress(rows))+chunk(b'IEND', b''))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sdk', type=Path, default=ROOT/'third_party/CubismSdkForNative-5-r.5')
    parser.add_argument('--output-dir', type=Path, default=ROOT/'target/kasane/core-regression')
    args = parser.parse_args()
    parent = ROOT/'target/kasane'
    parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix='core-regression-', dir=parent))
    report = dict(scope='source model, editing, nested transforms, drawing and package publication', status='failed',
                  git_revision=git('rev-parse', 'HEAD'), source_sha256=source_fingerprint(), working_tree=git('status', '--short'),
                  submodules=git('submodule', 'status'), purism_working_tree=git('-C', 'modules/purism-core', 'status', '--short'), platform=platform.platform(),
                  architecture=platform.machine(), build_configuration='Debug', moc_version=5,
                  coordinate_units='runtime model units; source pixels; pixels_per_unit=100',
                  canvas=dict(width=640, height=480, origin=[271, 193]),
                  parameter_samples=[{}], checks=[])
    try:
        fields = schema()
        if not (args.sdk/'Core/include/Live2DCubismCore.h').is_file():
            report['checks'].append(dict(name='official_core', status='not_run', reason='SDK missing'))
            raise RuntimeError('Official Core SDK missing; existing output is preserved')
        # Each invocation owns its build tree, including the generated provider binaries.
        build = stage/'build'
        run(['cmake', '-S', ROOT/'modules/kasane-core', '-B', build,
             '-DCMAKE_BUILD_TYPE=Debug', '-DKASANE_MOC3_CONFORMANCE=ON',
             '-DPURISM_CORE_BUILD_TESTS=ON', '-DPURISM_CORE_ABI=v6', '-DKASANE_CUBISM_ROOT='+str(args.sdk.resolve())], stage/'configure.log')
        run(['cmake', '--build', build, '-j8'], stage/'build.log')
        run(['ctest', '--test-dir', build, '--output-on-failure'], stage/'ctest.log')
        for provider in ('purism', 'official'):
            output = stage/provider
            text = run([build/f'kasane_moc3_{provider}_tests', output], stage/f'{provider}.log')
            summary = json.loads(next(line for line in text.splitlines() if line.startswith('{"status"')))
            report['checks'].append(dict(provider=provider, **summary))
        report['parameter_samples'] = json.loads((stage/'purism/samples.json').read_text())
        for name in sorted(p.name for p in (stage/'purism').glob('nested-*.moc3')):
            data=(stage/'purism'/name).read_bytes()
            if data!=(stage/'official'/name).read_bytes():
                raise RuntimeError(f'Providers evaluated different files: {name}')
            output=stage/('package-'+Path(name).stem)
            (output/'textures').mkdir(parents=True)
            (output/'model.moc3').write_bytes(data)
            shutil.copyfile(stage/'purism/model.model3.json',output/'model.model3.json')
            for slot in range(2):
                (output/f'textures/{slot}.png').write_bytes(png(slot))
            (stage/(Path(name).stem+'-layout.json')).write_text(json.dumps(inspect_layout(output/'model.moc3',fields),indent=2)+'\n')
        for dimension in (1, 2, 3):
            name = f'parameter-{dimension}d.moc3'
            data = (stage/'purism'/name).read_bytes()
            if data != (stage/'official'/name).read_bytes():
                raise RuntimeError(f'Providers evaluated different files: {name}')
            output = stage/f'package-{dimension}d'
            (output/'textures').mkdir(parents=True)
            (output/'model.moc3').write_bytes(data)
            shutil.copyfile(stage/'purism/model.model3.json', output/'model.model3.json')
            for slot in range(2):
                (output/f'textures/{slot}.png').write_bytes(png(slot))
            (stage/f'field-layout-{dimension}d.json').write_text(json.dumps(inspect_layout(output/'model.moc3', fields), indent=2)+'\n')
        a = (stage/'purism/model.moc3').read_bytes()
        if a != (stage/'official/model.moc3').read_bytes():
            raise RuntimeError('Providers did not evaluate identical generated MOC3 bytes')
        package = stage/'package'
        (package/'textures').mkdir(parents=True)
        for name in ('model.moc3', 'model.model3.json'):
            shutil.copyfile(stage/'purism'/name, package/name)
        for slot in range(2):
            (package/f'textures/{slot}.png').write_bytes(png(slot))
        refs = json.loads((package/'model.model3.json').read_text())['FileReferences']
        if refs != {'Moc': 'model.moc3', 'Textures': ['textures/0.png', 'textures/1.png']}:
            raise RuntimeError('Incorrect resource description or texture slot ordering')
        layout = inspect_layout(package/'model.moc3', fields)
        (stage/'field-layout.json').write_text(json.dumps(layout, indent=2)+'\n')
        report['files'] = [{"path": str(p.relative_to(stage)), "sha256": hashlib.sha256(p.read_bytes()).hexdigest()}
                           for p in sorted(stage.rglob('*')) if p.is_file()]
        report['checks'].append(dict(name='schema_layout_resource_references', status='passed'))
        report['status'] = 'passed'
        (stage/'report.json').write_text(json.dumps(report, indent=2)+'\n')
        # Publish only after all required checks of this increment have passed.
        # The previous verified run survives all build/validation/write failures.
        destination = args.output_dir.resolve()
        destination.parent.mkdir(parents=True, exist_ok=True)
        backup = stage.with_name(stage.name+'-previous')
        if destination.exists():
            destination.rename(backup)
        try:
            stage.rename(destination)
        except OSError:
            if backup.exists():
                backup.rename(destination)
            raise
        if backup.exists():
            shutil.rmtree(backup)
        print(f'Core regression checks passed. Report: {destination / "report.json"}')
        return 0
    except Exception as exc:
        report['error'] = str(exc)
        report['status'] = 'failed'
        if stage.exists():
            (stage/'report.json').write_text(json.dumps(report, indent=2)+'\n')
        print(f'Failed; previous verified output retained. Evidence: {stage}\n{exc}')
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
