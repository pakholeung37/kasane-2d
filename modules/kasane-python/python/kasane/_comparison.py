"""Registered image comparisons for frozen Observe packets.

The optional inspection extra supplies Pillow for decoding reference images.
All numeric measurements use the recorded renderer-native bytes; no color
normalization, exposure correction, or automatic alignment is performed.
"""
from __future__ import annotations

from dataclasses import dataclass
from io import BytesIO
import json
import math
from pathlib import Path
from typing import TYPE_CHECKING

from ._inspection import MAX_PAGE_PIXELS, _sha

if TYPE_CHECKING:
    from ._inspection import InspectionPacket, InspectionView


def _image_library():
    try:
        from PIL import Image
    except ImportError as error:
        raise RuntimeError(
            "Image comparison requires the inspection extra: pip install 'kasane[inspection]'"
        ) from error
    return Image


@dataclass(frozen=True)
class ExternalReference:
    path: Path
    alpha_policy: str = "opaque"
    color_policy: str = "renderer_native_v1"
    # Maps current image pixel coordinates to reference image coordinates.
    image_registration: tuple[float, float, float, float, float, float] | None = None

    def __post_init__(self) -> None:
        if not self.path.is_absolute() or not self.path.is_file():
            raise ValueError("External reference must be an existing absolute image path")
        if self.alpha_policy not in ("opaque", "straight"):
            raise ValueError("External reference alpha policy must be opaque or straight")
        if self.color_policy != "renderer_native_v1":
            raise ValueError("External reference color policy must be renderer_native_v1")
        _affine(self.image_registration)


@dataclass(frozen=True)
class CompareOptions:
    threshold: int = 16
    outline_threshold: int = 16
    heatmap_gain: float = 1.0
    onion_weight: float = 0.5
    target_roi: tuple[float, float, float, float] | None = None
    target_mesh_ids: tuple[str, ...] = ()
    # Maps current canvas coordinates to reference canvas coordinates.
    canvas_registration: tuple[float, float, float, float, float, float] | None = None
    object_map: tuple[tuple[str, str], ...] = ()

    def __post_init__(self) -> None:
        if (type(self.threshold) is not int or not 0 <= self.threshold <= 255 or
                type(self.outline_threshold) is not int or
                not 0 <= self.outline_threshold <= 255):
            raise ValueError("Comparison thresholds must be RGB8 values")
        if (not math.isfinite(self.heatmap_gain) or self.heatmap_gain <= 0 or
                not math.isfinite(self.onion_weight) or not 0 <= self.onion_weight <= 1):
            raise ValueError("Invalid heatmap gain or onion weight")
        if self.target_roi is not None:
            roi = self.target_roi
            if (len(roi) != 4 or not all(math.isfinite(v) for v in roi) or
                    roi[2] <= roi[0] or roi[3] <= roi[1]):
                raise ValueError("Target ROI must be finite with positive extent")
        _affine(self.canvas_registration)
        if (any(not isinstance(a, str) or not isinstance(b, str) or not a or not b
                for a, b in self.object_map) or
                len(set(a for a, _ in self.object_map)) != len(self.object_map) or
                len(set(b for _, b in self.object_map)) != len(self.object_map)):
            raise ValueError("Object map must contain unique nonempty ID pairs")


def _affine(values: tuple[float, ...] | None) -> None:
    if values is not None and (len(values) != 6 or
                               not all(math.isfinite(v) for v in values) or
                               abs(values[0] * values[4] - values[1] * values[3]) < 1e-12):
        raise ValueError("Registration must be an invertible finite six-value affine transform")


def _reference_bounds_in_current(
    bounds: tuple[float, float, float, float] | list[float],
    registration: tuple[float, ...] | None,
) -> tuple[float, float, float, float]:
    if registration is None:
        return tuple(bounds)
    a, b, c, d, e, f = registration
    determinant = a * e - b * d
    points = []
    for x in (bounds[0], bounds[2]):
        for y in (bounds[1], bounds[3]):
            rx, ry = x - c, y - f
            points.append(((e * rx - b * ry) / determinant,
                           (-d * rx + a * ry) / determinant))
    return (min(x for x, _ in points), min(y for _, y in points),
            max(x for x, _ in points), max(y for _, y in points))


@dataclass(frozen=True)
class ComparisonArtifact:
    kind: str
    width: int
    height: int
    png: bytes
    artifact_sha256: str


