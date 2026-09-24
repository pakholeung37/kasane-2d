"""GPU observation and reproducible observation-run reports."""
from __future__ import annotations

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
from ._session import Session

try:
    from ._native import NativeObserver, ObservationFailure
except ImportError:
    NativeObserver = None

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
        metadata, width, height, rgba, png, textures, adapter_name, backend = raw
        version, input_sha256, evaluation_revision, document_id, source_revision, parameters, canvas, scale, offset, bounds = metadata
        return ObservedFrame(
            version, input_sha256, evaluation_revision, document_id, source_revision,
            [ParameterSample(*item) for item in parameters], CanvasSnapshot(*canvas),
            scale, offset, [DrawableBounds(*item) for item in bounds],
            width, height, rgba, png,
            [TextureRevision(*item) for item in textures], adapter_name, backend,
        )

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
                    "texture_profile": "linear_no_mipmap",
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
