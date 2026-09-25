"""Expand a real Kasane project while preserving its mesh/keyform relationships.

Usage: python3 tools/make_project_stress_fixture.py SOURCE_DIR OUTPUT_DIR COPIES
COPIES includes the original set of meshes and bindings. Assets are hard-linked.
"""

import copy
import json
import os
from pathlib import Path
import sys
import uuid


def main() -> None:
    if len(sys.argv) != 4:
        raise SystemExit(__doc__)
    source, output = (Path(arg).resolve() for arg in sys.argv[1:3])
    copies = int(sys.argv[3])
    if copies < 1:
        raise ValueError("COPIES must be positive")
    output.mkdir(parents=True, exist_ok=True)
    input_data = json.loads((source / "project.kasane.json").read_text())
    document = input_data["document"]
    original_meshes = list(document["meshes"])
    original_bindings = list(document["bindings"])
    if any(document.get(key) for key in ("blend_bindings", "glues", "draw_order_groups")):
        raise ValueError("Source has additional mesh relationships; choose a different fixture")
    if any(binding["mesh_id"] not in {m["id"] for m in original_meshes} for binding in original_bindings):
        raise ValueError("Binding references a missing mesh")

    for index in range(1, copies):
        ids = {m["id"]: str(uuid.uuid5(uuid.NAMESPACE_URL, f"kasane-stress/{index}/{m['id']}"))
               for m in original_meshes}
        for original in original_meshes:
            mesh = copy.deepcopy(original)
            mesh["id"] = ids[original["id"]]
            mesh["runtime_id"] = f"{original['runtime_id']}_stress_{index}"
            mesh["name"] = f"{original['name']} stress {index}"
            mesh["properties"]["masks"] = [ids.get(mask, mask) for mask in mesh["properties"]["masks"]]
            document["meshes"].append(mesh)
        for original in original_bindings:
            binding = copy.deepcopy(original)
            binding["id"] = str(uuid.uuid5(uuid.NAMESPACE_URL, f"kasane-stress/{index}/{original['id']}"))
            binding["mesh_id"] = ids[original["mesh_id"]]
            document["bindings"].append(binding)

    assets = output / "assets"
    assets.mkdir(exist_ok=True)
    for path in (source / "assets").iterdir():
        target = assets / path.name
        if not target.exists():
            os.link(path, target)
    (output / "project.kasane.json").write_text(
        json.dumps(input_data, ensure_ascii=False, indent=2) + "\n"
    )
    print(f"{output}: {len(document['meshes'])} meshes, {len(document['bindings'])} bindings")


if __name__ == "__main__":
    main()
