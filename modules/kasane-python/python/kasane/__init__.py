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

    def parameter_ids(self) -> list[str]:
        return self._native.parameter_ids()

    def parameter(self, parameter_id: str) -> ParameterSnapshot | None:
        raw = self._native.parameter(parameter_id)
        if raw is None:
            return None
        return ParameterSnapshot(*raw, self.version)

    def mesh(self, mesh_id: str) -> MeshSnapshot | None:
        raw = self._native.mesh(mesh_id)
        if raw is None:
            return None
        return MeshSnapshot(*raw, self.version)

    def evaluate(self, values: Mapping[str, float]) -> Evaluation:
        parameters, drawables = self._native.evaluate(dict(values))
        return Evaluation(
            [ParameterSample(*sample) for sample in parameters],
            [DrawableSample(*sample) for sample in drawables],
        )

    def diagnose_resources(self) -> list[ResourceIssue]:
        return [ResourceIssue(*item) for item in self._native.diagnose_resources()]

    @property
    def version(self) -> Version:
        return self._native.version()

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
    "DrawableSample",
    "Edit",
    "Evaluation",
    "ExportResult",
    "ImportResult",
    "MeshSnapshot",
    "MeshKeyform",
    "ParameterSample",
    "ParameterSnapshot",
    "ResourceIssue",
    "SaveResult",
    "SdkFailure",
    "Session",
    "capabilities",
    "open_project",
]
