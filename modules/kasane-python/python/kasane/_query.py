"""Image-coordinate triangle queries over a frozen inspection packet."""
from __future__ import annotations

from dataclasses import dataclass, replace
import json
import math

from ._inspection import InspectionPacket, _failure, _unavailable


@dataclass(frozen=True)
class PixelProbe:
    sample_pixel: tuple[int, int] | None
    raw_rgba: tuple[int, int, int, int] | None
    raw_status: str
    presentation_rgba: tuple[int, int, int, int] | None
    presentation_status: str
    raw_view_id: str | None
    presentation_view_id: str | None


@dataclass(frozen=True)
class QueryHit:
    object_id: str
    mark: int | None
    triangle_key: str
    vertex_ids: tuple[int, int, int]
    barycentric: tuple[float, float, float] | None
    image_point: tuple[float, float] | None
    canvas_point: tuple[float, float] | None
    runtime_point: tuple[float, float] | None
    uv: tuple[float, float] | None
    coverage_status: str
    coverage_value: float | None
    composition_path: tuple[str, ...]
    render_order: int | None
    mask_ids: tuple[str, ...]
    geometry_space: str
    deformer_parent: str | None
    hit_reason: str
    edit_mapping_status: str
    editable_point: tuple[float, float] | None
    source_revision: int
    region_intersection_area: float | None = None
    coverage_pixel_count: int | None = None
    coverage_bounds: tuple[int, int, int, int] | None = None
    binding_provenance_ref: str | None = None


@dataclass(frozen=True)
class QueryResult:
    view_id: str
    mode: str
    requested_point: tuple[float, float] | None
    requested_region: tuple[int, int, int, int] | None
    sample_point: tuple[float, float] | None
    status: str
    hits: tuple[QueryHit, ...]
    total: int
    truncated: bool
    pixel_probe: PixelProbe | None
    frontmost_object_id: str | None = None
    pick_rule: str | None = None
    resources: dict | None = None


@dataclass(frozen=True)
class ObjectDetails:
    object_id: str
    kind: str
    authored_metadata: dict
    evaluated_geometry: dict | None
    topology_hash: str | None
    binding_provenance: dict
    edit_mapping_status: str
    diagnostics: tuple[str, ...]
    capabilities: dict[str, bool]


def object_details(packet: InspectionPacket, *, object_id: str) -> ObjectDetails:
    if packet.closed or packet.authoring is None or packet.evaluated_frame is None:
        raise _unavailable("Object details require an open analysis or scene packet")
    row = next((item for item in packet.objects if item["id"] == object_id), None)
    if row is None:
        raise ValueError("Unknown object ID")
    key = {"mesh": "meshes", "part": "parts", "transform": "transforms",
           "binding": "bindings", "parameter": "parameters", "asset": "assets",
           "scene_binding": "scene_bindings", "offscreen": "offscreens",
           "glue": "glues", "blend_binding": "blend_bindings",
           "blend_key_table": "blend_key_tables",
           "blend_constraint": "blend_constraints"}[row["kind"]]
    authored = next(item for item in packet.authoring[key] if item["id"] == object_id)
    evaluated = None
    provenance = {"status": "not_applicable", "binding_ids": (),
                  "selection_status": "not_computed"}
    status = "not_applicable"
    diagnostics = []
    if row["kind"] == "mesh":
        evaluated = next((item for item in packet.evaluated_frame["drawables"]
                          if item["id"] == object_id), None)
        bindings = tuple(item["id"] for item in packet.authoring.get("bindings", ())
                         if item["mesh_id"] == object_id)
        blend_bindings = tuple(item["id"] for item in
                               packet.authoring.get("blend_bindings", ())
                               if item.get("target_id") == object_id and
                               item.get("target_kind") == "mesh")
        glues = tuple(item["id"] for item in packet.authoring.get("glues", ())
                      if object_id in (item["mesh_a_id"], item["mesh_b_id"]))
        provenance = {"status": "source_records_only", "binding_ids": bindings,
                      "blend_binding_ids": blend_bindings, "glue_ids": glues,
                      "selection_status": "not_computed"}
        status = _edit_mapping_status(packet, object_id, row.get("deformer_parent"))
        if evaluated is None:
            diagnostics.append("not_evaluated")
        elif not evaluated["enabled"]:
            diagnostics.append("disabled")
        elif not evaluated["visible"]:
            diagnostics.append("hidden")
    return ObjectDetails(object_id, row["kind"], authored, evaluated,
                         row.get("topology_hash"), provenance, status,
                         tuple(diagnostics),
                         {"evaluated_geometry": evaluated is not None,
                          "editable_point": status == "identity_canvas"})


