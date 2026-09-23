"""Capture the imported model used by the independent Godot/wgpu image reference."""

from pathlib import Path
import sys

import kasane


DOCUMENT = "30000000-0000-4000-8000-000000000001"


def run(model3: Path, output: Path) -> Path:
    if not model3.is_absolute() or not output.is_absolute():
        raise ValueError("Model3 and output paths must be absolute")
    session = kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)
    imported = session.import_model3(model3)
    if imported.diagnostics or session.diagnose_resources():
        raise RuntimeError("External model resources are incomplete")
    with kasane.Observer(256, 256, 256) as observer:
        observation = observer.observe_run(session, [{}], output, focus=session.mesh_ids())
    return observation.report


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: python_reference_capture.py /absolute/model3 /absolute/output")
    print(run(Path(sys.argv[1]), Path(sys.argv[2])))
