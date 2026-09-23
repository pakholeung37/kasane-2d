"""Kasane 2D authoring SDK. Paths passed to native IO must be absolute."""

from __future__ import annotations

from pathlib import Path
from typing import Mapping, NamedTuple, Sequence
from weakref import WeakSet

from ._native import NativeSession, SdkFailure, capabilities

Version = tuple[int, int, int]
Point = tuple[float, float]


class MeshSnapshot(NamedTuple):
    id: str
    name: str
    vertex_ids: list[int]
    positions: list[Point]
    version: Version


class AssetSnapshot(NamedTuple):
    id: str
    name: str
    source: str
    width: int
    height: int
    sha256: str
    version: Version


class CanvasSnapshot(NamedTuple):
    width: float
    height: float
    origin_x: float
    origin_y: float
    pixels_per_unit: float


class GeometrySnapshot(NamedTuple):
    version: Version
    mesh_id: str
    vertex_ids: list[int]
    positions: list[Point]
    uvs: list[Point]
    triangles: list[tuple[int, int, int]]
    space: str
    parent_id: str | None


class HistoryState(NamedTuple):
    undo_steps: int
    redo_steps: int
    estimated_bytes: int
    max_steps: int
    max_bytes: int


class EditEvent(NamedTuple):
    label: str
    before: Version
    after: Version
    kind: str
    object_ids: list[str]
    changed: bool


class ParameterSnapshot(NamedTuple):
    id: str
    name: str
    minimum: float
    maximum: float
    default_value: float
    repeat: bool
    version: Version


class Axis(NamedTuple):
    parameter_id: str
    keys: list[float]


class MeshKeyform(NamedTuple):
    keys: list[float]
    positions: list[Point]


class ResourceIssue(NamedTuple):
    asset_id: str
    code: str
    message: str


class StructureIssue(NamedTuple):
    object_id: str
    code: str
    message: str


class ParameterSample(NamedTuple):
    id: str
    requested: float
    value: float
    clamped: bool


class DrawableSample(NamedTuple):
    id: str
    positions: list[Point]


class Evaluation(NamedTuple):
    parameters: list[ParameterSample]
    drawables: list[DrawableSample]


class SaveResult(NamedTuple):
    manifest: Path
    durable: bool
    warnings: list[str]
    history_warnings: list[str]


class ImportResult(NamedTuple):
    version: Version
    moc_version: int
    diagnostics: list[ResourceIssue]
    warnings: list[str]


class ExportResult(NamedTuple):
    published: bool
    durable: bool
    warnings: list[str]


_sessions: WeakSet[Session] = WeakSet()


class Edit:
    """An atomic SDK edit. Commands publish together on successful exit."""

    def __init__(self, native) -> None:
        self._native = native
        self._closed = False

    def __enter__(self) -> Edit:
        return self

    def __exit__(self, exception_type, exception, traceback) -> bool:
        if self._closed:
            return False
        if exception_type is not None:
            self.cancel()
            return False
        self.commit()
        return False

    def _call(self, operation) -> None:
        try:
            operation()
        except BaseException:
            self._native.abort()
            raise

    def add_png_asset(self, asset_id: str, name: str, absolute_path: Path) -> None:
        self._call(lambda: self._native.add_png_asset(asset_id, name, str(absolute_path)))

    def create_rectangle(
        self, mesh_id: str, name: str, asset_id: str, minimum: Point, maximum: Point
    ) -> None:
        self._call(
            lambda: self._native.create_rectangle(mesh_id, name, asset_id, minimum, maximum)
        )

    def rename_mesh(self, mesh_id: str, name: str) -> None:
        self._call(lambda: self._native.rename_mesh(mesh_id, name))

    def update_positions(
        self, mesh_id: str, vertex_ids: Sequence[int], positions: Sequence[Point]
    ) -> None:
        self._call(
            lambda: self._native.update_positions(mesh_id, list(vertex_ids), list(positions))
        )

    def create_parameter(
        self,
        parameter_id: str,
        name: str,
        minimum: float,
        maximum: float,
        default_value: float,
        repeat: bool = False,
    ) -> None:
        self._call(
            lambda: self._native.create_parameter(
                parameter_id, name, minimum, maximum, default_value, repeat
            )
        )

    def create_mesh_binding(
        self,
        binding_id: str,
        mesh_id: str,
        axes: Sequence[Axis],
        forms: Sequence[MeshKeyform],
    ) -> None:
        self._call(
            lambda: self._native.create_mesh_binding(
                binding_id,
                mesh_id,
                [(axis.parameter_id, list(axis.keys)) for axis in axes],
                [(list(form.keys), list(form.positions)) for form in forms],
            )
        )

    def commit(self) -> Version:
        try:
            return self._native.commit()
        finally:
            self._closed = True

    def cancel(self) -> None:
        self._native.cancel()
        self._closed = True