def _xy(point: dict) -> tuple[float, float]:
    return point["x"], point["y"]


def _cross(a: tuple[float, float], b: tuple[float, float],
           c: tuple[float, float]) -> float:
    return (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])


def _barycentric(point: tuple[float, float], triangle: tuple[
    tuple[float, float], tuple[float, float], tuple[float, float],
]) -> tuple[float, float, float] | None:
    a, b, c = triangle
    denominator = _cross(a, b, c)
    scale = max(1.0, *(math.dist(triangle[i], triangle[(i + 1) % 3])
                       for i in range(3)))
    if abs(denominator) <= 1e-12 * scale * scale:
        return None
    u = _cross(point, b, c) / denominator
    v = _cross(point, c, a) / denominator
    w = 1.0 - u - v
    if min(u, v, w) < -1e-9 or max(u, v, w) > 1 + 1e-9:
        return None
    return (u, v, w)


def _clip_polygon(points: list[tuple[float, float]], axis: int,
                  bound: float, keep_greater: bool) -> list[tuple[float, float]]:
    if not points:
        return []
    result = []
    previous = points[-1]
    previous_inside = previous[axis] >= bound if keep_greater else previous[axis] <= bound
    for current in points:
        current_inside = current[axis] >= bound if keep_greater else current[axis] <= bound
        if current_inside != previous_inside:
            delta = current[axis] - previous[axis]
            if delta:
                fraction = (bound - previous[axis]) / delta
                result.append((previous[0] + fraction * (current[0] - previous[0]),
                               previous[1] + fraction * (current[1] - previous[1])))
        if current_inside:
            result.append(current)
        previous, previous_inside = current, current_inside
    return result


def _triangle_region_area(triangle: tuple[tuple[float, float], ...],
                          region: tuple[int, int, int, int]) -> float:
    polygon = list(triangle)
    for axis, bound, greater in ((0, region[0], True), (0, region[2], False),
                                 (1, region[1], True), (1, region[3], False)):
        polygon = _clip_polygon(polygon, axis, bound, greater)
        if len(polygon) < 3:
            return 0.0
    area = abs(sum(_cross((0, 0), polygon[index], polygon[(index + 1) % len(polygon)])
                   for index in range(len(polygon)))) * 0.5
    return area if area > 1e-12 else 0.0


def _probe(packet: InspectionPacket, view, pixel: tuple[int, int]) -> PixelProbe:
    def sample(source):
        if source is None or source.rgba is None:
            return None
        if source.width != view.width or source.height != view.height or (
                source.requested_roi != view.requested_roi or
                source.view_scale != view.view_scale or
                source.view_offset != view.view_offset):
            return None
        offset = (pixel[1] * source.width + pixel[0]) * 4
        return tuple(source.rgba[offset:offset + 4])
    raw = next((item for item in packet.views if item.kind == "raw_context"), None)
    clean = next((item for item in packet.views if item.kind == "clean"), None)
    raw_pixel, clean_pixel = sample(raw), sample(clean)
    return PixelProbe(pixel, raw_pixel,
                      "captured" if raw_pixel is not None else "not_captured",
                      clean_pixel,
                      "captured" if clean_pixel is not None else "not_captured",
                      raw.view_id if raw_pixel is not None else None,
                      clean.view_id if clean_pixel is not None else None)


def _edit_mapping_status(packet: InspectionPacket, mesh_id: str,
                         deformer_parent: str | None) -> str:
    if any(mesh_id in (item["mesh_a_id"], item["mesh_b_id"])
           for item in packet.authoring.get("glues", ())):
        return "nonlinear_glue"
    if deformer_parent:
        return "requires_inverse_deformer"
    if any(item["mesh_id"] == mesh_id for item in
           packet.authoring.get("bindings", ())):
        return "requires_target_selection"
    if any(item.get("target_id") == mesh_id and item.get("target_kind") == "mesh"
           for item in packet.authoring.get("blend_bindings", ())):
        return "requires_target_selection"
    return "identity_canvas"


