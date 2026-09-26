"""Paged, recoverable Observe v2 reports from one frozen sample batch."""
from __future__ import annotations

from dataclasses import dataclass, replace
import hashlib
from importlib.metadata import version as package_version
import math
from pathlib import Path
import platform
from time import perf_counter
from typing import TYPE_CHECKING
from uuid import uuid4

from ._comparison import CompareOptions, compare_observations, _image_library
from ._inspection import (
    MAX_ARTIFACT_PIXELS, MAX_PAGE_PIXELS, InspectionPacket, InspectionRequest,
    _failure, _focus_and_objects, _json, _parse_json, _read_member, _sha, _write,
    open_inspection_packet,
)

if TYPE_CHECKING:
    from ._observe import Observer
    from ._session import Session


@dataclass(frozen=True)
class SequenceLayout:
    columns: int = 3
    label_height: int = 28
    gutter: int = 8

    def __post_init__(self) -> None:
        if (type(self.columns) is not int or not 1 <= self.columns <= 16 or
                type(self.label_height) is not int or not 16 <= self.label_height <= 96 or
                type(self.gutter) is not int or not 0 <= self.gutter <= 64):
            raise ValueError("Invalid contact-sheet sequence layout")


@dataclass(frozen=True)
class GridLayout:
    x_parameter_id: str
    y_parameter_id: str
    x_values: tuple[float, ...]
    y_values: tuple[float, ...]
    label_height: int = 28
    gutter: int = 8

    def __post_init__(self) -> None:
        if (not self.x_parameter_id or not self.y_parameter_id or
                self.x_parameter_id == self.y_parameter_id or
                not self.x_values or not self.y_values or
                len(self.x_values) > 16 or len(self.y_values) > 64 or
                any(not math.isfinite(v) for v in (*self.x_values, *self.y_values)) or
                len(set(self.x_values)) != len(self.x_values) or
                len(set(self.y_values)) != len(self.y_values) or
                type(self.label_height) is not int or not 16 <= self.label_height <= 96 or
                type(self.gutter) is not int or not 0 <= self.gutter <= 64):
            raise ValueError("Invalid explicit two-parameter grid layout")


@dataclass(frozen=True)
class InspectionRun:
    run_directory: Path
    report_path: Path
    status: str
    report: dict

    @property
    def pages(self) -> tuple[dict, ...]:
        return tuple(self.report.get("pages", ()))

    def packet(self, sample_index: int) -> InspectionPacket:
        if not 0 <= sample_index < len(self.report.get("samples", ())):
            raise IndexError("Sample index is outside the report")
        entry = self.report["samples"][sample_index]
        if entry.get("status") != "complete":
            raise _failure("CAPTURE_NOT_AVAILABLE", "This sample has no complete saved packet")
        return open_inspection_packet(self.run_directory / entry["packet_path"])


def _value_index(value: float, choices: tuple[float, ...]) -> int:
    found = [index for index, option in enumerate(choices)
             if math.isclose(value, option, rel_tol=0, abs_tol=1e-6)]
    if len(found) != 1:
        raise ValueError("GRID_VALUE_NOT_DECLARED: actual parameter value has no unique grid cell")
    return found[0]


def _sample_record(scene, index: int) -> dict:
    names = {item["id"]: item["name"] for item in scene.authoring.get("parameters", ())}
    actual = []
    for item in scene.evaluated_frame.get("parameters", ()):
        actual.append({
            "id": item["id"], "name": names.get(item["id"]),
            "requested": item["requested"], "actual": item["value"],
            "clamped": item["clamped"] or item["requested"] != item["value"],
        })
    return {"sample_index": index, "sample_id": uuid4().hex,
            "capture_id": scene.capture_id, "scene_digest": scene.scene_digest,
            "source": scene.source, "requested": scene.metadata["requested"],
            "actual": actual, "status": "pending"}


