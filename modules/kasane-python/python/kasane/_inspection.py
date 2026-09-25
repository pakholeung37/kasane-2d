"""O1 raw inspection packets and bounded, data-only save profiles.

Presentation, geometry queries and report runs are layered on this capture in
later stages. A packet never consults a live authoring session after capture.
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

    def canvas_to_image(self, point: tuple[float, float]) -> tuple[float, float]:
        return (point[0] * self.view_scale + self.view_offset[0],
                point[1] * self.view_scale + self.view_offset[1])

    def image_to_canvas(self, point: tuple[float, float]) -> tuple[float, float]:
        return ((point[0] - self.view_offset[0]) / self.view_scale,
                (point[1] - self.view_offset[1]) / self.view_scale)


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
            "raw_view": bool(self.views),
            "raw_pixel_data": all(view.rgba is not None for view in self.views),
            "evaluated_geometry_data": self.evaluated_frame is not None,
            "rerender_scene": self._scene is not None and not self._closed,
            "presentation": False,
            "geometry_query": False,
            "pixel_coverage_query": False,
            "playback_replay": False,
        }

    def close(self) -> None:
        """Release this packet's native scene reference; saved data stays readable."""
        object.__setattr__(self, "_scene", None)
        object.__setattr__(self, "_closed", True)

    def __enter__(self) -> InspectionPacket:
        return self

    def __exit__(self, exception_type, exception, traceback) -> bool:
        self.close()
        return False

    def save(self, absolute_directory: Path, *, profile: str = "analysis") -> PacketSaveReceipt:
        """Save report, analysis, or scene data to a new absolute directory."""
        if profile not in ("report", "analysis", "scene"):
            raise ValueError("profile must be report, analysis, or scene")
        if len(self.views) > MAX_VIEWS or sum(view.width * view.height for view in self.views) > MAX_ARTIFACT_PIXELS:
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
                "kind": "raw_context", "status": "complete",
                "width": view.width, "height": view.height,
                "png_path": png_path, "artifact_sha256": hashes[png_path],
                "requested_roi": view.requested_roi, "padded_roi": view.padded_roi,
                "visible_roi": view.visible_roi, "view_scale": view.view_scale,
                "view_offset": view.view_offset, "render_digest": view.render_digest,
                "adapter_name": view.adapter_name,
                "adapter_backend": view.adapter_backend,
                "pixel_policy": "premultiplied_linear_rgba8_transparent_v1",
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
            for relative, data in (("authoring.json", self.authoring),
                                   ("evaluated-frame.json", self.evaluated_frame)):
                content = _json(data)
                _write(absolute_directory / relative, content)
                hashes[relative] = _sha(content)
        if profile == "scene":
            assert self._scene is not None
            self._scene.save_scene(absolute_directory / "scene")
        manifest = {
            "schema_version": 2, "kind": "kasane-inspection-packet",
            "status": "complete", "profile": profile,
            "capture": self.metadata, "source": self.source,
            "objects": self.objects, "views": view_entries,
            "capabilities": {
                "raw_view": bool(self.views),
                "raw_pixel_data": profile != "report",
                "evaluated_geometry_data": profile != "report",
                "rerender_scene": profile == "scene",
                "presentation": False, "geometry_query": False,
                "pixel_coverage_query": False, "playback_replay": False,
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


def view_from_render(rendered: RenderedSceneView, sample_index: int = 0) -> InspectionView:
    frame = rendered.frame
    return InspectionView(
        uuid4().hex, sample_index, frame.width, frame.height, frame.png, frame.rgba,
        rendered.requested_roi, rendered.padded_roi, rendered.visible_roi,
        frame.view_scale, frame.view_offset, rendered.render_digest,
        _sha(frame.png), frame.adapter_name, frame.adapter_backend, frame,
    )


def packet_from_scene(scene: CapturedScene, rendered: RenderedSceneView) -> InspectionPacket:
    authoring = scene.authoring
    return InspectionPacket(
        scene.capture_id, scene.scene_digest, scene.metadata, scene.source,
        _objects(authoring), (view_from_render(rendered),), "scene", authoring,
        scene.evaluated_frame, scene,
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
        ))
    if sum(view.width * view.height for view in views) > MAX_ARTIFACT_PIXELS:
        raise ValueError("Packet artifact pixel budget exceeded")
    authoring = _parse_json(members["authoring.json"]) if profile != "report" else None
    evaluated = _parse_json(members["evaluated-frame.json"]) if profile != "report" else None
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
    return InspectionPacket(
        capture["capture_id"], capture["scene_digest"], capture,
        manifest["source"], tuple(manifest["objects"]), tuple(views), profile,
        authoring, evaluated, scene,
    )