def geometry_query(packet: InspectionPacket, *, view_id: str,
                   point: tuple[float, float] | None = None,
                   region: tuple[int, int, int, int] | None = None,
                   max_hits: int = 256) -> QueryResult:
    if packet.closed:
        raise _unavailable("Closed packet cannot be queried")
    if packet.evaluated_frame is None or packet.authoring is None:
        raise _unavailable("Geometry query requires an analysis or scene packet")
    if (point is None) == (region is None):
        raise ValueError("Exactly one of point or region is required")
    if type(max_hits) is not int or not 1 <= max_hits <= 256:
        raise ValueError("max_hits must be in 1..256")
    view = next((item for item in packet.views if item.view_id == view_id), None)
    if view is None:
        raise ValueError("Unknown packet view ID")
    if point is not None:
        if len(point) != 2 or not all(math.isfinite(value) for value in point):
            raise ValueError("Point must have two finite image coordinates")
        outside = not (0 <= point[0] < view.width and 0 <= point[1] < view.height)
    else:
        if (len(region) != 4 or any(type(value) is not int for value in region) or
                region[2] <= region[0] or region[3] <= region[1]):
            raise ValueError("Region needs half-open integer image bounds")
        outside = (region[0] < 0 or region[1] < 0 or region[2] > view.width or
                   region[3] > view.height)
    if outside:
        return QueryResult(view_id, "geometry", point, region, None, "outside", (), 0,
                           False, None)

    authored = {item["id"]: item for item in packet.authoring["meshes"]}
    objects = {item["id"]: item for item in packet.objects if item["kind"] == "mesh"}
    hits = []
    for mesh in packet.evaluated_frame["drawables"]:
        if mesh["id"] not in authored:
            continue
        source = authored[mesh["id"]]
        row = objects.get(mesh["id"], {})
        positions = [view.canvas_to_image(view.runtime_to_canvas(_xy(value)))
                     for value in mesh["positions"]]
        vertex_ids = source["vertex_ids"]
        uvs = [_xy(value) for value in mesh["uvs"]]
        indices = mesh["indices"]
        if len(indices) % 3 or len(positions) != len(vertex_ids) or len(uvs) != len(positions):
            raise _failure("INVALID_CAPTURE", "Evaluated mesh topology is inconsistent")
        for triangle_index in range(0, len(indices), 3):
            ia, ib, ic = indices[triangle_index:triangle_index + 3]
            if min(ia, ib, ic) < 0 or max(ia, ib, ic) >= len(positions):
                raise _failure("INVALID_CAPTURE", "Triangle index exceeds evaluated vertices")
            triangle = (positions[ia], positions[ib], positions[ic])
            if point is not None:
                if not (min(p[0] for p in triangle) - 1e-9 <= point[0] <=
                        max(p[0] for p in triangle) + 1e-9 and
                        min(p[1] for p in triangle) - 1e-9 <= point[1] <=
                        max(p[1] for p in triangle) + 1e-9):
                    continue
                barycentric = _barycentric(point, triangle)
                if barycentric is None:
                    continue
                image_point = point
                area = None
                uv = tuple(sum(barycentric[n] * uvs[index][axis]
                               for n, index in enumerate((ia, ib, ic)))
                           for axis in (0, 1))
            else:
                if (max(p[0] for p in triangle) <= region[0] or
                        min(p[0] for p in triangle) >= region[2] or
                        max(p[1] for p in triangle) <= region[1] or
                        min(p[1] for p in triangle) >= region[3]):
                    continue
                area = _triangle_region_area(triangle, region)
                if area == 0:
                    continue
                barycentric = None
                image_point = None
                uv = None
            canvas = view.image_to_canvas(image_point) if image_point is not None else None
            runtime = view.canvas_to_runtime(canvas) if canvas is not None else None
            deform_parent = row.get("deformer_parent")
            mapping_status = _edit_mapping_status(packet, mesh["id"], deform_parent)
            hits.append(QueryHit(
                mesh["id"], row.get("mark"),
                f"{row.get('topology_hash', 'unknown')}:{triangle_index // 3}",
                (vertex_ids[ia], vertex_ids[ib], vertex_ids[ic]),
                barycentric, image_point, canvas, runtime, uv,
                "not_requested", None, tuple(row.get("composition_path") or ()),
                row.get("render_order"), tuple(mesh["masks"]),
                row.get("geometry_space", "runtime_with_canvas_mapping"),
                deform_parent, "point_inside_triangle" if point is not None else
                "triangle_intersects_region", mapping_status,
                canvas if mapping_status == "identity_canvas" else None,
                packet.evaluation_revision, area,
                binding_provenance_ref=mesh["id"],
            ))
    total = len(hits)
    pixel = (int(math.floor(point[0])), int(math.floor(point[1]))) if point else None
    return QueryResult(view_id, "geometry", point, region, None,
                       "hit" if total else "no_hit", tuple(hits[:max_hits]), total,
                       total > max_hits, _probe(packet, view, pixel) if pixel else None)


