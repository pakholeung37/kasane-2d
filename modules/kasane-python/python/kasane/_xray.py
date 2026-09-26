"""X-ray presentation from an isolated renderer pass and evaluated geometry."""
from __future__ import annotations

from dataclasses import replace
import json

from ._deformation import _Raster, _image_point
from ._inspection import InspectionPacket, InspectionRequest, _failure

_GLYPHS = {
    "X": ("10001", "01010", "00100", "00100", "01010", "10001", "10001"),
    "R": ("11110", "10001", "10001", "11110", "10100", "10010", "10001"),
    "A": ("01110", "10001", "10001", "11111", "10001", "10001", "10001"),
    "Y": ("10001", "10001", "01010", "00100", "00100", "00100", "00100"),
}


def add_xray_view(observer, scene, packet: InspectionPacket,
                  request: InspectionRequest, selected: tuple[str, ...]) -> InspectionPacket:
    if not selected:
        raise ValueError("X-ray mode requires mesh or Part focus")
    if scene.evaluation_trace is None:
        raise _failure("CAPTURE_NOT_AVAILABLE", "X-ray requires an evaluation trace")
    if request.xray.include_disabled and not scene.metadata["hidden_geometry_captured"]:
        raise _failure("CAPTURE_NOT_AVAILABLE",
                       "Disabled X-ray requires include_hidden_geometry=True at capture")
    clean = packet.views[0]
    spec = request.xray
    frame_raw, (_, _, _), plan_json = scene._native.render_isolated(
        observer._native, clean.width, clean.height, clean.requested_roi,
        request.view.padding_canvas, "transparent", (0, 0, 0), (0, 0, 0),
        1, (0, 0), False, list(selected), spec.ignore_masks,
        spec.ignore_opacity, spec.include_disabled, False,
    )
    from ._observe import _frame_from_native
    isolated = _frame_from_native(frame_raw)
    if clean.rgba is None or len(isolated.rgba) != len(clean.rgba):
        raise _failure("INVALID_XRAY_FRAME", "X-ray and clean views differ in pixel extent")
    pixels = bytearray(clean.rgba)
    color = spec.highlight_rgb
    covered = 0
    for offset in range(0, len(pixels), 4):
        alpha = isolated.rgba[offset + 3]
        if alpha == 0:
            continue
        covered += 1
        weight = min(192, (alpha * 3 + 1) // 4)
        for channel in range(3):
            pixels[offset + channel] = (
                pixels[offset + channel] * (255 - weight) + color[channel] * weight + 127
            ) // 255
        pixels[offset + 3] = 255
    raster = _Raster(replace(clean, rgba=bytes(pixels)))
    selected_ids = set(selected)
    for mesh in scene.evaluation_trace["meshes"]:
        if mesh["id"] not in selected_ids or (not mesh["enabled"] and
                                               not spec.include_disabled):
            continue
        points = dict(zip(mesh["vertex_ids"], mesh["positions"]))
        for triangle in mesh["triangles"]:
            for a, b in ((0, 1), (1, 2), (2, 0)):
                raster.line(_image_point(clean, points[triangle[a]]),
                            _image_point(clean, points[triangle[b]]), color)

    # Mark the view in its pixels, not only in the report or filename.
    if clean.width >= 36 and clean.height >= 13:
        for y in range(2, 12):
            for x in range(2, 34):
                raster.dot(x, y, (12, 12, 12))
        for letter_index, char in enumerate("XRAY"):
            for gy, bits in enumerate(_GLYPHS[char]):
                for gx, bit in enumerate(bits):
                    if bit == "1":
                        raster.dot(5 + letter_index * 7 + gx, 3 + gy, color)
    plan = json.loads(plan_json)
    record = {"diagnostic": "XRAY", "source": "isolated_renderer_alpha_plus_evaluated_geometry",
              "selected_mesh_ids": list(selected), "occlusion_override": True,
              "geometry_outline_ignores_texture_alpha": True,
              "mask_and_opacity_gate_for_fill": not (spec.ignore_masks or spec.ignore_opacity),
              "highlight_rgb": color, "covered_pixels": covered,
              "overrides": plan["overrides"], "diagnostic_plan": plan,
              "alpha_status": "isolated_composite_alpha_not_color_contribution"}
    view = replace(raster.finish("xray", record), mode="xray")
    return replace(packet, views=(*packet.views, view))
