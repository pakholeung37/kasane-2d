#!/usr/bin/env python3
"""Submit a complete GDScript to a running Kasane editor, then read its result."""
import argparse
import json
import os
from pathlib import Path
import time
import uuid


def atomic_write(path, text):
    temporary = path.with_name(path.name + '.' + uuid.uuid4().hex + '.tmp')
    with temporary.open('x') as stream:
        stream.write(text)
        stream.flush()
        os.fsync(stream.fileno())
    temporary.replace(path)


def submit(directory, source, *, observe=False, object_id='', execution_id=None, timeout=30):
    status = json.loads((directory / 'status.json').read_text())
    if not status.get('running'):
        raise RuntimeError('The editor is not running')
    execution_id = execution_id or str(uuid.uuid4())
    if not execution_id or any(c not in 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_' for c in execution_id):
        raise ValueError('Execution ID must contain only ASCII letters, digits, underscore or hyphen')
    result = directory / 'results' / (execution_id + '.json')
    if result.exists():
        return json.loads(result.read_text())
    script = directory / 'scripts' / (execution_id + '.gd')
    # Never overwrite the source of an already published request.
    if not script.exists():
        atomic_write(script, source)
    elif script.read_text() != source:
        raise ValueError('Execution ID already belongs to a different script')
    request = directory / 'requests' / (execution_id + '.json')
    if not request.exists():
        atomic_write(request, json.dumps({'id': execution_id, 'app_id': status['app_id'],
            'generation': status['generation'], 'script_path': str(script.resolve()),
            'observe': observe, 'object_id': object_id}))
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if result.exists():
            return json.loads(result.read_text())
        time.sleep(0.1)
    raise TimeoutError(f'No result yet for {execution_id}. Execution may still be running; no cancellation was sent. Retry the same ID.')


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--directory', required=True, type=Path)
    p.add_argument('--script', required=True, type=Path)
    p.add_argument('--observe', action='store_true')
    p.add_argument('--object-id', default='')
    p.add_argument('--id')
    p.add_argument('--timeout', type=float, default=30)
    args = p.parse_args()
    result = submit(args.directory.resolve(), args.script.read_text(), observe=args.observe,
                    object_id=args.object_id, execution_id=args.id, timeout=args.timeout)
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0 if result.get('ok') else 1


if __name__ == '__main__':
    raise SystemExit(main())
