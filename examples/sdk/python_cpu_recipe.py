"""Create a small bound model with the installed kasane wheel.

Run from an empty output directory: python /absolute/path/to/python_cpu_recipe.py
"""

from pathlib import Path

import kasane

DOCUMENT = "00000000-0000-4000-8000-000000000001"
ASSET = "00000000-0000-4000-8000-000000000002"
MESH = "00000000-0000-4000-8000-000000000003"
PARAMETER = "00000000-0000-4000-8000-000000000004"
BINDING = "00000000-0000-4000-8000-000000000005"


def create(output: Path) -> kasane.Session:
    texture = Path(__file__).with_name("asymmetric-2x2.png").resolve()
    session = kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)
    with session.edit("create rectangle") as edit:
        edit.add_png_asset(ASSET, "texture", texture)
        edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))

    base = session.mesh(MESH).positions
    shifted = [(x + 10, y) for x, y in base]
    with session.edit("bind open parameter") as edit:
        edit.create_parameter(PARAMETER, "open", 0, 1, 0)
        edit.create_mesh_binding(
            BINDING,
            MESH,
            [kasane.Axis(PARAMETER, [0, 1])],
            [kasane.MeshKeyform([0], base), kasane.MeshKeyform([1], shifted)],
        )

    middle = session.evaluate({PARAMETER: 0.5})
    assert middle.parameters[0].value == 0.5
    assert middle.drawables[0].positions[0] == (-0.5, 1.0)
    receipt = session.save(output)
    reopened = kasane.open_project(receipt.manifest)
    assert reopened.evaluate({PARAMETER: 0.5}) == middle
    return session


if __name__ == "__main__":
    result = create(Path.cwd() / "kasane-python-example")
    print(result.project_path)
