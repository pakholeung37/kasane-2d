"""O4 geometry overlays and camera-independent triangle deformation diagnostics."""
from __future__ import annotations

from dataclasses import replace
import math
from uuid import uuid4

from ._inspection import InspectionPacket, InspectionRequest, InspectionView, _DIGITS, _failure, _json, _sha
from ._observe import _encode_rgba_png

MAX_SEGMENTS = 20_000


class _Raster:
    def __init__(self, view: InspectionView) -> None:
        if view.rgba is None:
            raise _failure("CAPTURE_NOT_AVAILABLE", "Geometry overlays require captured RGBA pixels")
        self.view = view
        self.pixels = bytearray(view.rgba)
        self.segments = 0
        self.omitted = 0

    def dot(self, x: int, y: int, rgb: tuple[int, int, int]) -> None:
        if 0 <= x < self.view.width and 0 <= y < self.view.height:
            offset = (y * self.view.width + x) * 4
            self.pixels[offset:offset + 4] = bytes((*rgb, 255))

    def line(self, a: tuple[float, float], b: tuple[float, float],
             rgb: tuple[int, int, int]) -> None:
        if not all(math.isfinite(value) for value in (*a, *b)):
            self.omitted += 1
            return
        if self.segments >= MAX_SEGMENTS:
            self.omitted += 1
            return
        self.segments += 1
        x0, y0 = (round(value) for value in a)
        x1, y1 = (round(value) for value in b)
        # Clip in image space before rasterizing; source geometry can extend far beyond the ROI.
        dx, dy = x1 - x0, y1 - y0
        limits = ((-dx, x0), (dx, self.view.width - 1 - x0),
                  (-dy, y0), (dy, self.view.height - 1 - y0))
        lo, hi = 0.0, 1.0
        for p, q in limits:
            if p == 0:
                if q < 0:
                    return
            elif p < 0:
                lo = max(lo, q / p)
            else:
                hi = min(hi, q / p)
        if lo > hi:
            return
        ax, ay = round(x0 + dx * lo), round(y0 + dy * lo)
        bx, by = round(x0 + dx * hi), round(y0 + dy * hi)
        dx, dy = abs(bx - ax), -abs(by - ay)
        sx, sy = (1 if ax < bx else -1), (1 if ay < by else -1)
        error = dx + dy
        while True:
            self.dot(ax, ay, rgb)
            if (ax, ay) == (bx, by):
                break
            doubled = error * 2
            if doubled >= dy:
                error += dy
                ax += sx
            if doubled <= dx:
                error += dx
                ay += sy

    def label(self, x: float, y: float, number: int) -> bool:
        if not math.isfinite(x) or not math.isfinite(y):
            return False
        x, y = round(x) + 3, round(y) - 4
        label = str(number)
        if x < 0 or y < 0 or x + len(label) * 4 > self.view.width or y + 5 > self.view.height:
            return False
        for index, char in enumerate(label):
            for gy, bits in enumerate(_DIGITS[int(char)]):
                for gx, bit in enumerate(bits):
                    if bit == "1":
                        self.dot(x + index * 4 + gx, y + gy, (255, 240, 32))
        return True

    def finish(self, kind: str, record: dict) -> InspectionView:
        png = _encode_rgba_png(self.view.width, self.view.height, self.pixels)
        record = {**(self.view.presentation or {}), **record,
                  "segments_drawn": self.segments,
                  "segments_omitted_limit": self.omitted,
                  "segment_limit": MAX_SEGMENTS,
                  "source": "same_evaluation_trace_v1"}
        return replace(self.view, view_id=uuid4().hex, kind=kind,
                       png=png, rgba=bytes(self.pixels), frame=None,
                       artifact_sha256=_sha(png), presentation=record,
                       render_digest=_sha(_json({"base": self.view.render_digest,
                                                 "overlay": record})))


def _image_point(view: InspectionView, runtime: dict) -> tuple[float, float]:
    return view.canvas_to_image(view.runtime_to_canvas((runtime["x"], runtime["y"])))


def _mesh_rows(packet: InspectionPacket, request: InspectionRequest) -> list[dict]:
    trace = packet.evaluation_trace
    if trace is None:
        raise _failure("CAPTURE_NOT_AVAILABLE", "Capture this scene with with_trace=True")
    selected = {row["id"] for row in packet.objects if row.get("kind") == "mesh"}
    if request.focus.mesh_ids or request.focus.part_ids:
        selected = {row["id"] for row in packet.objects
                    if row.get("kind") == "mesh" and (
                        row["id"] in request.focus.mesh_ids or
                        set(row.get("part_path", ())) & set(request.focus.part_ids))}
    return [row for row in trace["meshes"] if row["id"] in selected and
            row["enabled"] and row["visible"]]


