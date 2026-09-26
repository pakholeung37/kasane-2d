"""Read exact renderer mask attachments and present their alpha as grayscale."""
from __future__ import annotations

from dataclasses import replace
import math
from uuid import uuid4

from ._inspection import (InspectionPacket, InspectionRequest, InspectionView,
                          _failure, _json, _sha)
from ._observe import _encode_rgba_png


def _grayscale(raw: bytes, inverted: bool = False) -> bytes:
    pixels = bytearray(len(raw))
    for offset in range(0, len(raw), 4):
        alpha = raw[offset + 3]
        if inverted:
            alpha = 255 - alpha
        pixels[offset:offset + 4] = bytes((alpha, alpha, alpha, 255))
    return bytes(pixels)


def _attachment_view(clean: InspectionView, kind: str, metadata: dict,
                     width: int, height: int, raw: bytes,
                     inverted: bool = False) -> InspectionView:
    if len(raw) != width * height * 4 or width <= 0 or height <= 0:
        raise _failure("INVALID_MASK_ATTACHMENT", "Mask dimensions differ from readback")
    origin = metadata["origin_canvas"]
    scale = metadata["scale"]
    if scale <= 0:
        raise _failure("INVALID_MASK_ATTACHMENT", "Mask scale must be positive")
    extent = (origin[0], origin[1], origin[0] + width / scale,
              origin[1] + height / scale)
    pixels = _grayscale(raw, inverted)
    png = _encode_rgba_png(width, height, pixels)
    record = {**metadata, "alpha_encoding": "grayscale_rgb8_from_mask_alpha",
              "inversion_applied": inverted,
              "raw_attachment_sha256": _sha(raw),
              "mask_to_canvas": ((1 / scale, 0, origin[0]),
                                 (0, 1 / scale, origin[1]), (0, 0, 1)),
              "canvas_to_mask": ((scale, 0, -origin[0] * scale),
                                 (0, scale, -origin[1] * scale), (0, 0, 1))}
    return replace(clean, view_id=uuid4().hex, kind=kind, mode="context",
                   width=width, height=height, png=png, rgba=pixels, frame=None,
                   requested_roi=extent, padded_roi=extent, visible_roi=extent,
                   view_scale=scale,
                   view_offset=(-origin[0] * scale, -origin[1] * scale),
                   artifact_sha256=_sha(png), presentation=record,
                   render_digest=_sha(_json({"scene": clean.render_digest,
                                             "mask": record})))


def _mask_pixels(clean: InspectionView, sources: list[str], evaluated: dict,
                 offscreen: bool) -> int:
    """Conservative preflight using the renderer's 4 px grown mask bounds."""
    points = [clean.runtime_to_canvas((point["x"], point["y"]))
              for source_id in sources for point in evaluated[source_id]["positions"]]
    if points:
        width = math.ceil(max(point[0] for point in points) -
                          min(point[0] for point in points) + 8)
        height = math.ceil(max(point[1] for point in points) -
                           min(point[1] for point in points) + 8)
    else:
        width = height = 8
    width, height = max(width, 1), max(height, 1)
    scale = max(clean.view_scale, 1) if offscreen else clean.view_scale
    scale = min(scale, 4096 / max(width, height))
    return (math.ceil(width * scale) + 1) * (math.ceil(height * scale) + 1)