def _cells(samples: list[dict], layout: SequenceLayout | GridLayout) -> tuple[int, list[dict]]:
    if isinstance(layout, SequenceLayout):
        return layout.columns, [
            {"cell_index": index, "sample_index": index, "status": "complete"}
            for index in range(len(samples))
        ]
    if not isinstance(layout, GridLayout):
        raise TypeError("Layout must be SequenceLayout or GridLayout")
    x_id, y_id = layout.x_parameter_id, layout.y_parameter_id
    cells = [{"cell_index": index, "sample_index": None, "status": "missing",
              "x_value": layout.x_values[index % len(layout.x_values)],
              "y_value": layout.y_values[index // len(layout.x_values)]}
             for index in range(len(layout.x_values) * len(layout.y_values))]
    for sample in samples:
        values = {item["id"]: item["actual"] for item in sample["actual"]}
        if x_id not in values or y_id not in values:
            raise ValueError("GRID_PARAMETER_MISSING: grid axis is absent from the sample")
        x = _value_index(values[x_id], layout.x_values)
        y = _value_index(values[y_id], layout.y_values)
        cell = cells[y * len(layout.x_values) + x]
        if cell["sample_index"] is not None:
            raise ValueError("DUPLICATE_GRID_CELL: two samples have the same actual grid values")
        cell.update(sample_index=sample["sample_index"], status="complete")
    return len(layout.x_values), cells


def _ascii_label(sample: dict) -> tuple[str, bool]:
    values = sample["actual"]
    terms = []
    for item in values:
        name = item["name"] or item["id"][:8]
        label = f"{name}={item['actual']:.4g}"
        if item["clamped"]:
            label += " (clamped)"
        terms.append(label)
    text = f"#{sample['sample_index']} " + (" ".join(terms) if terms else "default")
    fallback = not text.isascii()
    return text.encode("ascii", "replace").decode("ascii"), fallback


def _page_geometry(cell_count: int, columns: int,
                   layout: SequenceLayout | GridLayout,
                   width: int, height: int) -> tuple[int, int, list[tuple[int, int]]]:
    cell_width = width + layout.gutter
    cell_height = height + layout.label_height + layout.gutter
    page_width = columns * cell_width + layout.gutter
    rows_per_page = min(64, (MAX_PAGE_PIXELS // page_width - layout.gutter) // cell_height)
    if rows_per_page < 1:
        raise _failure("OBSERVATION_BUDGET_EXCEEDED",
                       "One contact-sheet row exceeds the page pixel limit")
    page_capacity = columns * rows_per_page
    shapes = []
    for begin in range(0, cell_count, page_capacity):
        count = min(page_capacity, cell_count - begin)
        shapes.append((page_width, math.ceil(count / columns) * cell_height + layout.gutter))
    return page_width, rows_per_page, shapes


def _save_pages(root: Path, report: dict, columns: int, cells: list[dict],
                layout: SequenceLayout | GridLayout, budget: int) -> list[dict]:
    Image = _image_library()
    from PIL import ImageDraw, ImageFont

    font = ImageFont.load_default()
    report["layout"]["font"] = {
        "provider": "Pillow bundled default",
        "pillow_version": package_version("Pillow"),
        "font_class": type(font).__name__,
        "unicode_fallback": "ascii_question_mark",
    }
    width, height = report["view_size"]
    cell_width = width + layout.gutter
    cell_height = height + layout.label_height + layout.gutter
    page_width, rows_per_page, shapes = _page_geometry(
        len(cells), columns, layout, width, height)
    page_capacity = columns * rows_per_page
    pages = []
    for page_index, (_, page_height) in enumerate(shapes):
        begin = page_index * page_capacity
        page_cells = cells[begin:begin + page_capacity]
        pixels = page_width * page_height
        if pixels > budget:
            raise _failure("OBSERVATION_BUDGET_EXCEEDED", "Contact sheets exceed the artifact pixel budget")
        budget -= pixels
        sheet = Image.new("RGBA", (page_width, page_height), (24, 24, 24, 255))
        draw = ImageDraw.Draw(sheet)
        transforms = []
        for local, cell in enumerate(page_cells):
            x = layout.gutter + (local % columns) * cell_width
            y = layout.gutter + (local // columns) * cell_height
            sample_index = cell["sample_index"]
            if sample_index is None:
                draw.rectangle((x, y + layout.label_height, x + width - 1,
                                y + layout.label_height + height - 1),
                               outline=(110, 110, 110, 255))
                label, fallback = "missing", False
            else:
                sample = report["samples"][sample_index]
                clean = next(view for view in sample["views"] if view["kind"] == "clean")
                from io import BytesIO
                with Image.open(BytesIO((root / clean["path"]).read_bytes())) as source:
                    source.load()
                    sheet.paste(source.convert("RGBA"), (x, y + layout.label_height))
                label, fallback = _ascii_label(sample)
                full_label = label
                while label and font.getlength(label) > width - 2:
                    label = label[:-1]
                transforms.append({"sample_index": sample_index,
                                   "view_id": clean["view_id"],
                                   "sheet_to_view": [1, 0, -x, 0, 1,
                                                     -(y + layout.label_height)],
                                   "view_rect": [x, y + layout.label_height,
                                                 x + width, y + layout.label_height + height],
                                   "label_fallback": fallback,
                                   "label_truncated": label != full_label,
                                   "display_label": label})
            draw.text((x, y), label, fill=(240, 240, 240, 255),
                      font=font)
        from ._observe import _encode_rgba_png
        png = _encode_rgba_png(page_width, page_height, sheet.tobytes())
        relative = f"sheets/{page_index:03d}.png"
        _write(root / relative, png)
        report["files"][relative] = _sha(png)
        pages.append({"page_index": page_index, "path": relative,
                      "sha256": _sha(png), "width": page_width,
                      "height": page_height, "cells": page_cells,
                      "view_transforms": transforms,
                      "label_region_is_scene": False})
    return pages


def _budget(request: InspectionRequest, count: int, comparisons: int,
            layout: SequenceLayout | GridLayout) -> None:
    width, height = request.view.resolution
    estimated_views = len(request.channels) + (request.mode != "context") + (
        3 if "mask" in request.channels else 0)
    view_pixels = count * estimated_views * width * height
    comparison_pixels = comparisons * 5 * width * height
    columns = len(layout.x_values) if isinstance(layout, GridLayout) else layout.columns
    cell_count = (len(layout.x_values) * len(layout.y_values)
                  if isinstance(layout, GridLayout) else count)
    _, _, shapes = _page_geometry(cell_count, columns, layout, width, height)
    expected = view_pixels + comparison_pixels + sum(w * h for w, h in shapes)
    if expected > min(request.limits.max_artifact_pixels, MAX_ARTIFACT_PIXELS):
        raise _failure("OBSERVATION_BUDGET_EXCEEDED",
                       f"Requested at least {expected} artifact pixels; limit is {request.limits.max_artifact_pixels}")
    retained = width * height * 4 * (estimated_views * 2 +
                                     (estimated_views if comparisons else 0))
    if retained > request.limits.max_cpu_retained_bytes:
        raise _failure("OBSERVATION_BUDGET_EXCEEDED",
                       f"Estimated {retained} CPU image bytes exceed {request.limits.max_cpu_retained_bytes}")


def _render_resources(packet: InspectionPacket) -> tuple[int, int, int]:
    """Count GPU passes and readbacks; mask passes also read the main target."""
    gpu_kinds = {"clean", "alpha", "isolated", "xray", "mask_source",
                 "mask_combined", "mask_coverage"}
    sources = [view for view in packet.views if view.kind in gpu_kinds]
    masks = sum(view.kind in ("mask_source", "mask_combined") for view in sources)
    clean = packet.views[0]
    return (len(sources), len(sources) + masks,
            sum(view.width * view.height * 4 for view in sources) +
            masks * clean.width * clean.height * 4)


def _native_binary_sha256() -> str:
    from . import _native
    digest = hashlib.sha256()
    with open(_native.__file__, "rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def inspect_run(observer: Observer, session: Session, samples, *,
                request: InspectionRequest, output: Path,
                baseline_index: int | None = None,
                layout: SequenceLayout | GridLayout | None = None) -> InspectionRun:
    """Freeze samples once, render sequentially, and publish an atomic v2 report."""
    if not output.is_absolute():
        raise ValueError("Inspection run output must be absolute")
    if not 1 <= len(samples) <= request.limits.max_samples:
        raise ValueError("Inspection sample count exceeds 1..max_samples")
    if baseline_index is not None and (type(baseline_index) is not int or
                                       not 0 <= baseline_index < len(samples)):
        raise ValueError("baseline_index is outside samples")
    geometry_channels = ("wireframe", "vertices", "deformers", "displacement", "distortion")
    if baseline_index is None and any(channel in request.channels for channel in
                                      ("displacement", "distortion")):
        raise ValueError("Displacement and distortion channels require baseline_index")
    render_request = replace(request, channels=tuple(channel for channel in request.channels
        if channel not in ("displacement", "distortion")))
    if request.view.framing != "fixed_union":
        raise ValueError("Inspection report requires fixed_union framing")
    layout = layout or SequenceLayout()
    if not isinstance(layout, (SequenceLayout, GridLayout)):
        raise TypeError("Unknown contact-sheet layout")
    _budget(request, len(samples), len(samples) - 1 if baseline_index is not None else 0,
            layout)
    output.mkdir(parents=True, exist_ok=True)
    root = output / f"inspection-{uuid4().hex}"
    root.mkdir(exist_ok=False)
    (root / "packets").mkdir()
    start = perf_counter()
    report = {"schema_version": 2, "kind": "kasane-inspection-run",
              "status": "running", "run_id": root.name,
              "sdk": {"package": "kasane", "version": package_version("kasane"),
                      "python": platform.python_version(), "platform": platform.platform(),
                      "native_binary_sha256": _native_binary_sha256()},
              "capture": None, "samples": [], "objects": [], "views": [],
              "pages": [], "comparisons": [], "diagnostics": [], "files": {},
              "view_size": request.view.resolution,
              "layout": {"kind": "grid" if isinstance(layout, GridLayout) else "sequence",
                         "columns": len(layout.x_values) if isinstance(layout, GridLayout)
                         else layout.columns},
              "capabilities": {"comparison": baseline_index is not None,
                               "evaluation_trace": False,
                               "deformation": any(channel in request.channels for channel in ("displacement", "distortion")),
                               "geometry_query": False, "pixel_coverage_query": False,
                               "playback_replay": False},
              "resources": {"render_count": 0, "output_pixels": 0,
                            "readback_count": 0, "readback_bytes": 0,
                            "decoded_texture_estimate_bytes": None,
                            "peak_cpu_image_estimate_bytes": None,
                            "artifact_bytes": 0, "output_bytes": 0,
                            "elapsed_ms": None}}
    report_path = root / "report.json"

    def publish() -> None:
        _write(report_path, _json(report))

    def account_bytes() -> None:
        artifacts = sum((root / relative).stat().st_size for relative in report["files"]
                        if (root / relative).is_file())
        report["resources"]["artifact_bytes"] = artifacts
        total = artifacts
        for _ in range(8):
            report["resources"]["output_bytes"] = total
            next_total = artifacts + len(_json(report))
            if next_total == total:
                break
            total = next_total

    publish()
    try:
        _image_library()
        trace_required = request.mode == "xray" or any(
            channel in request.channels for channel in geometry_channels)
        scenes = observer.capture_scenes(
            session, samples, with_trace=trace_required,
            include_hidden_geometry=request.mode == "xray" and request.xray.include_disabled)
        report["capture"] = scenes[0].metadata
        report["samples"] = [_sample_record(scene, index)
                             for index, scene in enumerate(scenes)]
        columns, cells = _cells(report["samples"], layout)
        if request.view.framing == "fixed_union" and request.view.roi is None:
            bounds = []
            for scene in scenes:
                try:
                    roi, _, _ = _focus_and_objects(scene, request)
                    bounds.append(roi)
                except Exception as error:
                    if getattr(error, "code", None) != "EMPTY_FOCUS":
                        raise
            if not bounds:
                raise _failure("EMPTY_FOCUS", "All samples have empty selected geometry")
            union = (min(roi[0] for roi in bounds), min(roi[1] for roi in bounds),
                     max(roi[2] for roi in bounds), max(roi[3] for roi in bounds))
            request = replace(request, view=replace(request.view, roi=union))
        report["view_policy"] = {"framing": request.view.framing,
                                 "roi": request.view.roi,
                                 "resolution": request.view.resolution,
                                 "presentation": request.presentation.record(),
                                 "channels": request.channels,
                                 "mode": request.mode,
                                 "xray": (request.xray.__dict__ if request.mode == "xray" else None),
                                 "trace_used_during_capture": trace_required,
                                 "hidden_geometry_captured": request.mode == "xray" and
                                 request.xray.include_disabled}
        texture_bytes = sum(item["width"] * item["height"] * 4
                            for item in scenes[0].metadata["textures"])
        report["resources"]["decoded_texture_estimate_bytes"] = texture_bytes
        report["resources"]["peak_cpu_image_estimate_bytes"] = (
            texture_bytes + request.view.resolution[0] * request.view.resolution[1] *
            4 * (len(request.channels) + (request.mode != "context") +
                 (3 if "mask" in request.channels else 0)) *
            (3 if baseline_index is not None else 2))
        if (report["resources"]["peak_cpu_image_estimate_bytes"] >
                request.limits.max_cpu_retained_bytes):
            raise _failure("OBSERVATION_BUDGET_EXCEEDED",
                           "Estimated decoded textures and images exceed CPU retained byte budget")
        reference_packet = (observer.inspect_scene(scenes[baseline_index], request=render_request)
                            if baseline_index is not None else None)
        target_ids = (_focus_and_objects(scenes[baseline_index], request)[1]
                      if baseline_index is not None else ())
        if reference_packet is not None:
            passes, readbacks, readback_bytes = _render_resources(reference_packet)
            report["resources"]["render_count"] += passes
            report["resources"]["readback_count"] += readbacks
            report["resources"]["readback_bytes"] += readback_bytes
        for index, scene in enumerate(scenes):
            packet = (reference_packet if index == baseline_index else
                      observer.inspect_scene(scene, request=render_request))
            assert packet is not None
            image_bytes = sum(view.width * view.height * 4 for view in packet.views)
            retained = texture_bytes + image_bytes * (
                3 if reference_packet is not None else 2)
            report["resources"]["peak_cpu_image_estimate_bytes"] = max(
                report["resources"]["peak_cpu_image_estimate_bytes"], retained)
            if retained > request.limits.max_cpu_retained_bytes:
                raise _failure("OBSERVATION_BUDGET_EXCEEDED",
                               "Diagnostic views exceed CPU retained byte budget")
            if (reference_packet is not None and index != baseline_index and
                    any(channel in request.channels for channel in ("displacement", "distortion"))):
                from ._deformation import add_baseline_diagnostics
                packet = add_baseline_diagnostics(packet, reference_packet, request)
                report["diagnostics"].append({"sample_index": index,
                                              "baseline_index": baseline_index,
                                              "deformation": packet.deformation})
            if index != baseline_index:
                passes, readbacks, readback_bytes = _render_resources(packet)
                report["resources"]["render_count"] += passes
                report["resources"]["readback_count"] += readbacks
                report["resources"]["readback_bytes"] += readback_bytes
            if "adapter" not in report:
                first_view = packet.views[0]
                report["adapter"] = {"name": first_view.adapter_name,
                                     "backend": first_view.adapter_backend}
            report["objects"] = packet.objects if not report["objects"] else report["objects"]
            saved = packet.save(root / "packets" / f"{index:03d}", profile="report")
            base = f"packets/{index:03d}"
            report["files"][f"{base}/packet.json"] = saved.manifest_sha256
            for relative, digest in saved.artifact_hashes:
                report["files"][f"{base}/{relative}"] = digest
            entry = report["samples"][index]
            entry["status"] = "complete"
            entry["packet_path"] = base
            entry["focus_status"] = packet.focus_status
            entry["views"] = []
            for view_index, view in enumerate(packet.views):
                relative = f"{base}/views/{view_index:03d}.png"
                view_record = {"view_id": view.view_id, "sample_index": index,
                               "kind": view.kind, "path": relative,
                               "sha256": view.artifact_sha256,
                               "render_digest": view.render_digest,
                               "adapter_name": view.adapter_name,
                               "adapter_backend": view.adapter_backend,
                               "width": view.width, "height": view.height,
                               "requested_roi": view.requested_roi,
                               "padded_roi": view.padded_roi,
                               "visible_roi": view.visible_roi,
                               "content_rect": view.content_rect,
                               "canvas_to_image": view.canvas_to_image_matrix,
                               "image_to_canvas": view.image_to_canvas_matrix,
                               "canvas": view.canvas,
                               "presentation": view.presentation,
                               "pixel_policy": (
                                   "composite_alpha_from_raw_v1" if view.kind == "alpha"
                                   else "straight_alpha_renderer_native_v1" if
                                   (view.presentation or {}).get("alpha") == "straight"
                                   else "opaque_renderer_native_v1"),
                               "dependencies": (view.presentation or {}).get(
                                   "diagnostic_plan", {}).get("mask_only_mesh_ids", []),
                               "overrides": (view.presentation or {}).get("overrides", {}),
                               "mode": view.mode, "status": view.status}
                entry["views"].append(view_record)
                report["views"].append(view_record)
                report["resources"]["output_pixels"] += view.width * view.height
            if reference_packet is not None and index != baseline_index:
                comparison = compare_observations(packet, reference_packet,
                                                  options=CompareOptions(
                                                      target_mesh_ids=target_ids))
                record = {"sample_index": index, "baseline_index": baseline_index,
                          "current_view_id": comparison.current_view_id,
                          "reference_view_id": comparison.reference_view_id,
                          "reference_sha256": comparison.reference_sha256,
                          "registration_status": comparison.registration_status,
                          "registration": comparison.registration,
                          "view_compatibility": comparison.view_compatibility,
                          "metrics": comparison.metrics,
                          "change_bounds": comparison.change_bounds,
                          "contour_source": comparison.contour_source,
                          "target_basis": comparison.target_basis,
                          "thresholds": comparison.thresholds, "artifacts": []}
                for artifact in comparison.artifacts:
                    relative = f"comparisons/{index:03d}/{artifact.kind}.png"
                    _write(root / relative, artifact.png)
                    report["files"][relative] = artifact.artifact_sha256
                    record["artifacts"].append({"kind": artifact.kind,
                                                "path": relative,
                                                "sha256": artifact.artifact_sha256,
                                                "width": artifact.width,
                                                "height": artifact.height})
                    report["resources"]["output_pixels"] += artifact.width * artifact.height
                report["comparisons"].append(record)
            publish()
            if packet is not reference_packet:
                packet.close()
        report["pages"] = _save_pages(root, report, columns, cells, layout,
                                       request.limits.max_artifact_pixels -
                                       report["resources"]["output_pixels"])
        report["resources"]["output_pixels"] += sum(
            page["width"] * page["height"] for page in report["pages"])
        if report["resources"]["output_pixels"] > request.limits.max_artifact_pixels:
            raise _failure("OBSERVATION_BUDGET_EXCEEDED", "Run artifacts exceed pixel budget")
        report["resources"]["elapsed_ms"] = (perf_counter() - start) * 1000
        report["status"] = "complete"
        account_bytes()
        publish()
        if reference_packet is not None:
            reference_packet.close()
        return InspectionRun(root, report_path, "complete", report)
    except Exception as error:
        report["status"] = "failed"
        report["diagnostics"].append({"code": getattr(error, "code", type(error).__name__),
                                      "severity": "error", "message": str(error)})
        report["resources"]["elapsed_ms"] = (perf_counter() - start) * 1000
        account_bytes()
        publish()
        error.run_directory = root
        raise


def open_inspection_run(run_directory: Path) -> InspectionRun:
    """Validate a v2 report and every recorded artifact without a GPU wheel."""
    if not run_directory.is_absolute():
        raise ValueError("Inspection run directory must be absolute")
    root = run_directory.resolve()
    report_path = root / "report.json"
    if report_path.is_symlink() or not report_path.is_file() or report_path.stat().st_size > 64 * 1024 * 1024:
        raise ValueError("Inspection run report is missing or too large")
    report = _parse_json(report_path.read_bytes())
    if (not isinstance(report, dict) or report.get("schema_version") != 2 or
            report.get("kind") != "kasane-inspection-run" or
            report.get("status") not in ("complete", "failed")):
        raise ValueError("Unsupported or incomplete inspection run report")
    files = report.get("files")
    if not isinstance(files, dict) or len(files) > 1024:
        raise ValueError("Invalid inspection run file index")
    remaining = 512 * 1024 * 1024
    image_dimensions = {}
    for relative, digest in files.items():
        if not isinstance(relative, str) or not isinstance(digest, str):
            raise ValueError("Invalid inspection run artifact descriptor")
        content = _read_member(root, relative, digest, remaining)
        remaining -= len(content)
        if relative.endswith(".png"):
            if (content[:16] != b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR" or
                    len(content) < 24):
                raise ValueError("Inspection run artifact is not a PNG")
            image_dimensions[relative] = (int.from_bytes(content[16:20], "big"),
                                          int.from_bytes(content[20:24], "big"))
    samples = report.get("samples")
    if not isinstance(samples, list) or len(samples) > 64:
        raise ValueError("Invalid inspection run sample index")
    for index, sample in enumerate(samples):
        if sample.get("sample_index") != index:
            raise ValueError("Inspection run sample order is invalid")
        if sample.get("status") == "complete":
            base = f"packets/{index:03d}"
            if sample.get("packet_path") != base or f"{base}/packet.json" not in files:
                raise ValueError("Inspection run packet path is invalid")
            for view in sample.get("views", ()):
                path = view.get("path")
                if (not isinstance(path, str) or not path.startswith(base + "/views/") or
                        files.get(path) != view.get("sha256") or
                        image_dimensions.get(path) != (view.get("width"), view.get("height"))):
                    raise ValueError("Inspection run view artifact is invalid")
        elif report["status"] == "complete":
            raise ValueError("Complete run contains an incomplete sample")
    for page in report.get("pages", ()):
        if (files.get(page.get("path")) != page.get("sha256") or
                image_dimensions.get(page.get("path")) !=
                (page.get("width"), page.get("height"))):
            raise ValueError("Inspection run sheet artifact is invalid")
    for comparison in report.get("comparisons", ()):
        for artifact in comparison.get("artifacts", ()):
            if (files.get(artifact.get("path")) != artifact.get("sha256") or
                    image_dimensions.get(artifact.get("path")) !=
                    (artifact.get("width"), artifact.get("height"))):
                raise ValueError("Inspection run comparison artifact is invalid")
    return InspectionRun(root, report_path, report["status"], report)