@dataclass(frozen=True)
class ComparisonResult:
    current_view_id: str
    reference_view_id: str | None
    reference_sha256: str
    registration_status: str
    view_compatibility: dict
    metrics: dict
    change_bounds: tuple[int, int, int, int] | None
    contour_source: str | None
    artifacts: tuple[ComparisonArtifact, ...]
    target_basis: dict
    thresholds: dict
    registration: dict | None = None


def _view(packet: InspectionPacket, view_id: str | None) -> InspectionView:
    if view_id is not None:
        matches = [view for view in packet.views if view.view_id == view_id]
    else:
        matches = [view for view in packet.views if view.kind in ("clean", "raw_context")]
    if len(matches) != 1:
        raise ValueError("Select exactly one clean or raw view by view_id")
    if matches[0].kind not in ("clean", "raw_context"):
        raise ValueError("Comparison input must be a clean or raw view")
    return matches[0]


def _rgba(view: InspectionView) -> bytes:
    if view.rgba is not None:
        return view.rgba
    Image = _image_library()
    with Image.open(BytesIO(view.png)) as image:
        if image.size != (view.width, view.height):
            raise ValueError("Saved view PNG dimensions differ from the manifest")
        image.load()
        return image.convert("RGBA").tobytes()


def _policy(view: InspectionView) -> dict:
    if view.kind == "raw_context":
        return {"kind": "raw", "alpha": "premultiplied_linear_rgba8_transparent_v1"}
    record = view.presentation or {}
    values = {key: record.get(key) for key in (
        "background", "kind", "light_rgb", "dark_rgb", "checker_tile_px",
        "checker_origin_px", "alpha", "color_policy", "target_format",
        "display_conversion", "texture_sampling", "unpremultiply",
    )}
    # Manifest JSON changes tuples to lists; normalize both live and reopened
    # records before compatibility checks and keep the policy readable.
    return json.loads(json.dumps(values, ensure_ascii=False, allow_nan=False))


def _aligned_packet_reference(current: InspectionView, reference: InspectionView,
                              registration: tuple[float, ...]) -> bytes:
    Image = _image_library()
    a, b, c, d, e, f = registration
    cs, (cx, cy) = current.view_scale, current.view_offset
    rs, (rx, ry) = reference.view_scale, reference.view_offset
    # PIL samples at pixel centres; AFFINE uses output pixel indices, so add
    # the half-pixel before canvas conversion and subtract it after mapping.
    transform = (
        rs * a / cs, rs * b / cs,
        rs * (a * (0.5 - cx) / cs + b * (0.5 - cy) / cs + c) + rx - 0.5,
        rs * d / cs, rs * e / cs,
        rs * (d * (0.5 - cx) / cs + e * (0.5 - cy) / cs + f) + ry - 0.5,
    )
    image = Image.frombytes("RGBA", (reference.width, reference.height), _rgba(reference))
    return image.transform((current.width, current.height), Image.Transform.AFFINE,
                           transform, resample=Image.Resampling.BILINEAR,
                           fillcolor=(0, 0, 0, 0)).tobytes()


def _aligned_external(current: InspectionView, reference: ExternalReference) -> tuple[bytes, str, tuple[int, int]]:
    Image = _image_library()
    if reference.path.stat().st_size > 512 * 1024 * 1024:
        raise ValueError("External reference exceeds the 512 MiB input budget")
    source = reference.path.read_bytes()
    with Image.open(BytesIO(source)) as loaded:
        if loaded.width <= 0 or loaded.height <= 0 or loaded.width * loaded.height > MAX_PAGE_PIXELS:
            raise ValueError("External reference exceeds the image budget")
        loaded.load()
        image = loaded.convert("RGBA")
    if reference.alpha_policy == "opaque" and image.getchannel("A").getextrema() != (255, 255):
        raise ValueError("External reference declares opaque alpha but contains transparency")
    source_size = image.size
    if reference.image_registration is None:
        return image.tobytes(), _sha(source), source_size
    a, b, c, d, e, f = reference.image_registration
    transformed = image.transform(
        (current.width, current.height), Image.Transform.AFFINE,
        (a, b, a * 0.5 + b * 0.5 + c - 0.5,
         d, e, d * 0.5 + e * 0.5 + f - 0.5),
        resample=Image.Resampling.BILINEAR, fillcolor=(0, 0, 0, 0),
    )
    return transformed.tobytes(), _sha(source), source_size


