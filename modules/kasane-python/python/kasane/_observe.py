"""GPU observation and reproducible observation-run reports."""
# Native implementations are compiled extensions; their source contract is _native.pyi.
# pyright: reportMissingModuleSource=false
from __future__ import annotations

from dataclasses import dataclass
import hashlib
from importlib.metadata import version as package_version
import json
import math
from pathlib import Path
import platform
import struct
from typing import Literal, Mapping, Sequence, cast
from uuid import uuid4
import zlib
from . import _native as _native_module
from ._animation import MotionPreview
from ._session import Session
from ._inspection import (
    InspectionPacket, RawInspectionRequest, append_view, check_next_view,
    open_inspection_packet, packet_from_scene,
)
from ._spatial import ObjectBounds, ObjectRef, HitTestResult, object_bounds, hit_test

try:
    from ._native import (
        NativeCapturedScene as NativeCapturedScene, NativeObserver,
        ObservationFailure as _NativeObservationFailure,
    )
    ObservationFailure = _NativeObservationFailure
except ImportError:
    NativeObserver = None
    NativeCapturedScene = None

    class _UnavailableObservationFailure(Exception):
        """Observation capability failure in a wheel without GPU support."""

        code: str
        asset_id: str | None

    ObservationFailure = _UnavailableObservationFailure

from ._types import (
    CanvasSnapshot,
    DrawableBounds,
    ObservationRun,
    ObservedFrame,
    ParameterSample,
    TextureRevision,
)


