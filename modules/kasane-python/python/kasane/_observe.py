"""GPU observation and reproducible observation-run reports."""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
from importlib.metadata import version as package_version
import json
import math
from pathlib import Path
import platform
import struct
from typing import Mapping, Sequence
from uuid import UUID, uuid4
import zlib
from . import _native as _native_module
from ._animation import MotionPreview
from ._session import Session
from ._inspection import (
    InspectionPacket, RawInspectionRequest, append_view, check_next_view,
    open_inspection_packet, packet_from_scene,
)

try:
    from ._native import NativeCapturedScene, NativeObserver, ObservationFailure
except ImportError:
    NativeObserver = None
    NativeCapturedScene = None

    class ObservationFailure(Exception):
        """Raised only by wheels built with the observe feature."""

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
        )

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
        focus: Sequence[str] = (),
    ) -> ObservationRun:
        """Render nonempty samples into a unique child of an absolute directory.

        ``focus`` contains drawable IDs to crop. Return paths for the report,
        frames, crops, and contact sheet. A failed run still writes a report
        and attaches ``run_directory`` to the raised exception.
        """
        if not output.is_absolute():
            raise ValueError("Observation output path must be absolute")
        if not samples:
            raise ValueError("Observation run requires at least one sample")
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
                path = frames_dir / f"{index:03d}.png"
                frame.save_png(path)
                frames.append(path)
                captured.append(frame)
                crop_entries = []
                bounds_by_id = {item.id: item for item in frame.drawable_bounds}
                for object_id in focus:
                    try:
                        safe_id = str(UUID(object_id))
                    except ValueError:
                        diagnostics.append({"index": index, "object_id": object_id,
                                            "code": "INVALID_FOCUS_ID"})
                        continue
                    drawable = bounds_by_id.get(safe_id)
                    if drawable is None or drawable.bounds is None:
                        diagnostics.append({"index": index, "object_id": safe_id,
                                            "code": "FOCUS_NOT_VISIBLE" if drawable else "FOCUS_NOT_FOUND"})
                        continue
                    x0, y0, x1, y1 = drawable.bounds
                    cropped = _crop_rgba(frame.rgba, frame.width, drawable.bounds)
                    crop_png = _encode_rgba_png(x1 - x0, y1 - y0, cropped)
                    crop_path = directory / "crops" / safe_id / f"{index:03d}.png"
                    crop_path.parent.mkdir(parents=True, exist_ok=True)
                    crop_path.write_bytes(crop_png)
                    crops.append(crop_path)
                    crop_entries.append({
                        "object_id": safe_id, "bounds": drawable.bounds,
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


def _frame_from_native(raw: tuple) -> ObservedFrame:
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

    def __init__(self, native: NativeCapturedScene) -> None:
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

    def save_scene(self, absolute_directory: Path) -> Path:
        """Write a new data-only scene bundle and return its directory."""
        if not absolute_directory.is_absolute():
            raise ValueError("Scene directory must be absolute")
        self._native.save_scene(str(absolute_directory))
        return absolute_directory


@dataclass(frozen=True)
class RenderedSceneView:
    """An explicit ROI render plus source-canvas/image coordinate mapping."""

    frame: ObservedFrame
    requested_roi: tuple[float, float, float, float]
    padded_roi: tuple[float, float, float, float]
    visible_roi: tuple[float, float, float, float]
    render_digest: str

    def canvas_to_image(self, point: tuple[float, float]) -> tuple[float, float]:
        scale, (ox, oy) = self.frame.view_scale, self.frame.view_offset
        return point[0] * scale + ox, point[1] * scale + oy

    def image_to_canvas(self, point: tuple[float, float]) -> tuple[float, float]:
        scale, (ox, oy) = self.frame.view_scale, self.frame.view_offset
        return (point[0] - ox) / scale, (point[1] - oy) / scale


def _encode_rgba_png(width: int, height: int, rgba: bytes) -> bytes:
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