def _metric(current: bytes, reference: bytes, width: int, height: int,
            channels: tuple[int, ...], threshold: int,
            selected: list[bool]) -> dict:
    count = total = maximum = over = 0
    for pixel in range(width * height):
        if not selected[pixel]:
            continue
        count += 1
        delta = max(abs(current[pixel * 4 + channel] - reference[pixel * 4 + channel])
                    for channel in channels)
        total += sum(abs(current[pixel * 4 + channel] - reference[pixel * 4 + channel])
                     for channel in channels)
        maximum = max(maximum, delta)
        over += delta > threshold
    return {"status": "complete" if count else "empty_domain", "pixel_count": count,
            "mae": total / (count * len(channels)) if count else None,
            "max": maximum if count else None,
            "over_threshold_pixels": over,
            "over_threshold_ratio": over / count if count else None,
            "threshold": threshold}


def _artifact(kind: str, width: int, height: int, pixels: bytes) -> ComparisonArtifact:
    from ._observe import _encode_rgba_png
    png = _encode_rgba_png(width, height, pixels)
    return ComparisonArtifact(kind, width, height, png, _sha(png))


def _edge_map(pixels: bytes, width: int, height: int, source: str,
              threshold: int) -> list[bool]:
    result = [False] * (width * height)
    channels = (3,) if source == "alpha" else (0, 1, 2)
    for y in range(height):
        for x in range(width):
            index = y * width + x
            origin = index * 4
            for neighbour in (index + 1 if x + 1 < width else index,
                              index + width if y + 1 < height else index):
                if max(abs(pixels[origin + channel] - pixels[neighbour * 4 + channel])
                       for channel in channels) > threshold:
                    result[index] = True
                    break
    return result