class Observer:
    """Reusable GPU renderer available in wheels built with ``observe``."""

    def __init__(self, width: int, height: int, fit_long_side: float) -> None:
        """Create an offscreen observer with output size and fitted view extent."""
        if NativeObserver is None:
            raise RuntimeError("This kasane wheel has no GPU observation feature")
        self._native = NativeObserver(width, height, fit_long_side)

    def __enter__(self) -> Observer:
        """Return this observer for use in a context manager."""
        return self

    def __exit__(self, exception_type, exception, traceback) -> bool:
        """Leave the observer context without suppressing an exception."""
        return False

    def set_fit_long_side(self, value: float) -> None:
        """Change the fitted view extent while reusing the GPU observer."""
        self._native.set_fit_long_side(value)

    def observe(self, session: Session, values: Mapping[str, float] | None = None) -> ObservedFrame:
        """Render a session at parameter values without changing its preview state.

        Values may use parameter IDs or unique display names. The result holds
        RGBA bytes, PNG bytes, bounds, version, texture hashes, and adapter data.
        """
        raw = self._native.observe(session._native, session._parameter_values(values or {}))
        return _frame_from_native(raw)

    def capture_scene(
        self, session: Session, values: Mapping[str, float] | None = None,
    ) -> CapturedScene:
        """Freeze one evaluated scene and its decoded textures for later views."""
        native = self._native.capture_scene(
            session._native, dict(values or {})
        )
        return CapturedScene(native)

    def capture_scenes(
        self, session: Session, samples: Sequence[Mapping[str, float]],
    ) -> tuple[CapturedScene, ...]:
        """Freeze 1–64 parameter samples from one document snapshot.

        Names resolve against that snapshot. Decoded texture bytes are shared
        across samples, including assets used by only one sample.
        """
        if not 1 <= len(samples) <= 64:
            raise ValueError("capture_scenes requires 1–64 samples")
        return tuple(CapturedScene(native) for native in
                     self._native.capture_scenes(session._native, [dict(item) for item in samples]))

    def capture_animation_scene(
        self, session: Session, preview: MotionPreview, *,
        apply_model_opacity: bool = False,
    ) -> CapturedScene:
        """Freeze the preview's actual Motion/Expression/Physics/Pose frame.

        This does not advance or seek the preview. The scene includes its
        current snapshot and Model opacity policy, but no past update journal.
        """
        native = self._native.capture_animation_scene(
            session._native, preview._native, apply_model_opacity
        )
        return CapturedScene(native)

    def open_scene(self, absolute_directory: Path) -> CapturedScene:
        """Open a saved scene without consulting a live authoring session."""
        if not absolute_directory.is_absolute():
            raise ValueError("Scene directory must be absolute")
        if NativeCapturedScene is None:
            raise RuntimeError("This kasane wheel has no GPU observation feature")
        return CapturedScene(NativeCapturedScene.open_scene(str(absolute_directory)))

    def render_scene(
        self, scene: CapturedScene, *, roi: tuple[float, float, float, float],
        resolution: tuple[int, int], padding_canvas: float = 0,
    ) -> RenderedSceneView:
        """Rerender a frozen scene at an explicit source-canvas ROI."""
        raw, (requested, padded, visible), render_digest = scene._native.render(
            self._native, resolution[0], resolution[1], roi, padding_canvas
        )
        return RenderedSceneView(
            _frame_from_native(raw), requested, padded, visible, render_digest,
            scene.capture_id, scene.scene_digest,
        )

    def focus(
        self, scene: CapturedScene, targets: ObjectRef | Sequence[ObjectRef], *,
        resolution: tuple[int, int] = (1024, 1024), padding_canvas: float = 12,
        include_hidden: bool = True,
    ) -> FocusedSceneView:
        """Rerender the complete scene around evaluated mesh or Part geometry.

        ``include_hidden`` controls which geometry sets the ROI; it does not
        make hidden objects visible. ``padding_canvas`` is measured in source
        pixels. Raises ``ValueError`` when no selected geometry contributes to
        the ROI.
        """
        bounds = scene.bounds(targets, include_hidden=include_hidden)
        if bounds.canvas_bounds is None:
            raise ValueError(f"Selected objects have no evaluated geometry: {bounds.empty_reason}")
        roi = _nonempty_roi(bounds.canvas_bounds)
        return FocusedSceneView(
            self.render_scene(scene, roi=roi, resolution=resolution,
                              padding_canvas=padding_canvas), bounds,
        )

    def focus_scenes(
        self, scenes: Sequence[CapturedScene],
        targets: ObjectRef | Sequence[ObjectRef], *,
        resolution: tuple[int, int] = (1024, 1024), padding_canvas: float = 12,
        include_hidden: bool = True, follow: bool = False,
    ) -> tuple[FocusedSceneView, ...]:
        """Focus same-document captures with one shared ROI by default.

        The shared ROI is the union of available target bounds, preserving
        movement between captures. ``follow=True`` uses each scene's bounds;
        scenes with empty bounds fall back to the union ROI.
        """
        if not scenes:
            raise ValueError("Focus requires at least one scene")
        if len({scene.metadata["document_id"] for scene in scenes}) != 1:
            raise ValueError("Focused scenes must come from one document")
        bounds = tuple(scene.bounds(targets, include_hidden=include_hidden)
                       for scene in scenes)
        available = tuple(item.canvas_bounds for item in bounds
                          if item.canvas_bounds is not None)
        if not available:
            raise ValueError(f"Selected objects have no evaluated geometry: "
                             f"{bounds[0].empty_reason}")
        xs0, ys0, xs1, ys1 = zip(*available)
        union = _nonempty_roi((min(xs0), min(ys0), max(xs1), max(ys1)))
        if follow:
            rois = tuple(_nonempty_roi(item.canvas_bounds)
                         if item.canvas_bounds is not None else union
                         for item in bounds)
        else:
            rois = (union,) * len(scenes)
        return tuple(FocusedSceneView(
            self.render_scene(scene, roi=roi, resolution=resolution,
                              padding_canvas=padding_canvas), item,
        ) for scene, roi, item in zip(scenes, rois, bounds))

    def inspect_scene(
        self, scene: CapturedScene, *, request: RawInspectionRequest,
    ) -> InspectionPacket:
        """Create a raw O1 packet from a frozen scene without live session reads."""
        rendered = self.render_scene(
            scene, roi=request.roi, resolution=request.resolution,
            padding_canvas=request.padding_canvas,
        )
        return packet_from_scene(scene, rendered)

    def inspect(
        self, session: Session, values: Mapping[str, float] | None = None, *,
        request: RawInspectionRequest,
    ) -> InspectionPacket:
        """Freeze one parameter frame and return its raw inspection packet."""
        return self.inspect_scene(self.capture_scene(session, values), request=request)

    def inspect_animation(
        self, session: Session, preview: MotionPreview, *,
        request: RawInspectionRequest, apply_model_opacity: bool = False,
    ) -> InspectionPacket:
        """Freeze the preview's actual current frame without advancing it."""
        scene = self.capture_animation_scene(
            session, preview, apply_model_opacity=apply_model_opacity,
        )
        return self.inspect_scene(scene, request=request)

    def render(
        self, packet: InspectionPacket, *, request: RawInspectionRequest,
    ) -> InspectionPacket:
        """Append a raw view while preserving the packet's capture identity."""
        if packet.closed or packet._scene is None:
            from ._inspection import _unavailable
            raise _unavailable("Packet has no open scene for another render")
        check_next_view(packet, request)
        rendered = self.render_scene(
            packet._scene, roi=request.roi, resolution=request.resolution,
            padding_canvas=request.padding_canvas,
        )
        return append_view(packet, rendered)

    def open(self, absolute_directory: Path) -> InspectionPacket:
        """Open and validate a saved report, analysis, or scene packet."""
        return open_inspection_packet(absolute_directory)


    def observe_run(
        self, session: Session, samples: Sequence[Mapping[str, float]], output: Path,
        crop_targets: Sequence[ObjectRef] = (),
        crop_mode: Literal["each", "union"] = "each",
    ) -> ObservationRun:
        """Render nonempty samples into a unique child of an absolute directory.

        ``crop_targets`` selects mesh or Part image crops. ``crop_mode`` selects
        one crop per target or one crop around their union. Return paths for the report,
        frames, crops, and contact sheet. A failed run still writes a report
        and attaches ``run_directory`` to the raised exception.
        """
        if not output.is_absolute():
            raise ValueError("Observation output path must be absolute")
        if not samples:
            raise ValueError("Observation run requires at least one sample")
        if crop_mode not in ("each", "union"):
            raise ValueError("crop_mode must be 'each' or 'union'")
        from ._spatial import _targets
        requested_crops = _targets(crop_targets) if crop_targets else ()
        crop_meshes: dict[ObjectRef, tuple[str, ...]] = {}
        crop_version = session.version if requested_crops else None
        if requested_crops:
            resolved = session._native.resolve_spatial_targets(
                [(target.kind, target.id) for target in requested_crops]
            )
            crop_meshes = {target: tuple(mesh_ids)
                           for target, mesh_ids in zip(requested_crops, resolved)}
            if session.version != crop_version:
                raise RuntimeError("Document changed while resolving crop targets")
        if crop_mode == "union" and requested_crops:
            crop_groups = [(requested_crops, tuple(dict.fromkeys(
                mesh_id for target in requested_crops
                for mesh_id in crop_meshes[target])), "union")]
        else:
            crop_groups = [((target,), crop_meshes[target],
                            f"{target.kind}-{target.id}") for target in requested_crops]
        output.mkdir(parents=True, exist_ok=True)
        directory = output / uuid4().hex
        frames_dir = directory / "frames"
        frames_dir.mkdir(parents=True)
        report_path = directory / "report.json"
        frames: list[Path] = []
        crops: list[Path] = []
        captured: list[ObservedFrame] = []
        entries: list[dict] = []
        sample_entries: list[dict] = []
        diagnostics: list[dict] = []
        with Path(_native_module.__file__).open("rb") as native_binary:
            binary_sha256 = hashlib.file_digest(native_binary, "sha256").hexdigest()
        report = {
            "schema_version": 1,
            "sdk_version": package_version("kasane"),
            "sdk_binary_sha256": binary_sha256,
            "platform": platform.platform(),
            "status": "running", "frames": entries, "samples": sample_entries,
        }
        try:
            for index, requested in enumerate(samples):
                frame = self.observe(session, requested)
                if crop_version is not None and frame.version != crop_version:
                    raise RuntimeError("Document changed during a crop run")
                path = frames_dir / f"{index:03d}.png"
                frame.save_png(path)
                frames.append(path)
                captured.append(frame)
                crop_entries = []
                bounds_by_id = {item.id: item for item in frame.drawable_bounds}
                for group, mesh_ids, label in crop_groups:
                    rectangles = [cast(tuple[int, int, int, int], bounds_by_id[mesh_id].bounds)
                                  for mesh_id in mesh_ids
                                  if mesh_id in bounds_by_id
                                  and bounds_by_id[mesh_id].bounds is not None]
                    if not rectangles:
                        diagnostics.append({"index": index,
                                            "targets": [target.__dict__ for target in group],
                                            "code": "CROP_EMPTY"})
                        continue
                    x0 = min(rect[0] for rect in rectangles)
                    y0 = min(rect[1] for rect in rectangles)
                    x1 = max(rect[2] for rect in rectangles)
                    y1 = max(rect[3] for rect in rectangles)
                    cropped = _crop_rgba(frame.rgba, frame.width, (x0, y0, x1, y1))
                    crop_png = _encode_rgba_png(x1 - x0, y1 - y0, cropped)
                    crop_path = directory / "crops" / label / f"{index:03d}.png"
                    crop_path.parent.mkdir(parents=True, exist_ok=True)
                    crop_path.write_bytes(crop_png)
                    crops.append(crop_path)
                    crop_entries.append({
                        "object_id": group[0].id if len(group) == 1 else None,
                        "targets": [target.__dict__ for target in group],
                        "mesh_ids": mesh_ids, "bounds": (x0, y0, x1, y1),
                        "path": str(crop_path.relative_to(directory)),
                        "sha256": hashlib.sha256(crop_png).hexdigest(),
                    })
                sample_entries.append({
                    "requested": dict(requested),
                    "actual": [sample._asdict() for sample in frame.parameters],
                })
                entries.append({
                    "index": index, "path": str(path.relative_to(directory)),
                    "sha256": hashlib.sha256(frame.png).hexdigest(),
                    "version": frame.version,
                    "session_id": frame.version[0],
                    "generation": frame.version[1],
                    "document_revision": frame.version[2],
                    "input_sha256": frame.input_sha256,
                    "source_revision": frame.source_revision,
                    "evaluation_revision": frame.evaluation_revision,
                    "document_id": frame.document_id,
                    "canvas": frame.canvas._asdict(),
                    "view": {"scale": frame.view_scale, "offset": frame.view_offset,
                             "width": frame.width, "height": frame.height},
                    "adapter": {"name": frame.adapter_name, "backend": frame.adapter_backend},
                    "textures": [texture._asdict() for texture in frame.texture_revisions],
                    "format": "RGBA8Unorm",
                    "color_space": "linear_unorm_no_gamma_conversion",
                    "alpha_convention": "premultiplied_no_post_conversion",
                    "background": "transparent",
                    "texture_profile": (
                        "linear_mipmap_linear_repeat" if self._native.texture_mipmaps
                        else "linear_no_mipmap"
                    ),
                    "crops": crop_entries,
                })
            sheet = _contact_sheet(captured)
            contact_sheet = directory / "contact-sheet.png"
            contact_sheet.write_bytes(sheet)
            report["contact_sheet"] = {
                "path": contact_sheet.name, "sha256": hashlib.sha256(sheet).hexdigest(),
            }
            (directory / "samples.json").write_text(
                json.dumps(sample_entries, indent=2, allow_nan=False), encoding="utf-8",
            )
            report["status"] = "frames_complete"
        except Exception as error:
            report["status"] = "failed"
            report["failure"] = {
                "sample_index": len(entries), "type": type(error).__name__,
                "code": getattr(error, "code", None),
                "asset_id": getattr(error, "asset_id", None),
                "message": str(error),
            }
            setattr(error, "run_directory", directory)
            raise
        finally:
            (directory / "diagnostics.json").write_text(
                json.dumps(diagnostics, indent=2, allow_nan=False), encoding="utf-8",
            )
            report_path.write_text(json.dumps(report, indent=2, allow_nan=False), encoding="utf-8")
        return ObservationRun(directory, report_path, frames, crops, contact_sheet)