def add_geometry_views(packet: InspectionPacket, request: InspectionRequest) -> InspectionPacket:
    """Draw trace-only overlays; clean render bytes remain untouched."""
    trace = packet.evaluation_trace
    if trace is None:
        raise _failure("CAPTURE_NOT_AVAILABLE", "Geometry views require a scene captured with trace")
    clean = packet.views[0]
    rows = _mesh_rows(packet, request)
    views = list(packet.views)
    if "wireframe" in request.channels:
        raster = _Raster(clean)
        for mesh in rows:
            points = dict(zip(mesh["vertex_ids"], mesh["positions"]))
            for triangle in mesh["triangles"]:
                for a, b in ((0, 1), (1, 2), (2, 0)):
                    raster.line(_image_point(clean, points[triangle[a]]),
                                _image_point(clean, points[triangle[b]]), (0, 238, 238))
        views.append(raster.finish("wireframe", {"mesh_count": len(rows)}))
    if "vertices" in request.channels:
        raster = _Raster(clean)
        available = [(mesh["id"], vertex_id, point) for mesh in rows
                     for vertex_id, point in zip(mesh["vertex_ids"], mesh["positions"])]
        requested = set(request.overlay.vertex_ids)
        if requested:
            chosen = [row for row in available if row[1] in requested]
        else:
            budget = min(request.limits.max_vertex_labels, 64)
            stride = max(1, math.ceil(len(available) / max(1, budget)))
            chosen = available[::stride]
        chosen = chosen[:request.limits.max_vertex_labels]
        placed, omitted = [], []
        for mesh_id, vertex_id, point in chosen:
            x, y = _image_point(clean, point)
            if raster.label(x, y, vertex_id):
                placed.append({"mesh_id": mesh_id, "vertex_id": vertex_id})
            else:
                omitted.append({"mesh_id": mesh_id, "vertex_id": vertex_id,
                                "reason": "outside_view_or_label_space"})
        if requested:
            for vertex_id in sorted(requested - {row[1] for row in available}):
                omitted.append({"vertex_id": vertex_id, "reason": "vertex_id_missing"})
        if len(available) > len(chosen):
            omitted.append({"reason": "density_budget", "count": len(available) - len(chosen)})
        views.append(raster.finish("vertices", {"labels": placed,
                                                 "omitted_labels": omitted}))
    if "deformers" in request.channels:
        raster = _Raster(clean)
        for transform in trace["transforms"]:
            if not transform["enabled"]:
                continue
            points = transform["control_points"]
            rows_count, cols_count = transform["rows"], transform["columns"]
            if rows_count is not None and cols_count is not None:
                if len(points) != (rows_count + 1) * (cols_count + 1):
                    raise _failure("TRACE_TOPOLOGY_MISMATCH", transform["id"])
                for row in range(rows_count + 1):
                    for col in range(cols_count + 1):
                        index = row * (cols_count + 1) + col
                        point = _image_point(clean, points[index])
                        if col < cols_count:
                            raster.line(point, _image_point(clean, points[index + 1]),
                                        (255, 110, 32))
                        if row < rows_count:
                            raster.line(point, _image_point(clean, points[index + cols_count + 1]),
                                        (255, 110, 32))
                        if index < request.limits.max_vertex_labels:
                            raster.label(*point, index)
            axis = transform["rotation_axis_samples"]
            for a, b in zip(axis, axis[1:]):
                raster.line(_image_point(clean, a), _image_point(clean, b), (255, 80, 185))
            if axis:
                center = _image_point(clean, axis[len(axis) // 2])
                raster.line((center[0] - 3, center[1]), (center[0] + 3, center[1]), (255, 80, 185))
                raster.line((center[0], center[1] - 3), (center[0], center[1] + 3), (255, 80, 185))
        views.append(raster.finish("deformers", {
            "transform_count": len(trace["transforms"]),
            "control_point_identity": "deformer_topology_local_index",
        }))
    return replace(packet, views=tuple(views))


def _canvas_positions(packet: InspectionPacket, mesh: dict) -> dict[int, tuple[float, float]]:
    canvas = packet.evaluated_frame["canvas"]
    origin, ppu = canvas["origin"], canvas["pixels_per_unit"]
    return {vertex_id: (origin["x"] + point["x"] * ppu,
                        origin["y"] - point["y"] * ppu)
            for vertex_id, point in zip(mesh["vertex_ids"], mesh["positions"])}


def _det(a: tuple[float, float], b: tuple[float, float]) -> float:
    return a[0] * b[1] - a[1] * b[0]


def _triangle_metrics(base: list[tuple[float, float]],
                      current: list[tuple[float, float]],
                      minimum: float, maximum: float) -> dict:
    b0 = (base[1][0] - base[0][0], base[1][1] - base[0][1])
    b1 = (base[2][0] - base[0][0], base[2][1] - base[0][1])
    c0 = (current[1][0] - current[0][0], current[1][1] - current[0][1])
    c1 = (current[2][0] - current[0][0], current[2][1] - current[0][1])
    edges = (b0, b1, (b1[0] - b0[0], b1[1] - b0[1]))
    scale_sq = max(sum(value * value for value in edge) for edge in edges)
    epsilon = max(1e-12, 1e-8 * scale_sq)  # canvas pixel squared, twice-area units
    db, dc = _det(b0, b1), _det(c0, c1)
    if abs(db) <= epsilon:
        return {"status": "baseline_degenerate", "baseline_twice_area_px2": db,
                "current_twice_area_px2": dc, "epsilon_twice_area_px2": epsilon,
                "determinant": None, "singular_values": None,
                "flags": ["baseline_degenerate"]}
    f00 = (c0[0] * b1[1] - c1[0] * b0[1]) / db
    f01 = (-c0[0] * b1[0] + c1[0] * b0[0]) / db
    f10 = (c0[1] * b1[1] - c1[1] * b0[1]) / db
    f11 = (-c0[1] * b1[0] + c1[1] * b0[0]) / db
    determinant = f00 * f11 - f01 * f10
    fro2 = f00 * f00 + f01 * f01 + f10 * f10 + f11 * f11
    discriminant = max(0.0, fro2 * fro2 - 4 * determinant * determinant)
    smax = math.sqrt(max(0.0, (fro2 + math.sqrt(discriminant)) / 2))
    smin = abs(determinant) / smax if smax else 0.0
    flags = []
    if abs(dc) <= epsilon:
        flags.append("current_degenerate")
    if determinant < 0:
        flags.append("orientation_reversal")
    if smin < minimum:
        flags.append("compression")
    if smax > maximum:
        flags.append("stretch")
    if not all(math.isfinite(value) for value in (determinant, smin, smax)):
        raise _failure("NONFINITE_DEFORMATION", "Triangle deformation exceeded finite range")
    return {"status": "comparable", "baseline_twice_area_px2": db,
            "current_twice_area_px2": dc, "epsilon_twice_area_px2": epsilon,
            "determinant": determinant, "singular_values": [smin, smax],
            "flags": flags}


def _reflection_parity(packet: InspectionPacket, mesh_id: str) -> bool:
    mesh = next((item for item in packet.authoring["meshes"] if item["id"] == mesh_id), None)
    if mesh is None:
        return False
    transform_id = mesh.get("deformer_id")
    transforms = {item["id"]: item for item in packet.evaluation_trace["transforms"]}
    transform = transforms.get(transform_id)
    if transform is None:
        return False
    return sum(int(transforms[id]["reflection_parity"]) for id in
               (transform_id, *transform["parent_chain"]) if id in transforms) % 2 == 1


def diagnose_deformation(current: InspectionPacket, baseline: InspectionPacket,
                         request: InspectionRequest) -> dict:
    """Compare ordered authoring triangle IDs in canvas pixels, independent of view scale."""
    thresholds = {"min_stretch": request.diagnostics.min_stretch,
                  "max_stretch": request.diagnostics.max_stretch,
                  "epsilon_policy": "max(1e-12 px2, 1e-8 * baseline_max_edge_squared_px2)"}
    result = {"status": "comparable", "thresholds": thresholds,
              "coordinate_space": "source_canvas_pixels_y_down",
              "displacement_semantics": "full_evaluation_including_blendshape_and_glue",
              "meshes": [], "summary": {"triangles": 0, "abnormal": 0,
                                      "topology_mismatch": 0, "baseline_degenerate": 0,
                                      "current_degenerate": 0}}
    if current.document_id != baseline.document_id:
        result["status"] = "TOPOLOGY_MISMATCH"
        result["reason"] = "cross_project_without_object_mapping"
        return result
    if current.evaluation_trace is None or baseline.evaluation_trace is None:
        result["status"] = "not_comparable"
        result["reason"] = "evaluation_trace_missing"
        return result
    old = {mesh["id"]: mesh for mesh in baseline.evaluation_trace["meshes"]}
    new = {mesh["id"]: mesh for mesh in current.evaluation_trace["meshes"]}
    focus = set(request.focus.mesh_ids)
    if request.focus.part_ids:
        focus.update(row["id"] for row in current.objects if row.get("kind") == "mesh" and
                     set(row.get("part_path", ())) & set(request.focus.part_ids))
    ids = sorted(focus or (set(old) | set(new)))
    for mesh_id in ids:
        a, b = old.get(mesh_id), new.get(mesh_id)
        if (a is None or b is None or a["topology_hash"] != b["topology_hash"] or
                len(a["vertex_ids"]) != len(a["positions"]) or
                len(b["vertex_ids"]) != len(b["positions"]) or
                len(set(a["vertex_ids"])) != len(a["vertex_ids"])):
            result["meshes"].append({"mesh_id": mesh_id, "status": "TOPOLOGY_MISMATCH",
                                     "reason": "missing_or_changed_vertex_identity_or_triangles"})
            result["summary"]["topology_mismatch"] += 1
            continue
        if not a["enabled"] or not b["enabled"]:
            result["meshes"].append({"mesh_id": mesh_id, "status": "not_comparable",
                                     "reason": "inactive_geometry"})
            continue
        pa, pb = _canvas_positions(baseline, a), _canvas_positions(current, b)
        parity_changed = _reflection_parity(baseline, mesh_id) != _reflection_parity(current, mesh_id)
        triangles = []
        for triangle in b["triangles"]:
            if any(vertex_id not in pa or vertex_id not in pb for vertex_id in triangle):
                result["summary"]["topology_mismatch"] += 1
                triangles.append({"vertex_ids": triangle, "status": "TOPOLOGY_MISMATCH"})
                continue
            metrics = _triangle_metrics([pa[id] for id in triangle],
                                        [pb[id] for id in triangle],
                                        request.diagnostics.min_stretch,
                                        request.diagnostics.max_stretch)
            if "orientation_reversal" in metrics["flags"] and parity_changed:
                metrics["flags"].append("confirmed_reflection_source")
            metrics.update(vertex_ids=triangle, topology_hash=b["topology_hash"],
                           triangle_key=[*triangle, b["topology_hash"]])
            triangles.append(metrics)
            result["summary"]["triangles"] += 1
            result["summary"]["abnormal"] += bool(metrics["flags"])
            for flag in ("baseline_degenerate", "current_degenerate"):
                result["summary"][flag] += flag in metrics["flags"]
        distances = [math.dist(pa[id], pb[id]) for id in b["vertex_ids"]]
        result["meshes"].append({"mesh_id": mesh_id, "status": "comparable",
                                 "topology_hash": b["topology_hash"],
                                 "vertex_displacement_px": {"maximum": max(distances, default=0.0),
                                                             "mean": sum(distances) / len(distances) if distances else 0.0},
                                 "triangles": triangles})
    if result["summary"]["topology_mismatch"]:
        result["status"] = "TOPOLOGY_MISMATCH"
    return result


def add_baseline_diagnostics(current: InspectionPacket, baseline: InspectionPacket,
                             request: InspectionRequest) -> InspectionPacket:
    diagnosis = diagnose_deformation(current, baseline, request)
    clean = current.views[0]
    views = list(current.views)
    old = {mesh["id"]: mesh for mesh in (baseline.evaluation_trace or {}).get("meshes", ())}
    new = {mesh["id"]: mesh for mesh in (current.evaluation_trace or {}).get("meshes", ())}
    if "displacement" in request.channels:
        raster = _Raster(clean)
        arrows = 0
        for row in diagnosis["meshes"]:
            if row["status"] != "comparable":
                continue
            mesh_id = row["mesh_id"]
            base = _canvas_positions(baseline, old[mesh_id])
            now = _canvas_positions(current, new[mesh_id])
            for vertex_id in new[mesh_id]["vertex_ids"]:
                start, end = clean.canvas_to_image(base[vertex_id]), clean.canvas_to_image(now[vertex_id])
                if math.dist(start, end) < 0.5:
                    continue
                raster.line(start, end, (0, 255, 120))
                angle = math.atan2(end[1] - start[1], end[0] - start[0])
                for delta in (-0.55, 0.55):
                    tip = (end[0] - 5 * math.cos(angle + delta),
                           end[1] - 5 * math.sin(angle + delta))
                    raster.line(end, tip, (0, 255, 120))
                arrows += 1
        views.append(raster.finish("displacement", {"arrows": arrows,
            "semantics": "baseline_to_current_full_evaluation_not_keyform_delta"}))
    if "distortion" in request.channels:
        raster = _Raster(clean)
        marked = 0
        for row in diagnosis["meshes"]:
            if row["status"] != "comparable":
                continue
            points = dict(zip(new[row["mesh_id"]]["vertex_ids"], new[row["mesh_id"]]["positions"]))
            for triangle in row["triangles"]:
                flags = triangle.get("flags", ())
                if not flags:
                    continue
                color = (255, 35, 35) if "orientation_reversal" in flags else (
                    (255, 150, 0) if "stretch" in flags else (170, 80, 255))
                ids = triangle["vertex_ids"]
                for a, b in ((0, 1), (1, 2), (2, 0)):
                    raster.line(_image_point(clean, points[ids[a]]),
                                _image_point(clean, points[ids[b]]), color)
                marked += 1
        views.append(raster.finish("distortion", {"marked_triangles": marked,
                                                   "thresholds": diagnosis["thresholds"]}))
    return replace(current, views=tuple(views), deformation=diagnosis)