def compare_observations(
    current: InspectionPacket,
    reference: InspectionPacket | ExternalReference,
    *, view_id: str | None = None, reference_view_id: str | None = None,
    options: CompareOptions = CompareOptions(),
) -> ComparisonResult:
    """Compare compatible frozen pixels, or show an unregistered reference side by side."""
    from ._inspection import InspectionPacket

    view = _view(current, view_id)
    current_pixels = _rgba(view)
    width, height = view.width, view.height
    if width * height > MAX_PAGE_PIXELS or width * 2 * height > MAX_PAGE_PIXELS:
        raise ValueError("Comparison images exceed the 16 million pixel page budget")
    if len(current_pixels) != width * height * 4:
        raise ValueError("Current view has an invalid RGBA byte count")
    if options.target_roi is not None and options.target_mesh_ids:
        raise ValueError("Choose target_roi or target_mesh_ids, not both")

    external = isinstance(reference, ExternalReference)
    reference_id = None
    aligned = False
    comparison_policy = _policy(view)
    if external:
        if reference_view_id is not None or options.canvas_registration is not None:
            raise ValueError("External references use image_registration")
        if view.kind != "clean" or (view.presentation or {}).get("alpha") != reference.alpha_policy:
            raise ValueError("External reference alpha policy differs from the clean view")
        if (view.presentation or {}).get("color_policy") != reference.color_policy:
            raise ValueError("External reference color policy differs from the clean view")
        reference_pixels, reference_hash, reference_size = _aligned_external(view, reference)
        aligned = reference.image_registration is not None
    elif isinstance(reference, InspectionPacket):
        other = _view(reference, reference_view_id)
        reference_size = (other.width, other.height)
        reference_id, reference_hash = other.view_id, other.artifact_sha256
        if _policy(other) != comparison_policy or other.kind != view.kind:
            raise ValueError("INCOMPATIBLE_PRESENTATION: pixel policies differ")
        if current.source_kind != reference.source_kind:
            raise ValueError("INCOMPATIBLE_SAMPLE: source kinds differ")
        if (current.source_kind == "animation" and
                current.source.get("apply_model_opacity") !=
                reference.source.get("apply_model_opacity")):
            raise ValueError("INCOMPATIBLE_SAMPLE: animation opacity policies differ")
        if current.document_id != reference.document_id and options.canvas_registration is None:
            raise ValueError("UNREGISTERED_CANVAS: cross-document comparison needs registration")
        if options.target_mesh_ids and current.document_id != reference.document_id:
            mapped = dict(options.object_map)
            if any(mesh_id not in mapped for mesh_id in options.target_mesh_ids):
                raise ValueError("UNMAPPED_OBJECT: target meshes need an object map")
            reference_meshes = {row["id"] for row in reference.objects
                                if row["kind"] == "mesh"}
            if any(mapped[mesh_id] not in reference_meshes
                   for mesh_id in options.target_mesh_ids):
                raise ValueError("UNMAPPED_OBJECT: mapped reference mesh does not exist")
        same_view = (other.width == width and other.height == height and
                     other.canvas_to_image_matrix == view.canvas_to_image_matrix)
        if options.canvas_registration is None and not same_view:
            raise ValueError("INCOMPATIBLE_VIEW: comparison needs one fixed view or registration")
        aligned = True
        reference_pixels = (_rgba(other) if options.canvas_registration is None
                            else _aligned_packet_reference(view, other,
                                                           options.canvas_registration))
    else:
        raise TypeError("Reference must be an InspectionPacket or ExternalReference")

    if external and not aligned:
        # Unregistered images can be displayed, but there is no common pixel domain.
        Image = _image_library()
        source = Image.frombytes("RGBA", (width, height), current_pixels)
        other_image = Image.frombytes("RGBA", reference_size, reference_pixels)
        page_height = max(height, other_image.height)
        if (width + other_image.width) * page_height > MAX_PAGE_PIXELS:
            raise ValueError("Unregistered comparison sheet exceeds the page budget")
        side = Image.new("RGBA", (width + other_image.width, page_height), (0, 0, 0, 0))
        side.paste(source, (0, 0))
        side.paste(other_image, (width, 0))
        artifact = _artifact("side_by_side", side.width, side.height, side.tobytes())
        return ComparisonResult(
            view.view_id, None, reference_hash, "unregistered",
            {"status": "unregistered", "current_size": (width, height),
             "reference_size": reference_size, "current_policy": comparison_policy},
            {"status": "unavailable", "reason": "unregistered_reference"},
            None, None, (artifact,), {"status": "unavailable"},
            {"difference": options.threshold, "outline": options.outline_threshold,
             "heatmap_range": (0, 255), "heatmap_gain": options.heatmap_gain},
            {"kind": "none"},
        )

    if len(reference_pixels) != len(current_pixels):
        raise ValueError("Registered reference has an invalid RGBA byte count")
    target_roi = options.target_roi
    basis = {"status": "whole_view"}
    if options.target_mesh_ids:
        rows = {row["id"]: row for row in current.objects if row["kind"] == "mesh"}
        bounds = []
        reference_bounds_added = 0
        other_rows = ({} if external else
                      {row["id"]: row for row in reference.objects
                       if row["kind"] == "mesh"})
        object_map = dict(options.object_map)
        for mesh_id in options.target_mesh_ids:
            if mesh_id not in rows:
                raise ValueError(f"Unknown comparison target mesh: {mesh_id}")
            if rows[mesh_id]["geometry_bounds_canvas"] is not None:
                bounds.append(rows[mesh_id]["geometry_bounds_canvas"])
            reference_id_for_mesh = object_map.get(mesh_id, mesh_id)
            reference_row = other_rows.get(reference_id_for_mesh)
            if (reference_row is not None and
                    reference_row["geometry_bounds_canvas"] is not None):
                bounds.append(_reference_bounds_in_current(
                    reference_row["geometry_bounds_canvas"],
                    options.canvas_registration,
                ))
                reference_bounds_added += 1
        if not bounds:
            raise ValueError("Comparison target meshes have no evaluated geometry")
        target_roi = (min(row[0] for row in bounds), min(row[1] for row in bounds),
                      max(row[2] for row in bounds), max(row[3] for row in bounds))
        basis = {"status": "geometry_union", "mesh_ids": options.target_mesh_ids,
                 "roi_canvas": target_roi,
                 "reference_geometry_status": (
                     "included" if reference_bounds_added else "unavailable")}
    elif target_roi is not None:
        basis = {"status": "explicit_roi", "roi_canvas": target_roi}
    selected = []
    for y in range(height):
        for x in range(width):
            cx, cy = view.image_to_canvas((x + 0.5, y + 0.5))
            selected.append(target_roi is None or
                            (target_roi[0] <= cx < target_roi[2] and
                             target_roi[1] <= cy < target_roi[3]))
    outside = [not value for value in selected]
    rgb_available = view.kind == "clean" and (view.presentation or {}).get("alpha") == "opaque"
    metric_channels = (0, 1, 2) if rgb_available else (0, 1, 2, 3)
    metric_name = "opaque_rgb" if rgb_available else "compatible_raw_rgba"
    whole = [True] * (width * height)

    def regions(first: bytes, second: bytes, channels: tuple[int, ...]) -> dict:
        measure = lambda mask: _metric(first, second, width, height, channels,
                                       options.threshold, mask)
        if target_roi is None:
            target = {"status": "unavailable", "reason": "target_region_not_declared"}
            non_target = {"status": "unavailable", "reason": "target_region_not_declared"}
        else:
            target, non_target = measure(selected), measure(outside)
        return {"target": target, "non_target": non_target,
                "whole_view": measure(whole)}

    metrics = {metric_name: regions(current_pixels, reference_pixels, metric_channels)}
    alpha_current = next((item for item in current.views if item.kind == "alpha" and
                          item.sample_index == view.sample_index), None)
    alpha_reference = None if external else next(
        (item for item in reference.views if item.kind == "alpha" and
         item.sample_index == other.sample_index), None)
    if view.kind == "raw_context":
        metrics["transparent_alpha"] = regions(current_pixels, reference_pixels, (3,))
    elif (view.presentation or {}).get("alpha") == "straight":
        metrics["transparent_alpha"] = regions(current_pixels, reference_pixels, (3,))
    elif alpha_current and alpha_reference and options.canvas_registration is None:
        first, second = _rgba(alpha_current), _rgba(alpha_reference)
        metrics["transparent_alpha"] = regions(first, second, (0,))
    else:
        metrics["transparent_alpha"] = {"status": "unavailable",
                                         "reason": "compatible_alpha_view_missing"}
    if view.kind == "clean" and not rgb_available:
        metrics["opaque_rgb"] = {"status": "unavailable",
                                  "reason": "presentation_is_not_opaque"}
        metrics.pop("compatible_raw_rgba")
    if view.kind != "raw_context":
        metrics["compatible_raw_rgba"] = {"status": "unavailable",
                                          "reason": "compatible_raw_views_missing"}

    side = bytearray(width * 2 * height * 4)
    onion = bytearray(len(current_pixels))
    heat = bytearray(len(current_pixels))
    outline = bytearray(current_pixels)
    changed_x, changed_y = [], []
    weight = options.onion_weight
    for y in range(height):
        for x in range(width):
            index = y * width + x
            start = index * 4
            row_start = (y * width * 2 + x) * 4
            side[row_start:row_start + 4] = current_pixels[start:start + 4]
            side[row_start + width * 4:row_start + (width + 1) * 4] = reference_pixels[start:start + 4]
            for channel in range(4):
                onion[start + channel] = round((1 - weight) * current_pixels[start + channel] +
                                               weight * reference_pixels[start + channel])
            difference = max(abs(current_pixels[start + channel] - reference_pixels[start + channel])
                             for channel in metric_channels)
            intensity = min(255, round(difference * options.heatmap_gain))
            heat[start:start + 4] = bytes((intensity, 0, 0, 255))
            if difference > options.threshold:
                changed_x.append(x)
                changed_y.append(y)
    contour_source = "alpha" if (
        view.kind == "raw_context" or
        ((view.presentation or {}).get("alpha") == "straight")
    ) else "color_edge"
    current_edges = _edge_map(current_pixels, width, height, contour_source,
                              options.outline_threshold)
    reference_edges = _edge_map(reference_pixels, width, height, contour_source,
                                options.outline_threshold)
    for index in range(width * height):
        if current_edges[index] and reference_edges[index]:
            outline[index * 4:index * 4 + 4] = bytes((255, 255, 255, 255))
        elif current_edges[index]:
            outline[index * 4:index * 4 + 4] = bytes((255, 64, 64, 255))
        elif reference_edges[index]:
            outline[index * 4:index * 4 + 4] = bytes((64, 240, 255, 255))
    bounds = ((min(changed_x), min(changed_y), max(changed_x) + 1, max(changed_y) + 1)
              if changed_x else None)
    return ComparisonResult(
        view.view_id, reference_id, reference_hash, "registered",
        {"status": "compatible", "current_size": (width, height),
         "reference_size": reference_size, "aligned_size": (width, height),
         "pixel_policy": comparison_policy,
         "external_reference_policy": ({"alpha": reference.alpha_policy,
                                        "color": reference.color_policy}
                                       if external else None),
         "resampling": "bilinear" if
         (options.canvas_registration is not None or external) else "none"},
        metrics, bounds, contour_source,
        (_artifact("side_by_side", width * 2, height, side),
         _artifact("onion_skin", width, height, onion),
         _artifact("outline", width, height, outline),
         _artifact("abs_diff_heatmap", width, height, heat)),
        basis,
        {"difference": options.threshold, "outline": options.outline_threshold,
         "heatmap_range": (0, 255), "heatmap_gain": options.heatmap_gain},
        ({"kind": "image_affine", "coefficients": reference.image_registration}
         if external else
         {"kind": "canvas_affine", "coefficients": options.canvas_registration}
         if options.canvas_registration is not None else
         {"kind": "identical_view"}),
    )