def _frame_from_native(raw: _native_module.NativeFrameTuple) -> ObservedFrame:
    """Decode the legacy native tuple; the new view reuses its pixel record."""
    metadata, width, height, rgba, png, textures, adapter_name, backend = raw
    version, input_sha256, evaluation_revision, document_id, source_revision, parameters, canvas, scale, offset, bounds = metadata
    return ObservedFrame(
        version, input_sha256, evaluation_revision, document_id, source_revision,
        [ParameterSample(*item) for item in parameters], CanvasSnapshot(*canvas),
        scale, offset, [DrawableBounds(*item) for item in bounds],
        width, height, rgba, png,
        [TextureRevision(*item) for item in textures], adapter_name, backend,
    )


class CapturedScene:
    """Frozen evaluated scene and textures, independent of later session edits."""

    def __init__(self, native: _native_module.NativeCapturedScene) -> None:
        self._native = native

    @property
    def capture_id(self) -> str:
        """Stable ID for this acquisition, including after save and reopen."""
        return self._native.capture_id

    @property
    def scene_digest(self) -> str:
        """Canonical digest of frozen scene metadata, geometry and textures."""
        return self._native.scene_digest

    @property
    def source(self) -> dict:
        """Captured parameter/animation source and available snapshot metadata."""
        return json.loads(self._native.source_json())

    @property
    def authoring(self) -> dict:
        """Frozen object names, topology, hierarchy and mesh binding records."""
        return json.loads(self._native.authoring_json())

    @property
    def metadata(self) -> dict:
        """Frozen capture identity, document version, request and textures."""
        return json.loads(self._native.metadata_json())

    @property
    def evaluated_frame(self) -> dict:
        """The evaluated geometry and draw plan used by the renderer."""
        return json.loads(self._native.evaluated_frame_json())

    def bounds(
        self, targets: ObjectRef | Sequence[ObjectRef], *, include_hidden: bool = True,
    ) -> ObjectBounds:
        """Query target geometry in this capture's source-canvas coordinates.

        Part targets include descendant meshes. Hidden geometry is included
        unless ``include_hidden=False``; masks and occlusion are not measured.
        """
        return object_bounds(self._native, self.capture_id, self.scene_digest,
                             targets, include_hidden=include_hidden)

    def hit_test(
        self, view: RenderedSceneView, point: tuple[float, float], *,
        include_hidden: bool = True, details: bool = False,
        max_candidates: int = 256,
    ) -> HitTestResult:
        """Find meshes whose evaluated triangles contain an image-space point.

        ``view`` must belong to this capture. ``details=True`` includes every
        matching triangle and interpolation data; the default groups results
        by mesh. A hit does not imply painted or visible pixels.
        """
        if view.capture_id != self.capture_id or view.scene_digest != self.scene_digest:
            raise ValueError("View belongs to another captured scene")
        return hit_test(self._native, self.capture_id, self.scene_digest,
                        point, view.image_to_canvas,
                        (view.frame.width, view.frame.height),
                        include_hidden=include_hidden, details=details,
                        max_candidates=max_candidates)

    def save_scene(self, absolute_directory: Path) -> Path:
        """Write a new data-only scene bundle and return its directory."""
        if not absolute_directory.is_absolute():
            raise ValueError("Scene directory must be absolute")
        self._native.save_scene(str(absolute_directory))
        return absolute_directory