class Session:
    """Thread-safe handle to one Rust authoring session."""

    def __init__(
        self,
        document_id: str,
        width: float,
        height: float,
        origin: Point,
        pixels_per_unit: float,
    ) -> None:
        self._native = NativeSession(
            document_id, width, height, origin[0], origin[1], pixels_per_unit
        )
        _sessions.add(self)

    @classmethod
    def _from_native(cls, native: NativeSession) -> Session:
        session = cls.__new__(cls)
        session._native = native
        _sessions.add(session)
        return session

    def edit(self, label: str, expected_version: Version | None = None) -> Edit:
        return Edit(self._native.start_edit(label, expected_version))

    def new_project(
        self,
        document_id: str,
        width: float,
        height: float,
        origin: Point,
        pixels_per_unit: float,
        expected_version: Version | None = None,
    ) -> Version:
        return self._native.new_project(
            document_id,
            width,
            height,
            origin[0],
            origin[1],
            pixels_per_unit,
            expected_version,
        )

    def save(self, absolute_path: Path, expected_version: Version | None = None) -> SaveResult:
        manifest, durable, warnings, history_warnings = self._native.save(
            str(absolute_path), expected_version
        )
        return SaveResult(Path(manifest), durable, warnings, history_warnings)

    def import_model3(
        self, absolute_path: Path, expected_version: Version | None = None
    ) -> ImportResult:
        version, moc_version, diagnostics, warnings = self._native.import_model3(
            str(absolute_path), expected_version
        )
        return ImportResult(
            version, moc_version, [ResourceIssue(*item) for item in diagnostics], warnings
        )

    def import_bare_moc3(
        self,
        absolute_path: Path,
        texture_map: Mapping[int, Path],
        expected_version: Version | None = None,
    ) -> ImportResult:
        paths = {slot: str(path) for slot, path in texture_map.items()}
        version, moc_version, diagnostics, warnings = self._native.import_bare_moc3(
            str(absolute_path), paths, expected_version
        )
        return ImportResult(
            version, moc_version, [ResourceIssue(*item) for item in diagnostics], warnings
        )

    def export_package(
        self, absolute_path: Path, expected_version: Version | None = None
    ) -> ExportResult:
        return ExportResult(*self._native.export_package(str(absolute_path), expected_version))

    def undo(self) -> Version:
        return self._native.undo()

    def redo(self) -> Version:
        return self._native.redo()

    def mesh_ids(self) -> list[str]:
        return self._native.mesh_ids()

    def asset_ids(self) -> list[str]:
        return self._native.asset_ids()

    def asset(self, asset_id: str) -> AssetSnapshot | None:
        raw = self._native.asset(asset_id)
        return AssetSnapshot(*raw) if raw is not None else None

    def references_to(self, object_id: str) -> list[str]:
        return self._native.references_to(object_id)

    def parameter_ids(self) -> list[str]:
        return self._native.parameter_ids()

    def binding_ids(self) -> list[str]:
        return self._native.binding_ids()

    def part_ids(self) -> list[str]:
        return self._native.part_ids()

    def transform_ids(self) -> list[str]:
        return self._native.transform_ids()

    def scene_binding_ids(self) -> list[str]:
        return self._native.scene_binding_ids()

    def blend_key_table_ids(self) -> list[str]:
        return self._native.blend_key_table_ids()

    def blend_constraint_ids(self) -> list[str]:
        return self._native.blend_constraint_ids()

    def blend_binding_ids(self) -> list[str]:
        return self._native.blend_binding_ids()

    def glue_ids(self) -> list[str]:
        return self._native.glue_ids()

    def offscreen_ids(self) -> list[str]:
        return self._native.offscreen_ids()

    def parameter(self, parameter_id: str) -> ParameterSnapshot | None:
        raw = self._native.parameter(parameter_id)
        if raw is None:
            return None
        return ParameterSnapshot(*raw)

    def mesh(self, mesh_id: str) -> MeshSnapshot | None:
        raw = self._native.mesh(mesh_id)
        if raw is None:
            return None
        return MeshSnapshot(*raw)

    def find_meshes_by_name(self, name: str) -> list[MeshSnapshot]:
        return [MeshSnapshot(*mesh) for mesh in self._native.find_meshes_by_name(name)]

    def require_unique_mesh(self, name: str) -> MeshSnapshot:
        return MeshSnapshot(*self._native.require_unique_mesh(name))

    def geometry(self, mesh_id: str) -> GeometrySnapshot | None:
        raw = self._native.geometry(mesh_id)
        return GeometrySnapshot(*raw) if raw is not None else None

    def validate_structure(self) -> list[StructureIssue]:
        return [StructureIssue(*issue) for issue in self._native.validate_structure()]

    def history_state(self) -> HistoryState:
        return HistoryState(*self._native.history_state())

    def history_lengths(self) -> tuple[int, int]:
        return self._native.history_lengths()

    def estimated_content_bytes(self) -> int:
        return self._native.estimated_content_bytes()

    def drain_events(self) -> list[EditEvent]:
        return [EditEvent(*event) for event in self._native.drain_events()]

    def evaluate(self, values: Mapping[str, float]) -> Evaluation:
        parameters, drawables = self._native.evaluate(dict(values))
        return Evaluation(
            [ParameterSample(*sample) for sample in parameters],
            [DrawableSample(*sample) for sample in drawables],
        )

    def diagnose_resources(self) -> list[ResourceIssue]:
        return [ResourceIssue(*item) for item in self._native.diagnose_resources()]

    @property
    def preview_values(self) -> dict[str, float]:
        return self._native.preview_values()

    @property
    def preview_revision(self) -> int:
        return self._native.preview_revision()

    @property
    def preview_evaluation_count(self) -> int:
        return self._native.preview_evaluation_count()

    def preview_frame(self) -> Evaluation:
        parameters, drawables = self._native.preview_frame()
        return Evaluation(
            [ParameterSample(*sample) for sample in parameters],
            [DrawableSample(*sample) for sample in drawables],
        )

    def set_preview_values(self, values: Mapping[str, float]) -> bool:
        return self._native.set_preview_values(dict(values))

    def set_preview_parameter(self, parameter_id: str, value: float) -> bool:
        return self._native.set_preview_parameter(parameter_id, value)

    def reset_preview_values(self) -> bool:
        return self._native.reset_preview_values()

    @property
    def version(self) -> Version:
        return self._native.version()

    @property
    def document_id(self) -> str:
        return self._native.document_id()

    @property
    def canvas(self) -> CanvasSnapshot:
        return CanvasSnapshot(*self._native.canvas())

    @property
    def evaluation_revision(self) -> int:
        return self._native.evaluation_revision()

    @property
    def modified(self) -> bool:
        return self._native.modified()

    @property
    def project_path(self) -> Path | None:
        value = self._native.project_path()
        return Path(value) if value is not None else None


def open_project(absolute_path: Path) -> Session:
    """Open a saved project from an absolute path."""
    return Session._from_native(NativeSession.open(str(absolute_path)))


__all__ = [
    "Axis",
    "AssetSnapshot",
    "CanvasSnapshot",
    "DrawableSample",
    "Edit",
    "EditEvent",
    "Evaluation",
    "ExportResult",
    "GeometrySnapshot",
    "HistoryState",
    "ImportResult",
    "MeshSnapshot",
    "MeshKeyform",
    "ParameterSample",
    "ParameterSnapshot",
    "ResourceIssue",
    "SaveResult",
    "SdkFailure",
    "Session",
    "StructureIssue",
    "capabilities",
    "open_project",
]