def add_mask_views(observer, scene, packet: InspectionPacket,
                   request: InspectionRequest, selected: tuple[str, ...]) -> InspectionPacket:
    """Attach per-source and combined raw mask targets for one focused consumer."""
    selected_meshes = [mesh for mesh in scene.authoring["meshes"] if mesh["id"] in selected]
    if len(selected_meshes) != 1:
        raise ValueError("Mask channel requires exactly one focused mesh")
    mesh = selected_meshes[0]
    paths = {row["id"]: row.get("composition_path") or () for row in packet.objects
             if row["kind"] == "mesh"}
    offscreens = {group["id"]: group for group in scene.authoring.get("offscreens", ())}
    consumers = [("offscreen", offscreens[id]) for id in paths[mesh["id"]]
                 if id in offscreens and offscreens[id]["masks"]]
    if mesh["masks"]:
        consumers.append(("mesh", mesh))
    if not consumers:
        raise ValueError("Selected mesh and its composition path have no masks")
    clean = packet.views[0]
    roi = clean.requested_roi
    resolution = request.view.resolution
    padding = request.view.padding_canvas
    views = list(packet.views)
    evaluated = {item["id"]: item for item in scene.evaluated_frame["drawables"]}
    authored = {item["id"]: item for item in scene.authoring["meshes"]}
    future_views = 1 + sum(len(consumer["masks"]) + 2 for _, consumer in consumers)
    future_pixels = clean.width * clean.height
    for kind, consumer in consumers:
        offscreen = kind == "offscreen"
        future_pixels += sum(_mask_pixels(clean, [source], evaluated, offscreen)
                             for source in consumer["masks"])
        future_pixels += 2 * _mask_pixels(clean, consumer["masks"], evaluated,
                                          offscreen)
    if (len(views) + future_views > 64 or
            sum(view.width * view.height for view in views) + future_pixels >
            request.limits.max_artifact_pixels or
            future_pixels * 4 > request.limits.max_cpu_retained_bytes):
        raise _failure("OBSERVATION_BUDGET_EXCEEDED",
                       "Mask views exceed view, artifact, or retained image budget")
    import json
    for consumer_kind, consumer in consumers:
        consumer_id, sources = consumer["id"], consumer["masks"]
        for source_id in sources:
            meta_json, width, height, raw, _ = scene._native.render_mask(
                observer._native, *resolution, roi, padding, consumer_id, source_id,
            )
            metadata = json.loads(meta_json)
            metadata.update(mask_source_id=source_id, consumer_kind=consumer_kind,
                            composition_path=paths[mesh["id"]])
            source = evaluated[source_id]
            metadata["source_geometry"] = {
                "space": "canvas", "vertex_ids": authored[source_id]["vertex_ids"],
                "positions": [clean.runtime_to_canvas((point["x"], point["y"]))
                              for point in source["positions"]],
                "triangle_indices": source["indices"],
                "geometry_status": "evaluated" if source["enabled"] else "disabled",
            }
            views.append(_attachment_view(clean, "mask_source", metadata,
                                          width, height, raw))
        meta_json, width, height, raw, _ = scene._native.render_mask(
            observer._native, *resolution, roi, padding, consumer_id, None,
        )
        metadata = json.loads(meta_json)
        metadata.update(consumer_kind=consumer_kind,
                        composition_path=paths[mesh["id"]])
        views.append(_attachment_view(clean, "mask_combined", metadata,
                                      width, height, raw))
        views.append(_attachment_view(clean, "mask_consumer", metadata,
                                      width, height, raw, bool(metadata["inverted"])))

    # This is the consumer's alpha after actual mask and target composition in
    # an isolated transparent render. It is a coverage aid, not color contribution.
    frame_raw, (requested, padded, visible), plan_json = scene._native.render_isolated(
        observer._native, *resolution, roi, padding, "transparent",
        (0, 0, 0), (0, 0, 0), 1, (0, 0), False, [mesh["id"]],
        False, False, False,
    )
    from ._observe import _frame_from_native
    frame = _frame_from_native(frame_raw)
    pixels = _grayscale(frame.rgba)
    png = _encode_rgba_png(frame.width, frame.height, pixels)
    record = {"consumer_id": mesh["id"],
              "source_ids": sorted({source for _, consumer in consumers
                                     for source in consumer["masks"]}),
              "masked_consumer_ids": [consumer["id"] for _, consumer in consumers],
              "alpha_encoding": "grayscale_rgb8_from_isolated_composite_alpha",
              "destination_context": "isolated",
              "isolated_plan": json.loads(plan_json),
              "coverage_status": "composite_alpha_not_color_contribution"}
    views.append(replace(clean, view_id=uuid4().hex, kind="mask_coverage",
                         png=png, rgba=pixels, frame=None,
                         requested_roi=requested, padded_roi=padded, visible_roi=visible,
                         artifact_sha256=_sha(png), presentation=record,
                         render_digest=_sha(_json({"scene": scene.scene_digest,
                                                   "coverage": record}))))
    if (len(views) > 64 or sum(view.width * view.height for view in views) >
            request.limits.max_artifact_pixels):
        raise _failure("OBSERVATION_BUDGET_EXCEEDED", "Mask views exceed artifact pixel limit")
    return replace(packet, views=tuple(views))