@dataclass(frozen=True)
class RenderedSceneView:
    """An explicit ROI render and its source-canvas/image coordinate mapping.

    ``capture_id`` and ``scene_digest`` identify the capture accepted by
    :meth:`CapturedScene.hit_test`.
    """

    frame: ObservedFrame
    requested_roi: tuple[float, float, float, float]
    padded_roi: tuple[float, float, float, float]
    visible_roi: tuple[float, float, float, float]
    render_digest: str
    capture_id: str
    scene_digest: str

    def canvas_to_image(self, point: tuple[float, float]) -> tuple[float, float]:
        """Map a source-canvas pixel coordinate into this view's image pixels."""
        scale, (ox, oy) = self.frame.view_scale, self.frame.view_offset
        return point[0] * scale + ox, point[1] * scale + oy

    def image_to_canvas(self, point: tuple[float, float]) -> tuple[float, float]:
        """Map an image pixel coordinate back into source-canvas pixels."""
        scale, (ox, oy) = self.frame.view_scale, self.frame.view_offset
        return (point[0] - ox) / scale, (point[1] - oy) / scale


@dataclass(frozen=True)
class FocusedSceneView:
    """A rerendered ROI view and this scene's queried target bounds."""

    view: RenderedSceneView
    bounds: ObjectBounds


