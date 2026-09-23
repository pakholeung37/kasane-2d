"""S5 flow 3, script one: create a model, ID list, and image evidence."""

from __future__ import annotations

import json
from pathlib import Path
import sys

import kasane

from python_two_asset_recipe import MESH_B, PARAMETER, WARP, run as create_model


def run(output: Path) -> Path:
    if not output.is_absolute():
        raise ValueError("Output path must be absolute")
    creation_report = create_model(output / "draft")
    creation = json.loads(creation_report.read_text(encoding="utf-8"))
    session = kasane.open_project(Path(creation["project_manifest"]))
    with kasane.Observer(256, 256, 256) as observer:
        observation = observer.observe_run(
            session, [{PARAMETER: 0.5}], output / "draft-observation", focus=[MESH_B],
        )
    handoff = {
        "schema_version": 1,
        "creation_report": str(creation_report),
        "project_manifest": creation["project_manifest"],
        "observation_report": str(observation.report),
        "mesh_ids": creation["mesh_ids"],
        "target_mesh_id": MESH_B,
        "warp_id": WARP,
        "parameter_id": PARAMETER,
    }
    destination = output / "handoff.json"
    destination.write_text(json.dumps(handoff, indent=2) + "\n", encoding="utf-8")
    return destination


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: python_agent_draft.py /absolute/output/directory")
    print(run(Path(sys.argv[1])))
