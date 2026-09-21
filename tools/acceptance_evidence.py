"""Evidence-backed acceptance status and checkout identity; no inferred passes."""
import hashlib
import json
from pathlib import Path
import subprocess


def artifact(path, kind):
    path = Path(path).resolve()
    return {'path': str(path), 'kind': kind,
            'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


def source_identity(root):
    root = Path(root)
    files = subprocess.check_output(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard',
                                    '--', 'Cargo.toml', 'Cargo.lock', 'modules', 'apps', 'tools', 'tests'], cwd=root).split(b'\0')
    digest = hashlib.sha256()
    for name in sorted(set(files)):
        if not name: continue
        path = root / name.decode()
        if not path.is_file() or path.suffix == '.md':
            continue
        digest.update(name + b'\0' + hashlib.sha256(path.read_bytes()).digest())
    submodules = subprocess.check_output(['git', 'submodule', 'status'], cwd=root)
    digest.update(submodules)
    # Native source is a submodule; include local edits, not just its HEAD.
    native = root / 'modules/purism-core'
    if native.exists():
        for path in sorted(native.rglob('*')):
            if path.is_file() and path.suffix in {'.c', '.h'}:
                digest.update(str(path.relative_to(root)).encode() + hashlib.sha256(path.read_bytes()).digest())
    return {'revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
            'source_sha256': digest.hexdigest(), 'submodules': submodules.decode().strip()}


def check(id, kind, status, evidence=(), reason='', required=True):
    return dict(id=id, kind=kind, status=status, evidence=list(evidence), reason=reason, required=required)


def missing(id, kind, reason):
    return check(id, kind, 'not_run', reason=reason)


def finalize(report):
    required = [c for c in report.get('checks', []) if c.get('required', True)]
    for c in required:
        if c.get('status') == 'passed':
            if not c.get('evidence'):
                c.update(status='not_run', reason='No recorded evidence')
            else:
                for item in c['evidence']:
                    try:
                        actual = artifact(item['path'], item['kind'])['sha256']
                        if actual != item['sha256']: raise ValueError('Evidence changed')
                    except (OSError, KeyError, ValueError) as exc:
                        c.update(status='failed', reason=str(exc))
                        break
    status = ('failed' if any(c.get('status') == 'failed' for c in required) else
              'passed' if required and all(c.get('status') == 'passed' for c in required) else 'not_run')
    report['status'] = status
    report['gate'] = {c['id']: c['status'] for c in required}
    report['gate']['passed'] = status == 'passed'
    return report


def reusable(report, root):
    return report.get('provenance') == source_identity(root) and finalize(report)['status'] == 'passed'