def _nonempty_roi(bounds: tuple[float, float, float, float]) -> tuple[float, float, float, float]:
    x0, y0, x1, y1 = bounds
    if x0 == x1:
        x0, x1 = x0 - 0.5, x1 + 0.5
    if y0 == y1:
        y0, y1 = y0 - 0.5, y1 + 0.5
    return x0, y0, x1, y1


def _encode_rgba_png(width: int, height: int, rgba: bytes | bytearray) -> bytes:
    stride = width * 4
    if len(rgba) != stride * height:
        raise ValueError("RGBA buffer dimensions do not match")
    scanlines = b"".join(
        b"\0" + rgba[row * stride:(row + 1) * stride]
        for row in range(height)
    )
    def chunk(kind: bytes, payload: bytes) -> bytes:
        return (struct.pack(">I", len(payload)) + kind + payload +
                struct.pack(">I", zlib.crc32(kind + payload) & 0xffffffff))
    return (
        b"\x89PNG\r\n\x1a\n" +
        chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)) +
        chunk(b"IDAT", zlib.compress(scanlines)) +
        chunk(b"IEND", b"")
    )


def _crop_rgba(
    rgba: bytes, width: int, bounds: tuple[int, int, int, int],
) -> bytes:
    x0, y0, x1, y1 = bounds
    stride = width * 4
    return b"".join(
        rgba[row * stride + x0 * 4:row * stride + x1 * 4]
        for row in range(y0, y1)
    )


def _contact_sheet(frames: Sequence[ObservedFrame]) -> bytes:
    columns = math.ceil(math.sqrt(len(frames)))
    rows = math.ceil(len(frames) / columns)
    cell_width = frames[0].width
    cell_height = frames[0].height
    width = cell_width * columns
    height = cell_height * rows
    rgba = bytearray(width * height * 4)
    for index, frame in enumerate(frames):
        x = (index % columns) * cell_width
        y = (index // columns) * cell_height
        for row in range(cell_height):
            source = row * cell_width * 4
            target = ((y + row) * width + x) * 4
            rgba[target:target + cell_width * 4] = frame.rgba[source:source + cell_width * 4]
    return _encode_rgba_png(width, height, rgba)
