"""Combine explicit M3B evidence; absent or failed gates never count as passed."""
import hashlib
import json
from pathlib import Path


def collect(numerical, baseline_path: Path, editor_path: Path, gpu_path: Path):
    evidence = {}
    for name, path in [('dual_core_baseline', baseline_path), ('editor', editor_path), ('synthetic_gpu', gpu_path)]:
        if not path.is_file():
            evidence[name] = {'status':'not_run', 'path':str(path)}
            continue
        raw = path.read_bytes()
        data = json.loads(raw)
        evidence[name] = {'path':str(path.resolve()), 'sha256':hashlib.sha256(raw).hexdigest(),
                          'status':data.get('status', 'failed'), 'report':data}
    editor = evidence['editor'].get('report', {})
    gates = dict(editor.get('gates', {}))
    for name in ['gpu_comparison','packaged_editor_workflow','detached_texture_project','new_feature_edit_roundtrips']:
        gates.setdefault(name, {'status':'not_run'})
    for name in ['dual_core_baseline','synthetic_gpu']:
        gates[name] = {k:v for k,v in evidence[name].items() if k!='report'}
    source_hashes = {c.get('inputs', {}).get('orig', {}).get('sha256') for c in numerical.get('cases', [])}
    baseline_hash = evidence['dual_core_baseline'].get('report', {}).get('model', {}).get('moc3_sha256')
    if source_hashes != {editor.get('source_sha256')} or source_hashes != {baseline_hash} or None in source_hashes:
        gates['input_identity'] = {'status':'failed', 'reason':'Numerical, baseline and application inputs differ or are absent'}
    else:
        gates['input_identity'] = {'status':'passed', 'sha256':baseline_hash}
    numerical['required_acceptance'] = gates
    numerical['acceptance_evidence'] = {k:{f:v for f,v in e.items() if f!='report'} for k,e in evidence.items()}
    numerical['status'] = 'passed' if numerical.get('numerical_status') == 'passed' and all(g.get('status') == 'passed' for g in gates.values()) and editor.get('status') == 'passed' else 'failed'
    return numerical
