"""Render an unsaved masked Offscreen scene with the optional observe wheel.

Run from an output directory with a wheel built using `--features observe`.
"""

from pathlib import Path
import sys

import kasane


DOCUMENT = "00000000-0000-4000-8000-000000000001"
ASSET = "00000000-0000-4000-8000-000000000002"
TARGET = "00000000-0000-4000-8000-000000000003"
BACKGROUND = "00000000-0000-4000-8000-000000000004"
MASK = "00000000-0000-4000-8000-000000000005"
PART = "00000000-0000-4000-8000-000000000006"
OFFSCREEN = "00000000-0000-4000-8000-000000000007"
PARAMETER = "00000000-0000-4000-8000-000000000008"
BINDING = "00000000-0000-4000-8000-000000000009"


def create() -> kasane.Session:
    texture = Path(__file__).with_name("asymmetric-2x2.png").resolve()
    model = kasane.Session(DOCUMENT, 100, 100, (50, 50), 10)
    with model.edit("masked layer") as edit:
        edit.add_png_asset(ASSET, "texture", texture)
        edit.create_part(PART, "masked group")
        edit.create_rectangle(BACKGROUND, "background", ASSET, (20, 20), (80, 80))
        edit.create_rectangle(MASK, "mask", ASSET, (40, 40), (55, 60))
        edit.create_rectangle(TARGET, "target", ASSET, (40, 40), (60, 60))
        edit.set_mesh_part(TARGET, PART)
        edit.create_offscreen(kasane.OffscreenSpec(
            OFFSCREEN, "layer", PART, keyforms=[kasane.OffscreenKeyform(0.75)],
        ))
    base = model.mesh(TARGET).positions
    shifted = [(x + 8, y) for x, y in base]
    properties = model.mesh_properties(TARGET)
    with model.edit("animated mask") as edit:
        edit.update_mesh_properties(TARGET, kasane.MeshProperties(
            properties.texture_asset_id, properties.appearance,
            properties.draw_order, properties.blend_mode, properties.enabled,
            properties.double_sided, properties.inverted_mask, [MASK],
        ))
        edit.create_parameter(PARAMETER, "slide", 0, 1, 0)
        edit.create_mesh_binding(BINDING, TARGET, [kasane.Axis(PARAMETER, [0, 1])], [
            kasane.MeshKeyform([0], base), kasane.MeshKeyform([1], shifted),
        ])
    return model


def run(output: Path) -> kasane.ObservationRun:
    model = create()
    preview_before = model.preview_values
    with kasane.Observer(256, 256, 256) as observer:
        result = observer.observe_run(
            model, [{PARAMETER: value} for value in (0.0, 0.5, 1.0)],
            output.resolve(), focus=[TARGET],
        )
    assert model.preview_values == preview_before
    return result


if __name__ == "__main__":
    if len(sys.argv) > 2:
        raise SystemExit("usage: python_observe_recipe.py [absolute-output-directory]")
    output = Path(sys.argv[1]) if len(sys.argv) == 2 else Path.cwd() / "artifacts"
    result = run(output)
    print(result.report)