def _pixel_roi(view, region: tuple[int, int, int, int]) -> tuple[float, float, float, float]:
    x0, y0 = view.image_to_canvas((region[0], region[1]))
    x1, y1 = view.image_to_canvas((region[2], region[3]))
    return (x0, y0, x1, y1)


def _triangle_image(packet: InspectionPacket, view, hit: QueryHit) -> tuple[
    tuple[float, float], tuple[float, float], tuple[float, float],
]:
    mesh = next(item for item in packet.evaluated_frame["drawables"]
                if item["id"] == hit.object_id)
    index = int(hit.triangle_key.rsplit(":", 1)[1]) * 3
    return tuple(view.canvas_to_image(view.runtime_to_canvas(
        _xy(mesh["positions"][vertex_index])))
        for vertex_index in mesh["indices"][index:index + 3])


def coverage_query(observer, packet: InspectionPacket, *, view_id: str,
                   point: tuple[float, float] | None = None,
                   region: tuple[int, int, int, int] | None = None,
                   mode: str = "coverage", alpha_threshold: float = 1 / 255,
                   max_hits: int = 256) -> QueryResult:
    if packet.closed or packet._scene is None:
        raise _unavailable("GPU coverage requires an open scene-profile packet")
    if mode not in ("coverage", "frontmost_covered"):
        raise ValueError("Unknown query mode")
    view = next((item for item in packet.views if item.view_id == view_id), None)
    if view is None:
        raise ValueError("Unknown packet view ID")
    if point is not None:
        if len(point) != 2 or not all(math.isfinite(value) for value in point):
            raise ValueError("Point must have two finite image coordinates")
        if 0 <= point[0] < view.width and 0 <= point[1] < view.height:
            sample = (math.floor(point[0]) + 0.5, math.floor(point[1]) + 0.5)
            sample_region = (math.floor(point[0]), math.floor(point[1]),
                             math.floor(point[0]) + 1, math.floor(point[1]) + 1)
            base = geometry_query(packet, view_id=view_id, point=sample,
                                  max_hits=max_hits)
        else:
            base = geometry_query(packet, view_id=view_id, point=point,
                                  max_hits=max_hits)
            sample = None
            sample_region = None
    else:
        base = geometry_query(packet, view_id=view_id, region=region,
                              max_hits=max_hits)
        sample = None
        sample_region = region
    base = replace(base, mode=mode, requested_point=point,
                   sample_point=sample,
                   pick_rule=("frontmost_covered_not_color_contribution" if
                              mode == "frontmost_covered" else None))
    if base.status == "outside" or not base.hits:
        return base
    if region is not None and (region[2] - region[0]) * (region[3] - region[1]) > 262_144:
        raise _failure("OBSERVATION_BUDGET_EXCEEDED",
                       "Coverage region exceeds 262144 sampled pixels")

    candidate_count = len({hit.object_id for hit in base.hits})
    sampled_pixels = ((sample_region[2] - sample_region[0]) *
                      (sample_region[3] - sample_region[1]))
    if candidate_count * sampled_pixels > 8_000_000:
        raise _failure("OBSERVATION_BUDGET_EXCEEDED",
                       "Coverage candidate readbacks exceed 8 million pixels")

    meshes = {item["id"]: item for item in packet.evaluated_frame["drawables"]}
    offscreens = {item["id"]: item for item in packet.evaluated_frame.get("offscreens", ())}
    ordered = {}
    for index, command in enumerate(packet.evaluated_frame.get("render_plan", ())):
        if "DrawMesh" in command:
            ordered[command["DrawMesh"]["mesh_id"]] = index
    dimensions = (sample_region[2] - sample_region[0],
                  sample_region[3] - sample_region[1])
    roi = _pixel_roi(view, sample_region)
    evidence = {}
    render_count = 0
    for mesh_id in dict.fromkeys(hit.object_id for hit in base.hits):
        mesh = meshes[mesh_id]
        path = next(hit.composition_path for hit in base.hits if hit.object_id == mesh_id)
        ancestors = [offscreens[target] for target in path if target in offscreens]
        if not mesh["enabled"] or not mesh["visible"] or any(
                not layer["enabled"] for layer in ancestors):
            evidence[mesh_id] = ("disabled_or_hidden", None, None)
            continue
        if mesh["raw_blend_mode"] is not None or any(
                layer["blend_mode"] != 0 for layer in ancestors):
            evidence[mesh_id] = ("unsupported_composition", None, None)
            continue
        frame_raw, _, plan_json = packet._scene._native.render_isolated(
            observer._native, *dimensions, roi, 0, "transparent",
            (0, 0, 0), (0, 0, 0), 1, (0, 0), False, [mesh_id],
            False, False, False, True,
        )
        from ._observe import _frame_from_native
        frame = _frame_from_native(frame_raw)
        render_count += 1
        if json.loads(plan_json)["coverage_policy"] != "normal_alpha_with_original_gates":
            raise _failure("INVALID_COVERAGE_PASS", "Renderer did not normalize blend alpha")
        evidence[mesh_id] = ("complete", frame.rgba, None)

    hits = []
    unsupported = False
    covered = False
    for hit in base.hits:
        status, rgba, _ = evidence[hit.object_id]
        if status == "unsupported_composition":
            unsupported = True
            hits.append(replace(hit, coverage_status=status))
            continue
        if rgba is None:
            hits.append(replace(hit, coverage_status=status, coverage_value=0.0,
                                coverage_pixel_count=0 if region is not None else None))
            continue
        if point is not None:
            value = rgba[3] / 255
            covered |= value >= alpha_threshold
            hits.append(replace(hit, coverage_status="complete", coverage_value=value,
                                hit_reason="sampled_pixel_center"))
        else:
            triangle = _triangle_image(packet, view, hit)
            xmin = max(sample_region[0], math.floor(min(p[0] for p in triangle)))
            xmax = min(sample_region[2], math.ceil(max(p[0] for p in triangle)))
            ymin = max(sample_region[1], math.floor(min(p[1] for p in triangle)))
            ymax = min(sample_region[3], math.ceil(max(p[1] for p in triangle)))
            count = 0
            maximum = 0
            bounds = None
            for y in range(ymin, ymax):
                for x in range(xmin, xmax):
                    if _barycentric((x + 0.5, y + 0.5), triangle) is None:
                        continue
                    alpha = rgba[((y - sample_region[1]) * dimensions[0] +
                                  x - sample_region[0]) * 4 + 3]
                    maximum = max(maximum, alpha)
                    if alpha / 255 >= alpha_threshold:
                        count += 1
                        bounds = ((x, y, x + 1, y + 1) if bounds is None else
                                  (min(bounds[0], x), min(bounds[1], y),
                                   max(bounds[2], x + 1), max(bounds[3], y + 1)))
            covered |= count > 0
            hits.append(replace(hit, coverage_status="complete", coverage_value=maximum / 255,
                                coverage_pixel_count=count, coverage_bounds=bounds,
                                hit_reason="triangle_sampled_coverage"))

    frontmost = None
    if mode == "frontmost_covered" and not unsupported and not base.truncated:
        candidates = [hit for hit in hits if hit.coverage_status == "complete" and
                      ((hit.coverage_value or 0) >= alpha_threshold if point is not None
                       else (hit.coverage_pixel_count or 0) > 0)]
        if candidates:
            frontmost = max(candidates, key=lambda hit: ordered.get(hit.object_id, -1)).object_id
    status = ("truncated" if base.truncated else "unsupported_composition" if unsupported
              else "hit" if covered else "no_hit")
    return replace(base, status=status, hits=tuple(hits),
                   frontmost_object_id=frontmost,
                   resources={"render_count": render_count,
                              "readback_count": render_count,
                              "readback_bytes": render_count * sampled_pixels * 4})
