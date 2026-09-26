"""Frozen inspection packets, O2 presentation, and bounded save profiles.

A packet never consults a live authoring session after capture.
"""
from __future__ import annotations

from dataclasses import dataclass, replace
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
from time import perf_counter
from typing import TYPE_CHECKING
from uuid import uuid4

if TYPE_CHECKING:
    from ._comparison import ComparisonResult
    from ._observe import CapturedScene, RenderedSceneView
    from ._types import ObservedFrame

MAX_MANIFEST_BYTES = 64 * 1024 * 1024
MAX_PACKET_BYTES = 512 * 1024 * 1024
MAX_PAGE_PIXELS = 16_000_000
MAX_ARTIFACT_PIXELS = 64_000_000
MAX_VIEWS = 64


def _sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _unavailable(message: str) -> Exception:
    from ._observe import ObservationFailure
    error = ObservationFailure(message)
    error.code = "CAPTURE_NOT_AVAILABLE"
    error.asset_id = None
    return error


def _failure(code: str, message: str) -> Exception:
    from ._observe import ObservationFailure
    error = ObservationFailure(message)
    error.code = code
    error.asset_id = None
    return error


def _write(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_bytes(data)
    os.replace(temporary, path)


def _json(data: object) -> bytes:
    return (json.dumps(data, ensure_ascii=False, sort_keys=True, indent=2,
                       allow_nan=False) + "\n").encode("utf-8")


def _parse_json(data: bytes) -> object:
    def reject_constant(value: str) -> None:
        raise ValueError(f"Nonfinite JSON value {value} is forbidden")
    def finite_float(value: str) -> float:
        result = float(value)
        if not math.isfinite(result):
            raise ValueError("Nonfinite JSON number is forbidden")
        return result
    return json.loads(data, parse_constant=reject_constant, parse_float=finite_float)


def _read_member(directory: Path, relative: str, declared_hash: str, budget: int) -> bytes:
    path_part = PurePosixPath(relative)
    if (path_part.is_absolute() or not path_part.parts or
            any(part in (".", "..") for part in path_part.parts)):
        raise ValueError("Invalid packet member path")
    path = directory.joinpath(*path_part.parts)
    if path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(directory):
        raise ValueError("Packet member is missing or not a regular in-bundle file")
    if path.stat().st_size > budget:
        raise ValueError("Packet member exceeds size limit")
    data = path.read_bytes()
    if _sha(data) != declared_hash:
        raise ValueError(f"Packet member hash mismatch: {relative}")
    return data


@dataclass(frozen=True)
class RawInspectionRequest:
    """One explicit source-canvas ROI and raw RGBA output size."""

    roi: tuple[float, float, float, float]
    resolution: tuple[int, int] = (1024, 1024)
    padding_canvas: float = 0.0

    def __post_init__(self) -> None:
        if len(self.roi) != 4 or len(self.resolution) != 2:
            raise ValueError("ROI needs four values and resolution needs two")
        if (not all(math.isfinite(value) for value in (*self.roi, self.padding_canvas))
                or self.roi[2] <= self.roi[0] or self.roi[3] <= self.roi[1]
                or self.padding_canvas < 0):
            raise ValueError("ROI and padding must be finite with positive extent")
        if (any(type(side) is not int or side <= 0 or side > 4096
                for side in self.resolution)):
            raise ValueError("Each output side must be an integer in 1..4096")
        if self.resolution[0] * self.resolution[1] > MAX_PAGE_PIXELS:
            raise ValueError("A raw view may contain at most 16 million pixels")


@dataclass(frozen=True)
class Focus:
    mesh_ids: tuple[str, ...] = ()
    part_ids: tuple[str, ...] = ()

    def __post_init__(self) -> None:
        if any(not isinstance(item, str) or not item for item in (*self.mesh_ids, *self.part_ids)):
            raise ValueError("Focus IDs must be nonempty strings")


@dataclass(frozen=True)
class ViewSpec:
    roi: tuple[float, float, float, float] | None = None
    resolution: tuple[int, int] = (1024, 1024)
    padding_canvas: float = 0.0
    framing: str = "fixed_union"
    aspect: str = "contain"

    def __post_init__(self) -> None:
        if self.framing not in ("fixed_union", "follow") or self.aspect != "contain":
            raise ValueError("Unsupported framing or aspect policy")
        RawInspectionRequest(self.roi if self.roi is not None else (0.0, 0.0, 1.0, 1.0),
                             self.resolution, self.padding_canvas)


@dataclass(frozen=True)
class PresentationSpec:
    background: str = "light"
    alpha: str = "opaque"
    color_policy: str = "renderer_native_v1"
    light_rgb: tuple[int, int, int] = (238, 238, 238)
    dark_rgb: tuple[int, int, int] = (32, 32, 32)
    checker_tile_px: int = 16
    checker_origin_px: tuple[int, int] = (0, 0)

    def __post_init__(self) -> None:
        if self.background not in ("light", "dark", "checker", "transparent"):
            raise ValueError("Unsupported presentation background")
        if self.alpha not in ("opaque", "straight"):
            raise ValueError("Unsupported presentation alpha policy")
        if (self.alpha == "straight") != (self.background == "transparent"):
            raise ValueError("Straight alpha requires transparent background; opaque alpha requires a display background")
        if self.color_policy != "renderer_native_v1":
            raise ValueError("Unsupported color policy")
        for rgb in (self.light_rgb, self.dark_rgb):
            if len(rgb) != 3 or any(type(v) is not int or not 0 <= v <= 255 for v in rgb):
                raise ValueError("Presentation colors must be RGB8 triples")
        if type(self.checker_tile_px) is not int or not 1 <= self.checker_tile_px <= 4096:
            raise ValueError("Checker tile must be 1..4096 pixels")
        if (len(self.checker_origin_px) != 2 or any(
            type(v) is not int or abs(v) > 1_000_000 for v in self.checker_origin_px
        )):
            raise ValueError("Checker origin exceeds supported range")

    def native_background(self) -> tuple[str, tuple[int, int, int], tuple[int, int, int], int, tuple[int, int]]:
        if self.background == "transparent":
            kind, light, dark = "transparent", self.light_rgb, self.dark_rgb
        elif self.background == "checker":
            kind, light, dark = "checker", self.light_rgb, self.dark_rgb
        else:
            kind = "solid"
            light = self.light_rgb if self.background == "light" else self.dark_rgb
            dark = self.dark_rgb
        return kind, light, dark, self.checker_tile_px, self.checker_origin_px

    def record(self) -> dict:
        kind, light, dark, tile, origin = self.native_background()
        return {
            "background": self.background, "kind": kind,
            "light_rgb": light, "dark_rgb": dark if kind == "checker" else None,
            "checker_tile_px": tile if kind == "checker" else None,
            "checker_origin_px": origin if kind == "checker" else None,
            "alpha": self.alpha, "color_policy": self.color_policy,
            "target_format": "RGBA8Unorm", "display_conversion": "none",
            "texture_sampling": "renderer_configuration",
            "unpremultiply": "round_half_up_rgb_zero_at_alpha_zero" if self.alpha == "straight" else None,
        }


@dataclass(frozen=True)
class OverlaySpec:
    max_labels: int = 12
    vertex_ids: tuple[int, ...] = ()
    show_leaders: bool = True

    def __post_init__(self) -> None:
        if type(self.max_labels) is not int or not 0 <= self.max_labels <= 64:
            raise ValueError("max_labels must be 0..64")
        if len(self.vertex_ids) > 64 or any(type(value) is not int or value < 0 for value in self.vertex_ids):
            raise ValueError("Vertex labels require at most 64 nonnegative vertex IDs")


@dataclass(frozen=True)
class DiagnosticSpec:
    alpha_threshold: float = 1 / 255
    min_stretch: float = 0.5
    max_stretch: float = 2.0
    trace_fields: tuple[str, ...] = ()

    def __post_init__(self) -> None:
        if (not all(math.isfinite(v) for v in
                    (self.alpha_threshold, self.min_stretch, self.max_stretch)) or
                not 0 <= self.alpha_threshold <= 1 or self.min_stretch <= 0 or
                self.max_stretch < 1):
            raise ValueError("Invalid diagnostic thresholds")
        if any(field not in ("meshes", "transforms") for field in self.trace_fields):
            raise ValueError("Unsupported trace field")


@dataclass(frozen=True)
class XraySpec:
    ignore_masks: bool = False
    ignore_opacity: bool = False
    include_disabled: bool = False
    highlight_rgb: tuple[int, int, int] = (255, 96, 16)

    def __post_init__(self) -> None:
        if any(type(value) is not bool for value in
               (self.ignore_masks, self.ignore_opacity, self.include_disabled)):
            raise ValueError("X-ray overrides must be explicit booleans")
        if len(self.highlight_rgb) != 3 or any(type(value) is not int or
                                               not 0 <= value <= 255 for value in
                                               self.highlight_rgb):
            raise ValueError("X-ray highlight must be an RGB8 triple")


@dataclass(frozen=True)
class InspectionLimits:
    max_view_side: int = 4096
    max_page_pixels: int = MAX_PAGE_PIXELS
    max_artifact_pixels: int = MAX_ARTIFACT_PIXELS
    max_samples: int = 64
    max_labels: int = 12
    max_vertex_labels: int = 64
    max_query_candidates: int = 256
    max_cpu_retained_bytes: int = 268_435_456

    def __post_init__(self) -> None:
        if (not 1 <= self.max_view_side <= 4096 or
                not 1 <= self.max_page_pixels <= MAX_PAGE_PIXELS or
                not 1 <= self.max_artifact_pixels <= MAX_ARTIFACT_PIXELS or
                not 1 <= self.max_samples <= 64 or
                not 0 <= self.max_labels <= 64 or
                not 0 <= self.max_vertex_labels <= 64 or
                not 1 <= self.max_query_candidates <= 256 or
                not 1 <= self.max_cpu_retained_bytes <= 268_435_456):
            raise ValueError("Inspection limits exceed supported bounds")


@dataclass(frozen=True)
class InspectionRequest:
    focus: Focus = Focus()
    view: ViewSpec = ViewSpec()
    presentation: PresentationSpec = PresentationSpec()
    overlay: OverlaySpec = OverlaySpec()
    diagnostics: DiagnosticSpec = DiagnosticSpec()
    xray: XraySpec = XraySpec()
    channels: tuple[str, ...] = ("clean", "labels")
    mode: str = "context"
    limits: InspectionLimits = InspectionLimits()
    allow_partial: bool = False

    def __post_init__(self) -> None:
        if self.mode not in ("context", "isolated", "xray"):
            raise ValueError("Unsupported inspection mode")
        if not self.channels or len(set(self.channels)) != len(self.channels) or any(
            channel not in ("clean", "labels", "alpha", "wireframe", "vertices",
                            "deformers", "displacement", "distortion", "mask") for channel in self.channels
        ):
            raise ValueError("Unsupported inspection channel")
        if "clean" not in self.channels:
            raise ValueError("Inspection must retain a clean view")
        if self.allow_partial:
            raise ValueError("Partial inspection output is not yet supported")
        width, height = self.view.resolution
        if (max(width, height) > self.limits.max_view_side or
                width * height > self.limits.max_page_pixels or
                width * height * (len(self.channels) + (self.mode != "context")) >
                self.limits.max_artifact_pixels or
                self.overlay.max_labels > self.limits.max_labels):
            raise ValueError("Inspection request exceeds declared limits")


@dataclass(frozen=True)
class InspectionView:
    view_id: str
    sample_index: int
    width: int
    height: int
    png: bytes
    rgba: bytes | None
    requested_roi: tuple[float, float, float, float]
    padded_roi: tuple[float, float, float, float]
    visible_roi: tuple[float, float, float, float]
    view_scale: float
    view_offset: tuple[float, float]
    render_digest: str
    artifact_sha256: str
    adapter_name: str
    adapter_backend: str
    frame: ObservedFrame | None = None
    kind: str = "raw_context"
    presentation: dict | None = None
    omitted_labels: tuple[dict, ...] = ()
    canvas: tuple[float, float, float, float, float] | None = None
    mode: str = "context"
    status: str = "complete"

    def canvas_to_image(self, point: tuple[float, float]) -> tuple[float, float]:
        return (point[0] * self.view_scale + self.view_offset[0],
                point[1] * self.view_scale + self.view_offset[1])

    def image_to_canvas(self, point: tuple[float, float]) -> tuple[float, float]:
        return ((point[0] - self.view_offset[0]) / self.view_scale,
                (point[1] - self.view_offset[1]) / self.view_scale)

    @property
    def canvas_to_image_matrix(self) -> tuple[tuple[float, float, float], ...]:
        return ((self.view_scale, 0.0, self.view_offset[0]),
                (0.0, self.view_scale, self.view_offset[1]), (0.0, 0.0, 1.0))

    @property
    def image_to_canvas_matrix(self) -> tuple[tuple[float, float, float], ...]:
        inverse = 1.0 / self.view_scale
        return ((inverse, 0.0, -self.view_offset[0] * inverse),
                (0.0, inverse, -self.view_offset[1] * inverse), (0.0, 0.0, 1.0))

    @property
    def content_rect(self) -> tuple[float, float, float, float]:
        x0, y0 = self.canvas_to_image(self.padded_roi[:2])
        x1, y1 = self.canvas_to_image(self.padded_roi[2:])
        return x0, y0, x1, y1

    def runtime_to_canvas(self, point: tuple[float, float]) -> tuple[float, float]:
        if self.canvas is None:
            raise _unavailable("Runtime mapping was not stored in this view")
        _, _, ox, oy, ppu = self.canvas
        return ox + point[0] * ppu, oy - point[1] * ppu

    def canvas_to_runtime(self, point: tuple[float, float]) -> tuple[float, float]:
        if self.canvas is None:
            raise _unavailable("Runtime mapping was not stored in this view")
        _, _, ox, oy, ppu = self.canvas
        return (point[0] - ox) / ppu, (oy - point[1]) / ppu


@dataclass(frozen=True)
class PacketSaveReceipt:
    directory: Path
    profile: str
    manifest_sha256: str
    artifact_hashes: tuple[tuple[str, str], ...]
    saved_bytes: int
    elapsed_ms: float


@dataclass(frozen=True)
class InspectionPacket:
    """Frozen raw views plus an optional scene for further GPU rendering."""

    capture_id: str
    scene_digest: str
    metadata: dict
    source: dict
    objects: tuple[dict, ...]
    views: tuple[InspectionView, ...]
    profile: str
    authoring: dict | None
    evaluated_frame: dict | None
    _scene: CapturedScene | None = None
    _closed: bool = False
    focus_status: str = "not_requested"
    comparison: ComparisonResult | None = None
    evaluation_trace: dict | None = None
    deformation: dict | None = None

    @property
    def source_kind(self) -> str:
        return self.source["source_kind"]

    @property
    def document_id(self) -> str:
        return self.metadata["document_id"]

    @property
    def version(self) -> tuple[int, int, int]:
        return tuple(self.metadata["version"])

    @property
    def evaluation_revision(self) -> int:
        return self.metadata["evaluation_revision"]

    @property
    def closed(self) -> bool:
        return self._closed

    @property
    def capabilities(self) -> dict[str, bool]:
        return {
            "raw_view": any(view.kind == "raw_context" for view in self.views),
            "raw_pixel_data": all(view.rgba is not None for view in self.views),
            "evaluated_geometry_data": self.evaluated_frame is not None,
            "rerender_scene": self._scene is not None and not self._closed,
            "presentation": any(view.kind in ("clean", "labels", "alpha", "isolated") for view in self.views),
            "isolated": any(view.kind == "isolated" for view in self.views),
            "mask": any(view.kind.startswith("mask_") for view in self.views),
            "xray": any(view.kind == "xray" for view in self.views),
            "comparison": self.comparison is not None,
            "evaluation_trace": self.evaluation_trace is not None,
            "deformation": self.deformation is not None,
            "geometry_query": self.authoring is not None and self.evaluated_frame is not None and not self._closed,
            "pixel_coverage_query": self._scene is not None and not self._closed,
            "playback_replay": False,
        }

    def close(self) -> None:
        """Release this packet's native scene reference; saved data stays readable."""
        object.__setattr__(self, "_scene", None)
        object.__setattr__(self, "_closed", True)

    def object_details(self, object_id: str):
        """Read this packet's frozen source and evaluated object evidence."""
        from ._query import object_details
        return object_details(self, object_id=object_id)

    def query(self, *, view_id: str, point: tuple[float, float] | None = None,
              region: tuple[int, int, int, int] | None = None,
              mode: str = "geometry", alpha_threshold: float = 1 / 255,
              max_hits: int = 256, observer=None):
        """Query geometry offline, or pass an Observer for GPU coverage."""
        from ._query import coverage_query, geometry_query
        if not math.isfinite(alpha_threshold) or not 0 <= alpha_threshold <= 1:
            raise ValueError("alpha_threshold must be in 0..1")
        if mode not in ("geometry", "coverage", "frontmost_covered"):
            raise ValueError("Unknown query mode")
        if mode == "geometry":
            return geometry_query(self, view_id=view_id, point=point,
                                  region=region, max_hits=max_hits)
        if observer is None:
            raise _unavailable("GPU coverage requires an Observer")
        return coverage_query(observer, self, view_id=view_id, point=point,
                              region=region, mode=mode,
                              alpha_threshold=alpha_threshold, max_hits=max_hits)

    def __enter__(self) -> InspectionPacket:
        return self

    def __exit__(self, exception_type, exception, traceback) -> bool:
        self.close()
        return False

    def save(self, absolute_directory: Path, *, profile: str = "analysis") -> PacketSaveReceipt:
        """Save report, analysis, or scene data to a new absolute directory."""
        if profile not in ("report", "analysis", "scene"):
            raise ValueError("profile must be report, analysis, or scene")
        comparison_pixels = sum(item.width * item.height for item in
                                self.comparison.artifacts) if self.comparison else 0
        if (len(self.views) > MAX_VIEWS or
                sum(view.width * view.height for view in self.views) +
                comparison_pixels > MAX_ARTIFACT_PIXELS):
            raise ValueError("Packet view count or pixel budget exceeded")
        if not absolute_directory.is_absolute():
            raise ValueError("Packet directory must be absolute")
        if profile == "scene" and (self._scene is None or self._closed):
            raise _unavailable("This packet has no captured scene to save")
        if profile != "report" and (self.authoring is None or self.evaluated_frame is None or
                                    any(view.rgba is None for view in self.views)):
            raise _unavailable("Analysis payload was not captured")
        start = perf_counter()
        absolute_directory.mkdir(parents=False, exist_ok=False)
        hashes: dict[str, str] = {}
        view_entries = []
        for index, view in enumerate(self.views):
            if (view.width <= 0 or view.height <= 0 or view.width > 4096 or
                    view.height > 4096 or view.width * view.height > MAX_PAGE_PIXELS):
                raise ValueError("View dimensions exceed packet bounds")
            png_path = f"views/{index:03d}.png"
            _write(absolute_directory / png_path, view.png)
            hashes[png_path] = _sha(view.png)
            entry = {
                "view_id": view.view_id, "sample_index": view.sample_index,
                "kind": view.kind, "mode": view.mode, "status": view.status,
                "width": view.width, "height": view.height,
                "png_path": png_path, "artifact_sha256": hashes[png_path],
                "requested_roi": view.requested_roi, "padded_roi": view.padded_roi,
                "visible_roi": view.visible_roi, "view_scale": view.view_scale,
                "view_offset": view.view_offset, "render_digest": view.render_digest,
                "adapter_name": view.adapter_name,
                "adapter_backend": view.adapter_backend,
                "pixel_policy": (
                    "premultiplied_linear_rgba8_transparent_v1" if view.kind == "raw_context"
                    else "composite_alpha_from_raw_v1" if view.kind == "alpha"
                    else "straight_alpha_renderer_native_v1" if
                    (view.presentation or {}).get("alpha") == "straight"
                    else "opaque_renderer_native_v1"
                ),
                "presentation": view.presentation,
                "omitted_labels": view.omitted_labels,
                "canvas": view.canvas,
            }
            if profile != "report":
                raw = view.rgba
                assert raw is not None
                if len(raw) != view.width * view.height * 4:
                    raise ValueError("View RGBA byte count does not match dimensions")
                raw_path = f"views/{index:03d}.rgba"
                _write(absolute_directory / raw_path, raw)
                hashes[raw_path] = _sha(raw)
                entry.update(raw_path=raw_path, raw_sha256=hashes[raw_path],
                             raw_dtype="u8", raw_shape=[view.height, view.width, 4],
                             raw_byte_order="not_applicable")
            view_entries.append(entry)
        if profile != "report":
            data_files = [("authoring.json", self.authoring),
                          ("evaluated-frame.json", self.evaluated_frame)]
            if self.evaluation_trace is not None:
                data_files.append(("evaluation-trace.json", self.evaluation_trace))
            for relative, data in data_files:
                content = _json(data)
                _write(absolute_directory / relative, content)
                hashes[relative] = _sha(content)
        if profile == "scene":
            assert self._scene is not None
            self._scene.save_scene(absolute_directory / "scene")
        comparison_record = None
        if self.comparison is not None:
            result = self.comparison
            comparison_record = {
                "current_view_id": result.current_view_id,
                "reference_view_id": result.reference_view_id,
                "reference_sha256": result.reference_sha256,
                "registration_status": result.registration_status,
                "registration": result.registration,
                "view_compatibility": result.view_compatibility,
                "metrics": result.metrics,
                "change_bounds": result.change_bounds,
                "contour_source": result.contour_source,
                "target_basis": result.target_basis,
                "thresholds": result.thresholds,
                "artifacts": [],
            }
            for index, artifact in enumerate(result.artifacts):
                if (artifact.width <= 0 or artifact.height <= 0 or
                        artifact.width * artifact.height > MAX_PAGE_PIXELS or
                        _sha(artifact.png) != artifact.artifact_sha256):
                    raise ValueError("Invalid comparison artifact")
                relative = f"comparison/{index:03d}-{artifact.kind}.png"
                _write(absolute_directory / relative, artifact.png)
                hashes[relative] = artifact.artifact_sha256
                comparison_record["artifacts"].append({
                    "kind": artifact.kind, "path": relative,
                    "width": artifact.width, "height": artifact.height,
                    "sha256": artifact.artifact_sha256,
                })
        manifest = {
            "schema_version": 2, "kind": "kasane-inspection-packet",
            "status": "complete", "profile": profile,
            "capture": self.metadata, "source": self.source,
            "focus_status": self.focus_status,
            "objects": self.objects, "views": view_entries,
            "comparison": comparison_record,
            "deformation": self.deformation,
            "capabilities": {
                "raw_view": any(view.kind == "raw_context" for view in self.views),
                "raw_pixel_data": profile != "report",
                "evaluated_geometry_data": profile != "report",
                "rerender_scene": profile == "scene",
                "presentation": any(view.kind in ("clean", "labels", "alpha", "isolated") for view in self.views),
                "geometry_query": profile != "report",
                "isolated": any(view.kind == "isolated" for view in self.views),
                "mask": any(view.kind.startswith("mask_") for view in self.views),
                "xray": any(view.kind == "xray" for view in self.views),
                "comparison": self.comparison is not None,
                "evaluation_trace": self.evaluation_trace is not None and profile != "report",
                "deformation": self.deformation is not None,
                "pixel_coverage_query": profile == "scene", "playback_replay": False,
            },
            "files": hashes,
        }
        content = _json(manifest)
        if len(content) > MAX_MANIFEST_BYTES:
            raise ValueError("Packet manifest exceeds 64 MiB")
        _write(absolute_directory / "packet.json", content)
        saved_bytes = len(content) + sum((absolute_directory / path).stat().st_size
                                         for path in hashes)
        if profile == "scene":
            saved_bytes += sum(path.stat().st_size for path in
                               (absolute_directory / "scene").iterdir() if path.is_file())
        return PacketSaveReceipt(absolute_directory, profile, _sha(content),
                                 tuple(sorted(hashes.items())), saved_bytes,
                                 (perf_counter() - start) * 1000)


def _objects(authoring: dict) -> tuple[dict, ...]:
    result = []
    for kind, key in (("mesh", "meshes"), ("part", "parts"),
                      ("transform", "transforms"), ("binding", "bindings"),
                      ("parameter", "parameters"), ("asset", "assets"),
                      ("scene_binding", "scene_bindings"), ("offscreen", "offscreens"),
                      ("glue", "glues"), ("blend_key_table", "blend_key_tables"),
                      ("blend_constraint", "blend_constraints"),
                      ("blend_binding", "blend_bindings")):
        for item in authoring.get(key, []):
            result.append({"id": item["id"], "name": item.get("name"), "kind": kind})
    return tuple(result)


def _focus_and_objects(scene: CapturedScene, request: InspectionRequest) -> tuple[
    tuple[float, float, float, float], tuple[str, ...], tuple[dict, ...]
]:
    authoring, evaluated = scene.authoring, scene.evaluated_frame
    meshes = {item["id"]: item for item in authoring.get("meshes", ())}
    parts = {item["id"]: item for item in authoring.get("parts", ())}
    for item in request.focus.mesh_ids:
        if item not in meshes:
            raise _failure("UNKNOWN_FOCUS", f"Unknown mesh ID: {item}")
    for item in request.focus.part_ids:
        if item not in parts:
            raise _failure("UNKNOWN_FOCUS", f"Unknown Part ID: {item}")

    def part_path(part_id: str) -> tuple[str, ...]:
        path, seen = [], set()
        while part_id and part_id in parts and part_id not in seen:
            seen.add(part_id)
            path.append(part_id)
            part_id = parts[part_id]["parent_id"]
        return tuple(reversed(path))

    selected = tuple(sorted(set(request.focus.mesh_ids) | {
        mesh["id"] for mesh in meshes.values()
        if set(part_path(mesh.get("part_id", ""))) & set(request.focus.part_ids)
    }))
    frame_meshes = {item["id"]: item for item in evaluated.get("drawables", ())}
    canvas = evaluated["canvas"]
    origin, ppu = canvas["origin"], canvas["pixels_per_unit"]
    if not math.isfinite(ppu) or ppu <= 0:
        raise _failure("INVALID_CAPTURE", "Captured canvas has invalid pixels per unit")

    target_paths: dict[str, tuple[str, ...]] = {}
    stack: list[str] = []
    for command in evaluated.get("render_plan", ()):
        if "BeginOffscreen" in command:
            stack.append(command["BeginOffscreen"]["offscreen_id"])
        elif "EndOffscreen" in command:
            if stack:
                stack.pop()
        elif "DrawMesh" in command:
            target_paths[command["DrawMesh"]["mesh_id"]] = tuple(stack)

    marks = {mesh_id: index + 1 for index, mesh_id in enumerate(sorted(meshes))}
    details = []
    for kind, key in (("mesh", "meshes"), ("part", "parts"),
                      ("transform", "transforms"), ("binding", "bindings"),
                      ("parameter", "parameters"), ("asset", "assets"),
                      ("scene_binding", "scene_bindings"), ("offscreen", "offscreens"),
                      ("glue", "glues"), ("blend_key_table", "blend_key_tables"),
                      ("blend_constraint", "blend_constraints"),
                      ("blend_binding", "blend_bindings")):
        for item in authoring.get(key, ()):
            row = {"id": item["id"], "name": item.get("name"), "kind": kind}
            if kind == "mesh":
                current = frame_meshes.get(item["id"])
                points = () if current is None else current.get("positions", ())
                canvas_points = tuple((origin["x"] + point["x"] * ppu,
                                       origin["y"] - point["y"] * ppu) for point in points)
                bounds = (min(x for x, _ in canvas_points), min(y for _, y in canvas_points),
                          max(x for x, _ in canvas_points), max(y for _, y in canvas_points)) if canvas_points else None
                topology = {"vertex_ids": item["vertex_ids"], "triangles": item["triangles"]}
                row.update(
                    mark=marks[item["id"]], runtime_id=item["runtime_id"],
                    part_path=part_path(item.get("part_id", "")),
                    part_name_path=tuple(parts[id]["name"] for id in part_path(item.get("part_id", ""))),
                    deformer_parent=item.get("deformer_id") or None,
                    geometry_space="runtime_with_canvas_mapping",
                    enabled=current["enabled"] if current else item["enabled"],
                    visible=current["visible"] if current else False,
                    opacity=current["opacity"] if current else None,
                    render_order=current["render_order"] if current else None,
                    composition_path=target_paths.get(item["id"]),
                    composition_path_status=("known" if item["id"] in target_paths else "unknown"),
                    mask_ids=current["masks"] if current else item["masks"],
                    topology_hash=_sha(_json(topology)),
                    geometry_bounds_canvas=bounds,
                    coverage_status="unknown",
                )
            details.append(row)

    if request.view.roi is not None:
        roi = request.view.roi
    elif request.focus.mesh_ids or request.focus.part_ids:
        bounds = [row["geometry_bounds_canvas"] for row in details
                  if row["kind"] == "mesh" and row["id"] in selected and row["geometry_bounds_canvas"]]
        if not bounds:
            raise _failure("EMPTY_FOCUS", "Selected objects have no evaluated geometry")
        roi = (min(b[0] for b in bounds), min(b[1] for b in bounds),
               max(b[2] for b in bounds), max(b[3] for b in bounds))
        if roi[2] <= roi[0] or roi[3] <= roi[1]:
            raise _failure("EMPTY_FOCUS", "Selected geometry has zero canvas extent")
    else:
        roi = (0.0, 0.0, canvas["width"], canvas["height"])
    RawInspectionRequest(roi, request.view.resolution, request.view.padding_canvas)
    return roi, selected, tuple(details)


_DIGITS = (
    ("111", "101", "101", "101", "111"),
    ("010", "110", "010", "010", "111"),
    ("111", "001", "111", "100", "111"),
    ("111", "001", "111", "001", "111"),
    ("101", "101", "111", "001", "001"),
    ("111", "100", "111", "001", "111"),
    ("111", "100", "111", "101", "111"),
    ("111", "001", "001", "001", "001"),
    ("111", "101", "111", "101", "111"),
    ("111", "101", "111", "001", "111"),
)


def _labels_view(clean: InspectionView, objects: tuple[dict, ...],
                 selected: tuple[str, ...], request: InspectionRequest) -> InspectionView:
    from ._observe import _encode_rgba_png
    if clean.rgba is None:
        raise _unavailable("Clean pixels are required for labels")
    width, height = clean.width, clean.height
    pixels = bytearray(clean.rgba)

    def dot(x: int, y: int, color: tuple[int, int, int, int]) -> None:
        if 0 <= x < width and 0 <= y < height:
            pixels[(y * width + x) * 4:(y * width + x + 1) * 4] = bytes(color)

    def line(x0: int, y0: int, x1: int, y1: int,
             color: tuple[int, int, int, int]) -> None:
        dx, dy = abs(x1 - x0), -abs(y1 - y0)
        sx, sy = (1 if x0 < x1 else -1), (1 if y0 < y1 else -1)
        error = dx + dy
        while True:
            dot(x0, y0, color)
            if x0 == x1 and y0 == y1:
                break
            doubled = error * 2
            if doubled >= dy:
                error += dy
                x0 += sx
            if doubled <= dx:
                error += dx
                y0 += sy

    rows = [row for row in objects if row["kind"] == "mesh"]
    rows.sort(key=lambda row: (row["id"] not in selected, row["mark"]))
    for row in rows:
        bounds = row["geometry_bounds_canvas"]
        if (row["id"] not in selected or bounds is None or not row["enabled"] or
                not row["visible"] or not row["opacity"]):
            continue
        x0, y0 = clean.canvas_to_image((bounds[0], bounds[1]))
        x1, y1 = clean.canvas_to_image((bounds[2], bounds[3]))
        left, top = max(0, round(x0)), max(0, round(y0))
        right, bottom = min(width - 1, round(x1)), min(height - 1, round(y1))
        if left > right or top > bottom:
            continue
        for x in range(left, right + 1):
            dot(x, top, (0, 220, 255, 255))
            dot(x, bottom, (0, 220, 255, 255))
        for y in range(top, bottom + 1):
            dot(left, y, (0, 220, 255, 255))
            dot(right, y, (0, 220, 255, 255))
    occupied: list[tuple[int, int, int, int]] = []
    omitted: list[dict] = []
    placed: list[dict] = []
    label_limit = request.overlay.max_labels
    for row in rows:
        object_id = row["id"]
        bounds = row["geometry_bounds_canvas"]
        if bounds is None:
            omitted.append({"object_id": object_id, "reason": "no_geometry"})
            continue
        x0, y0 = clean.canvas_to_image((bounds[0], bounds[1]))
        x1, y1 = clean.canvas_to_image((bounds[2], bounds[3]))
        if x1 < 0 or y1 < 0 or x0 >= width or y0 >= height:
            omitted.append({"object_id": object_id, "reason": "outside_view"})
            continue
        if not row["enabled"] or not row["visible"] or not row["opacity"]:
            omitted.append({"object_id": object_id, "reason": "not_visible"})
            continue
        if len(placed) >= label_limit:
            omitted.append({"object_id": object_id, "reason": "label_limit"})
            continue
        ax = max(0, min(width - 1, round((x0 + x1) / 2)))
        ay = max(0, min(height - 1, round((y0 + y1) / 2)))
        mark = str(row["mark"])
        box_width = len(mark) * 4 + 2
        candidates = (
            (round(x1) + 4, round(y0) - 8),
            (round(x0) - box_width - 4, round(y0) - 8),
            (round(x1) + 4, round(y1) + 3),
            (round(x0) - box_width - 4, round(y1) + 3),
        )
        chosen = None
        for bx, by in candidates:
            rect = (bx, by, bx + box_width, by + 8)
            if (bx < 0 or by < 0 or rect[2] > width or rect[3] > height or
                    any(not (rect[2] <= old[0] or old[2] <= rect[0] or
                             rect[3] <= old[1] or old[3] <= rect[1]) for old in occupied)):
                continue
            chosen = rect
            break
        if chosen is None:
            omitted.append({"object_id": object_id, "reason": "no_label_space"})
            continue
        occupied.append(chosen)
        placed.append({"object_id": object_id, "mark": row["mark"],
                       "box": chosen, "anchor": (ax, ay)})
        bx, by, ex, ey = chosen
        if request.overlay.show_leaders:
            line(ax, ay, bx + box_width // 2, by + 4, (255, 224, 32, 255))
        for y in range(by, ey):
            for x in range(bx, ex):
                dot(x, y, (255, 224, 32, 255))
        for index, char in enumerate(mark):
            glyph = _DIGITS[int(char)]
            for gy, bitmap in enumerate(glyph):
                for gx, bit in enumerate(bitmap):
                    if bit == "1":
                        dot(bx + 1 + index * 4 + gx, by + 1 + gy,
                            (16, 16, 16, 255))
    png = _encode_rgba_png(width, height, pixels)
    record = dict(clean.presentation or {})
    record.update({"overlay": "geometry_labels_v1", "labels": placed,
                   "highlight_basis": "evaluated_geometry_bounds"})
    return replace(clean, view_id=uuid4().hex, png=png, rgba=bytes(pixels), frame=None,
                   artifact_sha256=_sha(png),
                   render_digest=_sha(_json({"base": clean.render_digest,
                                             "overlay": record})),
                   kind="labels", presentation=record,
                   omitted_labels=tuple(omitted))


def _alpha_view(clean: InspectionView, raw: InspectionView) -> InspectionView:
    from ._observe import _encode_rgba_png
    if raw.rgba is None:
        raise _unavailable("Transparent raw pixels are required for alpha view")
    if (clean.width, clean.height) != (raw.width, raw.height):
        raise ValueError("Alpha source and presentation view differ in size")
    pixels = bytearray(len(raw.rgba))
    for offset in range(0, len(pixels), 4):
        alpha = raw.rgba[offset + 3]
        pixels[offset:offset + 4] = bytes((alpha, alpha, alpha, 255))
    png = _encode_rgba_png(clean.width, clean.height, pixels)
    record = {"alpha_source": "transparent_raw_pass",
              "alpha_semantics": "composite_alpha", "raw_render_digest": raw.render_digest}
    return replace(clean, view_id=uuid4().hex, png=png, rgba=bytes(pixels), frame=None,
                   artifact_sha256=_sha(png),
                   render_digest=_sha(_json({"raw": raw.render_digest,
                                             "channel": "composite_alpha_v1"})),
                   kind="alpha", presentation=record)


def presentation_packet(scene: CapturedScene, clean_render: RenderedSceneView,
                        objects: tuple[dict, ...], selected: tuple[str, ...],
                        request: InspectionRequest,
                        raw_alpha_render: RenderedSceneView | None) -> InspectionPacket:
    clean = view_from_render(clean_render)
    views = [clean]
    if "labels" in request.channels:
        views.append(_labels_view(clean, objects, selected, request))
    if "alpha" in request.channels:
        if raw_alpha_render is None:
            raise _unavailable("Alpha channel requires a transparent raw pass")
        views.append(_alpha_view(clean, view_from_render(raw_alpha_render)))
    packet = packet_from_scene(scene, clean_render)
    focus_status = "not_requested"
    if request.focus.mesh_ids or request.focus.part_ids:
        focused = [row for row in objects if row["kind"] == "mesh" and row["id"] in selected]
        if not focused or not any(row["geometry_bounds_canvas"] for row in focused):
            focus_status = "empty"
        elif not any(row["enabled"] and row["visible"] and row["opacity"] for row in focused):
            focus_status = "not_visible"
        else:
            x0, y0, x1, y1 = clean.visible_roi
            if not any(row["geometry_bounds_canvas"] and
                       row["geometry_bounds_canvas"][2] > x0 and
                       row["geometry_bounds_canvas"][0] < x1 and
                       row["geometry_bounds_canvas"][3] > y0 and
                       row["geometry_bounds_canvas"][1] < y1 for row in focused):
                focus_status = "outside_view"
            else:
                focus_status = "visible"
    packet = replace(packet, objects=objects, views=tuple(views), focus_status=focus_status)
    if any(channel in request.channels for channel in ("wireframe", "vertices", "deformers")):
        from ._deformation import add_geometry_views
        packet = add_geometry_views(packet, request)
    return packet


def view_from_render(rendered: RenderedSceneView, sample_index: int = 0) -> InspectionView:
    frame = rendered.frame
    return InspectionView(
        uuid4().hex, sample_index, frame.width, frame.height, frame.png, frame.rgba,
        rendered.requested_roi, rendered.padded_roi, rendered.visible_roi,
        frame.view_scale, frame.view_offset, rendered.render_digest,
        _sha(frame.png), frame.adapter_name, frame.adapter_backend, frame,
        rendered.kind, rendered.presentation, (), tuple(frame.canvas),
    )


def packet_from_scene(scene: CapturedScene, rendered: RenderedSceneView) -> InspectionPacket:
    authoring = scene.authoring
    return InspectionPacket(
        scene.capture_id, scene.scene_digest, scene.metadata, scene.source,
        _objects(authoring), (view_from_render(rendered),), "scene", authoring,
        scene.evaluated_frame, scene, evaluation_trace=scene.evaluation_trace,
    )


def append_view(packet: InspectionPacket, rendered: RenderedSceneView) -> InspectionPacket:
    if packet._closed or packet._scene is None:
        raise _unavailable("Packet has no open scene for another render")
    return replace(packet, views=(*packet.views, view_from_render(rendered)))


def check_next_view(packet: InspectionPacket, request: RawInspectionRequest) -> None:
    if len(packet.views) >= MAX_VIEWS or (
        sum(view.width * view.height for view in packet.views)
        + request.resolution[0] * request.resolution[1] > MAX_ARTIFACT_PIXELS
    ):
        raise ValueError("Packet view count or pixel budget exceeded")


def open_inspection_packet(absolute_directory: Path) -> InspectionPacket:
    """Validate a saved packet and restore only its recorded capabilities."""
    if not absolute_directory.is_absolute():
        raise ValueError("Packet directory must be absolute")
    directory = absolute_directory.resolve()
    manifest_path = directory / "packet.json"
    if manifest_path.is_symlink() or not manifest_path.is_file():
        raise ValueError("Packet manifest must be a regular file")
    if manifest_path.stat().st_size > MAX_MANIFEST_BYTES:
        raise ValueError("Packet manifest exceeds 64 MiB")
    manifest = _parse_json(manifest_path.read_bytes())
    if (not isinstance(manifest, dict) or manifest.get("schema_version") != 2 or
            manifest.get("kind") != "kasane-inspection-packet"):
        raise ValueError("Unsupported packet schema")
    profile = manifest.get("profile")
    if profile not in ("report", "analysis", "scene") or manifest.get("status") != "complete":
        raise ValueError("Unsupported or incomplete packet")
    entries = manifest.get("views")
    hashes = manifest.get("files")
    if (not isinstance(entries, list) or len(entries) > 64 or
            not isinstance(hashes, dict) or len(hashes) > 194):
        raise ValueError("Invalid packet view/file list")
    members = {}
    total = 0
    for path, digest in hashes.items():
        if not isinstance(path, str) or not isinstance(digest, str):
            raise ValueError("Invalid packet file descriptor")
        content = _read_member(directory, path, digest, MAX_PACKET_BYTES - total)
        total += len(content)
        if total > MAX_PACKET_BYTES:
            raise ValueError("Packet exceeds 512 MiB read budget")
        members[path] = content
    views = []
    for entry in entries:
        if entry.get("kind") not in ("raw_context", "clean", "labels", "alpha", "isolated", "xray",
                                     "mask_source", "mask_combined", "mask_consumer", "mask_coverage",
                                     "wireframe", "vertices", "deformers", "displacement",
                                     "distortion"):
            raise ValueError("Unsupported packet view kind")
        if entry.get("mode", "context") not in ("context", "isolated", "xray") or entry.get("status", "complete") != "complete":
            raise ValueError("Unsupported packet view mode or status")
        width, height = entry["width"], entry["height"]
        if (type(width) is not int or type(height) is not int or
                width <= 0 or height <= 0 or width > 4096 or height > 4096 or
                width * height > MAX_PAGE_PIXELS):
            raise ValueError("Invalid packet view dimensions")
        scale = entry["view_scale"]
        if (type(scale) not in (int, float) or not math.isfinite(scale) or scale <= 0
                or not math.isfinite(1 / scale)):
            raise ValueError("Invalid packet view scale")
        for key, length in (("requested_roi", 4), ("padded_roi", 4),
                            ("visible_roi", 4), ("view_offset", 2)):
            values = entry[key]
            if (not isinstance(values, list) or len(values) != length or
                    any(type(value) not in (int, float) or not math.isfinite(value)
                        for value in values)):
                raise ValueError(f"Invalid packet {key}")
            if length == 4 and (values[2] <= values[0] or values[3] <= values[1]):
                raise ValueError(f"Empty packet {key}")
        canvas = entry.get("canvas")
        if canvas is not None and (not isinstance(canvas, list) or len(canvas) != 5 or
                                   any(type(v) not in (int, float) or not math.isfinite(v)
                                       for v in canvas) or canvas[4] <= 0):
            raise ValueError("Invalid packet canvas mapping")
        png = members[entry["png_path"]]
        if _sha(png) != entry["artifact_sha256"]:
            raise ValueError("Packet view artifact hash mismatch")
        if (png[:16] != b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR" or
                int.from_bytes(png[16:20], "big") != width or
                int.from_bytes(png[20:24], "big") != height):
            raise ValueError("Packet PNG dimensions mismatch")
        rgba = members[entry["raw_path"]] if profile != "report" else None
        if rgba is not None and (len(rgba) != width * height * 4 or
                                 entry["raw_shape"] != [height, width, 4] or
                                 entry["raw_dtype"] != "u8" or
                                 _sha(rgba) != entry["raw_sha256"]):
            raise ValueError("Packet raw buffer shape mismatch")
        views.append(InspectionView(
            entry["view_id"], entry["sample_index"], width, height, png, rgba,
            tuple(entry["requested_roi"]), tuple(entry["padded_roi"]),
            tuple(entry["visible_roi"]), entry["view_scale"],
            tuple(entry["view_offset"]), entry["render_digest"],
            entry["artifact_sha256"], entry["adapter_name"],
            entry["adapter_backend"],
            None, entry["kind"], entry.get("presentation"),
            tuple(entry.get("omitted_labels", ())),
            tuple(canvas) if canvas is not None else None,
            entry.get("mode", "context"), entry.get("status", "complete"),
        ))
    if sum(view.width * view.height for view in views) > MAX_ARTIFACT_PIXELS:
        raise ValueError("Packet artifact pixel budget exceeded")
    authoring = _parse_json(members["authoring.json"]) if profile != "report" else None
    evaluated = _parse_json(members["evaluated-frame.json"]) if profile != "report" else None
    trace = _parse_json(members["evaluation-trace.json"]) if "evaluation-trace.json" in members else None
    if profile == "scene" and (directory / "scene").is_symlink():
        raise ValueError("Scene bundle directory must not be a symlink")
    scene = None
    if profile == "scene":
        from ._observe import CapturedScene, NativeCapturedScene
        if NativeCapturedScene is None:
            raise _unavailable("Opening a scene profile requires the observe wheel")
        scene = CapturedScene(NativeCapturedScene.open_scene(str(directory / "scene")))
    capture = manifest["capture"]
    if scene is not None and (scene.capture_id != capture["capture_id"] or
                              scene.scene_digest != capture["scene_digest"]):
        raise ValueError("Packet capture identity differs from scene bundle")
    comparison = None
    comparison_entry = manifest.get("comparison")
    if comparison_entry is not None:
        from ._comparison import ComparisonArtifact, ComparisonResult
        if (not isinstance(comparison_entry, dict) or
                comparison_entry.get("current_view_id") not in
                {view.view_id for view in views} or
                not isinstance(comparison_entry.get("artifacts"), list) or
                len(comparison_entry["artifacts"]) > 4):
            raise ValueError("Invalid packet comparison descriptor")
        artifacts = []
        for index, entry in enumerate(comparison_entry["artifacts"]):
            if not isinstance(entry, dict):
                raise ValueError("Invalid packet comparison artifact descriptor")
            kind = entry.get("kind")
            relative = entry.get("path")
            width, height = entry.get("width"), entry.get("height")
            if (kind not in ("side_by_side", "onion_skin", "outline",
                             "abs_diff_heatmap") or
                    relative != f"comparison/{index:03d}-{kind}.png" or
                    relative not in members or
                    type(width) is not int or type(height) is not int or
                    width <= 0 or height <= 0 or width * height > MAX_PAGE_PIXELS):
                raise ValueError("Invalid packet comparison artifact descriptor")
            png = members[relative]
            if (hashes.get(relative) != entry["sha256"] or
                    _sha(png) != entry["sha256"] or
                    png[:16] != b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR" or
                    int.from_bytes(png[16:20], "big") != entry["width"] or
                    int.from_bytes(png[20:24], "big") != entry["height"]):
                raise ValueError("Invalid packet comparison artifact")
            artifacts.append(ComparisonArtifact(entry["kind"], entry["width"],
                                                entry["height"], png, entry["sha256"]))
        comparison = ComparisonResult(
            comparison_entry["current_view_id"],
            comparison_entry["reference_view_id"],
            comparison_entry["reference_sha256"],
            comparison_entry["registration_status"],
            comparison_entry["view_compatibility"],
            comparison_entry["metrics"],
            tuple(comparison_entry["change_bounds"]) if
            comparison_entry["change_bounds"] is not None else None,
            comparison_entry["contour_source"], tuple(artifacts),
            comparison_entry["target_basis"], comparison_entry["thresholds"],
            comparison_entry.get("registration"),
        )
    return InspectionPacket(
        capture["capture_id"], capture["scene_digest"], capture,
        manifest["source"], tuple(manifest["objects"]), tuple(views), profile,
        authoring, evaluated, scene, False,
        manifest.get("focus_status", "not_requested"),
        comparison,
        trace,
        manifest.get("deformation"),
    )
